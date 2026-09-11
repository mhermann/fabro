//! Forgejo (Gitea-lineage) API client and origin helpers.
//!
//! Mirrors the product surface [`fabro_github`] provides — token auth,
//! repository/branch reads, and the pull-request lifecycle — adapted to
//! Forgejo's simpler PAT model and its `/{owner}/{repo}/pulls/{index}` REST
//! shape. Instances are self-hosted, so every helper takes the configured
//! [`ForgejoInstance`] instead of assuming a fixed host.

use std::fmt;

use anyhow::{Context as _, anyhow, bail};
use fabro_redact::DisplaySafeUrl;
use fabro_static::EnvVars;
use fabro_types::PullRequestGithubDetail;
use fabro_types::settings::run::MergeStrategy;
use serde::{Deserialize, Serialize};

/// API path prefix every Forgejo instance exposes under its base URL.
pub const FORGEJO_API_BASE_SUFFIX: &str = "/api/v1";

/// Secret-free git credential helper: reads `$FORGEJO_TOKEN` from the
/// invoking git process's environment at invocation time, so the token never
/// lands in git configuration, argv, or rendered errors. Non-`get`
/// operations (`store`, `erase`) are ignored. The Forgejo counterpart of
/// [`fabro_github`]'s helper, shared by the runtime git bridge and preflight
/// probes.
pub const FORGEJO_CREDENTIAL_HELPER: &str = r#"!f() { if [ "$1" = get ]; then echo username=x-access-token; echo "password=$FORGEJO_TOKEN"; fi; }; f"#;

/// Username embedded in authenticated HTTPS clone URLs. Forgejo PATs
/// authenticate over HTTP basic auth with any non-empty username; the fixed
/// marker keeps redaction and debug output stable.
pub const FORGEJO_TOKEN_USERNAME: &str = "x-access-token";

/// Git config key that routes `instance` HTTPS credentials through
/// [`FORGEJO_CREDENTIAL_HELPER`]. The key is instance-scoped because the
/// helper must only answer for the configured instance.
#[must_use]
pub fn forgejo_credential_helper_key(instance: &ForgejoInstance) -> String {
    format!("credential.{}.helper", instance.as_str())
}

/// A token secret that never appears in `Debug` output. Call
/// [`SecretString::expose`] at the point of use (URL embedding, git
/// credentials) — never in a log line.
#[derive(Clone)]
pub struct SecretString(String);

impl SecretString {
    #[must_use]
    pub fn new(secret: String) -> Self {
        Self(secret)
    }

    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretString(<redacted>)")
    }
}

/// Errors from [`ForgejoInstance::new`]. Callers surface `NotHttps` with
/// actionable guidance because plain-HTTP instances would ship the PAT in
/// cleartext.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ForgejoInstanceError {
    #[error("Forgejo instance URL must use HTTPS: {0}")]
    NotHttps(String),
    #[error("Forgejo instance URL must not embed credentials: {0}")]
    Credentials(String),
    #[error("Forgejo instance URL must not contain a query or fragment: {0}")]
    QueryOrFragment(String),
    #[error("Invalid Forgejo instance URL `{0}`: {1}")]
    Invalid(String, String),
}

/// A validated Forgejo instance base URL (`https://git.example.com`, with an
/// optional subpath). Single instance per server; `owner/repo` slugs are
/// scoped to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgejoInstance {
    base_url: String,
    host:     String,
}

impl ForgejoInstance {
    /// Validate and normalize an instance base URL.
    ///
    /// Requires HTTPS, rejects embedded credentials and query/fragment
    /// components, strips a trailing slash, and keeps any subpath
    /// (`https://git.example.com/forge` is valid).
    pub fn new(url: &str) -> Result<Self, ForgejoInstanceError> {
        Self::parse(url, true)
    }

    /// Parse an instance URL for tests, allowing plain HTTP so the client can
    /// target an in-process twin server on loopback. Gated behind the
    /// `test-support` feature so production builds keep the HTTPS guarantee;
    /// every other validation rule is identical to [`Self::new`].
    #[cfg(feature = "test-support")]
    pub fn new_allowing_http(url: &str) -> Result<Self, ForgejoInstanceError> {
        Self::parse(url, false)
    }

