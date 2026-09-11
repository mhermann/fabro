Review complete. I've read the full diff (127 files) and dug into the security-relevant surfaces: the new `fabro-forgejo` crate, sandbox clone commands (Docker + Daytona), webhook HMAC route, install/token persistence, token threading (env injection, git bridge, PR host dispatch), and the CLI/web/doctor surfaces.

## Finding 1 — `FORGEJO_TOKEN` / `FORGEJO_WEBHOOK_SECRET` never registered as vault secrets (plaintext persistence + silently dead integration)

**File:** `lib/foundation/fabro-static/src/secret_registry.rs`

The tests were updated to assert both secrets are optional-vault secrets (`classifies_optional_vault_secrets`, lines 91–92), but the production `OPTIONAL_VAULT_SECRETS` array (lines 17–41) was **not** — `FORGEJO_TOKEN` and `FORGEJO_WEBHOOK_SECRET` are absent from it.

What actually goes wrong:

- The env→vault migration sweeps on **every server startup** (`serve.rs:683` → `migrations/2026052501_optional_server_env_secrets_to_vault.rs` iterates `optional_vault_secrets()`). It moves secrets out of `server.env`/process env into the store and **removes the plaintext copy from the env file**. Forgejo secrets are invisible to it: an operator who configures the PAT or webhook secret via env — exactly the way `GITHUB_TOKEN` and every other secret in that list is handled — keeps the token in plaintext env config indefinitely, with no scrub.
- Server-side, the token is read **only from the vault** (`resolve_startup_forgejo` in `server.rs`, `AppState::forgejo_config`, and `forgejo_webhook_secret` for route mounting). So the env-configured deployment gets a silently disabled integration ("Forgejo integration disabled" at startup, webhook route unmounted) while the credential sits in the env file — the operator has no signal that their secret is being ignored.
- The added test is deterministic-failing as written: `secret_scope(FORGEJO_TOKEN)` returns `None` (it's in neither array) while the assert expects `Some(SecretScope::OptionalVault)`. `fabro-static`'s suite cannot be green, which also means the "2,200+ passing tests" validation didn't cover this crate — and this exact test is the gate that would have caught the omission.

No direct attacker capability, but it's a secrets-hygiene regression in the mechanism this codebase uses to keep credentials out of plaintext env files, plus a broken test gate. Fix is two lines in the production array.

## Finding 2 (low) — operator-gated SSRF probe in Forgejo token validation

**File:** `lib/apps/fabro-server/src/install.rs` (`validate_forgejo_token`)

The GitHub token test validates against the configured upstream; the Forgejo one issues `GET {input.url}/api/v1/user` with the submitted token in `Authorization` to an **arbitrary HTTPS host** chosen in the request. A holder of the install-session token can point it at internal HTTPS hosts and use the success/failure status as a probe oracle, plus read back any `login` field the target returns as JSON (the `ForgejoInstance::new` validation constrains scheme/credentials/query, not the host). Requires install-token access, which already grants broader configuration control (LLM/object-store/sandbox endpoints), so impact is marginal — noting it for the asymmetry with the GitHub path.

## Checked and clean

- **Shell injection:** every token-bearing URL, repo path, branch/tag/refspec, and checkout path in the Docker and Daytona command builders goes through `shell_quote`; owner/repo components are validated (`validate_path_component` rejects `.`/`..`); no manual quoting anywhere in the new code.
- **Token scoping/embedding:** the PAT is embedded only into URLs whose normalized origin sits on the configured instance. I tried the usual spoofing shapes against `is_forgejo_origin`/`normalize_forgejo_origin_url` (`git.example.com@evil.com`, `git.example.com:443@evil.com`, subpath confusion, `:80` port tricks) — none yields a prefix match that would send the token off-instance. Sandbox `FORGEJO_TOKEN` injection is fail-closed and gated on run origin ∈ instance (`resolve_forgejo_token`), and the git credential-helper is instance-scoped so the token is never handed to other hosts.
- **Redaction:** `SecretString`/`ForgejoCredentials` redact `Debug`; errors use `DisplaySafeUrl::redacted_string`; clone events emit the token-free origin; `report_clone_failure` uses the unauthenticated origin.
- **Webhook:** HMAC-SHA256 verified (constant-time `verify_slice`) before any handling; route mounted only when the secret exists; handler is inert verify-and-log; missing/invalid signature returns 401 and invalidates the auth slot, matching the GitHub route.
- **Authorization:** install endpoints (`PUT /install/forgejo/token`, `POST .../test`) are guarded by the same `require_valid_token` install-session check as their GitHub counterparts and declare `security: []` consistently with them. Stored PR records can't redirect server API calls: `record.forge` is always the server-configured instance string (set at link/create time, never taken from client input), and `prepare_pull_request_host` requires exact equality with the configured instance before using the PAT. Token goes to the vault, never `settings.toml`.
- **No committed secrets, no new external dependencies** (only internal crates + existing workspace deps; hmac/sha2/hex already in the lock). The `test-support` HTTP-instance escape hatch is enabled only by `test/twin/forgejo`; the twin's fixture token is fake. `chrono` is an unused-looking dep in `fabro-forgejo` — cosmetic, not security.