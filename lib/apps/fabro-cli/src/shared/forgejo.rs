use fabro_config::ServerSettingsBuilder;
use fabro_forgejo::ForgejoCredentials;
use fabro_static::EnvVars;
use fabro_vault::Vault;

/// Resolve the single configured Forgejo/Gitea instance URL.
///
/// The `FORGEJO_URL` environment variable overrides the resolved
/// `[server.integrations.forgejo].url` setting; `None` means no instance is
/// configured and every origin falls back to GitHub classification.
pub(crate) fn resolve_forgejo_instance_url() -> Option<String> {
    if let Some(env_url) = fabro_forgejo::forgejo_instance_url() {
        return Some(env_url);
    }
    let settings = ServerSettingsBuilder::load_default().ok()?;
    let forgejo = settings.server.integrations.forgejo;
    if !forgejo.enabled {
        return None;
    }
    forgejo.url
}

/// Resolve Forgejo credentials from the environment or the vault.
///
/// Returns `Ok(None)` when no token is configured: callers decide whether a
/// missing token is an error (hard gates) or a soft skip (pull-request
/// paths).
pub(crate) fn build_forgejo_credentials(vault: &Vault) -> Option<ForgejoCredentials> {
    lookup_forgejo_token(vault).map(ForgejoCredentials::Pat)
}

/// Look up the Forgejo token: FORGEJO_TOKEN env -> vault FORGEJO_TOKEN.
#[expect(
    clippy::disallowed_methods,
    reason = "Forgejo credential resolution intentionally falls back from vault to documented process-env names."
)]
fn lookup_forgejo_token(vault: &Vault) -> Option<String> {
    std::env::var(EnvVars::FORGEJO_TOKEN)
        .ok()
        .or_else(|| vault.get(EnvVars::FORGEJO_TOKEN).map(str::to_string))
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
}
