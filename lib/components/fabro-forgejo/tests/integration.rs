use fabro_forgejo::{
    ForgejoContext, ForgejoCredentials, PullRequestApiError, branch_head_sha, close_pull_request,
    create_pull_request, find_open_pull_request, get_pull_request, get_repository,
    is_instance_origin, merge_pull_request, server_version, validate_token,
};
use fabro_test::{ForgejoAppState, TwinForgejo};
use fabro_types::settings::run::MergeStrategy;

const SHA_MAIN: &str = "1111111111111111111111111111111111111111";
const SHA_FEATURE: &str = "2222222222222222222222222222222222222222";

fn credentials() -> ForgejoCredentials {
    ForgejoCredentials::Pat("twin-token".to_string())
}

fn state() -> ForgejoAppState {
    let mut state = ForgejoAppState::new();
    state.register_token("twin-token", "octocat");
    state.register_token("other-token", "other-user");
    state.add_repository(
        "acme",
        "widgets",
        vec![("main", SHA_MAIN), ("feature", SHA_FEATURE)],
        "main",
        false,
    );
    state
}

#[fabro_macros::e2e_test(twin)]
async fn validate_token_reports_the_authenticated_user() {
    let twin = TwinForgejo::start(state()).await;
    let creds = credentials();
    let ctx = ForgejoContext::new(&creds, &twin.base_url);

    let user = validate_token(&ctx).await.unwrap();
    assert_eq!(user.login, "octocat");
    twin.shutdown().await;
}

#[fabro_macros::e2e_test(twin)]
async fn validate_token_rejects_unknown_tokens() {
    let twin = TwinForgejo::start(state()).await;
    let creds = ForgejoCredentials::Pat("not-a-token".to_string());
    let ctx = ForgejoContext::new(&creds, &twin.base_url);

    let error = validate_token(&ctx).await.unwrap_err();
    assert!(error.to_string().contains("authentication failed"));
    twin.shutdown().await;
}

#[fabro_macros::e2e_test(twin)]
async fn server_version_reports_the_instance_version() {
    let twin = TwinForgejo::start(state()).await;
    let creds = credentials();
    let ctx = ForgejoContext::new(&creds, &twin.base_url);

    let version = server_version(&ctx).await.unwrap();
    assert!(!version.is_empty());
    twin.shutdown().await;
}

#[fabro_macros::e2e_test(twin)]
async fn get_repository_reports_the_default_branch() {
    let twin = TwinForgejo::start(state()).await;
    let creds = credentials();
    let ctx = ForgejoContext::new(&creds, &twin.base_url);

    let repository = get_repository(&ctx, "acme", "widgets").await.unwrap();
    assert_eq!(repository.full_name, "acme/widgets");
    assert_eq!(repository.default_branch, "main");
    assert!(!repository.private);
    twin.shutdown().await;
}

#[fabro_macros::e2e_test(twin)]
async fn get_repository_is_not_found_for_unknown_repositories() {
    let twin = TwinForgejo::start(state()).await;
    let creds = credentials();
    let ctx = ForgejoContext::new(&creds, &twin.base_url);

    let error = get_repository(&ctx, "acme", "missing").await.unwrap_err();
    assert!(error.to_string().contains("not found"));
    twin.shutdown().await;
}

#[fabro_macros::e2e_test(twin)]
async fn branch_head_sha_resolves_branches_and_missing_branches() {
    let twin = TwinForgejo::start(state()).await;
    let creds = credentials();
    let ctx = ForgejoContext::new(&creds, &twin.base_url);

    let head = branch_head_sha(&ctx, "acme", "widgets", "feature")
        .await
        .unwrap();
    assert_eq!(head.as_deref(), Some(SHA_FEATURE));

    let missing = branch_head_sha(&ctx, "acme", "widgets", "nope")
        .await
        .unwrap();
    assert_eq!(missing, None);
    twin.shutdown().await;
}

#[fabro_macros::e2e_test(twin)]
async fn create_and_get_pull_request() {
    let twin = TwinForgejo::start(state()).await;
    let creds = credentials();
    let ctx = ForgejoContext::new(&creds, &twin.base_url);

    let created = create_pull_request(
        &ctx,
        "acme",
        "widgets",
        "main",
        "feature",
        "Add widgets",
        "PR body",
        false,
    )
    .await
    .unwrap();

    assert_eq!(created.title, "Add widgets");
    assert!(created.html_url.ends_with("/acme/widgets/pulls/1"));

    let details = get_pull_request(&ctx, "acme", "widgets", created.number)
        .await
        .unwrap();
    assert_eq!(details.title, "Add widgets");
    assert_eq!(details.state, "open");
    assert!(!details.draft);
    assert_eq!(details.head_branch, "feature");
    assert_eq!(details.base_branch, "main");
    assert_eq!(details.author.login, "octocat");
    // Forgejo/Gitea do not report diff stats.
    assert_eq!(details.additions, 0);
    assert_eq!(details.deletions, 0);
    assert_eq!(details.changed_files, 0);
    twin.shutdown().await;
}

#[fabro_macros::e2e_test(twin)]
async fn draft_pull_requests_use_the_work_in_progress_prefix() {
    let twin = TwinForgejo::start(state()).await;
    let creds = credentials();
    let ctx = ForgejoContext::new(&creds, &twin.base_url);

    let created = create_pull_request(
        &ctx,
        "acme",
        "widgets",
        "main",
        "feature",
        "Add widgets",
        "PR body",
        true,
    )
    .await
    .unwrap();

    assert_eq!(created.title, "WIP: Add widgets");

    let details = get_pull_request(&ctx, "acme", "widgets", created.number)
        .await
        .unwrap();
    assert!(details.draft, "the WIP title marks the PR as a draft");
    twin.shutdown().await;
}

