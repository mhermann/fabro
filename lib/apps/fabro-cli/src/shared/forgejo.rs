use std::path::Path;

use anyhow::{Context as _, anyhow};
use fabro_config::Storage;
use fabro_forgejo::{ForgejoConfig, ForgejoCredentials, ForgejoInstance};
use fabro_static::EnvVars;
use fabro_types::settings::server::ForgejoIntegrationSettings;
use fabro_vault::{SecretStore, Vault};

/// Resolve the configured Forgejo instance and PAT for local runs: the
/// `[server.integrations.forgejo]` settings (when available) or the
/// `FORGEJO_URL` env var, plus the `FORGEJO_TOKEN` env var or vault secret.
/// `Ok(None)` when the integration is disabled or no URL is configured.
pub(crate) fn build_forgejo_config(
    settings: Option<&ForgejoIntegrationSettings>,
    vault: &Vault,
) -> anyhow::Result<Option<ForgejoConfig>> {
    let url = settings
        .and_then(|settings| settings.url.as_deref())
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_string)
        .or_else(|| lookup_env(EnvVars::FORGEJO_URL));
    let Some(url) = url else {
        return Ok(None);
    };
    if settings.is_some_and(|settings| !settings.enabled) {
        return Ok(None);
    }
    let instance = ForgejoInstance::new(&url)?;
    let token = lookup_forgejo_token(vault).ok_or_else(|| {
        anyhow!("FORGEJO_TOKEN not configured — run fabro install or set FORGEJO_TOKEN")
    })?;
    Ok(Some(ForgejoConfig::new(
        instance,
        ForgejoCredentials::new(token),
    )))
}

/// Resolve the configured Forgejo instance and PAT for CLI commands that
/// target a server: workflow settings carry no server section, so the
/// instance comes from the `FORGEJO_URL` env var and the token from
/// `FORGEJO_TOKEN` (env, then the vault at `storage_dir`). `Ok(None)` when no
/// instance is configured.
pub(crate) async fn resolve_forgejo_config(
    storage_dir: &Path,
) -> anyhow::Result<Option<ForgejoConfig>> {
    let storage = Storage::new(storage_dir);
    let vault = SecretStore::open_snapshot(storage.sqlite_path(), storage.secrets_path())
        .await
        .context("opening the Fabro secret store")?
        .into_vault();
    build_forgejo_config(None, &vault)
}

/// Look up the Forgejo PAT: FORGEJO_TOKEN env -> vault FORGEJO_TOKEN.
fn lookup_forgejo_token(vault: &Vault) -> Option<String> {
    lookup_env_or_vault(EnvVars::FORGEJO_TOKEN, vault)
}

#[expect(
    clippy::disallowed_methods,
    reason = "Forgejo credential resolution intentionally falls back from vault to documented process-env names."
)]
fn lookup_env_or_vault(name: &str, vault: &Vault) -> Option<String> {
    std::env::var(name)
        .ok()
        .or_else(|| vault.get(name).map(str::to_string))
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}

#[expect(
    clippy::disallowed_methods,
    reason = "Forgejo instance URL resolution intentionally reads a documented process-env name."
)]
fn lookup_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}
