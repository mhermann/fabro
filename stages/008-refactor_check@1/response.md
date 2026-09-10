I decided this by reading the code the change will land in — `web_search.rs` in full, `ToolSecrets` in `config.rs`, both population sites (`initialize.rs`, `cli.rs`), `check_web_search` in diagnostics, and the test patterns — and asking each bar question against the actual shapes.

## What I found

**The seams the change needs already exist.** The varying dimension is "which HTTP backend serves `web_search`," and `SearchBackend` is precisely that seam: an enum with per-variant data, one `from_secrets` selection point, one `search()` dispatch, one formatter per provider feeding a shared `SearchHit`/`format_hits`. A third variant is the designed-for extension, not a workaround. Config flows as one struct populated at exactly two sites, each gaining one line — not a flag threaded through five call sites.

**Duplication: a third similar function, not triplication of logic.** `search_brave`/`search_venice`/`search_searxng` share a ~10-line send/parse skeleton but genuinely differ in request construction (GET+query vs POST+body), error semantics (Venice's 402 balance headers, SearXNG's 403 json-format hint), and timeouts. A shared helper would collapse ~5 lines per backend and blur those differences. Likewise the formatters: three ~15-line mappers over *structurally different* JSON shapes (nested `web.results` vs flat `results[].content` vs `results[].publishedDate`+truncation) into one struct — that's variety, not copy-paste. Unifying them would be abstraction for its own sake.

**No unit outgrows itself.** `web_search.rs` is ~295 lines of production code; the change adds ~60–80. It stays one coherent module.

**Tests reach everything.** The httpmock pattern (mutate the backend's URL field, assert exact request params and response rendering) and the diagnostics `TestAppStateBuilder.vault_entries` pattern cover the new behaviour with no manual verification gap.

**Names:** `SearchBackend`, the URL consts, and `check_web_search` all stay accurate. One honest drift: `ToolSecrets` will hold `searxng_url`, which is a URL, not a secret. I judge this below the bar: Fabro's own vocabulary stores this value in the vault via `fabro secret set` (the product calls vault entries secrets), a rename would churn ~8 literals and two crates for zero behavioral gain, and the Debug impl already frames fields as `*_configured` rather than values.

## On record but rejected as refactoring candidates

- Renaming `ToolSecrets` → `ToolConfig`: naming purity, not obstruction; expensive churn relative to the change.
- Extracting a shared send/parse helper for the three backend functions: collapses little, obscures per-backend error semantics.
- Splitting `web_search.rs` into per-backend files: premature until a fourth backend exists.

None of these make the implementation meaningfully harder, larger, or riskier; the change is small and self-contained against existing seams. No refactoring stage has run previously in this workflow — this is a decision on the merits, and the merits say no.

{"preferred_next_label": "none"}