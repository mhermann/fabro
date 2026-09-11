use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{DirtyStatus, GitContext, GitHubRepositorySlug, RunId, ScmProvider, WorkflowVersionId, repository};

/// A request to create a run from an immutable workflow version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunIntent {
    pub workflow_version_id: WorkflowVersionId,
    pub target:              RunTarget,
    pub args:                RunIntentArgs,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment_id:      Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id:           Option<RunId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title:               Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal:                Option<String>,
}

/// Structured run-setting overrides accepted by [`RunIntent`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunIntentArgs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model:            Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider:         Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub inputs:           HashMap<String, Value>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub labels:           HashMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dry_run:          Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_approve:     Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preserve_sandbox: Option<bool>,
}

/// Requested workspace content, independent of sandbox placement.
///
/// `None` is an empty struct variant rather than a unit variant so that the
/// derived deserializer enforces `deny_unknown_fields` on `{"kind": "none"}`
/// (serde ignores sibling fields on internally tagged unit variants).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, strum::IntoStaticStr)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[strum(serialize_all = "snake_case")]
pub enum RunTarget {
    Git(GitRunTarget),
    None {},
    Folder { path: String },
}

/// A Git-backed run target.
///
/// `branch` is always the attached working branch. When present, `tag` names
/// the requested release identity. An exact `sha` is authoritative over both
/// selectors while preserving the tag in durable state.
///
/// `provider` names the forge hosting `repo`. It defaults to
/// [`ScmProvider::Github`] on the wire, so every coordinate written before
/// Forgejo existed still deserializes unchanged; forgejo targets additionally
/// require a configured instance URL at validation time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitRunTarget {
    pub repo:     String,
    pub branch:   String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag:      Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha:      Option<String>,
    #[serde(default, skip_serializing_if = "is_github_provider")]
    pub provider: ScmProvider,
}

fn is_github_provider(provider: &ScmProvider) -> bool {
    *provider == ScmProvider::Github
}

impl GitRunTarget {
    /// Validates and canonicalizes this Git coordinate without resolving remote
    /// repository state.
    ///
    /// # Errors
    ///
    /// Returns an error when the repository slug, branch, tag, or exact commit
    /// does not use the canonical grammar accepted for Git-backed runs.
    pub fn validate(self) -> Result<ValidatedGitRunTarget, GitCoordinateValidationError> {
        self.validate_with_scm(None)
    }

    /// Validates this Git coordinate against an optionally configured Forgejo
    /// instance.
    ///
    /// Forgejo targets resolve their origin from `forgejo_url`; GitHub targets
    /// ignore it and keep the github.com origin. Callers without a Forgejo
    /// deployment pass `None`, which makes this behave exactly like
    /// [`GitRunTarget::validate`].
    ///
    /// # Errors
    ///
    /// Returns an error when the repository slug, branch, tag, or exact commit
    /// does not use the canonical grammar accepted for Git-backed runs, or
    /// when a forgejo target has no configured instance URL.
    pub fn validate_with_scm(
        self,
        forgejo_url: Option<&str>,
    ) -> Result<ValidatedGitRunTarget, GitCoordinateValidationError> {
        let Self {
            repo,
            branch,
            tag,
            sha,
            provider,
        } = self;
        let repository =
            GitHubRepositorySlug::try_new(&repo).ok_or(GitCoordinateValidationError::Repository)?;
        if !repository::is_valid_git_branch_name(&branch) {
            return Err(GitCoordinateValidationError::Branch);
        }
        if tag
            .as_deref()
            .is_some_and(|tag| !repository::is_valid_git_tag_name(tag))
        {
            return Err(GitCoordinateValidationError::Tag);
        }
        let sha = sha
            .map(|sha| {
                repository::normalize_git_commit_sha(&sha).ok_or(GitCoordinateValidationError::Sha)
            })
            .transpose()?;
        let origin_url = match provider {
            ScmProvider::Github => repository.https_url(),
            ScmProvider::Forgejo => {
                let forgejo_url = forgejo_url
                    .filter(|url| !url.trim().is_empty())
                    .ok_or(GitCoordinateValidationError::ForgejoUnconfigured)?;
                format!("{forgejo_url}/{repository}")
            }
        };
        let git = GitContext {
            origin_url,
            branch:     branch.clone(),
            sha:        sha.clone(),
            dirty:      DirtyStatus::Clean,
        };
        Ok(ValidatedGitRunTarget {
            target: Self {
                repo,
                branch,
                tag,
                sha,
                provider,
            },
            repository,
            git,
        })
    }
}

