//! Fabro sandbox agent.
//!
//! Runs as the pod's main process (PID 1) for Kubernetes sandboxes. The
//! provider talks to it over a kube port-forward using the JSON-lines
//! protocol in [`protocol`]:
//!
//! - `POST /exec` — run a command to completion; the response streams output
//!   events, and a client disconnect kills the process group.
//! - `POST /processes` — spawn a long-lived process; a client disconnect leaves
//!   it running. The assigned id is returned in the `X-Fabro-Process-Id`
//!   response header.
//! - `POST /processes/{id}/stdin` — deliver stdin bytes, optionally closing.
//! - `POST /processes/{id}/terminate` — SIGTERM the process group, then SIGKILL
//!   after a grace period.
//!
//! As PID 1 the agent reaps orphaned children, so a sandbox that spawns
//! background processes never accumulates zombies.

// This binary is the pod's PID-1 init process. Its zombie reaper and
// terminate-escalation loops are blocking OS work that cannot be Tokio tasks,
// and its process control goes through libc directly. The blanket relaxations
// below are scoped to this crate on purpose.
#![allow(
    clippy::disallowed_methods,
    reason = "PID-1 duties (waitpid reaping, signal escalation) require dedicated blocking \\
              OS threads and libc, not async task APIs"
)]
#![allow(
    clippy::absolute_paths,
    reason = "libc/tokio process types are clearer fully qualified in a standalone binary"
)]
#![allow(
    clippy::borrow_as_ptr,
    reason = "waitpid wants a raw *mut c_int; &mut status coerces to exactly that"
)]
#![allow(
    unsafe_code,
    reason = "process control (waitpid/setsid/killpg) has no safe std API"
)]

mod protocol;

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine as _;
use protocol::{ExecRequest, OutputChunk, OutputStream, ProcessExit, split_env_entry};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

const MAX_STDIN_BYTES: usize = 64 * 1024 * 1024;
const READ_BUFFER_BYTES: usize = 64 * 1024;
const REAP_POLL_INTERVAL: Duration = Duration::from_millis(25);
const TERMINATE_GRACE: Duration = Duration::from_secs(10);

/// Collected exit status of reaped children, keyed by pid.
fn reaper_registry() -> &'static Mutex<HashMap<i32, ProcessExit>> {
    static REGISTRY: std::sync::OnceLock<Mutex<HashMap<i32, ProcessExit>>> =
        std::sync::OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn process_counter() -> &'static AtomicU64 {
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    &COUNTER
}

struct ProcessEntry {
    pid:   i32,
    pgid:  i32,
    child: Option<tokio::process::Child>,
    stdin: Option<tokio::process::ChildStdin>,
}

fn processes() -> &'static Mutex<HashMap<u64, ProcessEntry>> {
    static PROCESSES: std::sync::OnceLock<Mutex<HashMap<u64, ProcessEntry>>> =
        std::sync::OnceLock::new();
    PROCESSES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Background waitpid loop. Because the agent is PID 1, orphaned children
/// reparent to it; without this loop they would pile up as zombies.
fn spawn_reaper() {
    std::thread::spawn(|| {
        loop {
            let mut status = 0;
            #[allow(
                unsafe_code,
                reason = "PID-1 zombie reaping requires the waitpid syscall;                       the pid/status pair are consumed immediately under the registry lock."
            )]
            let pid = unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) };
            match pid {
                0 | -1 => {
                    // 0: children exist but none has exited; -1 (ECHILD): nothing
                    // is running. Either way, poll again shortly.
                    std::thread::sleep(REAP_POLL_INTERVAL);
                }
                pid => {
                    let exit = decode_wait_status(status);
                    if let Ok(mut registry) = reaper_registry().lock() {
                        registry.insert(pid, exit);
                    }
                }
            }
        }
    });
}

fn decode_wait_status(status: i32) -> ProcessExit {
    if libc::WIFEXITED(status) {
        ProcessExit {
            exit_code: Some(libc::WEXITSTATUS(status)),
            signalled: false,
        }
    } else if libc::WIFSIGNALED(status) {
        ProcessExit {
            // Report signal deaths with the shell's 128+N convention so the
            // provider sees a comparable numeric code.
            exit_code: Some(128 + libc::WTERMSIG(status)),
            signalled: true,
        }
    } else {
        ProcessExit {
            exit_code: None,
            signalled: false,
        }
    }
}

