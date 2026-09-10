Done. The implementation is complete and fully verified.

## What was built

A third `web_search` backend — **SearXNG**, a self-hosted metasearch aggregator — used only when neither paid key (`BRAVE_SEARCH_API_KEY`, `VENICE_API_KEY`) exists. Paid-first precedence, no failover, zero behavior change for existing installs. Config surface stays vault-only per the `88ed2ac9` credential-presence model: `SEARXNG_URL` (required) + optional `SEARXNG_API_KEY`.

## Changes, file by file

**Rust**
- `lib/foundation/fabro-static/src/env_vars.rs` — added `SEARXNG_URL` / `SEARXNG_API_KEY` constants + test array entries.
- `lib/foundation/fabro-static/src/secret_registry.rs` — both names in `OPTIONAL_VAULT_SECRETS` + the classification test (so the startup env→vault migration treats them exactly like Brave/Venice).
- `lib/components/fabro-agent/src/config.rs` — `ToolSecrets` gains `searxng_url` / `searxng_api_key`; Debug impl reports presence booleans only; redaction test extended to assert the URL is also redacted.
- `lib/components/fabro-agent/src/web_search.rs` — `SearchBackend::Searxng { base_url, api_key }`; three-way `from_secrets` with empty-URL-treated-as-absent; trailing-slash-trimming constructor; `search_searxng` (GET `/search?q=…&format=json`, 30s timeout, conditional bearer, 403 → "JSON output may be disabled — add \"json\" to search.formats…" hint); `format_searxng_results` with client-side truncation → shared `format_hits`. Tests: 6 precedence/normalization, 2 httpmock wire tests (bearer + keyless), 403 hint, truncation, schema-parity, plus a live e2e gated on `SEARXNG_URL`.
- `lib/components/fabro-agent/src/tools.rs`, `cli.rs` — doc-comment updates; CLI reads both new vars from process env under the existing scoped `disallowed_methods` expectation.
- `lib/components/fabro-workflow/src/pipeline/initialize.rs` — run workers read both new values from the vault.
- `lib/apps/fabro-server/src/diagnostics.rs` — third `check_web_search` branch; `check_searxng_search` probe with a 403-specific remediation; not-configured remediation mentions `SEARXNG_URL`; 3 new tests (env-ignored, searxng-selected, brave-over-searxng).
- `lib/apps/fabro-server/src/demo/mod.rs` — demo diagnostics remediation updated.

**Docs**
- New `docs/public/integrations/searxng.mdx` (setup with `format=json` enabled — off by default —, precedence, privacy note that upstream engines still see queries, troubleshooting) and `docs/public/changelog/2026-09-10.mdx`.
- Updated `agents/tools.mdx` (three-backend selection, paid-first), `integrations/brave-search.mdx`, `integrations/venice-search.mdx`, `administration/troubleshooting.mdx`, `administration/server-configuration.mdx` (see deviations), `docs.json` nav, `.env.example`.

## Deviations from the plan

1. **`searxng(base_url: &str, …)` instead of `String`** and **`.as_deref()`** in `from_secrets` — nightly clippy (`needless_pass_by_value`, `option_as_ref_deref`) required both; Brave/Venice constructors keep `String` because they consume it.
2. **`server-configuration.mdx` added to the doc set** (not in the plan) — it's the authoritative vault-secrets table and stated the now-stale two-key rule; skipping it would have left docs contradictory.
3. **Three extra tests** beyond the plan's list (keyless wire test, env-ignored doctor test, brave-over-searxng doctor test) — each mirrors an existing Brave/Venice counterpart.

## Notes

- **Environment**: no Rust toolchain existed; I installed CI's stable (1.97.1), `cargo-nextest`, `cargo-insta`, and pinned `nightly-2026-04-14` for fmt/clippy.
- **Tooling anomaly**: several `edit_file` operations reported success but didn't persist; I caught each via post-edit verification (git diff / grep / compile errors) and re-applied them. Final state is verified on disk.
- **Verification**: fabro-static 5/5, fabro-agent 652/652 (95 e2e skipped as designed), fabro-server 906/906, fabro-workflow 1444/1444 (31 skipped), `cargo build --workspace` clean, `fmt --check` clean, `clippy -D warnings` clean, no pending snapshots. Optional live smoke against a real SearXNG instance was skipped — the httpmock tests carry the wire contract.