impl RunTarget {
    /// The wire `kind` discriminator (`git`, `none`, or `folder`), for
    /// diagnostics.
    pub fn kind_name(&self) -> &'static str {
        self.into()
    }

    /// Validates and canonicalizes the target without any network resolution.
    ///
    /// Git targets include their derived operational Git projection. Targets
    /// without a repository return no projection. Folder paths require
    /// filesystem validation and canonicalization during provider admission.
    ///
    /// # Errors
    ///
    /// Returns an error when a Git target's repository slug, branch, tag, or
    /// exact commit does not use the canonical grammar accepted for runs.
    pub fn validate(self) -> Result<ValidatedRunTarget, TargetValidationError> {
        self.validate_with_scm(None)
    }

    /// Validates the target against an optionally configured Forgejo instance.
    ///
    /// # Errors
    ///
    /// Returns an error when a Git target's repository slug, branch, tag, or
    /// exact commit does not use the canonical grammar accepted for runs, or
    /// when a forgejo target has no configured instance URL.
    pub fn validate_with_scm(
        self,
        forgejo_url: Option<&str>,
    ) -> Result<ValidatedRunTarget, TargetValidationError> {
        match self {
            Self::Git(target) => {
                let validated = target
                    .validate_with_scm(forgejo_url)
                    .map_err(TargetValidationError::from)?;
                Ok(ValidatedRunTarget {
                    target: Self::Git(validated.target),
                    git:    Some(validated.git),
                })
            }
            Self::None {} => Ok(ValidatedRunTarget {
                target: Self::None {},
                git:    None,
            }),
            Self::Folder { path } => Ok(ValidatedRunTarget {
                target: Self::Folder { path },
                git:    None,
            }),
        }
    }
}

/// A [`GitRunTarget`] whose local grammar has been validated and canonicalized.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedGitRunTarget {
    target:     GitRunTarget,
    repository: GitHubRepositorySlug,
    git:        GitContext,
}

/// A [`ValidatedGitRunTarget`] that also knows which forge it targets.
impl ValidatedGitRunTarget {
    /// The canonical Git target coordinate.
    #[must_use]
    pub fn target(&self) -> &GitRunTarget {
        &self.target
    }

    /// The parsed repository named by the target.
    #[must_use]
    pub fn repository(&self) -> &GitHubRepositorySlug {
        &self.repository
    }

    /// The forge this target runs against.
    #[must_use]
    pub fn provider(&self) -> ScmProvider {
        self.target.provider
    }

    /// Consume the validation proof and return the canonical Git target.
    #[must_use]
    pub fn into_target(self) -> GitRunTarget {
        self.target
    }
}

/// A [`RunTarget`] whose grammar has been validated, together with its
/// optional operational Git projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedRunTarget {
    pub target: RunTarget,
    pub git:    Option<GitContext>,
}

/// A Git coordinate that failed local grammar validation.
#[derive(Debug, thiserror::Error, Clone, Copy, PartialEq, Eq)]
pub enum GitCoordinateValidationError {
    #[error("repository must be a valid owner/name slug")]
    Repository,
    #[error("branch must be a non-empty branch name, not a ref or commit selector")]
    Branch,
    #[error("tag must be a non-empty bare tag name, not a ref or commit selector")]
    Tag,
    #[error("SHA must be exactly 40 ASCII hexadecimal characters")]
    Sha,
    #[error("forgejo targets require a configured Forgejo instance URL")]
    ForgejoUnconfigured,
}

/// A [`RunTarget`] that failed grammar validation.
#[derive(Debug, thiserror::Error, Clone, Copy, PartialEq, Eq)]
pub enum TargetValidationError {
    #[error("target repository must be a valid owner/name slug")]
    Repository,
    #[error("target branch must be a non-empty branch name, not a ref or commit selector")]
    Branch,
    #[error("target tag must be a non-empty bare tag name, not a ref or commit selector")]
    Tag,
    #[error("target SHA must be exactly 40 ASCII hexadecimal characters")]
    Sha,
    #[error("target forgejo runs require a configured Forgejo instance URL")]
    ForgejoUnconfigured,
}

impl From<GitCoordinateValidationError> for TargetValidationError {
    fn from(error: GitCoordinateValidationError) -> Self {
        match error {
            GitCoordinateValidationError::Repository => Self::Repository,
            GitCoordinateValidationError::Branch => Self::Branch,
            GitCoordinateValidationError::Tag => Self::Tag,
            GitCoordinateValidationError::Sha => Self::Sha,
            GitCoordinateValidationError::ForgejoUnconfigured => Self::ForgejoUnconfigured,
        }
    }
}
