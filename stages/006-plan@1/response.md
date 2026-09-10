All sites verified. Here is the implementation plan.

---

# Implementation plan: SearXNG backend for `web_search`

**Authoritative answers this plan implements:** client-only (no bundled runtime); SearXNG metasearch, cost motivation; paid-first precedence (Brave key > Venice key > SearXNG URL), no failover. Config surface is the vault (`SEARXNG_URL`, optional `SEARXNG_API_KEY`) per the credential-presence model restored by `88ed2ac9`.

## Files

### Created

| Path | Contents |
|---|---|
| `docs/public/integrations/searxng.mdx` | New integration page: run your own SearXNG with `format=json` enabled (docker run snippet + `settings.yml` `search.formats` note), `fabro secret set SEARXNG_URL http://localhost:8888`, optional `SEARXNG_API_KEY` for instances behind auth, `fabro doctor` verification, precedence note (used only when neither `BRAVE_SEARCH_API_KEY` nor `VENICE_API_KEY` is present), privacy note (the instance forwards queries to upstream engines — this is a cost win, not an offline index), permissions (unchanged, `shell` category), troubleshooting (403 = JSON disabled, connectivity, 30s timeout). Mirrors `brave-search.mdx` structure. |
| `docs/public/changelog/2026-09-10.mdx` | Dated entry: "SearXNG web search backend" — what it is, how to enable, precedence, JSON-format requirement. |

### Modified — Rust (in implementation order)

**1. `lib/foundation/fabro-static/src/env_vars.rs`**
Add `pub const SEARXNG_URL` and `pub const SEARXNG_API_KEY` in the "LLM providers and tool integrations" group (beside `BRAVE_SEARCH_API_KEY`/`VENICE_API_KEY`, line ~48). Add both to the sorted all-names list after `POOLSIDE_API_KEY` (line ~219).

**2. `lib/foundation/fabro-static/src/secret_registry.rs`**
Add both names to `OPTIONAL_VAULT_SECRETS` and to the `classifies_optional_vault_secrets` test list. No scope changes — they are optional vault values, exactly like the two existing search keys.

**3. `lib/components/fabro-agent/src/config.rs`**
`ToolSecrets` gains `searxng_url: Option<String>` and `searxng_api_key: Option<String>` (lines 103–107). `Debug` impl gains `searxng_configured` / `searxng_key_configured` boolean fields (never the values — matches the existing redaction pattern and its test at line ~354).

