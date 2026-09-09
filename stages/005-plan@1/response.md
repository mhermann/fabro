I have complete knowledge of every touchpoint. Here is the implementation plan.

---

# Implementation Plan: SearXNG local web-search backend

**Decisions locked in from the human's answers:** SearXNG engine · configured via the vault as `SEARXNG_URL` (value is a base URL like `http://searxng:8888`) · precedence **SearXNG > Brave > Venice** · ships code + doctor probe + integrations doc + optional compose service.

**Design in one sentence:** add a third `SearchBackend::SearXng { base_url }` variant that GETs `{base_url}/search?q=…&format=json&limit=…`, enabled when `SEARXNG_URL` is present in the vault, taking precedence over both paid backends.

---

## 1. Files created or modified, and what changes in each

### Stage 1 — Foundation constants

**1.1 `lib/foundation/fabro-static/src/env_vars.rs`** (modify)
- Add `pub const SEARXNG_URL: &'static str = "SEARXNG_URL";` beside `BRAVE_SEARCH_API_KEY`/`VENICE_API_KEY` (line ~50).
- Add `EnvVars::SEARXNG_URL` to the exhaustive list in the test `env_var_constants_are_non_empty_and_single_tokens` (line ~202) — this test enumerates every constant; omitting it fails compilation only if the list is used exhaustively, but convention is to keep it complete.

**1.2 `lib/foundation/fabro-static/src/secret_registry.rs`** (modify)
- Add `EnvVars::SEARXNG_URL` to `OPTIONAL_VAULT_SECRETS` (line ~21) so `fabro secret set SEARXNG_URL …` is an accepted vault-backed name.
- Add it to the test list in `classifies_optional_vault_secrets` (line ~94).

### Stage 2 — Agent backend (the core change)

**2.1 `lib/components/fabro-agent/src/config.rs`** (modify)
- `ToolSecrets`: add field `pub searxng_url: Option<String>` (struct derives `Default`, so `..ToolSecrets::default()` users — parity_matrix — keep compiling).
- `Debug for ToolSecrets`: add `.field("searxng_search_configured", &self.searxng_url.is_some())`. The URL value is **not** printed (hostnames can encode topology; keeps the redaction test pattern honest).
- Update test `tool_secrets_debug_redacts_values` (line ~351): this constructs `ToolSecrets` *without* `..Default::default()`, so it breaks unless the new field is added. Extend it to set `searxng_url: Some("http://localhost:8888")`, assert `searxng_search_configured: true` appears and `localhost:8888` does not.

**2.2 `lib/components/fabro-agent/src/web_search.rs`** (modify — the heart of the change)
- Add variant `SearchBackend::SearXng { base_url: String }`. Unlike Brave/Venice (which store the full endpoint and default to a constant), this stores a **user-configured base URL**; the search URL is derived as `{base_url trimmed of trailing '/'}/search`. Add a test-visible constructor `SearchBackend::searxng(base_url: String)`.
- Rewrite `from_secrets` as a three-way match: `(Some(searxng_url), _, _)` → SearXng; `(None, Some(brave), _)` → Brave; `(None, None, Some(venice))` → Venice; else `None`.
- New `async fn search_searxng(base_url: &str, query: &str, max_results: u64)`:
  - `GET {base}/search` with query params `q`, `format=json`, `limit = max_results.clamp(1, MAX_RESULTS)` (same clamp as Venice), header `Accept: application/json`, via the existing `search_http_client()`. No auth header — local keyless by design.
  - Non-2xx → `Err("SearXNG returned status {status}")`; **special-case 403** to append `(is the JSON format enabled? set 'search.formats' to include 'json' in your SearXNG settings.yml)` — SearXNG returns 403 for `format=json` unless the operator enables it, and this is the one error every first-time deployer will hit.
  - New `format_searxng_results(&serde_json::Value)` mirroring `format_venice_results`: top-level `results[]`, mapping `title`→title, `url`→url, `content`→description, `publishedDate`→date (optional), into the existing `SearchHit`/`format_hits` machinery.
  - `search()` match arm dispatches to it (no Venice-style query length cap; SearXNG has no such limit).
