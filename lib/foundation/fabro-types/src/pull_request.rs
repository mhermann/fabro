use std::fmt::Write as _;

use chrono::{DateTime, Utc};
use serde::de::Error as DeError;
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::id::ulid_id;

ulid_id!(PullRequestCreationId);

/// Durable status for an explicitly requested pull request creation.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum PullRequestCreationStatus {
    Pending,
    Succeeded,
    Failed,
}

/// Latest explicit pull request creation requested for a run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequestCreation {
    pub id:           PullRequestCreationId,
    pub status:       PullRequestCreationStatus,
    pub model:        String,
    pub force:        bool,
    pub requested_at: DateTime<Utc>,
    pub updated_at:   DateTime<Utc>,
    /// Copy of the run's pull request link so that polling the creation
    /// resource alone is enough to learn the outcome. Always equal to the
    /// run's `pull_request` when the status is `Succeeded`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull_request: Option<PullRequestLink>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error:        Option<String>,
}

impl PullRequestCreation {
    #[must_use]
    pub fn is_pending(&self) -> bool {
        self.status == PullRequestCreationStatus::Pending
    }

    pub fn succeed(&mut self, pull_request: PullRequestLink, ts: DateTime<Utc>) {
        self.status = PullRequestCreationStatus::Succeeded;
        self.updated_at = ts;
        self.pull_request = Some(pull_request);
        self.error = None;
    }

    pub fn fail(&mut self, error: String, ts: DateTime<Utc>) {
        self.status = PullRequestCreationStatus::Failed;
        self.updated_at = ts;
        self.error = Some(error);
    }
}

/// Minimal pull request reference stored on a workflow run.
///
/// GitHub runs store only `owner/repo/number` (the github.com URL shape is
/// implied). Forgejo runs additionally carry the instance base URL because
/// Forgejo is self-hosted and there is no canonical host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequestLink {
    pub owner:  String,
    pub repo:   String,
    pub number: u64,
    /// Set only for non-GitHub forges. Skipped during serialization so every
    /// GitHub link produced before Forgejo support stays byte-identical.
    pub forge:  Option<ForgeInstanceRef>,
}

/// The self-hosted forge instance a pull request lives on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForgeInstanceRef {
    pub base_url: String,
}

impl PullRequestLink {
    #[must_use]
    pub fn github(owner: impl Into<String>, repo: impl Into<String>, number: u64) -> Self {
        Self {
            owner: owner.into(),
            repo: repo.into(),
            number,
            forge: None,
        }
    }

    #[must_use]
    pub fn on_forge(
        base_url: impl Into<String>,
        owner: impl Into<String>,
        repo: impl Into<String>,
        number: u64,
    ) -> Self {
        Self {
            owner: owner.into(),
            repo: repo.into(),
            number,
            forge: Some(ForgeInstanceRef {
                base_url: base_url.into(),
            }),
        }
    }

    #[must_use]
    pub fn html_url(&self) -> String {
        match &self.forge {
            // Forgejo web UIs serve pull requests under `/pulls`.
            Some(forge) => format!(
                "{}/{}/{}/pulls/{}",
                forge.base_url.trim_end_matches('/'),
                self.owner,
                self.repo,
                self.number
            ),
            None => format!(
                "https://github.com/{}/{}/pull/{}",
                self.owner, self.repo, self.number
            ),
        }
    }

    pub fn from_github_url(url: &str) -> Result<Self, String> {
        github_pull_request_link_from_url(url)
    }

    /// Parses a pull request URL on the configured Forgejo instance. Both the
    /// Forgejo `/pulls/{n}` shape and GitHub's `/pull/{n}` (which Forgejo
    /// redirects for familiarity) are accepted.
    pub fn from_forgejo_url(url: &str, base_url: &str) -> Result<Self, String> {
        forgejo_pull_request_link_from_url(url, base_url)
    }

