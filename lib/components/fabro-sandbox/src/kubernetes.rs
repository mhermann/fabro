use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::Cursor;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use fabro_github::GitHubCredentials;
use fabro_github::token_source::InstallationTokenSource;
use fabro_types::settings::run::RunCloneSettings;
use fabro_types::{CommandOutputStream, CommandTermination, RunId, SandboxProviderKind};
use fabro_util::time::elapsed_ms;
use k8s_openapi::api::core::v1::{Container, EnvVar, Pod, PodSpec, ResourceRequirements};
use k8s_openapi::api::networking::v1::{
    IPBlock, NetworkPolicy, NetworkPolicyEgressRule, NetworkPolicyPeer, NetworkPolicyPort,
    NetworkPolicySpec,
};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{LabelSelector, ObjectMeta, OwnerReference};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::api::{Api, AttachParams, AttachedProcess, DeleteParams, PostParams};
use kube::{Client, Config};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _, duplex};
use tokio::sync::{Mutex as TokioMutex, Notify, OnceCell};
use tokio::{fs, time};
use tokio_util::sync::CancellationToken;

use crate::clone_source::{self, CloneDecision, EmptyWorkspaceReason};
use crate::git_retry::{self, CredentialContext};
use crate::managed_labels::{MANAGED_LABEL, MANAGED_LABEL_VALUE, RUN_ID_LABEL};
use crate::push_credentials::{self, PushCredentialState};
use crate::redact::redact_auth_url;
use crate::sandbox::{
    self, BASH_ENV_VAR, BASH_PROBE_SCRIPT, BASH_PROBE_TIMEOUT_MS, OutputCaptureBuffer, REMOTE_BASH,
    REMOTE_WALK_TIMEOUT_MS, RefreshOutcome, StdioProcessControl, optional_timeout, resolve_path,
    validate_bash_probe, write_process_stdin,
};
use crate::{
    CommandOutputCallback, DEFAULT_EXEC_OUTPUT_TAIL_BYTES, DirEntry, ExecResult,
    ExecStreamingRequest, ExecStreamingResult, GrepOptions, Sandbox, SandboxEvent,
    SandboxEventCallback, SandboxFile, StderrCollector, StdioProcess, StdioProcessHandle,
    StdioProcessTermination, WalkOptions, format_lines_numbered, shell_quote,
};

const KUBERNETES_BASH_REQUIREMENT: &str = "Kubernetes sandboxes require /bin/bash for every \
     command, with no `sh` fallback; use an image with bash and git, such as buildpack-deps:noble.";

pub(crate) const WORKING_DIRECTORY: &str = "/workspace";
pub(crate) const REPOS_ROOT: &str = "/repos";
// Beneath the system tmp dir so any container user can create it; the
// trailing `runtime` component is load-bearing — materialized blobs at
// `runtime/blobs/{hash}.json` are recognized as managed blob references and
// normalized back to `blob://` in durable context.
pub(crate) const RUNTIME_DIRECTORY: &str = "/tmp/fabro/runtime";
const DEFAULT_GIT_CLONE_DEPTH: usize = RunCloneSettings::DEFAULT_DEPTH.unsigned_abs() as usize;
const GIT_CLONE_TIMEOUT: Duration = Duration::from_mins(5);
/// Image pull can legitimately take minutes on a cold node; allow the same
/// budget the git clone path gives before giving up on the pod.
const POD_READY_TIMEOUT: Duration = Duration::from_mins(5);
const POD_READY_POLL_INTERVAL: Duration = Duration::from_secs(2);
/// The single Fabro container inside the sandbox pod.
pub(crate) const POD_CONTAINER_NAME: &str = "fabro";
/// Label holding the sandbox (pod) name; the NetworkPolicy pod selector uses
/// it.
pub(crate) const SANDBOX_LABEL: &str = "sh.fabro.sandbox";

#[cfg(test)]
const EXEC_STOP_POLL_SLEEP_SECONDS: &str = "0.005";
#[cfg(not(test))]
const EXEC_STOP_POLL_SLEEP_SECONDS: &str = "0.1";
#[cfg(test)]
const EXEC_TERM_GRACE_SECONDS: &str = "0.02";
#[cfg(not(test))]
const EXEC_TERM_GRACE_SECONDS: &str = "0.2";

static EXEC_CONTROL_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Egress policy enforced by a per-pod NetworkPolicy.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum KubernetesNetworkMode {
    /// No NetworkPolicy; pod egress follows the cluster default.
    #[default]
    AllowAll,
    /// Deny all egress.
    Block,
    /// Allow only the listed CIDRs (plus cluster DNS for name resolution).
    CidrAllowList(Vec<String>),
}

/// Options for a Kubernetes sandbox pod.
#[derive(Clone, Debug, PartialEq)]
pub struct KubernetesSandboxOptions {
    /// Pod container image reference.
    pub image:                   String,
    /// Additional `KEY=VALUE` environment variables for the container.
    pub env_vars:                Vec<String>,
    /// Memory limit in bytes. `None` = cluster default.
    pub memory_limit:            Option<i64>,
    /// CPU cores. `None` = cluster default.
    pub cpu:                     Option<i64>,
    /// Ephemeral storage limit in bytes. `None` = cluster default.
    pub ephemeral_storage_limit: Option<i64>,
    /// Egress policy enforced by a per-pod NetworkPolicy.
    pub network:                 KubernetesNetworkMode,
    /// User labels applied to the pod; Fabro's managed labels override any
    /// reserved keys.
    pub labels:                  HashMap<String, String>,
    /// Maximum Git history depth fetched during clone; `None` fetches full
    /// history.
    pub clone_depth:             Option<usize>,
    /// Create an empty workspace instead of cloning even when an origin exists.
    pub skip_clone:              bool,
}

impl Default for KubernetesSandboxOptions {
    fn default() -> Self {
        Self {
            image:                   "buildpack-deps:noble".to_string(),
            env_vars:                Vec::new(),
            memory_limit:            None,
            cpu:                     None,
            ephemeral_storage_limit: None,
            network:                 KubernetesNetworkMode::AllowAll,
            labels:                  HashMap::new(),
            clone_depth:             Some(DEFAULT_GIT_CLONE_DEPTH),
            skip_clone:              false,
        }
    }
}

pub struct KubernetesSandbox {
    client:            Client,
    namespace:         String,
    config:            KubernetesSandboxOptions,
    push_credentials:  PushCredentialState,
    run_id:            Option<RunId>,
    clone_origin_url:  Option<String>,
    clone_branch:      Option<String>,
    clone_tag:         Option<String>,
    clone_commit_sha:  Option<String>,
    pod_name:          OnceCell<String>,
    repo_cloned:       OnceCell<bool>,
    working_directory: OnceCell<String>,
    origin_url:        OnceCell<String>,
    cached_platform:   std::sync::OnceLock<String>,
    cached_os_version: std::sync::OnceLock<String>,
    rg_available:      OnceCell<bool>,
    event_callback:    Option<SandboxEventCallback>,
}

/// Strip the exit-code sentinel from a completed exec stream and report the
/// decoded exit code.
///
/// The Kubernetes exec API has no per-exec exit code, so the wrapped command
/// prints `<marker>=<status>` as its final stdout line. The reader holds back
/// the trailing (possibly unterminated) line and only releases it once more
/// output proves it was not the sentinel; `finish` anchors on the sentinel's
/// last occurrence so binary payloads without a trailing newline are stripped
/// cleanly.
struct ExecSentinelReader {
    marker: String,
    held:   Vec<u8>,
}

impl ExecSentinelReader {
    fn new(marker: String) -> Self {
        Self {
            marker,
            held: Vec::new(),
        }
    }

    /// Ingest a chunk and return the bytes safe to deliver downstream.
    fn push(&mut self, chunk: &[u8]) -> Vec<u8> {
        self.held.extend_from_slice(chunk);
        self.drain_releaseable()
    }

    /// Release everything except the trailing line (complete or not), which
    /// may still turn out to be the sentinel.
    fn drain_releaseable(&mut self) -> Vec<u8> {
        let Some(last_newline) = self.held.iter().rposition(|&byte| byte == b'\n') else {
            return Vec::new();
        };
        // The trailing line starts after the previous newline; everything
        // before it can no longer be part of the sentinel.
        let hold_from = self.held[..last_newline]
            .iter()
            .rposition(|&byte| byte == b'\n')
            .map_or(0, |position| position + 1);
        self.held.drain(..hold_from).collect()
    }

    /// Complete the stream: strip the sentinel if present and return the
    /// remaining bytes plus the decoded exit code.
    fn finish(mut self) -> (Vec<u8>, Option<i32>) {
        let held = std::mem::take(&mut self.held);
        let newline_len = usize::from(held.ends_with(b"\n"));
        let line = held.strip_suffix(b"\n".as_slice()).unwrap_or(&held);
        if let Some(code) = parse_sentinel(line, &self.marker) {
            let suffix_len = sentinel_pattern(&self.marker).len() + digits_len(line);
            let keep = held.len() - newline_len - suffix_len;
            let mut stripped = held;
            stripped.truncate(keep);
            return (stripped, Some(code));
        }
        (held, None)
    }
}

fn digits_len(line: &[u8]) -> usize {
    line.iter()
        .rev()
        .take_while(|byte| byte.is_ascii_digit())
        .count()
}

/// The full `<marker>=` pattern the wrapper prints before the status.
fn sentinel_pattern(marker: &str) -> Vec<u8> {
    let mut pattern = marker.as_bytes().to_vec();
    pattern.push(b'=');
    pattern
}

/// Parse `<marker>=<digits>` anchored at the end of `line`.
///
/// The last occurrence wins: a payload that merely contains the marker text
/// keeps its bytes and only the trailing sentinel is stripped.
fn parse_sentinel(line: &[u8], marker: &str) -> Option<i32> {
    let pattern = sentinel_pattern(marker);
    if line.len() <= pattern.len() {
        return None;
    }
    let start = line
        .windows(pattern.len())
        .rposition(|window| window == pattern.as_slice())?;
    let digits = &line[start + pattern.len()..];
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    std::str::from_utf8(digits).ok()?.parse().ok()
}

fn exec_control_paths() -> (String, String, String) {
    let sequence = EXEC_CONTROL_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nonce = format!("{}-{}", std::process::id(), sequence);
    let prefix = format!("/tmp/fabro-exec-{nonce}");
    (
        format!("{prefix}.stop"),
        format!("{prefix}.pid"),
        format!("__FABRO_RC_{nonce}__"),
    )
}

/// Per-exec kill wiring: the stop/pid file paths baked into the wrapper script
/// and the unique stdout sentinel the reader strips.
struct ExecControl {
    stop_file: String,
    pid_file:  String,
    marker:    String,
}

impl ExecControl {
    fn new() -> Self {
        let (stop_file, pid_file, marker) = exec_control_paths();
        Self {
            stop_file,
            pid_file,
            marker,
        }
    }
}

/// Blank the caller's `BASH_ENV` and forward `KEY=VALUE` entries through `env`
/// ahead of the wrapped shell.
fn kubernetes_env_prefix(env_vars: &[String]) -> String {
    let mut prefix = String::from("env");
    for entry in env_vars {
        let _ = write!(prefix, " {}", shell_quote(entry));
    }
    let _ = write!(prefix, " {BASH_ENV_VAR}=");
    prefix
}

fn kubernetes_exec_argv(script: String) -> Vec<String> {
    vec![
        "env".to_string(),
        format!("{BASH_ENV_VAR}="),
        REMOTE_BASH.to_string(),
        "-c".to_string(),
        script,
    ]
}

