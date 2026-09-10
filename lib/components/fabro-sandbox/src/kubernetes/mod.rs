//! Kubernetes sandbox provider.
//!
//! One pod per run sandbox. The main container runs the user's prebuilt image
//! with its command overridden to the Fabro sandbox agent — a static binary
//! injected by an init container from the published agent image. All sandbox
//! operations (exec, files, git setup) travel through the agent's HTTP
//! protocol over a kube port-forward, so the provider needs only pod
//! lifecycle and port-forward RBAC, never pod-exec.
//!
//! The clone transport is deliberately distinct from the Docker provider's:
//! both ride the shared `clone_source` command builders and `git_retry`
//! policy, but Kubernetes evaluates every command through the agent instead
//! of the Docker exec API.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use fabro_github::GitHubCredentials;
use fabro_github::token_source::InstallationTokenSource;
use fabro_types::settings::run::RunCloneSettings;
use fabro_types::{CommandOutputStream, CommandTermination, RunId, SandboxProviderKind};
use hmac::{Hmac, Mac};
use k8s_openapi::api::core::v1::Pod;
use kube::Api;
use kube::api::PostParams;
use sha2::Sha256;
use tokio::io::{AsyncWrite, AsyncWriteExt as _, duplex};
use tokio::sync::{Mutex, OnceCell, mpsc};
use tokio::{fs, time};
use tokio_util::sync::CancellationToken;

pub(crate) mod agent_client;
pub(crate) mod pod;

pub use agent_client::AgentProcessExit;
use pod as pod_manifest;

use crate::clone_source::{self, CloneDecision, EmptyWorkspaceReason};
use crate::git_retry::{self, CredentialContext};
use crate::managed_labels::MANAGED_LABEL;
use crate::push_credentials::{self, PushCredentialState};
use crate::redact::redact_auth_url;
use crate::sandbox::{
    self, BASH_PROBE_SCRIPT, BASH_PROBE_TIMEOUT_MS, OutputCaptureBuffer, REMOTE_BASH,
    REMOTE_WALK_TIMEOUT_MS, RefreshOutcome, StdioProcessControl, optional_timeout, resolve_path,
    validate_bash_probe,
};
use crate::{
    DEFAULT_EXEC_OUTPUT_TAIL_BYTES, DirEntry, Error, ExecResult, ExecStreamingRequest,
    ExecStreamingResult, GrepOptions, Sandbox, SandboxEvent, SandboxEventCallback, SandboxFile,
    StderrCollector, StdioProcess, StdioProcessHandle, StdioProcessTermination, WalkOptions,
    shell_quote,
};

pub(crate) const WORKING_DIRECTORY: &str = "/workspace";
pub(crate) const REPOS_ROOT: &str = "/repos";
// Matches the Docker layout: beneath the system tmp dir so any container user
// can create it, with the `runtime` component recognized by durable context.
pub(crate) const RUNTIME_DIRECTORY: &str = "/tmp/fabro/runtime";

pub(crate) const DEFAULT_AGENT_PORT: u16 = 7800;
const DEFAULT_GIT_CLONE_DEPTH: usize = RunCloneSettings::DEFAULT_DEPTH.unsigned_abs() as usize;
const GIT_CLONE_TIMEOUT: Duration = Duration::from_mins(5);
/// Bound on opening one port-forward and completing the HTTP/1.1 handshake.
const AGENT_CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
/// Image pull plus agent startup for the sandbox pod.
const POD_READY_TIMEOUT: Duration = Duration::from_mins(5);
const POD_READY_POLL_INTERVAL: Duration = Duration::from_millis(500);
const KUBERNETES_BASH_REQUIREMENT: &str = "Kubernetes sandboxes require /bin/bash for every \
     command, with no `sh` fallback; use an image with bash and git, such as buildpack-deps:noble.";

/// Prefix hashed together with the pod name to derive the agent bearer token.
const AGENT_TOKEN_CONTEXT: &str = "fabro-k8s-agent:";

impl From<agent_client::AgentClientError> for Error {
    fn from(err: agent_client::AgentClientError) -> Self {
        Self::context("Sandbox agent request failed", err)
    }
}

/// Derive the sandbox agent bearer token from the server session secret.
///
/// Every process that can read the secret (server, worker via its storage
/// root) re-derives the same token deterministically, so reconnecting after a
/// restart works without persisted per-pod state. Only the derived value ever
/// enters pod env.
#[must_use]
pub fn derive_agent_token(key: &str, pod_name: &str) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(key.as_bytes()).expect("HMAC accepts any key length");
    mac.update(format!("{AGENT_TOKEN_CONTEXT}{pod_name}").as_bytes());
    let digest = hex::encode(mac.finalize().into_bytes());
    digest[..64].to_string()
}

/// Options for a Kubernetes sandbox pod.
#[derive(Clone, Debug, PartialEq)]
pub struct KubernetesSandboxOptions {
    /// Prebuilt image reference for the sandbox pod. Kubernetes environments
    /// never build images, so `image.dockerfile` is rejected at validation.
    pub image:        String,
    /// Published agent image supplying the sandbox agent binary.
    pub agent_image:  String,
    /// Port the agent listens on inside the pod.
    pub agent_port:   u16,
    /// Cluster namespace for the pod; `None` uses the kubeconfig default.
    pub namespace:    Option<String>,
    /// Additional `KEY=VALUE` environment variables for the pod.
    pub env_vars:     Vec<String>,
    /// CPU cores requested (and limited) for the sandbox.
    pub cpu:          Option<i32>,
    /// Memory limit in bytes.
    pub memory_bytes: Option<i64>,
    /// Ephemeral-storage limit in bytes.
    pub disk_bytes:   Option<i64>,
    /// User labels merged under the managed labels.
    pub labels:       Option<HashMap<String, String>>,
    /// Maximum Git history depth fetched during clone; `None` fetches full
    /// history.
    pub clone_depth:  Option<usize>,
    /// Create an empty workspace instead of cloning even when an origin
    /// exists.
    pub skip_clone:   bool,
}

