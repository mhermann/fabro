//! Forgejo API client for Fabro.
//!
//! Forgejo is self-hostable, so there is no canonical host: the instance base
//! URL is configuration (server settings or `FORGEJO_URL`), and every
//! endpoint hangs off `{base_url}/api/v1`. Authentication is a scoped API
//! token (Forgejo has no GitHub-App equivalent) sent as
//! `Authorization: token <token>`.

use anyhow::{Context as _, bail};
use fabro_redact::DisplaySafeUrl;
use fabro_static::EnvVars;
use fabro_types::settings::run::MergeStrategy;
use serde::Deserialize;

#[cfg(test)]
pub(crate) mod tests_mock;

/// Path prefix of the Forgejo REST API under the instance base URL.
pub const FORGEJO_API_PATH: &str = "/api/v1";

/// Secret-free git credential helper: reads `$FORGEJO_TOKEN` from the
/// invoking git process's environment at invocation time, so the token never
/// lands in git configuration, argv, or rendered errors. Non-`get`
/// operations (`store`, `erase`) are ignored. Forgejo accepts basic auth with
/// any username and the token as the password.
pub const FORGEJO_CREDENTIAL_HELPER: &str = r#"!f() { if [ "$1" = get ]; then echo username=x-access-token; echo "password=$FORGEJO_TOKEN"; fi; }; f"#;

/// Git config key that routes HTTPS credentials for `host` through
/// [`FORGEJO_CREDENTIAL_HELPER`]. Forgejo is self-hosted, so the key scopes
/// the configured instance host rather than one fixed domain.
#[must_use]
pub fn credential_helper_key(base_url: &str) -> String {
    let host = DisplaySafeUrl::parse(base_url)
        .ok()
        .and_then(|url| {
            Some(match url.port() {
                Some(port) => format!("{}:{port}", url.host_str()?),
                None => url.host_str()?.to_string(),
            })
        })
        .unwrap_or_else(|| base_url.to_string());
    format!("credential.https://{host}.helper")
}

/// Credentials for authenticating against a Forgejo instance.
///
/// Forgejo has no App/installation-token machinery; a scoped API token is the
/// only supported credential kind, kept as an enum so later credential kinds
/// do not re-shape every call site.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ForgejoCredentials {
    Pat(String),
}

impl ForgejoCredentials {
    /// The API token these credentials resolve to.
    #[must_use]
    pub fn valid_token(&self) -> &str {
        match self {
            Self::Pat(token) => token,
        }
    }
}

/// Resolve the configured Forgejo instance base URL: an explicit value wins,
/// otherwise `FORGEJO_URL` is consulted. The URL is normalized to
/// `https://host[/path]` with credentials, a trailing slash, and any `.git`
/// suffix removed. Returns `None` when no instance is configured.
#[must_use]
pub fn forgejo_base_url(explicit: Option<&str>) -> Option<String> {
    let candidate = explicit
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map_or_else(
            #[expect(
                clippy::disallowed_methods,
                reason = "Forgejo instance URL supports a documented process-env source."
            )]
            || std::env::var(EnvVars::FORGEJO_URL).ok(),
            |url| Some(url.to_string()),
        )?;
    let normalized = normalize_repo_origin_url(&candidate);
    (!normalized.is_empty()).then_some(normalized)
}

/// Bundle of Forgejo credentials and the instance base URL, threaded through
/// every authenticated Forgejo call. Mirrors [`fabro_github`]'s context
/// shape so workflow/server call sites can hold either provider uniformly.
#[derive(Debug, Clone)]
pub struct ForgejoContext<'a> {
    creds:       &'a ForgejoCredentials,
    base_url:    &'a str,
    http_client: Option<fabro_http::HttpClient>,
}

impl<'a> ForgejoContext<'a> {
    pub fn new(creds: &'a ForgejoCredentials, base_url: &'a str) -> Self {
        Self {
            creds,
            base_url,
            http_client: None,
        }
    }

    pub fn with_http_client(
        creds: &'a ForgejoCredentials,
        base_url: &'a str,
        http_client: fabro_http::HttpClient,
    ) -> Self {
        Self {
            creds,
            base_url,
            http_client: Some(http_client),
        }
    }

    #[must_use]
    pub fn creds(&self) -> &ForgejoCredentials {
        self.creds
    }

    #[must_use]
    pub fn base_url(&self) -> &str {
        self.base_url
    }