/// Build the per-exec shell wrapper.
///
/// The Kubernetes exec API carries no working-directory or environment
/// parameters, so the wrapper `cd`s into the resolved directory and prefixes
/// the user command with `env`. The command runs under `setsid` in its own
/// process group so the stop-file watcher can terminate the whole tree, and
/// the final sentinel line carries the exit code the exec API does not.
fn controlled_shell_command(
    command: &str,
    stop_file: &str,
    pid_file: &str,
    marker: &str,
    cwd: &str,
    env_vars: &[String],
) -> String {
    format!(
        "\
cd {cwd} || {{ echo \"{marker}=$?\"; exit 1; }}; \
stop_file={stop_file}; \
pid_file={pid_file}; \
user_command={command}; \
rm -f \"$pid_file\"; \
if [ -e \"$stop_file\" ]; then \
  rm -f \"$stop_file\" \"$pid_file\"; \
  exit 143; \
fi; \
( \
  while [ ! -e \"$stop_file\" ]; do sleep {stop_poll_sleep}; done; \
  while [ ! -s \"$pid_file\" ]; do sleep {stop_poll_sleep}; done; \
  child=$(cat \"$pid_file\"); \
  kill -TERM \"-$child\" 2>/dev/null || kill -TERM \"$child\" 2>/dev/null || true; \
  sleep {term_grace}; \
  kill -KILL \"-$child\" 2>/dev/null || kill -KILL \"$child\" 2>/dev/null || true; \
) & watcher=$!; \
{env_prefix} setsid {bash} -c \"$user_command\" & \
child=$!; \
echo \"$child\" > \"$pid_file\"; \
wait \"$child\"; \
status=$?; \
kill \"$watcher\" 2>/dev/null || true; \
wait \"$watcher\" 2>/dev/null || true; \
rm -f \"$stop_file\" \"$pid_file\"; \
echo \"{marker}=$status\"\
",
        cwd = shell_quote(cwd),
        marker = marker,
        stop_file = shell_quote(stop_file),
        pid_file = shell_quote(pid_file),
        command = shell_quote(command),
        stop_poll_sleep = EXEC_STOP_POLL_SLEEP_SECONDS,
        term_grace = EXEC_TERM_GRACE_SECONDS,
        env_prefix = kubernetes_env_prefix(env_vars),
        bash = REMOTE_BASH,
    )
}

/// Build a terminate request exec that releases a running controlled command.
fn exec_stop_request_script(stop_file: &str) -> String {
    format!(
        "rm -f {} && touch {}",
        shell_quote(stop_file),
        shell_quote(stop_file)
    )
}

impl KubernetesSandbox {
    pub async fn new(
        config: KubernetesSandboxOptions,
        github_app: Option<&GitHubCredentials>,
        run_id: Option<RunId>,
        clone_origin_url: Option<String>,
        clone_branch: Option<String>,
        clone_tag: Option<String>,
        clone_commit_sha: Option<String>,
    ) -> crate::Result<Self> {
        if clone_tag.is_some() || clone_commit_sha.is_some() {
            clone_source::decide_clone(
                config.skip_clone,
                clone_origin_url.as_deref(),
                clone_branch.as_deref(),
                clone_tag.as_deref(),
                clone_commit_sha.as_deref(),
            )?;
        }
        let (client, namespace) = connect().await?;
        Self::with_client(
            client,
            namespace,
            config,
            github_app,
            run_id,
            clone_origin_url,
            clone_branch,
            clone_tag,
            clone_commit_sha,
        )
    }

    fn with_client(
        client: Client,
        namespace: String,
        config: KubernetesSandboxOptions,
        github_app: Option<&GitHubCredentials>,
        run_id: Option<RunId>,
        clone_origin_url: Option<String>,
        clone_branch: Option<String>,
        clone_tag: Option<String>,
        clone_commit_sha: Option<String>,
    ) -> crate::Result<Self> {
        let push_credentials = PushCredentialState::new(push_credentials::build_token_source(
            github_app,
            clone_origin_url.as_deref(),
        )?);
        Ok(Self {
            client,
            namespace,
            config,
            push_credentials,
            run_id,
            clone_origin_url,
            clone_branch,
            clone_tag,
            clone_commit_sha,
            pod_name: OnceCell::new(),
            repo_cloned: OnceCell::new(),
            working_directory: OnceCell::new(),
            origin_url: OnceCell::new(),
            cached_platform: std::sync::OnceLock::new(),
            cached_os_version: std::sync::OnceLock::new(),
            rg_available: OnceCell::const_new(),
            event_callback: None,
        })
    }

    pub async fn reconnect(
        pod_name: &str,
        repo_cloned: bool,
        working_directory: String,
        clone_origin_url: Option<String>,
        clone_branch: Option<String>,
        run_id: Option<RunId>,
    ) -> crate::Result<Self> {
        let sandbox = Self::new(
            KubernetesSandboxOptions::default(),
            None,
            run_id,
            clone_origin_url.clone(),
            clone_branch,
            None,
            None,
        )
        .await?;
        sandbox.validate_managed_pod(pod_name).await?;
        sandbox
            .pod_name
            .set(pod_name.to_string())
            .map_err(|_| "Pod already initialized".to_string())?;
        sandbox
            .repo_cloned
            .set(repo_cloned)
            .map_err(|_| "Clone state already initialized".to_string())?;
        sandbox
            .working_directory
            .set(working_directory)
            .map_err(|_| "Working directory already initialized".to_string())?;
        if repo_cloned {
            if let Some(origin) = clone_origin_url {
                let _ = sandbox.origin_url.set(origin);
            }
        }
        Ok(sandbox)
    }

    pub fn set_event_callback(&mut self, cb: SandboxEventCallback) {
        self.event_callback = Some(cb);
    }

    fn emit(&self, event: SandboxEvent) {
        event.trace();
        if let Some(ref cb) = self.event_callback {
            cb(event);
        }
    }

    fn pod_name(&self) -> crate::Result<&str> {
        self.pod_name.get().map(String::as_str).ok_or_else(|| {
            crate::Error::message("Sandbox not initialized — call initialize() first")
        })
    }

    pub(crate) fn pod_identifier(&self) -> crate::Result<&str> {
        self.pod_name()
    }

    fn namespace(&self) -> &str {
        &self.namespace
    }

    fn resolve_pod_path(&self, path: &str) -> String {
        resolve_path(path, self.working_directory())
    }

    fn repo_cloned(&self) -> bool {
        self.repo_cloned.get().copied().unwrap_or(false)
    }

    fn set_working_directory(&self, working_directory: impl Into<String>) -> crate::Result<()> {
        self.working_directory
            .set(working_directory.into())
            .map_err(|_| crate::Error::message("Kubernetes working directory already initialized"))
    }

    fn pods(&self) -> Api<Pod> {
        Api::namespaced(self.client.clone(), &self.namespace)
    }

    fn network_policies(&self) -> Api<NetworkPolicy> {
        Api::namespaced(self.client.clone(), &self.namespace)
    }

    async fn get_pod(&self, name: &str) -> crate::Result<Option<Pod>> {
        self.pods()
            .get_opt(name)
            .await
            .map_err(|err| crate::Error::context(format!("Failed to get pod '{name}'"), err))
    }

    async fn validate_managed_pod(&self, pod_name: &str) -> crate::Result<()> {
        let Some(pod) = self.get_pod(pod_name).await? else {
            return Err(crate::Error::message(format!(
                "Kubernetes pod '{pod_name}' is gone"
            )));
        };
        let labels = pod_labels(&pod);
        verify_managed_labels(pod_name, &labels, self.run_id.as_ref())
    }

    /// Create the sandbox pod and wait for it to become Ready.
    async fn create_pod(&self, pod_name: &str) -> crate::Result<Pod> {
        let pod = pod_manifest(pod_name, &self.config, self.run_id.as_ref());
        let created = self
            .pods()
            .create(&PostParams::default(), &pod)
            .await
            .map_err(|err| {
                if pod_already_exists(&err) {
                    crate::Error::message(format!(
                        "Kubernetes pod '{pod_name}' already exists. Remove the stale fabro pod \
                         manually before retrying."
                    ))
                } else {
                    crate::Error::context(
                        format!("Failed to create Kubernetes pod '{pod_name}'"),
                        err,
                    )
                }
            })?;

        // A per-pod NetworkPolicy owns egress enforcement; the pod owns it via
        // ownerReference so the API server garbage-collects both together.
        if !matches!(self.config.network, KubernetesNetworkMode::AllowAll) {
            let policy = network_policy_manifest(
                pod_name,
                created.metadata.uid.as_deref(),
                &self.config.network,
            );
            self.network_policies()
                .create(&PostParams::default(), &policy)
                .await
                .map_err(|err| {
                    crate::Error::context(
                        format!("Failed to create NetworkPolicy for pod '{pod_name}'"),
                        err,
                    )
                })?;
        }

        self.wait_pod_ready(pod_name).await?;
        Ok(created)
    }

    /// Poll the pod until its Ready condition is true or it reaches a terminal
    /// failure (image pull, crash loop, deadline).
    async fn wait_pod_ready(&self, pod_name: &str) -> crate::Result<()> {
        let deadline = time::Instant::now() + POD_READY_TIMEOUT;
        loop {
            let pod = self.get_pod(pod_name).await?.ok_or_else(|| {
                crate::Error::message(format!(
                    "Kubernetes pod '{pod_name}' disappeared while starting"
                ))
            })?;

            if let Some(reason) = terminal_failure_reason(&pod) {
                return Err(crate::Error::message(format!(
                    "Kubernetes pod '{pod_name}' failed to start ({reason}). {}",
                    image_remediation(&self.config.image, &reason)
                )));
            }
            if pod_is_ready(&pod) {
                return Ok(());
            }
            if time::Instant::now() >= deadline {
                return Err(crate::Error::message(format!(
                    "Timed out waiting for Kubernetes pod '{pod_name}' to become Ready after {}s",
                    POD_READY_TIMEOUT.as_secs()
                )));
            }
            time::sleep(POD_READY_POLL_INTERVAL).await;
        }
    }

    async fn pod_exec_raw(
        &self,
        argv: Vec<String>,
        attach_stdin: bool,
    ) -> crate::Result<AttachedProcess> {
        let pod_name = self.pod_name()?;
        let params = AttachParams {
            stdin: attach_stdin,
            stdout: true,
            stderr: true,
            tty: false,
            ..Default::default()
        };
        self.pods()
            .exec(pod_name, argv, &params)
            .await
            .map_err(|err| crate::Error::context("Failed to exec into Kubernetes pod", err))
    }

    /// Run a wrapped command, collect both streams through the sentinel
    /// reader, and return the stripped output plus the decoded exit code.
    ///
    /// Output is returned as raw bytes: tar-over-exec downloads carry binary
    /// payloads that must not pass through a lossy UTF-8 conversion.
    async fn kubernetes_exec(
        &self,
        command: &str,
        working_dir: &str,
        env_vars: &[String],
        control: &ExecControl,
    ) -> crate::Result<(Vec<u8>, Vec<u8>, i32)> {
        let script = controlled_shell_command(
            command,
            &control.stop_file,
            &control.pid_file,
            &control.marker,
            working_dir,
            env_vars,
        );
        let mut attached = self
            .pod_exec_raw(kubernetes_exec_argv(script), false)
            .await?;
        let mut stdout_reader = attached
            .stdout()
            .ok_or_else(|| crate::Error::message("Kubernetes exec started without stdout"))?;
        let mut stderr_reader = attached
            .stderr()
            .ok_or_else(|| crate::Error::message("Kubernetes exec started without stderr"))?;

        let mut sentinel = ExecSentinelReader::new(control.marker.clone());
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut stdout_chunk = vec![0u8; 8192];
        let mut stderr_chunk = vec![0u8; 8192];
        // Both streams carry data until the remote command exits; whichever
        // produces first is drained by the select, and the post-loop passes
        // below flush the stream that finishes last.
        loop {
            tokio::select! {
                biased;
                read = stdout_reader.read(&mut stdout_chunk) => {
                    match read {
                        Ok(0) | Err(_) => break,
                        Ok(n) => stdout.extend_from_slice(&sentinel.push(&stdout_chunk[..n])),
                    }
                }
                read = stderr_reader.read(&mut stderr_chunk) => {
                    match read {
                        Ok(0) | Err(_) => break,
                        Ok(n) => stderr.extend_from_slice(&stderr_chunk[..n]),
                    }
                }
            }
        }
        while let Ok(n) = stdout_reader.read(&mut stdout_chunk).await {
            if n == 0 {
                break;
            }
            stdout.extend_from_slice(&sentinel.push(&stdout_chunk[..n]));
        }
        while let Ok(n) = stderr_reader.read(&mut stderr_chunk).await {
            if n == 0 {
                break;
            }
            stderr.extend_from_slice(&stderr_chunk[..n]);
        }
        let _ = attached.join().await;
        let (tail, exit_code) = sentinel.finish();
        stdout.extend_from_slice(&tail);
        let exit_code = exit_code.unwrap_or(-1);
        Ok((stdout, stderr, exit_code))
    }

    async fn request_exec_stop(&self, stop_file: &str) -> crate::Result<()> {
        let script = exec_stop_request_script(stop_file);
        let attached = self
            .pod_exec_raw(kubernetes_exec_argv(script), false)
            .await?;
        let _ = attached.join().await;
        Ok(())
    }

