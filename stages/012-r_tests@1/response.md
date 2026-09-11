I have a complete picture now. Here is the test-coverage review.

## Verdict

Coverage is genuinely strong at the layers where this change is biggest — the `fabro-forgejo` client, the twin, clone decisions, the web wizard, and the API schema — but several **new integration behaviors have no test at all**, and two of them are fail-safe gates whose silent regression would matter.

## What's well covered (no findings)

- **Client ops** (`fabro-forgejo/src/tests.rs`, 31 tests): every REST op through a scripted mock asserting the actual wire contract (`Authorization: token …`, `{"Do":"squash"}`, `merge_when_checks_succeed`, `{"state":"closed"}`), status→error mapping, the Gitea `commit.id` field, subpath-instance semantics, and token redaction in errors/Debug. For an API client, the wire format *is* the behavior, so these test behavior, not internals.
- **Twin tests** (`test/twin/forgejo/tests/client_ops.rs`): drive the real client against real bare-repo smart HTTP — full PR lifecycle, reconcile-by-head-sha including the mismatch case, post-merge absence from the open list (this also covers open-state filtering the mock fixtures can't), auto-merge body shape, 401 on bad token.
- **Clone dispatch** (`clone_source.rs`): Forge decision, subpath instance, GitHub precedence with instance configured, foreign-host rejection.
- **Webhook route**: all three signature header spellings accepted, wrong secret → 401.
- **`PullRequestLink.forge`**: fabro-types parse/serialize tests + fabro-api round-trip with type-identity assertion and the omit-when-None compat case.
- **Web**: wizard save/https-validation-block/skip paths, API wrappers including structured errors, chip rendering.

## Findings — specific untested cases

**1. The sandbox token gate `resolve_forgejo_token` (fabro-workflow `pipeline/initialize.rs:194`) has zero coverage.** No test anywhere in the workspace sets `forgejo_requested: true` (verified by grep) or passes a non-`None` forgejo config to `build_sandbox_env` — every existing test passes `None`/`false`. Untested cases: token requested with no configured instance → must hard-error; configured instance but origin on a different host → must fail closed; happy path → token resolved *and* instance-scoped bridge entries merged. This is the gate that decides whether `FORGEJO_TOKEN` reaches a sandbox. Invert one condition and the token silently leaks to a non-instance origin — no test would fail.

**2. Server PR handlers: only the negative path is tested.** The three new tests in `server/tests.rs` (12315, 12347, 12380) all assert the *unconfigured* 503/record-fallback. No test ever constructs `AppState` with a configured instance (`TestAppStateBuilder` has no forgejo setter at all). Concretely untested: (a) merge/close of a Forgejo-scoped link *with* a configured instance — the `PreparedPullRequestHost::Forgejo` arm never executes in any test; (b) a link whose `forge` base URL differs from the configured instance (the `config.instance.as_str() == forge` filter at `pull_requests.rs`) — this must 503 but nothing pins it; (c) `POST /runs/{id}/pull_request` with a Forgejo URL — the `pull_request_record_from_link_request` forge branch is only exercised by the CLI test, which mocks the server away.

**3. Preflight Forgejo checks (`run_manifest.rs`) are untested, and untestable as written.** The test module has zero forgejo references. The "Forgejo Token" pass/error branches and `forgejo_repository_access_check` pass/fail have no tests. Notably, the GitHub path was made testable via the injectable `check_remote_ref: F` closure — the new Forge branch returns early and calls the network directly, bypassing that seam. So this is both a coverage gap and a missing testability seam where the codebase had just established one.

**4. Docker/Daytona Forge clone credential embedding is untested.** All sandbox tests pass `forgejo: None`. The new sequences — Docker's `clone_repo(CloneOrigin::Forge)` PAT embedding, and Daytona's basic-auth clone plus post-clone `set-url` with `embed_token_in_url` (a PAT written into the remote URL — security-sensitive) — execute in no test. Caveat: the GitHub clone path has a similar live-test-only posture, so this is consistent with house style; but no Forge variant was added to the existing `docker_streaming`/`daytona_integration` live tests either, so the new code path is strictly less covered than its GitHub sibling.

**5. Smaller gaps, each a named case:**
- `check_forgejo` diagnostics probe (`diagnostics.rs:132`) — no test; the configured/unconfigured/vault-missing branches are pure logic.
- `forgejo_integration_status` (`server/handler/system.rs:123`) — no test.
- CLI install wizard Forgejo step (`choose_forgejo_install`) and doctor's Forgejo check — no CLI test; `tests/it/cmd/install.rs` and doctor tests have no forgejo coverage, while the GitHub steps do have test patterns to copy.
- Webhook request with *no* signature header — the test covers wrong-secret but not missing-header (the GitHub route's equivalent case exists; forgejo's doesn't).
- `create_pull_request` with `draft: true` — only `draft: false` body shape is asserted.

**Bottom line:** the crate-internal work would likely catch a wire-format or parsing bug; the cross-crate fail-safe gates (finding 1 especially, then 2 and 3) are where this change can regress with a green test suite.