    /// Absolute URL for an API endpoint under this instance.
    #[must_use]
    pub fn api_url(&self, path: &str) -> String {
        format!(
            "{}{FORGEJO_API_PATH}{path}",
            self.base_url.trim_end_matches('/')
        )
    }

    fn http_client(&self) -> anyhow::Result<fabro_http::HttpClient> {
        self.http_client.clone().map_or_else(http_client, Ok)
    }
}

/// Errors returned by pull-request endpoints. Callers branch on `NotFound` to
/// distinguish a missing PR from any other failure. Mirrors the shape of
/// `fabro_github::PullRequestApiError`.
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

fn http_client() -> anyhow::Result<fabro_http::HttpClient> {
    fabro_http::http_client().map_err(Into::into)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Patch,
}

pub struct HttpResponse {
    pub status: u16,
    body:       String,
}

impl HttpResponse {
    pub fn new(status: u16, body: String) -> Self {
        Self { status, body }
    }

    pub fn text(&self) -> &str {
        &self.body
    }

    pub fn json<T: for<'de> Deserialize<'de>>(&self) -> anyhow::Result<T> {
        serde_json::from_str(&self.body).map_err(anyhow::Error::new)
    }
}

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

/// Standard Forgejo API headers. Forgejo authenticates API tokens with the
/// `token` scheme (`Bearer` is accepted by newer versions but `token` works
/// across all supported releases).
fn forgejo_headers(auth: &str) -> [(&str, &str); 2] {
    [("Authorization", auth), ("User-Agent", "fabro")]
}

/// `Authorization` header value for an API token.
fn forgejo_auth(creds: &ForgejoCredentials) -> String {
    format!("token {}", creds.valid_token())
}

/// Parse `owner` and `repo` from an HTTPS URL on the configured instance.
///
/// Accepts URLs like:
/// - `https://forgejo.example.com/owner/repo.git`
/// - `https://forgejo.example.com/owner/repo`
/// - `https://forgejo.example.com/owner/repo/`
/// - `https://x-access-token:TOKEN@forgejo.example.com/owner/repo.git`
/// - subpath installs: `https://example.com/forgejo/owner/repo.git`
///
/// The URL host (and base path, for subpath installs) must match
/// `base_url`; URLs for any other host are rejected so one configured
/// instance cannot silently clone from another.
pub fn parse_forgejo_owner_repo(url: &str, base_url: &str) -> anyhow::Result<(String, String)> {
    let normalized = normalize_repo_origin_url(url);
    let normalized_base = normalize_repo_origin_url(base_url);
    let display_url = redacted_url_for_error(&normalized);

    let expected_prefix = format!("{normalized_base}/");
    let path = normalized.strip_prefix(&expected_prefix).ok_or_else(|| {
        anyhow::anyhow!(
            "Not a URL on the configured Forgejo instance ({normalized_base}): {display_url}"
        )
    })?;

    let mut parts = path.split('/');
    let owner = parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Missing owner in Forgejo URL: {display_url}"))?;
    let repo = parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Missing repo in Forgejo URL: {display_url}"))?;
    if parts.next().is_some() {
        bail!("Forgejo URL must identify one repository: {display_url}");
    }

    Ok((owner.to_string(), repo.to_string()))
}

fn redacted_url_for_error(url: &str) -> String {
    DisplaySafeUrl::parse(url)
        .map_or_else(|_| "<invalid url>".to_string(), |url| url.redacted_string())
}

/// Convert `git@host:path` / `ssh://git@host/path` clone URLs to HTTPS.
/// Host-generic: the Forgejo instance is configuration, not a fixed domain.
#[must_use]
pub fn ssh_url_to_https(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("git@") {
        if let Some((host, path)) = rest.split_once(':') {
            return format!("https://{host}/{path}");
        }
    }
    if let Some(rest) = url.strip_prefix("ssh://git@") {
        return format!("https://{rest}");
    }
    url.to_string()
}

