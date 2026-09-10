Goal: Add support for forgejo similar to github integration
Run ID: 01M25MC3ZY20G7DN7YBDVK4189
Pipeline progress: 10 of 22 stages completed

## Stage: understand
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response (9.6 KB; full value: `/tmp/fabro/runtime/blobs/63f748a6b3aa2413a3942f2befe5519cc1adc5f0788f66840c4b62bc0d458a8e.json`)
  Preview: Exploration complete. Here is my full understanding of the task.
  
  ---
  
  ## 1. Repository structure and tooling
  
  **Fabro** is an AI workflow orchestration platform: workflows are Graphviz graphs whose stages (agents, prompts, commands, human gates) run in sandboxes. It's a **Rust workspace** with a **…

## Stage: sweep
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response (13.0 KB; full value: `/tmp/fabro/runtime/blobs/5769fa266c4ab71696601e16e76cde27e0bc2bf322cfd7321361818683a60951.json`)
  Preview: The sweep is complete. Here is the full analysis.
  
  ---
  
  # Dimension-by-dimension sweep
  
  ## 1. Scope boundaries
  
  **Clearly in:** a way to configure a Forgejo instance + credentials; a Rust client for its API; enough of the run path to be end-to-end useful. **Clearly out:** unrelated integrations; the…

## Stage: ask
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > All three answers are in. Restating for the record — this is what the planning stage should build from:
  > 
  > ## Answers and what they mean
  > 
  > 1. **Scope — "b) Core + install/ops surface."** The change includes the run-path core (sandbox clone from Forgejo, run-driven PR create/view/link/merge/close, PAT credentials in settings/vault, docs + changelog) **plus** the operational surface: `fabro install` support (interactive and non-interactive flags), browser install-wizard routes in the server, system-integrations status reporting, diagnostics, and doctor checks. It does **not** include Forgejo OAuth login for the web UI, inbound webhooks, or Forgejo automations/server-submitted run targets — GitHub's `server.auth`, webhook stack, and `GitRunTarget`/admission model stay untouched. (Install wizard collects instance URL + PAT per the standing PAT-only-v1 decision.)
  > 
  > 2. **Instances — "a) Single configured instance."** One Forgejo/Gitea instance per server: `[server.integrations.forgejo]`-style single `url` + one token in the vault; run/PR origins must match that host. Persisted PR links still carry their host so widening to multiple instances later is non-breaking.
  > 
  > 3. **Gitea — "b) Forgejo + Gitea."** Same code, dual-branded "Forgejo/Gitea" across docs, settings naming, error messages, and validation; Gitea instances (gitea.com and self-hosted CE) are first-class and covered by the testing matrix.
  > 
  > Combined with the standing decisions (new `fabro-forgejo` crate mirroring `fabro-github`, host-aware `PullRequestLink`, PAT-only v1, fail-loud error for `auto_merge` on Forgejo, `WIP:`-title draft mapping, three shared merge strategies mapped with clear errors, system-trust TLS, `FORGEJO_URL`/`FORGEJO_TOKEN` env + vault pipeline, mock-HttpClient tests with env-gated live tests, `IntegrationProvider::Forgejo` status, docs page + changelog), the scope is now fully determined.

