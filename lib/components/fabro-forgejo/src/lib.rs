//! Forgejo/Gitea API client for Fabro.
//!
//! Mirrors the shape of [`fabro-github`] against a single configured
//! self-hosted instance: token authentication, repository and branch reads,
//! and pull-request create/find/get/merge/close. GitHub paths stay in
//! [`fabro-github`]; the two crates are deliberately independent (each owns
//! its transport abstraction and secret type).
//!
//! Compatibility target is Forgejo and Gitea's shared REST API (`/api/v1`),
//! which is stable across both; the PR endpoints here avoid Forgejo-only or
//! Gitea-only extensions.

use anyhow::{Context as _, anyhow, bail};
use fabro_redact::DisplaySafeUrl;
use fabro_static::EnvVars;
use fabro_types::settings::run::MergeStrategy;
use fabro_types::{PullRequestDetails, PullRequestTimestamps, PullRequestUser};
use serde::Deserialize;

pub use crate::token_source::SecretString;

pub mod token_source;

/// Path prefix every Forgejo/Gitea REST route hangs off.
pub const FORGEJO_API_PATH: &str = "/api/v1";

/// Secret-free git credential helper template: reads `$FORGEJO_TOKEN` from
/// the invoking git process's environment at invocation time, so the token
/// never lands in git configuration, argv, or rendered errors. Scope the
/// helper to the instance host by pairing with the
/// `credential.https://{instance_host}.helper` config key
/// ([`forgejo_credential_helper_key`]). Non-`get` operations are ignored.
pub const FORGEJO_CREDENTIAL_HELPER: &str = r#"!f() { if [ "$1" = get ]; then echo username=oauth2; echo "password=$FORGEJO_TOKEN"; fi; }; f"#;

/// The host-scoped git config key routing `{instance_host}` HTTPS
/// credentials through [`FORGEJO_CREDENTIAL_HELPER`].
#[must_use]
pub fn forgejo_credential_helper_key(instance_url: &str) -> String {
    let host = instance_host(instance_url).unwrap_or_else(|| instance_url.to_string());
    format!("credential.https://{host}.helper")
}

/// Username used when embedding a token in HTTPS git URLs. Forgejo/Gitea
/// ignore the username for token authentication; `oauth2` is their
/// documented convention.
pub const FORGEJO_TOKEN_USERNAME: &str = "oauth2";

/// Returns the configured Forgejo/Gitea instance URL from the
/// `FORGEJO_URL` environment variable, mirroring
/// [`fabro_github::github_api_base_url`]'s documented-override pattern.
#[expect(
    clippy::disallowed_methods,
    reason = "Forgejo instance exposes a documented process-env override."
)]
pub fn forgejo_instance_url() -> Option<String> {
    let value = std::env::var(EnvVars::FORGEJO_URL).ok()?;
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Bundle of Forgejo credentials and the instance base URL, threaded through
/// every authenticated Forgejo call. Mirrors [`fabro_github::GitHubContext`].
#[derive(Debug, Clone)]
pub struct ForgejoContext<'a> {
    creds:        &'a ForgejoCredentials,
    instance_url: &'a str,
    http_client:  Option<fabro_http::HttpClient>,
}

impl<'a> ForgejoContext<'a> {
    pub fn new(creds: &'a ForgejoCredentials, instance_url: &'a str) -> Self {
        Self {
            creds,
            instance_url,
            http_client: None,
        }
    }

    pub fn with_http_client(
        creds: &'a ForgejoCredentials,
        instance_url: &'a str,
        http_client: fabro_http::HttpClient,
    ) -> Self {
        Self {
            creds,
            instance_url,
            http_client: Some(http_client),
        }
    }

    fn http_client(&self) -> anyhow::Result<fabro_http::HttpClient> {
        self.http_client.clone().map_or_else(http_client, Ok)
    }

    /// The configured instance base URL.
    #[must_use]
    pub fn instance_url(&self) -> &str {
        self.instance_url
    }

    /// The instance's REST API root: `{instance}/api/v1`.
    #[must_use]
    pub fn api_url(&self) -> String {
        forgejo_api_url(self.instance_url)
    }
}

pub fn forgejo_api_url(instance_url: &str) -> String {
    format!("{}{FORGEJO_API_PATH}", instance_url.trim_end_matches('/'))
}

fn http_client() -> anyhow::Result<fabro_http::HttpClient> {
    fabro_http::http_client().map_err(Into::into)
}

/// Credentials for authenticating to a Forgejo/Gitea instance.
///
/// An enum (rather than a bare token) so a future OAuth2 flow can be added
/// without changing every call-site signature.
#[derive(Clone, Debug)]
pub enum ForgejoCredentials {
    /// Personal access token; the v1 credential.
    Pat(String),
}

