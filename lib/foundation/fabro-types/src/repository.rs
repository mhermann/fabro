use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryRef {
    pub name:       String,
    #[serde(default)]
    pub origin_url: Option<String>,
    pub provider:   RepositoryProvider,
}

impl RepositoryRef {
    pub fn from_origin_and_source(
        origin_url: Option<String>,
        source_directory: Option<&str>,
    ) -> Self {
        Self {
            name: repository_name(origin_url.as_deref(), source_directory),
            provider: repository_provider(origin_url.as_deref()),
            origin_url,
        }
    }
}

/// A validated GitHub `owner/repo` coordinate.
///
/// Construction enforces GitHub's owner and repository name syntax on the
/// exact submitted bytes; no trimming, case folding, or other normalization
/// is performed. The original spelling is preserved for `Display` and
/// serialization, while identity (`Eq`, `Ord`, `Hash`) is case-insensitive
/// to match GitHub's treatment of owner and repository names.
#[derive(Debug, Clone)]
pub struct GitHubRepositorySlug {
    owner: String,
    repo:  String,
}

impl GitHubRepositorySlug {
    /// Validates `value` as a GitHub `owner/repo` slug, returning `None` when
    /// it is not exactly one `/`-separated pair of a valid owner and
    /// repository name.
    #[must_use]
    pub fn try_new(value: &str) -> Option<Self> {
        let (owner, repo) = value.split_once('/')?;
        if repo.contains('/') || !valid_github_owner(owner) || !valid_github_repo(repo) {
            return None;
        }
        Some(Self {
            owner: owner.to_string(),
            repo:  repo.to_string(),
        })
    }

    #[must_use]
    pub fn owner(&self) -> &str {
        &self.owner
    }

    #[must_use]
    pub fn repo(&self) -> &str {
        &self.repo
    }

    /// Canonical credential-free HTTPS URL for this GitHub repository.
    #[must_use]
    pub fn https_url(&self) -> String {
        format!("https://github.com/{self}")
    }

    /// Whether `other` names the same repository owner, ignoring ASCII case.
    /// Owner and repository names are validated ASCII, so ASCII folding is
    /// exact.
    #[must_use]
    pub fn same_owner(&self, other: &Self) -> bool {
        self.owner.eq_ignore_ascii_case(&other.owner)
    }
}

/// Case-folded bytes for identity comparisons without allocating; owner and
/// repository names are validated ASCII, so ASCII folding is exact.
fn folded_bytes(value: &str) -> impl Iterator<Item = u8> + '_ {
    value.bytes().map(|byte| byte.to_ascii_lowercase())
}

impl PartialEq for GitHubRepositorySlug {
    fn eq(&self, other: &Self) -> bool {
        self.owner.eq_ignore_ascii_case(&other.owner) && self.repo.eq_ignore_ascii_case(&other.repo)
    }
}

impl Eq for GitHubRepositorySlug {}

impl PartialOrd for GitHubRepositorySlug {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for GitHubRepositorySlug {
    fn cmp(&self, other: &Self) -> Ordering {
        folded_bytes(&self.owner)
            .cmp(folded_bytes(&other.owner))
            .then_with(|| folded_bytes(&self.repo).cmp(folded_bytes(&other.repo)))
    }
}

impl Hash for GitHubRepositorySlug {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for byte in folded_bytes(&self.owner) {
            state.write_u8(byte);
        }
        // `/` cannot appear in a validated owner, so the folded
        // `owner/repo` encoding stays unambiguous.
        state.write_u8(b'/');
        for byte in folded_bytes(&self.repo) {
            state.write_u8(byte);
        }
    }
}

impl fmt::Display for GitHubRepositorySlug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.owner, self.repo)
    }
}

/// Parse failure for [`GitHubRepositorySlug`]. The offending input is not
/// echoed back because config surfaces already attach the value and its path.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "expected a GitHub `owner/repository` slug with no scheme, host, ref, or extra path component"
)]
pub struct GitHubRepositorySlugError;

