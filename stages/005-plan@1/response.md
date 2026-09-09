# Implementation Plan: SearXNG local backend for `web_search`

Per the human's answers: **thin client** to an operator-run SearXNG instance, **local wins** over paid keys, config via **`SEARXNG_URL` env var** (non-secret) with an **optional `SEARXNG_API_KEY` vault secret** (forwarded as a bearer token), **docs-only** deployment artifacts.

Key plumbing fact verified during exploration: server runs execute in a spawned `fabro __run-worker` subprocess whose env is scrubbed by `WORKER_ENV_ALLOWLIST` (`lib/apps/fabro-server/src/spawn_env.rs`). Non-secret config is already allowed to cross that boundary (precedent: `FABRO_PUSH_CRED_REFRESH_*` tunables, documented in the allowlist's own comments). Secrets cross via the vault, which the worker reads through `FABRO_HOME`. So: URL crosses via the allowlist; token crosses via the vault.

---

## Files created or modified

### Rust core

**1. `lib/foundation/fabro-static/src/env_vars.rs`** (modify)
- Add `pub const SEARXNG_URL: &'static str = "SEARXNG_URL";` and `pub const SEARXNG_API_KEY: &'static str = "SEARXNG_API_KEY";` in the "LLM providers and tool integrations" section, next to `BRAVE_SEARCH_API_KEY`/`VENICE_API_KEY`.
- Add both to the `env_var_constants_are_non_empty_and_single_tokens` test list.

**2. `lib/foundation/fabro-static/src/secret_registry.rs`** (modify)
- Add `EnvVars::SEARXNG_API_KEY` to `OPTIONAL_VAULT_SECRETS` and to the `classifies_optional_vault_secrets` test.
- Add `EnvVars::SEARXNG_URL` to the `leaves_legacy_aliases_and_non_secret_config_unclassified` test list — pinning the decision that the URL is config, not a secret.

**3. `lib/components/fabro-agent/src/config.rs`** (modify)
- `ToolSecrets`: add `pub searxng_url: Option<String>` and `pub searxng_api_key: Option<String>`.
- `Debug` impl: add `searxng_url_configured` and `searxng_key_configured` boolean fields (never the URL/token value).
- Update the debug-redaction test (~line 353) that constructs `ToolSecrets` as a full struct literal.

**4. `lib/components/fabro-agent/src/web_search.rs`** (modify) — the heart of the change
- Add const `SEARXNG_REQUEST_TIMEOUT: Duration = Duration::from_secs(30)` (metasearch aggregation across upstream engines is slower than a direct API).
- Add variant `SearchBackend::Searxng { search_url: String, api_key: Option<String> }`.
- Add constructor `searxng(base_url: &str, api_key: Option<String>) -> Self` that trims a trailing `/` from the base and stores `format!("{base}/search")`. The `search_url` field stays directly mutable so httpmock tests can point it at a `MockServer`, exactly like the Brave/Venice tests.
- Change `from_secrets` selection to: `searxng_url` present (after trim) → `Searxng`; else brave key → `Brave`; else venice key → `Venice`; else `None`. Doc comment on the enum updated to state local-wins precedence.
- Add `search_searxng(search_url, api_key, query, max_results)`: `GET {search_url}?q={query}&format=json&pageno=1`, `Accept: application/json`, `.timeout(SEARXNG_REQUEST_TIMEOUT)`, `Authorization: Bearer <key>` header only when a key is present. Non-2xx → `Err("SearXNG search returned status {status}")` with a hint appended for 401/403 ("check SEARXNG_API_KEY"). Parse `results[]` into `SearchHit { title, url, description: content, date: publishedDate }` via the existing helpers, slice to `max_results.min(MAX_RESULTS)` client-side (SearXNG has no count parameter), delegate to the shared `format_hits`.
- Extend the `secrets()` test helper to take searxng fields; add tests: searxng wins over brave+venice; searxng registers alone with URL only; brave/venice selection unchanged when URL absent; omitted when nothing configured; `format_searxng_results` output; httpmock test asserting exact request shape (`format=json`, `q`, `pageno`, bearer header when key set, absent when not); status-error mapping; all three backends share the identical tool schema (extend the existing two-backend schema test).

**5. `lib/components/fabro-agent/src/tools.rs`** (modify)
- Update the doc comments on `register_core_tools`/`register_web_search_tool` (lines ~59–61, ~87): "included when a SearXNG URL, Brave key, or Venice key is configured; SearXNG wins." Registration logic itself is unchanged — it already delegates to `from_secrets`.

**6. `lib/components/fabro-agent/src/cli.rs`** (modify)
- `cli_tool_secrets`: add `searxng_url: std::env::var(EnvVars::SEARXNG_URL).ok()` and `searxng_api_key: std::env::var(EnvVars::SEARXNG_API_KEY).ok()`. (The existing `#[expect(clippy::disallowed_methods)]` on that fn already covers `std::env::var`.)

**7. `lib/components/fabro-workflow/src/pipeline/initialize.rs`** (modify)
- `tool_secrets_from_configured_sources` (line 314): add `searxng_api_key: vault.get(EnvVars::SEARXNG_API_KEY).map(str::to_string)` (vault, like brave/venice) and `searxng_url: std::env::var(EnvVars::SEARXNG_URL).ok()` — the worker env is allowlisted in file 8; in-process `fabro run` has the operator's shell env. Add the same `#[expect(clippy::disallowed_methods, reason = ...)]` attribute the CLI uses if clippy's `disallowed_methods` flags `std::env::var` here.
- Filter empty/whitespace-only URL values to `None` so a stray `SEARXNG_URL=` doesn't register a broken backend.

**8. `lib/apps/fabro-server/src/spawn_env.rs`** (modify)
- Add `EnvVars::SEARXNG_URL` to `WORKER_ENV_ALLOWLIST` with a comment: non-secret search-endpoint config must survive `env_clear()` to reach `tool_secrets_from_configured_sources`; deliberately **not** `SEARXNG_API_KEY`, which reaches the worker through the vault.
- Extend `worker_allowlist_is_fail_closed`: assert `SEARXNG_URL` crosses and `SEARXNG_API_KEY` does not.

**9. `lib/apps/fabro-server/src/diagnostics.rs`** (modify)
- `check_web_search` (line 760): new first branch — `state.config_env_lookup(EnvVars::SEARXNG_URL)` (mirroring the Daytona URL resolution at server.rs:1489); if present, fetch the optional token via `diagnostic_secret(state, WEB_SEARCH_CHECK_NAME, EnvVars::SEARXNG_API_KEY)` and call new `check_searxng_search(url, api_key)`, which probes `GET {url}/search?q=test&format=json&pageno=1` (with bearer header if token) under `EXTERNAL_SERVICE_PROBE_TIMEOUT` and reuses `match_web_search_probe(probe, "searxng", "SEARXNG_URL")`.
- Update the not-configured remediation string to include setting `SEARXNG_URL`.
- Update exact-string tests (~lines 1251–1310) and add: URL present → searxng branch probed (unreachable URL yields `"searxng: connectivity error"`); URL present + both vault keys → searxng still wins; URL absent → existing brave/venice behavior unchanged (those tests keep passing with only remediation-text edits).

**10. `lib/apps/fabro-server/src/demo/mod.rs`** (modify)
- Update the canned Web Search remediation (~line 675) to mention `SEARXNG_URL` alongside the two keys. Update any demo test that pins that string.

**11. Compiler-flagged struct-literal fixups** (modify as discovered)
- Known candidate: `lib/components/fabro-workflow/src/handler/llm/api.rs` (~line 3935) constructs `ToolSecrets` for a test. Sites using `..ToolSecrets::default()` or field assignment (`profiles/mod.rs:748`, `gpt56.rs:370`, `tools.rs:1754`) compile unchanged. `cargo build`/`cargo nextest` will enumerate every full-literal site; each gets the two new fields via `..Default::default()`.

### Docs

**12. `docs/public/integrations/searxng-search.mdx`** (create)
- Follow the `venice-search.mdx` skeleton: intro (free, self-hosted, local-wins precedence), Setup (compose snippet for an operator-run `searxng` service with a `settings.yml` enabling `search.formats: [html, json]`; `SEARXNG_URL` env on the Fabro server; optional `fabro secret set SEARXNG_API_KEY` documented as "forwarded as a Bearer header — for instances behind an authenticating proxy"; `fabro doctor` verification showing `searxng: configured and reachable`), How it works (GET `/search?format=json`, result mapping, 30s timeout, client-side `max_results` slicing, instance-side `search.max_results` governing the pool), Permissions (same `shell` category as today), Troubleshooting (403 → JSON format not enabled in settings.yml; timeout → upstream engines slow/unreachable; URL not reaching server runs → must be in the server process env, see deploy docs).

**13. `docs/public/docs.json`** (modify) — add `"integrations/searxng-search"` after `brave-search`/`venice-search` (line ~106).

**14. `docs/public/agents/tools.mdx`** (modify) — web_search rows/section (~lines 23, 126–133): selection is now SearXNG URL → Brave key → Venice key; local wins.

**15. `docs/public/agents/prompts.mdx`** (modify) — line ~148: guidance gating now includes "or a SearXNG URL is configured".

**16. `docs/public/administration/server-configuration.mdx`** (modify) — env table (~lines 413–423): add `SEARXNG_URL` (server process env, not vault) and `SEARXNG_API_KEY` (vault); update the backend-selection paragraph.

**17. `docs/public/administration/troubleshooting.mdx`** (modify) — line ~20: "web search credentials (Brave, Venice, or a SearXNG URL)".

**18. `docs/public/administration/security.mdx`** (modify) — line ~41: add `SEARXNG_API_KEY` to the vault list; note `SEARXNG_URL` is non-secret config.

**19. `docs/internal/server-secrets-strategy.md`** (modify) — add `SEARXNG_API_KEY` to the optional-integration-secrets list (it names `BRAVE_SEARCH_API_KEY`/`VENICE_API_KEY` explicitly at line ~41).

**20. `.env.example`** (modify) — add commented `SEARXNG_URL=` (with pointer to the integration doc) and `SEARXNG_API_KEY=`.

**21. `docs/public/changelog/2026-09-09.mdx`** (create or append if one exists for today) — SearXNG backend section mirroring the Venice changelog entry: precedence, config, docs link.

---

## Order

1. **Foundation**: files 1–2 (`fabro-static`), build + test that crate.
2. **Agent core**: files 3–5 (`fabro-agent`), `cargo nextest run -p fabro-agent` — this is where the httpmock backend tests land.
3. **Plumbing**: files 6–8 (`cli.rs`, `initialize.rs`, `spawn_env.rs`), building `fabro-workflow` and `fabro-server`.
4. **Server surface**: files 9–10 (`diagnostics.rs`, `demo/mod.rs`) + step 11 compiler fixups; `cargo nextest run -p fabro-server`.
5. **Docs**: files 12–21.
6. **Lint/format/verify**: see below.

## Verification

- `cargo nextest run -p fabro-static -p fabro-agent -p fabro-workflow -p fabro-server` — covers: selection precedence unit tests, httpmock request/response-shape tests for the new backend, `format_searxng_results`, spawn-env allowlist assertions, diagnostics branch tests, env-var/secret-registry classification.
- `cargo nextest run --workspace` — full suite; catches struct-literal fixups (step 11) and any snapshot drift. Then `cargo insta pending-snapshots` — expect **none**: profile prompt snapshots depend only on whether `web_search` is registered, and those tests set keys explicitly. If any appear, inspect before accepting.
- `cargo +nightly-2026-04-14 fmt --check --all` and `cargo +nightly-2026-04-14 clippy --workspace --all-targets -- -D warnings`.
- `cargo build --workspace` (proves no `test-support` leakage, per CLAUDE.md).
- Existing live/e2e tests stay untouched (`--profile e2e` requires credentials not present here).
- **Manual check I cannot do here**: a real end-to-end query against a live SearXNG instance — no Docker daemon/instance is guaranteed in this environment. The httpmock tests pin the exact wire contract (`format=json`, `q`, `pageno=1`, bearer header, response parsing) as the substitute; the docs page carries the operator-facing verification via `fabro doctor`.

## Deliberately not doing

- **No container lifecycle management** — human chose thin client; Fabro never spawns or health-monitors SearXNG.
- **No compose-file changes** — human chose docs-only; the snippet lives in the integration page.
- **No fallback between providers on failure** — keeps the existing explicit rule ("Failed calls do not fall back"); local-wins is a selection-time decision only.
- **No reintroduction of a `[server.integrations.search]` config knob** — deliberately removed in commit `88ed2ac9`; presence-driven selection is preserved (URL presence plays the credential-presence role).
- **No cross-engine dedup/ranking, no `pageno` pagination loop** — SearXNG already merges engines; client-side slicing to `max_results` keeps the tool contract identical.
- **No `fabro install` prompt flow for SearXNG** — install UX for optional integrations is a separate scope; doctor covers verification.
- **Not touching `parity_matrix.rs` live tests** — still Brave-key gated, unchanged behavior.
- **Not fixing the pre-existing Daytona-URL/worker-env gap** I noticed while tracing — unrelated bug, out of scope.
- **Not renaming `ToolSecrets`** to something like `ToolBackendConfig` — the URL field is a documented exception; a rename would churn every profile/test site for zero behavior gain.