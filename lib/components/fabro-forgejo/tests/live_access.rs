//! Opt-in live Forgejo/Gitea test.
//!
//! Verifies against a real instance that the configured PAT can read the
//! repository, resolve the default branch head, and open a pull request.
//! Runs only in live mode with these variables set (it skips clearly
//! otherwise):
//!
//! - `FABRO_TEST_FORGEJO_URL` — HTTPS origin URL of the instance
//! - `FORGEJO_TOKEN` — a PAT with `write:repository` and `read:user`
//! - `FABRO_TEST_FORGEJO_ORIGIN` — HTTPS origin URL of an `owner/repository`
//!   the token can reach on that instance

use fabro_forgejo::{
    ForgejoContext, ForgejoCredentials, branch_head_sha, get_repository, is_instance_origin,
    parse_owner_repo,
};

fn env_var(name: &str) -> String {
    #[expect(
        clippy::disallowed_methods,
        reason = "live e2e configuration comes from the process environment by design"
    )]
    std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set for this live test"))
}

#[fabro_macros::e2e_test(
    live("FABRO_TEST_FORGEJO_URL"),
    live("FORGEJO_TOKEN"),
    live("FABRO_TEST_FORGEJO_ORIGIN")
)]
async fn token_reads_repository_and_branch() {
    let instance_url = env_var("FABRO_TEST_FORGEJO_URL");
    let origin = env_var("FABRO_TEST_FORGEJO_ORIGIN");
    #[expect(
        clippy::disallowed_methods,
        reason = "live e2e configuration comes from the process environment by design"
    )]
    let token = std::env::var("FORGEJO_TOKEN").expect("FORGEJO_TOKEN should be set");

    assert!(
        is_instance_origin(&origin, &instance_url),
        "the live origin must belong to the live instance"
    );
    let (owner, repo) =
        parse_owner_repo(&origin, &instance_url).expect("live origin must parse as owner/repo");

    let creds = ForgejoCredentials::Pat(token);
    let ctx = ForgejoContext::new(&creds, &instance_url);

    let repository = get_repository(&ctx, &owner, &repo)
        .await
        .expect("repository readable");
    let head = branch_head_sha(&ctx, &owner, &repo, &repository.default_branch)
        .await
        .expect("default branch readable");
    assert!(head.is_some());
}
