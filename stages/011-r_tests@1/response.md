All tests pass in the checkout. I have everything I need for the coverage review.

## Test coverage review: SearXNG web-search backend

**Overall: coverage is good.** The core new behaviour is tested at the right level, tests assert behaviour (request shape, mapped output, error strings) rather than implementation detail, and they would genuinely fail if the code were wrong. Two specific gaps are worth naming.

### What holds up

- **Selection ladder fully covered** (`web_search.rs`): 6 `from_secrets` tests enumerate every combination of the 3-tuple and discriminate by variant (`matches!(.., SearXng { .. })` etc.), so any ladder change fails. This is the riskiest behavioural change (Brave > Venice becomes SearXNG > Brave > Venice) and it's pinned both here and at the doctor level.
- **Wire behaviour, not internals**: `searxng_search_gets_format_json_and_parses_results` asserts the actual GET `/search` with `q`/`format=json`/`limit=3` via httpmock `query_param` matchers and `mock.assert()` — wrong method, path, or params fail the test. This is behaviour (what SearXNG receives), not implementation.
- **Operator-facing error**: the 403 → "enable json in search.formats" hint is asserted as an exact string — that hint is the feature's main failure-mode UX and it's pinned.
- **Client-side truncation**: `format_searxng_results_truncates_to_limit_client_side` asserts `Three` is absent — fails if `.take(limit)` is dropped, which is the exact defect it guards (the `limit` param being best-effort server-side).
- **Diagnostics precedence** (`diagnostics.rs`): the three new tests discriminate which backend was probed by summary string (`searxng: connectivity error` vs `brave:`/`venice:`) against unreachable URLs, and `check_web_search_ignores_env_backed_searxng_url` pins the vault-only semantics matching the existing brave/venice env-ignore tests.
- **Secret hygiene**: `tool_secrets_debug_redacts_values` now asserts the SearXNG URL value never appears in `Debug` output — a real property, extended correctly.
- **Tool-schema parity** extended to all three backends: agents see an identical `web_search` schema regardless of backend — a real product guarantee.

### Gaps (specific, not "add more tests")

1. **No registry-level test for the new enable signal.** `tools.rs` has `register_core_tools_omits_web_search_without_api_key` and `register_core_tools_passes_configured_brave_search_key`, but there is no equivalent for SearXNG: a test constructing `NativeToolOptions { secrets: ToolSecrets { searxng_url: Some(..) } }` and asserting `web_search` is registered. The user-visible promise — "set `SEARXNG_URL`, get the tool" — is only proven at the `from_secrets` layer, not at the layer that actually builds the registry. If `register_web_search_tool`'s `Option` handling broke, the SearXNG path (the new default) would silently register nothing while the Brave test still passed. This is the one asymmetry I'd actually fix.

2. **`searxng_status_error`'s non-403 branch has zero coverage.** The Venice analog covers 401/402/429 with exact strings; SearXNG covers only 403. A test asserting e.g. status 500 yields `"SearXNG returned status 500"` *without* the formats hint would fail today if someone inverted the `status == 403` condition — right now nothing would.

3. Minor, boundary: `max_results: 0` takes different paths per backend — SearXNG clamps up to `limit=1`, Brave sends `count=0` — and nothing pins this (or `max_results` > 20 at the request level). Also, the doctor probe's URL construction (`/search?q=test&format=json&limit=1`) and the `Pass`/"configured and reachable" branch are untested — but the brave/venice probes share that exact gap, so it's a consistent pre-existing pattern rather than a regression introduced here.

No e2e/twin SearXNG scenario was added to `parity_matrix.rs`, but that harness is live-credential based and the httpmock unit coverage stands in; a reasonable scope decision, not a finding.

**Verdict:** adequate to ship. If you want one change before merge, add the `register_core_tools` SearXNG registration test (#1) — it closes the only gap where new user-facing behaviour has no test at its own layer.