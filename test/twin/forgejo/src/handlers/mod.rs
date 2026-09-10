pub mod branches;
pub mod pulls;
pub mod repos;
pub mod users;
pub mod version;

use axum::Router;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};

use crate::server::SharedState;
use crate::state::TokenSubject;

pub fn build_router(state: SharedState) -> Router {
    Router::new()
        .route("/api/v1/version", get(version::get_version))
        .route("/api/v1/user", get(users::get_user))
        .route("/api/v1/repos/{owner}/{repo}", get(repos::get_repository))
        .route(
            // Wildcard: branch names contain slashes (`fabro/run/<id>`).
            "/api/v1/repos/{owner}/{repo}/branches/{*branch}",
            get(branches::get_branch),
        )
        .route(
            "/api/v1/repos/{owner}/{repo}/pulls",
            get(pulls::list_pull_requests).post(pulls::create_pull_request),
        )
        .route(
            "/api/v1/repos/{owner}/{repo}/pulls/{number}",
            get(pulls::get_pull_request).patch(pulls::update_pull_request),
        )
        .route(
            "/api/v1/repos/{owner}/{repo}/pulls/{number}/merge",
            post(pulls::merge_pull_request),
        )
        .with_state(state)
}

/// The `Authorization: token X` (or `Bearer X`) subject for a request.
pub(crate) async fn authenticate(
    state: &SharedState,
    headers: &HeaderMap,
) -> Result<TokenSubject, Response> {
    let raw = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value
                .strip_prefix("token ")
                .or_else(|| value.strip_prefix("Bearer "))
        });
    let Some(token) = raw else {
        return Err(StatusCode::UNAUTHORIZED.into_response());
    };
    let state = state.read().await;
    state
        .tokens
        .get(token)
        .cloned()
        .ok_or_else(|| StatusCode::UNAUTHORIZED.into_response())
}

/// Whether a title marks the pull request as a draft, mirroring the
/// instance's default work-in-progress prefixes.
pub(crate) fn title_is_draft(title: &str) -> bool {
    ["WIP:", "[WIP]", "Draft:", "[DRAFT]", "DRAFT:"]
        .iter()
        .any(|prefix| title.starts_with(prefix))
}
