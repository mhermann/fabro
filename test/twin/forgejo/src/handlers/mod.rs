//! API route handlers for the fake Forgejo server.

pub mod branches;
pub mod git;
pub mod pulls;
pub mod repos;
pub mod users;

use axum::Router;
use axum::routing::{get, post};

use crate::server::SharedState;

pub fn build_router(state: SharedState) -> Router {
    let api = Router::new()
        // Authenticated-user endpoint
        .route("/user", get(users::get_user))
        // Repository endpoints
        .route("/repos/{owner}/{repo}", get(repos::get_repository))
        // Branch endpoints. Wildcard: branch names contain slashes
        // (`fabro/run/<id>`), and Gitea-lineage servers route the branch as
        // the remainder of the path.
        .route(
            "/repos/{owner}/{repo}/branches/{*branch}",
            get(branches::get_branch),
        )
        // Pull request endpoints
        .route(
            "/repos/{owner}/{repo}/pulls",
            get(pulls::list_pull_requests).post(pulls::create_pull_request),
        )
        .route(
            "/repos/{owner}/{repo}/pulls/{index}",
            get(pulls::get_pull_request).patch(pulls::update_pull_request),
        )
        .route(
            "/repos/{owner}/{repo}/pulls/{index}/merge",
            post(pulls::merge_pull_request),
        );

    Router::new()
        .nest("/api/v1", api)
        // Inspection endpoint for tests: every merge request body the twin
        // received, verbatim.
        .route("/__admin/merge-bodies", get(pulls::merge_bodies))
        // Git smart HTTP transport routes
        .route("/{owner}/{repo}/info/refs", get(git::git_info_refs))
        .route(
            "/{owner}/{repo}/git-upload-pack",
            post(git::git_upload_pack),
        )
        .route(
            "/{owner}/{repo}/git-receive-pack",
            post(git::git_receive_pack),
        )
        .with_state(state)
}