impl ForgejoCredentials {
    /// Reads `FORGEJO_TOKEN` from the process environment, returning `None`
    /// when unset or blank.
    #[expect(
        clippy::disallowed_methods,
        reason = "Forgejo credentials support a documented token env source."
    )]
    pub fn from_env() -> Result<Option<Self>, String> {
        let Ok(raw) = std::env::var(EnvVars::FORGEJO_TOKEN) else {
            return Ok(None);
        };
        let token = raw.trim();
        if token.is_empty() {
            return Ok(None);
        }
        Ok(Some(Self::Pat(token.to_string())))
    }

    /// The bearer token used for API calls.
    #[must_use]
    pub fn token(&self) -> &str {
        match self {
            Self::Pat(token) => token,
        }
    }
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

/// Standard Forgejo/Gitea API headers for authenticated requests.
fn forgejo_headers(auth: &str) -> [(&str, &str); 2] {
    [("Authorization", auth), ("User-Agent", "fabro")]
}

fn token_auth_header(creds: &ForgejoCredentials) -> String {
    format!("token {}", creds.token())
}

/// HTTP method used in Forgejo/Gitea API calls.
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

/// Abstract HTTP client for Forgejo/Gitea API calls.
///
/// Implemented for [`fabro_http::HttpClient`] in production; tests use a mock
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

/// The authenticated Forgejo/Gitea user (`GET /api/v1/user`).
#[derive(Debug, Clone, Deserialize)]
pub struct ForgejoUser {
    pub id:    i64,
    pub login: String,
}

/// A Forgejo/Gitea repository (`GET /api/v1/repos/{owner}/{repo}`).
#[derive(Debug, Clone, Deserialize)]
pub struct ForgejoRepository {
    pub full_name:      String,
    pub default_branch: String,
    pub private:        bool,
    pub empty:          bool,
}

/// Validate a personal access token by fetching the authenticated user.
pub async fn validate_token(ctx: &ForgejoContext<'_>) -> anyhow::Result<ForgejoUser> {
    let client = ctx.http_client()?;
    validate_token_with_client(&client, ctx).await
}

pub async fn validate_token_with_client(
    client: &impl HttpClient,
    ctx: &ForgejoContext<'_>,
) -> anyhow::Result<ForgejoUser> {
    let url = format!("{}/user", ctx.api_url());
    let auth = token_auth_header(ctx.creds);
    let resp = client
        .request(HttpMethod::Get, &url, &forgejo_headers(&auth), None)
        .await
        .context("Failed to validate Forgejo/Gitea token")?;

    match resp.status {
        200 => resp
            .json()
            .context("Failed to parse Forgejo/Gitea user response"),
        401 | 403 => bail!(
            "Forgejo/Gitea authentication failed. \
             Check that FORGEJO_TOKEN is a valid token for {instance}.",
            instance = ctx.instance_url
        ),
        status => bail!(
            "Unexpected status {status} validating Forgejo/Gitea token: {}",
            resp.text()
        ),
    }
}

/// The instance's Forgejo/Gitea version (`GET /api/v1/version`).
///
/// Used by diagnostics and doctor to prove the instance is reachable.
pub async fn server_version(ctx: &ForgejoContext<'_>) -> anyhow::Result<String> {
    let client = ctx.http_client()?;
    server_version_with_client(&client, ctx).await
}

pub async fn server_version_with_client(
    client: &impl HttpClient,
    ctx: &ForgejoContext<'_>,
) -> anyhow::Result<String> {
    #[derive(Deserialize)]
    struct VersionResponse {
        version: String,
    }

    let url = format!("{}/version", ctx.api_url());
    let auth = token_auth_header(ctx.creds);
    let resp = client
        .request(HttpMethod::Get, &url, &forgejo_headers(&auth), None)
        .await
        .context("Failed to fetch Forgejo/Gitea version")?;

    match resp.status {
        200 => {
            let version: VersionResponse = resp
                .json()
                .context("Failed to parse Forgejo/Gitea version response")?;
            Ok(version.version)
        }
        401 | 403 => bail!(
            "Forgejo/Gitea authentication failed. \
             Check that FORGEJO_TOKEN is a valid token for {instance}.",
            instance = ctx.instance_url
        ),
        status => bail!(
            "Unexpected status {status} fetching Forgejo/Gitea version: {}",
            resp.text()
        ),
    }
}

