//! Secret handling for Forgejo credentials.
//!
//! Forgejo/Gitea v1 credentials are static personal access tokens: no mint
//! step, no expiry, and therefore no token-source machinery like GitHub
//! installation tokens. Only the redacting [`SecretString`] wrapper is
//! needed.

use std::fmt;

/// A token secret that never appears in `Debug` output. Call
/// [`SecretString::expose`] at the point of use (URL embedding, git
/// credentials) — never in a log line.
#[derive(Clone)]
pub struct SecretString(String);

impl SecretString {
    #[must_use]
    pub fn new(secret: String) -> Self {
        Self(secret)
    }

    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretString(<redacted>)")
    }
}
