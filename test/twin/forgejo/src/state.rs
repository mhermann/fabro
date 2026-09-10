//! In-memory Forgejo/Gitea fixture state.

use std::collections::HashMap;

/// A pull request in the fake instance.
#[derive(Debug, Clone)]
pub struct PullRequest {
    pub number:     u64,
    pub title:      String,
    pub body:       String,
    pub state:      String,
    pub merged:     bool,
    pub mergeable:  bool,
    pub user_login: String,
    pub head_ref:   String,
    pub head_sha:   String,
    pub base_ref:   String,
    pub created_at: String,
    pub updated_at: String,
}

/// A repository in the fake instance.
#[derive(Debug, Clone)]
pub struct Repository {
    pub owner:          String,
    pub name:           String,
    pub default_branch: String,
    pub private:        bool,
    /// Branch name → head SHA.
    pub branches:       HashMap<String, String>,
}

/// The authenticated user a token belongs to.
#[derive(Debug, Clone)]
pub struct TokenSubject {
    pub username: String,
}

/// The running state of the fake instance.
#[derive(Debug, Default)]
pub struct AppState {
    /// The instance version reported by `/api/v1/version`.
    pub version:  String,
    /// API token → authenticated subject.
    pub tokens:   HashMap<String, TokenSubject>,
    /// `owner/name` → repository.
    pub repos:    HashMap<String, Repository>,
    /// `owner/name` → pull requests, keyed by index.
    pub pulls:    HashMap<String, Vec<PullRequest>>,
    /// Set by the test server after binding; used to render absolute
    /// `html_url`s.
    pub base_url: Option<String>,
}

const DEFAULT_VERSION: &str = "11.0.1+gitea-1.22.0";

impl AppState {
    pub fn new() -> Self {
        Self {
            version: DEFAULT_VERSION.to_string(),
            ..Self::default()
        }
    }

    /// The web base URL used in rendered `html_url`s.
    #[must_use]
    pub fn web_base_url(&self) -> &str {
        self.base_url
            .as_deref()
            .unwrap_or("https://forgejo.example.com")
    }

    /// Register an API token authenticating as `username`.
    pub fn register_token(&mut self, token: &str, username: &str) {
        self.tokens.insert(token.to_string(), TokenSubject {
            username: username.to_string(),
        });
    }

    /// Add a repository with `branch: sha` heads.
    pub fn add_repository(
        &mut self,
        owner: &str,
        name: &str,
        branches: Vec<(&str, &str)>,
        default_branch: &str,
        private: bool,
    ) {
        let full_name = format!("{owner}/{name}");
        self.repos.insert(full_name.clone(), Repository {
            owner: owner.to_string(),
            name: name.to_string(),
            default_branch: default_branch.to_string(),
            private,
            branches: branches
                .into_iter()
                .map(|(branch, sha)| (branch.to_string(), sha.to_string()))
                .collect(),
        });
        self.pulls.insert(full_name, Vec::new());
    }

    /// Overwrite the head SHA of a branch (e.g. to simulate a push).
    pub fn set_branch_sha(&mut self, owner: &str, name: &str, branch: &str, sha: &str) {
        if let Some(repo) = self.repos.get_mut(&format!("{owner}/{name}")) {
            repo.branches.insert(branch.to_string(), sha.to_string());
        }
    }

    /// Append a pull request, assigning the next index (1-based).
    pub fn push_pull_request(&mut self, owner: &str, name: &str, mut pull: PullRequest) -> u64 {
        let pulls = self.pulls.entry(format!("{owner}/{name}")).or_default();
        let number = pulls
            .iter()
            .map(|existing| existing.number)
            .max()
            .unwrap_or(0)
            + 1;
        pull.number = number;
        pulls.push(pull);
        number
    }
}