    fn parse(url: &str, require_https: bool) -> Result<Self, ForgejoInstanceError> {
        let trimmed = url.trim();
        let parsed = DisplaySafeUrl::parse(trimmed)
            .map_err(|err| ForgejoInstanceError::Invalid(trimmed.to_string(), err.to_string()))?;
        if require_https && parsed.scheme() != "https" {
            return Err(ForgejoInstanceError::NotHttps(trimmed.to_string()));
        }
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err(ForgejoInstanceError::Credentials(trimmed.to_string()));
        }
        if parsed.query().is_some() || parsed.fragment().is_some() {
            return Err(ForgejoInstanceError::QueryOrFragment(trimmed.to_string()));
        }
        let mut base_url = parsed.raw_string();
        while base_url.ends_with('/') {
            base_url.pop();
        }
        let host = parsed.host_str().map_or_else(
            || base_url.clone(),
            |host| match parsed.port() {
                Some(port) => format!("{host}:{port}"),
                None => host.to_string(),
            },
        );
        Ok(Self { base_url, host })
    }

    /// The normalized base URL, without a trailing slash.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.base_url
    }

    /// The host (and optional port) the instance is served from.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The REST API root for this instance.
    #[must_use]
    pub fn api_base_url(&self) -> String {
        format!("{}{FORGEJO_API_BASE_SUFFIX}", self.base_url)
    }
}

impl fmt::Display for ForgejoInstance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.base_url)
    }
}

impl Serialize for ForgejoInstance {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.base_url)
    }
}

impl<'de> Deserialize<'de> for ForgejoInstance {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error as _;
        let url = String::deserialize(deserializer)?;
        Self::new(&url).map_err(D::Error::custom)
    }
}

/// Forgejo access token (PAT). Forgejo tokens are static and unexpired, so
/// there is no refresh machinery — the token is either configured or not.
#[derive(Clone)]
pub struct ForgejoCredentials {
    token: SecretString,
}

impl ForgejoCredentials {
    #[must_use]
    pub fn new(token: String) -> Self {
        Self {
            token: SecretString::new(token),
        }
    }

    #[expect(
        clippy::disallowed_methods,
        reason = "Forgejo credentials support a documented process-env token source."
    )]
    #[must_use]
    pub fn from_env() -> Option<Self> {
        let raw = std::env::var(EnvVars::FORGEJO_TOKEN).ok()?;
        let token = raw.trim().to_string();
        if token.is_empty() {
            return None;
        }
        Some(Self::new(token))
    }

    /// The token, exposed only at the point of use. Callers must not log or
    /// persist the returned value.
    #[must_use]
    pub fn expose(&self) -> &str {
        self.token.expose()
    }
}

impl fmt::Debug for ForgejoCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ForgejoCredentials(<redacted>)")
    }
}

/// The configured instance and its access token, threaded as one value so
/// call sites cannot carry one without the other.
#[derive(Debug, Clone)]
pub struct ForgejoConfig {
    pub instance: ForgejoInstance,
    pub token:    ForgejoCredentials,
}

impl ForgejoConfig {
    pub fn new(instance: ForgejoInstance, token: ForgejoCredentials) -> Self {
        Self { instance, token }
    }
}

/// Bundle of Forgejo credentials and the instance they authenticate to,
/// threaded through every authenticated Forgejo call. The Forgejo counterpart
/// of `GitHubContext`.
#[derive(Debug, Clone)]
pub struct ForgejoContext<'a> {
    creds:    &'a ForgejoCredentials,
    instance: &'a ForgejoInstance,
}

impl<'a> ForgejoContext<'a> {
    pub fn new(creds: &'a ForgejoCredentials, instance: &'a ForgejoInstance) -> Self {
        Self { creds, instance }
    }

    #[must_use]
    pub fn instance(&self) -> &'a ForgejoInstance {
        self.instance
    }

    fn bearer_token(&self) -> String {
        format!("token {}", self.creds.expose())
    }
}