/// Fetch a repository (`GET /api/v1/repos/{owner}/{repo}`).
pub async fn get_repository(
    ctx: &ForgejoContext<'_>,
    owner: &str,
    repo: &str,
) -> anyhow::Result<ForgejoRepository> {
    let client = ctx.http_client()?;
    get_repository_with_client(&client, ctx, owner, repo).await
}

pub async fn get_repository_with_client(
    client: &impl HttpClient,
    ctx: &ForgejoContext<'_>,
    owner: &str,
    repo: &str,
) -> anyhow::Result<ForgejoRepository> {
    let url = format!("{}/repos/{owner}/{repo}", ctx.api_url());
    let auth = token_auth_header(ctx.creds);
    let resp = client
        .request(HttpMethod::Get, &url, &forgejo_headers(&auth), None)
        .await
        .context("Failed to fetch Forgejo/Gitea repository")?;

    match resp.status {
        200 => resp
            .json()
            .context("Failed to parse Forgejo/Gitea repository response"),
        404 => bail!(
            "Repository {owner}/{repo} not found on {}",
            ctx.instance_url
        ),
        401 | 403 => bail!(
            "Forgejo/Gitea authentication failed reading {owner}/{repo} ({})",
            resp.status
        ),
        status => bail!(
            "Unexpected status {status} fetching Forgejo/Gitea repository: {}",
            resp.text()
        ),
    }
}

/// Return the commit SHA at the head of a branch.
///
/// Returns `None` when the branch does not exist.
pub async fn branch_head_sha(
    ctx: &ForgejoContext<'_>,
    owner: &str,
    repo: &str,
    branch: &str,
) -> anyhow::Result<Option<String>> {
    let client = ctx.http_client()?;
    branch_head_sha_with_client(&client, ctx, owner, repo, branch).await
}

async fn branch_head_sha_with_client(
    client: &impl HttpClient,
    ctx: &ForgejoContext<'_>,
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

    let url = format!("{}/repos/{owner}/{repo}/branches/{branch}", ctx.api_url());
    let auth = token_auth_header(ctx.creds);
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
        status => bail!("Unexpected status {status} reading branch '{branch}'"),
    }
}

/// Pull request created on, or reconciled from, the instance.
///
/// Forgejo/Gitea pull requests have no `node_id`; the number is the stable
/// identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedPullRequest {
    pub html_url: String,
    pub number:   u64,
    pub title:    String,
}

/// Page size for pull-request list pagination. The Gitea default limit is
/// small; the documented maximum is 50.
const PULL_REQUEST_LIST_LIMIT: u32 = 50;

/// How many pages of open pull requests reconciliation walks before giving
/// up. A run reconciles against its own head SHA; matching PRs live on the
/// first page in practice.
const PULL_REQUEST_LIST_MAX_PAGES: u32 = 10;

/// Find an open pull request that already carries the expected head commit.
///
/// This supports recovery when the instance created a pull request but the
/// caller stopped before it could persist the result locally. Gitea's list
/// endpoint has no `head=` filter, so matching happens client-side across
/// paginated results.
pub async fn find_open_pull_request(
    ctx: &ForgejoContext<'_>,
    owner: &str,
    repo: &str,
    expected_head_sha: &str,
) -> anyhow::Result<Option<CreatedPullRequest>> {
    let client = ctx.http_client()?;
    find_open_pull_request_with_client(&client, ctx, owner, repo, expected_head_sha).await
}

pub async fn find_open_pull_request_with_client(
    client: &impl HttpClient,
    ctx: &ForgejoContext<'_>,
    owner: &str,
    repo: &str,
    expected_head_sha: &str,
) -> anyhow::Result<Option<CreatedPullRequest>> {
    #[derive(Deserialize)]
    struct PullRequestHead {
        sha: String,
    }

    #[derive(Deserialize)]
    struct PullRequestListItem {
        number:   u64,
        title:    String,
        html_url: String,
        head:     PullRequestHead,
    }

    let auth = token_auth_header(ctx.creds);
    for page in 1..=PULL_REQUEST_LIST_MAX_PAGES {
        let mut url =
            DisplaySafeUrl::parse(&format!("{}/repos/{owner}/{repo}/pulls", ctx.api_url()))
                .context("Failed to build pull request reconciliation URL")?;
        url.query_pairs_mut()
            .append_pair("state", "open")
            .append_pair("limit", &PULL_REQUEST_LIST_LIMIT.to_string())
            .append_pair("page", &page.to_string());
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

        let pull_requests: Vec<PullRequestListItem> = resp
            .json()
            .context("Failed to parse existing pull request response")?;
        if pull_requests.is_empty() {
            return Ok(None);
        }

        if let Some(pull_request) = pull_requests
            .into_iter()
            .find(|pull_request| pull_request.head.sha == expected_head_sha)
        {
            return Ok(Some(CreatedPullRequest {
                html_url: pull_request.html_url,
                number:   pull_request.number,
                title:    pull_request.title,
            }));
        }
    }

    Ok(None)
}

