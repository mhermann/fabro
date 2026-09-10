use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use serde_json::json;

use super::authenticate;
use crate::server::SharedState;

pub async fn get_repository(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((owner, repo)): Path<(String, String)>,
) -> Response {
    let Ok(subject) = authenticate(&state, &headers).await else {
        return axum::http::StatusCode::UNAUTHORIZED.into_response();
    };

    let state = state.read().await;
    let Some(repository) = state.repos.get(&format!("{owner}/{repo}")) else {
        return axum::http::StatusCode::NOT_FOUND.into_response();
    };

    axum::Json(json!({
        "id": 1,
        "owner": { "id": 1, "login": repository.owner },
        "name": repository.name,
        "full_name": format!("{}/{}", repository.owner, repository.name),
        "private": repository.private,
        "empty": repository.branches.is_empty(),
        "default_branch": repository.default_branch,
        "clone_url": format!("https://forgejo.example.com/{}/{}.git", repository.owner, repository.name),
        "permissions": {
            "admin": false,
            "push": true,
            "pull": true,
        },
        "full_user_link": format!("https://forgejo.example.com/{}", subject.username),
    }))
    .into_response()
}