## Stage: triage
- Status: succeeded
- Handler: prompt
- Model: glm-5.3
- Response:
  > I have enough to plan. Restating what I understand the human wants, plus the decisions I'm making on their behalf:
  > 
  > **Goal:** Add Forgejo support mirroring the existing GitHub integration, spanning the same surface GitHub currently occupies in this codebase.
  > 
  > **What I'll build (as understood):**
  > 
  > 1. **New `fabro-forgejo` crate** paralleling `fabro-github` — API client for PR creation and repo operations against a Forgejo instance's REST API (Gitea-compatible shape).
  > 2. **Auth**: token-based. Forgejo has no GitHub Apps equivalent, so the faithful analogue of "installation token" is a scoped personal access token. I'll support instance URL + token (env/config following the existing GitHub settings patterns), not an OAuth2 web flow.
  > 3. **Self-hosted instance URL**: unlike the fixed `github.com`, Forgejo requires a configured base URL per instance — this goes into config (`FABRO_FORGEJO_URL` / `FABRO_FORGEJO_TOKEN` style, matching existing conventions).
  > 4. **Sandbox clone sources**: extend `fabro-sandbox` so Docker and Daytona providers accept Forgejo origins (currently non-GitHub origins fail unless `skip_clone = true`). Forgejo clones use HTTPS with token auth; the branch/exact-commit contract (shallow fetch of SHA, verify HEAD, never fall back to branch HEAD) stays identical. Origin detection distinguishes Forgejo hosts from GitHub rather than treating any Git URL as cloneable.
  > 5. GitHub paths remain untouched; no speculative refactor of `fabro-github` into a generic forge abstraction — the two crates stay separate unless a clean shared type emerges naturally in a shared crate.
  > 
  > **Decisions made on the human's behalf:** PAT-first auth (no OAuth2 dance in v1), including sandbox clone support in scope since clone-based providers rejecting non-GitHub origins is half the integration, and keeping exact-commit admission branch-only for Forgejo initially (matching current production callers).
  > 
  > {"preferred_next_label": "enough"}

## Stage: plan
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response (20.3 KB; full value: `/tmp/fabro/runtime/blobs/6e1075fc13d65febaf6f35085c1154cc021b4cc88d47729f4e491b0e85222e20.json`)
  Preview: I re-read every file in the criticized path. Three facts materially shape the revision:
  
  - `observe_git_run_target` has exactly **one** production caller (`fabro-cli/src/commands/run/create.rs:161`), so threading the instance through admission is contained.
  - `ValidatedGitRunTarget::repository()` is…

## Stage: devils
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > I attacked the revised plan, focusing on whether the new Step 3 (run admission) is actually correct against the code, and hunting for new blocking defects. This is my second critique, so I applied a raised bar: only work that would be redone or expensive-to-reverse decisions count.
  > 
  > ## What I checked and found sound
  > 
  > **Step 3's anchors are real.** `GitRunTarget` (run_intent.rs:69, `deny_unknown_fields`) taking an optional `instance_url` follows the existing `tag`/`sha` optional-field pattern exactly; `validate()` (line 83) is the single validation chokepoint and its only `.repository()` consumer is `automation_materializer.rs`, which the plan guards. `observe_git_run_target` has exactly one production caller (`run/create.rs:161`) — threading is contained as claimed. The automation guard is the right mechanism to keep the out-of-scope exclusion without narrowing local runs.
  > 
  > **Previously-raised items are genuinely resolved, not papered over:** `normalize_repo_origin_url`/`ssh_url_to_https` verified host-generic (a real read, and the forge path inherits correct normalization for free); the draft-prefix doc wording now states `WORK_IN_PROGRESS_PREFIXES` is instance-configurable; the twin git endpoint is explicitly deferred rather than silently load-bearing; the verification plan now includes admission-level tests (manifest, CLI, workflow twin tests) so the "wrong but compiling" failure mode from round one fails a test.
  > 
  > ## Near-misses I considered and rejected as blocking
  > 
  > 1. **Server-side worker credential assembly (`worker_runtime.rs`).** If the server builds `StartServices` for any run path besides the CLI worker, forgejo credentials need mirroring there too. But the `maybe_build_github_credentials` pattern establishes exactly where credential assembly lives; the implementer follows the existing call sites, and a missed one surfaces as the plan's own soft-skip warning in logs, not silent wrongness. Ordinary judgment call.
  > 2. **Gitea `state=open` filter support.** The twin will match whatever the client sends, so twin-based tests can't prove the real API accepts the param. But the plan already carries the mitigations that matter: env-gated `live_access.rs` plus the manual dual-instance smoke. Worst case the implementer flips to client-side filtering — cheap, not rework.
  > 3. **`GitRunTarget.instance_url` accepted via public `RunIntent` API.** The scope exclusion (server-submitted forge targets) was about automations and playground UX; `validate()` accepting a valid forge target submitted by an API client just runs it through the same credential-gated path as a CLI run — harmless, arguably correct. The automation guard covers the exclusion that mattered.
  > 4. **`deny_unknown_fields` forward-compat (old binary reading new data).** Same trade the repo already made when `tag`/`sha` were added; downgrades aren't a supported promise here.
  > 5. **Slug grammar limits (39/100 chars) applied to forge owners.** Documented in the plan and docs step; edge-of-edge case, reversible later with a forge-specific slug type if ever needed.
  > 
  > ## Verdict
  > 
  > The one blocking defect from round one is fixed with verified anchors and tests that would catch its absence. What remains is ordinary implementation risk — the kind later review stages and the plan's own verification steps (workspace tests, OpenAPI conformance, live smoke against both a real Forgejo and a real Gitea) are designed to catch. I have no remaining objection that would force work to be redone.
  > 
  > {"preferred_next_label": "approve"}

