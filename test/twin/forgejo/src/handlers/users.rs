use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;

use super::authenticate;
use crate::server::SharedState;

pub async fn get_user(State(state): State<SharedState>, headers: HeaderMap) -> Response {
    let Ok(subject) = authenticate(&state, &headers).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    axum::Json(json!({
        "id": 1,
        "login": subject.username,
        "full_name": subject.username,
        "is_admin": false,
    }))
    .into_response()
}
