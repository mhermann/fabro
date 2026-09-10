use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;

use super::authenticate;
use crate::server::SharedState;

pub async fn get_branch(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((owner, repo, branch)): Path<(String, String, String)>,
) -> Response {
    let Ok(_) = authenticate(&state, &headers).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let state = state.read().await;
    let Some(repository) = state.repos.get(&format!("{owner}/{repo}")) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(sha) = repository.branches.get(&branch) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    axum::Json(json!({
        "name": branch,
        "commit": {
            "id": sha,
            "message": "twin commit",
        },
        "protected": false,
        "required_approvals": 0,
        "enable_status_check": false,
    }))
    .into_response()
}
