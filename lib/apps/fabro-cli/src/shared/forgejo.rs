use anyhow::anyhow;
use fabro_static::EnvVars;
use fabro_vault::Vault;
use fabro_workflow::operations::ForgejoRunCreds;

/// Build Forgejo run credentials from the configured instance URL plus the
/// `FORGEJO_TOKEN` secret.
///
/// The instance URL comes from the caller (server settings via
/// `forgejo_base_url_from_server_settings`, falling back to `FORGEJO_URL` env
/// inside `fabro_forgejo::forgejo_base_url`); the token resolves
/// `FORGEJO_TOKEN` env -> vault. Returns `Ok(None)` when no instance is
/// configured; an instance without a token is a hard error so a Forgejo run
/// fails fast with a remediation message instead of at clone time.
pub(crate) fn build_forgejo_credentials(vault: &Vault) -> anyhow::Result<Option<ForgejoRunCreds>> {
    let Some(base_url) = forgejo_base_url_from_server_settings() else {
        return Ok(None);
    };
    let token = lookup_forgejo_token(vault);
    match token {
        Some(token) => Ok(Some(ForgejoRunCreds::new(base_url, token))),
        None => Err(anyhow!(
            "FORGEJO_TOKEN not configured — run fabro install or set FORGEJO_TOKEN"
        )),
    }
}

/// The configured instance URL: server settings win over `FORGEJO_URL` env
/// (`forgejo_base_url` consults the env fallback itself).
fn forgejo_base_url_from_server_settings() -> Option<String> {
    let settings = fabro_config::ServerSettingsBuilder::load_default().ok()?;
    let forgejo = &settings.server.integrations.forgejo;
    if !forgejo.enabled {
        return None;
    }
    fabro_forgejo::forgejo_base_url(forgejo.url.as_deref())
}

#[expect(
    clippy::disallowed_methods,
    reason = "Forgejo credential resolution intentionally falls back from vault to the documented process-env name."
)]
fn lookup_forgejo_token(vault: &Vault) -> Option<String> {
    std::env::var(EnvVars::FORGEJO_TOKEN)
        .ok()
        .or_else(|| vault.get(EnvVars::FORGEJO_TOKEN).map(str::to_string))
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}
