//! Forgejo credential resolution for CLI-side runs.

use fabro_types::settings::server::ForgejoIntegrationSettings;
use fabro_vault::Vault;

use crate::shared::github::lookup_env_or_vault;

/// Resolves the Forgejo runtime credential bundle from settings and the
/// token in the environment or vault.
///
/// `Ok(None)` when the integration is disabled or its URL is missing; a
/// missing token is an error, mirroring the GitHub token behavior.
pub(crate) fn build_forgejo_context(
    settings: &ForgejoIntegrationSettings,
    vault: &Vault,
) -> anyhow::Result<Option<fabro_forgejo::ForgejoContext>> {
    let Some(url) = settings.instance_url() else {
        return Ok(None);
    };
    let token = lookup_env_or_vault(fabro_static::EnvVars::FORGEJO_TOKEN, vault)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "FORGEJO_TOKEN not configured — run fabro install or set FORGEJO_TOKEN"
            )
        })?;
    Ok(Some(fabro_forgejo::ForgejoContext::new(token, url)))
}
