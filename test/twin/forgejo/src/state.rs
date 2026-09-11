//! In-memory state for the fake Forgejo server.

use std::collections::HashMap;
use std::path::PathBuf;

use hmac::Mac as _;
use serde::{Deserialize, Serialize};

/// The PAT the twin accepts on every authenticated route when the state is
/// built with [`AppState::new`]. Tests can override it with
/// [`AppState::with_token`].
pub const FIXTURE_TOKEN: &str = "twin-forgejo-token";

/// Login reported by `GET /api/v1/user` for the fixture token.
pub const FIXTURE_USER_LOGIN: &str = "twin-user";

/// Login recorded as the PR author for pull requests created through the API.
pub const FIXTURE_BOT_LOGIN: &str = "twin-bot[bot]";

/// A branch in a fixture repository, with the commit SHA the branch endpoint
/// reports. Seeded with a synthetic value by [`AppState::add_repository`] and
/// replaced with the real bare-repo commit SHA when the server starts.
#[derive(Debug, Clone)]
pub struct Branch {
    pub name: String,
    pub sha:  String,
}

impl Branch {
    pub fn new(name: impl Into<String>, sha: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            sha:  sha.into(),
        }
    }
}

/// A repository in the fake.
#[derive(Debug, Clone)]
pub struct Repository {
    pub owner:          String,
    pub name:           String,
    pub branches:       Vec<Branch>,
    pub default_branch: String,
    pub private:        bool,
    pub git_dir:        Option<PathBuf>,
}

/// An auto-merge request recorded on a pull request through the merge
/// endpoint's `merge_when_checks_succeed` flag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoMerge {
    pub merge_method: String,
    pub enabled_at:   String,
}

/// A pull request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequest {
    pub index:      u64,
    pub title:      String,
    pub body:       String,
    pub state:      String,
    pub draft:      bool,
    pub merged:     bool,
    pub merged_at:  Option<String>,
    pub mergeable:  bool,
    pub html_url:   String,
    pub user_login: String,
    pub head_ref:   String,
    pub head_sha:   String,
    pub base_ref:   String,
    pub created_at: String,
    pub updated_at: String,
    pub auto_merge: Option<AutoMerge>,
}

/// A merge request body received by the merge endpoint, kept verbatim so
/// tests can assert the exact payload the client sent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeRequestBody {
    pub owner: String,
    pub repo:  String,
    pub index: u64,
    pub body:  serde_json::Value,
}

