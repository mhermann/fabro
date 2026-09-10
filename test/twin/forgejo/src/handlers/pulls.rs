use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{authenticate, title_is_draft};
use crate::server::SharedState;
use crate::state::PullRequest;

const CREATED_AT: &str = "2026-09-10T00:00:00Z";
const UPDATED_AT: &str = "2026-09-10T00:00:00Z";

#[derive(Deserialize, Default)]
pub struct ListQuery {
    state: Option<String>,
    limit: Option<u64>,
    page:  Option<u64>,
}

fn pull_json(
    base_url: &str,
    owner: &str,
    repo: &str,
    pull: &PullRequest,
    head_sha: Option<&str>,
) -> Value {
    let head_sha = head_sha.unwrap_or(&pull.head_sha);
    json!({
        "number": pull.number,
        "title": pull.title,
        "body": pull.body,
        "state": pull.state,
        "draft": title_is_draft(&pull.title),
        "merged": pull.merged,
        "merged_at": pull.merged.then_some(UPDATED_AT.to_string()),
        "mergeable": pull.mergeable,
        "html_url": format!("{base_url}/{owner}/{repo}/pulls/{}", pull.number),
        "user": { "login": pull.user_login },
        "head": { "ref": pull.head_ref, "label": pull.head_ref, "sha": head_sha },
        "base": { "ref": pull.base_ref, "label": pull.base_ref },
        "created_at": pull.created_at,
        "updated_at": pull.updated_at,
    })
}

/// Resolve a PR head's live SHA from the branch it tracks. A merged or
/// closed PR keeps its recorded snapshot.
fn live_head_sha(
    state: &crate::state::AppState,
    owner: &str,
    repo: &str,
    pull: &PullRequest,
) -> Option<String> {
    if pull.state != "open" {
        return Some(pull.head_sha.clone());
    }
    state
        .repos
        .get(&format!("{owner}/{repo}"))?
        .branches
        .get(&pull.head_ref)
        .cloned()
        .or(Some(pull.head_sha.clone()))
}

pub async fn list_pull_requests(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((owner, repo)): Path<(String, String)>,
    Query(query): Query<ListQuery>,
) -> Response {
    let Ok(_) = authenticate(&state, &headers).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let state = state.read().await;
    let Some(pulls) = state.pulls.get(&format!("{owner}/{repo}")) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let limit = usize::try_from(query.limit.unwrap_or(50).clamp(1, 50)).unwrap_or(50);
    let page = usize::try_from(query.page.unwrap_or(1).max(1)).unwrap_or(1);
    let matching: Vec<&PullRequest> = pulls
        .iter()
        .filter(|pull| {
            let wanted = query.state.as_deref().unwrap_or("open");
            wanted == "all" || pull.state == wanted
        })
        .collect();
    let page_items: Vec<&PullRequest> = matching
        .iter()
        .skip((page - 1) * limit)
        .take(limit)
        .copied()
        .collect();

    let items: Vec<Value> = page_items
        .iter()
        .map(|pull| {
            let head_sha = live_head_sha(&state, &owner, &repo, pull);
            pull_json(
                state.web_base_url(),
                &owner,
                &repo,
                pull,
                head_sha.as_deref(),
            )
        })
        .collect();

    axum::Json(items).into_response()
}

#[derive(Deserialize)]
pub struct CreatePullRequestInput {
    title: String,
    #[serde(default)]
    body:  Option<String>,
    base:  String,
    head:  String,
}