/// Poll the reaper registry until `pid`'s status appears.
async fn wait_for_exit(pid: i32) -> ProcessExit {
    loop {
        if let Ok(registry) = reaper_registry().lock() {
            if let Some(exit) = registry.get(&pid) {
                return *exit;
            }
        }
        tokio::time::sleep(REAP_POLL_INTERVAL).await;
    }
}

/// Spawn `request.argv` in its own session and register it.
fn spawn_process(request: &ExecRequest) -> Result<(u64, i32), String> {
    let (program, args) = request.argv.split_first().ok_or("argv is empty")?;
    let mut command = tokio::process::Command::new(program);
    command.args(args);
    if let Some(working_dir) = request.working_dir.as_deref() {
        command.current_dir(working_dir);
    }
    for entry in &request.env {
        let (key, value) = split_env_entry(entry);
        command.env(key, value);
    }
    // A fresh session isolates the command's process group so terminate can
    // signal the whole tree without touching the agent's own group.
    #[allow(
        unsafe_code,
        reason = "setsid must run between fork and exec; pre_exec is the \
                  only hook with that guarantee and libc::setsid is itself unsafe."
    )]
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command.stdin(std::process::Stdio::piped());
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());

    let mut child = command.spawn().map_err(|err| err.to_string())?;
    let pid = child
        .id()
        .ok_or("process exited immediately")?
        .cast_signed();
    let id = process_counter().fetch_add(1, Ordering::Relaxed);
    let entry = ProcessEntry {
        pid,
        pgid: pid,
        stdin: child.stdin.take(),
        child: Some(child),
    };
    if let Ok(mut processes) = processes().lock() {
        processes.insert(id, entry);
    }
    Ok((id, pid))
}

fn take_process_child(
    id: u64,
) -> Option<(
    i32,
    tokio::process::Child,
    Option<tokio::process::ChildStdin>,
)> {
    let mut processes = processes().lock().expect("process table lock");
    let entry = processes.get_mut(&id)?;
    Some((entry.pid, entry.child.take()?, entry.stdin.take()))
}

/// Stream a process's output as JSON lines, then its exit event.
///
/// `kill_on_disconnect` implements the /exec contract: a client that walks
/// away leaves nothing running behind it. Spawned processes keep running so a
/// reconnecting client can continue where it left off.
async fn stream_process(
    id: u64,
    kill_on_disconnect: bool,
    sender: tokio::sync::mpsc::Sender<Vec<u8>>,
) {
    let Some((pid, mut child, child_stdin)) = take_process_child(id) else {
        return;
    };

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let mut pumps = Vec::new();
    if let Some(stdout) = stdout {
        pumps.push(tokio::spawn(pump_stream(
            id,
            kill_on_disconnect,
            sender.clone(),
            stdout,
            OutputStream::Stdout,
        )));
    }
    if let Some(stderr) = stderr {
        pumps.push(tokio::spawn(pump_stream(
            id,
            kill_on_disconnect,
            sender.clone(),
            stderr,
            OutputStream::Stderr,
        )));
    }
    if let Some(mut stdin) = child_stdin {
        // Closing stdin unblocks commands that read their input to EOF.
        let _ = stdin.shutdown().await;
    }

    let exit = wait_for_exit(pid).await;
    for pump in pumps {
        let _ = pump.await;
    }
    if let Ok(mut processes) = processes().lock() {
        processes.remove(&id);
    }

    let terminal = protocol_line(&exit);
    if sender.send(terminal).await.is_err() && kill_on_disconnect {
        kill_process_group(id);
    }
}

fn protocol_line(event: &impl serde::Serialize) -> Vec<u8> {
    let mut line = serde_json::to_vec(event).expect("protocol event serializes");
    line.push(b'\n');
    line
}