/// Errors returned by pull-request endpoints. Callers branch on `NotFound` to
/// distinguish a missing PR from any other failure. Same contract as
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

/// Pull request created on, or reconciled from, a Forgejo instance. Forgejo
/// has no GraphQL layer, so there is no `node_id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedPullRequest {
    pub html_url: String,
    pub number:   u64,
    pub title:    String,
}

/// Standard Forgejo API headers for authenticated requests. Gitea-lineage
/// servers authenticate PATs as `Authorization: token <PAT>`.
fn forgejo_headers(auth: &str) -> [(&str, &str); 3] {
    [
        ("Authorization", auth),
        ("Accept", "application/json"),
        ("User-Agent", "fabro"),
    ]
}

/// HTTP method used in Forgejo API calls. Ported from `fabro_github` so the
/// mock-client test tooling keeps the same shape.
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

// ---------------------------------------------------------------------------
// Origin URL helpers
// ---------------------------------------------------------------------------

/// Convert a Git SSH URL to HTTPS format for token-based authentication.
///
/// SSH URLs like `git@git.example.com:owner/repo.git` become
/// `https://git.example.com/owner/repo.git`. URLs that are already HTTPS (or
/// any other non-SSH format) are returned unchanged.
#[must_use]
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

/// Normalize any accepted origin shape (`ssh://`, `git@host:path`,
/// credential-bearing HTTPS) into a bare HTTPS URL without a trailing slash
/// or `.git` suffix.
#[must_use]
pub fn normalize_forgejo_origin_url(url: &str) -> String {
    let https = ssh_url_to_https(url.trim());
    let without_credentials = strip_https_credentials(&https);
    let normalized = normalize_https_host_path(&without_credentials);
    let normalized = normalized.trim_end_matches('/');
    normalized
        .strip_suffix(".git")
        .unwrap_or(normalized)
        .to_string()
}

/// Whether `url` points at `instance`: the normalized origin must equal the
/// instance base URL or live underneath it (subpath instances).
#[must_use]
pub fn is_forgejo_origin(instance: &ForgejoInstance, url: &str) -> bool {
    let normalized = normalize_forgejo_origin_url(url);
    if normalized.is_empty() {
        return false;
    }
    normalized == instance.as_str() || normalized.starts_with(&format!("{}/", instance.as_str()))
}

fn redacted_url_for_error(url: &str) -> String {
    DisplaySafeUrl::parse(url)
        .map_or_else(|_| "<invalid url>".to_string(), |url| url.redacted_string())
}

/// Parse `owner` and `repo` from a repository URL on `instance`.
///
/// Accepts URLs like (for instance `https://git.example.com`):
/// - `https://git.example.com/owner/repo.git`
/// - `https://git.example.com/owner/repo`
/// - `https://git.example.com/owner/repo/`
/// - `https://x-access-token:TOKEN@git.example.com/owner/repo.git`
/// - `git@git.example.com:owner/repo.git`
/// - `https://git.example.com/forge/owner/repo.git` (subpath instances)
pub fn parse_forgejo_owner_repo(
    instance: &ForgejoInstance,
    url: &str,
) -> anyhow::Result<(String, String)> {
    let normalized = normalize_forgejo_origin_url(url);
    let display_url = redacted_url_for_error(&normalized);
    let prefix = format!("{}/", instance.as_str());
    let path = normalized
        .strip_prefix(&prefix)
        .ok_or_else(|| anyhow!("Not a repository on Forgejo instance {instance}: {display_url}"))?;

    let mut parts = path.split('/');
    let owner = parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("Missing owner in Forgejo URL: {display_url}"))?;
    let repo = parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("Missing repo in Forgejo URL: {display_url}"))?;
    if parts.next().is_some() {
        bail!("Forgejo URL must identify one repository: {display_url}");
    }

    Ok((owner.to_string(), repo.to_string()))
}

