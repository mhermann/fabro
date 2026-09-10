//! Fake Forgejo/Gitea API server for local black-box testing.
//!
//! Mirrors the shape of `twin-github`: an in-memory [`state::AppState`]
//! served over an ephemeral-port axum server. Implements the REST surface
//! fabro's Forgejo/Gitea client uses — version, user, repository, branch,
//! and pull-request endpoints — under the `/api/v1` prefix.

#![allow(
    clippy::absolute_paths,
    clippy::manual_let_else,
    clippy::redundant_closure_for_method_calls,
    clippy::redundant_else,
    reason = "This twin Forgejo harness prefers explicit fixture modules over pedantic style lints."
)]

pub mod handlers;
pub mod server;
pub mod state;

pub use server::TestServer;
pub use state::AppState;
