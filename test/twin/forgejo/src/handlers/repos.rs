use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;

use super::users::check_auth;
use crate::server::SharedState;

/// GET /api/v1/repos/{owner}/{repo}
pub async fn get_repository(
    State(state): State<SharedState>,
    Path((owner, repo)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let state = state.read().await;
    if let Err(response) = check_auth(&headers, &state) {
        return response;
    }

    // A token without access to the repository sees the same 404 an unknown
    // repository gets.
    let Some(repository) = state.find_repository(&owner, &repo) else {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "message": "Not Found" })),
        )
            .into_response();
    };

    Json(json!({
        "full_name": format!("{owner}/{repo}"),
        "private": repository.private,
        "fork": false,
    }))
    .into_response()
}