    async fn kubernetes_exec_shell(
        &self,
        command: &str,
        timeout_ms: u64,
        working_dir: Option<&str>,
        env_vars: Option<&HashMap<String, String>>,
        cancel_token: Option<CancellationToken>,
    ) -> crate::Result<ExecResult> {
        let start = Instant::now();
        let effective_dir = working_dir
            .unwrap_or_else(|| self.working_directory())
            .to_string();
        let env = env_entries(env_vars);
        let control = ExecControl::new();

        let timeout_duration = Duration::from_millis(timeout_ms);
        let token = cancel_token.unwrap_or_default();

        tokio::select! {
            result = self.kubernetes_exec(command, &effective_dir, &env, &control) => {
                let (stdout, stderr, exit_code) = result?;
                let duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
                Ok(ExecResult {
                    stdout: String::from_utf8_lossy(&stdout).into_owned(),
                    stderr: String::from_utf8_lossy(&stderr).into_owned(),
                    exit_code: Some(exit_code),
                    termination: CommandTermination::Exited,
                    duration_ms,
                })
            }
            () = time::sleep(timeout_duration) => {
                self.request_exec_stop(&control.stop_file).await?;
                let duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
                Ok(ExecResult {
                    stdout: String::new(),
                    stderr: "Command timed out".to_string(),
                    exit_code: None,
                    termination: CommandTermination::TimedOut,
                    duration_ms,
                })
            }
            () = token.cancelled() => {
                self.request_exec_stop(&control.stop_file).await?;
                let duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
                Ok(ExecResult {
                    stdout: String::new(),
                    stderr: "Command cancelled".to_string(),
                    exit_code: None,
                    termination: CommandTermination::Cancelled,
                    duration_ms,
                })
            }
        }
    }

    async fn kubernetes_exec_streaming(
        &self,
        request: ExecStreamingRequest<'_>,
    ) -> crate::Result<ExecStreamingResult> {
        let ExecStreamingRequest {
            command,
            timeout_ms,
            working_dir,
            env_vars,
            cancel_token,
            stdin,
            output_callback,
            stream_output_bytes_cap,
        } = request;
        let start = Instant::now();
        let effective_dir = working_dir
            .unwrap_or_else(|| self.working_directory())
            .to_string();
        let env = env_entries(env_vars);
        let control = ExecControl::new();
        let script = controlled_shell_command(
            command,
            &control.stop_file,
            &control.pid_file,
            &control.marker,
            &effective_dir,
            &env,
        );
        let attach_stdin = stdin.is_some();

        let pod_name = self.pod_name()?.to_string();
        let namespace = self.namespace().to_string();
        let mut output_task = tokio::spawn(run_streaming_exec(
            self.client.clone(),
            namespace,
            pod_name,
            kubernetes_exec_argv(script),
            attach_stdin,
            stdin,
            control.marker.clone(),
            output_callback,
            stream_output_bytes_cap,
        ));

        let timeout_future = optional_timeout(timeout_ms);
        tokio::pin!(timeout_future);
        let token = cancel_token.unwrap_or_default();

        let mut termination = CommandTermination::Exited;
        let output = tokio::select! {
            joined = &mut output_task => {
                joined
                    .map_err(|e| crate::Error::context("Kubernetes exec stream task failed", e))??
            }
            () = &mut timeout_future => {
                termination = CommandTermination::TimedOut;
                self.request_exec_stop(&control.stop_file).await?;
                output_task
                    .await
                    .map_err(|e| crate::Error::context("Kubernetes exec stream task failed", e))??
            }
            () = token.cancelled() => {
                termination = CommandTermination::Cancelled;
                self.request_exec_stop(&control.stop_file).await?;
                output_task
                    .await
                    .map_err(|e| crate::Error::context("Kubernetes exec stream task failed", e))??
            }
        };

        let (stdout, stderr, exit_code) = output;
        let (stdout, stdout_capture) = stdout.into_parts();
        let (stderr, stderr_capture) = stderr.into_parts();
        let duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
        Ok(ExecStreamingResult {
            result: ExecResult {
                stdout: String::from_utf8_lossy(&stdout).into_owned(),
                stderr: String::from_utf8_lossy(&stderr).into_owned(),
                exit_code: (termination == CommandTermination::Exited).then_some(exit_code),
                termination,
                duration_ms,
            },
            streams_separated: true,
            live_streaming: true,
            stdout_capture,
            stderr_capture,
        })
    }

    /// Download a file as a tar stream over exec.
    ///
    /// The raw bytes variant is required: archive payloads must not pass
    /// through a lossy UTF-8 conversion the way text exec output does.
    async fn download_file_bytes(&self, remote_path: &str) -> crate::Result<Vec<u8>> {
        let pod_path = self.resolve_pod_path(remote_path);
        let command = format!("tar -cf - {}", shell_quote(&pod_path));
        let control = ExecControl::new();
        let (stdout, stderr, exit_code) = tokio::select! {
            result = self.kubernetes_exec(&command, "/", &[], &control) => result?,
            () = time::sleep(Duration::from_secs(30)) => {
                self.request_exec_stop(&control.stop_file).await?;
                return Err(crate::Error::message(format!(
                    "Timed out downloading {pod_path} from pod"
                )));
            }
        };
        if exit_code != 0 {
            return Err(crate::Error::exec(
                format!("Failed to download {pod_path} from pod"),
                ExecResult {
                    stdout:      String::from_utf8_lossy(&stdout).into_owned(),
                    stderr:      String::from_utf8_lossy(&stderr).into_owned(),
                    exit_code:   Some(exit_code),
                    termination: CommandTermination::Exited,
                    duration_ms: 0,
                },
            ));
        }
        extract_single_file_tar(&stdout, &pod_path)
    }
    async fn upload_bytes_to_pod(&self, path: &str, bytes: &[u8]) -> crate::Result<()> {
        let pod_path = self.resolve_pod_path(path);
        let parent_dir = std::path::Path::new(&pod_path)
            .parent()
            .map_or_else(|| "/".to_string(), |p| p.to_string_lossy().to_string());
        let file_name = std::path::Path::new(&pod_path)
            .file_name()
            .ok_or_else(|| crate::Error::message(format!("Invalid path: {pod_path}")))?
            .to_string_lossy()
            .to_string();

        // Fabro runtime files stay owner-private; repository files keep the
        // conventional world-readable mode.
        let is_runtime_path = pod_path.starts_with(&format!("{RUNTIME_DIRECTORY}/"));
        let mkdir_cmd = if is_runtime_path {
            format!("umask 077 && mkdir -p {}", shell_quote(&parent_dir))
        } else {
            format!("mkdir -p {}", shell_quote(&parent_dir))
        };
        let result = self
            .kubernetes_exec_shell(&mkdir_cmd, 10_000, Some("/"), None, None)
            .await?;
        if !result.is_success() {
            return Err(crate::Error::message(format!(
                "Failed to create parent dirs for {pod_path}: {}",
                result.stderr
            )));
        }

        let file_mode = if is_runtime_path { 0o600 } else { 0o644 };
        let tar_bytes = build_single_file_tar(&file_name, bytes, file_mode)?;
        // There is no pod copy API: feed the archive to `tar -x` over stdin
        // (the same technique `kubectl cp` uses).
        let result = self
            .kubernetes_exec_streaming(ExecStreamingRequest {
                command: &format!("tar -xpf - -C {}", shell_quote(&parent_dir)),
                timeout_ms: Some(30_000),
                working_dir: Some("/"),
                stdin: Some(tar_bytes),
                ..ExecStreamingRequest::new("")
            })
            .await?
            .result;
        if !result.is_success() {
            return Err(crate::Error::message(format!(
                "Failed to upload {pod_path} (exit {}): {}",
                result.display_exit_code(),
                result.stderr
            )));
        }
        Ok(())
    }

    async fn probe_bash(&self, working_dir: Option<&str>) -> crate::Result<()> {
        let result = self
            .kubernetes_exec_shell(
                BASH_PROBE_SCRIPT,
                BASH_PROBE_TIMEOUT_MS,
                Some(working_dir.unwrap_or("/")),
                None,
                None,
            )
            .await
            .map_err(|err| crate::Error::context(KUBERNETES_BASH_REQUIREMENT, err))?;
        validate_bash_probe(result, KUBERNETES_BASH_REQUIREMENT)
    }

    async fn create_workspace(&self) -> crate::Result<()> {
        let result = self
            .kubernetes_exec_shell(
                &format!("mkdir -p {}", shell_quote(WORKING_DIRECTORY)),
                10_000,
                Some("/"),
                None,
                None,
            )
            .await?;
        if !result.is_success() {
            return Err(crate::Error::message(format!(
                "Failed to create Kubernetes workspace (exit {}): {}",
                result.display_exit_code(),
                result.stderr
            )));
        }
        self.set_working_directory(WORKING_DIRECTORY)?;
        Ok(())
    }

    /// Create the run-scoped Fabro runtime directory outside the repository
    /// checkout. The umask keeps every created level owner-private.
    async fn create_runtime_directory(&self) -> crate::Result<()> {
        let result = self
            .kubernetes_exec_shell(
                &format!("umask 077 && mkdir -p {}", shell_quote(RUNTIME_DIRECTORY)),
                10_000,
                Some("/"),
                None,
                None,
            )
            .await?;
        if !result.is_success() {
            return Err(crate::Error::message(format!(
                "Failed to create Kubernetes runtime directory (exit {}): {}",
                result.display_exit_code(),
                result.stderr
            )));
        }
        Ok(())
    }

    async fn verify_git_available(&self) -> crate::Result<()> {
        let result = self
            .kubernetes_exec_shell("git --version", 10_000, Some("/"), None, None)
            .await?;
        if !result.is_success() {
            return Err(crate::Error::message(format!(
                "Kubernetes image '{}' must include git for repository clone and git lifecycle operations. Use an image with bash and git, such as buildpack-deps:noble.",
                self.config.image
            )));
        }
        Ok(())
    }

    /// Preserve a failed git step result while masking the auth URL.
    fn clone_failure_error(
        &self,
        result: ExecResult,
        label: &'static str,
        auth_url: Option<&fabro_redact::DisplaySafeUrl>,
        step: CloneStep,
    ) -> crate::Error {
        let error =
            result.into_exec_error_with_redactor(label, |output| redact_auth_url(output, auth_url));
        let message = match step {
            CloneStep::Network if self.push_credentials.source().is_none() => {
                "Git clone failed. If this is a private repository, configure a GitHub App with \
                 `fabro install` and install it for your organization."
            }
            CloneStep::Network => "Failed to clone repository into Kubernetes sandbox",
            CloneStep::Local => "Failed to prepare the cloned repository in the Kubernetes sandbox",
        };
        crate::Error::context(message, error)
    }

    fn report_clone_failure(&self, origin_url: &str, err: crate::Error) -> crate::Error {
        self.emit(SandboxEvent::GitCloneFailed {
            url:    origin_url.to_string(),
            error:  err.to_string(),
            causes: err.causes(),
        });
        err
    }

    /// Run a local (non-network) step of the exact checkout under the shared
    /// clone deadline.
    ///
    /// Materializing a large working tree takes far longer than the short fixed
    /// timeout used for trivial pod commands, so these steps get the same
    /// budget the branch clone path gives its fetch and checkout.
    async fn run_exact_local_git_command(
        &self,
        command: &str,
        label: &'static str,
        clone_deadline: time::Instant,
        auth_url: Option<&fabro_redact::DisplaySafeUrl>,
    ) -> crate::Result<ExecResult> {
        let remaining = clone_deadline.saturating_duration_since(time::Instant::now());
        let timeout_ms = u64::try_from(remaining.as_millis()).unwrap_or(u64::MAX);
        if timeout_ms == 0 {
            return Err(crate::Error::message(format!(
                "{label} deadline expired before the step could run"
            )));
        }
        let result = self
            .kubernetes_exec_shell(command, timeout_ms, Some("/"), None, None)
            .await
            .map_err(|error| crate::Error::context(format!("{label} transport failed"), error))?;
        if result.is_success() {
            Ok(result)
        } else {
            Err(self.clone_failure_error(result, label, auth_url, CloneStep::Local))
        }
    }