- Update the module doc comment (lines 1–4) to state the three-backend ladder and that SearXNG is preferred when configured.
- **Tests added** (following the existing httpmock + injected-endpoint pattern):
  - `from_secrets_prefers_searxng_when_all_are_configured`, `from_secrets_registers_searxng_when_only_url_is_present`.
  - `searxng_search_gets_format_json_and_parses_results` — mock asserts method GET, path `/search`, query params `q`, `format=json`, `limit`; returns `{"results": [{"title","url","content","publishedDate"}]}`; asserts formatted output including date.
  - `searxng_maps_403_to_json_format_hint` — asserts the remediation text and `mock.assert()`.
  - `searxng_handles_trailing_slash_in_base_url` — base `http://…:8888/` must hit `/search`, not `//search`.
  - Extend `brave_and_venice_use_the_same_tool_schema` to a three-way check (tool schema is backend-agnostic).

**2.3 `lib/components/fabro-agent/src/tools.rs`** (modify, comments only)
- Update doc comments on `register_core_tools` (line ~60: "when a Brave or Venice Search API key is configured" → mention SearXNG URL) and `register_web_search_tool` (line ~87). No behavior change — `from_secrets` owns selection. Existing tests (`register_core_tools_omits_web_search_without_api_key`, `…passes_configured_brave_search_key`) pass unchanged.

### Stage 3 — Credential plumbing

**3.1 `lib/components/fabro-workflow/src/pipeline/initialize.rs`** (modify)
- In `tool_secrets_from_configured_sources` (line ~313): add `searxng_url: vault.get(EnvVars::SEARXNG_URL).map(str::to_string)`.

**3.2 `lib/components/fabro-agent/src/cli.rs`** (modify)
- Add `searxng_url: std::env::var(EnvVars::SEARXNG_URL).ok()` beside the brave/venice env reads (lines 46–47) — env fallback for direct CLI runs, matching existing behavior for the paid keys.

### Stage 4 — Server diagnostics (doctor)

**4.1 `lib/apps/fabro-server/src/diagnostics.rs`** (modify)
- `check_web_search` (line 760): read `SEARXNG_URL` from the vault **first** via `diagnostic_secret`; if present → `check_searxng_search(url)`. Then Brave, then Venice (unchanged order). Update the not-configured remediation to: `"Run \`fabro secret set SEARXNG_URL\` (local SearXNG), \`fabro secret set BRAVE_SEARCH_API_KEY\`, or \`fabro secret set VENICE_API_KEY\` to enable web search"`.
- New `check_searxng_search(base_url: String)`: probe `GET {base}/search?q=test&format=json&limit=1` under `EXTERNAL_SERVICE_PROBE_TIMEOUT`, reusing the existing generic `match_web_search_probe(probe, "searxng", "SEARXNG_URL")` — no new CheckResult plumbing needed.
- **Tests:** add `check_web_search_prefers_searxng_when_url_and_paid_keys_exist` (vault entry with unreachable URL → summary `"searxng: connectivity error"`, proving precedence), `check_web_search_uses_searxng_when_only_url_present`, `check_web_search_ignores_env_backed_searxng_url`; update the two existing env-backed tests (lines 1251, 1272) to the new remediation string.

**4.2 `lib/apps/fabro-server/src/demo/mod.rs`** (modify)
- Update the demo doctor fixture string (line ~675) to the same new remediation text so demo output matches real doctor output.

### Stage 5 — Compose service + docs

**5.1 `docker/searxng/settings.yml`** (create)
- Minimal SearXNG settings: `search.formats: [html, json]` (JSON is **disabled by default** in SearXNG — without this the backend 403s) and the bot limiter disabled (private sibling service, no public exposure).

**5.2 `docker-compose.local.yaml`** (modify)
- Add a `searxng` service behind `profiles: ["search"]` (default `docker compose up` behavior unchanged): image `docker.io/searxng/searxng:latest`, port `8888`, mounting `./docker/searxng/settings.yml` at `/etc/searxng/settings.yml`. On the shared compose network the Fabro service reaches it at `http://searxng:8888` — which is exactly what the tool executes against, since `web_search` runs in the Fabro server process, not in run containers.

**5.3 `docs/public/integrations/searxng.mdx`** (create, modeled on `brave-search.mdx`/`venice-search.mdx`)
- What/why (free, local, no API key), setup: `docker run` one-liner + the compose `--profile search` path + the settings.yml JSON-format requirement; `fabro secret set SEARXNG_URL http://searxng:8888` (compose) or `http://localhost:8888` (standalone); `fabro doctor` should show `searxng: configured and reachable`; precedence paragraph; troubleshooting (403 = JSON format disabled; connectivity is from the *server process*; upstream engines still queried from your machine so internet is required); permissions section copied from the Brave page (same `shell` category).

