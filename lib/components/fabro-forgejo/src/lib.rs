//! Forgejo (Gitea-compatible REST API v1) helpers for Fabro.
//!
//! Forgejo exposes the same REST surface as Gitea under `{instance}/api/v1`,
//! authenticated with a personal access token sent as
//! `Authorization: token <PAT>`. One instance is configured per deployment
//! (`server.integrations.forgejo.url`); the PAT lives in the vault as
//! `FORGEJO_TOKEN` and is threaded through this crate as part of
//! [`ForgejoContext`], which redacts it from `Debug` output.

use anyhow::{Context as _, anyhow, bail};
use fabro_redact::DisplaySafeUrl;
use fabro_types::settings::run::MergeStrategy;
use serde::Deserialize;

pub mod url;

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;

pub use url::{
    credential_helper, credential_helper_key, embed_token_in_url, normalize_origin_url,
    parse_owner_repo, repo_https_url,
};

/// Header scheme for Forgejo personal access tokens.
pub const TOKEN_HEADER_PREFIX: &str = "token";

/// Secret-free git credential helper for the configured Forgejo instance:
/// reads `$FORGEJO_TOKEN` from the invoking git process's environment at
/// invocation time, so the token never lands in git configuration, argv, or
/// rendered errors. Non-`get` operations (`store`, `erase`) are ignored.
pub const FORGEJO_CREDENTIAL_HELPER: &str = r#"!f() { if [ "$1" = get ]; then echo username=fabro; echo "password=$FORGEJO_TOKEN"; fi; }; f"#;

/// Bundle of the instance base URL and the PAT used against it, threaded
/// through every authenticated Forgejo call.
#[derive(Clone)]
pub struct ForgejoContext {
    token:    String,
    base_url: String,
}

impl ForgejoContext {
    /// Builds a context from a raw PAT and a normalized instance base URL
    /// (no trailing slash).
    #[must_use]
    pub fn new(token: impl Into<String>, base_url: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
        }
    }

    /// The configured instance base URL without a trailing slash.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// The PAT. Expose it only at the point of use (URL embedding, git
    /// credentials) — never in a log line.
    #[must_use]
    pub fn token(&self) -> &str {
        &self.token
    }

    /// The fully formed `Authorization` header value.
    fn auth_header(&self) -> String {
        format!("{TOKEN_HEADER_PREFIX} {}", self.token)
    }
}

impl std::fmt::Debug for ForgejoContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ForgejoContext")
            .field("token", &"<redacted>")
            .field("base_url", &self.base_url)
            .finish()
    }
}

/// HTTP method used in Forgejo API calls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
}

/// A minimal HTTP response for testability.
pub struct HttpResponse {
    pub status: u16,
    body:       String,
}

impl HttpResponse {
    pub fn new(status: u16, body: String) -> Self {
        Self { status, body }
    }

    pub fn json<T: for<'de> Deserialize<'de>>(&self) -> anyhow::Result<T> {
        serde_json::from_str(&self.body).context("Failed to parse response")
    }

    pub fn text(&self) -> &str {
        &self.body
    }
}

/// Abstract HTTP client for Forgejo API calls.
///
/// Implemented for `fabro_http::HttpClient` in production; tests use a mock
/// to avoid TCP/process overhead.
pub trait HttpClient: Send + Sync {
    fn request(
        &self,
        method: HttpMethod,
        url: &str,
        headers: &[(&str, &str)],
        body: Option<&serde_json::Value>,
    ) -> impl std::future::Future<Output = anyhow::Result<HttpResponse>> + Send;
}

impl HttpClient for fabro_http::HttpClient {
    async fn request(
        &self,
        method: HttpMethod,
        url: &str,
        headers: &[(&str, &str)],
        body: Option<&serde_json::Value>,
    ) -> anyhow::Result<HttpResponse> {
        let mut builder = match method {
            HttpMethod::Get => self.get(url),
            HttpMethod::Post => self.post(url),
            HttpMethod::Put => self.put(url),
            HttpMethod::Patch => self.patch(url),
        };
        for &(key, value) in headers {
            builder = builder.header(key, value);
        }
        if let Some(json_body) = body {
            builder = builder.json(json_body);
        }
        let resp = builder.send().await.map_err(anyhow::Error::new)?;
        let status = resp.status().as_u16();
        let text = resp.text().await.map_err(anyhow::Error::new)?;
        Ok(HttpResponse::new(status, text))
    }
}