/// Embed a token into an HTTPS URL for authenticated git operations.
///
/// Converts `https://git.example.com/owner/repo` to
/// `https://x-access-token:<token>@git.example.com/owner/repo`. The returned
/// [`DisplaySafeUrl`] renders the token redacted everywhere except
/// [`DisplaySafeUrl::as_raw_url`].
pub fn embed_token_in_url(url: &str, token: &str) -> anyhow::Result<DisplaySafeUrl> {
    let mut url = DisplaySafeUrl::parse(url).context("Failed to parse Forgejo HTTPS URL")?;
    if url.scheme() != "https" {
        bail!(
            "Forgejo clone URL must use HTTPS: {}",
            url.redacted_string()
        );
    }
    url.set_username(FORGEJO_TOKEN_USERNAME)
        .map_err(|()| anyhow!("Failed to set Forgejo token username"))?;
    url.set_password(Some(token))
        .map_err(|()| anyhow!("Failed to set Forgejo token password"))?;
    Ok(url)
}

// ---------------------------------------------------------------------------
// REST operations
// ---------------------------------------------------------------------------

/// User summary from `GET /user`. Used to validate a PAT.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ForgejoUser {
    pub login: String,
}

/// Validate the configured token by fetching the authenticated user.
pub async fn get_authenticated_user(
    client: &impl HttpClient,
    ctx: &ForgejoContext<'_>,
) -> anyhow::Result<ForgejoUser> {
    let url = format!("{}/user", ctx.instance.api_base_url());
    let auth = ctx.bearer_token();
    let resp = client
        .request(HttpMethod::Get, &url, &forgejo_headers(&auth), None)
        .await
        .context("Failed to fetch authenticated Forgejo user")?;

    match resp.status {
        200 => {}
        401 | 403 => {
            bail!(
                "Forgejo authentication failed. Check that the instance URL and token are correct."
            );
        }
        status => {
            bail!("Unexpected status {status} fetching authenticated Forgejo user");
        }
    }

    resp.json::<ForgejoUser>()
        .context("Failed to parse authenticated Forgejo user")
}

/// Repository summary from `GET /repos/{owner}/{repo}`. `Ok(None)` means the
/// token cannot see the repository (or it does not exist).
pub async fn get_repository(
    client: &impl HttpClient,
    ctx: &ForgejoContext<'_>,
    owner: &str,
    repo: &str,
) -> anyhow::Result<Option<ForgejoRepository>> {
    let url = format!("{}/repos/{owner}/{repo}", ctx.instance.api_base_url());
    let auth = ctx.bearer_token();
    let resp = client
        .request(HttpMethod::Get, &url, &forgejo_headers(&auth), None)
        .await
        .context("Failed to fetch Forgejo repository")?;

    match resp.status {
        200 => resp
            .json::<ForgejoRepository>()
            .map(Some)
            .context("Failed to parse Forgejo repository"),
        404 => Ok(None),
        401 | 403 => {
            bail!("Forgejo authentication failed. Check that the token grants repository access.")
        }
        status => bail!("Unexpected status {status} fetching Forgejo repository"),
    }
}

/// Minimal repository summary the client needs.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ForgejoRepository {
    pub full_name: String,
    #[serde(default)]
    pub private:   bool,
    #[serde(default)]
    pub fork:      bool,
}

/// Return the commit SHA at the head of a Forgejo branch.
///
/// Returns `None` when the branch does not exist. Gitea-lineage servers name
/// the commit field `id`, unlike GitHub's `sha`.
pub async fn branch_head_sha(
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

    let url = format!(
        "{}/repos/{owner}/{repo}/branches/{branch}",
        ctx.instance.api_base_url()
    );
    let auth = ctx.bearer_token();
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
        401 | 403 => bail!(
            "Forgejo authentication failed reading branch '{branch}'. \
             Check that the token grants repository access."
        ),
        status => bail!("Unexpected status {status} reading branch '{branch}'"),
    }
}