    /// Reconstructs a link from a stored pull request URL without knowing the
    /// forge instance up front: github.com URLs become GitHub links; any other
    /// host becomes a Forgejo link whose base URL is the URL origin plus any
    /// path segments before `owner/repo`.
    #[expect(
        clippy::disallowed_types,
        reason = "Reconstruction parses stored public PR URLs; credentials never occur in them."
    )]
    pub fn from_stored_pr_url(raw_url: &str) -> Result<Self, String> {
        let parsed =
            url::Url::parse(raw_url).map_err(|err| format!("Invalid pull request URL: {err}"))?;
        if parsed.host_str() == Some("github.com") {
            return github_pull_request_link_from_url(raw_url);
        }

        let segments = parsed
            .path_segments()
            .map(|segments| {
                segments
                    .filter(|segment| !segment.is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let segments_len = segments.len();
        let [.., owner, repo, "pulls" | "pull", number] = segments.as_slice() else {
            return Err("Pull request link must use {base}/owner/repo/pulls/123.".to_string());
        };
        let number = number
            .parse()
            .map_err(|_| "Pull request URL number must be an unsigned integer.".to_string())?;

        let base_path = &segments[..segments_len - 3];
        let mut base = format!(
            "{}://{}",
            parsed.scheme(),
            parsed.host_str().unwrap_or_default()
        );
        if let Some(port) = parsed.port() {
            let _ = write!(base, ":{port}");
        }
        for segment in base_path {
            let _ = write!(base, "/{segment}");
        }

        Ok(Self {
            owner: (*owner).to_string(),
            repo: (*repo).to_string(),
            number,
            forge: Some(ForgeInstanceRef { base_url: base }),
        })
    }
}

impl Serialize for PullRequestLink {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let field_count = if self.forge.is_some() { 5 } else { 4 };
        let mut state = serializer.serialize_struct("PullRequestLink", field_count)?;
        state.serialize_field("owner", &self.owner)?;
        state.serialize_field("repo", &self.repo)?;
        state.serialize_field("number", &self.number)?;
        state.serialize_field("html_url", &self.html_url())?;
        if let Some(forge) = &self.forge {
            state.serialize_field("forge", forge)?;
        }
        state.end()
    }
}

impl<'de> Deserialize<'de> for PullRequestLink {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            #[serde(default)]
            html_url: Option<String>,
            #[serde(default)]
            owner:    Option<String>,
            #[serde(default)]
            repo:     Option<String>,
            #[serde(default)]
            number:   Option<u64>,
            #[serde(default)]
            forge:    Option<ForgeInstanceRef>,
        }

        let wire = Wire::deserialize(deserializer)?;
        let (Some(owner), Some(repo), Some(number)) = (wire.owner, wire.repo, wire.number) else {
            return Err(D::Error::custom("missing pull request owner/repo/number"));
        };
        let link = Self {
            owner,
            repo,
            number,
            forge: wire.forge,
        };

        if let Some(html_url) = wire.html_url {
            let url_link = match &link.forge {
                Some(forge) => forgejo_pull_request_link_from_url(&html_url, &forge.base_url),
                None => github_pull_request_link_from_url(&html_url),
            }
            .map_err(D::Error::custom)?;
            if url_link != link {
                return Err(D::Error::custom(
                    "pull request html_url does not match owner/repo/number",
                ));
            }
        }

        Ok(link)
    }
}