/// Standard Forgejo API headers for authenticated requests. `auth` is the
/// fully formed `Authorization` value, borrowed from the caller.
fn forgejo_headers<'a>(auth: &'a str) -> [(&'a str, &'a str); 3] {
    [
        ("Authorization", auth),
        ("Accept", "application/json"),
        ("User-Agent", "fabro"),
    ]
}

/// Errors returned by pull-request endpoints. Callers branch on `NotFound` to
/// distinguish a missing PR from any other failure.
#[derive(Debug, thiserror::Error)]
pub enum PullRequestApiError {
    #[error("Pull request #{number} not found in {owner}/{repo}")]
    NotFound {
        owner:  String,
        repo:   String,
        number: u64,
    },
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// A created or reconciled pull request.
///
/// Forgejo has no GraphQL `node_id`; the field GitHub callers use for
/// auto-merge simply does not exist here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedPullRequest {
    pub html_url: String,
    pub number:   u64,
    pub title:    String,
}

/// Information about a repository from the authenticated repo endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ForgejoRepository {
    pub full_name:      String,
    pub private:        bool,
    pub default_branch: Option<String>,
    pub permissions:    Option<RepositoryPermissions>,
    pub owner:          Option<RepositoryOwner>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RepositoryOwner {
    pub login: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RepositoryPermissions {
    #[serde(default)]
    pub admin: bool,
    #[serde(default)]
    pub push:  bool,
    #[serde(default)]
    pub pull:  bool,
}

/// Information about the authenticated user from `GET /user`.
#[derive(Debug, Clone, Deserialize)]
pub struct ForgejoUser {
    pub login: String,
}

fn api_url(base_url: &str, path: &str) -> String {
    format!("{}/api/v1{}", base_url.trim_end_matches('/'), path)
}

/// Verifies the PAT by reading the authenticated user.
pub async fn current_user(
    client: &impl HttpClient,
    ctx: &ForgejoContext,
) -> anyhow::Result<ForgejoUser> {
    let url = api_url(ctx.base_url(), "/user");
    let auth = ctx.auth_header();
    let resp = client
        .request(HttpMethod::Get, &url, &forgejo_headers(&auth), None)
        .await
        .context("Failed to read the authenticated Forgejo user")?;
    match resp.status {
        200 => resp.json().context("Failed to parse Forgejo user response"),
        401 => bail!("Forgejo authentication failed; check that FORGEJO_TOKEN is valid"),
        status => bail!("Unexpected status {status} reading the authenticated Forgejo user"),
    }
}

/// Reads a repository, including its default branch and the token's
/// permission level.
///
/// Returns `None` when the repository does not exist (or the token cannot see
/// it).
pub async fn get_repo(
    client: &impl HttpClient,
    ctx: &ForgejoContext,
    owner: &str,
    repo: &str,
) -> anyhow::Result<Option<ForgejoRepository>> {
    let url = api_url(ctx.base_url(), &format!("/repos/{owner}/{repo}"));
    let auth = ctx.auth_header();
    let resp = client
        .request(HttpMethod::Get, &url, &forgejo_headers(&auth), None)
        .await
        .context("Failed to read the Forgejo repository")?;
    match resp.status {
        200 => Ok(Some(
            resp.json()
                .context("Failed to parse Forgejo repository response")?,
        )),
        404 => Ok(None),
        401 => bail!("Forgejo authentication failed; check that FORGEJO_TOKEN is valid"),
        status => bail!("Unexpected status {status} reading the Forgejo repository"),
    }
}

/// Return the commit SHA at the head of a Forgejo branch.
///
/// Returns `None` when the branch does not exist.
pub async fn branch_head_sha(
    client: &impl HttpClient,
    ctx: &ForgejoContext,
    owner: &str,
    repo: &str,
    branch: &str,
) -> anyhow::Result<Option<String>> {
    #[derive(Deserialize)]
    struct BranchResponse {
        commit: BranchCommit,
    }

    #[derive(Deserialize)]
    struct BranchCommit {
        id: String,
    }

    let url = api_url(
        ctx.base_url(),
        &format!("/repos/{owner}/{repo}/branches/{branch}"),
    );
    let auth = ctx.auth_header();
    let resp = client
        .request(HttpMethod::Get, &url, &forgejo_headers(&auth), None)
        .await
        .context("Failed to read remote branch head")?;
    match resp.status {
        200 => {
            let branch: BranchResponse = resp
                .json()
                .context("Failed to parse remote branch response")?;
            Ok(Some(branch.commit.id))
        }
        404 => Ok(None),
        401 => bail!("Forgejo authentication failed; check that FORGEJO_TOKEN is valid"),
        status => bail!("Unexpected status {status} reading branch '{branch}'"),
    }
}

/// Find an open pull request that already carries the expected head commit.
///
/// This supports recovery when the instance created a pull request but the
/// caller stopped before it could persist the result locally. Forgejo's list
/// endpoint filters by state only, so base/head/SHA matching happens
/// client-side.
pub async fn find_open_pull_request(
    client: &impl HttpClient,
    ctx: &ForgejoContext,
    owner: &str,
    repo: &str,
    base: &str,
    head: &str,
    expected_head_sha: &str,
) -> anyhow::Result<Option<CreatedPullRequest>> {
    #[derive(Deserialize)]
    struct PullRequestHead {
        #[serde(rename = "ref", default)]
        ref_name: Option<String>,
        #[serde(default)]
        sha: Option<String>,
    }

    #[derive(Deserialize)]
    struct PullRequestBase {
        #[serde(rename = "ref", default)]
        ref_name: Option<String>,
    }

    #[derive(Deserialize)]
    struct PullRequestListItem {
        html_url: String,
        number:   u64,
        title:    String,
        base:     PullRequestBase,
        head:     PullRequestHead,
    }

    let mut url = DisplaySafeUrl::parse(&api_url(
        ctx.base_url(),
        &format!("/repos/{owner}/{repo}/pulls"),
    ))
    .context("Failed to build pull request reconciliation URL")?;
    url.query_pairs_mut().append_pair("state", "open");
    let auth = ctx.auth_header();
    let resp = client
        .request(
            HttpMethod::Get,
            &url.raw_string(),
            &forgejo_headers(&auth),
            None,
        )
        .await
        .context("Failed to find an existing pull request")?;
    if resp.status != 200 {
        bail!(
            "Unexpected status {} finding an existing pull request: {}",
            resp.status,
            resp.text()
        );
    }

    let pull_requests = resp
        .json::<Vec<PullRequestListItem>>()
        .context("Failed to parse existing pull request response")?;
    Ok(pull_requests
        .into_iter()
        .find(|pull_request| {
            pull_request.base.ref_name.as_deref() == Some(base)
                && pull_request.head.ref_name.as_deref() == Some(head)
                && pull_request.head.sha.as_deref() == Some(expected_head_sha)
        })
        .map(|pull_request| CreatedPullRequest {
            html_url: pull_request.html_url,
            number:   pull_request.number,
            title:    pull_request.title,
        }))
}

/// Map a draft request onto Forgejo's WIP title-prefix convention: the PR
/// create endpoint has no draft flag.
pub fn draft_title_prefix(draft: bool) -> &'static str {
    if draft { "WIP: " } else { "" }
}