/// Forward one output stream as base64 JSON-lines chunks. Returns `true` when
/// the consumer disconnected.
async fn pump_stream(
    id: u64,
    kill_on_disconnect: bool,
    sender: tokio::sync::mpsc::Sender<Vec<u8>>,
    mut stream: impl tokio::io::AsyncRead + Unpin,
    stream_name: OutputStream,
) -> bool {
    let mut buffer = vec![0u8; READ_BUFFER_BYTES];
    loop {
        match stream.read(&mut buffer).await {
            Ok(0) | Err(_) => return false,
            Ok(read) => {
                let chunk = OutputChunk {
                    stream:   stream_name,
                    data_b64: base64::engine::general_purpose::STANDARD.encode(&buffer[..read]),
                };
                if sender.send(protocol_line(&chunk)).await.is_err() {
                    if kill_on_disconnect {
                        kill_process_group(id);
                    }
                    return true;
                }
            }
        }
    }
}

/// SIGTERM the process group, escalating to SIGKILL after the grace period.
fn kill_process_group(id: u64) {
    let pgid = {
        let processes = processes().lock().expect("process table lock");
        processes.get(&id).map(|entry| entry.pgid)
    };
    let Some(pgid) = pgid else {
        return;
    };
    #[allow(
        unsafe_code,
        reason = "Signalling a process group has no safe std wrapper; the \
                  pgid comes from the spawn record the agent itself created."
    )]
    unsafe {
        libc::killpg(pgid, libc::SIGTERM);
    }
    std::thread::spawn(move || {
        std::thread::sleep(TERMINATE_GRACE);
        let still_tracked = {
            let processes = processes().lock().expect("process table lock");
            processes.contains_key(&id)
        };
        if still_tracked {
            #[allow(
                unsafe_code,
                reason = "Escalating the SIGTERM to SIGKILL uses the same \
                          agent-created process group as the initial signal."
            )]
            unsafe {
                libc::killpg(pgid, libc::SIGKILL);
            }
        }
    });
}

#[derive(Clone)]
struct AgentState {
    token: std::sync::Arc<str>,
}

fn authorized(state: &AgentState, headers: &HeaderMap) -> bool {
    let expected = format!("Bearer {}", state.token);
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| constant_time_eq(value.as_bytes(), expected.as_bytes()))
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in left.iter().zip(right.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

async fn healthz() -> &'static str {
    "ok"
}

fn bad_request(message: &str) -> Response {
    (StatusCode::BAD_REQUEST, message.to_string()).into_response()
}

fn body_from_lines(receiver: tokio::sync::mpsc::Receiver<Vec<u8>>) -> axum::body::Body {
    axum::body::Body::from_stream(tokio_stream_wrapped(receiver))
}

fn tokio_stream_wrapped(
    receiver: tokio::sync::mpsc::Receiver<Vec<u8>>,
) -> impl futures_core::Stream<Item = std::result::Result<bytes::Bytes, std::io::Error>> {
    ByteStream { receiver }
}

struct ByteStream {
    receiver: tokio::sync::mpsc::Receiver<Vec<u8>>,
}

impl futures_core::Stream for ByteStream {
    type Item = std::result::Result<bytes::Bytes, std::io::Error>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.receiver
            .poll_recv(cx)
            .map(|chunk| chunk.map(|bytes| Ok(bytes::Bytes::from(bytes))))
    }
}