/// Normalize a repository origin URL to a credential-free HTTPS URL with no
/// trailing slash or `.git` suffix.
#[must_use]
pub fn normalize_repo_origin_url(url: &str) -> String {
    let https = ssh_url_to_https(url.trim());
    let without_credentials = strip_https_credentials(&https);
    let without_credentials = without_credentials.trim_end_matches('/');
    without_credentials
        .strip_suffix(".git")
        .unwrap_or(without_credentials)
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

/// Embed a token into an HTTPS URL for authenticated git operations.
///
/// Converts `https://host/owner/repo` to
/// `https://x-access-token:<token>@host/owner/repo`. Forgejo accepts basic
/// auth with any username and the token as the password.
pub fn embed_token_in_url(url: &str, token: &str) -> anyhow::Result<DisplaySafeUrl> {
    let mut url = DisplaySafeUrl::parse(url).context("Failed to parse Forgejo HTTPS URL")?;
    if url.scheme() != "https" {
        bail!(
            "Forgejo clone URL must use HTTPS: {}",
            url.redacted_string()
        );
    }
    url.set_username("x-access-token")
        .map_err(|()| anyhow::anyhow!("Failed to set Forgejo token username"))?;
    url.set_password(Some(token))
        .map_err(|()| anyhow::anyhow!("Failed to set Forgejo token password"))?;
    Ok(url)
}

/// Summary of the authenticated user; proves the configured token works.
#[derive(Debug, Clone, Deserialize)]
pub struct UserInfo {
    pub login: String,
}

/// Repository metadata for preflight repo-access checks.
#[derive(Debug, Clone, Deserialize)]
pub struct RepositoryInfo {
    pub full_name:      String,
    pub default_branch: Option<String>,
    pub private:        Option<bool>,
    pub html_url:       Option<String>,
}

/// Validate the configured token against the instance.
pub async fn get_authenticated_user(ctx: &ForgejoContext<'_>) -> anyhow::Result<UserInfo> {
    let client = ctx.http_client()?;
    get_authenticated_user_with_client(&client, ctx).await
}

pub async fn get_authenticated_user_with_client(
    client: &impl HttpClient,
    ctx: &ForgejoContext<'_>,
) -> anyhow::Result<UserInfo> {
    let url = ctx.api_url("/user");
    let auth = forgejo_auth(ctx.creds);
    let resp = client
        .request(HttpMethod::Get, &url, &forgejo_headers(&auth), None)
        .await
        .context("Failed to fetch the authenticated Forgejo user")?;
    match resp.status {
        200 => resp
            .json::<UserInfo>()
            .context("Failed to parse Forgejo user response"),
        401 => bail!(
            "Forgejo token rejected by {} -- check FORGEJO_TOKEN",
            ctx.base_url
        ),
        status => bail!(
            "Unexpected status {status} fetching the authenticated Forgejo user: {}",
            resp.text()
        ),
    }
}

/// Fetch repository metadata from the instance.
pub async fn get_repo(
    ctx: &ForgejoContext<'_>,
    owner: &str,
    repo: &str,
) -> anyhow::Result<RepositoryInfo> {
    let client = ctx.http_client()?;
    get_repo_with_client(&client, ctx, owner, repo).await
}

pub async fn get_repo_with_client(
    client: &impl HttpClient,
    ctx: &ForgejoContext<'_>,
    owner: &str,
    repo: &str,
) -> anyhow::Result<RepositoryInfo> {
    let url = ctx.api_url(&format!("/repos/{owner}/{repo}"));
    let auth = forgejo_auth(ctx.creds);
    let resp = client
        .request(HttpMethod::Get, &url, &forgejo_headers(&auth), None)
        .await
        .context("Failed to fetch Forgejo repository")?;
    match resp.status {
        200 => resp
            .json::<RepositoryInfo>()
            .context("Failed to parse Forgejo repository response"),
        404 => bail!("Repository {owner}/{repo} not found on {}", ctx.base_url),
        401 => bail!(
            "Forgejo token rejected by {} -- check FORGEJO_TOKEN",
            ctx.base_url
        ),
        status => bail!(
            "Unexpected status {status} fetching Forgejo repository: {}",
            resp.text()
        ),
    }
}

/// Return the commit SHA at the head of a branch on the instance.
///
/// Returns `None` when the branch does not exist. Forgejo reports the commit
/// under `commit.id` (GitHub uses `commit.sha`).
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

    let url = ctx.api_url(&format!("/repos/{owner}/{repo}/branches/{branch}"));
    let auth = forgejo_auth(ctx.creds);
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

/// Pull request created on, or reconciled from, Forgejo.
///
/// Forgejo has no GraphQL layer, so there is no `node_id` here; the GitHub
/// auto-merge flow cannot run against a Forgejo pull request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedPullRequest {
    pub html_url: String,
    pub number:   u64,
    pub title:    String,
}

