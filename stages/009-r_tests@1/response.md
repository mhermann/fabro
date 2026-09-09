Review complete. I read the full diff, opened the surrounding files, and ran every new/changed test (all pass: 32 `web_search`, 7 diagnostics, 2 `spawn_env`, 12 `config`, 5 `fabro-static`).

## Verdict

Coverage is genuinely good where it matters, and the tests are behavior-focused, not implementation-coupled. The httpmock tests pin the SearXNG wire contract (GET `/search` with `q`/`format=json`/`pageno=1`, bearer only when keyed, `header_missing("authorization")` when keyless), result slicing, and status→error mapping with exact user-facing messages. The doctor tests pin precedence and blank-URL fall-through, and all six web-search diagnostics tests pin `env_lookup` — I verified `default_env_lookup()` reads the real process env (`test_support.rs:655`), so the ambient-`SEARXNG_URL` trap was real and is fixed. The `spawn_env` test asserts both inclusion and exclusion (fail-closed on the key). I found no test that passes regardless of the code.

## Findings

**1. `SEARXNG_URL`'s "not a secret" classification is unpinned — the implementer's claim is false.** `lib/foundation/fabro-static/src/secret_registry.rs` added `SEARXNG_API_KEY` to the vault list and to `classifies_optional_vault_secrets`, but the existing test `leaves_legacy_aliases_and_non_secret_config_unclassified` (line 124) — whose whole purpose is pinning non-secret config vars, and which lists the direct analogue `DAYTONA_API_URL` — was not extended with `SEARXNG_URL`. This classification is load-bearing: `optional_vault_secrets()` feeds migration `2026052501_optional_server_env_secrets_to_vault.rs`, which vacuums env values into the vault — while both `tool_secrets_from_configured_sources` and the doctor read the URL from process env only. One future copy-paste line into `OPTIONAL_VAULT_SECRETS` silently kills web search for every operator with no failing test. The summary even states "SEARXNG_URL pinned as unclassified"; it is not.

**2. The doctor's SearXNG success and HTTP-status branches are untested, though they are now trivially testable.** Only the connectivity-error branch is covered (`http://127.0.0.1:1`). The pre-existing excuse — brave/venice probes hit hardcoded URLs — doesn't apply here: `check_web_search` takes the SearXNG URL from the injectable `config_env_lookup`, and `httpmock::MockServer` is already imported in this test module. "searxng: configured and reachable" (`Pass`) and "searxng: HTTP 403" plus the probe's exact query string are two-line tests that weren't written. Parity caveat: brave/venice probes share this gap, so it's consistency, not regression — but `fabro doctor` is the docs' prescribed operator verification step for this feature.

**3. Minor:** `search_searxng`'s 200-with-non-JSON parse-failure branch has no test (parity with brave/venice, which are also untested there — the docs correctly route the realistic misconfig to 403, which is covered).

**4. Trivial:** `env_var_constants_are_non_empty_and_single_tokens` (`env_vars.rs:166`) wasn't extended with the two new consts, though `BRAVE_SEARCH_API_KEY`/`VENICE_API_KEY` are in it; the summary's "+ test list" claim for `env_vars.rs` is inaccurate.

**5. Trivial:** agent-side `from_secrets` with blank URL *plus* a Brave key (the realistic compose misconfig) isn't tested; the diagnostics side has exactly that test (`check_web_search_ignores_blank_searxng_url`), the agent side only tests blank + no keys → `None`.

Findings 1 and 2 are worth fixing before merge; the rest are optional.