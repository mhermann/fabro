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
/// Links point either at github.com (the default, `instance_url == None`) or
/// at the configured Forgejo/Gitea instance (`instance_url == Some`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequestLink {
    pub owner:        String,
    pub repo:         String,
    pub number:       u64,
    /// Base URL of the Forgejo/Gitea instance when this link points at a
    /// self-hosted instance; `None` means github.com.
    pub instance_url: Option<String>,
}

impl PullRequestLink {
    #[must_use]
    pub fn html_url(&self) -> String {
        match &self.instance_url {
            Some(instance) => format!(
                "{}/{}/{}/pulls/{}",
                instance.trim_end_matches('/'),
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

    /// Parses a Forgejo/Gitea pull request URL like
    /// `https://forgejo.example.com/owner/repo/pulls/123`, requiring its host
    /// to match `expected_instance`.
    pub fn from_forge_url(url: &str, expected_instance: &str) -> Result<Self, String> {
        forge_pull_request_link_from_url(url, expected_instance)
    }
}

impl Serialize for PullRequestLink {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let len = if self.instance_url.is_some() { 5 } else { 4 };
        let mut state = serializer.serialize_struct("PullRequestLink", len)?;
        state.serialize_field("owner", &self.owner)?;
        state.serialize_field("repo", &self.repo)?;
        state.serialize_field("number", &self.number)?;
        state.serialize_field("html_url", &self.html_url())?;
        if let Some(instance_url) = &self.instance_url {
            state.serialize_field("instance_url", instance_url)?;
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
            html_url:     Option<String>,
            #[serde(default)]
            instance_url: Option<String>,
            #[serde(default)]
            owner:        Option<String>,
            #[serde(default)]
            repo:         Option<String>,
            #[serde(default)]
            number:       Option<u64>,
        }

        let wire = Wire::deserialize(deserializer)?;
        let (Some(owner), Some(repo), Some(number)) = (wire.owner, wire.repo, wire.number) else {
            return Err(D::Error::custom("missing pull request owner/repo/number"));
        };
        let link = Self {
            owner,
            repo,
            number,
            instance_url: wire.instance_url,
        };

        if let Some(html_url) = wire.html_url {
            let url_link =
                pull_request_link_from_stored_url(&html_url, link.instance_url.as_deref())
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
    let segments = pull_url_segments(&parsed);
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
        instance_url: None,
    })
}

#[expect(
    clippy::disallowed_types,
    reason = "Pull request links are instance URLs stored for display and coordinate inference."
)]
fn forge_pull_request_link_from_url(
    raw_url: &str,
    expected_instance: &str,
) -> Result<PullRequestLink, String> {
    let expected =
        url::Url::parse(expected_instance).map_err(|err| format!("Invalid instance URL: {err}"))?;
    let parsed =
        url::Url::parse(raw_url).map_err(|err| format!("Invalid pull request URL: {err}"))?;
    if parsed.host_str() != expected.host_str() || parsed.port() != expected.port() {
        return Err(format!(
            "Pull request link must belong to the configured Forgejo/Gitea instance {expected_instance}."
        ));
    }
    let segments = pull_url_segments(&parsed);
    let [owner, repo, "pulls", number] = segments.as_slice() else {
        return Err(format!(
            "Pull request link must use {expected_instance}/owner/repo/pulls/123."
        ));
    };
    let number = number
        .parse()
        .map_err(|_| "Pull request URL number must be an unsigned integer.".to_string())?;
    Ok(PullRequestLink {
        owner: (*owner).to_string(),
        repo: (*repo).to_string(),
        number,
        instance_url: Some(expected_instance.trim_end_matches('/').to_string()),
    })
}

/// Validates a stored `html_url` against the link's own instance: forge links
/// are checked against their instance, GitHub links against github.com.
fn pull_request_link_from_stored_url(
    raw_url: &str,
    instance_url: Option<&str>,
) -> Result<PullRequestLink, String> {
    match instance_url {
        Some(instance) => forge_pull_request_link_from_url(raw_url, instance),
        None => github_pull_request_link_from_url(raw_url),
    }
}

#[expect(
    clippy::disallowed_types,
    reason = "Pull request URLs are validated credential-free web URLs; the redacted wrapper is not needed for segment parsing."
)]
fn pull_url_segments(parsed: &url::Url) -> Vec<&str> {
    parsed
        .path_segments()
        .map(|segments| {
            segments
                .filter(|segment| !segment.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
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
            owner:        "fabro-sh".to_string(),
            repo:         "fabro".to_string(),
            number:       270,
            instance_url: None,
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
    fn forge_pull_request_link_renders_pulls_url() {
        let link = PullRequestLink {
            owner:        "fabro-sh".to_string(),
            repo:         "fabro".to_string(),
            number:       12,
            instance_url: Some("https://forgejo.example.com".to_string()),
        };

        assert_eq!(
            link.html_url(),
            "https://forgejo.example.com/fabro-sh/fabro/pulls/12"
        );
        assert_eq!(
            serde_json::to_value(&link).unwrap(),
            json!({
                "owner": "fabro-sh",
                "repo": "fabro",
                "number": 12,
                "html_url": "https://forgejo.example.com/fabro-sh/fabro/pulls/12",
                "instance_url": "https://forgejo.example.com"
            })
        );
    }

    #[test]
    fn forge_pull_request_link_round_trips() {
        let link = PullRequestLink::from_forge_url(
            "https://forgejo.example.com/acme/widget/pulls/7",
            "https://forgejo.example.com",
        )
        .unwrap();

        assert_eq!(link, PullRequestLink {
            owner:        "acme".to_string(),
            repo:         "widget".to_string(),
            number:       7,
            instance_url: Some("https://forgejo.example.com".to_string()),
        });
        assert_eq!(
            serde_json::from_value::<PullRequestLink>(serde_json::to_value(&link).unwrap())
                .unwrap(),
            link
        );
    }

    #[test]
    fn forge_pull_request_link_rejects_other_instance_hosts() {
        let result = PullRequestLink::from_forge_url(
            "https://other.example.com/acme/widget/pulls/7",
            "https://forgejo.example.com",
        );

        assert!(result.is_err());
    }

    #[test]
    fn forge_pull_request_link_rejects_github_style_pull_path() {
        let result = PullRequestLink::from_forge_url(
            "https://forgejo.example.com/acme/widget/pull/7",
            "https://forgejo.example.com",
        );

        assert!(result.is_err());
    }

    #[test]
    fn forge_pull_request_link_serialization_omits_instance_url_for_github() {
        let link =
            PullRequestLink::from_github_url("https://github.com/acme/widget/pull/3").unwrap();
        let value = serde_json::to_value(&link).unwrap();
        assert!(value.get("instance_url").is_none());
        assert_eq!(
            serde_json::from_value::<PullRequestLink>(value).unwrap(),
            link
        );
    }
}