/// Create a pull request on the Forgejo instance.
///
/// Forgejo's create endpoint has no draft flag; a draft request is expressed
/// with the conventional `WIP: ` title prefix.
#[allow(
    clippy::too_many_arguments,
    reason = "Creating a pull request needs explicit repo, branch, and body fields."
)]
pub async fn create_pull_request(
    client: &impl HttpClient,
    ctx: &ForgejoContext,
    owner: &str,
    repo: &str,
    base: &str,
    head: &str,
    title: &str,
    body: &str,
    draft: bool,
) -> anyhow::Result<CreatedPullRequest> {
    #[derive(Deserialize)]
    struct PullRequestResponse {
        html_url: String,
        number:   u64,
    }

    let full_title = format!("{}{title}", draft_title_prefix(draft));

    tracing::info!(title = %full_title, head = %head, base = %base, draft, "Creating pull request");

    let pr_body = serde_json::json!({
        "title": full_title,
        "head": head,
        "base": base,
        "body": body,
    });

    let url = api_url(ctx.base_url(), &format!("/repos/{owner}/{repo}/pulls"));
    let auth = ctx.auth_header();
    let resp = HttpClient::request(
        client,
        HttpMethod::Post,
        &url,
        &forgejo_headers(&auth),
        Some(&pr_body),
    )
    .await
    .context("Failed to create pull request")?;

    match resp.status {
        201 => {}
        422 => {
            bail!("Pull request could not be created (422): {}", resp.text());
        }
        401 | 403 => {
            bail!(
                "Authentication failed creating pull request ({})",
                resp.status
            );
        }
        404 => {
            bail!("Pull request could not be created: repository or branch not found (404)");
        }
        status => {
            bail!(
                "Unexpected status {status} creating pull request: {}",
                resp.text()
            );
        }
    }

    let pr: PullRequestResponse = resp
        .json()
        .context("Failed to parse pull request response")?;

    Ok(CreatedPullRequest {
        html_url: pr.html_url,
        number:   pr.number,
        title:    full_title,
    })
}

