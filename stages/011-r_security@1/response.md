I've now traced every hunk of the diff (all 24 files), the full source of the new backend, the trust boundaries for the new env vars, and the downstream consumers. Here is my security review.

## Verdict

**The diff introduces no security problems.** I traced every new input-to-sink path; the secret-handling hygiene is consistent with (and in places better than) the existing Brave/Venice backends. Details below.

## Attack surfaces I checked

**Query injection into the HTTP request** (`lib/components/fabro-agent/src/web_search.rs:213-221`). The agent-controlled `query` reaches the only outbound sink via reqwest's `.query(&[("q", query), ...])`, which percent-encodes — no header smuggling, no path manipulation (the path is fixed as `/search` appended to operator config, and query-string structure is fixed). Same mechanism as the existing Brave path.

**Who controls `SEARXNG_URL`** — this is the only genuinely new trust question, and it holds:
- Server/diagnostics path: `config_env_lookup` → `process_env_var` → `std::env::var` (server.rs:1471, interp.rs:16, serve.rs:811). No API handler, run request, demo header, or workflow author input reaches it.
- Worker path: `initialize.rs:324` reads process env, and the worker env is fail-closed via `WORKER_ENV_ALLOWLIST` (`spawn_env.rs:78-85` `env_clear()` + allowlist). The diff adds `SEARXNG_URL` to that allowlist but deliberately **excludes** `SEARXNG_API_KEY`, with a new assertion in `worker_allowlist_is_fail_closed` that the key does not cross. The key reaches the worker only through the vault, like every other integration secret.
- Worst case if an operator points the URL somewhere hostile: Fabro GETs that endpoint with the agent's query and the optional bearer token. That is exactly the pre-existing trust model for `DAYTONA_API_URL`. Note reqwest strips `Authorization` on cross-host redirects, so even a compromised instance can't launder the vault token elsewhere.

**Secret exposure**:
- `ToolSecrets::Debug` is already manual-redaction; the diff extends it to `searxng_url_configured` / `searxng_key_configured` presence booleans, with a test asserting neither the hostname nor the key value appears.
- `ToolSecrets` has no `Serialize` derive — the key can't leak into manifests, checkpoints, or API responses.
- The doctor probe (`diagnostics.rs:849-870`, `match_web_search_probe:872-909`) emits only status codes and error text, never the key or response body.
- `SEARXNG_API_KEY` added to `OPTIONAL_VAULT_SECRETS` enrolls it in the env→vault startup migration (serve.rs:683 → migration iterates `optional_vault_secrets()`), so a key left in `server.env` gets migrated into SQLite with a backup — same protective treatment as Brave/Venice. `SEARXNG_URL` is correctly left unclassified.
- In sandbox shells, `SEARXNG_API_KEY` matches the existing local-sandbox strip suffixes (`_API_KEY`), and it isn't in worker env anyway. Defense in depth holds.
- No credentials in the diff: `.env.example` values are empty; tests/docs use only `searxng-key` / `http://searxng.internal:8080` placeholders. Pattern scan for token-like strings found nothing.

**Deserialization**: `serde_json::Value` only; string fields extracted via `as_str` with fallbacks. No unsafe deserialization, no dynamic type interpretation.

**Other**: no new dependencies (no Cargo files in the diff), no new routes or auth surfaces, no path handling. The demo diagnostics change is a canned JSON string.

## Non-security notes (flagging because they have operator impact, not because they're vulnerabilities)

1. **Doc inaccuracy with silent paid-fallback cost**: `searxng-search.mdx` and `server-configuration.mdx` say `SEARXNG_URL` is read "from the server process env (or `server.env`)". It is not — `config_env_lookup` reads only the process env, and nothing loads `server.env` into process env (`ServerSecrets` file entries are consumed only for startup auth/object-store). An operator who follows the doc and puts the URL in `server.env` gets silent fallback to Brave and starts paying per query. Worth fixing the wording before merge.
2. **Bearer token over cleartext**: the docs' primary example is `http://searxng.internal:8080` and the token is forwarded as `Authorization: Bearer` on that scheme. A one-line warning ("use HTTPS, or keep the token on a trusted network segment") would prevent a cleartext-token leak in misconfigured remote setups.
3. Untrusted search-result text flows into agent context unescaped — inherent to any `web_search` tool and unchanged relative to Brave/Venice; no action needed.