    /// Run a network git command inside the pod with clone retry semantics
    /// under the shared clone deadline.
    async fn retry_git_transfer(
        &self,
        command: &str,
        op: &'static str,
        label: &'static str,
        exec_label: &'static str,
        clone_deadline: time::Instant,
        credential_context: CredentialContext,
        auth_url: Option<&fabro_redact::DisplaySafeUrl>,
    ) -> Result<(), KubernetesCloneFailure> {
        let plan = git_retry::RetryPlan::clone_default(Some(clone_deadline));
        git_retry::retry_git_operation(
            SandboxProviderKind::Kubernetes,
            op,
            &plan,
            |_attempt| async move {
                let remaining = clone_deadline.saturating_duration_since(time::Instant::now());
                let timeout_ms = u64::try_from(remaining.as_millis()).unwrap_or(u64::MAX);
                if timeout_ms == 0 {
                    return Err(KubernetesCloneFailure {
                        error:        crate::Error::message(format!(
                            "{label} deadline expired before retry"
                        )),
                        retry_reason: None,
                    });
                }
                let result = self
                    .kubernetes_exec_streaming(ExecStreamingRequest {
                        timeout_ms: Some(timeout_ms),
                        working_dir: Some("/"),
                        ..ExecStreamingRequest::new(command)
                    })
                    .await
                    .map_err(|error| KubernetesCloneFailure {
                        error:        crate::Error::context(
                            format!("{label} transport failed"),
                            error,
                        ),
                        retry_reason: None,
                    })?
                    .result;
                if result.is_success() {
                    return Ok(());
                }
                let retry_reason = classify_kubernetes_clone_result(&result, credential_context);
                Err(KubernetesCloneFailure {
                    error: self.clone_failure_error(
                        result,
                        exec_label,
                        auth_url,
                        CloneStep::Network,
                    ),
                    retry_reason,
                })
            },
            |failure: &KubernetesCloneFailure| failure.retry_reason,
        )
        .await
    }

    async fn clone_github_repo(
        &self,
        origin_url: String,
        branch: Option<String>,
        tag: Option<String>,
        commit_sha: Option<String>,
    ) -> crate::Result<()> {
        self.verify_git_available().await?;
        let layout = clone_source::github_repo_layout(&origin_url, WORKING_DIRECTORY, REPOS_ROOT)?;
        // The clone mints its own token (never a warm-cache reuse) and seeds
        // the shared source, so the first refresh compares against the clone
        // token instead of believing nothing was ever embedded.
        let resolved_token = match self.push_credentials.source() {
            Some(source) => Some(source.mint_for_clone().await.map_err(|err| {
                crate::Error::context_anyhow("Failed to get GitHub App credentials for clone", err)
            })?),
            None => None,
        };
        // The clone call site maps its mint knowledge onto the credential
        // context: a token minted for this clone is FreshApp; a static
        // credential cannot become valid by waiting.
        let clone_credential_context =
            CredentialContext::from_snapshot(resolved_token.as_ref().map(|token| &token.snapshot));

        let auth_url = match &resolved_token {
            Some(token) => Some(
                fabro_github::embed_token_in_url(&origin_url, token.token.expose()).map_err(
                    |err| {
                        crate::Error::context_anyhow(
                            "Failed to build authenticated GitHub clone URL",
                            err,
                        )
                    },
                )?,
            ),
            None => None,
        };
        let clone_url = auth_url
            .as_ref()
            .map_or(origin_url.as_str(), |url| url.as_raw_url().as_str());

        self.emit(SandboxEvent::GitCloneStarted {
            url:    origin_url.clone(),
            branch: branch.clone(),
        });
        let clone_start = Instant::now();

        let prepare_command = format!(
            "mkdir -p {} {}",
            shell_quote(WORKING_DIRECTORY),
            shell_quote(&layout.repos_owner_path),
        );
        match self
            .kubernetes_exec_shell(&prepare_command, 10_000, Some("/"), None, None)
            .await
        {
            Ok(result) if result.is_success() => {}
            Ok(result) => {
                let err = result.into_exec_error("prepare Kubernetes repository checkout");
                return Err(self.report_clone_failure(&origin_url, err));
            }
            Err(err) => {
                return Err(self.report_clone_failure(&origin_url, err));
            }
        }

        let clone_deadline = time::Instant::now() + GIT_CLONE_TIMEOUT;
        if let Some(pin) =
            clone_source::PinnedRevision::from_selectors(tag.as_deref(), commit_sha.as_deref())
        {
            // `decide_clone` already rejects a pinned revision without a
            // branch; re-check here so the checkout can never silently drop the
            // branch name callers read back out of the workspace.
            let Some(branch) = branch.as_deref().filter(|branch| !branch.trim().is_empty()) else {
                let error =
                    crate::Error::message(format!("{} requires a repository branch", pin.label()));
                return Err(self.report_clone_failure(&origin_url, error));
            };

            let init_command =
                clone_source::exact_repository_init_command(clone_url, &layout.primary_repo_path);
            if let Err(error) = self
                .run_exact_local_git_command(
                    &init_command,
                    "initialize Kubernetes pinned repository checkout",
                    clone_deadline,
                    auth_url.as_ref(),
                )
                .await
            {
                return Err(self.report_clone_failure(&origin_url, error));
            }

            let fetch_command = clone_source::pinned_fetch_command(
                &layout.primary_repo_path,
                "origin",
                &pin.fetch_refspec(),
                self.config.clone_depth,
            );
            if let Err(failure) = self
                .retry_git_transfer(
                    &fetch_command,
                    "fetch",
                    "Kubernetes pinned fetch",
                    "git fetch pinned revision",
                    clone_deadline,
                    clone_credential_context,
                    auth_url.as_ref(),
                )
                .await
            {
                return Err(self.report_clone_failure(&origin_url, failure.error));
            }

            let checkout_command = clone_source::exact_checkout_verify_command(
                &layout.primary_repo_path,
                branch,
                clone_source::FETCH_HEAD_COMMIT,
            );
            let head = match self
                .run_exact_local_git_command(
                    &checkout_command,
                    "git checkout pinned revision",
                    clone_deadline,
                    auth_url.as_ref(),
                )
                .await
            {
                Ok(result) => result,
                Err(error) => return Err(self.report_clone_failure(&origin_url, error)),
            };
            if let Err(error) = pin.verify_head(&head.stdout) {
                return Err(self.report_clone_failure(&origin_url, error));
            }
        } else {
            let command = git_clone_command(
                clone_url,
                branch.as_deref(),
                &layout.primary_repo_path,
                self.config.clone_depth,
            );
            if let Err(failure) = self
                .retry_git_transfer(
                    &command,
                    "clone",
                    "Kubernetes git clone",
                    "git clone",
                    clone_deadline,
                    clone_credential_context,
                    auth_url.as_ref(),
                )
                .await
            {
                return Err(self.report_clone_failure(&origin_url, failure.error));
            }
        }

        let symlink_command = clone_source::repo_symlink_command(&layout);
        match self
            .kubernetes_exec_shell(&symlink_command, 10_000, Some("/"), None, None)
            .await
        {
            Ok(result) if result.is_success() => {}
            Ok(result) => {
                let err = result.into_exec_error("create Kubernetes workspace repo symlink");
                return Err(self.report_clone_failure(&origin_url, err));
            }
            Err(err) => {
                return Err(self.report_clone_failure(&origin_url, err));
            }
        }

        let _ = self.repo_cloned.set(true);
        let _ = self.origin_url.set(origin_url.clone());
        self.set_working_directory(layout.execution_directory.clone())?;
        if let Some(token) = resolved_token {
            // The clone URL embedded this token in `origin`; record it so
            // refreshes compare against the clone generation.
            self.push_credentials.record_embedded(token).await;
        }

        if let Some(auth_url) = auth_url.as_ref() {
            let command = format!(
                "git -c maintenance.auto=0 remote set-url origin {}",
                shell_quote(auth_url.as_raw_url().as_str())
            );
            let result = self
                .kubernetes_exec_shell(
                    &command,
                    10_000,
                    Some(&layout.execution_directory),
                    None,
                    None,
                )
                .await?;
            if !result.is_success() {
                let err = result
                    .into_exec_error_with_redactor("git remote set-url origin (post-clone)", |s| {
                        redact_auth_url(s, Some(auth_url))
                    });
                tracing::warn!(
                    error = %err,
                    "Failed to set Kubernetes sandbox push credentials on origin — \
                     subsequent git push from this sandbox will fail"
                );
            }
        }

        let clone_duration = u64::try_from(clone_start.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.emit(SandboxEvent::GitCloneCompleted {
            url:         origin_url,
            duration_ms: clone_duration,
        });
        Ok(())
    }

    async fn delete_pod(&self, pod_name: &str) -> crate::Result<()> {
        match self.pods().delete(pod_name, &DeleteParams::default()).await {
            Ok(_) => Ok(()),
            Err(err) if pod_not_found(&err) => Ok(()),
            Err(err) => Err(crate::Error::context(
                format!("Failed to delete Kubernetes pod '{pod_name}'"),
                err,
            )),
        }
    }

    fn begin_start(&self) -> Instant {
        self.emit(SandboxEvent::StartStarted {
            provider: "kubernetes".into(),
        });
        Instant::now()
    }

    fn start_error(&self, error: crate::Error) -> crate::Result<()> {
        self.emit(SandboxEvent::StartFailed {
            provider: "kubernetes".into(),
            error:    error.to_string(),
            causes:   error.causes(),
        });
        Err(error)
    }

    fn stop_error(&self, error: crate::Error) -> crate::Result<()> {
        self.emit(SandboxEvent::StopFailed {
            provider: "kubernetes".into(),
            error:    error.to_string(),
            causes:   error.causes(),
        });
        Err(error)
    }

    fn delete_error(&self, error: crate::Error) -> crate::Result<()> {
        self.emit(SandboxEvent::DeleteFailed {
            provider: "kubernetes".into(),
            error:    error.to_string(),
            causes:   error.causes(),
        });
        Err(error)
    }

    fn fail_init(&self, init_start: Instant, err: crate::Error) -> crate::Error {
        let duration_ms = u64::try_from(init_start.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.emit(SandboxEvent::InitializeFailed {
            provider: "kubernetes".into(),
            error: err.to_string(),
            causes: err.causes(),
            duration_ms,
        });
        err
    }
}

#[derive(Clone, Copy)]
enum CloneStep {
    Network,
    Local,
}

struct KubernetesCloneFailure {
    error:        crate::Error,
    retry_reason: Option<git_retry::GitRetryReason>,
}

static RUSTLS_PROVIDER: std::sync::Once = std::sync::Once::new();

/// kube builds its TLS config through rustls, which cannot auto-select a
/// crypto provider when both ring and aws-lc-rs are compiled in. Install ring,
/// matching the provider fabro-cli installs at startup.
fn ensure_rustls_provider() {
    use rustls::crypto::ring;

    RUSTLS_PROVIDER.call_once(|| {
        let _ = ring::default_provider().install_default();
    });
}

/// Connect with kube-standard inference: in-cluster ServiceAccount first, then
/// `KUBECONFIG`, then `~/.kube/config`. Returns the client plus the namespace
/// its context resolves to.
pub(crate) async fn connect() -> crate::Result<(Client, String)> {
    let config = Config::infer()
        .await
        .map_err(|err| crate::Error::context("Failed to infer Kubernetes configuration", err))?;
    let namespace = config.default_namespace.clone();
    ensure_rustls_provider();
    let client = Client::try_from(config)
        .map_err(|err| crate::Error::context("Failed to build Kubernetes client", err))?;
    Ok((client, namespace))
}

fn env_entries(env_vars: Option<&HashMap<String, String>>) -> Vec<String> {
    env_vars.map_or_else(Vec::new, |vars| {
        let mut entries: Vec<String> = vars
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect();
        entries.sort();
        entries
    })
}

/// Run a streaming exec end-to-end: write stdin, multiplex both streams into
/// capture buffers and callbacks, strip the sentinel, and decode the exit code.
async fn run_streaming_exec(
    client: Client,
    namespace: String,
    pod_name: String,
    argv: Vec<String>,
    attach_stdin: bool,
    stdin: Option<Vec<u8>>,
    marker: String,
    output_callback: Option<CommandOutputCallback>,
    stream_output_bytes_cap: Option<usize>,
) -> crate::Result<(OutputCaptureBuffer, OutputCaptureBuffer, i32)> {
    let pods: Api<Pod> = Api::namespaced(client, &namespace);
    let params = AttachParams {
        stdin: attach_stdin,
        stdout: true,
        stderr: true,
        tty: false,
        ..Default::default()
    };
    let mut attached = pods
        .exec(&pod_name, argv, &params)
        .await
        .map_err(|err| crate::Error::context("Failed to exec into Kubernetes pod", err))?;

    let mut stdout_reader = attached
        .stdout()
        .ok_or_else(|| crate::Error::message("Kubernetes exec started without stdout"))?;
    let mut stderr_reader = attached
        .stderr()
        .ok_or_else(|| crate::Error::message("Kubernetes exec started without stderr"))?;
    let mut stdin_writer = if attach_stdin { attached.stdin() } else { None };

    let mut stdout = OutputCaptureBuffer::new(stream_output_bytes_cap);
    let mut stderr = OutputCaptureBuffer::new(stream_output_bytes_cap);
    let mut sentinel = ExecSentinelReader::new(marker);
    let mut stdout_chunk = vec![0u8; 8192];
    let mut stderr_chunk = vec![0u8; 8192];

    let write_stdin = async {
        if let (Some(stdin), Some(writer)) = (stdin.as_deref(), stdin_writer.as_mut()) {
            write_process_stdin(writer, stdin).await?;
        }
        crate::Result::Ok(())
    };
    let read_output = async {
        loop {
            tokio::select! {
                biased;
                read = stdout_reader.read(&mut stdout_chunk) => {
                    match read {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            let released = sentinel.push(&stdout_chunk[..n]);
                            stdout.push(&released);
                            if let Some(output_callback) = output_callback.as_ref() {
                                output_callback(CommandOutputStream::Stdout, released).await?;
                            }
                        }
                    }
                }
                read = stderr_reader.read(&mut stderr_chunk) => {
                    match read {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            stderr.push(&stderr_chunk[..n]);
                            if let Some(output_callback) = output_callback.as_ref() {
                                output_callback(CommandOutputStream::Stderr, stderr_chunk[..n].to_vec())
                                    .await?;
                            }
                        }
                    }
                }
            }
        }
        crate::Result::Ok(())
    };
    tokio::try_join!(write_stdin, read_output)?;

