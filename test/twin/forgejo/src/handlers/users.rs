use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::auth::{TokenError, authorize};
use crate::server::SharedState;

fn unauthorized(error: TokenError) -> Response {
    let message = match error {
        TokenError::Missing => "a valid token is required",
        TokenError::Invalid => "invalid token",
    };
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "message": message })),
    )
        .into_response()
}

/// Authorize the request or build its 401 response.
#[expect(
    clippy::result_large_err,
    reason = "Axum handlers return Response directly; the twin mirrors the crate's handler shape."
)]
pub(crate) fn check_auth(
    headers: &HeaderMap,
    state: &crate::state::AppState,
) -> Result<(), Response> {
    authorize(headers, state).map_err(unauthorized)
}

/// GET /api/v1/user
pub async fn get_user(State(state): State<SharedState>, headers: HeaderMap) -> Response {
    let state = state.read().await;
    if let Err(response) = check_auth(&headers, &state) {
        return response;
    }

    axum::Json(json!({ "login": state.user_login })).into_response()
}
