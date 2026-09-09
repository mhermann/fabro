Review complete. Here is my correctness review of the SearXNG search backend change.

## Verdict

**The Rust code is essentially correct — I found one minor edge-case divergence and no logic defects in the main paths.**

## What I verified (with evidence)

**Selection precedence (SearXNG → Brave → Venice), both paths agree:**
- `web_search.rs:48-56` — `from_secrets` checks a trimmed, non-empty `searxng_url` first, then falls through to the pre-existing Brave/Venice match. Blank-URL fallthrough is at this single choke point, so the standalone-CLI path (`cli_tool_secrets`, which reads env unfiltered) also can't register a broken backend on `SEARXNG_URL=" "`. Correct.
- `diagnostics.rs:761-773` — doctor mirrors the same order; trailing slash stripped before building the probe URL, matching `SearchBackend::searxng` (`web_search.rs:87`). The devils-review test-hermeticity fix is real: `.env_lookup(|_| None)` pins the two pre-existing precedence tests, and new tests cover searxng-wins, blank-URL fallthrough, and a refused-connection probe.

**Plumbing is sound:**
- `spawn_env.rs` — `SEARXNG_URL` added to `WORKER_ENV_ALLOWLIST` (crosses worker `env_clear()`); `SEARXNG_API_KEY` deliberately excluded with a negative assertion (`worker_allowlist_is_fail_closed` passes). Vault-only token flow in `initialize.rs:319-327` matches; `tool_secrets` is built once (line 366) and cloned for the API backend (247/265) — no second construction path.
- `#[expect(clippy::disallowed_methods)]` on `tool_secrets_from_configured_sources` is required, not decorative — the function does call `std::env::var`, which is in the repo's disallowed list, and an unfulfilled `#[expect]` is a hard error, so this is self-checking.
- `ToolSecrets` derives `Default`; every other literal site uses `..ToolSecrets::default()`. Grep confirms no consumer of these fields outside the five touched files, and no non-exhaustive `match` on `SearchBackend` exists anywhere (all call sites are `if let Some(backend)`), so the new variant can't be silently dropped.
- Wire format: GET `{base}/search` with `format=json`, `pageno=1`, optional bearer, 30 s timeout, client-side slicing clamped to `MAX_RESULTS` (double clamp with `max_results_arg` is harmless), missing/empty `results` → "No results found.", `content`→description / `publishedDate`→date mapping mirrors the Venice formatter. httpmock tests pin the request shape.

**Independent test runs:** 9/9 searxng unit tests (fabro-agent), 7/7 `check_web_search` diagnostics tests + worker allowlist test (fabro-server), 21/21 initialize tests (fabro-workflow) — all pass. (First fabro-workflow attempt hit a transient compile failure that vanished on immediate re-run — concurrent-build race in this shared checkout, not a code defect.)

## Finding 1 (minor): config-validity boundary diverges between the agent and doctor paths

- **`lib/components/fabro-agent/src/web_search.rs:49-53` + `:87`** — emptiness is filtered *before* trailing-slash stripping. For `SEARXNG_URL=/`: trim gives `"/"`, which is non-empty, so the backend registers; then `"/".trim_end_matches('/')` is `""`, producing `search_url = "/search"`. Every `web_search` call then fails at request build with reqwest's "relative URL without a base".
- **`lib/apps/fabro-server/src/diagnostics.rs:762-765`** — slash stripping happens *before* the emptiness filter. The same `SEARXNG_URL=/` value is treated as unconfigured, so doctor reports "optional, not configured" with remediation "Set SEARXNG_URL…" — while runs are actually attempting (and failing) calls against the broken backend.

**Trigger:** `SEARXNG_URL=/` (or `"///"`). Severity is low — it's a degenerate config value — but the two paths are supposed to mirror each other and they disagree on the boundary. Fixing it means applying the same trim → slash-strip → filter order in `from_secrets` (ideally one shared helper), not reordering `check_web_search`.

## Non-defect observations (no action needed)

- Doctor's searxng branch reads `SEARXNG_API_KEY` from the vault *before* probing, so a broken vault turns a keyless-instance check into "secret store unavailable" instead of a probe. This exactly mirrors the pre-existing Brave/Venice branches and a broken vault errors other checks anyway — consistent, not wrong.
- `SearchBackend`'s derived `Debug` includes `api_key` (now also for the Searxng variant). This follows the pre-existing Brave/Venice pattern, and I found no production code that debug-logs a `SearchBackend`.