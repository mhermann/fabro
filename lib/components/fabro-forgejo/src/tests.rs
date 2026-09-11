//! Unit tests for the Forgejo client: URL parsing, token embedding, and
//! every REST operation through the scripted [`MockHttpClient`].

use serde_json::json;

use super::tests_mock::MockHttpClient;
use super::*;

fn instance() -> ForgejoInstance {
    ForgejoInstance::new("https://git.example.com").unwrap()
}

fn ctx<'a>(instance: &'a ForgejoInstance, creds: &'a ForgejoCredentials) -> ForgejoContext<'a> {
    ForgejoContext::new(creds, instance)
}

fn creds() -> ForgejoCredentials {
    ForgejoCredentials::new("forgejo_pat_123".to_string())
}

fn user_body() -> String {
    json!({ "login": "forgejo-bot" }).to_string()
}

// ---------------------------------------------------------------------------
// ForgejoInstance
// ---------------------------------------------------------------------------

#[test]
fn instance_accepts_plain_https_host() {
    let instance = ForgejoInstance::new("https://git.example.com").unwrap();
    assert_eq!(instance.as_str(), "https://git.example.com");
    assert_eq!(instance.host(), "git.example.com");
    assert_eq!(instance.api_base_url(), "https://git.example.com/api/v1");
}

#[test]
fn instance_normalizes_trailing_slash_and_keeps_subpath() {
    let instance = ForgejoInstance::new("https://git.example.com/forge/").unwrap();
    assert_eq!(instance.as_str(), "https://git.example.com/forge");
    assert_eq!(instance.host(), "git.example.com");
}

#[test]
fn instance_rejects_plain_http() {
    let err = ForgejoInstance::new("http://git.example.com").unwrap_err();
    assert_eq!(
        err,
        ForgejoInstanceError::NotHttps("http://git.example.com".to_string())
    );
}

#[test]
fn instance_rejects_credentials_query_and_fragment() {
    assert!(ForgejoInstance::new("https://user:pass@git.example.com").is_err());
    assert!(ForgejoInstance::new("https://git.example.com/?q=1").is_err());
    assert!(ForgejoInstance::new("https://git.example.com/#frag").is_err());
    assert!(ForgejoInstance::new("not a url").is_err());
}

#[test]
fn instance_deserialize_validates() {
    let parsed: ForgejoInstance = serde_json::from_value(json!("https://git.example.com")).unwrap();
    assert_eq!(parsed.as_str(), "https://git.example.com");
    assert!(
        serde_json::from_value::<ForgejoInstance>(json!("http://git.example.com")).is_err(),
        "plain HTTP must be rejected on deserialize"
    );
    assert_eq!(
        serde_json::to_value(&parsed).unwrap(),
        json!("https://git.example.com")
    );
}

// ---------------------------------------------------------------------------
// parse_forgejo_owner_repo / normalize / is_forgejo_origin
// ---------------------------------------------------------------------------

#[test]
fn parse_https_with_git_suffix() {
    let (owner, repo) =
        parse_forgejo_owner_repo(&instance(), "https://git.example.com/acme/widgets.git").unwrap();
    assert_eq!(owner, "acme");
    assert_eq!(repo, "widgets");
}

#[test]
fn parse_https_without_git_suffix_and_trailing_slash() {
    let (owner, repo) =
        parse_forgejo_owner_repo(&instance(), "https://git.example.com/acme/widgets/").unwrap();
    assert_eq!(owner, "acme");
    assert_eq!(repo, "widgets");
}

#[test]
fn parse_rejects_extra_path_components() {
    let error =
        parse_forgejo_owner_repo(&instance(), "https://git.example.com/acme/widgets/issues")
            .unwrap_err();
    assert!(
        error.to_string().contains("must identify one repository"),
        "got: {error}"
    );
}

#[test]
fn parse_rejects_other_host() {
    let error = parse_forgejo_owner_repo(&instance(), "https://other.example.com/acme/widgets")
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Not a repository on Forgejo instance"),
        "got: {error}"
    );
}

#[test]
fn parse_accepts_subpath_instance() {
    let instance = ForgejoInstance::new("https://git.example.com/forge").unwrap();
    let (owner, repo) =
        parse_forgejo_owner_repo(&instance, "https://git.example.com/forge/acme/widgets.git")
            .unwrap();
    assert_eq!(owner, "acme");
    assert_eq!(repo, "widgets");
    assert!(
        parse_forgejo_owner_repo(&instance, "https://git.example.com/acme/widgets").is_err(),
        "origin outside the subpath must not parse"
    );
}

