Both files are written, and the repository is untouched (clean `git status`).

**`/tmp/pr-title.txt`** (46 chars):
> Add SearXNG as a free local web_search backend

**`/tmp/pr-body.md`** covers:

- **What & why** — `web_search` now prefers a self-hosted SearXNG instance (`SEARXNG_URL` → Brave → Venice) so agents get free local search instead of per-query-billed APIs. Details the new `SearchBackend::SearXng` variant (JSON format, clamped + client-side-enforced `limit`, 403 → actionable `search.formats` hint), the vault/env secret plumbing, the `fabro doctor` probe, the optional compose `search` profile with JSON pre-enabled, and the doc/changelog updates.
- **Reviewer focus** — the four parallel review passes produced findings that were recorded but *not* fixed, and the body says so explicitly rather than claiming they were resolved: (1) empty `SEARXNG_URL` shadows working Brave/Venice keys with a cryptic `relative URL without a base` failure, (2) `SEARXNG_URL` is the first vault value used as an outbound request destination (SSRF surface, extension of the existing MCP trust pattern), (3) committed known-default `secret_key` with limiter off in the dev compose settings, (4) two test gaps (`register_core_tools`-level registration test, non-403 status branch).
- **Verification** — I re-ran the focused suites myself: `cargo test -p fabro-agent web_search` (31 passed), `-p fabro-server check_web_search` (7 passed), `-p fabro-static`, plus the redaction test; review agents traced all `ToolSecrets`/`SearchBackend` consumers with no missed match sites.