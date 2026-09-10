use std::sync::Arc;

use axum::Router;
use tokio::net::TcpListener;
use tokio::sync::RwLock;

use crate::handlers;
use crate::state::AppState;

pub type SharedState = Arc<RwLock<AppState>>;

/// A running test server instance.
pub struct TestServer {
    url:         String,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    handle:      Option<tokio::task::JoinHandle<()>>,
}

impl TestServer {
    /// Start the fake Forgejo/Gitea API server on a random port.
    pub async fn start(state: AppState) -> Self {
        let shared_state: SharedState = Arc::new(RwLock::new(state));

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("test server should bind an ephemeral port");
        let port = listener
            .local_addr()
            .expect("bound test server should have a local address")
            .port();
        let url = format!("http://127.0.0.1:{port}");
        shared_state.write().await.base_url = Some(url.clone());

        let router = build_router(shared_state);

        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

        let handle = tokio::spawn(async move {
            axum::serve(listener, router)
                .with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                })
                .await
                .ok();
        });

        Self {
            url,
            shutdown_tx: Some(shutdown_tx),
            handle: Some(handle),
        }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.await;
        }
    }
}

pub fn build_router(state: SharedState) -> Router {
    handlers::build_router(state)
}

#[cfg(test)]
mod tests {
    use crate::state::PullRequest;
    use crate::{AppState, TestServer};

    fn test_state() -> AppState {
        let mut state = AppState::new();
        state.register_token("twin-token", "octocat");
        state.add_repository(
            "acme",
            "widgets",
            vec![
                ("main", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
                ("feature", "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
            ],
            "main",
            false,
        );
        state
    }

    #[tokio::test]
    async fn version_requires_a_registered_token() {
        let server = TestServer::start(test_state()).await;
        let client = fabro_http::test_http_client().unwrap();

        let unauthorized = client
            .get(format!("{}/api/v1/version", server.url()))
            .send()
            .await
            .unwrap();
        assert_eq!(unauthorized.status(), 401);

        let ok = client
            .get(format!("{}/api/v1/version", server.url()))
            .header("Authorization", "token twin-token")
            .send()
            .await
            .unwrap();
        assert_eq!(ok.status(), 200);
        let body: serde_json::Value = ok.json().await.unwrap();
        assert!(body["version"].as_str().is_some());
        server.shutdown().await;
    }

    #[tokio::test]
    async fn pull_request_lifecycle_round_trips() {
        let server = TestServer::start(test_state()).await;
        let client = fabro_http::test_http_client().unwrap();
        let base = format!("{}/api/v1/repos/acme/widgets", server.url());
        let auth = ("Authorization", "token twin-token");

        // branch head
        let branch = client
            .get(format!("{base}/branches/feature"))
            .header(auth.0, auth.1)
            .send()
            .await
            .unwrap();
        assert_eq!(branch.status(), 200);

        // create
        let created = client
            .post(format!("{base}/pulls"))
            .header(auth.0, auth.1)
            .json(&serde_json::json!({
                "title": "WIP: add widgets",
                "body": "body",
                "base": "main",
                "head": "feature",
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(created.status(), 201);
        let body: serde_json::Value = created.json().await.unwrap();
        let number = body["number"].as_u64().unwrap();
        assert_eq!(
            body["head"]["sha"],
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        );
        assert_eq!(body["draft"], true);

        // list open
        let listed = client
            .get(format!("{base}/pulls?state=open"))
            .header(auth.0, auth.1)
            .send()
            .await
            .unwrap();
        assert_eq!(listed.status(), 200);
        let items: serde_json::Value = listed.json().await.unwrap();
        assert_eq!(items.as_array().unwrap().len(), 1);

        // close (edit) answers 201
        let closed = client
            .patch(format!("{base}/pulls/{number}"))
            .header(auth.0, auth.1)
            .json(&serde_json::json!({ "state": "closed" }))
            .send()
            .await
            .unwrap();
        assert_eq!(closed.status(), 201);

        // merge of a closed PR is rejected with 405
        let merged = client
            .post(format!("{base}/pulls/{number}/merge"))
            .header(auth.0, auth.1)
            .json(&serde_json::json!({ "Do": "merge" }))
            .send()
            .await
            .unwrap();
        assert_eq!(merged.status(), 405);
        let _ = PullRequest {
            number:     0,
            title:      String::new(),
            body:       String::new(),
            state:      String::new(),
            merged:     false,
            mergeable:  false,
            user_login: String::new(),
            head_ref:   String::new(),
            head_sha:   String::new(),
            base_ref:   String::new(),
            created_at: String::new(),
            updated_at: String::new(),
        };
        server.shutdown().await;
    }
}