#[test]
fn parse_accepts_ssh_shape() {
    let (owner, repo) =
        parse_forgejo_owner_repo(&instance(), "git@git.example.com:acme/widgets.git").unwrap();
    assert_eq!(owner, "acme");
    assert_eq!(repo, "widgets");
}

#[test]
fn parse_accepts_credential_bearing_https() {
    let (owner, repo) = parse_forgejo_owner_repo(
        &instance(),
        "https://x-access-token:forgejo_pat_123@git.example.com/acme/widgets.git",
    )
    .unwrap();
    assert_eq!(owner, "acme");
    assert_eq!(repo, "widgets");
}

#[test]
fn parse_error_message_never_contains_the_token() {
    let error = parse_forgejo_owner_repo(
        &instance(),
        "https://x-access-token:forgejo_pat_123@git.example.com/acme/widgets/extra",
    )
    .unwrap_err();
    let rendered = error.to_string();
    assert!(
        !rendered.contains("forgejo_pat_123"),
        "leaked token: {rendered}"
    );
}

#[test]
fn is_forgejo_origin_matches_instance_and_subpaths() {
    let instance = instance();
    assert!(is_forgejo_origin(
        &instance,
        "https://git.example.com/acme/widgets"
    ));
    assert!(is_forgejo_origin(
        &instance,
        "git@git.example.com:acme/widgets.git"
    ));
    assert!(!is_forgejo_origin(
        &instance,
        "https://github.com/acme/widgets"
    ));
    assert!(!is_forgejo_origin(&instance, ""));

    // A subpath instance must not claim every repository on its host.
    let subpath = ForgejoInstance::new("https://git.example.com/forge").unwrap();
    assert!(
        is_forgejo_origin(&subpath, "https://git.example.com/forge/acme/widgets"),
        "repo under the subpath belongs to the instance"
    );
    assert!(
        !is_forgejo_origin(&subpath, "https://git.example.com/acme/widgets"),
        "repo outside the subpath does not belong to the instance"
    );
}

// ---------------------------------------------------------------------------
// embed_token_in_url
// ---------------------------------------------------------------------------

#[test]
fn embed_token_in_url_redacts_display_and_keeps_raw_access() {
    let url = embed_token_in_url(
        "https://git.example.com/acme/widgets.git",
        "forgejo_pat_123",
    )
    .unwrap();

    assert_eq!(
        url.redacted_string(),
        "https://x-access-token:****@git.example.com/acme/widgets.git"
    );
    assert_eq!(
        url.raw_string(),
        "https://x-access-token:forgejo_pat_123@git.example.com/acme/widgets.git"
    );
}

#[test]
fn embed_token_in_url_rejects_non_https() {
    let err =
        embed_token_in_url("http://git.example.com/acme/widgets", "forgejo_pat_123").unwrap_err();
    assert!(err.to_string().contains("must use HTTPS"), "got: {err}");
}

#[test]
fn credential_helper_key_is_instance_scoped() {
    assert_eq!(
        forgejo_credential_helper_key(&instance()),
        "credential.https://git.example.com.helper"
    );
    assert!(
        FORGEJO_CREDENTIAL_HELPER.contains("$FORGEJO_TOKEN"),
        "helper must read the token from the environment"
    );
}

// ---------------------------------------------------------------------------
// REST operations
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_authenticated_user_sends_token_header() {
    let instance = instance();
    let creds = creds();
    let client = MockHttpClient::new()
        .on(HttpMethod::Get, "/api/v1/user", 200, &user_body())
        .with_req_header("Authorization", "token forgejo_pat_123");
    let user = get_authenticated_user(&client, &ctx(&instance, &creds))
        .await
        .unwrap();
    assert_eq!(user.login, "forgejo-bot");
}

#[tokio::test]
async fn get_authenticated_user_reports_auth_failures() {
    let instance = instance();
    let creds = creds();
    let client = MockHttpClient::new().on(HttpMethod::Get, "/api/v1/user", 401, "{}");
    let err = get_authenticated_user(&client, &ctx(&instance, &creds))
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("Forgejo authentication failed"),
        "got: {err}"
    );
}

