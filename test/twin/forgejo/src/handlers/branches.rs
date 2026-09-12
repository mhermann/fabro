use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;

use super::users::check_auth;
use crate::server::SharedState;

/// GET /api/v1/repos/{owner}/{repo}/branches/{branch}
///
/// Gitea-lineage servers name the commit field `id` (not GitHub's `sha`).
pub async fn get_branch(
    State(state): State<SharedState>,
    Path((owner, repo, branch)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> Response {
    let state = state.read().await;
    if let Err(response) = check_auth(&headers, &state) {
        return response;
    }

    if state.find_repository(&owner, &repo).is_none() {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "message": "Not Found" })),
        )
            .into_response();
    }

    let Some(sha) = state.branch_head_sha(&owner, &repo, &branch) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "message": "Branch not found" })),
        )
            .into_response();
    };

    Json(json!({
        "name": branch,
        "commit": {
            "id": sha,
            "message": format!("initial commit on {branch}"),
        },
    }))
    .into_response()
}
