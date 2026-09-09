//! Endpoint, URL, and wire-format tests. The fixtures encode the Forgejo API
//! shapes this crate relies on: `Authorization: token`, `commit.id` branch
//! heads, the capital-`Do` merge action, token-as-password git auth, and the
//! `WIP:` draft convention.

use fabro_types::settings::run::MergeStrategy;

use crate::tests_mock::MockHttpClient;
use crate::{
    ForgejoContext, ForgejoCredentials, HttpMethod, PullRequestApiError,
    branch_head_sha_with_client, close_pull_request_with_client, create_pull_request_with_client,
    credential_helper_key, embed_token_in_url, find_open_pull_request_with_client,
    forgejo_base_url, get_authenticated_user_with_client, get_pull_request_with_client,
    merge_do_value, merge_pull_request_with_client, normalize_repo_origin_url,
    parse_forgejo_owner_repo,
};

const BASE: &str = "https://forgejo.example.com";

fn ctx() -> ForgejoContext<'static> {
    let creds: &'static ForgejoCredentials = Box::leak(Box::new(ForgejoCredentials::Pat(
        "fp_test_token".to_string(),
    )));
    ForgejoContext::new(creds, BASE)
}

#[tokio::test]
async fn authenticated_user_uses_token_scheme_header() {
    let client = MockHttpClient::new()
        .on(HttpMethod::Get, "/api/v1/user", 200, r#"{"login": "acme"}"#)
        .with_req_header("Authorization", "token fp_test_token");

    let user = get_authenticated_user_with_client(&client, &ctx())
        .await
        .expect("user lookup should succeed");
    assert_eq!(user.login, "acme");
}

#[tokio::test]
async fn authenticated_user_rejects_invalid_token() {
    let client = MockHttpClient::new().on(HttpMethod::Get, "/api/v1/user", 401, "{}");

    let err = get_authenticated_user_with_client(&client, &ctx())
        .await
        .expect_err("invalid token should error");
    let rendered = err.to_string();
    assert!(rendered.contains("FORGEJO_TOKEN"), "{rendered}");
}

#[tokio::test]
async fn get_repo_parses_metadata() {
    let client = MockHttpClient::new().on(
        HttpMethod::Get,
        "/api/v1/repos/acme/widgets",
        200,
        r#"{
            "full_name": "acme/widgets",
            "default_branch": "main",
            "private": true,
            "html_url": "https://forgejo.example.com/acme/widgets"
        }"#,
    );

    let repo = crate::get_repo_with_client(&client, &ctx(), "acme", "widgets")
        .await
        .expect("repo lookup should succeed");
    assert_eq!(repo.full_name, "acme/widgets");
    assert_eq!(repo.default_branch.as_deref(), Some("main"));
    assert_eq!(repo.private, Some(true));
}

#[tokio::test]
async fn get_repo_404_is_reported_with_instance() {
    let client = MockHttpClient::new().on(HttpMethod::Get, "/api/v1/repos/acme/missing", 404, "{}");

    let err = crate::get_repo_with_client(&client, &ctx(), "acme", "missing")
        .await
        .expect_err("missing repo should error");
    let rendered = err.to_string();
    assert!(rendered.contains("acme/missing"), "{rendered}");
    assert!(rendered.contains(BASE), "{rendered}");
}

#[tokio::test]
async fn branch_head_sha_reads_commit_id_field() {
    // Forgejo reports the branch commit under `commit.id`, not `commit.sha`.
    let client = MockHttpClient::new().on(
        HttpMethod::Get,
        "/api/v1/repos/acme/widgets/branches/feature",
        200,
        r#"{
            "name": "feature",
            "commit": {
                "id": "abc123def456abc123def456abc123def456abcd",
                "message": "work"
            }
        }"#,
    );

    let sha = branch_head_sha_with_client(&client, &ctx(), "acme", "widgets", "feature")
        .await
        .expect("branch lookup should succeed");
    assert_eq!(
        sha.as_deref(),
        Some("abc123def456abc123def456abc123def456abcd")
    );
}

#[tokio::test]
async fn branch_head_sha_is_none_for_missing_branch() {
    let client = MockHttpClient::new().on(
        HttpMethod::Get,
        "/api/v1/repos/acme/widgets/branches/ghost",
        404,
        "{}",
    );

    let sha = branch_head_sha_with_client(&client, &ctx(), "acme", "widgets", "ghost")
        .await
        .expect("missing branch is not an error");
    assert!(sha.is_none());
}

#[tokio::test]
async fn create_pull_request_posts_minimal_body_and_wip_title_for_drafts() {
    // Forgejo has no `draft` field: a draft is a `WIP:` title.
    let client = MockHttpClient::new()
        .on(
            HttpMethod::Post,
            "/api/v1/repos/acme/widgets/pulls",
            201,
            r#"{
                "number": 7,
                "title": "WIP: Add widget",
                "html_url": "https://forgejo.example.com/acme/widgets/pulls/7"
            }"#,
        )
        .with_req_body(
            r#"{
                "title": "WIP: Add widget",
                "head": "fabro/run/demo",
                "base": "main",
                "body": "Automated change"
            }"#,
        );

    let created = create_pull_request_with_client(
        &client,
        &ctx(),
        "acme",
        "widgets",
        "main",
        "fabro/run/demo",
        "Add widget",
        "Automated change",
        true,
    )
    .await
    .expect("create should succeed");
    assert_eq!(created.number, 7);
    assert_eq!(created.title, "WIP: Add widget");
    assert_eq!(
        created.html_url,
        "https://forgejo.example.com/acme/widgets/pulls/7"
    );
}

