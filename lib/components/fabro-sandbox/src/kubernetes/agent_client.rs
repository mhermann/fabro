//! HTTP client for the Fabro sandbox agent inside a Kubernetes pod.
//!
//! Every call opens a dedicated kube port-forward stream and speaks HTTP/1.1
//! over it, which behaves identically in-cluster and from an operator
//! workstation. The agent protocol is JSON-lines: request bodies are JSON,
//! process responses stream `{stream, data_b64}` events terminated by an
//! `{exit_code, signalled}` event, with payload bytes base64-encoded so
//! binary output survives JSON.

use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper::client::conn::http1;
use hyper::{Method, Request, Response as HttpResponse, header};
use hyper_util::rt::TokioIo;
use k8s_openapi::api::core::v1::Pod;
use kube::Api;
use serde::{Deserialize, Serialize};
use tokio::task::JoinHandle;
use tokio::time::timeout;

pub(crate) const AGENT_PROTOCOL_VERSION: &str = "1";

/// Termination outcome for an agent process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AgentProcessExit {
    pub(crate) exit_code: Option<i32>,
    /// True when termination came from a signal rather than a normal exit.
    #[serde(default)]
    pub(crate) signalled: bool,
}

/// One decoded chunk of process output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AgentOutput {
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
    Exit(AgentProcessExit),
}

/// Request payload for the exec and spawn endpoints. `argv` is evaluated
/// directly, keeping interpreter selection on the provider side.
#[derive(Debug, Serialize)]
pub(crate) struct AgentExecRequest {
    pub(crate) argv:        Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) working_dir: Option<String>,
    /// `KEY=VALUE` entries layered over the pod's environment.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(crate) env:         Vec<String>,
    /// Base64 process input delivered before the process output is read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) stdin_b64:   Option<String>,
}

impl AgentExecRequest {
    pub(crate) fn new(argv: Vec<String>) -> Self {
        Self {
            argv,
            working_dir: None,
            env: Vec::new(),
            stdin_b64: None,
        }
    }
}

/// Errors raised while talking to the agent.
#[derive(Debug, thiserror::Error)]
pub(crate) enum AgentClientError {
    #[error("sandbox agent stream closed before the process exited")]
    Closed,
    #[error("sandbox agent rejected the request with status {0}")]
    Status(u16),
    #[error("port-forward to the sandbox agent failed: {0}")]
    Transport(String),
    #[error("sandbox agent response could not be decoded: {0}")]
    Protocol(String),
}

#[derive(Clone)]
pub(crate) struct KubernetesAgentClient {
    pods:          Api<Pod>,
    pod_name:      String,
    port:          u16,
    token:         String,
    connect_limit: Duration,
}

/// A single open port-forward connection speaking one agent request.
struct AgentConnection {
    sender: http1::SendRequest<Full<Bytes>>,
    /// Keeps the port-forward duplex stream and its pump task alive for the
    /// life of the request.
    _pump:  JoinHandle<()>,
}

impl KubernetesAgentClient {
    pub(crate) fn new(
        pods: Api<Pod>,
        pod_name: String,
        port: u16,
        token: String,
        connect_limit: Duration,
    ) -> Self {
        Self {
            pods,
            pod_name,
            port,
            token,
            connect_limit,
        }
    }

    /// Perform a request and return the response with its streaming body.
    async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<HttpResponse<Incoming>, AgentClientError> {
        let mut connection =
            timeout(self.connect_limit, self.connect())
                .await
                .map_err(|_| {
                    AgentClientError::Transport(format!(
                        "timed out opening a port-forward to pod '{}' on port {}",
                        self.pod_name, self.port
                    ))
                })??;
        let payload = body.map_or_else(Bytes::new, |value| {
            Bytes::from(serde_json::to_vec(&value).expect("agent request body should serialize"))
        });
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header(header::AUTHORIZATION, format!("Bearer {}", self.token))
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::CONNECTION, "close")
            .body(Full::new(payload))
            .map_err(|err| AgentClientError::Protocol(err.to_string()))?;