#[tokio::test]
async fn get_repository_distinguishes_found_and_missing() {
    let instance = instance();
    let creds = creds();
    let body = json!({ "full_name": "acme/widgets", "private": true, "fork": false }).to_string();
    let client =
        MockHttpClient::new().on(HttpMethod::Get, "/api/v1/repos/acme/widgets", 200, &body);
    let repo = get_repository(&client, &ctx(&instance, &creds), "acme", "widgets")
        .await
        .unwrap();
    assert_eq!(repo.unwrap().full_name, "acme/widgets");

    let client = MockHttpClient::new().on(HttpMethod::Get, "/api/v1/repos/acme/missing", 404, "{}");
    let repo = get_repository(&client, &ctx(&instance, &creds), "acme", "missing")
        .await
        .unwrap();
    assert!(repo.is_none());
}

#[tokio::test]
async fn branch_head_sha_reads_the_gitea_commit_id_field() {
    let instance = instance();
    let creds = creds();
    let body = json!({
        "name": "main",
        "commit": { "id": "abc123def456", "message": "init" }
    })
    .to_string();
    let client = MockHttpClient::new().on(
        HttpMethod::Get,
        "/api/v1/repos/acme/widgets/branches/main",
        200,
        &body,
    );
    let sha = branch_head_sha(&client, &ctx(&instance, &creds), "acme", "widgets", "main")
        .await
        .unwrap();
    assert_eq!(sha.as_deref(), Some("abc123def456"));

    let client = MockHttpClient::new().on(
        HttpMethod::Get,
        "/api/v1/repos/acme/widgets/branches/nope",
        404,
        "{}",
    );
    let sha = branch_head_sha(&client, &ctx(&instance, &creds), "acme", "widgets", "nope")
        .await
        .unwrap();
    assert_eq!(sha, None);
}

fn pull_list_item(number: u64, head_ref: &str, head_sha: &str) -> String {
    json!([{
        "number": number,
        "html_url": format!("https://git.example.com/acme/widgets/pulls/{number}"),
        "title": format!("PR {number}"),
        "state": "open",
        "head": { "ref": head_ref, "sha": head_sha },
        "base": { "ref": "main", "sha": "base0000" },
    }])
    .to_string()
}

#[tokio::test]
async fn find_open_pull_request_matches_head_ref_and_sha() {
    let instance = instance();
    let creds = creds();
    let client = MockHttpClient::new().on(
        HttpMethod::Get,
        "/api/v1/repos/acme/widgets/pulls",
        200,
        &pull_list_item(7, "fabro/run-1", "head0001"),
    );
    let found = find_open_pull_request(
        &client,
        &ctx(&instance, &creds),
        "acme",
        "widgets",
        "fabro/run-1",
        "head0001",
    )
    .await
    .unwrap();
    let found = found.expect("expected the open PR to reconcile");
    assert_eq!(found.number, 7);
    assert_eq!(
        found.html_url,
        "https://git.example.com/acme/widgets/pulls/7"
    );
}