#[fabro_macros::e2e_test(twin)]
async fn find_open_pull_request_matches_the_expected_head_sha() {
    let twin = TwinForgejo::start(state()).await;
    let creds = credentials();
    let ctx = ForgejoContext::new(&creds, &twin.base_url);

    create_pull_request(
        &ctx,
        "acme",
        "widgets",
        "main",
        "feature",
        "Add widgets",
        "PR body",
        false,
    )
    .await
    .unwrap();

    let found = find_open_pull_request(&ctx, "acme", "widgets", SHA_FEATURE)
        .await
        .unwrap();
    assert_eq!(found.map(|pull| pull.number), Some(1));

    let not_found = find_open_pull_request(&ctx, "acme", "widgets", SHA_MAIN)
        .await
        .unwrap();
    assert_eq!(not_found, None);
    twin.shutdown().await;
}

#[fabro_macros::e2e_test(twin)]
async fn find_open_pull_request_paginates_beyond_the_first_page() {
    let mut test_state = state();
    // The matching PR sits behind 51 prior open PRs, so the client must
    // follow the `page` parameter past the first 50-item response.
    for index in 0..51usize {
        let branch = format!("filler-{index}");
        let sha = format!("{index:040}");
        test_state.push_pull_request("acme", "widgets", fabro_test::ForgejoTwinPullRequest {
            number:     0,
            title:      format!("filler {index}"),
            body:       String::new(),
            state:      "open".to_string(),
            merged:     false,
            mergeable:  true,
            user_login: "octocat".to_string(),
            head_ref:   branch,
            head_sha:   sha,
            base_ref:   "main".to_string(),
            created_at: "2026-09-10T00:00:00Z".to_string(),
            updated_at: "2026-09-10T00:00:00Z".to_string(),
        });
    }
    test_state.push_pull_request("acme", "widgets", fabro_test::ForgejoTwinPullRequest {
        number:     0,
        title:      "Add widgets".to_string(),
        body:       String::new(),
        state:      "open".to_string(),
        merged:     false,
        mergeable:  true,
        user_login: "octocat".to_string(),
        head_ref:   "feature".to_string(),
        head_sha:   SHA_FEATURE.to_string(),
        base_ref:   "main".to_string(),
        created_at: "2026-09-10T00:00:00Z".to_string(),
        updated_at: "2026-09-10T00:00:00Z".to_string(),
    });

    let twin = TwinForgejo::start(test_state).await;
    let creds = credentials();
    let ctx = ForgejoContext::new(&creds, &twin.base_url);

    let found = find_open_pull_request(&ctx, "acme", "widgets", SHA_FEATURE)
        .await
        .unwrap();
    assert_eq!(found.map(|pull| pull.number), Some(52));
    twin.shutdown().await;
}

#[fabro_macros::e2e_test(twin)]
async fn merge_pull_request_accepts_every_shared_strategy() {
    for method in [
        MergeStrategy::Merge,
        MergeStrategy::Squash,
        MergeStrategy::Rebase,
    ] {
        let twin = TwinForgejo::start(state()).await;
        let creds = credentials();
        let ctx = ForgejoContext::new(&creds, &twin.base_url);

        let created = create_pull_request(
            &ctx,
            "acme",
            "widgets",
            "main",
            "feature",
            "Add widgets",
            "PR body",
            false,
        )
        .await
        .unwrap();

        merge_pull_request(&ctx, "acme", "widgets", created.number, method)
            .await
            .unwrap();

        let error = merge_pull_request(&ctx, "acme", "widgets", created.number, method)
            .await
            .unwrap_err();
        assert!(matches!(error, PullRequestApiError::Other(_)));
        twin.shutdown().await;
    }
}

#[fabro_macros::e2e_test(twin)]
async fn pull_request_endpoints_report_not_found() {
    let twin = TwinForgejo::start(state()).await;
    let creds = credentials();
    let ctx = ForgejoContext::new(&creds, &twin.base_url);

    let error = get_pull_request(&ctx, "acme", "widgets", 999)
        .await
        .unwrap_err();
    assert!(matches!(error, PullRequestApiError::NotFound {
        number: 999,
        ..
    }));

    let error = merge_pull_request(&ctx, "acme", "widgets", 999, MergeStrategy::Merge)
        .await
        .unwrap_err();
    assert!(matches!(error, PullRequestApiError::NotFound {
        number: 999,
        ..
    }));

    let error = close_pull_request(&ctx, "acme", "widgets", 999)
        .await
        .unwrap_err();
    assert!(matches!(error, PullRequestApiError::NotFound {
        number: 999,
        ..
    }));
    twin.shutdown().await;
}

#[fabro_macros::e2e_test(twin)]
async fn close_pull_request_marks_the_pr_closed() {
    let twin = TwinForgejo::start(state()).await;
    let creds = credentials();
    let ctx = ForgejoContext::new(&creds, &twin.base_url);

    let created = create_pull_request(
        &ctx,
        "acme",
        "widgets",
        "main",
        "feature",
        "Add widgets",
        "PR body",
        false,
    )
    .await
    .unwrap();

    close_pull_request(&ctx, "acme", "widgets", created.number)
        .await
        .unwrap();

    let details = get_pull_request(&ctx, "acme", "widgets", created.number)
        .await
        .unwrap();
    assert_eq!(details.state, "closed");
    twin.shutdown().await;
}

#[test]
fn instance_origin_matching_gates_forge_classification() {
    assert!(is_instance_origin(
        "https://forgejo.example.com/acme/widgets.git",
        "https://forgejo.example.com"
    ));
    assert!(!is_instance_origin(
        "https://github.com/acme/widgets.git",
        "https://forgejo.example.com"
    ));
}
