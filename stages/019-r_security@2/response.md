Review complete. I read the full new crate, the webhook/clone/PR/install/preflight paths with their surrounding context, and ran the affected test suites.

## Finding 1 — `FORGEJO_*` secrets missing from the vault-secret registry (confirmed, test red)

**File:** `lib/foundation/fabro-static/src/secret_registry.rs` — the production `OPTIONAL_VAULT_SECRETS` list (lines 17–41) was not extended, but this change's own test `classifies_optional_vault_secrets` (lines 91–92) asserts `FORGEJO_TOKEN` and `FORGEJO_WEBHOOK_SECRET` are `OptionalVault`. I ran it:

```
cargo test -p fabro-static --lib
... classifies_optional_vault_secrets ... FAILED
assertion `left == right` failed: FORGEJO_TOKEN
  left: None, right: Some(OptionalVault)
```

**What happens:** the startup migration `lib/apps/fabro-server/migrations/2026052501_optional_server_env_secrets_to_vault.rs` iterates `optional_vault_secrets()` to drain legacy `server.env` entries into the vault. With `FORGEJO_TOKEN`/`FORGEJO_WEBHOOK_SECRET` unclassified, an operator who puts either in `server.env` (the legacy location that migration exists to empty) gets **neither** migration into the vault **nor** removal from the plaintext file. Meanwhile every server-side Forgejo resolution is vault-only (`vault.get(...)` in `resolve_startup_forgejo`, `AppState::forgejo_config`, and the `FORGEJO_WEBHOOK_SECRET` snapshot in `build_app_state`), so the integration and webhook route silently stay unconfigured.

**What an attacker gets:** read access to the server's config directory yields a live, long-lived, server-wide Forgejo PAT (repo read/write) and the webhook HMAC key from `server.env` — where the change's own design comments and docs promise "the access token never belongs in settings files." This is a secret-handling/plaintext-retention defect (plus a guaranteed CI failure), not an injection sink. Looks like a fan-in merge casualty: the test half of the edit survived, the list half didn't.

**Fix:** add `EnvVars::FORGEJO_TOKEN` and `EnvVars::FORGEJO_WEBHOOK_SECRET` to `OPTIONAL_VAULT_SECRETS` (matching the test).

## Verified sound — no other security problems found

- **Webhook (`forgejo_webhooks.rs`):** constant-time `verify_slice` HMAC; hex/length mismatch rejected; missing header → 401 with auth slot marked invalid; route only mounted when a secret is configured; handler is inert (logs event type/delivery/repo name only). Parity with the GitHub route.
- **Token secrecy:** `SecretString`/`ForgejoCredentials` render redacted in `Debug`; raw token URLs appear only inside `shell_quote`d git commands; Docker failure paths redact via `redact_auth_url`, Daytona's forge path reports stdout/label only; sandbox events and persisted records carry the credential-stripped origin; diagnostics and docs print username/URL only; CLI collects the PAT via password prompt; web form keeps it in component state.
- **SSRF:** no caller can steer the server's Forgejo client elsewhere — stored `forge` values must equal the configured instance exactly before any call (`prepare_pull_request_host`), and `forgejo_pull_request_link_from_url` enforces HTTPS + host + port. Owner/repo are interpolated only into paths on the fixed instance.
- **Injection/traversal:** clone/fetch/`remote set-url` commands all `shell_quote` their inputs; the git bridge uses the `GIT_CONFIG_KEY_n/VALUE_n` mechanism with instance-validated values and no secret in any config value; workspace `owner`/`repo` components are validated (rejects `..`, `%2e%2e`, non-alnum).
- **Authz:** new install endpoints sit behind the install-token gate with validate-before-persist; `/system/integrations` unchanged behind `RequiredUser` and exposes only the URL + configured booleans; `FORGEJO_TOKEN` reaches a sandbox only on explicit run opt-in with the origin on the configured instance.
- **Dependencies/features:** no new external crates; the `test-support` HTTP allowance on `ForgejoInstance` is enabled only by the twin test crate; the twin's fixture token is synthetic test-only; `.env.example` ships empty values.