/// Find an open pull request that already carries the expected head commit.
///
/// This supports recovery when the instance created a pull request but the
/// caller stopped before it could persist the result locally. Forgejo's list
/// endpoint has no `base`/`head` query filters (unlike GitHub's), so this
/// fetches every open pull request and filters client-side; acceptable
/// because the reconciliation path runs at most once per run, but a busy
/// repository pays an O(open pull requests) listing.
pub async fn find_open_pull_request(
    ctx: &ForgejoContext<'_>,
    owner: &str,
    repo: &str,
    base: &str,
    expected_head_sha: &str,
) -> anyhow::Result<Option<CreatedPullRequest>> {
    let client = ctx.http_client()?;
    find_open_pull_request_with_client(&client, ctx, owner, repo, base, expected_head_sha).await
}

pub async fn find_open_pull_request_with_client(
    client: &impl HttpClient,
    ctx: &ForgejoContext<'_>,
    owner: &str,
    repo: &str,
    base: &str,
    expected_head_sha: &str,
) -> anyhow::Result<Option<CreatedPullRequest>> {
    #[derive(Deserialize)]
    struct PullRequestHead {
        sha: String,
    }

    #[derive(Deserialize)]
    struct PullRequestBase {
        #[serde(rename = "ref")]
        ref_name: String,
    }

    #[derive(Deserialize)]
    struct PullRequestListItem {
        html_url: String,
        number:   u64,
        title:    String,
        head:     PullRequestHead,
        base:     PullRequestBase,
    }

    let mut url = DisplaySafeUrl::parse(&ctx.api_url(&format!("/repos/{owner}/{repo}/pulls")))
        .context("Failed to build pull request reconciliation URL")?;
    url.query_pairs_mut().append_pair("state", "open");
    let auth = forgejo_auth(ctx.creds);
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
            pull_request.head.sha == expected_head_sha && pull_request.base.ref_name == base
        })
        .map(|pull_request| CreatedPullRequest {
            html_url: pull_request.html_url,
            number:   pull_request.number,
            title:    pull_request.title,
        }))
}

/// Forgejo marks drafts through the `WIP:` title convention rather than a
/// payload field. These prefixes mirror the instance-side detection so
/// Fabro-created drafts are recognized by both.
fn has_wip_prefix(title: &str) -> bool {
    title.starts_with("WIP:") || title.starts_with("[WIP]") || title.starts_with("WIP] ")
}

fn wip_title(title: &str) -> String {
    if has_wip_prefix(title) {
        title.to_string()
    } else {
        format!("WIP: {title}")
    }
}

/// Create a pull request on the instance.
///
/// Forgejo's create-pull-request payload has no `draft` field; a draft is
/// requested by prefixing the title with `WIP:` when it is not already a WIP
/// title.
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
        html_url: String,
        number:   u64,
    }

    let title = if draft {
        wip_title(title)
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

    let url = ctx.api_url(&format!("/repos/{owner}/{repo}/pulls"));
    let auth = forgejo_auth(ctx.creds);
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
        number: pr.number,
        title,
    })
}

/// Map Fabro's merge strategy onto Forgejo's `Do` merge action value.
#[must_use]
pub fn merge_do_value(method: MergeStrategy) -> &'static str {
    match method {
        MergeStrategy::Merge => "merge",
        MergeStrategy::Squash => "squash",
        MergeStrategy::Rebase => "rebase",
    }
}

/// Merge a pull request on the instance.
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

    let url = ctx.api_url(&format!("/repos/{owner}/{repo}/pulls/{number}/merge"));
    // Forgejo names the merge action field `Do` (capital D), unlike GitHub's
    // `merge_method`.
    let body = serde_json::json!({ "Do": merge_do_value(method) });

    let auth = forgejo_auth(ctx.creds);
    let resp = client
        .request(HttpMethod::Post, &url, &forgejo_headers(&auth), Some(&body))
        .await
        .context("Failed to merge pull request")?;

    match resp.status {
        200 => Ok(()),
        405 => Err(anyhow::anyhow!(
            "Pull request #{number} is not mergeable (method may not be allowed)"
        )
        .into()),
        409 => Err(anyhow::anyhow!("Pull request #{number} has a merge conflict").into()),
        404 => Err(PullRequestApiError::NotFound {
            owner: owner.to_string(),
            repo: repo.to_string(),
            number,
        }),
        401 | 403 => Err(anyhow::anyhow!(
            "Authentication failed merging pull request ({})",
            resp.status
        )
        .into()),
        status => Err(anyhow::anyhow!(
            "Unexpected status {status} merging pull request: {}",
            resp.text()
        )
        .into()),
    }
}

