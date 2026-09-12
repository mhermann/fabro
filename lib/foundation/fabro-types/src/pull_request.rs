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

/// Minimal pull request reference stored on a workflow run. GitHub links are
/// the historical default; a `forge` base URL marks a pull request on a
/// self-hosted Forgejo instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequestLink {
    pub owner:  String,
    pub repo:   String,
    pub number: u64,
    /// Instance base URL (e.g. `https://git.example.com`) when this pull
    /// request lives on a configured Forgejo instance. `None` (and omitted
    /// from the wire format) means GitHub.
    pub forge:  Option<String>,
}

impl PullRequestLink {
    #[must_use]
    pub fn html_url(&self) -> String {
        match &self.forge {
            Some(forge) => format!("{forge}/{}/{}/pulls/{}", self.owner, self.repo, self.number),
            None => format!(
                "https://github.com/{}/{}/pull/{}",
                self.owner, self.repo, self.number
            ),
        }
    }

    pub fn from_github_url(url: &str) -> Result<Self, String> {
        github_pull_request_link_from_url(url)
    }

    /// Parse a pull request URL on `instance`
    /// (`https://instance/owner/repo/pulls/123`, GitHub-style
    /// `/pull/123` also accepted) into a Forgejo-scoped link.
    pub fn from_forgejo_url(instance: &str, url: &str) -> Result<Self, String> {
        forgejo_pull_request_link_from_url(instance, url)
    }
}

impl Serialize for PullRequestLink {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // `forge` is omitted entirely for GitHub links so the wire format of
        // existing records stays byte-identical to earlier releases.
        let mut state = serializer
            .serialize_struct("PullRequestLink", if self.forge.is_some() { 5 } else { 4 })?;
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
            forge:    Option<String>,
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
                Some(forge) => forgejo_pull_request_link_from_url(forge, &html_url),
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

/// Parse a pull request URL on a Forgejo instance into a Forgejo-scoped
/// link. Forgejo renders pull requests at `/pulls/{index}`; the GitHub-style
/// `/pull/{number}` path is accepted too because instances redirect it.
#[expect(
    clippy::disallowed_types,
    reason = "Forgejo pull request links are instance URLs stored for display and coordinate inference."
)]
fn forgejo_pull_request_link_from_url(
    instance: &str,
    raw_url: &str,
) -> Result<PullRequestLink, String> {
    let instance = instance.trim_end_matches('/');
    let parsed =
        url::Url::parse(raw_url).map_err(|err| format!("Invalid pull request URL: {err}"))?;
    if parsed.scheme() != "https" {
        return Err(
            "Forgejo pull request link must be an HTTPS URL like https://git.example.com/owner/repo/pulls/123."
                .to_string(),
        );
    }
    let instance_host =
        url::Url::parse(instance).map_err(|err| format!("Invalid Forgejo instance URL: {err}"))?;
    if parsed.host_str() != instance_host.host_str()
        || parsed.port_or_known_default() != instance_host.port_or_known_default()
    {
        return Err(format!(
            "Pull request link host does not match the configured Forgejo instance ({instance})."
        ));
    }
    // Strip the instance's subpath (if any) before matching owner/repo.
    let instance_path = instance_host.path().trim_matches('/');
    let segments = parsed
        .path_segments()
        .map(|segments| {
            segments
                .filter(|segment| !segment.is_empty())
                .skip(
                    instance_path
                        .split('/')
                        .filter(|part| !part.is_empty())
                        .count(),
                )
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let [owner, repo, kind, number] = segments.as_slice() else {
        return Err(
            "Forgejo pull request link must use https://instance/owner/repo/pulls/123.".to_string(),
        );
    };
    if *kind != "pulls" && *kind != "pull" {
        return Err(
            "Forgejo pull request link must use https://instance/owner/repo/pulls/123.".to_string(),
        );
    }
    let number = number
        .parse()
        .map_err(|_| "Pull request URL number must be an unsigned integer.".to_string())?;
    Ok(PullRequestLink {
        owner: (*owner).to_string(),
        repo: (*repo).to_string(),
        number,
        forge: Some(instance.to_string()),
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
    fn forgejo_link_serializes_instance_html_url() {
        let link = PullRequestLink {
            owner:  "acme".to_string(),
            repo:   "widgets".to_string(),
            number: 7,
            forge:  Some("https://git.example.com".to_string()),
        };

        assert_eq!(
            link.html_url(),
            "https://git.example.com/acme/widgets/pulls/7"
        );
        assert_eq!(
            serde_json::to_value(&link).unwrap(),
            json!({
                "owner": "acme",
                "repo": "widgets",
                "number": 7,
                "html_url": "https://git.example.com/acme/widgets/pulls/7",
                "forge": "https://git.example.com"
            })
        );
    }

    #[test]
    fn forgejo_link_round_trips_through_the_wire_format() {
        let link = PullRequestLink {
            owner:  "acme".to_string(),
            repo:   "widgets".to_string(),
            number: 7,
            forge:  Some("https://git.example.com/forge".to_string()),
        };
        let parsed: PullRequestLink =
            serde_json::from_value(serde_json::to_value(&link).unwrap()).unwrap();
        assert_eq!(parsed, link);
    }

    #[test]
    fn forgejo_link_parses_instance_urls() {
        let link = PullRequestLink::from_forgejo_url(
            "https://git.example.com",
            "https://git.example.com/acme/widgets/pulls/7",
        )
        .unwrap();
        assert_eq!(link.owner, "acme");
        assert_eq!(link.repo, "widgets");
        assert_eq!(link.number, 7);
        assert_eq!(link.forge.as_deref(), Some("https://git.example.com"));

        // GitHub-style `/pull/` paths redirect on instances; accept them.
        let link = PullRequestLink::from_forgejo_url(
            "https://git.example.com",
            "https://git.example.com/acme/widgets/pull/8",
        )
        .unwrap();
        assert_eq!(link.number, 8);

        // Subpath instances strip the instance prefix before matching.
        let link = PullRequestLink::from_forgejo_url(
            "https://git.example.com/forge",
            "https://git.example.com/forge/acme/widgets/pulls/9",
        )
        .unwrap();
        assert_eq!(link.number, 9);
        assert_eq!(link.forge.as_deref(), Some("https://git.example.com/forge"));

        assert!(
            PullRequestLink::from_forgejo_url(
                "https://git.example.com",
                "https://other.example.com/acme/widgets/pulls/7",
            )
            .is_err(),
            "a different host must not parse against this instance"
        );
        assert!(
            PullRequestLink::from_forgejo_url(
                "https://git.example.com",
                "https://git.example.com/acme/widgets/issues/7",
            )
            .is_err()
        );
    }

    #[test]
    fn github_link_without_forge_rejects_extra_legacy_record_fields() {
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
}
