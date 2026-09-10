use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use serde_json::json;

use super::authenticate;
use crate::server::SharedState;

pub async fn get_version(State(state): State<SharedState>, headers: HeaderMap) -> Response {
    let Err(response) = authenticate(&state, &headers).await else {
        let state = state.read().await;
        return axum::Json(json!({ "version": state.version })).into_response();
    };
    response
}