## Stage: plan
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response (20.3 KB; full value: `/tmp/fabro/runtime/blobs/6e1075fc13d65febaf6f35085c1154cc021b4cc88d47729f4e491b0e85222e20.json`)
  Preview: I re-read every file in the criticized path. Three facts materially shape the revision:
  
  - `observe_git_run_target` has exactly **one** production caller (`fabro-cli/src/commands/run/create.rs:161`), so threading the instance through admission is contained.
  - `ValidatedGitRunTarget::repository()` is…

## Stage: devils
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > I attacked the revised plan, focusing on whether the new Step 3 (run admission) is actually correct against the code, and hunting for new blocking defects. This is my second critique, so I applied a raised bar: only work that would be redone or expensive-to-reverse decisions count.
  > 
  > ## What I checked and found sound
  > 
  > **Step 3's anchors are real.** `GitRunTarget` (run_intent.rs:69, `deny_unknown_fields`) taking an optional `instance_url` follows the existing `tag`/`sha` optional-field pattern exactly; `validate()` (line 83) is the single validation chokepoint and its only `.repository()` consumer is `automation_materializer.rs`, which the plan guards. `observe_git_run_target` has exactly one production caller (`run/create.rs:161`) — threading is contained as claimed. The automation guard is the right mechanism to keep the out-of-scope exclusion without narrowing local runs.
  > 
  > **Previously-raised items are genuinely resolved, not papered over:** `normalize_repo_origin_url`/`ssh_url_to_https` verified host-generic (a real read, and the forge path inherits correct normalization for free); the draft-prefix doc wording now states `WORK_IN_PROGRESS_PREFIXES` is instance-configurable; the twin git endpoint is explicitly deferred rather than silently load-bearing; the verification plan now includes admission-level tests (manifest, CLI, workflow twin tests) so the "wrong but compiling" failure mode from round one fails a test.
  > 
  > ## Near-misses I considered and rejected as blocking
  > 
  > 1. **Server-side worker credential assembly (`worker_runtime.rs`).** If the server builds `StartServices` for any run path besides the CLI worker, forgejo credentials need mirroring there too. But the `maybe_build_github_credentials` pattern establishes exactly where credential assembly lives; the implementer follows the existing call sites, and a missed one surfaces as the plan's own soft-skip warning in logs, not silent wrongness. Ordinary judgment call.
  > 2. **Gitea `state=open` filter support.** The twin will match whatever the client sends, so twin-based tests can't prove the real API accepts the param. But the plan already carries the mitigations that matter: env-gated `live_access.rs` plus the manual dual-instance smoke. Worst case the implementer flips to client-side filtering — cheap, not rework.
  > 3. **`GitRunTarget.instance_url` accepted via public `RunIntent` API.** The scope exclusion (server-submitted forge targets) was about automations and playground UX; `validate()` accepting a valid forge target submitted by an API client just runs it through the same credential-gated path as a CLI run — harmless, arguably correct. The automation guard covers the exclusion that mattered.
  > 4. **`deny_unknown_fields` forward-compat (old binary reading new data).** Same trade the repo already made when `tag`/`sha` were added; downgrades aren't a supported promise here.
  > 5. **Slug grammar limits (39/100 chars) applied to forge owners.** Documented in the plan and docs step; edge-of-edge case, reversible later with a forge-specific slug type if ever needed.
  > 
  > ## Verdict
  > 
  > The one blocking defect from round one is fixed with verified anchors and tests that would catch its absence. What remains is ordinary implementation risk — the kind later review stages and the plan's own verification steps (workspace tests, OpenAPI conformance, live smoke against both a real Forgejo and a real Gitea) are designed to catch. I have no remaining objection that would force work to be redone.
  > 
  > {"preferred_next_label": "approve"}

