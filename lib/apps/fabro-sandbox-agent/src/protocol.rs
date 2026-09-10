//! Wire protocol shared between the provider and the in-pod sandbox agent.
//!
//! Kept dependency-light: the agent binary ships inside a distroless image,
//! and the provider decodes the same shapes on its side of the port-forward.

use serde::{Deserialize, Serialize};

/// Terminal event sent as the last JSON line of a process response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ProcessExit {
    pub(crate) exit_code: Option<i32>,
    #[serde(default)]
    pub(crate) signalled: bool,
}

/// One streamed chunk of process output. Payloads are base64 so binary
/// output survives JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct OutputChunk {
    pub(crate) stream:   OutputStream,
    pub(crate) data_b64: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum OutputStream {
    Stdout,
    Stderr,
}

/// Request payload for exec and spawn. `argv` is evaluated directly; the
/// caller owns interpreter selection (the provider always sends
/// `/bin/bash -c <command>`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ExecRequest {
    pub(crate) argv:        Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) working_dir: Option<String>,
    /// `KEY=VALUE` entries layered over the pod environment.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) env:         Vec<String>,
    /// Base64 input delivered to the process before its output is read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) stdin_b64:   Option<String>,
}

/// `KEY=VALUE` entry passed through to the spawned process.
pub(crate) fn split_env_entry(entry: &str) -> (&str, &str) {
    entry.split_once('=').unwrap_or((entry, ""))
}