/// Find an open pull request that already carries the expected head commit.
///
/// This supports recovery when a pull request was created but the caller
/// stopped before it could persist the result locally. Gitea-lineage list
/// endpoints filter by `state` only, so head matching happens client-side.
pub async fn find_open_pull_request(
    client: &impl HttpClient,
    ctx: &ForgejoContext<'_>,
    owner: &str,
    repo: &str,
    head_branch: &str,
    expected_head_sha: &str,
) -> anyhow::Result<Option<CreatedPullRequest>> {
    #[derive(Deserialize)]
    struct PullRequestHead {
        #[serde(default, rename = "ref")]
        ref_name: Option<String>,
        #[serde(default)]
        sha:      Option<String>,
    }

    #[derive(Deserialize)]
    struct PullRequestListItem {
        html_url: String,
        number:   u64,
        title:    String,
        head:     PullRequestHead,
    }

    let mut url = DisplaySafeUrl::parse(&format!(
        "{}/repos/{owner}/{repo}/pulls",
        ctx.instance.api_base_url()
    ))
    .context("Failed to build pull request reconciliation URL")?;
    url.query_pairs_mut()
        .append_pair("state", "open")
        .append_pair("limit", "50");
    let auth = ctx.bearer_token();
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
            pull_request.head.ref_name.as_deref() == Some(head_branch)
                && pull_request.head.sha.as_deref() == Some(expected_head_sha)
        })
        .map(|pull_request| CreatedPullRequest {
            html_url: pull_request.html_url,
            number:   pull_request.number,
            title:    pull_request.title,
        }))
}

/// Create a pull request on `instance`.
#[allow(
    clippy::too_many_arguments,
    reason = "Creating a pull request needs explicit repo, branch, and body fields."
)]
pub async fn create_pull_request(
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
        title:    String,
    }

    tracing::info!(title = %title, head = %head, base = %base, draft, "Creating pull request");

    let pr_body = serde_json::json!({
        "title": title,
        "head": head,
        "base": base,
        "body": body,
        "draft": draft,
    });

    let url = format!("{}/repos/{owner}/{repo}/pulls", ctx.instance.api_base_url());
    let auth = ctx.bearer_token();
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
        404 => {
            bail!("Repository {owner}/{repo} not found on Forgejo instance");
        }
        409 => {
            bail!("Pull request already exists for {head} -> {base}");
        }
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
        number:   pr.number,
        title:    pr.title,
    })
}

/// Fields of a Forgejo pull request payload the client maps into the shared
/// [`PullRequestGithubDetail`] projection. Gitea-lineage payloads carry a
/// different `state`/ref vocabulary and no diff-stat fields, so the mapping
/// is explicit instead of a direct deserialize.
#[derive(Debug, Clone, Deserialize)]
pub struct ForgejoPullRequest {
    pub number:     u64,
    pub title:      String,
    #[serde(default)]
    pub body:       Option<String>,
    #[serde(default = "default_pr_state")]
    pub state:      String,
    #[serde(default)]
    pub draft:      bool,
    #[serde(default)]
    pub merged:     bool,
    #[serde(default)]
    pub merged_at:  Option<String>,
    pub html_url:   String,
    pub user:       ForgejoPrUser,
    pub head:       ForgejoPrRef,
    pub base:       ForgejoPrRef,
    pub created_at: String,
    pub updated_at: String,
}