**4. `lib/components/fabro-agent/src/web_search.rs`** — the core change.
- Constants: `SEARXNG_SEARCH_PATH: &str = "/search"`, `SEARXNG_REQUEST_TIMEOUT: Duration = Duration::from_secs(30)` (local instance should beat Venice's 1-minute remote budget).
- `SearchBackend::Searxng { base_url: String, api_key: Option<String> }`.
- `from_secrets` becomes a three-way match on `(brave, venice, searxng_url)`: Brave if brave key, Venice if venice key, `Searxng { base_url, api_key }` if URL present. Empty-string URL counts as absent (`.filter(|s| !s.is_empty())` — env reads can produce `""`).
- Constructor `searxng(base_url, api_key)` trims a trailing `/` so `{base}/search` joins cleanly.
- `search_searxng(base_url, api_key, query, max_results)`: `GET {base_url}/search` with query params `[("q", query), ("format", "json")]`, `.timeout(SEARXNG_REQUEST_TIMEOUT)`, `.bearer_auth(api_key)` only when the key is `Some`. Non-2xx → `"SearXNG instance returned status {status}"`, with a 403 case appending "(JSON output may be disabled — add \"json\" to search.formats in the instance's settings.yml)". Parse errors follow the existing `"Failed to parse response: {e}"` shape.
- `format_searxng_results(body, limit)`: maps `results[].{title, url, content, publishedDate}` into `SearchHit { description: content, date: publishedDate }`, truncates to `limit` (SearXNG has no count param — client-side truncation, `max_results.clamp(1, MAX_RESULTS)`), renders via the shared `format_hits` so agent-visible output is identical to Brave/Venice.
- Live e2e: `#[fabro_macros::e2e_test(live("SEARXNG_URL"))]` mirroring `web_search_returns_results` (tools.rs:2169), reading `SEARXNG_URL` (+ optional `SEARXNG_API_KEY`) from env.
- Unit tests: precedence (`searxng`-only registers; brave+searxng → Brave; venice+searxng → Venice; all absent → `None`); httpmock wire test asserting `GET /search`, `q` and `format=json` params, optional bearer header, and formatted output including `publishedDate`; 403 error-message test; truncation test; trailing-slash normalization test. All follow the existing construct-backend-mutate-URL httpmock pattern.

**5. `lib/components/fabro-agent/src/tools.rs`**
Comment-only updates: the `register_core_tools` doc (line ~60) and `register_web_search_tool` doc (line ~87) change "a Brave or Venice Search API key" to name SearXNG/`SEARXNG_URL`. The `register_web_search_tool` body is unchanged — `from_secrets` already returns `Option<Backend>`.

**6. `lib/components/fabro-agent/src/cli.rs`**
`cli_tool_secrets` (line 44) gains `searxng_url: std::env::var(EnvVars::SEARXNG_URL).ok()` and the matching API-key read. The existing scoped `#[expect(clippy::disallowed_methods, reason = "...search process-env credentials...")]` already covers these reads; extend the reason wording only if clippy disagrees.

**7. `lib/components/fabro-workflow/src/pipeline/initialize.rs`**
`tool_secrets_from_configured_sources` (line 314) gains `searxng_url: vault.get(EnvVars::SEARXNG_URL).map(str::to_string)` and the API-key equivalent — run workers read both from the server vault, same channel as the existing keys.

**8. `lib/apps/fabro-server/src/diagnostics.rs`**
`check_web_search` (line 760) gains a third branch after Venice: read `SEARXNG_URL` via `diagnostic_secret`, then `check_searxng_search(url, api_key)` probing `GET {url}/search?q=test&format=json` with optional bearer under `EXTERNAL_SERVICE_PROBE_TIMEOUT`. Success → `"searxng: configured and reachable"`; a 403 response gets the pointed "JSON format likely disabled" remediation (small searxng-specific matcher wrapping the existing `match_web_search_probe` shape); other failures map to `searxng: HTTP {status}` / `connectivity error` with `Check SEARXNG_URL and network connectivity`. The not-configured remediation (line 790) becomes "...or `fabro secret set SEARXNG_URL <url>` to enable web search". Tests: searxng selected when no vault keys exist (vault fixture with `SEARXNG_URL = "invalid\n"` → `"searxng: connectivity error"`, mirroring the Venice test at line 1309); the two exact-string not-configured tests (lines 1266, 1287) updated.

**9. `lib/apps/fabro-server/src/demo/mod.rs`**
Demo diagnostics fixture (line 675): remediation string updated to mention `SEARXNG_URL`.

### Modified — docs and env (after code is green)

**10. `docs/public/agents/tools.mdx`** — table row line 23 ("via Brave or Venice" → "via Brave, Venice, or SearXNG"); `web_search` section lines 124–137 rewritten: three-backend selection paragraph, paid-first precedence stated explicitly, SearXNG specifics (vault `SEARXNG_URL`, JSON-format requirement, 30s timeout, client-side truncation), keeping the documented no-failover sentence intact.

**11. `docs/public/integrations/brave-search.mdx`** — line 8's backend-selection sentence mentions the SearXNG fallback when no key exists.

**12. `docs/public/integrations/venice-search.mdx`** — same parallel sentence updated (page follows the brave-search template).

**13. `docs/public/administration/troubleshooting.mdx`** — line 20 "(Brave or Venice)" → "(Brave, Venice, or SearXNG)".

**14. `docs/public/docs.json`** — add `"integrations/searxng"` after `"integrations/venice-search"` (line ~107); add `"changelog/2026-09-10"` at the top of the changelog list (line ~312).

**15. `.env.example`** — add `SEARXNG_URL=` and `SEARXNG_API_KEY=` beside `BRAVE_SEARCH_API_KEY` (feeds the live e2e test and aids discoverability).

## Order

1. Foundation constants/registry (files 1–2) — everything downstream references these.
2. `config.rs` → `web_search.rs` → `tools.rs` → `cli.rs` (files 3–6) — the backend itself, tests written alongside.
3. `pipeline/initialize.rs` (file 7) — worker plumbing.
4. `diagnostics.rs` + `demo/mod.rs` (files 8–9) — doctor surface.
5. Docs, changelog, `.env.example` (files 10–15).
6. Full verification pass.

Rationale: each layer only depends on the ones above it, so the workspace compiles after every step and `cargo nextest` stays meaningful throughout.

## Verification

Repository tooling covers this completely (httpmock wire tests, vault-fixture diagnostics tests, live-e2e gating — no new test kind needed):

- `cargo nextest run -p fabro-static` — registry classification tests.
- `cargo nextest run -p fabro-agent web_search` — precedence, wire, formatter, error tests.
- `cargo nextest run -p fabro-agent` — full crate (profiles/tools tests construct `ToolSecrets` via `..Default::default()`, so they must still compile and pass unchanged — that's the regression guard for existing behavior).
- `cargo nextest run -p fabro-server diagnostics` — doctor selection and remediation strings.
- `cargo nextest run -p fabro-workflow initialize` — worker secret plumbing.
- `cargo build --workspace` — no `test-support` leakage, cross-crate compile.
- `cargo +nightly-2026-04-14 fmt --all` and `cargo +nightly-2026-04-14 clippy --workspace --all-targets -- -D warnings`.
- `cargo insta pending-snapshots` — expected empty; grep confirmed no snapshot embeds the remediation strings. If one appears, inspect before accepting.
- Optional live smoke (only if an instance is reachable): `docker run --rm -d -p 8888:8080 searxng/searxng` with JSON enabled, then `set -a && source .env && set +a && cargo nextest run -p fabro-agent --profile e2e --run-ignored only web_search`. Skipped otherwise — the httpmock tests carry the contract.

Manual check if desired: `fabro secret set SEARXNG_URL http://localhost:8888 && fabro doctor` shows `searxng: configured and reachable`, and with a Brave key also vaulted the summary stays `brave: ...` (paid-first proof).

## Deliberately not doing

- **No compose sidecar or Fabro-managed container** — human chose client-only.
- **No failover and no precedence change for existing installs** — human chose paid-first; when a Brave or Venice key exists, behavior is byte-identical to today. The documented "no retry through the other backend" contract stands.
- **No `[server.integrations.search]` settings table, no OpenAPI/API client changes** — would reverse the maintainer's deliberate `88ed2ac9` direction; the vault preserves the credential-presence selection model with zero API surface.
- **No new Rust dependencies** — SearXNG is a GET+JSON call; `fabro-http` suffices.
- **No tool schema, permission, or event changes** — the backend is invisible to agents (shared schema via `format_hits`), and tool events already exist.
- **No SearXNG engine/language/pagination parameters, no result reranking** — default engine set, v1 minimal.
- **No changes to `parity_matrix.rs` or agent profiles** — backend-agnostic (the `claude5_tools.rs` `SearchBackend` uses are `#[cfg(test)]`-gated at line 457).
- **No migrations** — vault entries are operator-created; rollback is deleting the entry.