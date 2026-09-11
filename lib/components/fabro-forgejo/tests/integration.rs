//! Integration tests for every Forgejo API operation against a local
//! httpmock server, exercising the real production HTTP client.

use fabro_forgejo::{
    ForgejoContext, PullRequestApiError, branch_head_sha, close_pull_request, create_pull_request,
    current_user, draft_title_prefix, find_open_pull_request, get_pull_request, get_repo,
    merge_pull_request,
};
use fabro_types::settings::run::MergeStrategy;
use httpmock::Method::{GET, PATCH, POST};
use httpmock::MockServer;

fn client() -> fabro_http::HttpClient {
    fabro_http::http_client().expect("test HTTP client should build")
}

struct ForgejoMock {
    server: MockServer,
    ctx:    ForgejoContext,
}

impl ForgejoMock {
    async fn start() -> Self {
        let server = MockServer::start_async().await;
        let ctx = ForgejoContext::new("test-token", server.url(""));
        Self { server, ctx }
    }
}

fn open_pulls_body() -> serde_json::Value {
    serde_json::json!([
        {
            "number": 7,
            "title": "Other change",
            "html_url": "pulls/7",
            "base": { "ref": "main" },
            "head": { "ref": "feature", "sha": "deadbeef" }
        },
        {
            "number": 9,
            "title": "Add widgets",
            "html_url": "https://git.example.com/acme/widgets/pulls/9",
            "base": { "ref": "main" },
            "head": { "ref": "run-abc", "sha": "cafe1234" }
        }
    ])
}

#[tokio::test]
async fn find_open_pull_request_matches_base_head_and_sha() {
    let mock = ForgejoMock::start().await;
    let pulls = open_pulls_body();
    let list_route = mock
        .server
        .mock_async(move |when, then| {
            when.method(GET)
                .path("/api/v1/repos/acme/widgets/pulls")
                .query_param("state", "open")
                .header("authorization", "token test-token");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(pulls);
        })
        .await;

    let found = find_open_pull_request(
        &client(), &mock.ctx, "acme", "widgets", "main", "run-abc", "cafe1234",
    )
    .await
    .expect("reconciliation should succeed");
    assert_eq!(found.expect("matching PR").number, 9);

    let missing = find_open_pull_request(
        &client(), &mock.ctx, "acme", "widgets", "main", "run-abc", "other",
    )
    .await
    .expect("reconciliation should succeed");
    assert_eq!(missing, None);
    // Both reconciliations served by the one list route, filtered to open
    // state only.
    assert_eq!(list_route.calls(), 2);
}

#[tokio::test]
async fn create_pull_request_posts_wip_title_for_drafts() {
    let mock = ForgejoMock::start().await;
    let create_mock = mock
        .server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/api/v1/repos/acme/widgets/pulls")
                .header("authorization", "token test-token")
                .json_body(serde_json::json!({
                    "title": "WIP: Add widgets",
                    "head": "run-abc",
                    "base": "main",
                    "body": "body"
                }));
            then.status(201)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({
                    "number": 12,
                    "html_url": "https://git.example.com/acme/widgets/pulls/12"
                }));
        })
        .await;

    let created = create_pull_request(
        &client(), &mock.ctx, "acme", "widgets", "main", "run-abc",
        "Add widgets", "body", true,
    )
    .await
    .expect("create should succeed");
    assert_eq!(created.number, 12);
    assert_eq!(created.title, "WIP: Add widgets");
    create_mock.assert();
}

#[tokio::test]
async fn create_pull_request_sends_plain_title_without_draft() {
    let mock = ForgejoMock::start().await;
    let create_mock = mock
        .server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/api/v1/repos/acme/widgets/pulls")
                .json_body(serde_json::json!({
                    "title": "Add widgets",
                    "head": "run-abc",
                    "base": "main",
                    "body": "body"
                }));
            then.status(201)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({
                    "number": 13,
                    "html_url": "https://git.example.com/acme/widgets/pulls/13"
                }));
        })
        .await;

    let created = create_pull_request(
        &client(), &mock.ctx, "acme", "widgets", "main", "run-abc",
        "Add widgets", "body", false,
    )
    .await
    .expect("create should succeed");
    assert_eq!(created.title, "Add widgets");
    create_mock.assert();
}