/// Title prefix added when a draft pull request is requested.
///
/// Forgejo/Gitea have no `draft` flag on PR creation; the WIP prefix (their
/// default work-in-progress marker) makes the instance treat it as a draft.
pub const WORK_IN_PROGRESS_PREFIX: &str = "WIP: ";

/// Title prefixes Forgejo/Gitea treat as work in progress.
///
/// Instances can configure additional prefixes
/// (`[repository] WORK_IN_PROGRESS_PREFIXES`); Fabro always uses the default
/// `WIP: ` prefix when creating drafts and detects drafts by any of these
/// common prefixes.
pub const WORK_IN_PROGRESS_PREFIXES: &[&str] = &[
    "WIP:", "[WIP]", "WIP", "Draft:", "[Draft]", "DRAFT:", "[DRAFT]",
];

/// Whether a pull request title marks it as work in progress.
#[must_use]
pub fn is_work_in_progress_title(title: &str) -> bool {
    WORK_IN_PROGRESS_PREFIXES
        .iter()
        .any(|prefix| title.starts_with(prefix))
}

/// Create a pull request on the instance.
///
/// When `draft` is set the title is prefixed with [`WORK_IN_PROGRESS_PREFIX`]
/// because the Forgejo/Gitea creation endpoint has no draft flag.
#[allow(
    clippy::too_many_arguments,
    reason = "Creating a pull request needs explicit repo, branch, and body fields."
)]
pub async fn create_pull_request(
    ctx: &ForgejoContext<'_>,
    owner: &str,
    repo: &str,
    base: &str,
    head: &str,
    title: &str,
    body: &str,
    draft: bool,
) -> anyhow::Result<CreatedPullRequest> {
    let client = ctx.http_client()?;
    create_pull_request_with_client(&client, ctx, owner, repo, base, head, title, body, draft).await
}

#[allow(
    clippy::too_many_arguments,
    reason = "Creating a pull request needs explicit repo, branch, and body fields."
)]
pub async fn create_pull_request_with_client(
    client: &impl HttpClient,
    ctx: &ForgejoContext<'_>,
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
        number:   u64,
        title:    String,
        html_url: String,
    }

    let title = if draft && !is_work_in_progress_title(title) {
        format!("{WORK_IN_PROGRESS_PREFIX}{title}")
    } else {
        title.to_string()
    };

    tracing::info!(title = %title, head = %head, base = %base, draft, "Creating pull request");

    let pr_body = serde_json::json!({
        "title": title,
        "head": head,
        "base": base,
        "body": body,
    });

    let url = format!("{}/repos/{owner}/{repo}/pulls", ctx.api_url());
    let auth = token_auth_header(ctx.creds);
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
        409 => {
            bail!(
                "A pull request for {head} into {base} already exists (409): {}",
                resp.text()
            );
        }
        401 | 403 => {
            bail!(
                "Authentication failed creating pull request ({})",
                resp.status
            );
        }
        404 => {
            bail!(
                "Pull request could not be created (404): base or head branch not found: {}",
                resp.text()
            );
        }
        422 => {
            bail!("Pull request could not be created (422): {}", resp.text());
        }
        _ => {
            bail!(
                "Unexpected status {} creating pull request: {}",
                resp.status,
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
        title:    pr.title,
    })
}

