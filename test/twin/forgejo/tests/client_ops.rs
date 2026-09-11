//! Functional tests that drive the twin Forgejo server with the real
//! `fabro-forgejo` client ops, mirroring how the production code calls a
//! Forgejo instance.

use fabro_forgejo::{
    ForgejoConfig, ForgejoContext, ForgejoCredentials, ForgejoInstance, branch_head_sha,
    close_pull_request, create_pull_request, enable_auto_merge, find_open_pull_request,
    get_authenticated_user, get_pull_request, get_repository, merge_pull_request,
};
use fabro_types::settings::run::MergeStrategy;
use twin_forgejo::{AppState, TestServer};

const OWNER: &str = "twin";
const REPO: &str = "app";

fn http_client() -> fabro_http::HttpClient {
    fabro_http::test_http_client().expect("test HTTP client should build")
}

/// A client config pointing at the twin instance with the fixture token.
/// The twin serves plain loopback HTTP, so the test-support constructor is
/// required instead of the production HTTPS-only [`ForgejoInstance::new`].
fn config(server: &TestServer) -> ForgejoConfig {
    let instance = ForgejoInstance::new_allowing_http(server.url())
        .expect("twin instance URL should validate");
    ForgejoConfig::new(
        instance,
        ForgejoCredentials::new(server.token().to_string()),
    )
}

fn context(config: &ForgejoConfig) -> ForgejoContext<'_> {
    ForgejoContext::new(&config.token, &config.instance)
}

async fn create_fixture_pr(server: &TestServer, head: &str) -> u64 {
    let config = config(server);
    let ctx = context(&config);
    let pr = create_pull_request(
        &http_client(),
        &ctx,
        OWNER,
        REPO,
        "main",
        head,
        "Test pull request",
        "twin-made body",
        false,
    )
    .await
    .expect("twin forgejo op should succeed");
    pr.number
}

async fn merge_bodies(server: &TestServer) -> Vec<serde_json::Value> {
    http_client()
        .get(format!("{}/__admin/merge-bodies", server.url()))
        .send()
        .await
        .expect("twin admin endpoint should respond")
        .json()
        .await
        .expect("twin admin merge bodies should parse")
}

#[tokio::test]
async fn client_ops_drive_the_full_pull_request_lifecycle() {
    let mut state = AppState::new();
    state.add_repository(
        OWNER,
        REPO,
        vec!["main".to_string(), "feature".to_string()],
        false,
    );
    let server = TestServer::start(state).await;

    let config = config(&server);
    let ctx = context(&config);
    let client = http_client();

    // Token validation and repository visibility.
    let user = get_authenticated_user(&client, &ctx)
        .await
        .expect("twin forgejo op should succeed");
    assert_eq!(user.login, "twin-user");

    let repository = get_repository(&client, &ctx, OWNER, REPO)
        .await
        .expect("twin forgejo op should succeed");
    let repository = repository.expect("fixture repository should be visible");
    assert_eq!(repository.full_name, format!("{OWNER}/{REPO}"));
    assert!(!repository.private);
    assert!(!repository.fork);

    // The twin reports the seeded bare-repo commit as the branch head.
    let sha = branch_head_sha(&client, &ctx, OWNER, REPO, "feature")
        .await
        .expect("twin forgejo op should succeed")
        .expect("fixture branch should exist");
    assert_eq!(sha.len(), 40);

    // Create and reconcile a pull request by head ref + head SHA.
    let created = create_pull_request(
        &client,
        &ctx,
        OWNER,
        REPO,
        "main",
        "feature",
        "Test pull request",
        "twin-made body",
        false,
    )
    .await
    .expect("twin forgejo op should succeed");
    assert_eq!(created.number, 1);
    assert_eq!(
        created.html_url,
        format!("{}/{OWNER}/{REPO}/pulls/1", server.url())
    );
    assert_eq!(created.title, "Test pull request");

    let reconciled = find_open_pull_request(&client, &ctx, OWNER, REPO, "feature", &sha)
        .await
        .expect("twin forgejo op should succeed")
        .expect("open PR with the expected head should be found");
    assert_eq!(reconciled.number, created.number);
    assert_eq!(reconciled.html_url, created.html_url);

    // A different head SHA must not match.
    let mismatched = find_open_pull_request(
        &client,
        &ctx,
        OWNER,
        REPO,
        "feature",
        "0".repeat(40).as_str(),
    )
    .await
    .expect("twin forgejo op should succeed");
    assert!(mismatched.is_none());

    // Merge, then confirm the PR is no longer open for that head.
    merge_pull_request(
        &client,
        &ctx,
        OWNER,
        REPO,
        created.number,
        MergeStrategy::Squash,
    )
    .await
    .expect("twin forgejo op should succeed");
    let after_merge = find_open_pull_request(&client, &ctx, OWNER, REPO, "feature", &sha)
        .await
        .expect("twin forgejo op should succeed");
    assert!(after_merge.is_none(), "merged PR must leave the open list");

    // The recorded merge body carried the Gitea-lineage `Do` spelling.
    let bodies = merge_bodies(&server).await;
    let last = bodies.last().expect("merge body should be recorded");
    assert_eq!(last["owner"], OWNER);
    assert_eq!(last["repo"], REPO);
    assert_eq!(last["index"], created.number);
    assert_eq!(last["body"]["Do"], "squash");
    assert!(last["body"].get("merge_when_checks_succeed").is_none());

    server.shutdown().await;
}

