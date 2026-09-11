use hmac::{Hmac, Mac};
use sha2::Sha256;

/// Compute the HMAC-SHA256 hex signature of `body` with `secret` — the value
/// a Forgejo instance places in `X-Forgejo-Signature` / `X-Gitea-Signature`
/// (and in `X-Hub-Signature-256` with a `sha256=` prefix) when delivering a
/// webhook. Tests use this to sign deliveries the same way.
#[must_use]
pub fn webhook_signature(secret: &str, body: &[u8]) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(body);
    hex::encode(mac.finalize().into_bytes())
}

#[cfg(test)]
pub fn test_http_client() -> fabro_http::HttpClient {
    fabro_http::test_http_client().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_is_hex_and_deterministic() {
        let first = webhook_signature("secret", b"hello world");
        let second = webhook_signature("secret", b"hello world");
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
        assert!(first.chars().all(|c| c.is_ascii_hexdigit()));

        let other = webhook_signature("other-secret", b"hello world");
        assert_ne!(first, other);
    }

    /// The twin's helper must agree with the server-side verifier in
    /// `fabro-server/src/forgejo_webhooks.rs`.
    #[test]
    fn signature_round_trips_through_verifier() {
        // Inlined from forgejo_webhooks to keep the twin dependency-free.
        fn verify(secret: &[u8], body: &[u8], signature_header: &str) -> bool {
            let hex_digest = signature_header
                .strip_prefix("sha256=")
                .unwrap_or(signature_header);
            let Ok(expected) = hex::decode(hex_digest) else {
                return false;
            };
            let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret) else {
                return false;
            };
            mac.update(body);
            mac.verify_slice(&expected).is_ok()
        }

        let body = br#"{"action":"opened"}"#;
        let signature = webhook_signature("webhook-secret", body);
        assert!(verify(b"webhook-secret", body, &signature));
        assert!(verify(
            b"webhook-secret",
            body,
            &format!("sha256={signature}")
        ));
        assert!(!verify(b"wrong-secret", body, &signature));
    }
}