/// Read one pull request with the fields Fabro displays on run detail.
pub async fn get_pull_request(
    client: &impl HttpClient,
    ctx: &ForgejoContext,
    owner: &str,
    repo: &str,
    number: u64,
) -> Result<fabro_types::PullRequestGithubDetail, PullRequestApiError> {
    #[derive(Deserialize)]
    struct PullRequestUser {
        login: String,
    }

    #[derive(Deserialize)]
    struct PullRequestRef {
        #[serde(rename = "ref")]
        ref_name: String,
    }

    #[derive(Deserialize)]
    struct PullRequestResponse {
        number:     u64,
        title:      String,
        #[serde(default)]
        body:       Option<String>,
        state:      String,
        #[serde(default)]
        draft:      bool,
        #[serde(default)]
        merged:     bool,
        #[serde(default)]
        merged_at:  Option<String>,
        #[serde(default)]
        mergeable:  bool,
        html_url:   String,
        user:       Option<PullRequestUser>,
        head:       PullRequestRef,
        base:       PullRequestRef,
        #[serde(default)]
        created_at: Option<String>,
        #[serde(default)]
        updated_at: Option<String>,
    }

    let url = api_url(
        ctx.base_url(),
        &format!("/repos/{owner}/{repo}/pulls/{number}"),
    );
    let auth = ctx.auth_header();
    let resp = client
        .request(HttpMethod::Get, &url, &forgejo_headers(&auth), None)
        .await
        .context("Failed to read pull request")?;
    match resp.status {
        200 => {
            let pr: PullRequestResponse = resp
                .json()
                .context("Failed to parse pull request response")?;
            Ok(fabro_types::PullRequestGithubDetail {
                number: pr.number,
                title: pr.title,
                body: pr.body,
                state: pr.state,
                draft: pr.draft,
                merged: pr.merged,
                merged_at: pr.merged_at,
                // Forgejo reports mergeability as a plain boolean with no
                // tri-state, and its PR payload carries no diff stats; the
                // GitHub-shaped detail projects those as absent.
                mergeable: Some(pr.mergeable),
                additions: 0,
                deletions: 0,
                changed_files: 0,
                html_url: pr.html_url,
                user: pr.user.map_or_else(
                    || fabro_types::PullRequestUser {
                        login: "unknown".to_string(),
                    },
                    |user| fabro_types::PullRequestUser { login: user.login },
                ),
                head: fabro_types::PullRequestRef {
                    ref_name: pr.head.ref_name,
                },
                base: fabro_types::PullRequestRef {
                    ref_name: pr.base.ref_name,
                },
                created_at: pr.created_at.unwrap_or_default(),
                updated_at: pr.updated_at.unwrap_or_default(),
            })
        }
        404 => Err(PullRequestApiError::NotFound {
            owner: owner.to_string(),
            repo: repo.to_string(),
            number,
        }),
        401 => Err(anyhow!(
            "Forgejo authentication failed reading pull request; check that FORGEJO_TOKEN is valid"
        )
        .into()),
        status => Err(anyhow!("Unexpected status {status} reading pull request: {}", resp.text()).into()),
    }
}