    // Whichever stream ended first inside the select, drain the other to EOF
    // so no output is dropped before the sentinel can be decoded.
    loop {
        match stdout_reader.read(&mut stdout_chunk).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                let released = sentinel.push(&stdout_chunk[..n]);
                stdout.push(&released);
                if let Some(output_callback) = output_callback.as_ref() {
                    output_callback(CommandOutputStream::Stdout, released).await?;
                }
            }
        }
    }
    while let Ok(n) = stderr_reader.read(&mut stderr_chunk).await {
        if n == 0 {
            break;
        }
        stderr.push(&stderr_chunk[..n]);
        if let Some(output_callback) = output_callback.as_ref() {
            output_callback(CommandOutputStream::Stderr, stderr_chunk[..n].to_vec()).await?;
        }
    }
    // The sentinel is only known once the stdout stream has ended.
    let (tail, exit_code) = sentinel.finish();
    stdout.push(&tail);
    if let Some(output_callback) = output_callback.as_ref() {
        output_callback(CommandOutputStream::Stdout, tail).await?;
    }
    let exit_code = exit_code.unwrap_or(-1);
    let _ = attached.join().await;
    Ok((stdout, stderr, exit_code))
}

fn git_clone_command(
    clone_url: &str,
    branch: Option<&str>,
    checkout_path: &str,
    depth: Option<usize>,
) -> String {
    let mut command = format!("{} clone", sandbox::GIT);
    if let Some(branch) = branch {
        command.push_str(" --branch ");
        command.push_str(&shell_quote(branch));
        command.push_str(" --single-branch");
    }
    command.push_str(&clone_source::depth_argument(depth));
    command.push_str(" --no-tags");
    command.push_str(" -- ");
    command.push_str(&shell_quote(clone_url));
    command.push(' ');
    command.push_str(&shell_quote(checkout_path));
    command
}

fn classify_kubernetes_clone_result(
    result: &ExecResult,
    cred: CredentialContext,
) -> Option<git_retry::GitRetryReason> {
    git_retry::classify_output(&result.stderr, &result.stdout, cred).retry_reason()
}

fn pod_labels(pod: &Pod) -> HashMap<String, String> {
    pod.metadata
        .labels
        .as_ref()
        .map(|labels| {
            labels
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect()
        })
        .unwrap_or_default()
}

fn verify_managed_labels(
    pod_name: &str,
    labels: &HashMap<String, String>,
    run_id: Option<&RunId>,
) -> crate::Result<()> {
    if labels.get(MANAGED_LABEL).map(String::as_str) != Some(MANAGED_LABEL_VALUE) {
        return Err(crate::Error::message(format!(
            "Refusing to operate on Kubernetes pod '{pod_name}' because it is missing label {MANAGED_LABEL}=true"
        )));
    }
    if let Some(run_id) = run_id {
        let actual = labels.get(RUN_ID_LABEL).map(String::as_str);
        let expected = run_id.to_string();
        if actual != Some(expected.as_str()) {
            return Err(crate::Error::message(format!(
                "Refusing to operate on Kubernetes pod '{pod_name}' because label {RUN_ID_LABEL}={actual:?} does not match run {run_id}"
            )));
        }
    }
    Ok(())
}

fn pod_not_found(err: &kube::Error) -> bool {
    matches!(
        err,
        kube::Error::Api(status) if status.code == 404
    )
}

fn pod_already_exists(err: &kube::Error) -> bool {
    matches!(
        err,
        kube::Error::Api(status) if status.code == 409
    )
}

/// RFC 1123 pod names must be lowercase; run IDs are uppercase ULIDs.
pub(crate) fn pod_name_for_run(run_id: &RunId) -> String {
    format!("fabro-{}", run_id.to_string().to_lowercase())
}

pub(crate) fn preflight_pod_name() -> String {
    format!("fabro-{}", uuid::Uuid::new_v4().simple())
}

fn labels_for_pod(
    pod_name: &str,
    run_id: Option<&RunId>,
    user_labels: &HashMap<String, String>,
) -> std::collections::BTreeMap<String, String> {
    let mut labels: std::collections::BTreeMap<String, String> = user_labels
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    labels.insert(MANAGED_LABEL.to_string(), MANAGED_LABEL_VALUE.to_string());
    if let Some(run_id) = run_id {
        labels.insert(RUN_ID_LABEL.to_string(), run_id.to_string());
    }
    labels.insert(SANDBOX_LABEL.to_string(), pod_name.to_string());
    labels
}

type QuantityMap = std::collections::BTreeMap<String, Quantity>;

fn resource_quantities(config: &KubernetesSandboxOptions) -> Option<QuantityMap> {
    let mut quantities: QuantityMap = std::collections::BTreeMap::new();
    if let Some(cpu) = config.cpu {
        quantities.insert("cpu".to_string(), Quantity(cpu.to_string()));
    }
    if let Some(memory) = config.memory_limit {
        quantities.insert("memory".to_string(), Quantity(memory.to_string()));
    }
    if let Some(disk) = config.ephemeral_storage_limit {
        quantities.insert("ephemeral-storage".to_string(), Quantity(disk.to_string()));
    }
    (!quantities.is_empty()).then_some(quantities)
}

fn pod_manifest(pod_name: &str, config: &KubernetesSandboxOptions, run_id: Option<&RunId>) -> Pod {
    // Requests equal limits so a sandbox gets exactly what it asked for and
    // cannot be evicted for exceeding a soft request.
    let quantities = resource_quantities(config);
    let env = config.env_vars.iter().map(|entry| {
        let (key, value) = entry.split_once('=').unwrap_or((entry, ""));
        EnvVar {
            name: key.to_string(),
            value: Some(value.to_string()),
            ..Default::default()
        }
    });
    Pod {
        metadata: ObjectMeta {
            name: Some(pod_name.to_string()),
            labels: Some(labels_for_pod(pod_name, run_id, &config.labels)),
            ..Default::default()
        },
        spec:     Some(PodSpec {
            restart_policy: Some("Never".to_string()),
            containers: vec![Container {
                name: POD_CONTAINER_NAME.to_string(),
                image: Some(config.image.clone()),
                command: Some(vec!["sleep".to_string(), "infinity".to_string()]),
                working_dir: Some(WORKING_DIRECTORY.to_string()),
                env: (!config.env_vars.is_empty()).then(|| env.collect()),
                resources: quantities.map(|quantities| ResourceRequirements {
                    limits: Some(quantities.clone()),
                    requests: Some(quantities),
                    ..Default::default()
                }),
                ..Default::default()
            }],
            ..Default::default()
        }),
        status:   None,
    }
}

fn network_policy_manifest(
    pod_name: &str,
    pod_uid: Option<&str>,
    mode: &KubernetesNetworkMode,
) -> NetworkPolicy {
    let egress = match mode {
        KubernetesNetworkMode::AllowAll => None,
        KubernetesNetworkMode::Block => Some(Vec::new()),
        KubernetesNetworkMode::CidrAllowList(cidrs) => {
            let mut rules: Vec<NetworkPolicyEgressRule> = cidrs
                .iter()
                .filter(|cidr| !cidr.trim().is_empty())
                .map(|cidr| NetworkPolicyEgressRule {
                    ports: None,
                    to:    Some(vec![NetworkPolicyPeer {
                        ip_block: Some(IPBlock {
                            cidr: cidr.trim().to_string(),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }]),
                })
                .collect();
            // Cluster DNS must stay reachable for allow-listed host names;
            // UDP/TCP 53 to anywhere is the narrowest portable expression of
            // that without knowing the DNS service CIDR.
            rules.push(NetworkPolicyEgressRule {
                ports: Some(vec![
                    NetworkPolicyPort {
                        port:     Some(IntOrString::Int(53)),
                        protocol: Some("UDP".to_string()),
                        end_port: None,
                    },
                    NetworkPolicyPort {
                        port:     Some(IntOrString::Int(53)),
                        protocol: Some("TCP".to_string()),
                        end_port: None,
                    },
                ]),
                to:    Some(vec![NetworkPolicyPeer {
                    ip_block: Some(IPBlock {
                        cidr: "0.0.0.0/0".to_string(),
                        ..Default::default()
                    }),
                    ..Default::default()
                }]),
            });
            Some(rules)
        }
    };

    NetworkPolicy {
        metadata: ObjectMeta {
            name: Some(pod_name.to_string()),
            owner_references: pod_uid.map(|uid| {
                vec![OwnerReference {
                    api_version:          "v1".to_string(),
                    kind:                 "Pod".to_string(),
                    name:                 pod_name.to_string(),
                    uid:                  uid.to_string(),
                    controller:           Some(true),
                    block_owner_deletion: Some(true),
                }]
            }),
            ..Default::default()
        },
        spec:     Some(NetworkPolicySpec {
            pod_selector: Some(LabelSelector {
                match_labels: Some(std::collections::BTreeMap::from([(
                    SANDBOX_LABEL.to_string(),
                    pod_name.to_string(),
                )])),
                ..Default::default()
            }),
            policy_types: Some(vec!["Egress".to_string()]),
            egress,
            ingress: None,
        }),
    }
}

fn pod_is_ready(pod: &Pod) -> bool {
    pod.status
        .as_ref()
        .and_then(|status| status.conditions.as_ref())
        .is_some_and(|conditions| {
            conditions
                .iter()
                .any(|condition| condition.type_ == "Ready" && condition.status == "True")
        })
}

/// A reason the pod will never become Ready on its own.
fn terminal_failure_reason(pod: &Pod) -> Option<String> {
    let status = pod.status.as_ref()?;
    if status.phase.as_deref() == Some("Failed") {
        return Some("phase Failed".to_string());
    }
    status
        .container_statuses
        .as_ref()?
        .iter()
        .find_map(|container| {
            let waiting = container.state.as_ref()?.waiting.as_ref()?;
            let reason = waiting.reason.as_deref()?;
            matches!(
                reason,
                "ErrImagePull"
                    | "ImagePullBackOff"
                    | "CrashLoopBackOff"
                    | "CreateContainerError"
                    | "CreateContainerConfigError"
                    | "RunContainerError"
                    | "InvalidImageName"
            )
            .then(|| {
                waiting.message.as_ref().map_or_else(
                    || reason.to_string(),
                    |message| format!("{reason}: {message}"),
                )
            })
        })
}

fn image_remediation(image: &str, reason: &str) -> String {
    if reason.contains("ImagePull")
        || reason.contains("ImagePullBackOff")
        || reason.contains("InvalidImageName")
    {
        format!(
            "Verify that image '{image}' exists and is pullable from the cluster (registry \
             credentials come from the namespace's ServiceAccount imagePullSecrets)."
        )
    } else {
        format!(
            "Inspect the pod with `kubectl describe pod` and verify image '{image}' can run in \
             this cluster."
        )
    }
}

fn build_single_file_tar(file_name: &str, bytes: &[u8], mode: u32) -> crate::Result<Vec<u8>> {
    let mut tar_builder = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header
        .set_path(file_name)
        .map_err(|e| crate::Error::context("Failed to set tar path", e))?;
    header.set_size(
        u64::try_from(bytes.len())
            .map_err(|_| crate::Error::message("file is too large for tar header"))?,
    );
    header.set_mode(mode);
    header.set_cksum();
    tar_builder
        .append(&header, bytes)
        .map_err(|e| crate::Error::context("Failed to build tar archive", e))?;
    tar_builder
        .into_inner()
        .map_err(|e| crate::Error::context("Failed to finalize tar archive", e))
}

fn extract_single_file_tar(archive_bytes: &[u8], remote_path: &str) -> crate::Result<Vec<u8>> {
    #[expect(
        clippy::disallowed_types,
        reason = "tar entries are synchronous in-memory readers; bytes are collected before any await"
    )]
    use std::io::Read as _;

    let mut archive = tar::Archive::new(Cursor::new(archive_bytes));
    let entries = archive.entries().map_err(|e| {
        crate::Error::context(format!("Failed to read pod archive for {remote_path}"), e)
    })?;
    for entry in entries {
        let mut entry = entry.map_err(|e| {
            crate::Error::context(
                format!("Failed to read pod archive entry for {remote_path}"),
                e,
            )
        })?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).map_err(|e| {
            crate::Error::context(
                format!("Failed to read pod archive file for {remote_path}"),
                e,
            )
        })?;
        return Ok(bytes);
    }

    Err(crate::Error::message(format!(
        "Pod archive for {remote_path} did not contain a file"
    )))
}