async fn exec_handler(
    State(state): State<AgentState>,
    headers: HeaderMap,
    Json(request): Json<ExecRequest>,
) -> Response {
    if !authorized(&state, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let stdin_bytes = match request.stdin_b64.as_deref() {
        Some(encoded) => match base64::engine::general_purpose::STANDARD.decode(encoded) {
            Ok(bytes) if bytes.len() <= MAX_STDIN_BYTES => Some(bytes),
            Ok(_) => return bad_request("stdin exceeds the size limit"),
            Err(err) => return bad_request(&format!("invalid stdin base64: {err}")),
        },
        None => None,
    };

    let (id, _pid) = match spawn_process(&request) {
        Ok(spawned) => spawned,
        Err(message) => return bad_request(&message),
    };

    // /exec stdin semantics: write the upfront bytes, close, then read.
    if let Some(stdin_bytes) = stdin_bytes {
        let child_stdin = {
            let mut processes = processes().lock().expect("process table lock");
            processes.get_mut(&id).and_then(|entry| entry.stdin.take())
        };
        if let Some(mut child_stdin) = child_stdin {
            if child_stdin.write_all(&stdin_bytes).await.is_err() {
                kill_process_group(id);
            }
            let _ = child_stdin.shutdown().await;
        }
    }

    let (sender, receiver) = tokio::sync::mpsc::channel(16);
    tokio::spawn(stream_process(id, true, sender));
    Response::new(body_from_lines(receiver))
}

async fn spawn_handler(
    State(state): State<AgentState>,
    headers: HeaderMap,
    Json(request): Json<ExecRequest>,
) -> Response {
    if !authorized(&state, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    // Spawned processes are long-lived; upfront stdin is not part of this
    // endpoint's contract (use /exec).
    if request.stdin_b64.is_some() {
        return bad_request("spawn does not accept stdin");
    }

    let (id, _pid) = match spawn_process(&request) {
        Ok(spawned) => spawned,
        Err(message) => return bad_request(&message),
    };

    let (sender, receiver) = tokio::sync::mpsc::channel(16);
    tokio::spawn(stream_process(id, false, sender));
    let mut response = Response::new(body_from_lines(receiver));
    response
        .headers_mut()
        .insert("x-fabro-process-id", id.into());
    response
}

#[derive(Debug, serde::Deserialize)]
struct StdinRequest {
    #[serde(default)]
    data_b64: String,
    #[serde(default)]
    close:    bool,
}

async fn stdin_handler(
    State(state): State<AgentState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(request): Json<StdinRequest>,
) -> Response {
    if !authorized(&state, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Ok(process_id) = id.parse::<u64>() else {
        return bad_request("invalid process id");
    };
    let bytes = match base64::engine::general_purpose::STANDARD.decode(request.data_b64.as_bytes())
    {
        Ok(bytes) if bytes.len() <= MAX_STDIN_BYTES => bytes,
        Ok(_) => return bad_request("stdin exceeds the size limit"),
        Err(err) => return bad_request(&format!("invalid stdin base64: {err}")),
    };

    let child_stdin = {
        let mut processes = processes().lock().expect("process table lock");
        processes
            .get_mut(&process_id)
            .and_then(|entry| entry.stdin.take())
    };
    let Some(mut child_stdin) = child_stdin else {
        // Either the process is gone or its stdin already closed.
        return StatusCode::CONFLICT.into_response();
    };
    if !bytes.is_empty() && child_stdin.write_all(&bytes).await.is_err() {
        return StatusCode::CONFLICT.into_response();
    }
    if request.close {
        let _ = child_stdin.shutdown().await;
    } else {
        // The handle goes back so later writes still work.
        let mut processes = processes().lock().expect("process table lock");
        if let Some(entry) = processes.get_mut(&process_id) {
            entry.stdin = Some(child_stdin);
        }
    }
    StatusCode::OK.into_response()
}

async fn terminate_handler(
    State(state): State<AgentState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if !authorized(&state, &headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Ok(process_id) = id.parse::<u64>() else {
        return bad_request("invalid process id");
    };
    let tracked = {
        let processes = processes().lock().expect("process table lock");
        processes.contains_key(&process_id)
    };
    if !tracked {
        return StatusCode::NOT_FOUND.into_response();
    }
    kill_process_group(process_id);
    StatusCode::OK.into_response()
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("serve") => serve(&args[1..]),
        Some("self-install") => self_install(&args[1..]),
        Some("healthz") => healthz_probe(&args[1..]),
        _ => Err(anyhow::anyhow!(
            "usage: fabro-sandbox-agent <serve|self-install|healthz> [args]"
        )),
    }
}

fn serve(args: &[String]) -> anyhow::Result<()> {
    let port = parse_port_argument(args)?;
    let token = std::env::var("FABRO_KUBERNETES_AGENT_TOKEN")
        .map_err(|_| anyhow::anyhow!("FABRO_KUBERNETES_AGENT_TOKEN is required"))?;
    if token.is_empty() {
        anyhow::bail!("FABRO_KUBERNETES_AGENT_TOKEN is empty");
    }
    // The token must not leak into spawned processes' environments.
    std::env::remove_var("FABRO_KUBERNETES_AGENT_TOKEN");

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    spawn_reaper();

    let state = AgentState {
        token: std::sync::Arc::from(token.as_str()),
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let app = Router::new()
            .route("/healthz", get(healthz))
            .route("/exec", post(exec_handler))
            .route("/processes", post(spawn_handler))
            .route("/processes/{id}/stdin", post(stdin_handler))
            .route("/processes/{id}/terminate", post(terminate_handler))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
        tracing::info!(port, "sandbox agent listening");
        axum::serve(listener, app).await?;
        Ok::<_, anyhow::Error>(())
    })?;
    Ok(())
}

fn parse_port_argument(args: &[String]) -> anyhow::Result<u16> {
    let mut port: Option<u16> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--port" if index + 1 < args.len() => {
                port = Some(args[index + 1].parse()?);
                index += 1;
            }
            other => anyhow::bail!("unknown argument: {other}"),
        }
        index += 1;
    }
    Ok(port.unwrap_or(7800))
}

