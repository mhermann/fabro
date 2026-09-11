//! Token authentication for the fake Forgejo API routes.

use axum::http::{HeaderMap, header};

use crate::state::AppState;

/// Extract the PAT from an `Authorization: token <pat>` header, the spelling
/// Gitea-lineage servers document for PAT auth.
pub fn token_from_headers(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    value.strip_prefix("token ")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenError {
    Missing,
    Invalid,
}

/// Validate the request's PAT against the fixture token.
pub fn authorize(headers: &HeaderMap, state: &AppState) -> Result<(), TokenError> {
    let token = token_from_headers(headers).ok_or(TokenError::Missing)?;
    if token == state.token {
        Ok(())
    } else {
        Err(TokenError::Invalid)
    }
}