impl FromStr for GitHubRepositorySlug {
    type Err = GitHubRepositorySlugError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_new(value).ok_or(GitHubRepositorySlugError)
    }
}

impl Serialize for GitHubRepositorySlug {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for GitHubRepositorySlug {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error as _;

        let value = String::deserialize(deserializer)?;
        value.parse().map_err(D::Error::custom)
    }
}

fn valid_github_owner(value: &str) -> bool {
    if value.is_empty() || value.len() > 39 {
        return false;
    }
    let bytes = value.as_bytes();
    let first = bytes[0];
    let last = bytes[bytes.len() - 1];
    (first.is_ascii_alphanumeric() && last.is_ascii_alphanumeric())
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
}

fn valid_github_repo(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

/// Reports whether `value` is a safe GitHub git ref selector.
///
/// The grammar checks the exact untrimmed ASCII byte content; no
/// normalization is performed.
#[must_use]
pub fn is_valid_github_ref_selector(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && value.trim() == value
        && !value.starts_with(['/', '-', '.'])
        && !value.ends_with(['/', '.'])
        && !has_lock_suffix(value)
        && value != "@"
        && !value.contains("..")
        && !value.contains("//")
        && !value.contains("@{")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-'))
        && value
            .split('/')
            .all(|part| !part.is_empty() && !part.starts_with('.') && !has_lock_suffix(part))
}

/// Reports whether `value` is a canonical bare Git branch name.
///
/// Bare names exclude symbolic selectors, fully qualified refs, tag-prefixed
/// selectors, commit SHAs, and the `heads/` prefix accepted only after Git has
/// already entered the refs namespace.
#[must_use]
pub fn is_valid_git_branch_name(value: &str) -> bool {
    is_valid_bare_git_ref_name(value) && !value.starts_with("heads/")
}

/// Reports whether `value` is a canonical bare Git tag name.
///
/// The input is a tag name rather than a selector, so `tags/`, `refs/`,
/// symbolic `HEAD`, and exact commit SHAs are rejected.
#[must_use]
pub fn is_valid_git_tag_name(value: &str) -> bool {
    is_valid_bare_git_ref_name(value)
}

fn is_valid_bare_git_ref_name(value: &str) -> bool {
    value != "HEAD"
        && !value.starts_with("tags/")
        && !value.starts_with("refs/")
        && normalize_git_commit_sha(value).is_none()
        && is_valid_github_ref_selector(value)
}

/// Validates and canonicalizes an exact Git commit SHA.
///
/// The grammar accepts exactly 40 untrimmed ASCII hexadecimal bytes. It does
/// not resolve abbreviations or verify that the object exists or belongs to a
/// branch.
#[must_use]
pub fn normalize_git_commit_sha(value: &str) -> Option<String> {
    (value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| value.to_ascii_lowercase())
}

fn has_lock_suffix(value: &str) -> bool {
    value
        .rsplit_once('.')
        .is_some_and(|(_, extension)| extension == "lock")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryProvider {
    Github,
    Forgejo,
    Git,
    Unknown,
}

/// The SCM forge a Git coordinate belongs to.
///
/// Run targets and pull request links carry this tag so that GitHub and a
/// configured Forgejo instance can coexist on one deployment. Absent tags
/// deserialize as [`ScmProvider::Github`], which keeps every existing
/// serialized coordinate on the GitHub path.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    strum::Display,
    strum::EnumString,
    strum::IntoStaticStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum ScmProvider {
    #[default]
    Github,
    Forgejo,
}

fn repository_provider(origin_url: Option<&str>) -> RepositoryProvider {
    repository_provider_with(origin_url, None)
}

/// Classifies an origin URL, optionally recognizing a configured Forgejo
/// instance by host.
///
/// Without `forgejo_url` this behaves exactly like the historical GitHub
/// classification.
#[must_use]
pub fn repository_provider_with(
    origin_url: Option<&str>,
    forgejo_url: Option<&str>,
) -> RepositoryProvider {
    let Some(origin) = origin_url.filter(|origin| !origin.trim().is_empty()) else {
        return RepositoryProvider::Unknown;
    };
    if is_github_origin(origin) {
        return RepositoryProvider::Github;
    }
    if forgejo_url.is_some_and(|url| origin_matches_instance(origin, url)) {
        return RepositoryProvider::Forgejo;
    }
    RepositoryProvider::Git
}

/// Whether `origin` points at the Forgejo instance published at
/// `instance_url`.
///
/// Hosts are compared case-insensitively. SSH remotes (`git@host:owner/repo`,
/// `ssh://git@host/owner/repo`) match by host alone, while HTTP(S) origins
/// must agree on the explicit or scheme-default port, so
/// `https://host:8443/…` does not match an instance published at
/// `https://host`.
#[must_use]
pub fn origin_matches_instance(origin: &str, instance_url: &str) -> bool {
    let Some((origin_host, origin_port)) = url_host_port(origin.trim()) else {
        return false;
    };
    let Some((instance_host, instance_port)) = url_host_port(instance_url.trim()) else {
        return false;
    };
    if !origin_host.eq_ignore_ascii_case(&instance_host) {
        return false;
    }
    match (origin_port, instance_port) {
        // SSH spellings identify the deployment by host alone.
        (None, _) | (_, None) => true,
        (Some(origin_port), Some(instance_port)) => origin_port == instance_port,
    }
}

/// Host and effective port from an HTTPS/HTTP/SSH URL or an SCP-like
/// `git@host:path` spelling. SSH spellings yield `None` for the port because
/// they identify the deployment by host alone; HTTP(S) URLs yield the
/// explicit or scheme-default port.
fn url_host_port(value: &str) -> Option<(String, Option<u16>)> {
    if let Some(rest) = value.strip_prefix("git@") {
        let (host, _) = rest.split_once(':')?;
        return Some((host.to_string(), None));
    }
    let parsed = url::Url::parse(value).ok()?;
    let host = parsed.host_str()?.to_string();
    if parsed.scheme() == "ssh" {
        return Some((host, None));
    }
    Some((host, parsed.port_or_known_default()))
}

fn is_github_origin(origin: &str) -> bool {
    origin.starts_with("git@github.com:")
        || origin.starts_with("https://github.com/")
        || origin.starts_with("http://github.com/")
        || origin.starts_with("ssh://git@github.com/")
}

fn repository_name(origin_url: Option<&str>, source_directory: Option<&str>) -> String {
    origin_url
        .and_then(repository_name_from_origin)
        .or_else(|| {
            source_directory
                .and_then(path_basename)
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| "unknown".to_string())
}

#[expect(
    clippy::disallowed_types,
    reason = "Run summaries parse the origin only to extract an owner/repo label; raw URLs are not logged."
)]
fn repository_name_from_origin(origin: &str) -> Option<String> {
    if let Some(path) = origin
        .strip_prefix("git@")
        .and_then(|url| url.split_once(':').map(|(_, path)| path))
    {
        return repository_name_from_path(path).map(ToOwned::to_owned);
    }

    let parsed = url::Url::parse(origin).ok()?;
    let path = parsed.path().trim_matches('/');
    repository_name_from_path(path).map(ToOwned::to_owned)
}

fn repository_name_from_path(path: &str) -> Option<&str> {
    let stripped = path.strip_suffix(".git").unwrap_or(path);
    let mut segments = stripped.rsplit('/').filter(|segment| !segment.is_empty());
    let repo = segments.next()?;
    let owner = segments.next();
    if let Some(owner) = owner {
        let start = stripped.len() - owner.len() - repo.len() - 1;
        stripped.get(start..)
    } else {
        Some(repo)
    }
}

fn path_basename(path: &str) -> Option<&str> {
    path.rsplit(['/', '\\']).find(|segment| !segment.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_slugs_preserve_exact_parts() {
        let cases = [
            ("fabro-sh/fabro", "fabro-sh", "fabro"),
            ("owner/.github", "owner", ".github"),
        ];
        for (input, owner, repo) in cases {
            let slug = GitHubRepositorySlug::try_new(input).expect(input);
            assert_eq!(slug.owner(), owner, "{input}");
            assert_eq!(slug.repo(), repo, "{input}");
        }

        let max_owner = "a".repeat(39);
        let max_repo = "b".repeat(100);
        let boundary = format!("{max_owner}/{max_repo}");
        let slug = GitHubRepositorySlug::try_new(&boundary).expect("boundary-length slug");
        assert_eq!(slug.owner(), max_owner);
        assert_eq!(slug.repo(), max_repo);
    }

    #[test]
    fn invalid_slugs_are_rejected() {
        let cases = [
            "",
            "fabro",
            "not/github/slug",
            "/repo",
            "owner/",
            "-owner/repo",
            "owner-/repo",
            "own_er/repo",
            "owner/.",
            "owner/..",
            "owner/re po",
            "öwner/repo",
            "owner/rëpo",
        ];
        for input in cases {
            assert!(GitHubRepositorySlug::try_new(input).is_none(), "{input}");
        }

        let over_owner = format!("{}/repo", "a".repeat(40));
        let over_repo = format!("owner/{}", "b".repeat(101));
        assert!(GitHubRepositorySlug::try_new(&over_owner).is_none());
        assert!(GitHubRepositorySlug::try_new(&over_repo).is_none());
    }

    #[test]
    fn slug_identity_is_case_insensitive_but_display_preserves_case() {
        let mixed: GitHubRepositorySlug = "Fabro-SH/Keystone".parse().unwrap();
        let lower: GitHubRepositorySlug = "fabro-sh/keystone".parse().unwrap();

        assert_eq!(mixed, lower);
        assert_eq!(mixed.cmp(&lower), std::cmp::Ordering::Equal);
        assert!(mixed.same_owner(&lower));
        assert_eq!(mixed.to_string(), "Fabro-SH/Keystone");

        let mut hashes = std::collections::HashSet::new();
        hashes.insert(mixed.clone());
        assert!(
            !hashes.insert(lower.clone()),
            "case variants share identity"
        );

        let mut ordered = std::collections::BTreeSet::new();
        ordered.insert(mixed);
        assert!(!ordered.insert(lower), "case variants share ordering");
    }

    #[test]
    fn slug_https_url_preserves_spelling_and_has_no_credentials() {
        let slug: GitHubRepositorySlug = "Fabro-SH/Keystone".parse().unwrap();

        assert_eq!(slug.https_url(), "https://github.com/Fabro-SH/Keystone");
        assert!(!slug.https_url().contains('@'));
    }

    #[test]
    fn slug_ordering_sorts_by_canonical_form() {
        let mut slugs: Vec<GitHubRepositorySlug> = ["owner/Zeta", "Owner/alpha", "owner/Beta"]
            .iter()
            .map(|value| value.parse().unwrap())
            .collect();
        slugs.sort();
        let rendered: Vec<String> = slugs.iter().map(ToString::to_string).collect();
        assert_eq!(rendered, ["Owner/alpha", "owner/Beta", "owner/Zeta"]);
    }

    #[test]
    fn slug_from_str_rejects_urls_and_hosts() {
        let cases = [
            "https://github.com/owner/repo",
            "git@github.com:owner/repo.git",
            "ssh://git@github.com/owner/repo",
            "github.com/owner/repo",
            "owner/repo@main",
            "owner/repo#ref",
            " owner/repo",
            "owner/repo ",
        ];
        for input in cases {
            assert!(input.parse::<GitHubRepositorySlug>().is_err(), "{input}");
        }
    }

    #[test]
    fn slug_serde_round_trips_as_a_string() {
        let slug: GitHubRepositorySlug = "Fabro-SH/Keystone".parse().unwrap();
        let json = serde_json::to_string(&slug).unwrap();
        assert_eq!(json, "\"Fabro-SH/Keystone\"");

        let parsed: GitHubRepositorySlug = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, slug);

        let err = serde_json::from_str::<GitHubRepositorySlug>("\"not a slug\"").unwrap_err();
        assert!(err.to_string().contains("owner/repository"), "{err}");
    }

    #[test]
    fn scm_provider_serde_and_display_use_snake_case() {
        assert_eq!(serde_json::to_value(ScmProvider::Github).unwrap(), "github");
        assert_eq!(
            serde_json::to_value(ScmProvider::Forgejo).unwrap(),
            "forgejo"
        );
        assert_eq!(ScmProvider::Forgejo.to_string(), "forgejo");
        assert_eq!(
            serde_json::from_str::<ScmProvider>("\"github\"").unwrap(),
            ScmProvider::Github
        );
    }

    #[test]
    fn repository_provider_recognizes_forgejo_instances() {
        let instance = "https://git.example.com";
        let cases = [
            ("https://git.example.com/owner/repo.git", true),
            ("https://git.example.com/owner/repo", true),
            ("git@git.example.com:owner/repo.git", true),
            ("ssh://git@git.example.com/owner/repo", true),
            ("https://GIT.EXAMPLE.COM/owner/repo.git", true),
        ];
        for (origin, expected) in cases {
            assert_eq!(
                repository_provider_with(Some(origin), Some(instance)),
                if expected {
                    RepositoryProvider::Forgejo
                } else {
                    RepositoryProvider::Git
                },
                "{origin}"
            );
        }

        // Different hosts and ports stay plain Git origins.
        for origin in [
            "https://other.example.com/owner/repo.git",
            "https://git.example.com:8443/owner/repo.git",
            "git@other.example.com:owner/repo.git",
        ] {
            assert_eq!(
                repository_provider_with(Some(origin), Some(instance)),
                RepositoryProvider::Git,
                "{origin}"
            );
        }
    }

    #[test]
    fn repository_provider_matches_localhost_instance_ports() {
        let instance = "http://localhost:3001";
        assert_eq!(
            repository_provider_with(Some("http://localhost:3001/owner/repo"), Some(instance)),
            RepositoryProvider::Forgejo
        );
        assert_eq!(
            repository_provider_with(Some("git@localhost:owner/repo.git"), Some(instance)),
            RepositoryProvider::Forgejo
        );
        assert_eq!(
            repository_provider_with(Some("http://localhost:3000/owner/repo"), Some(instance)),
            RepositoryProvider::Git
        );
    }

    #[test]
    fn repository_provider_without_instance_keeps_github_classification() {
        assert_eq!(
            repository_provider_with(Some("https://github.com/owner/repo"), None),
            RepositoryProvider::Github
        );
        assert_eq!(
            repository_provider_with(Some("https://git.example.com/owner/repo"), None),
            RepositoryProvider::Git
        );
        assert_eq!(repository_provider_with(None, None), RepositoryProvider::Unknown);
    }

    #[test]
    fn valid_ref_selectors_are_accepted() {
        let max = "a".repeat(255);
        let cases = [
            "main",
            "feature/release-1.2_rc",
            "refs/tags/v1.0.0",
            max.as_str(),
        ];
        for input in cases {
            assert!(is_valid_github_ref_selector(input), "{input}");
        }
    }

    #[test]
    fn invalid_ref_selectors_are_rejected() {
        let cases = [
            "",
            " main",
            "main ",
            "/main",
            "-main",
            ".main",
            "main/",
            "main.",
            "feature//x",
            "feature/.hidden",
            "main.lock",
            "branch.lock/x",
            "@",
            "a..b",
            "a@{b",
            "main;rm",
            "ma\tin",
            "mäin",
        ];
        for input in cases {
            assert!(!is_valid_github_ref_selector(input), "{input:?}");
        }

        let over = "a".repeat(256);
        assert!(!is_valid_github_ref_selector(&over));
    }
}
