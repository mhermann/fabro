use anyhow::{Result, bail};
use fabro_sandbox::daytona::detect_repo_info;

/// The configured Forgejo instance URL when `origin` points at it.
fn forgejo_instance_url_matching(origin: &str) -> Option<String> {
    let resolved = fabro_config::ServerSettingsBuilder::load_default().ok()?;
    let settings = resolved.server.integrations.forgejo;
    settings
        .instance_url()
        .filter(|url| fabro_types::origin_matches_instance(origin, url))
        .map(str::to_string)
}

/// Normalize the checkout's origin for comparison against the run's expected
/// repository. SSH spellings of the configured Forgejo instance rewrite to
/// the instance HTTPS URL; everything else keeps the GitHub normalization.
fn normalize_current_origin(origin: &str, forgejo_instance: Option<&str>) -> String {
    match forgejo_instance {
        Some(instance) => fabro_forgejo::normalize_origin_url(instance, origin),
        None => fabro_github::normalize_repo_origin_url(origin),
    }
}

pub(crate) fn ensure_matching_repo_origin(
    expected_origin_url: Option<&str>,
    action: &str,
) -> Result<()> {
    let Some(expected_origin_url) = expected_origin_url else {
        return Ok(());
    };

    let cwd = std::env::current_dir()?;
    let (origin_url, _) = detect_repo_info(&cwd).map_err(|_| {
        anyhow::anyhow!(
            "Current directory is not a git repository with an origin remote; refusing to {action} run from repository '{expected_origin_url}'"
        )
    })?;
    // Forgejo origins compare against the instance-aware normalizer so SSH
    // checkouts (`git@host:owner/repo`) match the expected HTTPS origin
    // instead of failing the guard on a spelling difference.
    let instance = forgejo_instance_url_matching(&origin_url);
    let current_origin_url = normalize_current_origin(&origin_url, instance.as_deref());

    if current_origin_url != expected_origin_url {
        bail!(
            "Current repository origin '{current_origin_url}' does not match run repository '{expected_origin_url}'; refusing to {action} this run from the wrong checkout"
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ensure_matching_repo_origin, normalize_current_origin};

    #[test]
    fn missing_expected_origin_skips_guard() {
        ensure_matching_repo_origin(None, "fork").unwrap();
    }

    #[test]
    fn forgejo_ssh_origin_compares_equal_to_the_expected_https_origin() {
        // An SSH checkout of a forgejo repo normalizes to the instance HTTPS
        // origin, so the guard does not reject a legitimate checkout.
        assert_eq!(
            normalize_current_origin(
                "git@git.example.com:acme/widgets.git",
                Some("https://git.example.com"),
            ),
            "https://git.example.com/acme/widgets"
        );
        assert_eq!(
            normalize_current_origin(
                "https://git.example.com/acme/widgets.git",
                Some("https://git.example.com"),
            ),
            "https://git.example.com/acme/widgets"
        );
        // Non-forgejo origins keep the historical normalization.
        assert_eq!(
            normalize_current_origin("https://github.com/acme/widgets", None),
            "https://github.com/acme/widgets"
        );
    }
}
