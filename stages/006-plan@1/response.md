I have everything verified against the current code. Here is the implementation plan.

---

# Implementation Plan: SearXNG backend for `web_search`

## Decisions locked in (from the human + repo conventions)

- **Backend:** self-hosted SearXNG, JSON API (`GET /search?q=…&format=json`). Operator-managed service (LiteLLM pattern) — Fabro only connects, never manages containers.
- **Config:** new name `SEARXNG_URL` holding the instance base URL (e.g. `http://searxng:8080`). Vault for server runs, process env for the standalone CLI — exactly mirroring `BRAVE_SEARCH_API_KEY`/`VENICE_API_KEY`.
- **Precedence:** `searxng > brave > venice`. Cost is the motivation, so a configured local instance wins; users who don't set `SEARXNG_URL` see zero behavior change.
- **No fallback** between backends on failure (existing documented/tested invariant); **no provider enum / settings section** (commit `88ed2ac9` deliberately deleted it).

## Files to create or modify

### Rust

1. **`lib/foundation/fabro-static/src/env_vars.rs`**
   - Add `pub const SEARXNG_URL: &'static str = "SEARXNG_URL";` in the "LLM providers and tool integrations" block (next to `BRAVE_SEARCH_API_KEY`, line ~49).
   - Add `EnvVars::SEARXNG_URL` to the `values` array in `env_var_constants_are_non_empty_and_single_tokens` (~line 202).

