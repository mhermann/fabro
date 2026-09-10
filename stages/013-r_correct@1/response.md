Review complete. I examined every hunk of the 15-file diff, read the full `web_search.rs`, `diagnostics.rs` context, all `ToolSecrets` consumers, registration sites, and the legacy secrets migration, and ran the focused test suites.

## Verdict: the change is correct. No findings.

### What I verified

**Selection logic** (`web_search.rs:45-56`): the tuple match is exhaustive and correctly ordered — `(Some,_,_) → Searxng`, `(None,Some,_) → Brave`, `(None,None,Some) → Venice`, `(None,None,None) → None`. No overlapping/unreachable arms; no exhaustive `match` on `SearchBackend` exists outside this file, so the new variant breaks nothing (registration sites at `tools.rs:92` and `claude5.rs:47` both route through `from_secrets`; `claude5_tools.rs` only constructs backends).

**SearXNG request path** (`web_search.rs:201-233`): trailing-slash trim in the constructor makes `format!("{base_url}/search")` join correctly; pre-HTTP `Url::parse` + scheme filter fails with zero HTTP calls (test pins `mock.assert_calls(0)`); GET with `q` + `format=json` matches the httpmock parameter assertions; non-2xx → error with the 403 json-format hint; parse failure → error; `take(limit)` truncation applied before mapping.

**Diagnostics** (`diagnostics.rs:760-826`): searxng → brave → venice order mirrors the tool; the `match_web_search_probe` → `match_web_search_probe_with_remediation` refactor preserves brave/venice remediation strings byte-for-byte (success arm correctly ignores the remediation). The precedence test is genuinely discriminating — a `\n` brave key would fail any brave probe, so a `Pass` + `mock.assert()` proves the searxng branch ran.

**Plumbing**: both population sites (env in `cli.rs`, vault in `initialize.rs`) gained exactly one line each; every other `ToolSecrets` literal uses `..ToolSecrets::default()` (verified all 6 remaining sites); the redacting `Debug` impl hides the URL value; the new `env_vars` entry satisfies the test's non-empty/no-whitespace invariant.

**`secret_registry.rs` exclusion is correct, not an omission**: its only consumer is the legacy `server.env` → vault migration shim, whose removal deadline (2026-08-18) is already past and which exists purely for pre-vault installs — a brand-new variable cannot have legacy entries to migrate, and the docs correctly state the server reads it from the vault only.

### Empirical checks
- `fabro-agent` web_search tests: **30/30 pass**
- `fabro-server` check_web_search tests: **7/7 pass**
- `fabro-workflow` search tests: **1/1 pass**
- The one `unused_imports` warning in the server test build is in `server/tests.rs:42`, a file untouched by this diff — pre-existing.

### Considered and rejected as non-defects (for the record)
- **Set-but-empty `SEARXNG_URL=""` selects SearXNG and masks paid keys** — mirrors the existing behavior of an empty `BRAVE_SEARCH_API_KEY` masking Venice exactly; not a new defect class.
- **Base URLs containing `?` query or junk paths mangle the endpoint** — GIGO config that yields a clear error (parse failure or instance error status), never a silent wrong result; the validation promise is scheme-only by design.
- **`max_results=0` renders "No results found."** — identical to Brave's `count=0` behavior; consistent with the existing contract.