#[async_trait]
impl Sandbox for KubernetesSandbox {
    async fn download_file_to_local(
        &self,
        remote_path: &str,
        local_path: &std::path::Path,
    ) -> crate::Result<()> {
        let bytes = self.download_file_bytes(remote_path).await?;
        fs::create_dir_all(parent_of(local_path))
            .await
            .map_err(|e| crate::Error::context("Failed to create parent dirs", e))?;
        fs::write(local_path, bytes).await.map_err(|e| {
            crate::Error::context(format!("Failed to write {}", local_path.display()), e)
        })
    }

    async fn upload_file_from_local(
        &self,
        local_path: &std::path::Path,
        remote_path: &str,
    ) -> crate::Result<()> {
        let bytes = fs::read(local_path).await.map_err(|e| {
            crate::Error::context(format!("Failed to read {}", local_path.display()), e)
        })?;
        self.upload_bytes_to_pod(remote_path, &bytes).await
    }

    async fn initialize(&self) -> crate::Result<()> {
        self.emit(SandboxEvent::Initializing {
            provider: "kubernetes".into(),
        });
        let init_start = Instant::now();

        let pod_name = self
            .run_id
            .as_ref()
            .map_or_else(preflight_pod_name, pod_name_for_run);
        if let Some(run_id) = self.run_id.as_ref() {
            let existing = self
                .get_pod(&pod_name)
                .await
                .map_err(|e| self.fail_init(init_start, e))?;
            if existing.is_some() {
                let error = crate::Error::message(format!(
                    "Kubernetes pod '{pod_name}' already exists for run {run_id}. Remove the \
                     stale pod manually before retrying."
                ));
                return Err(self.fail_init(init_start, error));
            }
        }

        self.emit(SandboxEvent::SnapshotPulling {
            name: self.config.image.clone(),
        });
        let pull_start = Instant::now();
        match self.create_pod(&pod_name).await {
            Ok(_) => {
                let pull_duration =
                    u64::try_from(pull_start.elapsed().as_millis()).unwrap_or(u64::MAX);
                self.emit(SandboxEvent::SnapshotReady {
                    name:        self.config.image.clone(),
                    duration_ms: pull_duration,
                });
            }
            Err(e) => {
                self.emit(SandboxEvent::SnapshotFailed {
                    name:   self.config.image.clone(),
                    error:  e.to_string(),
                    causes: e.causes(),
                });
                return Err(self.fail_init(init_start, e));
            }
        }

        self.pod_name
            .set(pod_name.clone())
            .map_err(|_| crate::Error::message("Pod already initialized"))?;

        if let Err(e) = self.probe_bash(Some(WORKING_DIRECTORY)).await {
            return Err(self.fail_init(init_start, e));
        }

        let uname = self
            .kubernetes_exec_shell("uname -r", 10_000, Some("/"), None, None)
            .await?;
        let _ = self.cached_platform.set("linux".to_string());
        let _ = self
            .cached_os_version
            .set(format!("linux {}", uname.stdout.trim()));

        if let Err(e) = self.create_runtime_directory().await {
            return Err(self.fail_init(init_start, e));
        }

        let clone_decision = clone_source::decide_clone(
            self.config.skip_clone,
            self.clone_origin_url.as_deref(),
            self.clone_branch.as_deref(),
            self.clone_tag.as_deref(),
            self.clone_commit_sha.as_deref(),
        )
        .map_err(|e| self.fail_init(init_start, e))?;

        match clone_decision {
            CloneDecision::EmptyWorkspace { reason } => {
                if matches!(reason, EmptyWorkspaceReason::MissingOrigin) {
                    tracing::warn!(
                        provider = "kubernetes",
                        reason = reason.message(),
                        "Clone source missing for clone-based sandbox"
                    );
                }
                if let Err(e) = self.create_workspace().await {
                    return Err(self.fail_init(init_start, e));
                }
                let _ = self.repo_cloned.set(false);
            }
            CloneDecision::GitHub {
                origin_url,
                branch,
                tag,
                commit_sha,
            } => {
                if let Err(e) = self
                    .clone_github_repo(origin_url, branch, tag, commit_sha)
                    .await
                {
                    return Err(self.fail_init(init_start, e));
                }
            }
        }

        let init_duration = u64::try_from(init_start.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.emit(SandboxEvent::Ready {
            provider:    "kubernetes".into(),
            duration_ms: init_duration,
            name:        None,
            cpu:         None,
            memory:      None,
            url:         None,
        });

        Ok(())
    }

    async fn start(&self) -> crate::Result<()> {
        let started = self.begin_start();
        let pod_name = self.pod_name()?.to_string();
        if let Err(error) = self.verify_pod_running(&pod_name).await {
            return self.start_error(error);
        }
        if let Err(error) = self.probe_bash(None).await {
            return self.start_error(crate::Error::context(
                format!("Kubernetes pod '{pod_name}' health check"),
                error,
            ));
        }

        self.emit(SandboxEvent::StartCompleted {
            provider:    "kubernetes".into(),
            duration_ms: elapsed_ms(started),
        });
        Ok(())
    }

    async fn activate(&self) -> crate::Result<()> {
        let pod_name = self.pod_name()?.to_string();
        self.verify_pod_running(&pod_name).await?;
        self.probe_bash(None).await
    }

    async fn stop(&self) -> crate::Result<()> {
        self.emit(SandboxEvent::StopStarted {
            provider: "kubernetes".into(),
        });
        let start = Instant::now();

        // A Kubernetes pod cannot be paused and resumed like a Docker
        // container: stopping a sandbox deletes it, and resume recreates from
        // the checkpoint instead.
        let Some(pod_name) = self.pod_name.get().cloned() else {
            let duration_ms = elapsed_ms(start);
            self.emit(SandboxEvent::StopCompleted {
                provider: "kubernetes".into(),
                duration_ms,
            });
            return Ok(());
        };

        if let Err(e) = self.delete_pod(&pod_name).await {
            return self.stop_error(e);
        }

        let duration_ms = elapsed_ms(start);
        self.emit(SandboxEvent::StopCompleted {
            provider: "kubernetes".into(),
            duration_ms,
        });

        Ok(())
    }

    async fn delete(&self) -> crate::Result<()> {
        self.emit(SandboxEvent::DeleteStarted {
            provider: "kubernetes".into(),
        });
        let start = Instant::now();

        let Some(pod_name) = self.pod_name.get().cloned() else {
            let duration_ms = elapsed_ms(start);
            self.emit(SandboxEvent::DeleteCompleted {
                provider: "kubernetes".into(),
                duration_ms,
            });
            return Ok(());
        };

        if let Err(e) = self.delete_pod(&pod_name).await {
            return self.delete_error(e);
        }

        let duration_ms = elapsed_ms(start);
        self.emit(SandboxEvent::DeleteCompleted {
            provider: "kubernetes".into(),
            duration_ms,
        });

        Ok(())
    }

    async fn cleanup(&self) -> crate::Result<()> {
        self.delete().await
    }

    async fn exec_command(
        &self,
        command: &str,
        timeout_ms: u64,
        working_dir: Option<&str>,
        env_vars: Option<&HashMap<String, String>>,
        cancel_token: Option<CancellationToken>,
    ) -> crate::Result<ExecResult> {
        let dir = working_dir.map(|path| self.resolve_pod_path(path));
        self.kubernetes_exec_shell(command, timeout_ms, dir.as_deref(), env_vars, cancel_token)
            .await
    }

    async fn exec_command_streaming(
        &self,
        request: ExecStreamingRequest<'_>,
    ) -> crate::Result<ExecStreamingResult> {
        let dir = request.working_dir.map(|path| self.resolve_pod_path(path));
        self.kubernetes_exec_streaming(ExecStreamingRequest {
            working_dir: dir.as_deref(),
            ..request
        })
        .await
    }

    async fn spawn_stdio_process(
        &self,
        command: &str,
        working_dir: Option<&str>,
        env_vars: Option<&HashMap<String, String>>,
        cancel_token: Option<CancellationToken>,
    ) -> crate::Result<StdioProcess> {
        let effective_dir = working_dir.map_or_else(
            || self.working_directory().to_string(),
            |path| self.resolve_pod_path(path),
        );
        let env = env_entries(env_vars);
        let (stop_file, pid_file, _marker) = exec_control_paths();
        // MCP stdio is length-prefixed JSON: no sentinel may ever appear in
        // the stream, and termination is detected by the attach streams
        // closing rather than an exit code.
        let script = controlled_shell_command_no_marker(
            command,
            &stop_file,
            &pid_file,
            &effective_dir,
            &env,
        );

        let pod_name = self.pod_name()?.to_string();
        let namespace = self.namespace().to_string();
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &namespace);
        let mut attached = pods
            .exec(&pod_name, kubernetes_exec_argv(script), &AttachParams {
                stdin: true,
                stdout: true,
                stderr: true,
                tty: false,
                ..Default::default()
            })
            .await
            .map_err(|err| crate::Error::context("Failed to exec into Kubernetes pod", err))?;

        let mut stdout_reader = attached
            .stdout()
            .ok_or_else(|| crate::Error::message("Kubernetes stdio exec started without stdout"))?;
        let mut stderr_reader = attached
            .stderr()
            .ok_or_else(|| crate::Error::message("Kubernetes stdio exec started without stderr"))?;
        let stdin_writer = attached
            .stdin()
            .ok_or_else(|| crate::Error::message("Kubernetes stdio exec started without stdin"))?;

        let stderr_collector = StderrCollector::new(DEFAULT_EXEC_OUTPUT_TAIL_BYTES);
        let stderr_for_output = stderr_collector.clone();
        let (mut stdout_writer, stdout_reader_duplex) = duplex(64 * 1024);
        let state = Arc::new(KubernetesStdioProcessState::default());
        let state_for_output = Arc::clone(&state);
        tokio::spawn(async move {
            let mut chunk = vec![0u8; 8192];
            loop {
                match stdout_reader.read(&mut chunk).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if stdout_writer.write_all(&chunk[..n]).await.is_err() {
                            tracing::warn!("Failed to forward Kubernetes stdio stdout");
                            break;
                        }
                    }
                }
            }
            while let Ok(n) = stderr_reader.read(&mut chunk).await {
                if n == 0 {
                    break;
                }
                stderr_for_output.push(&chunk[..n]).await;
            }
            // The attach streams closing is the only termination signal the
            // exec API provides; the exit code is unknowable here.
            state_for_output
                .cache_termination(StdioProcessTermination::exited(None))
                .await;
        });

        let handle = StdioProcessHandle::new(KubernetesStdioProcessControl {
            namespace,
            pod_name,
            stop_file,
            client: self.client.clone(),
            state,
        });

        if let Some(token) = cancel_token {
            let handle_for_cancel = handle.clone();
            tokio::spawn(async move {
                token.cancelled().await;
                if let Err(err) = handle_for_cancel.terminate().await {
                    tracing::warn!(error = %err, "Failed to terminate cancelled Kubernetes stdio exec");
                }
            });
        }

        Ok(StdioProcess {
            stdin: Box::pin(stdin_writer),
            stdout: Box::pin(stdout_reader_duplex),
            stderr: stderr_collector,
            handle,
        })
    }

    async fn read_file_bytes(&self, path: &str) -> crate::Result<Vec<u8>> {
        self.download_file_bytes(path).await
    }

    async fn read_file(
        &self,
        path: &str,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> crate::Result<String> {
        let pod_path = self.resolve_pod_path(path);
        let result = self
            .kubernetes_exec_shell(
                &format!("cat {}", shell_quote(&pod_path)),
                30_000,
                Some("/"),
                None,
                None,
            )
            .await?;
        if !result.is_success() {
            return Err(crate::Error::message(format!(
                "Failed to read {pod_path}: {}",
                result.stderr
            )));
        }

        Ok(format_lines_numbered(&result.stdout, offset, limit))
    }

    async fn write_file(&self, path: &str, content: &str) -> crate::Result<()> {
        self.upload_bytes_to_pod(path, content.as_bytes()).await
    }

    async fn delete_file(&self, path: &str) -> crate::Result<()> {
        let pod_path = self.resolve_pod_path(path);
        let result = self
            .kubernetes_exec_shell(
                &format!("rm -f {}", shell_quote(&pod_path)),
                10_000,
                Some("/"),
                None,
                None,
            )
            .await?;
        if !result.is_success() {
            return Err(crate::Error::message(format!(
                "Failed to delete {pod_path}: {}",
                result.stderr
            )));
        }
        Ok(())
    }

    async fn file_exists(&self, path: &str) -> crate::Result<bool> {
        let pod_path = self.resolve_pod_path(path);
        let result = self
            .kubernetes_exec_shell(
                &format!("test -e {}", shell_quote(&pod_path)),
                10_000,
                Some("/"),
                None,
                None,
            )
            .await?;
        Ok(result.is_success())
    }

    async fn list_directory(
        &self,
        path: &str,
        depth: Option<usize>,
    ) -> crate::Result<Vec<DirEntry>> {
        let pod_path = self.resolve_pod_path(path);
        let max_depth = depth.unwrap_or(1);
        let result = self
            .kubernetes_exec_shell(
                &format!(
                    "find {} -mindepth 1 -maxdepth {} -printf '%y\\t%s\\t%P\\n'",
                    shell_quote(&pod_path),
                    max_depth
                ),
                30_000,
                Some("/"),
                None,
                None,
            )
            .await?;
        if !result.is_success() {
            return Err(crate::Error::message(format!(
                "Failed to list directory {pod_path}: {}",
                result.stderr
            )));
        }

        let mut entries: Vec<DirEntry> = result
            .stdout
            .lines()
            .filter(|line| !line.is_empty())
            .filter_map(|line| {
                let parts: Vec<&str> = line.splitn(3, '\t').collect();
                if parts.len() < 3 {
                    return None;
                }
                let file_type = parts[0];
                let size: Option<u64> = parts[1].parse().ok();
                let name = parts[2].to_string();
                let is_dir = file_type == "d";
                Some(DirEntry {
                    name,
                    is_dir,
                    size: if is_dir { None } else { size },
                })
            })
            .collect();

        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }

    async fn grep(
        &self,
        pattern: &str,
        path: &str,
        options: &GrepOptions,
    ) -> crate::Result<Vec<String>> {
        let pod_path = self.resolve_pod_path(path);
        let use_rg = *self
            .rg_available
            .get_or_init(|| async {
                self.kubernetes_exec_shell("which rg", 10_000, Some("/"), None, None)
                    .await
                    .is_ok_and(|result| result.is_success())
            })
            .await;

        let command = if use_rg {
            let mut command = "rg -n".to_string();
            if options.case_insensitive {
                command.push_str(" -i");
            }
            if let Some(ref glob_filter) = options.glob_filter {
                command.push_str(" --glob ");
                command.push_str(&shell_quote(glob_filter));
            }
            if let Some(max) = options.max_results {
                let _ = write!(&mut command, " -m {max}");
            }
            command.push_str(" -- ");
            command.push_str(&shell_quote(pattern));
            command.push(' ');
            command.push_str(&shell_quote(&pod_path));
            command
        } else {
            let mut command = "grep -rn".to_string();
            if options.case_insensitive {
                command.push_str(" -i");
            }
            if let Some(ref glob_filter) = options.glob_filter {
                command.push_str(" --include ");
                command.push_str(&shell_quote(glob_filter));
            }
            if let Some(max) = options.max_results {
                let _ = write!(&mut command, " -m {max}");
            }
            command.push_str(" -- ");
            command.push_str(&shell_quote(pattern));
            command.push(' ');
            command.push_str(&shell_quote(&pod_path));
            command
        };

        let result = self
            .kubernetes_exec_shell(&command, 30_000, Some("/"), None, None)
            .await?;
        if result.exit_code == Some(1) {
            return Ok(Vec::new());
        }
        if !result.is_success() {
            return Err(crate::Error::message(format!(
                "grep failed (exit {}): {}",
                result.display_exit_code(),
                result.stderr
            )));
        }

        Ok(result
            .stdout
            .lines()
            .map(String::from)
            .filter(|line| !line.is_empty())
            .collect())
    }

    async fn walk_files(
        &self,
        base: &str,
        relative_start: &str,
        options: &WalkOptions,
    ) -> crate::Result<Vec<SandboxFile>> {
        if options.excludes_relative_path(relative_start) {
            return Ok(Vec::new());
        }

        let base = self.resolve_pod_path(base);
        let command = sandbox::build_remote_walk_command(&base, relative_start, options);
        let result = self
            .kubernetes_exec_shell(&command, REMOTE_WALK_TIMEOUT_MS, Some("/"), None, None)
            .await?;
        if !result.is_success() {
            return Err(crate::Error::exec("recursive file traversal", result));
        }

        sandbox::parse_remote_walk_output(&base, relative_start, &result.stdout)
    }

    fn working_directory(&self) -> &str {
        self.working_directory
            .get()
            .map_or(WORKING_DIRECTORY, String::as_str)
    }

    fn runtime_directory(&self) -> Option<&str> {
        Some(RUNTIME_DIRECTORY)
    }

    async fn ssh_access_command(&self) -> crate::Result<Option<String>> {
        Ok(Some(kubernetes_access_command(
            self.pod_name()?,
            self.namespace(),
            self.working_directory(),
        )))
    }

    fn platform(&self) -> &str {
        self.cached_platform.get().map_or("linux", String::as_str)
    }

    fn os_version(&self) -> String {
        self.cached_os_version
            .get()
            .cloned()
            .unwrap_or_else(|| "linux".to_string())
    }

    fn sandbox_info(&self) -> String {
        self.pod_name.get().cloned().unwrap_or_default()
    }

    async fn setup_git(
        &self,
        intent: &crate::GitSetupIntent,
    ) -> crate::Result<Option<crate::GitRunInfo>> {
        if !self.repo_cloned() {
            return Ok(None);
        }
        crate::setup_git_via_exec(self, intent).await.map(Some)
    }

    fn resume_setup_commands(&self, run_branch: &str) -> Vec<String> {
        if !self.repo_cloned() {
            return Vec::new();
        }
        vec![format!(
            "git fetch origin {} && git checkout {}",
            shell_quote(run_branch),
            shell_quote(run_branch)
        )]
    }

    async fn git_push_ref(
        &self,
        refspec: &str,
        plan: &crate::RetryPlan,
    ) -> Result<crate::PushReport, crate::PushError> {
        if !self.repo_cloned() {
            return Ok(crate::PushReport::default());
        }
        let credentials = self
            .origin_url
            .get()
            .map(|origin_url| (&self.push_credentials, origin_url.as_str()));
        sandbox::git_push_via_exec(self, credentials, refspec, plan).await
    }

    fn origin_url(&self) -> Option<&str> {
        if !self.repo_cloned() {
            return None;
        }
        self.origin_url.get().map(String::as_str)
    }

    #[tracing::instrument(name = "git_op", skip_all, fields(op = "refresh-credentials"))]
    async fn refresh_push_credentials(&self) -> crate::Result<RefreshOutcome> {
        if !self.repo_cloned() {
            return Ok(RefreshOutcome::none());
        }
        let Some(origin_url) = self.origin_url.get() else {
            return Ok(RefreshOutcome::none());
        };
        self.push_credentials
            .refresh(origin_url, |auth_url| {
                push_credentials::set_auth_url_via_exec(self, auth_url)
            })
            .await
    }

    fn push_token_source(&self) -> Option<Arc<InstallationTokenSource>> {
        self.push_credentials.source().cloned()
    }
}

