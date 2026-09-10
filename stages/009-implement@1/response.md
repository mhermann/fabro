Implemented. SearXNG is now the preferred `web_search` backend when `SEARXNG_URL` is configured — free, self-hosted, operator-managed, with `searxng > brave > venice` precedence and no fallback between backends.

## What changed, file by file

**Rust**
- `lib/foundation/fabro-static/src/env_vars.rs` — added `EnvVars::SEARXNG_URL` constant + test-array entry. `secret_registry.rs` deliberately untouched (URL config, not a secret; `fabro secret set` accepts it as-is).
- `lib/components/fabro-agent/src/config.rs` — `ToolSecrets.searxng_url`, `searxng_configured` in the redacting `Debug` impl, extended redaction test.
- `lib/components/fabro-agent/src/web_search.rs` — `SearchBackend::Searxng { base_url }` variant, `searxng(&str)` constructor (trims trailing `/`), 30s timeout, pre-HTTP URL/scheme validation, `GET {base}/search?q=…&format=json`, 403 → actionable "enable the json format" hint, `format_searxng_results` (`content`→description, `publishedDate`→date, client-side truncation since SearXNG has no count param). New tests: precedence (2), schema parity, httpmock success/trailing-slash/truncation/403/500/invalid-URL-zero-calls.
- `lib/components/fabro-agent/src/cli.rs`, `lib/components/fabro-workflow/src/pipeline/initialize.rs` — the two population sites (env, vault) each gained one line.
- `lib/apps/fabro-server/src/diagnostics.rs` — `check_web_search` reads `SEARXNG_URL` from the vault first; new `check_searxng_search` probe (same `q=test&format=json` contract); split `match_web_search_probe` into a thin wrapper over `match_web_search_probe_with_remediation` so searxng gets `"Check the SearXNG instance and SEARXNG_URL"`; updated the not-configured remediation; 3 new tests (precedence-with-params, 403, connectivity) + 2 updated remediation pins.
- `lib/apps/fabro-server/src/demo/mod.rs` — demo diagnostics string mentions `SEARXNG_URL`.

**Docs & config**
- New `docs/public/integrations/searxng.mdx` (setup with `settings.yml` JSON-format snippet, compose-network reachability note, precedence, troubleshooting); new `docs/public/changelog/2026-09-10.mdx`; `docs.json` nav entries for both (folded in the devils-stage finding); updated `tools.mdx`, `server-configuration.mdx`, `prompts.mdx`, `troubleshooting.mdx`, `.env.example`.

## Adaptations from the plan
1. **Struct-literal fallout list was mostly unnecessary** — `profiles/mod.rs`, `tools.rs`, `parity_matrix.rs`, and `api.rs` all use `..ToolSecrets::default()`, so none needed edits.
2. **Clippy forced two small deviations**: `fabro_http::Url` is a disallowed type, so I followed the Bedrock precedent (`#[expect]` with reason — the URL is never logged/rendered) and dropped the URL value from the error message (now `"SEARXNG_URL is not a valid URL"`), reverting my own embellishment back to the plan's spirit. `searxng()` takes `&str` per `needless_pass_by_value`.
3. **Probe matcher refactor**: one shared 4-arm matcher instead of a duplicated searxng-specific copy.

## Verification
- Full workspace: **8431/8431 tests pass** (touched crates: fabro-static 5, fabro-agent 653, fabro-workflow 1444, fabro-server 906). 210 skipped are credential-gated live E2E.
- `cargo build --workspace`, pinned-nightly `fmt --check`, and `clippy --workspace --all-targets -D warnings` all clean; `docs.json` valid JSON.
- The only failures encountered were environmental, both fixed: no Rust toolchain existed (installed stable + pinned nightly + nextest), and 3 SVG render tests failed because `target/debug/fabro` was never built (the render subprocess falls back to the test binary); after `cargo build -p fabro-cli` they pass.
- The optional live SearXNG container smoke wasn't possible (no Docker in this sandbox); the httpmock tests pin the exact wire contract instead.