fn self_install(args: &[String]) -> anyhow::Result<()> {
    let mut target: Option<String> = None;
    let mut prepare_dirs: Vec<String> = Vec::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--prepare-dir" if index + 1 < args.len() => {
                prepare_dirs.push(args[index + 1].clone());
                index += 1;
            }
            other if target.is_none() && !other.starts_with("--") => {
                target = Some(other.to_string());
            }
            other => anyhow::bail!("unknown self-install argument: {other}"),
        }
        index += 1;
    }
    let target = target.ok_or_else(|| anyhow::anyhow!("self-install requires a target path"))?;

    // The distroless agent image has no shell or coreutils, so the agent
    // copies its own image binary out through /proc/self/exe and prepares the
    // workspace directories the main container's unknown user must be able to
    // write.
    for dir in &prepare_dirs {
        std::fs::create_dir_all(dir).map_err(|err| anyhow::anyhow!("creating {dir}: {err}"))?;
        set_mode(dir, 0o777);
    }

    if let Some(parent) = std::path::Path::new(&target).parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| anyhow::anyhow!("creating {}: {err}", parent.display()))?;
    }
    std::fs::copy("/proc/self/exe", &target)
        .map_err(|err| anyhow::anyhow!("installing agent binary to {target}: {err}"))?;
    set_mode(&target, 0o755);
    Ok(())
}

#[cfg(target_os = "linux")]
fn set_mode(path: &str, mode: u32) {
    use std::os::unix::fs::PermissionsExt as _;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
}

#[cfg(not(target_os = "linux"))]
fn set_mode(_path: &str, _mode: u32) {}

fn healthz_probe(args: &[String]) -> anyhow::Result<()> {
    let port = parse_port_argument(args)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let reachable = runtime.block_on(async {
        tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_ok()
    });
    if reachable {
        Ok(())
    } else {
        anyhow::bail!("agent is not listening on port {port}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wait_status_decodes_exit_codes_and_signals() {
        // Raw wait status for a normal exit with code 7 is `7 << 8`.
        assert_eq!(decode_wait_status(7 << 8), ProcessExit {
            exit_code: Some(7),
            signalled: false,
        });
        // Raw status 9 means the child was killed by SIGKILL (signal 9).
        assert_eq!(decode_wait_status(9), ProcessExit {
            exit_code: Some(128 + 9),
            signalled: true,
        });
    }

    #[test]
    fn env_entries_split_on_first_equals() {
        assert_eq!(split_env_entry("A=B=C"), ("A", "B=C"));
        assert_eq!(split_env_entry("NOVALUE"), ("NOVALUE", ""));
    }

    #[test]
    fn port_argument_parses_with_default() {
        assert_eq!(
            parse_port_argument(&["--port".to_string(), "7801".to_string()]).unwrap(),
            7801
        );
        assert_eq!(parse_port_argument(&[]).unwrap(), 7800);
        assert!(parse_port_argument(&["--wat".to_string()]).is_err());
    }

    #[test]
    fn constant_time_eq_matches_only_equal_slices() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
    }
}