impl Default for KubernetesSandboxOptions {
    fn default() -> Self {
        Self {
            image:        "buildpack-deps:noble".to_string(),
            agent_image:  String::new(),
            agent_port:   DEFAULT_AGENT_PORT,
            namespace:    None,
            env_vars:     Vec::new(),
            cpu:          None,
            memory_bytes: None,
            disk_bytes:   None,
            labels:       None,
            clone_depth:  Some(DEFAULT_GIT_CLONE_DEPTH),
            skip_clone:   false,
        }
    }
}

pub struct KubernetesSandbox {
    pods:              Api<Pod>,
    config:            KubernetesSandboxOptions,
    push_credentials:  PushCredentialState,
    run_id:            Option<RunId>,
    clone_origin_url:  Option<String>,
    clone_branch:      Option<String>,
    clone_tag:         Option<String>,
    clone_commit_sha:  Option<String>,
    agent_token_key:   Option<String>,
    pod_name:          OnceCell<String>,
    repo_cloned:       OnceCell<bool>,
    working_directory: OnceCell<String>,
    origin_url:        OnceCell<String>,
    cached_platform:   std::sync::OnceLock<String>,
    cached_os_version: std::sync::OnceLock<String>,
    rg_available:      OnceCell<bool>,
    event_callback:    Option<SandboxEventCallback>,
}

impl KubernetesSandbox {
    /// Construct a sandbox and its cluster client.
    ///
    /// The cluster is resolved from the ambient environment (in-cluster
    /// service account or default kubeconfig); no settings schema exists for
    /// cluster targeting.
    pub async fn new(
        config: KubernetesSandboxOptions,
        github_app: Option<&GitHubCredentials>,
        run_id: Option<RunId>,
        clone_origin_url: Option<String>,
        clone_branch: Option<String>,
        clone_tag: Option<String>,
        clone_commit_sha: Option<String>,
        agent_token_key: Option<String>,
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
        Self::with_kube_client(
            kube::Client::try_default()
                .await
                .map_err(Error::kubernetes_connect)?,
            config,
            github_app,
            run_id,
            clone_origin_url,
            clone_branch,
            clone_tag,
            clone_commit_sha,
            agent_token_key,
        )
        .await
    }