#[tokio::test]
async fn close_pull_request_marks_the_pr_closed() {
    let mut state = AppState::new();
    state.add_repository(
        OWNER,
        REPO,
        vec!["main".to_string(), "feature".to_string()],
        false,
    );
    let server = TestServer::start(state).await;
    let number = create_fixture_pr(&server, "feature").await;

    let config = config(&server);
    let ctx = context(&config);
    close_pull_request(&http_client(), &ctx, OWNER, REPO, number)
        .await
        .expect("twin forgejo op should succeed");

    let detail = get_pull_request(&http_client(), &ctx, OWNER, REPO, number)
        .await
        .expect("twin forgejo op should succeed");
    assert_eq!(detail.state, "closed");
    assert_eq!(detail.number, number);

    server.shutdown().await;
}

#[tokio::test]
async fn enable_auto_merge_sends_merge_when_checks_succeed() {
    let mut state = AppState::new();
    state.add_repository(
        OWNER,
        REPO,
        vec!["main".to_string(), "feature".to_string()],
        false,
    );
    let server = TestServer::start(state).await;
    let number = create_fixture_pr(&server, "feature").await;

    let config = config(&server);
    let ctx = context(&config);
    enable_auto_merge(
        &http_client(),
        &ctx,
        OWNER,
        REPO,
        number,
        MergeStrategy::Squash,
    )
    .await
    .expect("twin forgejo op should succeed");

    // The merge request must carry the auto-merge flag alongside the `Do`
    // value, which is how Gitea-lineage servers express auto-merge.
    let bodies = merge_bodies(&server).await;
    let last = bodies.last().expect("merge body should be recorded");
    assert_eq!(last["index"], number);
    assert_eq!(last["body"]["Do"], "squash");
    assert_eq!(last["body"]["merge_when_checks_succeed"], true);

    // Auto-merge schedules the merge; the PR stays open until checks pass.
    let detail = get_pull_request(&http_client(), &ctx, OWNER, REPO, number)
        .await
        .expect("twin forgejo op should succeed");
    assert_eq!(detail.state, "open");

    server.shutdown().await;
}

#[tokio::test]
async fn bad_tokens_are_rejected_with_401() {
    let mut state = AppState::new();
    state.add_repository(OWNER, REPO, vec!["main".to_string()], false);
    let server = TestServer::start(state).await;

    let instance = ForgejoInstance::new_allowing_http(server.url())
        .expect("twin instance URL should validate");
    let config = ForgejoConfig::new(instance, ForgejoCredentials::new("wrong-token".to_string()));
    let ctx = context(&config);
    let client = http_client();

    let err = get_authenticated_user(&client, &ctx)
        .await
        .expect_err("bad token must not authenticate");
    assert!(err.to_string().contains("authentication failed"));

    let heads = branch_head_sha(&client, &ctx, OWNER, REPO, "main").await;
    assert!(heads.is_err(), "bad token must not read branches");

    server.shutdown().await;
}