**5.4 `docs/public/docs.json`** (modify) — add `"integrations/searxng"` to the integrations nav group (line ~106), placed before `brave-search` since it's the free default.

**5.5 `docs/public/agents/tools.mdx`** (modify) — table row (line 23) and `web_search` section (line ~126): "SearXNG, Brave, or Venice" + precedence sentence.

**5.6 `docs/public/integrations/brave-search.mdx`** (modify) — selection paragraph: note SearXNG is preferred when `SEARXNG_URL` is set; Brave is next.

**5.7 `docs/public/integrations/venice-search.mdx`** (modify) — same precedence update.

**5.8 `docs/public/administration/server-configuration.mdx`** (modify) — secret list (lines 413–423): add `SEARXNG_URL` row ("Base URL of a self-hosted SearXNG instance; the preferred `web_search` backend when present") and update the selection paragraph (line 417).

**5.9 `docs/internal/server-secrets-strategy.md`** (modify) — add `SEARXNG_URL` to the optional vault list (line ~41) with one sentence noting it's an endpoint URL, not a credential, stored in the vault to reuse the proven vault→run plumbing and keep server/CLI parity.

**5.10 `docs/public/changelog/2026-09-09.mdx`** (create) — entry: new SearXNG backend, precedence change, `SEARXNG_URL`, compose profile.

**5.11 `.env.example`** (modify) — add `SEARXNG_URL=` beside `BRAVE_SEARCH_API_KEY=` (alphabetical) for CLI-run parity.

---

## 2. Order of work

Stages 1→5 as listed: constants → backend → plumbing → diagnostics → compose/docs. Each stage compiles and tests independently; the backend (2.2) is where the unit tests concentrate. Docs last, against the final behavior.

## 3. Verification

- `cargo nextest run -p fabro-static` — registry + env-var tests.
- `cargo nextest run -p fabro-agent` — new `web_search.rs` unit tests (httpmock: request shape, parsing, 403 hint, trailing slash, precedence), `config.rs` redaction test.
- `cargo nextest run -p fabro-workflow` — plumbing compiles; existing `agent_run_web_search_uses_configured_brave_search_key` still passes (it sets only the Brave key).
- `cargo nextest run -p fabro-server` — new/updated diagnostics tests, demo fixture.
- `cargo +nightly-2026-04-14 fmt --all` then `cargo +nightly-2026-04-14 clippy --workspace --all-targets -- -D warnings`.
- `docker compose -f docker-compose.local.yaml --profile search config` — validates compose syntax + profile gating without a daemon.
- Manual end-to-end (only if Docker is available in this environment; otherwise I will state it was not run): start SearXNG with the settings.yml, `fabro secret set SEARXNG_URL http://localhost:8888`, `fabro doctor` → `searxng: configured and reachable`, then a `web_search` call from a workflow.

## 4. Deliberately not doing

- **No generic pluggable-endpoint system** — a hardcoded variant matches the existing code; a config-driven provider abstraction is a bigger design change than the goal needs (decision made in the Q&A stage).
- **No optional auth for SearXNG** — keyless local service per the chosen design; if someone fronts it with auth later, a `SEARXNG_API_KEY` companion is a small follow-up. Troubleshooting doc will note exposure guidance instead.
- **No changes to Venice *LLM provider* references** (`fabro-model/src/catalog/providers/venice.toml`, `model_resolution.rs`, `fabro-validate`) — that's the model provider, a different concern that happens to share the vendor name.
- **No parity_matrix changes** — its `web_search_provider_test!` pins Brave via `ToolSecrets { brave.., ..default() }`; the new precedence rung is absent there, so Brave is still selected and tests pass as-is.
- **No new e2e live test** — the existing live test is gated on a real Brave key; a SearXNG e2e needs a running service CI doesn't have. httpmock unit tests pin the wire contract; the doc carries manual verification steps.
- **No changes to prod compose files** (`docker-compose.yaml`, `-prod`, `-split-web`, `-tailscale`) — scope agreed as `docker-compose.local.yaml`; the doc's snippet covers other deployments.
- **No tool-schema/prompt changes** — `web_search`'s definition and system-prompt gating are already backend-agnostic (`has_web_search` flips on registry presence, which `from_secrets` drives).