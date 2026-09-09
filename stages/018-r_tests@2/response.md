I've completed my review. Here is my verdict on test coverage.

## Overall

The core of this change — backend selection precedence, the SearXNG wire contract, and the env-allowlist plumbing — is well tested with behavioral tests that would genuinely fail on regression. I ran the targeted suites myself: 32 fabro-agent tests and 9 fabro-server tests, all passing. But the coverage has a significant hole: **the one code path real runs take has no test at all**, plus three smaller named gaps.

## What is well covered (verified behavioral, not implementation-pinned)

- **Precedence** is pinned at both layers via backend identity (`matches!`) and doctor summary strings — flipping the order fails them.
- **Wire contract** is pinned with httpmock `when` matchers + `mock.assert()`: GET, `/search`, `q`/`format=json`/`pageno=1`, `Accept`, bearer present *and* absent (`header_missing`). Changing the request shape fails these.
- **Slicing** asserts the third result is absent — fails if `take()` is dropped.
- **Blank-URL filtering** (`""` and `"   "`), trailing-slash strip, and URL construction are pinned agent-side.
- **Hermeticity was done right**: the pre-existing brave/venice doctor tests got `.env_lookup(|_| None)` pins, and the new tests set or absent `SEARXNG_URL` explicitly — an ambient `SEARXNG_URL` in a developer shell cannot flip them.
- **spawn_env** asserts both directions: `SEARXNG_URL` crosses `env_clear()`, `SEARXNG_API_KEY` does not. **Debug redaction** asserts neither the URL nor key leaks.

## Findings

**1. The worker pipeline path — the feature's actual integration point — is untested.** `tool_secrets_from_configured_sources` (`lib/components/fabro-workflow/src/pipeline/initialize.rs:319`) is the only place real server-driven runs assemble `ToolSecrets`, and it implements the deliberately split-source design: `SEARXNG_URL` from process env, `SEARXNG_API_KEY` from the vault. No test references it (the file's `mod tests` already imports `Vault`/`AsyncRwLock`, so the vault half is testable today). If `std::env::var(EnvVars::SEARXNG_URL)` were deleted or read the wrong const, every existing test still passes, `fabro doctor` still passes (it reads env directly), and `web_search` silently disappears from all runs. Concretely: delete that one line and nothing in the repo goes red. The env half needs a small refactor to test hermetically (an injected lookup closure, as `TestAppStateBuilder.env_lookup` already models — `std::env::set_var` is clippy-banned here), which is why this is worth doing rather than waving off.

**2. The doctor never exercises `SEARXNG_API_KEY` from the vault.** All four searxng doctor tests configure no searxng key, so `diagnostic_secret(..., EnvVars::SEARXNG_API_KEY)` returning `Some` and the `bearer_auth` attach in `check_searxng_search` never execute. If the vault lookup used the wrong constant, no test fails. The brave/venice equivalents *are* exercised (their vault-key tests read `"invalid\n"` from the vault before probing). Fix is one line: add `(EnvVars::SEARXNG_API_KEY, "...")` to the vault entries in `check_web_search_prefers_searxng_when_url_and_both_vault_keys_exist`.

**3. The doctor's trailing-slash trim is untested.** `check_web_search` does `.trim().trim_end_matches('/')` before `format!("{base_url}/search?...")`, but no doctor test passes a trailing-slash URL; removing the trim (yielding `//search`) fails nothing. The agent-side equivalent is tested (`from_secrets_appends_search_path_and_strips_trailing_slash`) — the doctor copy is not.

**4. `from_secrets` blank-URL *fall-through* is untested.** `from_secrets_ignores_blank_searxng_url` passes no other keys, so an implementation that returned `None` early on a blank URL would pass while breaking the "blank URL falls through to Brave" behavior. One line fixes it: `secrets(Some("brave-key"), None, Some("   "), None)` → assert `Brave`.

**Minor:** `register_web_search_tool` (tools.rs:92) is only tested negatively (default secrets → absent); the searxng-only → registered case is covered only transitively through `from_secrets`. It's a 3-line wrapper, so borderline acceptable.

## Non-issues I checked

JSON-parse-failure and transport-error mapping are untested, but that matches the existing brave/venice coverage style; the 403 hint branch shares the tested 401 branch; the `MAX_RESULTS` clamp in `format_searxng_results` is defensive duplication of the tool-arg clamp; the 30s timeout isn't unit-testable. The parity-matrix tests build `ToolSecrets` explicitly and are unaffected by ambient `SEARXNG_URL`.

Findings 1 and 2 are the ones I'd insist on before calling coverage adequate — 2 is a one-liner, and 1 is the difference between "the feature works in unit tests" and "the feature works in a run."