impl KubernetesSandbox {
    async fn verify_pod_running(&self, pod_name: &str) -> crate::Result<()> {
        let Some(pod) = self.get_pod(pod_name).await? else {
            return Err(crate::Error::message(format!(
                "Kubernetes pod '{pod_name}' is gone"
            )));
        };
        let labels = pod_labels(&pod);
        verify_managed_labels(pod_name, &labels, self.run_id.as_ref())?;
        if pod_is_ready(&pod) {
            return Ok(());
        }
        if let Some(reason) = terminal_failure_reason(&pod) {
            return Err(crate::Error::message(format!(
                "Kubernetes pod '{pod_name}' is not Running ({reason})"
            )));
        }
        Err(crate::Error::message(format!(
            "Kubernetes pod '{pod_name}' is not Running yet (phase {:?})",
            pod.status.as_ref().and_then(|status| status.phase.clone())
        )))
    }
}

fn parent_of(path: &std::path::Path) -> &std::path::Path {
    path.parent().unwrap_or_else(|| std::path::Path::new("/"))
}

pub fn kubernetes_access_command(
    pod_name: &str,
    namespace: &str,
    working_directory: &str,
) -> String {
    let shell = format!("cd {} && exec sh -l", shell_quote(working_directory));
    format!(
        "kubectl exec -it {pod_name} -n {namespace} -- sh -lc {}",
        shell_quote(&shell)
    )
}

/// Controlled wrapper without the exit-code sentinel, for stdio sessions whose
/// streams carry a binary/JSON protocol.
fn controlled_shell_command_no_marker(
    command: &str,
    stop_file: &str,
    pid_file: &str,
    cwd: &str,
    env_vars: &[String],
) -> String {
    format!(
        "\
cd {cwd} || exit 1; \
stop_file={stop_file}; \
pid_file={pid_file}; \
user_command={command}; \
rm -f \"$pid_file\"; \
if [ -e \"$stop_file\" ]; then \
  rm -f \"$stop_file\" \"$pid_file\"; \
  exit 143; \
fi; \
( \
  while [ ! -e \"$stop_file\" ]; do sleep {stop_poll_sleep}; done; \
  while [ ! -s \"$pid_file\" ]; do sleep {stop_poll_sleep}; done; \
  child=$(cat \"$pid_file\"); \
  kill -TERM \"-$child\" 2>/dev/null || kill -TERM \"$child\" 2>/dev/null || true; \
  sleep {term_grace}; \
  kill -KILL \"-$child\" 2>/dev/null || kill -KILL \"$child\" 2>/dev/null || true; \
) & watcher=$!; \
{env_prefix} setsid {bash} -c \"$user_command\" & \
child=$!; \
echo \"$child\" > \"$pid_file\"; \
wait \"$child\"; \
kill \"$watcher\" 2>/dev/null || true; \
wait \"$watcher\" 2>/dev/null || true; \
rm -f \"$stop_file\" \"$pid_file\"\
",
        cwd = shell_quote(cwd),
        stop_file = shell_quote(stop_file),
        pid_file = shell_quote(pid_file),
        command = shell_quote(command),
        stop_poll_sleep = EXEC_STOP_POLL_SLEEP_SECONDS,
        term_grace = EXEC_TERM_GRACE_SECONDS,
        env_prefix = kubernetes_env_prefix(env_vars),
        bash = REMOTE_BASH,
    )
}

struct KubernetesStdioProcessControl {
    client:    Client,
    namespace: String,
    pod_name:  String,
    stop_file: String,
    state:     Arc<KubernetesStdioProcessState>,
}

#[derive(Default)]
struct KubernetesStdioProcessState {
    stop_requested:     std::sync::atomic::AtomicBool,
    termination:        TokioMutex<Option<StdioProcessTermination>>,
    termination_notify: Notify,
}

impl KubernetesStdioProcessState {
    async fn cached_termination(&self) -> Option<StdioProcessTermination> {
        *self.termination.lock().await
    }

    async fn request_stop_once(&self) -> bool {
        self.cached_termination().await.is_none()
            && !self.stop_requested.swap(true, Ordering::AcqRel)
    }

    async fn cache_termination(&self, termination: StdioProcessTermination) {
        let mut cached = self.termination.lock().await;
        if cached.is_none() {
            *cached = Some(termination);
            self.termination_notify.notify_waiters();
        }
    }