/// Central in-memory state for the fake Forgejo server.
#[derive(Debug, Clone)]
pub struct AppState {
    /// The PAT accepted on authenticated routes.
    pub token:          String,
    /// Login the fixture token maps to.
    pub user_login:     String,
    /// Instance base URL (`http://127.0.0.1:<port>`), set by
    /// [`crate::server::TestServer::start`] once the port is known; used to
    /// build PR `html_url`s in the Forgejo
    /// `{instance}/{owner}/{repo}/pulls/{n}` form.
    pub base_url:       String,
    pub repositories:   Vec<Repository>,
    pub pull_requests:  HashMap<(String, String), Vec<PullRequest>>,
    pub merge_requests: Vec<MergeRequestBody>,
    pub next_pr_index:  u64,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            token:          FIXTURE_TOKEN.to_string(),
            user_login:     FIXTURE_USER_LOGIN.to_string(),
            base_url:       String::new(),
            repositories:   Vec::new(),
            pull_requests:  HashMap::new(),
            merge_requests: Vec::new(),
            next_pr_index:  1,
        }
    }

    /// Override the PAT the twin accepts.
    #[must_use]
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = token.into();
        self
    }

    /// Override the login reported for the fixture token.
    #[must_use]
    pub fn with_user_login(mut self, login: impl Into<String>) -> Self {
        self.user_login = login.into();
        self
    }

    /// Register a fixture repository. Branches receive synthetic SHAs that
    /// are replaced with real bare-repo commit SHAs when the server starts.
    pub fn add_repository(
        &mut self,
        owner: &str,
        name: &str,
        branches: Vec<String>,
        private: bool,
    ) {
        let default_branch = branches
            .first()
            .cloned()
            .unwrap_or_else(|| "main".to_string());
        self.repositories.push(Repository {
            owner: owner.to_string(),
            name: name.to_string(),
            branches: branches
                .into_iter()
                .map(|branch| {
                    let sha = synthetic_sha(&format!("{owner}/{name}/{branch}"));
                    Branch::new(branch, sha)
                })
                .collect(),
            default_branch,
            private,
            git_dir: None,
        });
    }

    pub fn find_repository(&self, owner: &str, repo: &str) -> Option<&Repository> {
        self.repositories
            .iter()
            .find(|r| r.owner == owner && r.name == repo)
    }

    /// The commit SHA the branch endpoint reports for `branch`.
    pub fn branch_head_sha(&self, owner: &str, repo: &str, branch: &str) -> Option<&str> {
        self.find_repository(owner, repo)?
            .branches
            .iter()
            .find(|b| b.name == branch)
            .map(|b| b.sha.as_str())
    }

    pub fn pull_requests_mut(&mut self, owner: &str, repo: &str) -> Option<&mut Vec<PullRequest>> {
        self.pull_requests
            .get_mut(&(owner.to_string(), repo.to_string()))
    }

    pub fn find_pull_request(&self, owner: &str, repo: &str, index: u64) -> Option<&PullRequest> {
        self.pull_requests
            .get(&(owner.to_string(), repo.to_string()))?
            .iter()
            .find(|pr| pr.index == index)
    }

    /// Record a merge request body verbatim for later assertions.
    pub fn record_merge_request(
        &mut self,
        owner: &str,
        repo: &str,
        index: u64,
        body: serde_json::Value,
    ) {
        self.merge_requests.push(MergeRequestBody {
            owner: owner.to_string(),
            repo: repo.to_string(),
            index,
            body,
        });
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// A deterministic 40-hex-char stand-in commit SHA for branches that have not
/// been seeded into a bare git repository yet.
fn synthetic_sha(input: &str) -> String {
    let mut mac = hmac::Hmac::<sha2::Sha256>::new_from_slice(input.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(input.as_bytes());
    // Git object IDs are 40 hex chars (SHA-1); match that shape.
    hex::encode(mac.finalize().into_bytes())[..40].to_string()
}

/// Initialize a bare git repository at `{git_root}/{owner}/{repo}.git/`, seed
/// each branch with a real empty-tree commit, and point HEAD at the default
/// branch. Returns the bare repo path and the seeded branch SHAs.
pub fn init_bare_repo(
    git_root: &std::path::Path,
    owner: &str,
    repo: &str,
    branches: &[Branch],
    default_branch: &str,
) -> Result<(PathBuf, Vec<Branch>), String> {
    let repo_dir = git_root.join(owner).join(format!("{repo}.git"));
    if repo_dir.exists() {
        let seeded = branches
            .iter()
            .map(|branch| Branch::new(&branch.name, &branch.sha))
            .collect();
        return Ok((repo_dir, seeded));
    }
    std::fs::create_dir_all(&repo_dir)
        .map_err(|e| format!("failed to create git dir {}: {e}", repo_dir.display()))?;

    #[expect(
        clippy::disallowed_methods,
        reason = "This synchronous test harness helper initializes fixture git repositories with the real git CLI."
    )]
    let output = std::process::Command::new("git")
        .args(["init", "--bare"])
        .arg(&repo_dir)
        .output()
        .map_err(|e| format!("failed to run git init: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "git init --bare failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    // Enable http.receivepack so push works via git-http-backend.
    #[expect(
        clippy::disallowed_methods,
        reason = "This synchronous test harness helper configures fixture git repositories with the real git CLI."
    )]
    let output = std::process::Command::new("git")
        .args(["config", "http.receivepack", "true"])
        .current_dir(&repo_dir)
        .output()
        .map_err(|e| format!("failed to configure git repo: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "git config http.receivepack failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    // Seed each branch with a real commit so branch SHAs reported by the API
    // are fetchable and clones produce a checkable worktree.
    let mut seeded = Vec::new();
    for branch in branches {
        let sha = seed_branch_commit(&repo_dir, &branch.name)?;
        seeded.push(Branch::new(&branch.name, sha));
    }

    // Point HEAD at the default branch so clones check out something.
    #[expect(
        clippy::disallowed_methods,
        reason = "This synchronous test harness helper configures fixture git repositories with the real git CLI."
    )]
    let output = std::process::Command::new("git")
        .args([
            "symbolic-ref",
            "HEAD",
            &format!("refs/heads/{default_branch}"),
        ])
        .current_dir(&repo_dir)
        .output()
        .map_err(|e| format!("failed to set default branch: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "git symbolic-ref HEAD failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok((repo_dir, seeded))
}

/// Create an empty-tree commit on `branch` inside the bare repo and return
/// its SHA.
fn seed_branch_commit(repo_dir: &std::path::Path, branch: &str) -> Result<String, String> {
    #[expect(
        clippy::disallowed_methods,
        reason = "This synchronous test harness helper seeds fixture git commits with the real git CLI."
    )]
    let output = std::process::Command::new("git")
        .args(["mktree"])
        .stdin(std::process::Stdio::null())
        .current_dir(repo_dir)
        .output()
        .map_err(|e| format!("git mktree failed: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "git mktree failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let tree = String::from_utf8_lossy(&output.stdout).trim().to_string();

    #[expect(
        clippy::disallowed_methods,
        reason = "This synchronous test harness helper seeds fixture git commits with the real git CLI."
    )]
    let output = std::process::Command::new("git")
        .args([
            "-c",
            "user.name=twin-forgejo",
            "-c",
            "user.email=twin-forgejo@localhost",
            "commit-tree",
            &tree,
            "-m",
            &format!("initial commit on {branch}"),
        ])
        .current_dir(repo_dir)
        .output()
        .map_err(|e| format!("git commit-tree failed: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "git commit-tree failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let commit = String::from_utf8_lossy(&output.stdout).trim().to_string();

    #[expect(
        clippy::disallowed_methods,
        reason = "This synchronous test harness helper seeds fixture git commits with the real git CLI."
    )]
    let output = std::process::Command::new("git")
        .args(["update-ref", &format!("refs/heads/{branch}"), &commit])
        .current_dir(repo_dir)
        .output()
        .map_err(|e| format!("git update-ref failed: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "git update-ref failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(commit)
}

/// Initialize bare git repos for all repositories in the state, replacing
/// each branch's synthetic SHA with the seeded commit SHA.
pub fn init_git_repos(state: &mut AppState, git_root: &std::path::Path) -> Result<(), String> {
    for repo in &mut state.repositories {
        let (git_dir, seeded) = init_bare_repo(
            git_root,
            &repo.owner,
            &repo.name,
            &repo.branches,
            &repo.default_branch,
        )?;
        repo.git_dir = Some(git_dir);
        repo.branches = seeded;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_state_has_fixture_token() {
        let state = AppState::new();
        assert_eq!(state.token, FIXTURE_TOKEN);
        assert!(state.repositories.is_empty());
        assert_eq!(state.next_pr_index, 1);
    }

    #[test]
    fn repository_records_branches() {
        let mut state = AppState::new();
        state.add_repository(
            "acme",
            "widgets",
            vec!["main".to_string(), "feature".to_string()],
            false,
        );
        assert!(state.find_repository("acme", "widgets").is_some());
        assert!(state.find_repository("acme", "missing").is_none());
        let sha = state.branch_head_sha("acme", "widgets", "main").unwrap();
        assert_eq!(sha.len(), 40);
        assert!(
            state
                .branch_head_sha("acme", "widgets", "missing")
                .is_none()
        );
    }

    #[test]
    fn git_root_seeds_bare_repo_with_branches() {
        let tmp = tempfile::tempdir().unwrap();
        let mut state = AppState::new();
        state.add_repository(
            "acme",
            "widgets",
            vec!["main".to_string(), "fabro/run/1".to_string()],
            false,
        );
        init_git_repos(&mut state, tmp.path()).unwrap();

        let repo = state.find_repository("acme", "widgets").unwrap();
        let git_dir = repo.git_dir.as_ref().unwrap();
        assert!(git_dir.join("HEAD").exists());
        assert!(git_dir.join("objects").exists());

        // The seeded SHAs are real refs in the bare repo, including the
        // slashed run branch.
        for branch in &repo.branches {
            #[expect(
                clippy::disallowed_methods,
                reason = "test harness asserts fixture git state with the real git CLI"
            )]
            let output = std::process::Command::new("git")
                .args(["rev-parse", &format!("refs/heads/{}", branch.name)])
                .current_dir(git_dir)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "branch {} should exist",
                branch.name
            );
            assert_eq!(
                String::from_utf8_lossy(&output.stdout).trim(),
                branch.sha,
                "state SHA should match the seeded commit"
            );
        }
    }
}
