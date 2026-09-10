## Test coverage review — SearXNG backend for `web_search`

**Verdict: coverage is adequate.** The new behaviour is tested at every layer it touches, the tests assert behaviour (wire contract, rendered output, error text) rather than internals, and I verified they discriminate — details below. All 20 touched tests pass in this checkout (I ran the scoped suites: 13 in fabro-agent, 7 in fabro-server).

### What's covered, and why the tests would actually fail

- **Selection precedence** — six `from_secrets` tests cover every decisive branch, including `searxng > brave > venice` with both paid keys present and the searxng-only case. A reorder of the match arms fails these.
- **Wire contract** (httpmock): GET, path `/search`, `q`, and `format=json` are all asserted via `mock.assert()`. Dropping `format=json` — the single most important request detail for real instances — fails the mock match.
- **Result mapping**: title/url/`content`→description/`publishedDate`→date all asserted on rendered output.
- **Boundary**: truncation to `max_results` uses 3 results / limit 2 with a *negative* assertion (`!output.contains("Three")`), so both no-truncation and off-by-one mutations fail.
- **Failure semantics**: 403 → exact actionable hint string; 500 → plain status error; invalid/`ftp://` URL → error with `mock.assert_calls(0)`, proving the pre-HTTP validation genuinely short-circuits.
- **Trailing-slash trim** is pinned at *both* layers: the agent test feeds `{base}/` against a `/search` mock (an untrimmed URL would request `//search` and not match), and the diagnostics precedence test does the same through `check_web_search`.
- **Diagnostics**: precedence over a paid key, 403 with the searxng-specific remediation, connectivity error with remediation, and both not-configured remediation pins updated.
- **Redaction**: `searxng_configured: true` present, URL value absent from `Debug`.
- **Schema parity** across all three backends (`all_backends_use_the_same_tool_schema`).

### Named gaps (none blocking)

1. **`claude5_tools.rs` per-backend schema test (~line 539) pins brave and venice but has no searxng arm** — the one symmetry this change left unpinned. Low severity because claude5's schema is a backend-independent overwrite, but the new backend should appear there for consistency.
2. **The probe-timeout arm of the refactored `match_web_search_probe_with_remediation` is untested for every provider** (the only timeout test is Daytona's). Pre-existing — the refactor only changed where remediation comes from — but any mutation in that arm's behaviour would go unnoticed.
3. **No `check_web_search_ignores_env_backed_searxng_url`** mirroring the brave/venice vault-only pins. Not a real defect: the property lives in shared `diagnostic_secret` code already pinned twice, so a dedicated test would pass trivially.
4. **The two population sites** (`cli_tool_secrets`, `tool_secrets_from_configured_sources`) remain untested — pre-existing for all three fields; the new lines mirror their siblings exactly.

No live/E2E variant exists for SearXNG (the parity matrix stays credential-gated on Brave), which is reasonable — the httpmock suite pins the exact wire contract a live instance would speak.