#[expect(
    clippy::disallowed_types,
    reason = "Pull request links are public github.com URLs stored for display and coordinate inference."
)]
fn github_pull_request_link_from_url(raw_url: &str) -> Result<PullRequestLink, String> {
    let parsed =
        url::Url::parse(raw_url).map_err(|err| format!("Invalid pull request URL: {err}"))?;
    if parsed.scheme() != "https" || parsed.host_str() != Some("github.com") {
        return Err(
            "Pull request link must be a GitHub pull request URL like https://github.com/owner/repo/pull/123."
                .to_string(),
        );
    }
    let segments = parsed
        .path_segments()
        .map(|segments| {
            segments
                .filter(|segment| !segment.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let [owner, repo, "pull", number] = segments.as_slice() else {
        return Err(
            "Pull request link must use https://github.com/owner/repo/pull/123.".to_string(),
        );
    };
    let number = number
        .parse()
        .map_err(|_| "Pull request URL number must be an unsigned integer.".to_string())?;
    Ok(PullRequestLink {
        owner: (*owner).to_string(),
        repo: (*repo).to_string(),
        number,
        forge: None,
    })
}

/// Parses a Forgejo pull request URL against the configured instance. The URL
/// host (and any base path for subpath installs) must match the instance, and
/// the last segments must be `pulls/{n}` or `pull/{n}`.
#[expect(
    clippy::disallowed_types,
    reason = "Pull request links are public forge URLs stored for display and coordinate inference."
)]
fn forgejo_pull_request_link_from_url(
    raw_url: &str,
    base_url: &str,
) -> Result<PullRequestLink, String> {
    let parsed =
        url::Url::parse(raw_url).map_err(|err| format!("Invalid pull request URL: {err}"))?;
    let base =
        url::Url::parse(base_url).map_err(|err| format!("Invalid Forgejo instance URL: {err}"))?;
    if parsed.scheme() != "https" || parsed.host_str() != base.host_str() {
        return Err(format!(
            "Pull request link must be a {base_host} pull request URL like \
             {base}/owner/repo/pulls/123.",
            base_host = base.host_str().unwrap_or("instance"),
            base = base_url.trim_end_matches('/')
        ));
    }

    let origin_path = base.path().trim_matches('/');
    let segments = parsed
        .path_segments()
        .map(|segments| {
            segments
                .filter(|segment| !segment.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    // Skip the instance base path for subpath installs.
    let scoped = if origin_path.is_empty() {
        segments.as_slice()
    } else {
        let origin_segments: Vec<&str> = origin_path.split('/').collect();
        if segments
            .get(..origin_segments.len())
            .is_some_and(|prefix| prefix == origin_segments.as_slice())
        {
            &segments[origin_segments.len()..]
        } else {
            segments.as_slice()
        }
    };

    let [owner, repo, "pulls" | "pull", number] = scoped else {
        return Err(format!(
            "Pull request link must use {base}/owner/repo/pulls/123.",
            base = base_url.trim_end_matches('/')
        ));
    };
    let number = number
        .parse()
        .map_err(|_| "Pull request URL number must be an unsigned integer.".to_string())?;
    Ok(PullRequestLink {
        owner: (*owner).to_string(),
        repo: (*repo).to_string(),
        number,
        forge: Some(ForgeInstanceRef {
            base_url: base_url.trim_end_matches('/').to_string(),
        }),
    })
}

/// Stored pull request link plus optional live GitHub details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequest {
    pub link:    PullRequestLink,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<PullRequestDetails>,
}

/// Response metadata for `GET /runs/{id}/pull_request`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequestMeta {
    pub details_status:             PullRequestDetailsStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details_unavailable_reason: Option<PullRequestDetailsUnavailableReason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequestResponse {
    pub data: PullRequest,
    pub meta: PullRequestMeta,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum PullRequestDetailsStatus {
    Available,
    Unavailable,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum PullRequestDetailsUnavailableReason {
    IntegrationUnavailable,
    NotFound,
    FetchFailed,
}

/// GitHub user summary for a pull request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequestUser {
    pub login: String,
}

/// Git reference summary for a pull request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequestRef {
    #[serde(rename = "ref")]
    pub ref_name: String,
}

/// Fields mirrored directly from GitHub's pull request REST payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequestGithubDetail {
    pub number:        u64,
    pub title:         String,
    pub body:          Option<String>,
    pub state:         String,
    pub draft:         bool,
    #[serde(default)]
    pub merged:        bool,
    #[serde(default)]
    pub merged_at:     Option<String>,
    pub mergeable:     Option<bool>,
    pub additions:     u64,
    pub deletions:     u64,
    pub changed_files: u64,
    pub html_url:      String,
    pub user:          PullRequestUser,
    pub head:          PullRequestRef,
    pub base:          PullRequestRef,
    pub created_at:    String,
    pub updated_at:    String,
}

/// Live GitHub pull request fields returned only after a successful GitHub API
/// fetch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequestDetails {
    pub title:         String,
    pub body:          Option<String>,
    pub state:         String,
    pub draft:         bool,
    pub merged:        bool,
    pub merged_at:     Option<String>,
    pub mergeable:     Option<bool>,
    pub additions:     u64,
    pub deletions:     u64,
    pub changed_files: u64,
    pub author:        PullRequestUser,
    pub head_branch:   String,
    pub base_branch:   String,
    pub timestamps:    PullRequestTimestamps,
}

impl From<PullRequestGithubDetail> for PullRequestDetails {
    fn from(detail: PullRequestGithubDetail) -> Self {
        Self {
            title:         detail.title,
            body:          detail.body,
            state:         detail.state,
            draft:         detail.draft,
            merged:        detail.merged,
            merged_at:     detail.merged_at,
            mergeable:     detail.mergeable,
            additions:     detail.additions,
            deletions:     detail.deletions,
            changed_files: detail.changed_files,
            author:        detail.user,
            head_branch:   detail.head.ref_name,
            base_branch:   detail.base.ref_name,
            timestamps:    PullRequestTimestamps {
                created_at: detail.created_at,
                updated_at: detail.updated_at,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequestTimestamps {
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckRun {
    pub name:       String,
    pub status:     CheckRunStatus,
    pub conclusion: Option<String>,
    pub html_url:   Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckRunStatus {
    Queued,
    InProgress,
    Completed,
    Unknown,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn pull_request_link_serializes_computed_html_url() {
        let link = PullRequestLink {
            owner:  "fabro-sh".to_string(),
            repo:   "fabro".to_string(),
            number: 270,
            forge:  None,
        };

        assert_eq!(
            serde_json::to_value(link).unwrap(),
            json!({
                "owner": "fabro-sh",
                "repo": "fabro",
                "number": 270,
                "html_url": "https://github.com/fabro-sh/fabro/pull/270"
            })
        );
    }

    #[test]
    fn pull_request_link_rejects_extra_legacy_record_fields() {
        let result = serde_json::from_value::<PullRequestLink>(json!({
            "provider": "github",
            "html_url": "https://github.com/fabro-sh/fabro/pull/270",
            "number": 270,
            "owner": "fabro-sh",
            "repo": "fabro",
            "title": "ignored live metadata"
        }));

        assert!(result.is_err());
    }

    #[test]
    fn forgejo_pull_request_link_serializes_with_forge_instance() {
        let link = PullRequestLink::on_forge("https://forgejo.example.com", "acme", "widgets", 12);

        assert_eq!(
            serde_json::to_value(&link).unwrap(),
            json!({
                "owner": "acme",
                "repo": "widgets",
                "number": 12,
                "html_url": "https://forgejo.example.com/acme/widgets/pulls/12",
                "forge": { "base_url": "https://forgejo.example.com" },
            })
        );
        assert_eq!(
            link.html_url(),
            "https://forgejo.example.com/acme/widgets/pulls/12"
        );
    }

    #[test]
    fn legacy_github_link_json_round_trips_without_a_forge_key() {
        // Pre-Forgejo stored links carry no `forge` key; they must keep
        // deserializing and re-serializing byte-identically.
        let wire = json!({
            "owner": "fabro-sh",
            "repo": "fabro",
            "number": 270,
            "html_url": "https://github.com/fabro-sh/fabro/pull/270"
        });
        let link: PullRequestLink = serde_json::from_value(wire.clone()).unwrap();
        assert!(link.forge.is_none());
        assert_eq!(serde_json::to_value(&link).unwrap(), wire);
    }

    #[test]
    fn forgejo_pull_request_url_parsing_accepts_pulls_and_pull() {
        let base = "https://forgejo.example.com";
        for path in ["/acme/widgets/pulls/12", "/acme/widgets/pull/12"] {
            let link = PullRequestLink::from_forgejo_url(&format!("{base}{path}"), base).unwrap();
            assert_eq!(link.owner, "acme", "{path}");
            assert_eq!(link.repo, "widgets", "{path}");
            assert_eq!(link.number, 12, "{path}");
            assert_eq!(
                link.forge.as_ref().map(|forge| forge.base_url.as_str()),
                Some(base)
            );
        }
    }

    #[test]
    fn forgejo_pull_request_url_requires_the_configured_instance() {
        let base = "https://forgejo.example.com";
        let result =
            PullRequestLink::from_forgejo_url("https://github.com/acme/widgets/pulls/12", base);
        assert!(result.is_err());
    }

    #[test]
    fn forgejo_pull_request_url_matches_subpath_instances() {
        let base = "https://example.com/forgejo";
        let link = PullRequestLink::from_forgejo_url(
            "https://example.com/forgejo/acme/widgets/pulls/7",
            base,
        )
        .unwrap();
        assert_eq!(link.owner, "acme");
        assert_eq!(link.repo, "widgets");
        assert_eq!(link.number, 7);
    }

    #[test]
    fn forgejo_pull_request_deserialize_validates_html_url_against_instance() {
        let wire = json!({
            "owner": "acme",
            "repo": "widgets",
            "number": 12,
            "html_url": "https://other.example.com/acme/widgets/pulls/12",
            "forge": { "base_url": "https://forgejo.example.com" },
        });
        let result = serde_json::from_value::<PullRequestLink>(wire);
        assert!(result.is_err());
    }
}
