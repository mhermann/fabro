I've completed the security review of the Forgejo integration. Here is my assessment.

## Scope reviewed

New `fabro-forgejo` client crate, sandbox clone/push credential paths (`clone_source.rs`, `push_credentials.rs`, `docker.rs`, `daytona/mod.rs`, `sandbox_spec.rs`), workflow env bridge (`git_bridge.rs`, `pipeline/initialize.rs`, `publish.rs`), server routes and stored-PR handling (`server.rs`, `handler/system.rs`, `handler/pull_requests.rs`, `pull_request_supervisor.rs`, `run_manifest.rs`), config validation (`resolve/server.rs`), CLI install (`commands/install.rs`, `shared/forgejo.rs`), `PullRequestLink.forge` type/OpenAPI changes, and the full diff scanned for committed secrets.

## What holds up well

- **Origin classification** (`is_forgejo_origin`): case-insensitive host equality with correct base-path prefix handling — `host.evil.test`, `forgejo-evil/` subpaths, and github.com origins are all rejected (tested).
- **Workspace layout**: `validate_path_component` allows only `[A-Za-z0-9._-]` and rejects `.`/`..`, so owner/repo can't traverse; all shell interpolations go through `shell_quote`.
- **Token hygiene**: token lives in the vault, never settings.toml; `ForgejoRunCreds`/`ForgejoSandboxCreds` have redacting `Debug`; error strings use `DisplaySafeUrl::redacted_string`; `GitCloneStarted` events carry the credential-stripped origin; the credential helper is a fixed script reading `$FORGEJO_TOKEN`, so no secret is interpolated into git config or argv.
- **Confused-deputy guard**: stored `PullRequestLink.forge.base_url` can *not* redirect the server's token — `server_forgejo_parts` requires `configured_base == forge.base_url`, so a forged/malicious link just gets "integration unavailable". Forgejo `html_url` values from instance responses are stored but never used as request targets.
- **Server route**: `/repos/forgejo/{owner}/{name}` sits behind `RequiredUser` with slug validation, mirroring the GitHub route; the demo mirror is likewise auth'd.
- **Config resolution**: `resolve_forgejo_instance_url` enforces https, host presence, and no embedded credentials, and resolve errors are fatal on settings load (fail-closed).
- `embed_token_in_url` refuses non-HTTPS URLs, so git transport can't go plaintext. No new external dependencies; no secrets committed; no XSS sinks in the web changes.

## Findings

**1. LOW — CLI install accepts a plaintext `http://` instance URL despite claiming https-only.**
`lib/apps/fabro-cli/src/commands/install.rs` (`run_install_forgejo_inner`) validates `--url` via `fabro_forgejo::forgejo_base_url(...)`, which only normalizes (ssh→https, strip userinfo) and never checks the scheme — the https check exists only in `fabro-config`'s `resolve_forgejo_instance_url`. An operator running `fabro install forgejo --url http://host` gets the token transmitted in a cleartext `Authorization: token` header during verification. What an on-path attacker gets: the Forgejo API token. The misconfig is later caught (settings resolution fails closed), but the cleartext transmission has already happened. Fix: enforce https in the CLI path or in `forgejo_base_url` itself, matching the resolve-time check and the command's own error message.

**2. LOW — server-side `FORGEJO_URL` env fallback is used raw.**
`AppState::forgejo_base_url()` (`lib/apps/fabro-server/src/server.rs`) applies only `trim`/trailing-slash to the env value — unlike settings (validated credential-free) and unlike the CLI helper (`normalize_repo_origin_url` strips userinfo). If an operator sets `FORGEJO_URL=https://user:pass@host`, that value flows into the run credentials and is persisted as `PullRequestLink.forge.base_url`, exposing the embedded credential in API responses and the UI; an `http://` value enables cleartext API calls. Requires operator misconfiguration, and settings-based config is validated — but the two fallback paths normalize inconsistently.

**3. INFO — `FORGEJO_TOKEN` is injected into every run's sandbox env when Forgejo is configured, regardless of run origin.**
`build_sandbox_env` (`pipeline/initialize.rs`) inserts the token unconditionally when creds exist; only the credential-helper bridge is origin-gated. GitHub-origin runs that need nothing from Forgejo still receive the raw instance PAT in env. This is comparable to GitHub's token-strategy posture (raw PAT in env) but broader than needed; gating the insert on `is_forgejo_origin` the way the bridge is gated would shrink the blast radius.

**4. INFO — port is ignored when classifying Forgejo origins.**
`is_forgejo_origin` compares host+path only, so `https://host:8443/owner/repo` classifies as the instance configured at `https://host`. Consequence is a functional failure (the credential-helper key includes the port, so auth won't resolve), not credential exposure, and it stays within one administrative host.

No injection, traversal, unsafe deserialization, authorization bypass, SSRF, or secret-commit issues were found. Findings 1 and 2 are the only ones I'd consider worth fixing before ship; both are operator-input hardening gaps rather than remotely exploitable paths.