        let response = connection
            .sender
            .send_request(request)
            .await
            .map_err(|err| AgentClientError::Transport(err.to_string()))?;
        if !response.status().is_success() {
            return Err(AgentClientError::Status(response.status().as_u16()));
        }
        Ok(response)
    }

    async fn connect(&self) -> Result<AgentConnection, AgentClientError> {
        let mut forwarder = self
            .pods
            .portforward(&self.pod_name, &[self.port])
            .await
            .map_err(|err| AgentClientError::Transport(err.to_string()))?;
        let stream = forwarder.take_stream(self.port).ok_or_else(|| {
            AgentClientError::Transport(format!(
                "port-forward did not provide a stream for port {}",
                self.port
            ))
        })?;
        // kube hands back a Tokio stream; hyper 1.x wants its own readiness
        // traits, so adapt it.
        let stream = TokioIo::new(stream);
        let (sender, connection) = http1::handshake(stream)
            .await
            .map_err(|err| AgentClientError::Transport(err.to_string()))?;
        // hyper requires the connection future to be driven while the
        // response body streams; the port-forward error channel is logged if
        // the kubelet tears the tunnel down.
        let pump = tokio::spawn(async move {
            if let Err(err) = connection.await {
                tracing::debug!(error = %err, "sandbox agent port-forward connection ended");
            }
        });
        Ok(AgentConnection {
            sender,
            _pump: pump,
        })
    }

    /// Run a command to completion, collecting its output.
    pub(crate) async fn exec(
        &self,
        request: AgentExecRequest,
    ) -> Result<AgentExecOutput, AgentClientError> {
        let response = self
            .send(
                Method::POST,
                &format!("/exec?v={AGENT_PROTOCOL_VERSION}"),
                Some(json_body(&request)?),
            )
            .await?;
        let mut events = AgentEventStream::new(response.into_body());
        let mut output = AgentExecOutput::default();
        while let Some(event) = events.next().await? {
            match event {
                AgentOutput::Stdout(chunk) => output.stdout.extend_from_slice(&chunk),
                AgentOutput::Stderr(chunk) => output.stderr.extend_from_slice(&chunk),
                AgentOutput::Exit(exit) => {
                    output.exit = exit;
                    return Ok(output);
                }
            }
        }
        Err(AgentClientError::Closed)
    }

    /// Spawn a long-lived process; the response body streams its output until
    /// it terminates. The agent-assigned process id arrives in the
    /// `X-Fabro-Process-Id` response header, before any output.
    pub(crate) async fn spawn(
        &self,
        request: AgentExecRequest,
    ) -> Result<(u64, AgentProcessStream), AgentClientError> {
        let response = self
            .send(
                Method::POST,
                &format!("/processes?v={AGENT_PROTOCOL_VERSION}"),
                Some(json_body(&request)?),
            )
            .await?;
        let process_id = response
            .headers()
            .get("x-fabro-process-id")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse().ok())
            .ok_or_else(|| {
                AgentClientError::Protocol(
                    "spawn response is missing X-Fabro-Process-Id".to_string(),
                )
            })?;
        Ok((process_id, AgentProcessStream::new(response.into_body())))
    }

    pub(crate) async fn write_stdin(
        &self,
        process_id: u64,
        bytes: &[u8],
        close: bool,
    ) -> Result<(), AgentClientError> {
        let payload = serde_json::json!({
            "data_b64": BASE64.encode(bytes),
            "close": close,
        });
        self.send(
            Method::POST,
            &format!("/processes/{process_id}/stdin"),
            Some(payload),
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn terminate(&self, process_id: u64) -> Result<(), AgentClientError> {
        self.send(
            Method::POST,
            &format!("/processes/{process_id}/terminate"),
            Some(serde_json::json!({})),
        )
        .await?;
        Ok(())
    }
}

fn json_body<T: Serialize>(value: &T) -> Result<serde_json::Value, AgentClientError> {
    serde_json::to_value(value).map_err(|err| AgentClientError::Protocol(err.to_string()))
}

/// Output of a completed exec.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct AgentExecOutput {
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
    pub(crate) exit:   AgentProcessExit,
}

