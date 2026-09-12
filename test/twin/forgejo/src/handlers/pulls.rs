use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use hmac::Mac as _;
use serde::Deserialize;
use serde_json::json;

use super::users::check_auth;
use crate::server::SharedState;
use crate::state::{AutoMerge, PullRequest};

fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "message": "Not Found" })),
    )
        .into_response()
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn pr_to_json(pr: &PullRequest) -> serde_json::Value {
    let mut body = json!({
        "number": pr.index,
        "html_url": pr.html_url,
        "title": pr.title,
        "body": pr.body,
        "state": pr.state,
        "draft": pr.draft,
        "merged": pr.merged,
        "merged_at": pr.merged_at,
        "head": { "ref": pr.head_ref, "sha": pr.head_sha },
        "base": { "ref": pr.base_ref },
        "user": { "login": pr.user_login },
        "created_at": pr.created_at,
        "updated_at": pr.updated_at,
    });

    if let Some(auto_merge) = &pr.auto_merge {
        body["auto_merge"] = json!({
            "merge_method": auto_merge.merge_method,
            "enabled_at": auto_merge.enabled_at,
        });
    }

    body
}

/// GET /api/v1/repos/{owner}/{repo}/pulls?state=open
///
/// Gitea-lineage list endpoints filter by `state` only; head matching (by
/// `head.ref`/`head.sha`) happens client-side, exactly as the fabro-forgejo
/// client does.
#[derive(Deserialize)]
pub struct ListQuery {
    state: Option<String>,
}