#[tokio::test]
async fn create_pull_request_maps_error_statuses() {
    for (status, fragment) in [(422u16, "422"), (401, "401"), (404, "404"), (500, "500")] {
        let mock = ForgejoMock::start().await;
        mock.server.mock(|when, then| {
            when.method(POST).path("/api/v1/repos/acme/widgets/pulls");
            then.status(status).body("{}");
        });
        let error = create_pull_request(
            &client(), &mock.ctx, "acme", "widgets", "main", "run-abc",
            "Add widgets", "body", false,
        )
        .await
        .expect_err("create should fail");
        assert!(error.to_string().contains(fragment), "{error}");
    }
}

#[tokio::test]
async fn get_pull_request_projects_the_forgejo_payload() {
    let mock = ForgejoMock::start().await;
    let detail_mock = mock
        .server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/api/v1/repos/acme/widgets/pulls/9")
                .header("accept", "application/json");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(serde_json::json!({
                    "number": 9,
                    "title": "WIP: Add widgets",
                    "body": "PR body",
                    "state": "open",
                    "draft": true,
                    "merged": false,
                    "mergeable": true,
                    "html_url": "https://git.example.com/acme/widgets/pulls/9",
                    "user": { "login": "fabro" },
                    "head": { "ref": "run-abc" },
                    "base": { "ref": "main" },
                    "created_at": "2026-09-10T00:00:00Z",
                    "updated_at": "2026-09-10T01:00:00Z"
                }));
        })
        .await;

    let detail = get_pull_request(&client(), &mock.ctx, "acme", "widgets", 9)
        .await
        .expect("read should succeed");
    assert_eq!(detail.number, 9);
    assert_eq!(detail.title, "WIP: Add widgets");
    assert!(detail.draft);
    assert_eq!(detail.state, "open");
    assert_eq!(detail.user.login, "fabro");
    assert_eq!(detail.head.ref_name, "run-abc");
    assert_eq!(detail.base.ref_name, "main");
    detail_mock.assert();

    // Missing PRs are distinguished from other failures.
    let mock = ForgejoMock::start().await;
    mock.server.mock(|when, then| {
        when.method(GET).path("/api/v1/repos/acme/widgets/pulls/404");
        then.status(404).body("{}");
    });
    match get_pull_request(&client(), &mock.ctx, "acme", "widgets", 404).await
    {
        Err(PullRequestApiError::NotFound { owner, repo, number }) => {
            assert_eq!((owner.as_str(), repo.as_str(), number), ("acme", "widgets", 404));
        }
        other => panic!("expected NotFound, got {other:?}"),
    }

    // Malformed JSON surfaces as a generic error.
    let mock = ForgejoMock::start().await;
    mock.server.mock(|when, then| {
        when.method(GET).path("/api/v1/repos/acme/widgets/pulls/9");
        then.status(200).body("not json");
    });
    assert!(
        get_pull_request(&client(), &mock.ctx, "acme", "widgets", 9)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn merge_pull_request_uses_the_do_field() {
    for (strategy, do_value) in [
        (MergeStrategy::Merge, "merge"),
        (MergeStrategy::Squash, "squash"),
        (MergeStrategy::Rebase, "rebase"),
    ] {
        let mock = ForgejoMock::start().await;
        let merge_mock = mock
            .server
            .mock_async(move |when, then| {
                when.method(POST)
                    .path("/api/v1/repos/acme/widgets/pulls/9/merge")
                    .json_body(serde_json::json!({ "Do": do_value }));
                then.status(200);
            })
            .await;
        merge_pull_request(&client(), &mock.ctx, "acme", "widgets", 9, strategy)
            .await
            .expect("merge should succeed");
        merge_mock.assert();
    }

    let mock = ForgejoMock::start().await;
    mock.server.mock(|when, then| {
        when.method(POST).path("/api/v1/repos/acme/widgets/pulls/9/merge");
        then.status(405).body("{}");
    });
    assert!(
        merge_pull_request(&client(), &mock.ctx, "acme", "widgets", 9, MergeStrategy::Merge)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn close_pull_request_patches_the_issue_endpoint() {
    let mock = ForgejoMock::start().await;
    let close_mock = mock
        .server
        .mock_async(|when, then| {
            when.method(PATCH)
                .path("/api/v1/repos/acme/widgets/issues/9")
                .json_body(serde_json::json!({ "state": "closed" }));
            then.status(200).json_body(serde_json::json!({ "state": "closed" }));
        })
        .await;
    close_pull_request(&client(), &mock.ctx, "acme", "widgets", 9)
        .await
        .expect("close should succeed");
    close_mock.assert();

    let mock = ForgejoMock::start().await;
    mock.server.mock(|when, then| {
        when.method(PATCH).path("/api/v1/repos/acme/widgets/issues/9");
        then.status(403).body("{}");
    });
    assert!(
        close_pull_request(&client(), &mock.ctx, "acme", "widgets", 9)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn branch_head_sha_reads_the_commit_id() {
    let mock = ForgejoMock::start().await;
    mock.server.mock(|when, then| {
        when.method(GET).path("/api/v1/repos/acme/widgets/branches/main");
        then.status(200).json_body(serde_json::json!({
            "name": "main",
            "commit": { "id": "abc1234" }
        }));
    });
    let sha = branch_head_sha(&client(), &mock.ctx, "acme", "widgets", "main")
        .await
        .expect("read should succeed");
    assert_eq!(sha.as_deref(), Some("abc1234"));

    let mock = ForgejoMock::start().await;
    mock.server.mock(|when, then| {
        when.method(GET).path("/api/v1/repos/acme/widgets/branches/missing");
        then.status(404).body("{}");
    });
    let sha = branch_head_sha(&client(), &mock.ctx, "acme", "widgets", "missing")
        .await
        .expect("missing branch is not an error");
    assert_eq!(sha, None);

    let mock = ForgejoMock::start().await;
    mock.server.mock(|when, then| {
        when.method(GET).path("/api/v1/repos/acme/widgets/branches/main");
        then.status(401).body("{}");
    });
    assert!(
        branch_head_sha(&client(), &mock.ctx, "acme", "widgets", "main")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn get_repo_returns_none_for_missing_repositories() {
    let mock = ForgejoMock::start().await;
    let repo_mock = mock
        .server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/api/v1/repos/acme/widgets")
                .header("authorization", "token test-token");
            then.status(200).json_body(serde_json::json!({
                "full_name": "acme/widgets",
                "private": true,
                "default_branch": "main",
                "owner": { "login": "acme" },
                "permissions": { "admin": false, "push": true, "pull": true }
            }));
        })
        .await;

    let repo = get_repo(&client(), &mock.ctx, "acme", "widgets")
        .await
        .expect("read should succeed")
        .expect("repository exists");
    assert_eq!(repo.full_name, "acme/widgets");
    assert_eq!(repo.default_branch.as_deref(), Some("main"));
    assert!(repo.permissions.expect("permissions").push);
    repo_mock.assert();

    let mock = ForgejoMock::start().await;
    mock.server.mock(|when, then| {
        when.method(GET).path("/api/v1/repos/acme/missing");
        then.status(404).body("{}");
    });
    assert_eq!(
        get_repo(&client(), &mock.ctx, "acme", "missing")
            .await
            .expect("404 is not an error"),
        None
    );

    let mock = ForgejoMock::start().await;
    mock.server.mock(|when, then| {
        when.method(GET).path("/api/v1/repos/acme/widgets");
        then.status(403).body("{}");
    });
    assert!(
        get_repo(&client(), &mock.ctx, "acme", "widgets")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn current_user_probes_the_token() {
    let mock = ForgejoMock::start().await;
    let user_mock = mock
        .server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/api/v1/user")
                .header("authorization", "token test-token");
            then.status(200).json_body(serde_json::json!({ "login": "fabro" }));
        })
        .await;
    let user = current_user(&client(), &mock.ctx)
        .await
        .expect("probe should succeed");
    assert_eq!(user.login, "fabro");
    user_mock.assert();

    let mock = ForgejoMock::start().await;
    mock.server
        .mock(|when, then| {
            when.method(GET).path("/api/v1/user");
            then.status(401).body("{}");
        });
    let error = current_user(&client(), &mock.ctx)
        .await
        .expect_err("invalid tokens fail");
    assert!(error.to_string().contains("FORGEJO_TOKEN"));
}

#[test]
fn draft_prefix_maps_onto_the_wip_convention() {
    assert_eq!(draft_title_prefix(true), "WIP: ");
    assert_eq!(draft_title_prefix(false), "");
}