impl AgentExecOutput {
    pub(crate) fn exit_code(&self) -> i32 {
        self.exit.exit_code.unwrap_or(-1)
    }

    pub(crate) fn stdout_lossy(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    pub(crate) fn stderr_lossy(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
}

/// Streaming view of a spawned agent process.
pub(crate) struct AgentProcessStream {
    events: AgentEventStream,
}

impl AgentProcessStream {
    fn new(body: Incoming) -> Self {
        Self {
            events: AgentEventStream::new(body),
        }
    }

    pub(crate) async fn next(&mut self) -> Result<Option<AgentOutput>, AgentClientError> {
        self.events.next().await
    }
}

/// Decodes the agent's JSON-lines response body into typed events.
struct AgentEventStream {
    body:   Incoming,
    buffer: Vec<u8>,
    eof:    bool,
}

impl AgentEventStream {
    /// A single JSON line carrying a chunk of process output can be large
    /// (file reads are shipped through this channel), but it must stay
    /// bounded so a broken agent cannot exhaust provider memory.
    const MAX_LINE_BYTES: usize = 16 * 1024 * 1024;

    fn new(body: Incoming) -> Self {
        Self {
            body,
            buffer: Vec::new(),
            eof: false,
        }
    }

    async fn next(&mut self) -> Result<Option<AgentOutput>, AgentClientError> {
        loop {
            if let Some(event) = self.decode_buffered_line()? {
                return Ok(Some(event));
            }
            if self.eof {
                return if self.buffer.is_empty() {
                    Ok(None)
                } else {
                    Err(AgentClientError::Protocol(
                        "truncated agent response".to_string(),
                    ))
                };
            }
            let frame = match self.body.frame().await {
                Some(Ok(frame)) => frame,
                Some(Err(err)) => return Err(AgentClientError::Transport(err.to_string())),
                None => {
                    self.eof = true;
                    continue;
                }
            };
            let bytes = frame.into_data().map_err(|_| {
                AgentClientError::Protocol("unexpected agent control frame".to_string())
            })?;
            self.buffer.extend_from_slice(&bytes);
            if self.buffer.len() > Self::MAX_LINE_BYTES {
                return Err(AgentClientError::Protocol(
                    "agent event line exceeded the size limit".to_string(),
                ));
            }
        }
    }

    fn decode_buffered_line(&mut self) -> Result<Option<AgentOutput>, AgentClientError> {
        let Some(newline) = self.buffer.iter().position(|byte| *byte == b'\n') else {
            return Ok(None);
        };
        let line: Vec<u8> = self.buffer.drain(..=newline).collect();
        let line = &line[..line.len() - 1];
        if line.is_empty() {
            return Ok(None);
        }
        decode_agent_event(line)
    }
}

#[derive(Deserialize)]
struct WireEvent {
    #[serde(default)]
    stream:    Option<String>,
    #[serde(default)]
    data_b64:  Option<String>,
    #[serde(default)]
    exit_code: Option<i32>,
    #[serde(default)]
    signalled: bool,
}

fn decode_agent_event(line: &[u8]) -> Result<Option<AgentOutput>, AgentClientError> {
    let event: WireEvent = serde_json::from_slice(line)
        .map_err(|err| AgentClientError::Protocol(format!("invalid agent event: {err}")))?;
    if event.exit_code.is_some() || event.signalled {
        return Ok(Some(AgentOutput::Exit(AgentProcessExit {
            exit_code: event.exit_code,
            signalled: event.signalled,
        })));
    }
    let stream = event.stream.ok_or_else(|| {
        AgentClientError::Protocol("agent event missing stream and exit".to_string())
    })?;
    let data = event.data_b64.unwrap_or_default();
    let bytes = BASE64
        .decode(data.as_bytes())
        .map_err(|err| AgentClientError::Protocol(format!("invalid base64 payload: {err}")))?;
    Ok(Some(match stream.as_str() {
        "stdout" => AgentOutput::Stdout(bytes),
        "stderr" => AgentOutput::Stderr(bytes),
        other => {
            return Err(AgentClientError::Protocol(format!(
                "unknown agent stream '{other}'"
            )));
        }
    }))
}
