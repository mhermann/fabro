//! Instance-aware URL helpers for Forgejo repositories.
//!
//! Every helper is anchored on the configured instance base URL, so origins
//! are classified and normalized against the deployment's own Forgejo host
//! rather than a hard-coded public forge. Host matching delegates to
//! [`fabro_types::origin_matches_instance`] so SSH spellings, ports, and
//! case handling agree with run-target validation.

use anyhow::{Context as _, bail};
use fabro_redact::DisplaySafeUrl;
use fabro_types::origin_matches_instance;


/// Canonical credential-free HTTPS URL for a repository on the instance.
#[must_use]
pub fn repo_https_url(instance_url: &str, owner: &str, repo: &str) -> String {
    format!(
        "{}/{}",
        instance_url.trim().trim_end_matches('/'),
        repository_path(owner, repo)
    )
}

fn repository_path(owner: &str, repo: &str) -> String {
    let repo = repo.strip_suffix(".git").unwrap_or(repo);
    format!("{owner}/{repo}")
}

/// Parses `owner` and `repo` from a repository URL hosted on the given
/// instance.
///
/// Accepts HTTPS spellings (with or without embedded credentials, trailing
/// slash, or `.git` suffix) and the SSH spellings of the same instance
/// (`git@host:owner/repo.git`, `ssh://git@host/owner/repo`). Paths beyond
/// `owner/repo` are rejected so callers never act on a non-repository URL.
pub fn parse_owner_repo(instance_url: &str, repo_url: &str) -> anyhow::Result<(String, String)> {
    let display_url = redacted(repo_url);
    let path = repository_path_of(repo_url, &display_url)?;
    if !origin_matches_instance(repo_url, instance_url) {
        bail!("URL is not hosted on the configured Forgejo instance: {display_url}");
    }

    let path = path.trim_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut parts = path.split('/');
    let owner = parts
        .next()
        .filter(|segment| !segment.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Missing owner in Forgejo URL: {display_url}"))?;
    let repo = parts
        .next()
        .filter(|segment| !segment.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Missing repo in Forgejo URL: {display_url}"))?;
    if parts.next().is_some() {
        bail!("Forgejo URL must identify one repository: {display_url}");
    }

    Ok((owner.to_string(), repo.to_string()))
}

/// Path component of an HTTPS or SSH repository URL.
fn repository_path_of(repo_url: &str, display_url: &str) -> anyhow::Result<String> {
    if let Some(rest) = repo_url.trim().strip_prefix("git@") {
        let (_, path) = rest
            .split_once(':')
            .context(format!("Invalid SSH URL: {display_url}"))?;
        return Ok(path.to_string());
    }
    let parsed = DisplaySafeUrl::parse(repo_url.trim())
        .context(format!("Invalid repository URL: {display_url}"))?;
    if parsed.scheme() != "https" && parsed.scheme() != "http" && parsed.scheme() != "ssh" {
        bail!("Repository URL must use https, http, or ssh: {display_url}");
    }
    Ok(parsed.path().to_string())
}

/// Normalizes a repository origin against the configured instance.
///
/// The result is a credential-free HTTPS URL: trailing slashes and `.git`
/// suffixes are removed, and SSH spellings of the instance host are rewritten
/// to the instance HTTPS URL so SSH checkouts compare equal to their
/// canonical origin. Origins of other hosts keep their (credential-stripped)
/// spelling.
#[must_use]
pub fn normalize_origin_url(instance_url: &str, origin: &str) -> String {
    let trimmed = origin.trim();
    if origin_matches_instance(trimmed, instance_url) {
        if let Ok(path) = repository_path_of(trimmed, &redacted(trimmed)) {
            let path = path.trim_matches('/');
            let path = path.strip_suffix(".git").unwrap_or(path);
            let mut parts = path.split('/');
            let owner = parts.next().unwrap_or_default();
            let repo = parts.next().unwrap_or_default();
            if parts.next().is_none() && !owner.is_empty() && !repo.is_empty() {
                return repo_https_url(instance_url, owner, repo);
            }
        }
    }

    // Non-instance (or malformed) origins fall back to credential-stripping
    // normalization, matching the GitHub behavior for other hosts.
    let stripped = strip_credentials(trimmed);
    let without_suffix = stripped.strip_suffix(".git").unwrap_or(&stripped);
    without_suffix.trim_end_matches('/').to_string()
}

fn strip_credentials(url: &str) -> String {
    let trimmed = url.trim();
    let Some(rest) = trimmed.strip_prefix("https://") else {
        return trimmed.to_string();
    };
    match rest.split_once('@') {
        Some((before, after)) if !before.contains('/') => format!("https://{after}"),
        _ => trimmed.to_string(),
    }
}

fn redacted(url: &str) -> String {
    DisplaySafeUrl::parse(url.trim())
        .map_or_else(|_| "<invalid url>".to_string(), |parsed| parsed.redacted_string())
}

/// Embeds the PAT into an instance HTTPS URL for an authenticated clone.
///
/// The returned [`DisplaySafeUrl`] renders its password as `***` in `Display`
/// output; callers that need the raw URL use [`DisplaySafeUrl::as_raw_url`].
pub fn embed_token_in_url(url: &str, token: &str) -> anyhow::Result<DisplaySafeUrl> {
    let mut url = DisplaySafeUrl::parse(url).context("Failed to parse Forgejo HTTPS URL")?;
    if url.scheme() != "https" && url.scheme() != "http" {
        bail!(
            "Forgejo clone URL must use HTTPS: {}",
            url.redacted_string()
        );
    }
    url.set_username("fabro")
        .map_err(|()| anyhow::anyhow!("Failed to set Forgejo token username"))?;
    url.set_password(Some(token))
        .map_err(|()| anyhow::anyhow!("Failed to set Forgejo token password"))?;
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ForgejoContext;

    const INSTANCE: &str = "https://git.example.com";

    #[test]
    fn repo_https_url_trims_and_strips_git_suffix() {
        assert_eq!(
            repo_https_url(INSTANCE, "acme", "widgets"),
            "https://git.example.com/acme/widgets"
        );
        assert_eq!(
            repo_https_url("http://localhost:3001/", "acme", "widgets.git"),
            "http://localhost:3001/acme/widgets"
        );
    }

    #[test]
    fn parse_owner_repo_accepts_instance_spellings() {
        let cases = [
            "https://git.example.com/acme/widgets",
            "https://git.example.com/acme/widgets.git",
            "https://git.example.com/acme/widgets/",
            "https://fabro:token@git.example.com/acme/widgets.git",
            "git@git.example.com:acme/widgets.git",
            "ssh://git@git.example.com/acme/widgets",
        ];
        for case in cases {
            assert_eq!(
                parse_owner_repo(INSTANCE, case).expect(case),
                ("acme".to_string(), "widgets".to_string()),
                "{case}"
            );
        }
    }

    #[test]
    fn parse_owner_repo_rejects_other_hosts_and_deep_paths() {
        for case in [
            "https://github.com/acme/widgets",
            "https://other.example.com/acme/widgets",
            "https://git.example.com/acme/widgets/pulls/3",
            "https://git.example.com/only-owner",
            "git@github.com:acme/widgets.git",
        ] {
            assert!(parse_owner_repo(INSTANCE, case).is_err(), "{case}");
        }
    }

    #[test]
    fn parse_owner_repo_respects_instance_ports() {
        let instance = "http://localhost:3001";
        assert_eq!(
            parse_owner_repo(instance, "http://localhost:3001/acme/widgets").expect("same port"),
            ("acme".to_string(), "widgets".to_string())
        );
        assert!(
            parse_owner_repo(instance, "http://localhost:3000/acme/widgets").is_err(),
            "a different port is a different service"
        );
        // SSH spellings have no port and match by host.
        assert_eq!(
            parse_owner_repo(instance, "git@localhost:acme/widgets.git").expect("ssh spelling"),
            ("acme".to_string(), "widgets".to_string())
        );
    }

    #[test]
    fn normalize_origin_url_rewrites_instance_ssh_spellings() {
        for origin in [
            "git@git.example.com:acme/widgets.git",
            "ssh://git@git.example.com/acme/widgets",
        ] {
            assert_eq!(
                normalize_origin_url(INSTANCE, origin),
                "https://git.example.com/acme/widgets",
                "{origin}"
            );
        }
    }

    #[test]
    fn normalize_origin_url_passes_through_https_and_credentials() {
        assert_eq!(
            normalize_origin_url(INSTANCE, "https://git.example.com/acme/widgets.git"),
            "https://git.example.com/acme/widgets"
        );
        assert_eq!(
            normalize_origin_url(INSTANCE, "https://fabro:token@git.example.com/acme/widgets"),
            "https://git.example.com/acme/widgets"
        );
        // Non-instance hosts keep their spelling (credential-stripped).
        assert_eq!(
            normalize_origin_url(INSTANCE, "https://other.example.com/acme/widgets.git"),
            "https://other.example.com/acme/widgets"
        );
    }

    #[test]
    fn embed_token_in_url_sets_username_and_redacts_display() {
        let url =
            embed_token_in_url("https://git.example.com/acme/widgets", "s3cret").expect("url");
        assert_eq!(
            url.as_raw_url().as_str(),
            "https://fabro:s3cret@git.example.com/acme/widgets"
        );
        assert!(!url.to_string().contains("s3cret"), "{}", url);

        assert!(embed_token_in_url("git@git.example.com:acme/widgets", "s3cret").is_err());
    }

    #[test]
    fn context_hides_the_token_from_debug() {
        let ctx = ForgejoContext::new("s3cret", "https://git.example.com/");
        let debug = format!("{ctx:?}");
        assert!(!debug.contains("s3cret"), "{debug}");
        assert_eq!(ctx.base_url(), "https://git.example.com");
        assert_eq!(ctx.token(), "s3cret");
    }
}