#[tokio::test]
async fn create_pull_request_keeps_existing_wip_prefix() {
    let client = MockHttpClient::new()
        .on(
            HttpMethod::Post,
            "/api/v1/repos/acme/widgets/pulls",
            201,
            r#"{ "number": 8, "title": "[WIP] Already wip", "html_url": "https://forgejo.example.com/acme/widgets/pulls/8" }"#,
        )
        .with_req_body(
            r#"{
                "title": "[WIP] Already wip",
                "head": "fabro/run/demo",
                "base": "main",
                "body": ""
            }"#,
        );

    let created = create_pull_request_with_client(
        &client,
        &ctx(),
        "acme",
        "widgets",
        "main",
        "fabro/run/demo",
        "[WIP] Already wip",
        "",
        true,
    )
    .await
    .expect("create should succeed");
    assert_eq!(created.title, "[WIP] Already wip");
}

#[tokio::test]
async fn find_open_pull_request_filters_by_base_and_head_sha() {
    let client = MockHttpClient::new().on(
        HttpMethod::Get,
        "/api/v1/repos/acme/widgets/pulls",
        200,
        r#"[
            {
                "number": 3,
                "title": "Stale",
                "html_url": "https://forgejo.example.com/acme/widgets/pulls/3",
                "head": { "ref": "fabro/run/demo", "sha": "0000000000000000000000000000000000000000" },
                "base": { "ref": "main" }
            },
            {
                "number": 9,
                "title": "Match",
                "html_url": "https://forgejo.example.com/acme/widgets/pulls/9",
                "head": { "ref": "fabro/run/demo", "sha": "1111111111111111111111111111111111111111" },
                "base": { "ref": "main" }
            },
            {
                "number": 10,
                "title": "Wrong base",
                "html_url": "https://forgejo.example.com/acme/widgets/pulls/10",
                "head": { "ref": "fabro/run/demo", "sha": "1111111111111111111111111111111111111111" },
                "base": { "ref": "release" }
            }
        ]"#,
    );

    let found = find_open_pull_request_with_client(
        &client,
        &ctx(),
        "acme",
        "widgets",
        "main",
        "1111111111111111111111111111111111111111",
    )
    .await
    .expect("reconciliation should succeed");
    let found = found.expect("one PR should match");
    assert_eq!(found.number, 9);
    assert_eq!(found.title, "Match");
}

#[tokio::test]
async fn get_pull_request_maps_forgejo_detail() {
    let client = MockHttpClient::new().on(
        HttpMethod::Get,
        "/api/v1/repos/acme/widgets/pulls/12",
        200,
        r#"{
            "number": 12,
            "title": "WIP: Add widget",
            "body": "Body text",
            "state": "open",
            "html_url": "https://forgejo.example.com/acme/widgets/pulls/12",
            "user": { "login": "octocat" },
            "head": { "ref": "fabro/run/demo", "sha": "abc" },
            "base": { "ref": "main", "sha": "def" },
            "merged": false,
            "mergeable": true,
            "created_at": "2026-09-01T10:00:00Z",
            "updated_at": "2026-09-02T10:00:00Z"
        }"#,
    );

    let detail = get_pull_request_with_client(&client, &ctx(), "acme", "widgets", 12)
        .await
        .expect("detail fetch should succeed");
    let details: fabro_types::PullRequestDetails = detail.into();
    assert_eq!(details.title, "WIP: Add widget");
    assert!(details.draft, "WIP titles map to draft");
    assert_eq!(details.author.login, "octocat");
    assert_eq!(details.head_branch, "fabro/run/demo");
    assert_eq!(details.base_branch, "main");
    assert_eq!(details.mergeable, Some(true));
    // Forgejo does not expose diff counters on the pull request object.
    assert_eq!(details.additions, 0);
    assert_eq!(details.changed_files, 0);
}

#[tokio::test]
async fn get_pull_request_404_maps_to_not_found() {
    let client = MockHttpClient::new().on(
        HttpMethod::Get,
        "/api/v1/repos/acme/widgets/pulls/99",
        404,
        "{}",
    );

    let err = get_pull_request_with_client(&client, &ctx(), "acme", "widgets", 99)
        .await
        .expect_err("missing PR should error");
    assert!(matches!(err, PullRequestApiError::NotFound {
        number: 99,
        ..
    }));
}