    /// Assemble the sandbox around an existing kube client (tests pass a
    /// client bound to a mock server here).
    #[allow(
        clippy::unused_async,
        reason = "Kept async for signature parity with `new`; \
            Daytona-style construction paths await on the same call shape."
    )]
    pub(crate) async fn with_kube_client(
        client: kube::Client,
        config: KubernetesSandboxOptions,
        github_app: Option<&GitHubCredentials>,
        run_id: Option<RunId>,
        clone_origin_url: Option<String>,
        clone_branch: Option<String>,
        clone_tag: Option<String>,
        clone_commit_sha: Option<String>,
        agent_token_key: Option<String>,
    ) -> crate::Result<Self> {
        let push_credentials = PushCredentialState::new(push_credentials::build_token_source(
            github_app,
            clone_origin_url.as_deref(),
        )?);
        let namespace = config
            .namespace
            .clone()
            .unwrap_or_else(|| "default".to_string());
        Ok(Self {
            pods: Api::namespaced(client, &namespace),
            config,
            push_credentials,
            run_id,
            clone_origin_url,
            clone_branch,
            clone_tag,
            clone_commit_sha,
            agent_token_key,
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
        self.pod_name
            .get()
            .map(String::as_str)
            .ok_or_else(|| Error::message("Kubernetes sandbox pod was not initialized"))
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
            .map_err(|_| Error::message("Working directory already initialized"))
    }

    /// Derive the bearer token this sandbox's agent expects.
    fn agent_token(&self) -> crate::Result<String> {
        let key = self.agent_token_key.as_deref().ok_or_else(|| {
            Error::message(
                "Kubernetes sandboxes require the server session secret to derive the \
                 sandbox agent token",
            )
        })?;
        Ok(derive_agent_token(key, self.pod_name()?))
    }

    fn agent_client(&self) -> crate::Result<agent_client::KubernetesAgentClient> {
        Ok(agent_client::KubernetesAgentClient::new(
            self.pods.clone(),
            self.pod_name()?.to_string(),
            self.config.agent_port,
            self.agent_token()?,
            AGENT_CONNECT_TIMEOUT,
        ))
    }

    /// Evaluate `argv` in the pod and collect its output.
    async fn pod_exec(
        &self,
        argv: Vec<String>,
        working_dir: Option<&str>,
        env: Option<Vec<String>>,
    ) -> crate::Result<agent_client::AgentExecOutput> {
        let mut request = agent_client::AgentExecRequest::new(argv);
        request.working_dir = working_dir.map(ToString::to_string);
        request.env = env.unwrap_or_default();
        Ok(self.agent_client()?.exec(request).await?)
    }

    /// Evaluate a Bash program under the sandbox interpreter contract.
    async fn exec_shell(
        &self,
        command: &str,
        timeout_ms: u64,
        working_dir: Option<&str>,
        env_vars: Option<&HashMap<String, String>>,
        cancel_token: Option<CancellationToken>,
    ) -> crate::Result<ExecResult> {
        let start = Instant::now();
        let effective_dir = working_dir.unwrap_or_else(|| self.working_directory());
        let mut request = agent_client::AgentExecRequest::new(vec![
            REMOTE_BASH.to_string(),
            "-c".to_string(),
            command.to_string(),
        ]);
        request.working_dir = Some(effective_dir.to_string());
        request.env = kubernetes_bash_exec_env(env_vars);

        let timeout_duration = Duration::from_millis(timeout_ms);
        let token = cancel_token.unwrap_or_default();
        let client = self.agent_client()?;

        tokio::select! {
            result = client.exec(request) => {
                let output = result?;
                let duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
                Ok(ExecResult {
                    stdout: output.stdout_lossy(),
                    stderr: output.stderr_lossy(),
                    exit_code: Some(output.exit_code()),
                    termination: CommandTermination::Exited,
                    duration_ms,
                })
            }
            () = time::sleep(timeout_duration) => {
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

    /// Streaming variant of [`exec_shell`](Self::exec_shell).
    ///
    /// The command runs as a tracked agent process so timeouts and
    /// cancellation terminate its process group deterministically instead of
    /// leaving it running behind an abandoned connection.
    async fn exec_shell_streaming(
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
        let mut agent_request = agent_client::AgentExecRequest::new(vec![
            REMOTE_BASH.to_string(),
            "-c".to_string(),
            command.to_string(),
        ]);
        agent_request.working_dir = Some(effective_dir);
        agent_request.env = kubernetes_bash_exec_env(env_vars);
        agent_request.stdin_b64 = stdin.map(|bytes| BASE64.encode(bytes));

        let client = self.agent_client()?;
        let (process_id, mut stream) = client
            .spawn(agent_request)
            .await
            .map_err(|err| Error::context("Failed to start Kubernetes sandbox command", err))?;

        let timeout_future = optional_timeout(timeout_ms);
        tokio::pin!(timeout_future);
        let token = cancel_token.unwrap_or_default();

        let mut stdout = OutputCaptureBuffer::new(stream_output_bytes_cap);
        let mut stderr = OutputCaptureBuffer::new(stream_output_bytes_cap);
        let mut termination = CommandTermination::Exited;
        let mut exit: Option<agent_client::AgentProcessExit> = None;
        let mut transport_failed: Option<Error> = None;

        loop {
            tokio::select! {
                event = stream.next() => {
                    match event {
                        Ok(Some(agent_client::AgentOutput::Stdout(bytes))) => {
                            stdout.push(&bytes);
                            if let Some(output_callback) = output_callback.as_ref() {
                                output_callback(CommandOutputStream::Stdout, bytes).await?;
                            }
                        }
                        Ok(Some(agent_client::AgentOutput::Stderr(bytes))) => {
                            stderr.push(&bytes);
                            if let Some(output_callback) = output_callback.as_ref() {
                                output_callback(CommandOutputStream::Stderr, bytes).await?;
                            }
                        }
                        Ok(Some(agent_client::AgentOutput::Exit(process_exit))) => {
                            exit = Some(process_exit);
                            break;
                        }
                        Ok(None) => break,
                        Err(err) => {
                            transport_failed = Some(Error::context(
                                "Kubernetes sandbox command stream failed",
                                err,
                            ));
                            break;
                        }
                    }
                }
                () = &mut timeout_future => {
                    termination = CommandTermination::TimedOut;
                    self.request_process_stop(&client, process_id).await?;
                    break;
                }
                () = token.cancelled() => {
                    termination = CommandTermination::Cancelled;
                    self.request_process_stop(&client, process_id).await?;
                    break;
                }
            }
        }

        if termination != CommandTermination::Exited && exit.is_none() {
            // Termination was requested; drain the terminal event so the
            // output reflects everything the process produced.
            loop {
                let event = time::timeout(Duration::from_secs(10), stream.next()).await;
                match event {
                    Ok(Ok(Some(agent_client::AgentOutput::Exit(process_exit)))) => {
                        exit = Some(process_exit);
                        break;
                    }
                    Ok(Ok(Some(agent_client::AgentOutput::Stdout(bytes)))) => stdout.push(&bytes),
                    Ok(Ok(Some(agent_client::AgentOutput::Stderr(bytes)))) => stderr.push(&bytes),
                    Ok(Ok(None) | Err(_)) | Err(_) => break,
                }
            }
        }

        if let Some(error) = transport_failed {
            return Err(error);
        }

        let (stdout_bytes, stdout_capture) = stdout.into_parts();
        let (stderr_bytes, stderr_capture) = stderr.into_parts();
        let duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
        Ok(ExecStreamingResult {
            result: ExecResult {
                stdout: String::from_utf8_lossy(&stdout_bytes).into_owned(),
                stderr: String::from_utf8_lossy(&stderr_bytes).into_owned(),
                exit_code: (termination == CommandTermination::Exited)
                    .then(|| exit.and_then(|exit| exit.exit_code).unwrap_or(-1)),
                termination,
                duration_ms,
            },
            streams_separated: true,
            live_streaming: true,
            stdout_capture,
            stderr_capture,
        })
    }

    async fn request_process_stop(
        &self,
        client: &agent_client::KubernetesAgentClient,
        process_id: u64,
    ) -> crate::Result<()> {
        client.terminate(process_id).await.map_err(|err| {
            Error::context(
                format!("Failed to stop Kubernetes sandbox process {process_id}"),
                err,
            )
        })
    }

    /// Verify the pod evaluates commands as non-login Bash.
    async fn probe_bash(&self, working_dir: Option<&str>) -> crate::Result<()> {
        let result = self
            .exec_shell(
                BASH_PROBE_SCRIPT,
                BASH_PROBE_TIMEOUT_MS,
                Some(working_dir.unwrap_or("/")),
                None,
                None,
            )
            .await?;
        validate_bash_probe(result, KUBERNETES_BASH_REQUIREMENT)
    }

    async fn verify_git_available(&self) -> crate::Result<()> {
        let output = self
            .pod_exec(vec!["git".to_string(), "--version".to_string()], None, None)
            .await?;
        if output.exit_code() != 0 {
            return Err(Error::message(format!(
                "git is not available in the Kubernetes sandbox image: {}",
                output.stderr_lossy()
            )));
        }
        Ok(())
    }

    async fn create_workspace(&self) -> crate::Result<()> {
        let result = self
            .exec_shell(
                &format!("mkdir -p {}", shell_quote(WORKING_DIRECTORY)),
                10_000,
                Some("/"),
                None,
                None,
            )
            .await?;
        if !result.is_success() {
            return Err(Error::message(format!(
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
            .exec_shell(
                &format!("umask 077 && mkdir -p {}", shell_quote(RUNTIME_DIRECTORY)),
                10_000,
                Some("/"),
                None,
                None,
            )
            .await?;
        if !result.is_success() {
            return Err(Error::message(format!(
                "Failed to create Kubernetes runtime directory (exit {}): {}",
                result.display_exit_code(),
                result.stderr
            )));
        }
        Ok(())
    }

    /// Wait until the pod reports Running and its agent answers execs.
    async fn wait_for_pod_ready(&self, name: &str) -> crate::Result<()> {
        let deadline = time::Instant::now() + POD_READY_TIMEOUT;
        loop {
            let pod = self.get_pod(name).await?;
            let phase = pod_manifest::pod_phase(&pod)
                .unwrap_or("Unknown")
                .to_string();
            match phase.as_str() {
                "Running" => {
                    self.probe_bash(Some("/")).await.map_err(|err| {
                        Error::context(format!("Sandbox agent in pod '{name}' is not ready"), err)
                    })?;
                    return Ok(());
                }
                "Succeeded" | "Failed" => {
                    return Err(Error::message(format!(
                        "Kubernetes sandbox pod '{name}' reached phase {phase} before \
                         becoming ready; check pod events for the agent image and entrypoint"
                    )));
                }
                _ => {}
            }
            if time::Instant::now() >= deadline {
                return Err(Error::message(format!(
                    "Kubernetes sandbox pod '{name}' did not become Running within \
                     {}s (phase {phase}); the cluster may be unable to pull the sandbox \
                     or agent image",
                    POD_READY_TIMEOUT.as_secs()
                )));
            }
            time::sleep(POD_READY_POLL_INTERVAL).await;
        }
    }

    async fn get_pod(&self, name: &str) -> crate::Result<Pod> {
        self.pods.get(name).await.map_err(|err| {
            if kubernetes_not_found(&err) {
                Error::message(format!("Kubernetes sandbox pod '{name}' is gone"))
            } else {
                Error::context(format!("Failed to get Kubernetes pod '{name}'"), err)
            }
        })
    }

    async fn ensure_name_available(&self, name: &str) -> crate::Result<()> {
        match self.pods.get(name).await {
            Ok(_) => Err(Error::message(format!(
                "Kubernetes pod '{name}' already exists. Remove the stale pod manually \
                 before retrying."
            ))),
            Err(err) if kubernetes_not_found(&err) => Ok(()),
            Err(err) => Err(Error::context(
                format!("Failed to check Kubernetes pod '{name}' before creation"),
                err,
            )),
        }
    }

    async fn fetch_pod(&self, name: &str) -> crate::Result<Option<Pod>> {
        match self.pods.get(name).await {
            Ok(pod) => Ok(Some(pod)),
            Err(err) if kubernetes_not_found(&err) => Ok(None),
            Err(err) => Err(Error::context(
                format!("Failed to get Kubernetes pod '{name}'"),
                err,
            )),
        }
    }

    async fn delete_pod(&self, name: &str) -> crate::Result<()> {
        use kube::api::DeleteParams;
        match self.pods.delete(name, &DeleteParams::default()).await {
            Ok(_) => Ok(()),
            Err(err) if kubernetes_not_found(&err) => Ok(()),
            Err(err) => Err(Error::context(
                format!("Failed to delete Kubernetes pod '{name}'"),
                err,
            )),
        }
    }

    fn validate_managed_pod(&self, pod: &Pod, name: &str) -> crate::Result<()> {
        if !pod_manifest::managed_from_pod(pod) {
            return Err(Error::message(format!(
                "Kubernetes pod '{name}' is missing label {MANAGED_LABEL}; refusing to \
                 manage a pod Fabro did not create"
            )));
        }
        if let Some(run_id) = self.run_id.as_ref() {
            let recorded = pod_manifest::run_id_from_pod(pod);
            if recorded.as_deref() != Some(run_id.to_string().as_str()) {
                return Err(Error::message(format!(
                    "Kubernetes pod '{name}' belongs to run {recorded:?}, not run {run_id}"
                )));
            }
        }
        Ok(())
    }

    /// Read a file through the agent's binary-safe output channel.
    async fn download_file_bytes(&self, remote_path: &str) -> crate::Result<Vec<u8>> {
        let pod_path = self.resolve_pod_path(remote_path);
        let output = self
            .pod_exec(vec!["cat".to_string(), pod_path.clone()], None, None)
            .await?;
        if output.exit_code() != 0 {
            return Err(Error::message(format!(
                "Failed to read {pod_path}: {}",
                output.stderr_lossy()
            )));
        }
        Ok(output.stdout)
    }

    /// Write a file through the agent's stdin channel so binary content
    /// survives without shell quoting.
    async fn upload_bytes_to_pod(&self, path: &str, bytes: &[u8]) -> crate::Result<()> {
        let pod_path = self.resolve_pod_path(path);
        let parent_dir = std::path::Path::new(&pod_path)
            .parent()
            .map_or_else(|| "/".to_string(), |p| p.to_string_lossy().to_string());

        // Fabro runtime files stay owner-private; repository files keep the
        // conventional world-readable mode.
        let is_runtime_path = pod_path.starts_with(&format!("{RUNTIME_DIRECTORY}/"));
        let mkdir_cmd = if is_runtime_path {
            format!("umask 077 && mkdir -p {}", shell_quote(&parent_dir))
        } else {
            format!("mkdir -p {}", shell_quote(&parent_dir))
        };
        let result = self
            .exec_shell(&mkdir_cmd, 10_000, Some("/"), None, None)
            .await?;
        if !result.is_success() {
            return Err(Error::message(format!(
                "Failed to create parent dirs for {pod_path}: {}",
                result.stderr
            )));
        }

        let command = format!("cat > {}", shell_quote(&pod_path));
        let mut request = agent_client::AgentExecRequest::new(vec![
            REMOTE_BASH.to_string(),
            "-c".to_string(),
            command,
        ]);
        request.working_dir = Some("/".to_string());
        request.stdin_b64 = Some(BASE64.encode(bytes));
        let output = self.agent_client()?.exec(request).await?;
        if output.exit_code() != 0 {
            return Err(Error::message(format!(
                "Failed to write {pod_path}: {}",
                output.stderr_lossy()
            )));
        }
        Ok(())
    }

    fn clone_failure_error(
        &self,
        result: ExecResult,
        label: &'static str,
        auth_url: Option<&fabro_redact::DisplaySafeUrl>,
        network_step: bool,
    ) -> Error {
        let error =
            result.into_exec_error_with_redactor(label, |output| redact_auth_url(output, auth_url));
        let message = match network_step {
            true if self.push_credentials.source().is_none() => {
                "Git clone failed. If this is a private repository, configure a GitHub App with \
                 `fabro install` and install it for your organization."
            }
            true => "Failed to clone repository into Kubernetes sandbox",
            false => "Failed to prepare the cloned repository in the Kubernetes sandbox",
        };
        Error::context(message, error)
    }

    fn report_clone_failure(&self, origin_url: &str, err: Error) -> Error {
        self.emit(SandboxEvent::GitCloneFailed {
            url:    origin_url.to_string(),
            error:  err.to_string(),
            causes: err.causes(),
        });
        err
    }

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
            return Err(Error::message(format!(
                "{label} deadline expired before the step could run"
            )));
        }
        let result = self
            .exec_shell(command, timeout_ms, Some("/"), None, None)
            .await
            .map_err(|error| Error::context(format!("{label} transport failed"), error))?;
        if result.is_success() {
            Ok(result)
        } else {
            Err(self.clone_failure_error(result, label, auth_url, false))
        }
    }

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
                        error:        Error::message(format!(
                            "{label} deadline expired before retry"
                        )),
                        retry_reason: None,
                    });
                }
                let result = self
                    .exec_shell(command, timeout_ms, Some("/"), None, None)
                    .await
                    .map_err(|error| KubernetesCloneFailure {
                        error:        Error::context(format!("{label} transport failed"), error),
                        retry_reason: None,
                    })?;
                if result.is_success() {
                    return Ok(());
                }
                let retry_reason = classify_kubernetes_clone_result(&result, credential_context);
                Err(KubernetesCloneFailure {
                    error: self.clone_failure_error(result, exec_label, auth_url, true),
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
        // the shared source, matching the Docker provider.
        let resolved_token = match self.push_credentials.source() {
            Some(source) => Some(source.mint_for_clone().await.map_err(|err| {
                Error::context_anyhow("Failed to get GitHub App credentials for clone", err)
            })?),
            None => None,
        };
        let clone_credential_context =
            CredentialContext::from_snapshot(resolved_token.as_ref().map(|token| &token.snapshot));

        let auth_url = match &resolved_token {
            Some(token) => Some(
                fabro_github::embed_token_in_url(&origin_url, token.token.expose()).map_err(
                    |err| {
                        Error::context_anyhow("Failed to build authenticated GitHub clone URL", err)
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
            .exec_shell(&prepare_command, 10_000, Some("/"), None, None)
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
            let Some(branch) = branch.as_deref().filter(|branch| !branch.trim().is_empty()) else {
                let error = Error::message(format!("{} requires a repository branch", pin.label()));
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
            .exec_shell(&symlink_command, 10_000, Some("/"), None, None)
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
            self.push_credentials.record_embedded(token).await;
        }

        if let Some(auth_url) = auth_url.as_ref() {
            let command = format!(
                "git -c maintenance.auto=0 remote set-url origin {}",
                shell_quote(auth_url.as_raw_url().as_str())
            );
            let result = self
                .exec_shell(
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

    /// Reconnect to an existing sandbox pod from a saved run record.
    ///
    /// The agent token is re-derived from the shared secret, so a server
    /// restart never orphans a running sandbox.
    pub async fn reconnect(
        pod_name: &str,
        agent_token_key: Option<String>,
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
            agent_token_key,
        )
        .await?;
        let pod = sandbox.fetch_pod(pod_name).await?.ok_or_else(|| {
            Error::message(
                "Kubernetes sandbox workspace was ephemeral and its pod no longer exists",
            )
        })?;
        sandbox.validate_managed_pod(&pod, pod_name)?;
        sandbox
            .pod_name
            .set(pod_name.to_string())
            .map_err(|_| Error::message("Pod already initialized"))?;
        sandbox
            .repo_cloned
            .set(repo_cloned)
            .map_err(|_| Error::message("Clone state already initialized"))?;
        sandbox
            .working_directory
            .set(working_directory)
            .map_err(|_| Error::message("Working directory already initialized"))?;
        if repo_cloned {
            if let Some(origin) = clone_origin_url {
                let _ = sandbox.origin_url.set(origin);
            }
        }
        Ok(sandbox)
    }
}

struct KubernetesCloneFailure {
    error:        Error,
    retry_reason: Option<git_retry::GitRetryReason>,
}

fn classify_kubernetes_clone_result(
    result: &ExecResult,
    cred: CredentialContext,
) -> Option<git_retry::GitRetryReason> {
    git_retry::classify_output(&result.stderr, &result.stdout, cred).retry_reason()
}

/// Remove caller/image startup-file injection and explicitly override any
/// inherited image value for sandbox-created processes.
fn kubernetes_bash_exec_env(env_vars: Option<&HashMap<String, String>>) -> Vec<String> {
    let entries = env_vars.map_or_else(Vec::new, |vars| {
        vars.iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect()
    });
    sandbox::scrub_bash_env_entries(entries)
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

pub(crate) fn kubernetes_not_found(error: &kube::Error) -> bool {
    matches!(error, kube::Error::Api(status) if status.is_not_found())
}

#[async_trait]
impl Sandbox for KubernetesSandbox {
    async fn download_file_to_local(
        &self,
        remote_path: &str,
        local_path: &std::path::Path,
    ) -> crate::Result<()> {
        let bytes = self.download_file_bytes(remote_path).await?;
        if let Some(parent) = local_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| Error::context("Failed to create parent dirs", e))?;
        }
        fs::write(local_path, bytes)
            .await
            .map_err(|e| Error::context(format!("Failed to write {}", local_path.display()), e))
    }

    async fn upload_file_from_local(
        &self,
        local_path: &std::path::Path,
        remote_path: &str,
    ) -> crate::Result<()> {
        let bytes = fs::read(local_path)
            .await
            .map_err(|e| Error::context(format!("Failed to read {}", local_path.display()), e))?;
        self.upload_bytes_to_pod(remote_path, &bytes).await
    }

    async fn initialize(&self) -> crate::Result<()> {
        self.emit(SandboxEvent::Initializing {
            provider: "kubernetes".into(),
        });
        let init_start = Instant::now();

        let name = match self.pod_name.get() {
            Some(existing) => existing.clone(),
            None => match self.run_id.as_ref() {
                Some(run_id) => pod_manifest::pod_name(run_id),
                None => pod_manifest::anonymous_pod_name(),
            },
        };
        self.ensure_name_available(&name)
            .await
            .map_err(|e| self.fail_init(init_start, e))?;
        let _ = self.pod_name.set(name.clone());

        let token = self
            .agent_token()
            .map_err(|e| self.fail_init(init_start, e))?;
        let manifest = pod_manifest::build_pod(&name, &self.config, self.run_id.as_ref(), &token);
        self.pods
            .create(&PostParams::default(), &manifest)
            .await
            .map_err(|err| {
                self.fail_init(
                    init_start,
                    Error::context(format!("Failed to create Kubernetes pod '{name}'"), err),
                )
            })?;

        if let Err(e) = self.wait_for_pod_ready(&name).await {
            return Err(self.fail_init(init_start, e));
        }

        let (uname_output, _, _) = {
            let output = self
                .pod_exec(vec!["uname".to_string(), "-r".to_string()], None, None)
                .await
                .map_err(|e| self.fail_init(init_start, e))?;
            (
                output.stdout_lossy(),
                output.stderr_lossy(),
                output.exit_code(),
            )
        };
        let _ = self.cached_platform.set("linux".to_string());
        let _ = self
            .cached_os_version
            .set(format!("linux {}", uname_output.trim()));

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
            name:        Some(name),
            cpu:         self.config.cpu.map(f64::from),
            memory:      self.config.memory_bytes.map(|bytes| bytes as f64),
            url:         None,
        });

        Ok(())
    }

    async fn start(&self) -> crate::Result<()> {
        self.emit(SandboxEvent::StartStarted {
            provider: "kubernetes".into(),
        });
        let start = Instant::now();
        let name = self.pod_name()?.to_string();
        let pod = self.fetch_pod(&name).await?;
        let Some(pod) = pod else {
            return self.start_error(Error::message(format!(
                "Kubernetes sandbox pod '{name}' is gone"
            )));
        };
        if let Err(error) = self.validate_managed_pod(&pod, &name) {
            return self.start_error(error);
        }
        let phase = pod_manifest::pod_phase(&pod).unwrap_or("Unknown");
        if phase != "Running" {
            return self.start_error(Error::message(format!(
                "Kubernetes sandbox pod '{name}' is in phase {phase}, not Running"
            )));
        }
        if let Err(error) = self.probe_bash(Some(self.working_directory())).await {
            return self.start_error(error);
        }
        let duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.emit(SandboxEvent::StartCompleted {
            provider: "kubernetes".into(),
            duration_ms,
        });
        Ok(())
    }

    async fn stop(&self) -> crate::Result<()> {
        self.emit(SandboxEvent::StopStarted {
            provider: "kubernetes".into(),
        });
        let start = Instant::now();
        // A pod cannot be parked like a Docker container: `emptyDir` storage
        // dies with the pod and a RestartPolicy=Never pod cannot resume, so
        // stopping a Kubernetes sandbox deletes it. Runs that need to survive
        // stopping use the git-checkpoint based resume instead.
        if let Some(name) = self.pod_name.get().cloned() {
            if let Err(e) = self.delete_pod(&name).await {
                return self.stop_error(e);
            }
        }
        let duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
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
        if let Some(name) = self.pod_name.get().cloned() {
            if let Err(e) = self.delete_pod(&name).await {
                return self.delete_error(e);
            }
        }
        let duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
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
        self.exec_shell(command, timeout_ms, dir.as_deref(), env_vars, cancel_token)
            .await
    }

    async fn exec_command_streaming(
        &self,
        request: ExecStreamingRequest<'_>,
    ) -> crate::Result<ExecStreamingResult> {
        let dir = request.working_dir.map(|path| self.resolve_pod_path(path));
        self.exec_shell_streaming(ExecStreamingRequest {
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
        let mut agent_request = agent_client::AgentExecRequest::new(vec![
            REMOTE_BASH.to_string(),
            "-c".to_string(),
            command.to_string(),
        ]);
        agent_request.working_dir = Some(effective_dir);
        agent_request.env = kubernetes_bash_exec_env(env_vars);

        let client = self.agent_client()?;
        let (process_id, mut stream) = client
            .spawn(agent_request)
            .await
            .map_err(|err| Error::context("Failed to start Kubernetes sandbox process", err))?;

        let stderr_collector = StderrCollector::new(DEFAULT_EXEC_OUTPUT_TAIL_BYTES);
        let stderr_for_output = stderr_collector.clone();
        let (mut stdout_writer, stdout_reader) = duplex(64 * 1024);

        let state = Arc::new(KubernetesStdioProcessState::default());
        let state_for_output = Arc::clone(&state);
        let state_for_control = Arc::clone(&state);
        let client_for_control = client.clone();
        // The pump forwards process stdout to the reader and caches the exit
        // status; dropping the JoinHandle detaches it for the process's life.
        let pump = tokio::spawn(async move {
            loop {
                match stream.next().await {
                    Ok(Some(agent_client::AgentOutput::Stdout(bytes))) => {
                        if stdout_writer.write_all(&bytes).await.is_err() {
                            break;
                        }
                    }
                    Ok(Some(agent_client::AgentOutput::Stderr(bytes))) => {
                        stderr_for_output.push(&bytes).await;
                    }
                    Ok(Some(agent_client::AgentOutput::Exit(exit))) => {
                        state_for_output
                            .cache_termination(StdioProcessTermination::exited(exit.exit_code))
                            .await;
                        break;
                    }
                    Ok(None) | Err(_) => break,
                }
            }
            let _ = stdout_writer.shutdown().await;
            state_for_output
                .cache_termination(StdioProcessTermination::exited(Some(-1)))
                .await;
        });
        drop(pump);

        let (stdin_sender, stdin_receiver) = mpsc::unbounded_channel::<StdinMessage>();
        let stdin_client = client.clone();
        let stdin_process_id = process_id;
        let stdin_task = tokio::spawn(async move {
            let mut stdin_receiver = stdin_receiver;
            while let Some(message) = stdin_receiver.recv().await {
                if stdin_client
                    .write_stdin(stdin_process_id, &message.bytes, message.close)
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });
        drop(stdin_task);

        let handle = StdioProcessHandle::new(KubernetesStdioProcessControl {
            client: client_for_control,
            process_id,
            state: state_for_control,
        });

        if let Some(token) = cancel_token {
            let handle_for_cancel = handle.clone();
            tokio::spawn(async move {
                token.cancelled().await;
                if let Err(err) = handle_for_cancel.terminate().await {
                    tracing::warn!(error = %err, "Failed to terminate cancelled Kubernetes sandbox process");
                }
            });
        }

        Ok(StdioProcess {
            stdin: Box::pin(StdioWriterHandle {
                sender: stdin_sender,
            }),
            stdout: Box::pin(stdout_reader),
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
        let output = self
            .pod_exec(vec!["cat".to_string(), pod_path.clone()], None, None)
            .await?;
        if output.exit_code() != 0 {
            return Err(Error::message(format!(
                "Failed to read {pod_path}: {}",
                output.stderr_lossy()
            )));
        }
        Ok(crate::format_lines_numbered(
            &output.stdout_lossy(),
            offset,
            limit,
        ))
    }

    async fn write_file(&self, path: &str, content: &str) -> crate::Result<()> {
        self.upload_bytes_to_pod(path, content.as_bytes()).await
    }

    async fn delete_file(&self, path: &str) -> crate::Result<()> {
        let pod_path = self.resolve_pod_path(path);
        let output = self
            .pod_exec(
                vec!["rm".to_string(), "-f".to_string(), pod_path.clone()],
                None,
                None,
            )
            .await?;
        if output.exit_code() != 0 {
            return Err(Error::message(format!(
                "Failed to delete {pod_path}: {}",
                output.stderr_lossy()
            )));
        }
        Ok(())
    }

    async fn file_exists(&self, path: &str) -> crate::Result<bool> {
        let pod_path = self.resolve_pod_path(path);
        let output = self
            .pod_exec(
                vec!["test".to_string(), "-e".to_string(), pod_path],
                None,
                None,
            )
            .await?;
        Ok(output.exit_code() == 0)
    }

    async fn list_directory(
        &self,
        path: &str,
        depth: Option<usize>,
    ) -> crate::Result<Vec<DirEntry>> {
        let pod_path = self.resolve_pod_path(path);
        let max_depth = depth.unwrap_or(1);
        let output = self
            .pod_exec(
                vec![
                    "find".to_string(),
                    pod_path.clone(),
                    "-mindepth".to_string(),
                    "1".to_string(),
                    "-maxdepth".to_string(),
                    max_depth.to_string(),
                    "-printf".to_string(),
                    "%y\t%s\t%P\n".to_string(),
                ],
                None,
                None,
            )
            .await?;
        if output.exit_code() != 0 {
            return Err(Error::message(format!(
                "Failed to list directory {pod_path}: {}",
                output.stderr_lossy()
            )));
        }

        let mut entries: Vec<DirEntry> = output
            .stdout_lossy()
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
        use std::fmt::Write as _;
        let pod_path = self.resolve_pod_path(path);
        let use_rg = *self
            .rg_available
            .get_or_init(|| async {
                matches!(
                    self.pod_exec(vec!["which".to_string(), "rg".to_string()], None, None).await,
                    Ok(output) if output.exit_code() == 0
                )
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

        let result = self.exec_shell(&command, 30_000, None, None, None).await?;
        if result.exit_code == Some(1) {
            return Ok(Vec::new());
        }
        if !result.is_success() {
            return Err(Error::message(format!(
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
            .exec_shell(&command, REMOTE_WALK_TIMEOUT_MS, None, None, None)
            .await?;
        if !result.is_success() {
            return Err(Error::exec("recursive file traversal", result));
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
        Err(Error::message(
            "Kubernetes sandboxes do not support access commands",
        ))
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
    fn fail_init(&self, init_start: Instant, error: Error) -> Error {
        let duration_ms = u64::try_from(init_start.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.emit(SandboxEvent::InitializeFailed {
            provider: "kubernetes".into(),
            error: error.to_string(),
            causes: error.causes(),
            duration_ms,
        });
        error
    }

    fn start_error(&self, error: Error) -> crate::Result<()> {
        self.emit(SandboxEvent::StartFailed {
            provider: "kubernetes".into(),
            error:    error.to_string(),
            causes:   error.causes(),
        });
        Err(error)
    }

    fn stop_error(&self, error: Error) -> crate::Result<()> {
        self.emit(SandboxEvent::StopFailed {
            provider: "kubernetes".into(),
            error:    error.to_string(),
            causes:   error.causes(),
        });
        Err(error)
    }

    fn delete_error(&self, error: Error) -> crate::Result<()> {
        self.emit(SandboxEvent::DeleteFailed {
            provider: "kubernetes".into(),
            error:    error.to_string(),
            causes:   error.causes(),
        });
        Err(error)
    }
}

#[derive(Default)]
struct KubernetesStdioProcessState {
    termination: Mutex<Option<StdioProcessTermination>>,
}

impl KubernetesStdioProcessState {
    async fn cache_termination(&self, termination: StdioProcessTermination) {
        let mut cached = self.termination.lock().await;
        if cached.is_none() {
            *cached = Some(termination);
        }
    }

    async fn cached_termination(&self) -> Option<StdioProcessTermination> {
        *self.termination.lock().await
    }
}

struct KubernetesStdioProcessControl {
    client:     agent_client::KubernetesAgentClient,
    process_id: u64,
    state:      Arc<KubernetesStdioProcessState>,
}

#[async_trait]
impl StdioProcessControl for KubernetesStdioProcessControl {
    async fn terminate(&self) -> crate::Result<()> {
        self.client.terminate(self.process_id).await.map_err(|err| {
            Error::context(
                format!(
                    "Failed to terminate Kubernetes sandbox process {}",
                    self.process_id
                ),
                err,
            )
        })
    }

    async fn wait(&self) -> crate::Result<StdioProcessTermination> {
        if let Some(termination) = self.state.cached_termination().await {
            return Ok(termination);
        }
        // Termination arrives through the output pump; poll the cached value
        // until it lands.
        loop {
            time::sleep(Duration::from_millis(50)).await;
            if let Some(termination) = self.state.cached_termination().await {
                return Ok(termination);
            }
        }
    }
}

struct StdinMessage {
    bytes: Vec<u8>,
    close: bool,
}

/// AsyncWrite sink delivering process stdin to the agent in order. Writes
/// enqueue onto an unbounded channel so the caller never blocks on the
/// network; `shutdown` delivers EOF.
struct StdioWriterHandle {
    sender: mpsc::UnboundedSender<StdinMessage>,
}

impl AsyncWrite for StdioWriterHandle {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        self.sender
            .send(StdinMessage {
                bytes: buf.to_vec(),
                close: false,
            })
            .map(|()| buf.len())
            .map_err(|_| std::io::Error::other("sandbox process stdin is closed"))
            .into()
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        self.sender
            .send(StdinMessage {
                bytes: Vec::new(),
                close: true,
            })
            .map_err(|_| std::io::Error::other("sandbox process stdin is closed"))
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_agent_token_is_deterministic_and_key_bound() {
        let first = derive_agent_token("session-secret", "fabro-run-01hy");
        let second = derive_agent_token("session-secret", "fabro-run-01hy");

        // Re-derivation from the same secret and pod name must yield the same
        // token: reconnecting after a restart depends on it.
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
        assert!(first.chars().all(|ch| ch.is_ascii_hexdigit()));

        // A different pod name or secret must produce a different token.
        assert_ne!(
            first,
            derive_agent_token("session-secret", "fabro-run-01hz")
        );
        assert_ne!(first, derive_agent_token("other-secret", "fabro-run-01hy"));
    }

    #[test]
    fn git_clone_command_matches_docker_shape() {
        let command = git_clone_command(
            "https://github.com/acme/widgets",
            Some("main"),
            "/repos/acme/widgets",
            Some(100),
        );
        assert!(command.contains("clone"));
        assert!(command.contains("--branch main --single-branch"));
        assert!(command.contains("--depth 100"));
        assert!(command.contains("--no-tags"));
    }

    #[test]
    fn bash_exec_env_blanks_caller_bash_env() {
        let env = kubernetes_bash_exec_env(Some(&HashMap::from([
            ("BASH_ENV".to_string(), "/tmp/untrusted-startup".to_string()),
            ("MODE".to_string(), "test".to_string()),
        ])));

        assert!(env.contains(&"MODE=test".to_string()));
        assert!(env.contains(&"BASH_ENV=".to_string()));
        assert_eq!(
            env.iter()
                .filter(|entry| entry.starts_with("BASH_ENV"))
                .count(),
            1
        );
    }

    #[test]
    fn pod_paths_match_docker_layout() {
        assert_eq!(WORKING_DIRECTORY, "/workspace");
        assert_eq!(REPOS_ROOT, "/repos");
        assert_eq!(RUNTIME_DIRECTORY, "/tmp/fabro/runtime");
    }
}