/// Forgejo merge-method values for the `Do` field of the merge endpoint.
fn merge_method_as_do(method: MergeStrategy) -> &'static str {
    match method {
        MergeStrategy::Merge => "merge",
        MergeStrategy::Squash => "squash",
        MergeStrategy::Rebase => "rebase",
    }
}

/// Merge a pull request.
pub async fn merge_pull_request(
    client: &impl HttpClient,
    ctx: &ForgejoContext,
    owner: &str,
    repo: &str,
    number: u64,
    method: MergeStrategy,
) -> Result<(), PullRequestApiError> {
    tracing::debug!(owner, repo, number, method = %method, "Merging pull request");

    let url = api_url(
        ctx.base_url(),
        &format!("/repos/{owner}/{repo}/pulls/{number}/merge"),
    );
    let body = serde_json::json!({ "Do": merge_method_as_do(method) });
    let auth = ctx.auth_header();

    let resp = client
        .request(
            HttpMethod::Post,
            &url,
            &forgejo_headers(&auth),
            Some(&body),
        )
        .await
        .context("Failed to merge pull request")?;

    match resp.status {
        200 => Ok(()),
        405 => Err(
            anyhow!("Pull request #{number} is not mergeable (method may not be allowed)").into(),
        ),
        409 => Err(anyhow!("Pull request #{number} has a merge conflict").into()),
        404 => Err(PullRequestApiError::NotFound {
            owner: owner.to_string(),
            repo: repo.to_string(),
            number,
        }),
        401 | 403 => Err(anyhow!(
            "Authentication failed merging pull request ({})",
            resp.status
        )
        .into()),
        status => Err(anyhow!(
            "Unexpected status {status} merging pull request: {}",
            resp.text()
        )
        .into()),
    }
}

/// Close a pull request.
///
/// Forgejo (like Gitea) models PR state on the underlying issue, so closing
/// goes through the issues endpoint.
pub async fn close_pull_request(
    client: &impl HttpClient,
    ctx: &ForgejoContext,
    owner: &str,
    repo: &str,
    number: u64,
) -> Result<(), PullRequestApiError> {
    tracing::debug!(owner, repo, number, "Closing pull request");

    let url = api_url(
        ctx.base_url(),
        &format!("/repos/{owner}/{repo}/issues/{number}"),
    );
    let body = serde_json::json!({ "state": "closed" });
    let auth = ctx.auth_header();

    let resp = client
        .request(
            HttpMethod::Patch,
            &url,
            &forgejo_headers(&auth),
            Some(&body),
        )
        .await
        .context("Failed to close pull request")?;

    match resp.status {
        200 | 201 => Ok(()),
        404 => Err(PullRequestApiError::NotFound {
            owner: owner.to_string(),
            repo: repo.to_string(),
            number,
        }),
        401 | 403 => Err(anyhow!(
            "Authentication failed closing pull request ({})",
            resp.status
        )
        .into()),
        status => Err(anyhow!(
            "Unexpected status {status} closing pull request: {}",
            resp.text()
        )
        .into()),
    }
}

#[expect(
    dead_code,
    reason = "kept for parity with fabro-github's lazy client helper; call sites arrive with the run-pipeline wiring"
)]
fn unused_http_client_helper() {}
