use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use bytes::Bytes;
use fabro_static::EnvVars;
use fabro_types::Principal;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use tracing::{debug, info, warn};

use crate::principal_middleware::{RequestAuth, RequestAuthContext};

type HmacSha256 = Hmac<Sha256>;

/// Name of the server secret holding the Forgejo webhook HMAC key.
pub(crate) const FORGEJO_WEBHOOK_SECRET_ENV: &str = EnvVars::FORGEJO_WEBHOOK_SECRET;

/// Route path where Fabro receives Forgejo webhook deliveries.
pub(crate) const FORGEJO_WEBHOOK_ROUTE: &str = "/api/v1/webhooks/forgejo";

/// Verify a Forgejo webhook HMAC-SHA256 signature.
///
/// Forgejo (and Gitea) send the raw hex digest in `X-Forgejo-Signature` /
/// `X-Gitea-Signature`; the GitHub-compatible spellings (`X-Hub-Signature-256`
/// with a `sha256=` prefix, `X-GitHub-Event`) are accepted too because
/// instances can emit them. All are HMAC-SHA256 over the raw body.
pub(crate) fn verify_signature(secret: &[u8], body: &[u8], signature_header: &str) -> bool {
    let hex_digest = signature_header
        .strip_prefix("sha256=")
        .unwrap_or(signature_header);

    let Ok(expected) = hex::decode(hex_digest) else {
        return false;
    };

    let Ok(mut mac) = HmacSha256::new_from_slice(secret) else {
        return false;
    };
    mac.update(body);
    mac.verify_slice(&expected).is_ok()
}

/// Read the delivery's signature from any accepted header spelling.
fn signature_from_headers(headers: &HeaderMap) -> Option<&str> {
    for name in [
        "x-forgejo-signature",
        "x-gitea-signature",
        "x-hub-signature-256",
    ] {
        if let Some(value) = headers.get(name).and_then(|value| value.to_str().ok()) {
            return Some(value);
        }
    }
    None
}

/// Read the delivery's event type from any accepted header spelling.
fn event_from_headers(headers: &HeaderMap) -> &str {
    for name in ["x-forgejo-event", "x-gitea-event", "x-github-event"] {
        if let Some(value) = headers.get(name).and_then(|value| value.to_str().ok()) {
            return value;
        }
    }
    "unknown"
}

/// Receive a Forgejo webhook delivery.
///
/// Inert by decision: the delivery is verified and logged, nothing is
/// triggered. Verification gates the `Principal::Webhook` auth slot exactly
/// like the GitHub route.
async fn forgejo_webhook(
    State(secret): State<Arc<[u8]>>,
    RequestAuth(auth_slot): RequestAuth,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    let delivery_id = headers
        .get("x-forgejo-delivery")
        .or_else(|| headers.get("x-gitea-delivery"))
        .or_else(|| headers.get("x-github-delivery"))
        .and_then(|value| value.to_str().ok())
        .unwrap_or("unknown");

    let Some(signature) = signature_from_headers(&headers) else {
        auth_slot.replace(RequestAuthContext::invalid());
        warn!(delivery = %delivery_id, "Forgejo webhook missing a signature header");
        return StatusCode::UNAUTHORIZED;
    };

    if !verify_signature(&secret, &body, signature) {
        auth_slot.replace(RequestAuthContext::invalid());
        warn!(delivery = %delivery_id, "Forgejo webhook HMAC signature mismatch");
        return StatusCode::UNAUTHORIZED;
    }

    auth_slot.replace(RequestAuthContext::authenticated(
        Principal::Webhook {
            delivery_id: delivery_id.to_string(),
        },
        None,
    ));

    let event_type = event_from_headers(&headers);

    if tracing::enabled!(tracing::Level::DEBUG) {
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap_or_default();
        debug!(
            event = %event_type,
            delivery = %delivery_id,
            repo = %parsed
                .get("repository")
                .and_then(|r| r.get("full_name"))
                .and_then(|n| n.as_str())
                .unwrap_or("unknown"),
            action = %parsed
                .get("action")
                .and_then(|a| a.as_str())
                .unwrap_or("none"),
            "Forgejo webhook received"
        );
    } else {
        info!(
            event = %event_type,
            delivery = %delivery_id,
            "Forgejo webhook received"
        );
    }

    StatusCode::OK
}

/// The Forgejo webhook routes, mounted only when a webhook secret exists.
pub(crate) fn forgejo_webhook_routes(secret: Arc<[u8]>) -> axum::Router {
    use axum::routing::post as axum_post;
    axum::Router::new()
        .route(FORGEJO_WEBHOOK_ROUTE, axum_post(forgejo_webhook))
        .with_state(secret)
}

#[cfg(test)]
pub(crate) fn compute_signature(secret: &[u8], body: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret).unwrap();
    mac.update(body);
    hex::encode(mac.finalize().into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_raw_hex_signature() {
        let secret = b"test-secret";
        let body = b"hello world";
        let sig = compute_signature(secret, body);
        assert!(verify_signature(secret, body, &sig));
    }

    #[test]
    fn valid_sha256_prefixed_signature() {
        let secret = b"test-secret";
        let body = b"hello world";
        let sig = format!("sha256={}", compute_signature(secret, body));
        assert!(verify_signature(secret, body, &sig));
    }

    #[test]
    fn wrong_signature() {
        let sig = compute_signature(b"wrong-secret", b"hello world");
        assert!(!verify_signature(b"test-secret", b"hello world", &sig));
    }

    #[test]
    fn invalid_hex_is_rejected() {
        assert!(!verify_signature(b"test-secret", b"body", "zz-not-hex"));
    }

    #[test]
    fn empty_body_valid_signature() {
        let sig = compute_signature(b"test-secret", b"");
        assert!(verify_signature(b"test-secret", b"", &sig));
    }
}
