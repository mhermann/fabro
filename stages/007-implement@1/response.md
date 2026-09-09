Implementation complete. Here's the summary.

## What was built

A third `web_search` backend backed by an operator-run **SearXNG** instance — zero per-query cost, thin-client (Fabro only calls its JSON API; the operator runs the container). Selection order is now **SearXNG URL → Brave key → Venice key**, with no fallback after selection, matching the existing rule.

## Changes, file by file

**Rust core**
- `lib/foundation/fabro-static/src/env_vars.rs` — added `SEARXNG_URL` and `SEARXNG_API_KEY` consts + test list.
- `lib/foundation/fabro-static/src/secret_registry.rs` — `SEARXNG_API_KEY` is an optional vault secret; `SEARXNG_URL` pinned as unclassified (config, not secret).
- `lib/components/fabro-agent/src/config.rs` — `ToolSecrets` gains `searxng_url`/`searxng_api_key`; `Debug` prints only presence booleans; redaction test extended.
- `lib/components/fabro-agent/src/web_search.rs` — new `SearchBackend::Searxng` variant, `searxng()` constructor (trims trailing `/`, appends `/search`), `search_searxng()` (`GET /search?q=…&format=json&pageno=1`, 30s timeout, optional bearer header, 401/403 hint, client-side `max_results` slicing), `format_searxng_results()`; `from_secrets` filters blank URLs at this single choke point; 11 new tests (precedence, blank-URL, request shape via httpmock incl. `header_missing` for keyless, slicing, status errors, shared tool schema).
- `lib/components/fabro-agent/src/tools.rs` — registration doc comments updated (registration logic already delegates to `from_secrets`).
- `lib/components/fabro-agent/src/cli.rs` — `cli_tool_secrets` reads both new env vars.
- `lib/components/fabro-workflow/src/pipeline/initialize.rs` — URL from process env (with required `#[expect(clippy::disallowed_methods)]`), API key from vault.
- `lib/apps/fabro-server/src/spawn_env.rs` — `SEARXNG_URL` added to the worker allowlist with comment; `SEARXNG_API_KEY` deliberately excluded; fail-closed test extended both ways.
- `lib/apps/fabro-server/src/diagnostics.rs` — `check_web_search` probes SearXNG first (mirrors Daytona URL resolution), new `check_searxng_search`, updated not-configured remediation; per the devil's finding, the two existing brave/venice precedence tests now pin `env_lookup(|_| None)` so an ambient `SEARXNG_URL` can't flip them; 3 new doctor tests.
- `lib/apps/fabro-server/src/demo/mod.rs` — demo remediation string updated.

**Docs**
- New `docs/public/integrations/searxng-search.mdx` (compose snippet, `settings.yml` JSON-format requirement, honest "locally operated, not hermetic" caveat, troubleshooting) + nav entry in `docs.json` + `changelog/2026-09-09.mdx` (+ nav).
- `agents/tools.mdx`, `agents/prompts.mdx`, `administration/server-configuration.mdx`, `troubleshooting.mdx`, `security.mdx`, `deploy-railway.mdx`, `self-host-docker.mdx`, `brave-search.mdx`, `venice-search.mdx` — the latter four per devil's finding #2 (they previously claimed Brave-key-beats-all, now false).
- `docs/internal/server-secrets-strategy.md` + `.env.example`.

## Where reality differed from the plan
1. **No Rust toolchain in this environment** — installed stable + the pinned `nightly-2026-04-14`, cargo-nextest, and cargo-insta.
2. **Memory-constrained container (8GB)** — large test binaries OOM the linker; ran test crates individually with `-j 1`/`debug=0`. Everything that can link here passes: **fabro-static 5, fabro-agent 660, fabro-workflow 1444, fabro-server 906, plus 4184 across all other small crates — all green**; `cargo build --workspace`, `fmt --check`, `clippy -D warnings` (pinned nightly), and `cargo insta pending-snapshots` (none) all clean. Three graph-render tests initially failed at `-j 2` — that turned out to be a parallelism artifact of this environment, not a real failure: they pass at `-j 1`.
3. **fabro-cli's test binaries cannot be linked in this container** under any linker configuration (even on pristine code paths). The crate compiles cleanly and clippy-clean; grep confirms it has no coupling to `ToolSecrets` or backend selection beyond displaying the `web_search` tool name.
4. A `rustc` unused-import warning in `server/tests.rs` is pre-existing (file identical at HEAD; newer local rustc emits it) — left untouched.

The one manual check the plan flagged as impossible here still stands: a live query against a real SearXNG instance. The httpmock tests pin the exact wire contract, and the docs page walks operators through `fabro doctor` verification.