#[tokio::test]
async fn find_open_pull_request_returns_none_when_head_differs() {
    let instance = instance();
    let creds = creds();
    let client = MockHttpClient::new().on(
        HttpMethod::Get,
        "/api/v1/repos/acme/widgets/pulls",
        200,
        &pull_list_item(7, "fabro/run-1", "different"),
    );
    let found = find_open_pull_request(
        &client,
        &ctx(&instance, &creds),
        "acme",
        "widgets",
        "fabro/run-1",
        "head0001",
    )
    .await
    .unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn create_pull_request_posts_gitea_shape_and_parses_response() {
    let instance = instance();
    let creds = creds();
    let body = json!({
        "number": 42,
        "html_url": "https://git.example.com/acme/widgets/pulls/42",
        "title": "Fabro run",
        "state": "open",
        "head": { "ref": "fabro/run-1", "sha": "head0001" },
        "base": { "ref": "main", "sha": "base0000" },
        "user": { "login": "forgejo-bot" },
        "created_at": "2026-09-11T00:00:00Z",
        "updated_at": "2026-09-11T00:00:00Z",
    })
    .to_string();
    let client = MockHttpClient::new()
        .on(HttpMethod::Post, "/api/v1/repos/acme/widgets/pulls", 201, &body)
        .with_req_header("Authorization", "token forgejo_pat_123")
        .with_req_body(
            r#"{"title":"Fabro run","head":"fabro/run-1","base":"main","body":"body text","draft":false}"#,
        );
    let pr = create_pull_request(
        &client,
        &ctx(&instance, &creds),
        "acme",
        "widgets",
        "main",
        "fabro/run-1",
        "Fabro run",
        "body text",
        false,
    )
    .await
    .unwrap();
    assert_eq!(pr.number, 42);
    assert_eq!(pr.title, "Fabro run");
    assert_eq!(pr.html_url, "https://git.example.com/acme/widgets/pulls/42");
}

#[tokio::test]
async fn create_pull_request_maps_error_statuses() {
    let instance = instance();
    let creds = creds();

    let client = MockHttpClient::new().on(
        HttpMethod::Post,
        "/api/v1/repos/acme/widgets/pulls",
        422,
        r#"{"message":"validation error"}"#,
    );
    let err = create_pull_request(
        &client,
        &ctx(&instance, &creds),
        "acme",
        "widgets",
        "main",
        "fabro/run-1",
        "t",
        "b",
        false,
    )
    .await
    .unwrap_err();
    assert!(
        err.to_string().contains("could not be created"),
        "got: {err}"
    );

    let client = MockHttpClient::new().on(
        HttpMethod::Post,
        "/api/v1/repos/acme/missing/pulls",
        404,
        r#"{"message":"not found"}"#,
    );
    let err = create_pull_request(
        &client,
        &ctx(&instance, &creds),
        "acme",
        "missing",
        "main",
        "fabro/run-1",
        "t",
        "b",
        false,
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("not found"), "got: {err}");
}

fn pr_detail_body() -> String {
    json!({
        "number": 42,
        "title": "Fabro run",
        "body": "changes",
        "state": "open",
        "draft": false,
        "merged": false,
        "merged_at": null,
        "html_url": "https://git.example.com/acme/widgets/pulls/42",
        "user": { "login": "forgejo-bot" },
        "head": { "ref": "fabro/run-1", "sha": "head0001", "label": "acme:fabro/run-1" },
        "base": { "ref": "main", "sha": "base0000", "label": "acme:main" },
        "created_at": "2026-09-11T00:00:00Z",
        "updated_at": "2026-09-11T01:00:00Z",
    })
    .to_string()
}

/// The riskiest parsing code in the crate: a full Forgejo payload must project
/// onto the shared `PullRequestGithubDetail` the server and web render.
#[test]
fn forgejo_pull_request_maps_onto_github_detail_fixture() {
    let pr: ForgejoPullRequest =
        serde_json::from_str(&pr_detail_body()).expect("fixture should deserialize");
    let detail = pr.to_detail();

    assert_eq!(detail.number, 42);
    assert_eq!(detail.title, "Fabro run");
    assert_eq!(detail.state, "open");
    assert_eq!(detail.user.login, "forgejo-bot");
    assert_eq!(detail.head.ref_name, "fabro/run-1");
    assert_eq!(detail.base.ref_name, "main");
    assert_eq!(
        detail.html_url,
        "https://git.example.com/acme/widgets/pulls/42"
    );
    // Diff-stat fields have no Gitea-lineage equivalent in the base payload.
    assert_eq!(detail.additions, 0);
    assert_eq!(detail.deletions, 0);
    assert_eq!(detail.changed_files, 0);
    assert_eq!(detail.mergeable, None);

    // The projection must round-trip through the shared type's serde shape.
    let serialized = serde_json::to_value(&detail).unwrap();
    let reparsed: PullRequestGithubDetail = serde_json::from_value(serialized).unwrap();
    assert_eq!(reparsed, detail);
}

#[tokio::test]
async fn get_pull_request_maps_not_found_and_detail() {
    let instance = instance();
    let creds = creds();

    let client = MockHttpClient::new()
        .on(
            HttpMethod::Get,
            "/api/v1/repos/acme/widgets/pulls/42",
            200,
            &pr_detail_body(),
        )
        .with_req_header("Authorization", "token forgejo_pat_123");
    let detail = get_pull_request(&client, &ctx(&instance, &creds), "acme", "widgets", 42)
        .await
        .unwrap();
    assert_eq!(detail.head.ref_name, "fabro/run-1");

    let client = MockHttpClient::new().on(
        HttpMethod::Get,
        "/api/v1/repos/acme/widgets/pulls/43",
        404,
        "{}",
    );
    let err = get_pull_request(&client, &ctx(&instance, &creds), "acme", "widgets", 43)
        .await
        .unwrap_err();
    assert!(
        matches!(err, PullRequestApiError::NotFound { ref number, .. } if *number == 43),
        "got: {err:?}"
    );
}

#[tokio::test]
async fn merge_pull_request_posts_do_field_and_maps_statuses() {
    let instance = instance();
    let creds = creds();

    let client = MockHttpClient::new()
        .on(
            HttpMethod::Post,
            "/api/v1/repos/acme/widgets/pulls/42/merge",
            200,
            "",
        )
        .with_req_body(r#"{"Do":"squash"}"#);
    merge_pull_request(
        &client,
        &ctx(&instance, &creds),
        "acme",
        "widgets",
        42,
        MergeStrategy::Squash,
    )
    .await
    .unwrap();

    let client = MockHttpClient::new().on(
        HttpMethod::Post,
        "/api/v1/repos/acme/widgets/pulls/42/merge",
        405,
        r#"{"message":"not mergeable"}"#,
    );
    let err = merge_pull_request(
        &client,
        &ctx(&instance, &creds),
        "acme",
        "widgets",
        42,
        MergeStrategy::Merge,
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("not mergeable"), "got: {err}");

    let client = MockHttpClient::new().on(
        HttpMethod::Post,
        "/api/v1/repos/acme/widgets/pulls/42/merge",
        409,
        r#"{"message":"conflict"}"#,
    );
    let err = merge_pull_request(
        &client,
        &ctx(&instance, &creds),
        "acme",
        "widgets",
        42,
        MergeStrategy::Rebase,
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("merge conflict"), "got: {err}");
}

#[tokio::test]
async fn enable_auto_merge_sets_merge_when_checks_succeed() {
    let instance = instance();
    let creds = creds();

    let client = MockHttpClient::new()
        .on(
            HttpMethod::Post,
            "/api/v1/repos/acme/widgets/pulls/42/merge",
            200,
            "",
        )
        .with_req_body(r#"{"Do":"merge","merge_when_checks_succeed":true}"#);
    enable_auto_merge(
        &client,
        &ctx(&instance, &creds),
        "acme",
        "widgets",
        42,
        MergeStrategy::Merge,
    )
    .await
    .unwrap();

    // Servers without the feature reject the parameter; the error must tell
    // the operator auto-merge is unsupported rather than look like a bug.
    let client = MockHttpClient::new().on(
        HttpMethod::Post,
        "/api/v1/repos/acme/widgets/pulls/42/merge",
        422,
        r#"{"message":"unsupported"}"#,
    );
    let err = enable_auto_merge(
        &client,
        &ctx(&instance, &creds),
        "acme",
        "widgets",
        42,
        MergeStrategy::Merge,
    )
    .await
    .unwrap_err();
    assert!(
        err.to_string().contains("Auto-merge is not supported"),
        "got: {err}"
    );
}

#[tokio::test]
async fn close_pull_request_patches_state_closed() {
    let instance = instance();
    let creds = creds();

    let client = MockHttpClient::new()
        .on(
            HttpMethod::Patch,
            "/api/v1/repos/acme/widgets/pulls/42",
            200,
            "{}",
        )
        .with_req_body(r#"{"state":"closed"}"#);
    close_pull_request(&client, &ctx(&instance, &creds), "acme", "widgets", 42)
        .await
        .unwrap();

    let client = MockHttpClient::new().on(
        HttpMethod::Patch,
        "/api/v1/repos/acme/widgets/pulls/43",
        404,
        "{}",
    );
    let err = close_pull_request(&client, &ctx(&instance, &creds), "acme", "widgets", 43)
        .await
        .unwrap_err();
    assert!(
        matches!(err, PullRequestApiError::NotFound { .. }),
        "got: {err:?}"
    );
}

#[test]
fn credentials_and_context_redact_the_token() {
    let creds = creds();
    let rendered = format!("{creds:?}");
    assert!(!rendered.contains("forgejo_pat_123"), "leaked: {rendered}");

    let instance = instance();
    let context = ctx(&instance, &creds);
    let rendered = format!("{context:?}");
    assert!(!rendered.contains("forgejo_pat_123"), "leaked: {rendered}");
}