#[tokio::test]
async fn merge_pull_request_sends_capital_do_action() {
    let client = MockHttpClient::new()
        .on(
            HttpMethod::Post,
            "/api/v1/repos/acme/widgets/pulls/12/merge",
            200,
            "",
        )
        .with_req_body(r#"{ "Do": "squash" }"#);

    merge_pull_request_with_client(
        &client,
        &ctx(),
        "acme",
        "widgets",
        12,
        MergeStrategy::Squash,
    )
    .await
    .expect("merge should succeed");
}

#[tokio::test]
async fn merge_pull_request_405_maps_to_not_mergeable() {
    let client = MockHttpClient::new().on(
        HttpMethod::Post,
        "/api/v1/repos/acme/widgets/pulls/12/merge",
        405,
        r#"{"message": "not in mergeable state"}"#,
    );

    let err = merge_pull_request_with_client(
        &client,
        &ctx(),
        "acme",
        "widgets",
        12,
        MergeStrategy::Merge,
    )
    .await
    .expect_err("405 should error");
    assert!(err.to_string().contains("not mergeable"), "{err}");
}

#[tokio::test]
async fn close_pull_request_accepts_201_from_edit_endpoint() {
    // Forgejo's edit-pull-request endpoint answers 201, not 200.
    let client = MockHttpClient::new()
        .on(
            HttpMethod::Patch,
            "/api/v1/repos/acme/widgets/pulls/12",
            201,
            r#"{ "number": 12, "state": "closed" }"#,
        )
        .with_req_body(r#"{ "state": "closed" }"#);

    close_pull_request_with_client(&client, &ctx(), "acme", "widgets", 12)
        .await
        .expect("close should succeed");
}

#[test]
fn merge_do_value_covers_all_strategies() {
    assert_eq!(merge_do_value(MergeStrategy::Merge), "merge");
    assert_eq!(merge_do_value(MergeStrategy::Squash), "squash");
    assert_eq!(merge_do_value(MergeStrategy::Rebase), "rebase");
}

#[test]
fn parse_owner_repo_accepts_https_ssh_and_subpath_origins() {
    let cases = [
        (
            "https://forgejo.example.com/acme/widgets.git",
            ("acme", "widgets"),
        ),
        (
            "https://forgejo.example.com/acme/widgets",
            ("acme", "widgets"),
        ),
        (
            "git@forgejo.example.com:acme/widgets.git",
            ("acme", "widgets"),
        ),
        (
            "https://token@forgejo.example.com/acme/widgets.git/",
            ("acme", "widgets"),
        ),
        (
            "https://example.com/forgejo/acme/widgets.git",
            ("acme", "widgets"),
        ),
    ];
    for (url, (owner, repo)) in cases {
        let base = if url.starts_with("https://example.com") {
            "https://example.com/forgejo"
        } else {
            BASE
        };
        let parsed = parse_forgejo_owner_repo(url, base).expect(url);
        assert_eq!(parsed, (owner.to_string(), repo.to_string()), "{url}");
    }
}

#[test]
fn parse_owner_repo_rejects_other_hosts() {
    for url in [
        "https://github.com/acme/widgets",
        "https://other.example.com/acme/widgets",
        // `forgejo.example.com.evil.test` shares a prefix but is another host.
        "https://forgejo.example.com.evil.test/acme/widgets",
    ] {
        assert!(parse_forgejo_owner_repo(url, BASE).is_err(), "{url}");
    }
}

#[test]
fn parse_owner_repo_rejects_extra_path_segments() {
    assert!(
        parse_forgejo_owner_repo("https://forgejo.example.com/acme/widgets/extra", BASE).is_err()
    );
}

#[test]
fn embed_token_uses_basic_auth_password_and_redacts_display() {
    let url = embed_token_in_url("https://forgejo.example.com/acme/widgets", "fp_secret")
        .expect("embedding should succeed");
    assert_eq!(
        url.as_str(),
        "https://x-access-token:fp_secret@forgejo.example.com/acme/widgets"
    );
    assert!(
        !url.redacted_string().contains("fp_secret"),
        "display form must redact: {}",
        url.redacted_string()
    );
}

#[test]
fn normalize_repo_origin_url_is_host_generic() {
    assert_eq!(
        normalize_repo_origin_url("git@forgejo.example.com:acme/widgets.git"),
        "https://forgejo.example.com/acme/widgets"
    );
    assert_eq!(
        normalize_repo_origin_url("https://token@forgejo.example.com/acme/widgets.git/"),
        "https://forgejo.example.com/acme/widgets"
    );
}

#[test]
fn credential_helper_key_scopes_the_instance_host() {
    assert_eq!(
        credential_helper_key("https://forgejo.example.com"),
        "credential.https://forgejo.example.com.helper"
    );
    // Subpath installs scope credentials to the host; the path check stays
    // with the clone decision.
    assert_eq!(
        credential_helper_key("https://example.com/forgejo"),
        "credential.https://example.com.helper"
    );
}

#[test]
fn forgejo_base_url_prefers_explicit_and_trims() {
    assert_eq!(
        forgejo_base_url(Some("https://forgejo.example.com/")),
        Some("https://forgejo.example.com".to_string())
    );
    assert_eq!(forgejo_base_url(Some("  ")), None);
    assert_eq!(forgejo_base_url(None), None);
}