fn default_pr_state() -> String {
    "open".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct ForgejoPrUser {
    pub login: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ForgejoPrRef {
    #[serde(rename = "ref")]
    pub ref_name: String,
}

impl ForgejoPullRequest {
    /// Project the Forgejo payload onto the shared detail shape the server
    /// and web surfaces render. Diff-stat fields have no Forgejo equivalent
    /// in the base payload and read as zero; `mergeable` is unknown.
    #[must_use]
    pub fn to_detail(&self) -> PullRequestGithubDetail {
        PullRequestGithubDetail {
            number:        self.number,
            title:         self.title.clone(),
            body:          self.body.clone(),
            state:         self.state.clone(),
            draft:         self.draft,
            merged:        self.merged,
            merged_at:     self.merged_at.clone(),
            mergeable:     None,
            additions:     0,
            deletions:     0,
            changed_files: 0,
            html_url:      self.html_url.clone(),
            user:          fabro_types::PullRequestUser {
                login: self.user.login.clone(),
            },
            head:          fabro_types::PullRequestRef {
                ref_name: self.head.ref_name.clone(),
            },
            base:          fabro_types::PullRequestRef {
                ref_name: self.base.ref_name.clone(),
            },
            created_at:    self.created_at.clone(),
            updated_at:    self.updated_at.clone(),
        }
    }
}

/// Fetch detailed information about a pull request.
pub async fn get_pull_request(
    client: &impl HttpClient,
    ctx: &ForgejoContext<'_>,
    owner: &str,
    repo: &str,
    number: u64,
) -> Result<PullRequestGithubDetail, PullRequestApiError> {
    tracing::debug!(owner, repo, number, "Fetching pull request");

    let url = format!(
        "{}/repos/{owner}/{repo}/pulls/{number}",
        ctx.instance.api_base_url()
    );
    let auth = ctx.bearer_token();
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

    let pr: ForgejoPullRequest = resp
        .json()
        .context("Failed to parse pull request response")?;
    Ok(pr.to_detail())
}

/// The Gitea-lineage merge `Do` value for a Fabro merge strategy.
fn merge_method_as_do_value(method: MergeStrategy) -> &'static str {
    match method {
        MergeStrategy::Merge => "merge",
        MergeStrategy::Squash => "squash",
        MergeStrategy::Rebase => "rebase",
    }
}

/// Merge a pull request.
pub async fn merge_pull_request(
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
        ctx.instance.api_base_url()
    );
    // Gitea-lineage merges pick the style through the capitalized `Do` field.
    let body = serde_json::json!({ "Do": merge_method_as_do_value(method) });
    let auth = ctx.bearer_token();

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

/// Enable auto-merge on a pull request.
///
/// Gitea-lineage servers expose auto-merge as a merge-endpoint parameter
/// (`merge_when_checks_succeed`) rather than GitHub's separate endpoint. The
/// request carries the same `Do` value a direct merge would, so the merge
/// happens as soon as required checks pass. Repositories (or server
/// versions) without the feature reject the request; that surfaces as a
/// clear error instead of failing the run's PR creation.
pub async fn enable_auto_merge(
    client: &impl HttpClient,
    ctx: &ForgejoContext<'_>,
    owner: &str,
    repo: &str,
    number: u64,
    merge_method: MergeStrategy,
) -> anyhow::Result<()> {
    let url = format!(
        "{}/repos/{owner}/{repo}/pulls/{number}/merge",
        ctx.instance.api_base_url()
    );
    let body = serde_json::json!({
        "Do": merge_method_as_do_value(merge_method),
        "merge_when_checks_succeed": true,
    });
    let auth = ctx.bearer_token();

    tracing::debug!(owner, repo, number, merge_method = %merge_method, "Enabling auto-merge");

    let resp = client
        .request(HttpMethod::Post, &url, &forgejo_headers(&auth), Some(&body))
        .await
        .context("Failed to enable auto-merge")?;

    match resp.status {
        200 => {
            tracing::info!(owner, repo, number, "Auto-merge enabled");
            Ok(())
        }
        404 => bail!("Pull request #{number} not found in {owner}/{repo}"),
        405 => bail!("Pull request #{number} is not mergeable (method may not be allowed)"),
        400 | 422 => bail!(
            "Auto-merge is not supported for pull request #{number} on this Forgejo instance; \
             merge it manually once checks pass"
        ),
        401 | 403 => bail!(
            "Authentication failed enabling auto-merge ({})",
            resp.status
        ),
        status => bail!(
            "Unexpected status {status} enabling auto-merge: {}",
            resp.text()
        ),
    }
}

/// Close a pull request.
pub async fn close_pull_request(
    client: &impl HttpClient,
    ctx: &ForgejoContext<'_>,
    owner: &str,
    repo: &str,
    number: u64,
) -> Result<(), PullRequestApiError> {
    tracing::debug!(owner, repo, number, "Closing pull request");

    let url = format!(
        "{}/repos/{owner}/{repo}/pulls/{number}",
        ctx.instance.api_base_url()
    );
    let body = serde_json::json!({ "state": "closed" });
    let auth = ctx.bearer_token();

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
        200 => Ok(()),
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

#[cfg(test)]
pub(crate) mod tests_mock;

#[cfg(test)]
mod tests;