/// Raw Forgejo/Gitea pull request payload (`GET .../pulls/{index}`).
///
/// Mirrors the API field names; use [`ForgejoPullDetail::into_details`] (or
/// the `From` conversion) to get the product-level
/// [`fabro_types::PullRequestDetails`].
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ForgejoPullDetail {
    pub number:     u64,
    pub title:      String,
    pub body:       Option<String>,
    pub state:      String,
    #[serde(default)]
    pub draft:      bool,
    #[serde(default)]
    pub merged:     bool,
    #[serde(default)]
    pub merged_at:  Option<String>,
    pub mergeable:  Option<bool>,
    pub html_url:   String,
    pub user:       ForgejoPullUser,
    pub head:       ForgejoPullRef,
    pub base:       ForgejoPullRef,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ForgejoPullUser {
    pub login: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ForgejoPullRef {
    #[serde(rename = "ref")]
    pub ref_name: String,
}

impl ForgejoPullDetail {
    /// Convert to product-level details.
    ///
    /// Forgejo/Gitea do not report per-PR diff stats, so `additions`,
    /// `deletions`, and `changed_files` are always `0`. Draft detection adds
    /// the title-prefix check because instances with customized
    /// work-in-progress prefixes report those PRs with `draft: false`.
    #[must_use]
    pub fn into_details(self) -> PullRequestDetails {
        let draft = self.draft || is_work_in_progress_title(&self.title);
        PullRequestDetails {
            title: self.title,
            body: self.body,
            state: self.state,
            draft,
            merged: self.merged,
            merged_at: self.merged_at,
            mergeable: self.mergeable,
            additions: 0,
            deletions: 0,
            changed_files: 0,
            author: PullRequestUser {
                login: self.user.login,
            },
            head_branch: self.head.ref_name,
            base_branch: self.base.ref_name,
            timestamps: PullRequestTimestamps {
                created_at: self.created_at,
                updated_at: self.updated_at,
            },
        }
    }
}

impl From<ForgejoPullDetail> for PullRequestDetails {
    fn from(detail: ForgejoPullDetail) -> Self {
        detail.into_details()
    }
}

/// Fetch detailed information about a pull request.
pub async fn get_pull_request(
    ctx: &ForgejoContext<'_>,
    owner: &str,
    repo: &str,
    number: u64,
) -> Result<PullRequestDetails, PullRequestApiError> {
    let client = ctx.http_client()?;
    get_pull_request_with_client(&client, ctx, owner, repo, number).await
}

pub async fn get_pull_request_with_client(
    client: &impl HttpClient,
    ctx: &ForgejoContext<'_>,
    owner: &str,
    repo: &str,
    number: u64,
) -> Result<PullRequestDetails, PullRequestApiError> {
    tracing::debug!(owner, repo, number, "Fetching pull request");

    let url = format!("{}/repos/{owner}/{repo}/pulls/{number}", ctx.api_url());
    let auth = token_auth_header(ctx.creds);
    let resp = client
        .request(HttpMethod::Get, &url, &forgejo_headers(&auth), None)
        .await
        .context("Failed to fetch pull request")?;

    match resp.status {
        200 => {}
        404 => {
            return Err(PullRequestApiError::NotFound {
                owner: owner.to_string(),
                repo: repo.to_string(),
                number,
            });
        }
        401 | 403 => {
            return Err(anyhow!(
                "Authentication failed fetching pull request ({})",
                resp.status
            )
            .into());
        }
        status => {
            return Err(anyhow!(
                "Unexpected status {status} fetching pull request: {}",
                resp.text()
            )
            .into());
        }
    }

    Ok(resp
        .json::<ForgejoPullDetail>()
        .context("Failed to parse pull request response")?
        .into_details())
}

/// Map a Fabro merge strategy to the Gitea merge `Do` verb.
fn merge_method_as_do_value(method: MergeStrategy) -> &'static str {
    match method {
        MergeStrategy::Merge => "merge",
        MergeStrategy::Squash => "squash",
        MergeStrategy::Rebase => "rebase",
    }
}

/// Merge a pull request.
///
/// There is no auto-merge equivalent on Forgejo/Gitea; callers that need
/// auto-merge must fail loudly before reaching this crate.
pub async fn merge_pull_request(
    ctx: &ForgejoContext<'_>,
    owner: &str,
    repo: &str,
    number: u64,
    method: MergeStrategy,
) -> Result<(), PullRequestApiError> {
    let client = ctx.http_client()?;
    merge_pull_request_with_client(&client, ctx, owner, repo, number, method).await
}

pub async fn merge_pull_request_with_client(
    client: &impl HttpClient,
    ctx: &ForgejoContext<'_>,
    owner: &str,
    repo: &str,
    number: u64,
    method: MergeStrategy,
) -> Result<(), PullRequestApiError> {
    tracing::debug!(owner, repo, number, method = %method, "Merging pull request");

    let url = format!(
        "{}/repos/{owner}/{repo}/pulls/{number}/merge",
        ctx.api_url()
    );
    let body = serde_json::json!({ "Do": merge_method_as_do_value(method) });
    let auth = token_auth_header(ctx.creds);

    let resp = client
        .request(HttpMethod::Post, &url, &forgejo_headers(&auth), Some(&body))
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
pub async fn close_pull_request(
    ctx: &ForgejoContext<'_>,
    owner: &str,
    repo: &str,
    number: u64,
) -> Result<(), PullRequestApiError> {
    let client = ctx.http_client()?;
    close_pull_request_with_client(&client, ctx, owner, repo, number).await
}

pub async fn close_pull_request_with_client(
    client: &impl HttpClient,
    ctx: &ForgejoContext<'_>,
    owner: &str,
    repo: &str,
    number: u64,
) -> Result<(), PullRequestApiError> {
    tracing::debug!(owner, repo, number, "Closing pull request");

    let url = format!("{}/repos/{owner}/{repo}/pulls/{number}", ctx.api_url());
    let body = serde_json::json!({ "state": "closed" });
    let auth = token_auth_header(ctx.creds);

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

/// Convert a Git SSH URL to HTTPS format for token-based authentication.
///
/// SSH URLs like `git@forgejo.example.com:owner/repo.git` become
/// `https://forgejo.example.com/owner/repo.git`. URLs that are already HTTPS
/// (or any other non-SSH format) are returned unchanged.
pub fn ssh_url_to_https(url: &str) -> String {
    // Match `git@<host>:<path>` (standard SSH URL format)
    if let Some(rest) = url.strip_prefix("git@") {
        if let Some((host, path)) = rest.split_once(':') {
            return format!("https://{host}/{path}");
        }
    }
    // Match `ssh://git@<host>/<path>`
    if let Some(rest) = url.strip_prefix("ssh://git@") {
        return format!("https://{rest}");
    }
    url.to_string()
}

/// Normalize a repository origin URL to a bare HTTPS web URL: SSH forms
/// become HTTPS, embedded credentials are stripped, `.git` suffixes and
/// trailing slashes are removed.
#[must_use]
pub fn normalize_origin_url(url: &str) -> String {
    let https = ssh_url_to_https(url.trim());
    let without_credentials = strip_https_credentials(&https);
    let normalized = normalize_https_host_path(&without_credentials);
    let normalized = normalized.trim_end_matches('/');
    normalized
        .strip_suffix(".git")
        .unwrap_or(normalized)
        .to_string()
}

fn strip_https_credentials(url: &str) -> String {
    let Some(rest) = url.strip_prefix("https://") else {
        return url.to_string();
    };

    match rest.split_once('@') {
        Some((before, after)) if !before.contains('/') => format!("https://{after}"),
        _ => url.to_string(),
    }
}

fn normalize_https_host_path(url: &str) -> String {
    let Some(rest) = url.strip_prefix("https://") else {
        return url.to_string();
    };

    match rest.split_once(':') {
        Some((host, path)) if !host.contains('/') && !path.starts_with('/') => {
            format!("https://{host}/{path}")
        }
        _ => url.to_string(),
    }
}

/// The `host[:port]` of an origin URL (SSH forms included), `None` when the
/// URL has no host.
#[must_use]
pub fn instance_host(url: &str) -> Option<String> {
    let normalized = normalize_origin_url(url);
    let parsed = DisplaySafeUrl::parse(&normalized).ok()?;
    let host = parsed.host_str()?;
    Some(match parsed.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    })
}

/// Whether `origin_url` points at the configured `instance_url`.
///
/// Host (and explicit port) equality is scheme-agnostic: a clone from
/// `git@host:owner/repo.git` matches an instance configured as
/// `https://host`.
#[must_use]
pub fn is_instance_origin(origin_url: &str, instance_url: &str) -> bool {
    match (instance_host(origin_url), instance_host(instance_url)) {
        (Some(origin), Some(instance)) => origin.eq_ignore_ascii_case(&instance),
        _ => false,
    }
}

/// Parse `owner` and `repo` from an origin URL that points at
/// `instance_url`.
///
/// Returns `None` when the origin does not belong to the instance or does
/// not identify exactly one `owner/repo` pair.
#[must_use]
pub fn parse_owner_repo(origin_url: &str, instance_url: &str) -> Option<(String, String)> {
    let normalized_origin = normalize_origin_url(origin_url);
    let normalized_instance = normalize_origin_url(instance_url);
    let path = normalized_origin
        .strip_prefix(&normalized_instance)
        .filter(|rest| rest.is_empty() || rest.starts_with('/'))?;
    let path = path.strip_prefix('/').unwrap_or(path);
    if path.is_empty() {
        return None;
    }

    let mut segments = path.split('/');
    let owner = segments.next().filter(|segment| !segment.is_empty())?;
    let repo = segments.next().filter(|segment| !segment.is_empty())?;
    if segments.next().is_some() {
        return None;
    }

    Some((owner.to_string(), repo.to_string()))
}

fn redacted_url_for_error(url: &str) -> String {
    DisplaySafeUrl::parse(url)
        .map_or_else(|_| "<invalid url>".to_string(), |url| url.redacted_string())
}

/// Embed a token into an HTTPS URL for authenticated git operations.
///
/// Converts `https://forgejo.example.com/owner/repo` to
/// `https://oauth2:<token>@forgejo.example.com/owner/repo`. Forgejo/Gitea
/// ignore the username for token authentication.
pub fn embed_token_in_url(url: &str, token: &str) -> anyhow::Result<DisplaySafeUrl> {
    let mut url = DisplaySafeUrl::parse(url).context("Failed to parse Forgejo/Gitea HTTPS URL")?;
    if url.scheme() != "https" && url.scheme() != "http" {
        bail!(
            "Forgejo/Gitea clone URL must use HTTP(S): {}",
            url.redacted_string()
        );
    }
    url.set_username(FORGEJO_TOKEN_USERNAME)
        .map_err(|()| anyhow!("Failed to set Forgejo/Gitea token username"))?;
    url.set_password(Some(token))
        .map_err(|()| anyhow!("Failed to set Forgejo/Gitea token password"))?;
    Ok(url)
}

/// Resolve an authenticated HTTPS URL for a repository on the instance.
///
/// Parses owner/repo from the URL and embeds the configured PAT.
pub fn resolve_authenticated_url(
    creds: &ForgejoCredentials,
    instance_url: &str,
    url: &str,
) -> anyhow::Result<DisplaySafeUrl> {
    if !is_instance_origin(url, instance_url) {
        bail!(
            "Origin {} does not point at the configured Forgejo/Gitea instance {}",
            redacted_url_for_error(url),
            instance_url
        );
    }
    embed_token_in_url(url, creds.token())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------------
    // ssh_url_to_https / normalize_origin_url
    // ---------------------------------------------------------------------

    #[test]
    fn ssh_url_to_https_handles_scp_and_ssh_forms() {
        assert_eq!(
            ssh_url_to_https("git@forgejo.example.com:acme/widgets.git"),
            "https://forgejo.example.com/acme/widgets.git"
        );
        assert_eq!(
            ssh_url_to_https("ssh://git@forgejo.example.com/acme/widgets.git"),
            "https://forgejo.example.com/acme/widgets.git"
        );
        assert_eq!(
            ssh_url_to_https("https://forgejo.example.com/acme/widgets.git"),
            "https://forgejo.example.com/acme/widgets.git"
        );
    }

    #[test]
    fn normalize_origin_url_is_host_generic() {
        assert_eq!(
            normalize_origin_url("git@forgejo.example.com:acme/widgets.git"),
            "https://forgejo.example.com/acme/widgets"
        );
        assert_eq!(
            normalize_origin_url("https://user:token@forgejo.example.com/acme/widgets.git/"),
            "https://forgejo.example.com/acme/widgets"
        );
        assert_eq!(
            normalize_origin_url("http://127.0.0.1:3000/acme/widgets"),
            "http://127.0.0.1:3000/acme/widgets"
        );
    }

    // ---------------------------------------------------------------------
    // instance_host / is_instance_origin
    // ---------------------------------------------------------------------

    #[test]
    fn instance_host_includes_explicit_port() {
        assert_eq!(
            instance_host("https://forgejo.example.com").as_deref(),
            Some("forgejo.example.com")
        );
        assert_eq!(
            instance_host("http://127.0.0.1:3000").as_deref(),
            Some("127.0.0.1:3000")
        );
        assert_eq!(instance_host("not a url"), None);
    }

    #[test]
    fn is_instance_origin_matches_host_across_schemes() {
        assert!(is_instance_origin(
            "https://forgejo.example.com/acme/widgets.git",
            "https://forgejo.example.com"
        ));
        assert!(is_instance_origin(
            "git@forgejo.example.com:acme/widgets.git",
            "https://forgejo.example.com"
        ));
        assert!(is_instance_origin(
            "http://127.0.0.1:3000/acme/widgets.git",
            "http://127.0.0.1:3000"
        ));
    }

    #[test]
    fn is_instance_origin_rejects_other_hosts_and_ports() {
        assert!(!is_instance_origin(
            "https://github.com/acme/widgets.git",
            "https://forgejo.example.com"
        ));
        assert!(!is_instance_origin(
            "http://127.0.0.1:9999/acme/widgets.git",
            "http://127.0.0.1:3000"
        ));
        assert!(!is_instance_origin(
            "not a url",
            "https://forgejo.example.com"
        ));
    }

    // ---------------------------------------------------------------------
    // parse_owner_repo
    // ---------------------------------------------------------------------

    #[test]
    fn parse_owner_repo_accepts_https_git_and_trailing_slash() {
        for origin in [
            "https://forgejo.example.com/acme/widgets.git",
            "https://forgejo.example.com/acme/widgets",
            "https://forgejo.example.com/acme/widgets/",
            "git@forgejo.example.com:acme/widgets.git",
        ] {
            let (owner, repo) = parse_owner_repo(origin, "https://forgejo.example.com")
                .unwrap_or_else(|| panic!("should parse {origin}"));
            assert_eq!(owner, "acme");
            assert_eq!(repo, "widgets");
        }
    }

    #[test]
    fn parse_owner_repo_rejects_other_hosts_and_nested_paths() {
        assert!(
            parse_owner_repo(
                "https://github.com/acme/widgets.git",
                "https://forgejo.example.com"
            )
            .is_none()
        );
        assert!(
            parse_owner_repo(
                "https://forgejo.example.com/acme/widgets/extra",
                "https://forgejo.example.com"
            )
            .is_none()
        );
        assert!(
            parse_owner_repo(
                "https://forgejo.example.com/acme",
                "https://forgejo.example.com"
            )
            .is_none()
        );
    }

    // ---------------------------------------------------------------------
    // embed_token_in_url
    // ---------------------------------------------------------------------

    #[test]
    fn embed_token_in_url_uses_oauth2_username() {
        let embedded =
            embed_token_in_url("https://forgejo.example.com/acme/widgets.git", "s3cret").unwrap();
        assert_eq!(
            embedded.as_str(),
            "https://oauth2:s3cret@forgejo.example.com/acme/widgets.git"
        );
        // The rendered form is redacted.
        assert!(!embedded.redacted_string().contains("s3cret"));
    }

    #[test]
    fn embed_token_in_url_rejects_non_http_schemes() {
        let err =
            embed_token_in_url("ftp://forgejo.example.com/acme/widgets.git", "s3cret").unwrap_err();
        assert!(err.to_string().contains("must use HTTP(S)"));
    }

    // ---------------------------------------------------------------------
    // resolve_authenticated_url
    // ---------------------------------------------------------------------

    #[test]
    fn resolve_authenticated_url_requires_instance_origin() {
        let creds = ForgejoCredentials::Pat("s3cret".to_string());
        let err = resolve_authenticated_url(
            &creds,
            "https://forgejo.example.com",
            "https://github.com/acme/widgets.git",
        )
        .unwrap_err();
        assert!(err.to_string().contains("does not point at"));

        let embedded = resolve_authenticated_url(
            &creds,
            "https://forgejo.example.com",
            "https://forgejo.example.com/acme/widgets.git",
        )
        .unwrap();
        assert_eq!(
            embedded.as_str(),
            "https://oauth2:s3cret@forgejo.example.com/acme/widgets.git"
        );
    }

    // ---------------------------------------------------------------------
    // draft title handling
    // ---------------------------------------------------------------------

    #[test]
    fn work_in_progress_prefixes_are_detected() {
        assert!(is_work_in_progress_title("WIP: add widgets"));
        assert!(is_work_in_progress_title("[WIP] add widgets"));
        assert!(is_work_in_progress_title("Draft: add widgets"));
        assert!(!is_work_in_progress_title("add widgets"));
        assert!(!is_work_in_progress_title("wip: lowercase is not a prefix"));
    }

    #[test]
    fn forgejo_pull_detail_maps_to_details_with_zero_diff_stats() {
        let detail: ForgejoPullDetail = serde_json::from_str(
            r#"{
                "number": 7,
                "title": "WIP: add widgets",
                "body": "body text",
                "state": "open",
                "merged": false,
                "mergeable": true,
                "html_url": "https://forgejo.example.com/acme/widgets/pulls/7",
                "user": {"login": "octocat"},
                "head": {"ref": "feature"},
                "base": {"ref": "main"},
                "created_at": "2026-09-10T00:00:00Z",
                "updated_at": "2026-09-10T01:00:00Z"
            }"#,
        )
        .unwrap();

        let details: PullRequestDetails = detail.into();
        assert!(details.draft);
        assert_eq!(details.additions, 0);
        assert_eq!(details.deletions, 0);
        assert_eq!(details.changed_files, 0);
        assert_eq!(details.author.login, "octocat");
        assert_eq!(details.head_branch, "feature");
        assert_eq!(details.base_branch, "main");
    }

    #[test]
    fn api_url_normalizes_trailing_slash() {
        let creds = ForgejoCredentials::Pat("t".to_string());
        let ctx = ForgejoContext::new(&creds, "https://forgejo.example.com/");
        assert_eq!(ctx.api_url(), "https://forgejo.example.com/api/v1");
        assert_eq!(
            forgejo_api_url("https://forgejo.example.com"),
            "https://forgejo.example.com/api/v1"
        );
    }
}
