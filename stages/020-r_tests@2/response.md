# Test coverage review — Forgejo integration

**Verdict: the leaf layers are well tested, but the seams between them are not.** Every individual piece has tests, yet no test anywhere exercises a *configured* Forgejo instance through the server or the workflow pipeline — the exact paths where this change's dispatch logic lives. The twin server and a `TwinForgejo` helper were built to make that testable, and the helper has zero users.

## What is adequately covered

- **`fabro-forgejo` crate** — strong. 30+ tests assert the client contract (request headers, Gitea-shaped bodies like `{"Do":"squash"}`, status→error mapping), token redaction, URL normalization, and the port-bearing regression cases. Tests ask "what does the client send/return", not internals. Twin tests drive the full PR lifecycle (create → reconcile by head SHA → merge closes it → auto-merge carries `merge_when_checks_succeed`) over real HTTP.
- **`clone_source.rs`** — 8 new tests: forge decision, port-bearing decision/parse/record, subpath origins, and negatives (`github_origins_stay_github_even_with_a_forgejo_instance_configured`, `forge_origins_fail_without_a_configured_instance`). These would fail if the dispatch broke.
- **Webhooks** — unit tests for HMAC verification plus an HTTP-route test covering all three accepted header spellings and wrong-secret rejection.
- **Wire compatibility** — `forgejo_pull_request_link_matches_openapi_shape` proves `forge` is omitted (not null) for GitHub links, with type-identity and round-trip checks; run-integrations omission test likewise.
- **Web** — 3 behavioural wizard tests (save, invalid-URL block, skip), chip rendering, URL validation, install-api request shapes.
- **CLI** — `pr_link_labels_forgejo_pull_request_from_configured_instance` snapshot test.

## Specific untested cases

**1. Server PR endpoints with a configured instance — the core dispatch is never tested.** `server/tests.rs` only tests the *unconfigured* path (`merge/close/get ... _unconfigured_forgejo_link` → 503). The Forgejo arms of `prepare_pull_request_host`, `forgejo_http_client()`, and the merge/close/get dispatch with the instance PAT have no test. Same for `pull_request_record_from_link_request`'s forge branch: no test POSTs a link URL to the server. Telling detail: `TwinForgejo` was added to `fabro-test` precisely to serve this kind of test, and nothing in the repo uses it. Concretely testable: start `TwinForgejo`, set `state.forgejo` to the twin URL, POST `/runs/:id/pull_request` with a twin PR URL, assert the record carries `forge`, then merge and assert the twin received the merge.

**2. `resolve_forgejo_token` (`initialize.rs`) — zero tests on all three branches.** The focused `build_sandbox_env` test module exists, the `spec()` helper even gained a `forgejo_requested` parameter — and it is `false` in every call. The GitHub analogues (declared repos require credentials / require an origin) are tested right there. A regression that makes a requested token silently resolve to `Ok(None)` instead of failing closed would pass the entire suite.

**3. `PullRequestLink.forge` written at PR creation (`link_forge`) — never asserted.** Every workflow test runs `forgejo: None`. If `link_forge` dropped the port or embedded credentials into the instance URL, the persisted record would be wrong and *every existing test would still pass* — yet that value drives all later server-side merge/close dispatch (the port bug deep_review caught would have been caught here).

**4. "Forgejo Token" preflight check (`run_manifest.rs` ~521–560) — both branches untested.** The GitHub Token check has multiple tests asserting it runs, passes, and reports invalid permissions; a run requesting `run.integrations.forgejo.token` with no configured instance (the failure users will actually hit) has none.

**5. `resolve_workflow_env`'s forgejo arm (`services.rs:381`) — `FORGEJO_TOKEN` injection untested.** The same module tests `workflow_tool_env_provider_merges_current_github_token`; the forgejo equivalent is missing.

**6. Forgejo git-bridge entries are asserted as constructed data, never executed.** `forgejo_bridge_entries_are_instance_scoped_and_secret_free` asserts env keys/values. The GitHub counterpart in the same file runs real `git ls-remote` against a local bare fixture, proving the credential helper fills username/password and `insteadOf` rewrites actually work. The `FORGEJO_CREDENTIAL_HELPER` shell script is never executed by any test — a quoting bug in it would ship green.

**7. Smaller gaps:** link-endpoint error branches (instance URL failing forge parse → `invalid_pull_request_url`; forge URL with no instance → `unsupported_pull_request_provider`); the supervisor's forge dispatch (`pull_request_supervisor.rs:190`); the forgejo webhook route returning 401 when the signature header is *absent* (only wrong-signature is tested at route level); `[server.integrations.forgejo]` with `url` set but `enabled` omitted → silently disabled (`unwrap_or(false)` boundary, config test only covers the absent section). Server `check_forgejo`/`forgejo_integration_status` are untested, though their GitHub counterparts appear equally untested — parity, noted for completeness.

## Structural observation

Coverage is inverted relative to risk. The pure functions (URL parsing, HMAC, serde) — the least likely to regress — have redundant coverage across crate tests, twin tests, and round-trip tests. The dispatch glue connecting them (settings → `forgejo_config` → host selection → forge link → server merge) — where every real defect so far lived, including the port bug — has none. One end-to-end test with a configured twin instance would close findings 1, 3, and most of the risk in 2 and 4 simultaneously; the harness for it already exists and is currently dead code.