pub async fn list_pull_requests(
    State(state): State<SharedState>,
    Path((owner, repo)): Path<(String, String)>,
    Query(query): Query<ListQuery>,
    headers: HeaderMap,
) -> Response {
    let state = state.read().await;
    if let Err(response) = check_auth(&headers, &state) {
        return response;
    }

    if state.find_repository(&owner, &repo).is_none() {
        return not_found();
    }

    let wanted = query.state.unwrap_or_else(|| "open".to_string());
    let pulls = state
        .pull_requests
        .get(&(owner, repo))
        .map(|pulls| {
            pulls
                .iter()
                .filter(|pr| wanted == "all" || pr.state == wanted)
                .map(pr_to_json)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Json(pulls).into_response()
}

/// POST /api/v1/repos/{owner}/{repo}/pulls
pub async fn create_pull_request(
    State(state): State<SharedState>,
    Path((owner, repo)): Path<(String, String)>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let mut state = state.write().await;
    if let Err(response) = check_auth(&headers, &state) {
        return response;
    }

    if state.find_repository(&owner, &repo).is_none() {
        return not_found();
    }

    let title = string_field(&body, "title");
    let head = string_field(&body, "head");
    let base = string_field(&body, "base");
    let pr_body = string_field(&body, "body");
    let draft = body.get("draft").and_then(|v| v.as_bool()).unwrap_or(false);

    // Forgejo resolves the head branch to its current commit; unknown heads
    // get a stand-in SHA so synthetic fixtures still work.
    let head_sha = state.branch_head_sha(&owner, &repo, &head).map_or_else(
        || synthetic_head_sha(&owner, &repo, &head),
        |sha| sha.to_string(),
    );

    let index = state.next_pr_index;
    state.next_pr_index += 1;
    let timestamp = now();
    let html_url = format!("{}/{owner}/{repo}/pulls/{index}", state.base_url);

    let pr = PullRequest {
        index,
        title: title.clone(),
        body: pr_body,
        state: "open".to_string(),
        draft,
        merged: false,
        merged_at: None,
        mergeable: true,
        html_url: html_url.clone(),
        user_login: crate::state::FIXTURE_BOT_LOGIN.to_string(),
        head_ref: head.clone(),
        head_sha: head_sha.clone(),
        base_ref: base.clone(),
        created_at: timestamp.clone(),
        updated_at: timestamp.clone(),
        auto_merge: None,
    };

    state
        .pull_requests
        .entry((owner.clone(), repo.clone()))
        .or_default()
        .push(pr);

    (
        StatusCode::CREATED,
        Json(json!({
            "number": index,
            "html_url": html_url,
            "title": title,
            "state": "open",
            "head": { "ref": head, "sha": head_sha },
            "base": { "ref": base },
            "user": { "login": crate::state::FIXTURE_BOT_LOGIN },
            "created_at": timestamp,
            "updated_at": timestamp,
        })),
    )
        .into_response()
}

/// GET /api/v1/repos/{owner}/{repo}/pulls/{index}
pub async fn get_pull_request(
    State(state): State<SharedState>,
    Path((owner, repo, index)): Path<(String, String, u64)>,
    headers: HeaderMap,
) -> Response {
    let state = state.read().await;
    if let Err(response) = check_auth(&headers, &state) {
        return response;
    }

    match state.find_pull_request(&owner, &repo, index) {
        Some(pr) => Json(pr_to_json(pr)).into_response(),
        None => not_found(),
    }
}

/// PATCH /api/v1/repos/{owner}/{repo}/pulls/{index}
///
/// Supports `{"state": "closed"}` (and `"open"` to reopen), plus `title` and
/// `body` edits.
pub async fn update_pull_request(
    State(state): State<SharedState>,
    Path((owner, repo, index)): Path<(String, String, u64)>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let mut state = state.write().await;
    if let Err(response) = check_auth(&headers, &state) {
        return response;
    }

    let Some(pulls) = state.pull_requests_mut(&owner, &repo) else {
        return not_found();
    };
    let Some(pr) = pulls.iter_mut().find(|pr| pr.index == index) else {
        return not_found();
    };

    if let Some(new_state) = body.get("state").and_then(|v| v.as_str()) {
        if new_state == "open" {
            pr.auto_merge = None;
        }
        pr.state = new_state.to_string();
    }
    if let Some(new_title) = body.get("title").and_then(|v| v.as_str()) {
        pr.title = new_title.to_string();
    }
    if let Some(new_body) = body.get("body").and_then(|v| v.as_str()) {
        pr.body = new_body.to_string();
    }
    pr.updated_at = now();

    Json(pr_to_json(pr)).into_response()
}

/// POST /api/v1/repos/{owner}/{repo}/pulls/{index}/merge
///
/// Body: `{"Do": "merge"|"squash"|"rebase", "merge_when_checks_succeed":
/// optional}`. With `merge_when_checks_succeed` the request schedules an
/// auto-merge and the PR stays open; otherwise the PR merges immediately.
/// Success is `200` (Forgejo returns an empty body), a PR that cannot merge
/// is `405`, and a missing PR is `404`.
pub async fn merge_pull_request(
    State(state): State<SharedState>,
    Path((owner, repo, index)): Path<(String, String, u64)>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let mut state = state.write().await;
    if let Err(response) = check_auth(&headers, &state) {
        return response;
    }

    if state.find_pull_request(&owner, &repo, index).is_none() {
        return not_found();
    }

    // Keep the verbatim body around for test assertions.
    state.record_merge_request(&owner, &repo, index, body.clone());

    let do_method = body
        .get("Do")
        .and_then(|v| v.as_str())
        .unwrap_or("merge")
        .to_string();
    let when_checks_succeed = body
        .get("merge_when_checks_succeed")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let Some(pulls) = state.pull_requests_mut(&owner, &repo) else {
        return not_found();
    };
    let Some(pr) = pulls.iter_mut().find(|pr| pr.index == index) else {
        return not_found();
    };

    if pr.state != "open" || pr.merged || !pr.mergeable {
        return (
            StatusCode::METHOD_NOT_ALLOWED,
            Json(json!({ "message": "Pull request is not mergeable" })),
        )
            .into_response();
    }

    if when_checks_succeed {
        pr.auto_merge = Some(AutoMerge {
            merge_method: do_method,
            enabled_at:   now(),
        });
        pr.updated_at = now();
        return StatusCode::OK.into_response();
    }

    pr.state = "closed".to_string();
    pr.merged = true;
    pr.merged_at = Some(now());
    pr.updated_at = now();
    StatusCode::OK.into_response()
}

/// GET /__admin/merge-bodies — every merge request body the twin received,
/// in arrival order. Unauthenticated (loopback test harness only).
pub async fn merge_bodies(State(state): State<SharedState>) -> Response {
    let state = state.read().await;
    Json(&state.merge_requests).into_response()
}

fn string_field(body: &serde_json::Value, key: &str) -> String {
    body.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn synthetic_head_sha(owner: &str, repo: &str, head: &str) -> String {
    let mut mac = hmac::Hmac::<sha2::Sha256>::new_from_slice(head.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(format!("{owner}/{repo}/{head}").as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

#[cfg(test)]
mod tests {
    use crate::server::TestServer;
    use crate::state::AppState;
    use crate::test_support::test_http_client;

    fn fixture_state() -> AppState {
        let mut state = AppState::new();
        state.add_repository(
            "owner",
            "repo",
            vec!["main".to_string(), "feature".to_string()],
            false,
        );
        state
    }

    async fn create_pr(server: &TestServer, token: &str, head: &str) -> serde_json::Value {
        let client = test_http_client();
        let resp = client
            .post(format!("{}/api/v1/repos/owner/repo/pulls", server.url()))
            .header("Authorization", format!("token {token}"))
            .json(&serde_json::json!({
                "title": "Test PR",
                "head": head,
                "base": "main",
                "body": "PR body",
                "draft": false,
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 201);
        resp.json().await.unwrap()
    }

    #[tokio::test]
    async fn create_pr_returns_201_with_forgejo_url_form() {
        let state = fixture_state();
        let server = TestServer::start(state).await;

        let created = create_pr(&server, server.token(), "feature").await;
        assert_eq!(created["number"], 1);
        assert_eq!(
            created["html_url"],
            format!("{}/owner/repo/pulls/1", server.url())
        );
        assert_eq!(created["state"], "open");
        assert_eq!(created["head"]["ref"], "feature");
        assert_eq!(
            created["head"]["sha"],
            server_url_branch_sha(&server, "feature").await
        );
        assert_eq!(created["base"]["ref"], "main");
        assert_eq!(created["user"]["login"], "twin-bot[bot]");

        server.shutdown().await;
    }

    async fn server_url_branch_sha(server: &TestServer, branch: &str) -> String {
        let client = test_http_client();
        let resp = client
            .get(format!(
                "{}/api/v1/repos/owner/repo/branches/{branch}",
                server.url()
            ))
            .header("Authorization", format!("token {}", server.token()))
            .send()
            .await
            .unwrap();
        let body: serde_json::Value = resp.json().await.unwrap();
        body["commit"]["id"].as_str().unwrap().to_string()
    }

    #[tokio::test]
    async fn list_pull_requests_filters_open_state() {
        let state = fixture_state();
        let server = TestServer::start(state).await;
        create_pr(&server, server.token(), "feature").await;

        // Close PR 1 via PATCH, then confirm state filters.
        let client = test_http_client();
        let resp = client
            .patch(format!("{}/api/v1/repos/owner/repo/pulls/1", server.url()))
            .header("Authorization", format!("token {}", server.token()))
            .json(&serde_json::json!({ "state": "closed" }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let closed: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(closed["state"], "closed");

        let open: Vec<serde_json::Value> = test_http_client()
            .get(format!(
                "{}/api/v1/repos/owner/repo/pulls?state=open",
                server.url()
            ))
            .header("Authorization", format!("token {}", server.token()))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert!(open.is_empty());

        let all: Vec<serde_json::Value> = test_http_client()
            .get(format!(
                "{}/api/v1/repos/owner/repo/pulls?state=all",
                server.url()
            ))
            .header("Authorization", format!("token {}", server.token()))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(all.len(), 1);

        server.shutdown().await;
    }

    #[tokio::test]
    async fn merge_open_pr_returns_200_and_marks_merged() {
        let state = fixture_state();
        let server = TestServer::start(state).await;
        create_pr(&server, server.token(), "feature").await;

        let client = test_http_client();
        let resp = client
            .post(format!(
                "{}/api/v1/repos/owner/repo/pulls/1/merge",
                server.url()
            ))
            .header("Authorization", format!("token {}", server.token()))
            .json(&serde_json::json!({ "Do": "squash" }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);

        let pr: serde_json::Value = client
            .get(format!("{}/api/v1/repos/owner/repo/pulls/1", server.url()))
            .header("Authorization", format!("token {}", server.token()))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(pr["state"], "closed");
        assert_eq!(pr["merged"], true);
        assert!(pr["merged_at"].is_string());

        server.shutdown().await;
    }

    #[tokio::test]
    async fn merge_closed_pr_returns_405_and_missing_returns_404() {
        let state = fixture_state();
        let server = TestServer::start(state).await;
        create_pr(&server, server.token(), "feature").await;

        let client = test_http_client();
        // Close PR 1 first, so it is no longer mergeable.
        client
            .patch(format!("{}/api/v1/repos/owner/repo/pulls/1", server.url()))
            .header("Authorization", format!("token {}", server.token()))
            .json(&serde_json::json!({ "state": "closed" }))
            .send()
            .await
            .unwrap();

        let resp = client
            .post(format!(
                "{}/api/v1/repos/owner/repo/pulls/1/merge",
                server.url()
            ))
            .header("Authorization", format!("token {}", server.token()))
            .json(&serde_json::json!({ "Do": "merge" }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 405);

        let resp = client
            .post(format!(
                "{}/api/v1/repos/owner/repo/pulls/999/merge",
                server.url()
            ))
            .header("Authorization", format!("token {}", server.token()))
            .json(&serde_json::json!({ "Do": "merge" }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 404);

        server.shutdown().await;
    }

    #[tokio::test]
    async fn auto_merge_keeps_pr_open_and_records_auto_merge() {
        let state = fixture_state();
        let server = TestServer::start(state).await;
        create_pr(&server, server.token(), "feature").await;

        let client = test_http_client();
        let resp = client
            .post(format!(
                "{}/api/v1/repos/owner/repo/pulls/1/merge",
                server.url()
            ))
            .header("Authorization", format!("token {}", server.token()))
            .json(&serde_json::json!({
                "Do": "squash",
                "merge_when_checks_succeed": true,
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);

        let pr: serde_json::Value = client
            .get(format!("{}/api/v1/repos/owner/repo/pulls/1", server.url()))
            .header("Authorization", format!("token {}", server.token()))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(pr["state"], "open");
        assert_eq!(pr["merged"], false);
        assert_eq!(pr["auto_merge"]["merge_method"], "squash");

        server.shutdown().await;
    }
}