2. **`lib/foundation/fabro-static/src/secret_registry.rs`** — *no change, deliberately.* `SEARXNG_URL` is not a secret (the registry's own test keeps URL-style config like `DAYTONA_API_URL` unclassified), it's a brand-new name with no legacy env users to migrate, and `fabro secret set` accepts arbitrary non-bootstrap names, so vault storage works without registration.

3. **`lib/components/fabro-agent/src/config.rs`**
   - Extend `ToolSecrets` (line 103) with `pub searxng_url: Option<String>`.
   - Extend the manual `Debug` impl (line 109) with `.field("searxng_configured", &self.searxng_url.is_some())`.
   - Update `tool_secrets_debug_redacts_values` (~line 351): add the field to the literal, assert `searxng_configured: true` and that the URL value does not appear.

4. **`lib/components/fabro-agent/src/web_search.rs`** — the core change:
   - Add `SEARXNG_REQUEST_TIMEOUT: Duration = Duration::from_secs(30)` (local aggregation is slower than Brave but shouldn't need Venice's 60s).
   - Add `SearchBackend::Searxng { base_url: String }` variant + `pub(crate) fn searxng(base_url: String) -> Self` (trim trailing `/` from the base).
   - `from_secrets` (line 37): new precedence — `(Some(url), _, _) => Searxng`, then brave, then venice, then `None`.
   - `search()` dispatch arm: call new `search_searxng(base_url, query, max_results)`.
   - New `search_searxng`:
     - Validate `base_url` with `fabro_http::Url::parse` and require an `http`/`https` scheme; on failure return `"SEARXNG_URL is not a valid URL: {err}"` **before** any HTTP call.
     - `GET {base_url}/search` with query `[("q", query), ("format", "json")]`, `.timeout(SEARXNG_REQUEST_TIMEOUT)`, `Accept: application/json` — same shared `search_http_client()`.
     - Non-2xx → `Err`. Special-case **403** with an actionable suffix (SearXNG returns 403 when the `json` format isn't whitelisted): `"SearXNG returned status 403 (enable the json format on the instance: settings.yml → search → formats)"`. Other statuses: `"SearXNG returned status {status}"`.
   - New `format_searxng_results`: map `body["results"][]` → `SearchHit { title, url, description: content, date: publishedDate (optional) }`, then truncate to `max_results.min(MAX_RESULTS)` (SearXNG has no count param) and reuse `format_hits` so output shape matches Brave/Venice exactly.
   - Tests: extend `secrets()` helper to take a searxng param; update the four `from_secrets_*` tests; add `from_secrets_prefers_searxng_over_paid_keys` and `from_secrets_registers_searxng_when_only_url_present`; extend `brave_and_venice_use_the_same_tool_schema` to include searxng; new httpmock tests — successful GET asserting `q`/`format=json` params and result/date rendering, trailing-slash base URL, truncation to `max_results`, 403 error text, generic status error, invalid URL rejected with zero HTTP calls.

5. **`lib/components/fabro-agent/src/cli.rs`**
   - `cli_tool_secrets()` (line 44): add `searxng_url: std::env::var(EnvVars::SEARXNG_URL).ok()`.

6. **`lib/components/fabro-workflow/src/pipeline/initialize.rs`**
   - `tool_secrets_from_configured_sources` (line 314): add `searxng_url: vault.get(EnvVars::SEARXNG_URL).map(str::to_string)`.

7. **Compile-fallout fixups (struct literals only, no behavior change):**
   - `lib/components/fabro-agent/src/profiles/mod.rs` (~line 748 test literal)
   - `lib/components/fabro-agent/src/tools.rs` (~line 1754 test literal)
   - `lib/components/fabro-agent/tests/it/parity_matrix.rs` (~line 228 literal)
   - `lib/components/fabro-workflow/src/handler/llm/api.rs` (~line 3935 test literal)
   - Existing behavior tests (e.g. `web_search_is_registered_only_when_a_key_is_configured` in `profiles/gpt56.rs`) stay valid: a brave key still registers the tool.

8. **`lib/apps/fabro-server/src/diagnostics.rs`**
   - `check_web_search` (line 760): read `EnvVars::SEARXNG_URL` from the vault **first**; if present → new `check_searxng_search(url)`: probe `GET {url}/search?q=test&format=json` (reuse `http_client_or_check` + `EXTERNAL_SERVICE_PROBE_TIMEOUT`), then a searxng-specific matcher that reuses the pass/HTTP-error/timeout shape of `match_web_search_probe` but with remediation `"Check the SearXNG instance and SEARXNG_URL"` (no secret name to check). Brave/Venice branches unchanged below it.
   - Update the not-configured `remediation` (line 790) to: `"Run \`fabro secret set SEARXNG_URL <url>\` (self-hosted SearXNG), or \`fabro secret set BRAVE_SEARCH_API_KEY\` / \`fabro secret set VENICE_API_KEY\` to enable web search"` — and update the three existing tests (lines ~1251–1316) that pin the old string.
   - New tests: searxng URL in vault → probe issued against mock server (pass on 200, warning on 403/timeout, zero brave probe when searxng wins precedence).

9. **`lib/apps/fabro-server/src/demo/mod.rs`** — update the static diagnostics string (line 675) to mention `SEARXNG_URL` alongside the two keys.

### Docs & config

10. **`docs/public/integrations/searxng.mdx`** (new; modeled on `brave-search.mdx`): what it is; setup = run SearXNG (docker snippet), **enable `json` in `settings.yml` `search.formats`**, ensure reachability *from the Fabro process* (inside docker-compose use `http://searxng:8080`, not `localhost`); `fabro secret set SEARXNG_URL http://…`; verify with `fabro doctor`; how it works (GET `/search`, precedence `searxng > brave > venice`, no fallback); permissions (unchanged, `shell` category/`full`); troubleshooting (403 = json format disabled, 429 = limiter, connection refused = wrong network URL, upstream engine rate-limits); example research workflow (same dot graph shape as brave-search.mdx).
11. **`docs/public/docs.json`** — add `"integrations/searxng"` to the integrations nav next to `brave-search`/`venice-search` (~line 106).
12. **`docs/public/agents/tools.mdx`** — line 23 table row → "Search the web via SearXNG, Brave, or Venice"; rewrite the web_search selection paragraph (~126–137): SearXNG when `SEARXNG_URL` is set, else Brave, else Venice; note SearXNG includes `date` when the instance supplies `publishedDate`; keep the vault-vs-env sentence and the Venice 400-char note as Venice-specific.
13. **`docs/public/administration/server-configuration.mdx`** — add `SEARXNG_URL` row to the secrets table (~line 420) + one sentence: self-hosted SearXNG is the preferred (free) backend when configured.
14. **`docs/public/agents/prompts.mdx`** — line 148: web_search guidance present when a SearXNG URL, Brave key, or Venice key is configured.
15. **`docs/public/administration/troubleshooting.mdx`** — line 20: "web search credentials (Brave or Venice)" → include SearXNG.
16. **`docs/public/changelog/2026-09-10.mdx`** (new; frontmatter `title`/`date` like `2026-09-04.mdx`): announce the SearXNG backend, precedence, setup snippet.
17. **`.env.example`** — add `SEARXNG_URL=` next to `BRAVE_SEARCH_API_KEY` (standalone-CLI path parity).

## Order

1. `fabro-static` (env constant + test) — foundation, no dependents break.
2. `fabro-agent/config.rs` (`ToolSecrets`) — knowingly breaks literals.
3. `fabro-agent/web_search.rs` (variant, request, formatter, errors, all unit tests) — the heart.
4. `fabro-agent/cli.rs` + `fabro-workflow/initialize.rs` (the two population sites).
5. Fix struct-literal fallout (step 7 list) until `cargo build --workspace` is green.
6. `fabro-server` diagnostics + demo string.
7. Docs, docs.json, changelog, `.env.example`.
8. Full verification sweep.

## Verification (repository tooling)

- `cargo nextest run -p fabro-static` — env var registry tests.
- `cargo nextest run -p fabro-agent web_search` then `-p fabro-agent` — new httpmock/precedence/redaction suites plus existing profile/tool tests.
- `cargo nextest run -p fabro-workflow` — initialize.rs path compiles and passes.
- `cargo nextest run -p fabro-server diagnostics` then `-p fabro-server` — updated check tests.
- `cargo build --workspace` — proves no `test-support` leakage and no literal fallout.
- `cargo +nightly-2026-04-14 fmt --all` and `cargo +nightly-2026-04-14 clippy --workspace --all-targets -- -D warnings`.
- `python3 -m json.tool docs/public/docs.json` — nav JSON still valid (no Mintlify build available here).
- Manual read-through of the new/edited mdx pages against the brave-search.mdx structure.
- Live smoke (optional, if Docker is usable in this environment): `docker run --rm -d -p 8888:8080 searxng/searxng`, set a `SEARXNG_URL` vault/env value, confirm the tool call path and the doctor probe — noting the stock image needs the `json` format enabled, which itself exercises the 403 error message.

## Deliberately not doing

- **No Fabro-managed container lifecycle** — the human chose operator-managed; nothing in `fabro-sandbox`/Docker code is touched.
- **No keyless/scraping backend (DDG et al.)** — the human chose SearXNG only.
- **No provider enum / `[server.integrations.search]` settings** — `88ed2ac9` deliberately deleted that; selection stays presence-based, so no OpenAPI/TS-client regeneration.
- **No `SEARXNG_API_KEY`** — SearXNG core has no documented API-key header; inventing one is speculative. Additive later if deployments demand it.
- **No fallback/retry across backends** — existing invariant, documented and tested; a dead instance errors the call like a dead Brave key does today.
- **No changes to Brave/Venice request code, the parity-matrix live tests (Brave-keyed, still valid), tool schema, permission category, or event stream.**
- **Not updating the "integration secrets" enumerations** in `deploy-railway.mdx`/`security.mdx`/`self-host-docker.mdx` — they enumerate *secrets*; a URL doesn't belong in those lists, and the canonical setup doc is the new `searxng.mdx` + `server-configuration.mdx` table.
- **No new Rust dependencies** — `fabro_http` + `serde_json` cover everything.