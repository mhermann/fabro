//! Opt-in live Forgejo test. Verifies against a real instance that the PAT
//! authenticates and can read a repository. Runs only in live mode with
//! these variables set:
//!
//! - `FABRO_FORGEJO_URL` — instance base URL (e.g. `http://localhost:3001`)
//! - `FABRO_FORGEJO_TOKEN` — a personal access token with repository read
//! - `FABRO_FORGEJO_REPO_OWNER` / `FABRO_FORGEJO_REPO_NAME` — a repository
//!   the token can see

use fabro_forgejo::{ForgejoContext, current_user, get_repo};

fn env_var(name: &str) -> String {
    #[expect(
        clippy::disallowed_methods,
        reason = "live e2e configuration comes from the process environment by design"
    )]
    std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set for this live test"))
}

#[fabro_macros::e2e_test(live("FABRO_FORGEJO_TOKEN"))]
async fn token_reads_the_instance_and_repository() {
    let base_url = env_var("FABRO_FORGEJO_URL");
    let token = env_var("FABRO_FORGEJO_TOKEN");
    let owner = env_var("FABRO_FORGEJO_REPO_OWNER");
    let repo = env_var("FABRO_FORGEJO_REPO_NAME");
    let ctx = ForgejoContext::new(token, base_url);
    let client = fabro_http::http_client().expect("test HTTP client should build");

    let user = current_user(&client, &ctx)
        .await
        .expect("token should authenticate");
    assert!(!user.login.is_empty());

    let repository = get_repo(&client, &ctx, &owner, &repo)
        .await
        .expect("repository read should succeed")
        .expect("repository should exist");
    assert!(!repository.full_name.is_empty());
}