    async fn wait_for_cached_termination(&self) -> StdioProcessTermination {
        loop {
            if let Some(termination) = self.cached_termination().await {
                return termination;
            }
            self.termination_notify.notified().await;
        }
    }
}

#[async_trait]
impl StdioProcessControl for KubernetesStdioProcessControl {
    async fn terminate(&self) -> crate::Result<()> {
        if !self.state.request_stop_once().await {
            return Ok(());
        }
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &self.namespace);
        let attached = pods
            .exec(
                &self.pod_name,
                kubernetes_exec_argv(exec_stop_request_script(&self.stop_file)),
                &AttachParams {
                    stdout: true,
                    stderr: true,
                    ..Default::default()
                },
            )
            .await
            .map_err(|err| crate::Error::context("Failed to request Kubernetes exec stop", err))?;
        let _ = attached.join().await;
        Ok(())
    }

    async fn wait(&self) -> crate::Result<StdioProcessTermination> {
        Ok(self.state.wait_for_cached_termination().await)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sentinel_reader_releases_line_after_a_newer_line_proves_it_is_not_the_marker() {
        let mut reader = ExecSentinelReader::new("__FABRO_RC_t1__".to_string());
        assert!(reader.push(b"hello\n").is_empty());
        assert_eq!(reader.push(b"world\n"), b"hello\n");
        let (output, code) = reader.finish();
        assert_eq!(output, b"world\n");
        assert_eq!(code, None);
    }

    #[test]
    fn sentinel_reader_strips_success_marker_and_reports_exit_code() {
        let mut reader = ExecSentinelReader::new("__FABRO_RC_t1__".to_string());
        // One line of latency: the previous line is only released once the
        // next line proves it was not the sentinel.
        assert!(reader.push(b"probe\n").is_empty());
        assert_eq!(reader.push(b"__FABRO_RC_t1__=0\n"), b"probe\n");
        let (output, code) = reader.finish();
        assert!(output.is_empty());
        assert_eq!(code, Some(0));
    }

    #[test]
    fn sentinel_reader_strips_marker_from_binary_payload_without_trailing_newline() {
        let mut reader = ExecSentinelReader::new("__FABRO_RC_t2__".to_string());
        assert!(reader.push(b"\x00\x01binary").is_empty());
        assert!(reader.push(b"__FABRO_RC_t2__=7\n").is_empty());
        let (output, code) = reader.finish();
        assert_eq!(output, b"\x00\x01binary");
        assert_eq!(code, Some(7));
    }

    #[test]
    fn sentinel_reader_reports_negative_exit_codes() {
        let mut reader = ExecSentinelReader::new("__FABRO_RC_t3__".to_string());
        let _ = reader.push(b"out\n");
        let _ = reader.push(b"__FABRO_RC_t3__=143\n");
        let (_, code) = reader.finish();
        assert_eq!(code, Some(143));
    }

    #[test]
    fn sentinel_reader_keeps_output_when_last_line_merely_contains_marker_text() {
        let mut reader = ExecSentinelReader::new("__FABRO_RC_t4__".to_string());
        let _ = reader.push(b"user output __FABRO_RC_t4__=12 no newline");
        let (output, code) = reader.finish();
        assert_eq!(code, None);
        assert_eq!(
            String::from_utf8_lossy(&output),
            "user output __FABRO_RC_t4__=12 no newline"
        );
    }

    #[test]
    fn sentinel_parser_rejects_nondigit_status() {
        assert_eq!(
            parse_sentinel(b"__FABRO_RC_t5__==abc", "__FABRO_RC_t5__"),
            None
        );
        assert_eq!(parse_sentinel(b"__FABRO_RC_t5__=", "__FABRO_RC_t5__"), None);
        assert_eq!(parse_sentinel(b"__FABRO_RC_t5__", "__FABRO_RC_t5__"), None);
    }

    #[test]
    fn pod_names_are_rfc_1123_lowercase_derived_from_the_run_id() {
        let run_id: RunId = "01HY0000000000000000000000".parse().unwrap();
        assert_eq!(
            pod_name_for_run(&run_id),
            "fabro-01hy0000000000000000000000"
        );
    }

    #[test]
    fn preflight_pod_names_are_unique_and_dns_safe() {
        let first = preflight_pod_name();
        let second = preflight_pod_name();
        assert_ne!(first, second);
        assert!(first.starts_with("fabro-"));
        assert!(first.len() <= 63);
        assert!(
            first
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
        );
    }

    #[test]
    fn pod_labels_include_managed_run_id_and_sandbox_keys_over_user_values() {
        let run_id: RunId = "01HY0000000000000000000000".parse().unwrap();
        let user_labels = HashMap::from([
            ("team".to_string(), "platform".to_string()),
            (MANAGED_LABEL.to_string(), "false".to_string()),
            (RUN_ID_LABEL.to_string(), "wrong".to_string()),
        ]);
        let labels = labels_for_pod("fabro-pod", Some(&run_id), &user_labels);
        assert_eq!(labels.get("team").map(String::as_str), Some("platform"));
        assert_eq!(labels.get(MANAGED_LABEL).map(String::as_str), Some("true"));
        assert_eq!(
            labels.get(RUN_ID_LABEL).map(String::as_str),
            Some("01HY0000000000000000000000")
        );
        assert_eq!(
            labels.get(SANDBOX_LABEL).map(String::as_str),
            Some("fabro-pod")
        );
        assert_eq!(labels.len(), 4);
    }

    #[test]
    fn exec_wrapper_blank_caller_bash_env_and_quote_the_command() {
        let (stop_file, pid_file, marker) = exec_control_paths();
        let wrapper = controlled_shell_command(
            "echo 'hi there'",
            &stop_file,
            &pid_file,
            &marker,
            "/workspace/repo",
            &["K=V with spaces".to_string()],
        );
        assert!(
            wrapper.contains("'K=V with spaces'"),
            "env values must be quoted: {wrapper}"
        );
        assert!(
            wrapper.contains("BASH_ENV="),
            "BASH_ENV must be blanked: {wrapper}"
        );
        assert!(
            wrapper.contains(&format!("user_command={}", shell_quote("echo 'hi there'"))),
            "the user command must be shell-quoted: {wrapper}"
        );
        assert!(
            wrapper.contains("cd /workspace/repo"),
            "cwd must be quoted: {wrapper}"
        );
        assert!(
            wrapper.contains(&marker),
            "the sentinel must be emitted: {wrapper}"
        );
        assert!(
            wrapper.contains("setsid"),
            "the command must run in its own group: {wrapper}"
        );
    }

    #[test]
    fn exec_argv_blank_bash_env_before_the_wrapper_bash_starts() {
        let argv = kubernetes_exec_argv("echo hi".to_string());
        assert_eq!(argv.first().map(String::as_str), Some("env"));
        assert!(argv.contains(&"BASH_ENV=".to_string()));
        assert_eq!(argv.get(2).map(String::as_str), Some(REMOTE_BASH));
    }

    #[test]
    fn network_policy_block_denies_all_egress() {
        let policy =
            network_policy_manifest("fabro-pod", Some("uid-1"), &KubernetesNetworkMode::Block);
        let spec = policy.spec.expect("policy spec");
        assert_eq!(
            spec.policy_types.as_deref(),
            Some(&["Egress".to_string()][..])
        );
        assert_eq!(spec.egress.as_ref().map(Vec::is_empty), Some(true));
        let selector = spec.pod_selector.expect("pod selector");
        assert_eq!(
            selector
                .match_labels
                .as_ref()
                .map(|labels| labels.get(SANDBOX_LABEL).cloned()),
            Some(Some("fabro-pod".to_string()))
        );
        let owner = policy.metadata.owner_references.expect("owner reference");
        assert_eq!(owner.len(), 1);
        assert_eq!(owner[0].kind, "Pod");
        assert_eq!(owner[0].uid, "uid-1");
    }

    #[test]
    fn network_policy_allow_all_needs_no_manifest() {
        let policy = network_policy_manifest("fabro-pod", None, &KubernetesNetworkMode::AllowAll);
        assert!(policy.spec.expect("spec").egress.is_none());
        assert!(policy.metadata.owner_references.is_none());
    }

    #[test]
    fn network_policy_cidr_allow_list_permits_cidrs_and_cluster_dns() {
        let policy = network_policy_manifest(
            "fabro-pod",
            None,
            &KubernetesNetworkMode::CidrAllowList(vec!["10.0.0.0/8".to_string()]),
        );
        let egress = policy.spec.expect("spec").egress.expect("egress rules");
        assert_eq!(egress.len(), 2);
        let dns_rule = &egress[1];
        let ports = dns_rule.ports.as_ref().expect("dns ports");
        assert_eq!(ports.len(), 2);
        let destinations = dns_rule.to.as_ref().expect("dns destinations");
        assert!(destinations.iter().any(|peer| {
            peer.ip_block
                .as_ref()
                .is_some_and(|block| block.cidr == "0.0.0.0/0")
        }));
        let cidr_rule = &egress[0];
        let destinations = cidr_rule.to.as_ref().expect("cidr destinations");
        assert!(destinations.iter().any(|peer| {
            peer.ip_block
                .as_ref()
                .is_some_and(|block| block.cidr == "10.0.0.0/8")
        }));
    }

    #[test]
    fn pod_manifest_applies_quantities_restart_policy_and_labels() {
        const GIB: i64 = 1024 * 1024 * 1024;
        let run_id: RunId = "01HY0000000000000000000000".parse().unwrap();
        let config = KubernetesSandboxOptions {
            cpu: Some(2),
            memory_limit: Some(4 * GIB),
            ephemeral_storage_limit: Some(20 * GIB),
            ..KubernetesSandboxOptions::default()
        };
        let pod = pod_manifest("fabro-pod", &config, Some(&run_id));
        let spec = pod.spec.expect("pod spec");
        assert_eq!(spec.restart_policy.as_deref(), Some("Never"));
        let container = &spec.containers[0];
        assert_eq!(container.name, POD_CONTAINER_NAME);
        assert_eq!(
            container.command.clone(),
            Some(vec!["sleep".to_string(), "infinity".to_string()])
        );
        let resources = container.resources.as_ref().expect("resources");
        let limits = resources.limits.as_ref().expect("limits");
        assert_eq!(
            limits.get("cpu").map(|q| q.0.clone()),
            Some("2".to_string())
        );
        assert_eq!(
            limits.get("memory").map(|q| q.0.clone()),
            Some((4 * GIB).to_string())
        );
        assert_eq!(
            limits.get("ephemeral-storage").map(|q| q.0.clone()),
            Some((20 * GIB).to_string())
        );
        assert_eq!(resources.requests, resources.limits);
        let labels = pod.metadata.labels.expect("labels");
        assert_eq!(
            labels.get(RUN_ID_LABEL).map(String::as_str),
            Some("01HY0000000000000000000000")
        );
    }

    #[test]
    fn pod_manifest_omits_empty_quantities_and_env() {
        let pod = pod_manifest("fabro-pod", &KubernetesSandboxOptions::default(), None);
        let container = &pod.spec.expect("spec").containers[0];
        assert!(container.resources.is_none());
        assert!(container.env.is_none());
        assert!(
            !pod.metadata
                .labels
                .expect("labels")
                .contains_key(RUN_ID_LABEL)
        );
    }

    #[test]
    fn terminal_failure_detection_names_image_pull_and_crash_reasons() {
        let pod_json = serde_json::json!({
            "metadata": { "name": "fabro-pod" },
            "status": {
                "containerStatuses": [{
                    "state": { "waiting": {
                        "reason": "ImagePullBackOff",
                        "message": "Back-off pulling image"
                    }}
                }]
            }
        });
        let pod: Pod = serde_json::from_value(pod_json).unwrap();
        let reason = terminal_failure_reason(&pod).expect("terminal failure");
        assert!(reason.contains("ImagePullBackOff"));

        let ready_json = serde_json::json!({
            "metadata": { "name": "fabro-pod" },
            "status": {
                "phase": "Running",
                "conditions": [{ "type": "Ready", "status": "True" }]
            }
        });
        let ready: Pod = serde_json::from_value(ready_json).unwrap();
        assert!(pod_is_ready(&ready));
        assert!(terminal_failure_reason(&ready).is_none());
    }

    #[test]
    fn access_command_targets_the_pod_namespace_and_working_directory() {
        let command = kubernetes_access_command("fabro-pod", "fabro-ns", "/workspace/repo");
        assert!(command.contains("kubectl exec -it fabro-pod -n fabro-ns"));
        assert!(command.contains(&shell_quote("cd /workspace/repo && exec sh -l")));
    }
}