pub async fn create_pull_request(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((owner, repo)): Path<(String, String)>,
    axum::Json(input): axum::Json<CreatePullRequestInput>,
) -> Response {
    let Ok(subject) = authenticate(&state, &headers).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let mut state = state.write().await;
    let Some(repository) = state.repos.get_mut(&format!("{owner}/{repo}")) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(head_sha) = repository.branches.get(&input.head).cloned() else {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            axum::Json(json!({ "message": "head branch does not exist" })),
        )
            .into_response();
    };
    if !repository.branches.contains_key(&input.base) {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            axum::Json(json!({ "message": "base branch does not exist" })),
        )
            .into_response();
    }

    // Gitea rejects a second open PR for the same base/head pair.
    let duplicate = state
        .pulls
        .get(&format!("{owner}/{repo}"))
        .is_some_and(|pulls| {
            pulls.iter().any(|pull| {
                pull.state == "open" && pull.head_ref == input.head && pull.base_ref == input.base
            })
        });
    if duplicate {
        return (
            StatusCode::CONFLICT,
            axum::Json(json!({ "message": "pull request already exists for these targets" })),
        )
            .into_response();
    }

    let pull = PullRequest {
        number: 0,
        title: input.title,
        body: input.body.unwrap_or_default(),
        state: "open".to_string(),
        merged: false,
        mergeable: true,
        user_login: subject.username,
        head_ref: input.head,
        head_sha,
        base_ref: input.base,
        created_at: CREATED_AT.to_string(),
        updated_at: UPDATED_AT.to_string(),
    };
    let number = state.push_pull_request(&owner, &repo, pull);
    let created = state
        .pulls
        .get(&format!("{owner}/{repo}"))
        .and_then(|pulls| pulls.iter().find(|pull| pull.number == number))
        .cloned()
        .expect("just-created pull request should be present");
    let body = pull_json(
        state.web_base_url(),
        &owner,
        &repo,
        &created,
        Some(&created.head_sha),
    );
    drop(state);

    (StatusCode::CREATED, axum::Json(body)).into_response()
}

pub async fn get_pull_request(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((owner, repo, number)): Path<(String, String, u64)>,
) -> Response {
    let Ok(_) = authenticate(&state, &headers).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let state = state.read().await;
    let Some(pull) = state
        .pulls
        .get(&format!("{owner}/{repo}"))
        .and_then(|pulls| pulls.iter().find(|pull| pull.number == number))
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let head_sha = live_head_sha(&state, &owner, &repo, pull);
    let body = pull_json(
        state.web_base_url(),
        &owner,
        &repo,
        pull,
        head_sha.as_deref(),
    );

    axum::Json(body).into_response()
}

#[derive(Deserialize)]
pub struct UpdatePullRequestInput {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    state: Option<String>,
}

pub async fn update_pull_request(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((owner, repo, number)): Path<(String, String, u64)>,
    axum::Json(input): axum::Json<UpdatePullRequestInput>,
) -> Response {
    let Ok(_) = authenticate(&state, &headers).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let mut state = state.write().await;
    let Some(pull) = state
        .pulls
        .get_mut(&format!("{owner}/{repo}"))
        .and_then(|pulls| pulls.iter_mut().find(|pull| pull.number == number))
    else {
        return StatusCode::NOT_FOUND.into_response();
    };

    if let Some(title) = input.title {
        pull.title = title;
    }
    if let Some(requested) = input.state {
        match requested.as_str() {
            "closed" => {
                pull.state = "closed".to_string();
                pull.updated_at = UPDATED_AT.to_string();
            }
            "open" if !pull.merged => {
                pull.state = "open".to_string();
                pull.updated_at = UPDATED_AT.to_string();
            }
            _ => {}
        }
    }

    let snapshot = pull.clone();
    let base_url = state.web_base_url().to_string();
    drop(state);

    let body = pull_json(
        &base_url,
        &owner,
        &repo,
        &snapshot,
        Some(&snapshot.head_sha),
    );

    // Gitea's edit endpoint answers 201 Created with the updated pull request.
    (StatusCode::CREATED, axum::Json(body)).into_response()
}

#[derive(Deserialize)]
pub struct MergePullRequestInput {
    /// Gitea's merge verb (`merge`, `squash`, `rebase`). The twin records the
    /// request shape but accepts any verb.
    #[allow(dead_code, reason = "request-shape fidelity only")]
    #[serde(rename = "Do", default)]
    do_merge: Option<String>,
}

pub async fn merge_pull_request(
    State(state): State<SharedState>,
    headers: HeaderMap,
    Path((owner, repo, number)): Path<(String, String, u64)>,
    axum::Json(_input): axum::Json<MergePullRequestInput>,
) -> Response {
    let Ok(_) = authenticate(&state, &headers).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let mut state = state.write().await;
    let Some(pull) = state
        .pulls
        .get_mut(&format!("{owner}/{repo}"))
        .and_then(|pulls| pulls.iter_mut().find(|pull| pull.number == number))
    else {
        return StatusCode::NOT_FOUND.into_response();
    };

    if pull.merged || pull.state != "open" {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    if !pull.mergeable {
        return StatusCode::CONFLICT.into_response();
    }

    pull.merged = true;
    pull.state = "closed".to_string();
    pull.updated_at = UPDATED_AT.to_string();
    drop(state);

    StatusCode::OK.into_response()
}
