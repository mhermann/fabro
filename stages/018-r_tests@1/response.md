I've completed the coverage review. One caveat up front: the shared disk is at 100%, so I could not compile or run the suite myself. Everything below is from reading signatures against call sites; where I claim a type error, I verified the exact pattern with a standalone `rustc` repro since `cargo` cannot run in this environment.

# Test-coverage review — Forgejo integration

## Blocking: the sandbox crate's existing test suite no longer compiles

The change modified production signatures but did not update existing test call sites. As written, `cargo nextest run -p fabro-sandbox`, `-p fabro-agent`, and any `--tests` build of this tree cannot compile — which also means the implement stage's claim of a successful full `--workspace --tests` build cannot be accurate for the final state.

- `lib/components/fabro-sandbox/src/clone_source.rs` — `decide_clone` gained a 6th parameter (`forgejo_instance_url`); ~12 existing test calls (lines 480, 497, 507, 527, 571, 588, 604, 632, 651, 662, 668, 683) still pass 5 args. E0061.
- `lib/components/fabro-sandbox/src/docker.rs` — `DockerSandbox::new` grew a `forgejo` param (now 8); the crate's own tests at lines 2717 and 2735 pass 7 args.
- `lib/components/fabro-sandbox/src/daytona/mod.rs` — `DaytonaSandbox::new` now takes 9 params; the internal tests at 3277, 3297, 3850, 3887, 4206 pass 8.
- `lib/components/fabro-sandbox/src/push_credentials.rs` — `PushCredentialState::new` now takes `Option<Arc<PushTokenSource>>`; the test helper `minting_state` (line 493) passes `Arc<InstallationTokenSource>`. E0308 (confirmed via minimal repro).
- `lib/components/fabro-sandbox/src/sandbox.rs` — same mismatch at three test sites (lines ~2262, ~2394, ~2669); the file was not touched by the diff at all.
- `lib/components/fabro-sandbox/tests/docker_streaming.rs` — six `DockerSandbox::new` calls with 7 args; file untouched.
- `lib/components/fabro-agent/tests/it/docker_shell.rs` — 7-arg `DockerSandbox::new`; file untouched (ignored test, but it must still compile).

This is a coverage regression: all the existing sandbox regression tests (lease/refresh races, pinned-revision clone, SHA validation, empty-workspace decisions) are currently unrunnable, and none of the new forgejo sandbox behavior was ever executed by a test.

## New sandbox behavior with no test at all

Even after mechanically fixing the arity/type breaks, none of these have any test:

- `decide_clone`'s Forgejo classification: instance match → `CloneDecision::Forgejo`; non-GitHub origin with no configured instance → the new error message; and GitHub-wins precedence when both could apply. Note the *existing* test `skip_clone_overrides_present_origin` uses a `gitlab.com` origin and now lands on the new error path — its expectation needs revisiting, which is itself evidence the path was never run.
- `build_token_source`: forgejo origin + context → static PAT source; GitHub origin without App creds but with forgejo context → forgejo; neither → `None`.
- `PushTokenSource::Forgejo` resolve/mint (`TokenProvenance::Static`, never refreshes).
- `forgejo_repo_layout` (SSH spellings of the instance → shared checkout layout).
- The plan's promised serialization assertion that `SandboxSpec::Docker/Daytona.forgejo` never persists: `sandbox_spec.rs`'s test module (line 286) is unchanged — no such test exists.

## Workflow engine: the PR-pipeline dispatch is untested

The devils stage explicitly flagged this gap and it was not addressed. `pipeline/pull_request.rs` gained ~246 lines of provider dispatch (`ScmTarget::resolve`, forgejo arms of `find_open_pull_request`, `create_pull_request_on_forge`, `branch_head_sha`, and the warn-and-skip auto-merge branch, plus `link_provider`/`link_origin`), but its test module received only one fixture line (`forgejo: None`). The 121 added lines in `tests/it/integration.rs` are all the same fixture field. Specific untested cases: an origin that is neither GitHub nor on the instance resolving to `None` and failing with the remediation message; a forgejo target building a link with `provider: "forgejo"` and the instance `origin`; auto-merge requested on Forgejo producing warn-and-skip rather than an error. The "manual Docker gate" cited in the implement summary is not a regression guard.

## Server: behavioral branches added with zero new tests

`server/tests.rs` and `tests/it/api/automations.rs` changes are purely fixture fields (`provider: ...Github`, `provider: None`, `origin: None`). Untested:

- `forgejo_integration_status` (disabled / missing URL / missing `FORGEJO_TOKEN` / configured).
- `GET /repos/forgejo/{owner}/{name}`: disabled → 503, unconfigured → 503, upstream 404.
- Run admission (`handler/runs.rs`): a grammar-valid forgejo target with the integration unconfigured → 422 `forgejo_integration_unconfigured`.
- PR merge/close/live-detail dispatch on `link.provider == forgejo`, including the new "origin does not match the configured instance" BAD_REQUEST branch in `load_pull_request_forgejo_context`.
- `run_manifest.rs` (+179 lines: `ConfiguredOrigin`, the forgejo-without-instance hard error) — its test module is untouched.
- Automation materializer forgejo workflow-source resolution (host-namespaced cache, basic-auth extraheader) and the diagnostics forgejo probe — fixture-only / untouched.

## Automation store: the new provider columns are untested

The migration adds `target_provider`/`workflow_source_provider`; `store.rs` maps provider↔column with a legacy NULL→GitHub default. `fabro-automation/tests/store.rs` gained no test that a forgejo target round-trips through SQLite or that a legacy row loads as GitHub — only fixture fields were added. This is precisely the behavior that fails silently (provider dropped on save).

## Web: the new provider select is untested

`automation-form.tsx` gained `targetProvider`, the `forgejoConfigured`-gated select, and `provider` in `targetFromFormValues`, but `automation-form.test.tsx` is untouched — no test that the select is hidden when not configured, or that the create/replace payload carries `provider: "forgejo"`. The `settings-integrations.test.tsx` change is a fixture row only. `repo/init.rs`'s `[run.scm]` forgejo block and the doctor check are also untested (`tests/it/cmd/repo_init.rs` untouched).

## What is adequately covered

For balance: the new crate is well tested — 11 httpmock integration tests exercising the real client (find/create/get/merge/close/branch/repo/user, auth header, error-status mapping, 404-vs-error distinction, WIP prefix) plus `url.rs` unit tests (instance spellings, ports, credential stripping, token redaction). `fabro-types` has strong behavioral coverage: legacy JSON without `provider` still deserializes, forgejo links serialize a 6-field shape vs GitHub's unchanged 4-field shape, `ForgejoUnconfigured` validation, `from_url` host dispatch. `fabro-config` covers normalization, loopback http, and enabled-without-URL failure. The `fabro-api` parity tests pin type identity and wire shape for both providers. Where tests exist, they assert observable wire/request behavior rather than internals — the quality is good; it is the middle layers (sandbox, workflow, server, store) that are either untested or currently unable to compile.