## Stage: refactor_check
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > I examined the code shapes the plan will write into. Findings against the six signals:
  > 
  > **1. Working around the code?** No. The origin-classification ("github vs configured instance vs other") recurs across call sites, but each is a single `is_instance_origin` check selecting between two independent clients — that thin seam was the deliberate architecture decision, reaffirmed in the approved plan. There is no flag-through-five-call-sites problem: `observe_git_run_target` has exactly one production caller (`run/create.rs:161`), and `decide_clone`'s instance parameter threads through only the two provider constructors.
  > 
  > **2. Missing seam?** The candidate — a `GitForge` trait unifying `fabro-github`/`fabro-forgejo` behind one interface — was weighed and rejected twice during planning (it would refactor a stable, well-tested crate's entire public surface speculatively). The shared-seams approach (fabro-types + small `CredentialSource` enum) holds the change without it. That refactor would be its own piece of work with its own conversation if ever wanted.
  > 
  > **3. Duplication becoming triplication?** Verified: the `HttpClient`/`HttpResponse` transport abstraction exists exactly **once** today (`fabro-github/src/lib.rs:361,384` — `fabro-tracker` and `fabro-slack` define none). `fabro-forgejo` would be the second copy, ~50 lines of thin transport boilerplate. Extracting it into `fabro-http` now would churn `fabro-github`'s public API and mock infrastructure — violating the plan's own "GitHub paths byte-identical" safety property — to save marginal duplication. Below the bar.
  > 
  > **4. Unit outgrown?** Checked the riskiest: `open_pull_request` (`pipeline/pull_request.rs:592`) is 75 cohesive lines where the provider-varying calls (parse, verify, reconcile, create, auto-merge) are cleanly separable — a forge path parallelizes ~30 of them and the implementer can split into two functions sharing `build_pr_content` if it reads badly. `docker.rs` (3101 lines) takes one dispatch arm inside a contained clone function; `clone_source.rs` stays a coherent decision function; `server/handler/pull_requests.rs` (607 lines) holds parallel credential helpers fine. None is made unreadable by the planned addition.
  > 
  > **5. Tests can't reach the behavior?** No — the twin pattern (`test/twin/github`) is established and the plan adds `twin/forgejo` plus admission-level tests at manifest/CLI/workflow layers.
  > 
  > **6. Names that will lie?** Minor ones (`clone_github_repo` gaining a forge arm; `github_run_target` becoming origin-classifying) — the plan already renames the latter's siblings (`repo_layout`, `build_credential_source`); the rest are one-word renames naturally part of the change, not a pre-refactor.
  > 
  > **Conclusion:** every candidate either fails the bar or is already handled inside the plan's own steps. The only item worth putting on record for the future: extracting the shared HTTP transport trait into `fabro-http` becomes worthwhile if a third forge client ever appears — it is not needed for this change. When torn, say no; I am not even torn.
  > 
  > No refactoring is needed.
  > 
  > {"preferred_next_label": "none"}

## Stage: implement
- Status: succeeded
- Handler: agent
- Model: glm-5.3-flash
- Files touched: /workspace/fabro/Cargo.toml, /workspace/fabro/docs/public/api-reference/fabro-api.yaml, /workspace/fabro/docs/public/changelog/2026-09-10.mdx, /workspace/fabro/docs/public/integrations/forgejo.mdx, /workspace/fabro/lib/apps/fabro-cli/Cargo.toml, /workspace/fabro/lib/apps/fabro-cli/src/commands/doctor.rs, /workspace/fabro/lib/apps/fabro-cli/src/commands/run/create.rs, /workspace/fabro/lib/apps/fabro-cli/src/commands/run/run_progress/event.rs, /workspace/fabro/lib/apps/fabro-cli/src/commands/run/run_progress/mod.rs, /workspace/fabro/lib/apps/fabro-cli/src/commands/run/runner.rs, /workspace/fabro/lib/apps/fabro-cli/src/commands/runs/mod.rs, /workspace/fabro/lib/apps/fabro-cli/src/shared/forgejo.rs, /workspace/fabro/lib/apps/fabro-cli/src/shared/mod.rs, /workspace/fabro/lib/apps/fabro-cli/tests/it/cmd/pr_view.rs, /workspace/fabro/lib/apps/fabro-server/Cargo.toml, /workspace/fabro/lib/apps/fabro-server/src/install.rs, /workspace/fabro/lib/apps/fabro-server/src/run_manifest.rs, /workspace/fabro/lib/apps/fabro-server/src/server.rs, /workspace/fabro/lib/apps/fabro-server/src/server/handler/pull_requests.rs, /workspace/fabro/lib/apps/fabro-server/src/server/handler/system.rs, /workspace/fabro/lib/apps/fabro-server/src/server/pull_request_supervisor.rs, /workspace/fabro/lib/apps/fabro-server/src/server/tests.rs, /workspace/fabro/lib/components/fabro-automation/src/error.rs, /workspace/fabro/lib/components/fabro-automation/src/model.rs, /workspace/fabro/lib/components/fabro-automation/src/store.rs, /workspace/fabro/lib/components/fabro-forgejo/Cargo.toml, /workspace/fabro/lib/components/fabro-forgejo/src/lib.rs, /workspace/fabro/lib/components/fabro-forgejo/src/tests_mock.rs, /workspace/fabro/lib/components/fabro-forgejo/src/token_source.rs, /workspace/fabro/lib/components/fabro-forgejo/tests/integration.rs, /workspace/fabro/lib/components/fabro-forgejo/tests/live_access.rs, /workspace/fabro/lib/components/fabro-install/src/lib.rs, /workspace/fabro/lib/components/fabro-manifest/Cargo.toml, /workspace/fabro/lib/components/fabro-manifest/src/lib.rs, /workspace/fabro/lib/components/fabro-sandbox/Cargo.toml, /workspace/fabro/lib/components/fabro-sandbox/src/clone_source.rs, /workspace/fabro/lib/components/fabro-sandbox/src/daytona/mod.rs, /workspace/fabro/lib/components/fabro-sandbox/src/docker.rs, /workspace/fabro/lib/components/fabro-sandbox/src/lib.rs, /workspace/fabro/lib/components/fabro-sandbox/src/provider/daytona.rs, /workspace/fabro/lib/components/fabro-sandbox/src/push_credentials.rs, /workspace/fabro/lib/components/fabro-sandbox/src/sandbox.rs, /workspace/fabro/lib/components/fabro-sandbox/src/sandbox_spec.rs, /workspace/fabro/lib/components/fabro-sandbox/tests/docker_streaming.rs, /workspace/fabro/lib/components/fabro-store/src/run_state.rs, /workspace/fabro/lib/components/fabro-workflow/src/event/convert.rs, /workspace/fabro/lib/components/fabro-workflow/src/event/events.rs, /workspace/fabro/lib/components/fabro-workflow/src/git_bridge.rs, /workspace/fabro/lib/components/fabro-workflow/src/handler/manager_loop.rs, /workspace/fabro/lib/components/fabro-workflow/src/operations/retry.rs, /workspace/fabro/lib/components/fabro-workflow/src/pipeline/publish.rs, /workspace/fabro/lib/components/fabro-workflow/src/pipeline/pull_request.rs, /workspace/fabro/lib/components/fabro-workflow/src/run_options.rs, /workspace/fabro/lib/components/fabro-workflow/src/services.rs, /workspace/fabro/lib/foundation/fabro-api/build.rs, /workspace/fabro/lib/foundation/fabro-api/tests/pull_request_round_trip.rs, /workspace/fabro/lib/foundation/fabro-api/tests/run_intent_round_trip.rs, /workspace/fabro/lib/foundation/fabro-api/tests/run_summary_round_trip.rs, /workspace/fabro/lib/foundation/fabro-api/tests/server_settings_round_trip.rs, /workspace/fabro/lib/foundation/fabro-api/tests/system_integrations_round_trip.rs, /workspace/fabro/lib/foundation/fabro-client/src/client.rs, /workspace/fabro/lib/foundation/fabro-config/src/layers/server.rs, /workspace/fabro/lib/foundation/fabro-config/src/resolve/server.rs, /workspace/fabro/lib/foundation/fabro-config/src/tests/resolve_server.rs, /workspace/fabro/lib/foundation/fabro-static/src/env_vars.rs, /workspace/fabro/lib/foundation/fabro-static/src/secret_registry.rs, /workspace/fabro/lib/foundation/fabro-test/Cargo.toml, /workspace/fabro/lib/foundation/fabro-test/src/lib.rs, /workspace/fabro/lib/foundation/fabro-types/src/pull_request.rs, /workspace/fabro/lib/foundation/fabro-types/src/run_event/misc.rs, /workspace/fabro/lib/foundation/fabro-types/src/run_event/mod.rs, /workspace/fabro/lib/foundation/fabro-types/src/run_intent.rs, /workspace/fabro/lib/foundation/fabro-types/src/settings/server.rs, /workspace/fabro/lib/foundation/fabro-types/src/system_integrations.rs, /workspace/fabro/lib/foundation/fabro-types/tests/run_event_serde.rs, /workspace/fabro/lib/foundation/fabro-types/tests/run_intent.rs, /workspace/fabro/test/twin/forgejo/Cargo.toml, /workspace/fabro/test/twin/forgejo/src/handlers/branches.rs, /workspace/fabro/test/twin/forgejo/src/handlers/mod.rs, /workspace/fabro/test/twin/forgejo/src/handlers/pulls.rs, /workspace/fabro/test/twin/forgejo/src/handlers/repos.rs, /workspace/fabro/test/twin/forgejo/src/handlers/users.rs, /workspace/fabro/test/twin/forgejo/src/lib.rs, /workspace/fabro/test/twin/forgejo/src/server.rs, /workspace/fabro/test/twin/forgejo/src/state.rs
- Response (8.1 KB; full value: `/tmp/fabro/runtime/blobs/6b6882cd2cbacff386bf4f37bc43672db6c6a4db1df2a4eb339e1226e4e9abe1.json`)
  Preview: All verification is green. Here's the complete summary:
  
  ## Forgejo/Gitea Integration — Implementation Summary
  
  ### What was built (by plan step)
  
  **Step 1 — Foundation**
  - `lib/foundation/fabro-static/src/env_vars.rs` — added `FORGEJO_TOKEN`, `FORGEJO_URL`
  - `lib/foundation/fabro-static/src/secret_…


**First, get the change in front of you.** Nothing has been handed to you, so
find it yourself:

    git --no-pager diff origin/main...HEAD

If `origin/main` does not resolve, use `main` instead. Read whole files with
`git show` or by opening them whenever the diff alone is not enough to judge —
a diff hides the context a line sits in, and most real defects live in that
context.

You are reviewing only. Do not edit, create, or delete any file, and do not
commit. Three other reviewers are working in this same checkout at the same
time, and a write from you would corrupt what they are reading.

Review the change for test coverage only.

Ask: does this change have tests, do they test behaviour rather than
implementation detail, and would they actually fail if the code were wrong?

Look for: new behaviour with no test, tests asserting what the code does rather
than what it should do, tests that pass regardless of the implementation,
missing failure and boundary cases, and tests coupled to internals in a way
that will break on any refactor.

Name the specific untested case. "Add more tests" is not a finding.

If coverage is adequate, say so plainly.