/// Close a pull request on the instance.
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

    let url = ctx.api_url(&format!("/repos/{owner}/{repo}/pulls/{number}"));
    let body = serde_json::json!({ "state": "closed" });

    let auth = forgejo_auth(ctx.creds);
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
        // Forgejo's edit-pull-request endpoint answers 201 (GitHub answers
        // 200); both mean the state change landed.
        200 | 201 => Ok(()),
        404 => Err(PullRequestApiError::NotFound {
            owner: owner.to_string(),
            repo: repo.to_string(),
            number,
        }),
        401 | 403 => Err(anyhow::anyhow!(
            "Authentication failed closing pull request ({})",
            resp.status
        )
        .into()),
        status => Err(anyhow::anyhow!(
            "Unexpected status {status} closing pull request: {}",
            resp.text()
        )
        .into()),
    }
}

/// Fields mirrored from Forgejo's pull request payload.
///
/// Forgejo's pull request object does not carry diff counters
/// (`additions`/`deletions`/`changed_files`) or a dedicated `merged_at`
/// timestamp on every release; those map to neutral defaults in
/// [`PullRequestDetails`].
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PullRequestForgejoDetail {
    pub number:     u64,
    pub title:      String,
    pub body:       Option<String>,
    pub state:      String,
    pub html_url:   String,
    pub user:       ForgejoUser,
    pub head:       ForgejoPullRequestRef,
    pub base:       ForgejoPullRequestRef,
    #[serde(default)]
    pub merged:     bool,
    #[serde(default)]
    pub mergeable:  Option<bool>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ForgejoUser {
    pub login: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ForgejoPullRequestRef {
    #[serde(rename = "ref")]
    pub ref_name: String,
}

impl From<PullRequestForgejoDetail> for fabro_types::PullRequestDetails {
    fn from(detail: PullRequestForgejoDetail) -> Self {
        // Forgejo marks drafts through the WIP title convention, not a
        // payload field.
        let draft = has_wip_prefix(&detail.title);
        Self {
            title: detail.title,
            body: detail.body,
            state: detail.state,
            draft,
            merged: detail.merged,
            merged_at: None,
            mergeable: detail.mergeable,
            // Forgejo's pull request payload has no diff counters.
            additions: 0,
            deletions: 0,
            changed_files: 0,
            author: fabro_types::PullRequestUser {
                login: detail.user.login,
            },
            head_branch: detail.head.ref_name,
            base_branch: detail.base.ref_name,
            timestamps: fabro_types::PullRequestTimestamps {
                created_at: detail.created_at,
                updated_at: detail.updated_at,
            },
        }
    }
}

/// Fetch detailed information about a pull request.
pub async fn get_pull_request(
    ctx: &ForgejoContext<'_>,
    owner: &str,
    repo: &str,
    number: u64,
) -> Result<PullRequestForgejoDetail, PullRequestApiError> {
    let client = ctx.http_client()?;
    get_pull_request_with_client(&client, ctx, owner, repo, number).await
}

pub async fn get_pull_request_with_client(
    client: &impl HttpClient,
    ctx: &ForgejoContext<'_>,
    owner: &str,
    repo: &str,
    number: u64,
) -> Result<PullRequestForgejoDetail, PullRequestApiError> {
    tracing::debug!(owner, repo, number, "Fetching pull request");

    let url = ctx.api_url(&format!("/repos/{owner}/{repo}/pulls/{number}"));
    let auth = forgejo_auth(ctx.creds);
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
            return Err(anyhow::anyhow!(
                "Authentication failed fetching pull request ({})",
                resp.status
            )
            .into());
        }
        status => {
            return Err(anyhow::anyhow!(
                "Unexpected status {status} fetching pull request: {}",
                resp.text()
            )
            .into());
        }
    }

    Ok(resp
        .json::<PullRequestForgejoDetail>()
        .context("Failed to parse pull request response")?)
}

#[cfg(test)]
mod tests;
