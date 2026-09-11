Goal: Add support for forgejo similar to github integration
Run ID: 01M27HQ4ZCMNF8T64PSGESXABZ
Pipeline progress: 8 of 22 stages completed

## Stage: understand
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response (8.6 KB; full value: `/tmp/fabro/runtime/blobs/438d2ca833e28212e413fc9ce7598ae9a92103303fd1a15be6f33ca84a89e372.json`)
  Preview: Exploration complete. Here is my understanding.
  
  ## 1. Repository structure and where the change lands
  
  **Language/tooling:** Rust workspace (crates in `lib/apps`, `lib/components`, `lib/foundation`) + TypeScript/React 19 web app (`apps/fabro-web`, Bun). OpenAPI-first API (`docs/public/api-reference…

## Stage: sweep
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response (14.6 KB; full value: `/tmp/fabro/runtime/blobs/0dd3cad2b1b6e6023c89191fe2d0320423f45d6c420b6b311e81a1f04f8b31d9.json`)
  Preview: I've now read the remaining integration surfaces (automation triggers, install flow, OAuth login, config defaults, API wire format, git bridge, PR pipeline). Here is the full sweep.
  
  ## The sweep, dimension by dimension
  
  ### 1. Scope boundaries
  "Similar to github integration" maps to a ten-surface s…

## Stage: ask
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > All five answers are back. Restatement for the planning stage:
  > 
  > ## Answers and what they mean
  > 
  > 1. **Scope — B: Core + tokens, webhooks, install, diagnostics.** Build: `fabro-forgejo` credentials, clone into Docker/Daytona sandboxes, PR create/get/merge/close (API + web UI), `[server.integrations.forgejo]` settings, sandbox token minting (`run.integrations.forgejo` → `FORGEJO_TOKEN`), webhook route, install flow (CLI + web wizard), doctor/diagnostics, docs. **No OAuth2 login, no automation-target changes** (those were scope C), and the tracker stays out regardless.
  > 
  > 2. **Instances — A: Single instance per server.** One URL + one PAT in server settings; `owner/repo` slugs are reused scoped to that instance. No wire-format change to `GitRunTarget`/`RunTarget`, no persisted-data migration, no host-qualified identity. Non-configured hosts are rejected.
  > 
  > 3. **Auth — A: PAT only.** One credential variant (`Authorization: token <PAT>`). No OAuth2 app machinery, no Forgejo Apps, no version gate beyond current stable Forgejo.
  > 
  > 4. **Architecture — A: Additive parallel crate.** New `lib/components/fabro-forgejo` mirroring `fabro-github`, with explicit dispatch at boundaries. GitHub code paths, wire formats, and persisted data remain untouched — zero GitHub regression risk; duplicated shape is accepted.
  > 
  > 5. **Webhooks — A: Inert parity.** The Forgejo webhook route verifies HMAC and logs the delivery, exactly like the GitHub route. No event-driven triggering, no new security surface.
  > 
  > Standing decisions from the sweep that remain in force: HTTPS-only instance URLs; accept `X-Forgejo-*`/`X-Gitea-*`/GitHub-compat webhook headers (all HMAC-SHA256); `MergeStrategy` maps directly (merge/squash/rebase), auto-merge endpoint verified against the real API during implementation; env vars `FORGEJO_URL`/`FORGEJO_TOKEN` registered in `fabro-static` env_vars + secret registry; integration default-off via `enabled` flag; tests via ported `MockHttpClient` + env-gated live tests; OpenAPI-first workflow for any spec changes; docs + changelog per repo convention.

## Stage: triage
- Status: succeeded
- Handler: prompt
- Model: glm-5.3
- Response:
  > The answers covered the material questions: scope mirrors the existing `fabro-github` integration (auth + PR creation), Forgejo uses token-based auth rather than GitHub App JWT/installation tokens, and self-hosted base URLs must be first-class.
  > 
  > What I understand the human wants, including decisions I'm making on their behalf:
  > 
  > - **New `fabro-forgejo` crate** parallel to `fabro-github**: a Forgejo API client covering token auth (PAT, with base-URL config for self-hosted instances), PR creation/update, and issue comments — the same product surface `fabro-github` provides today, adapted to Forgejo's simpler token model.
  > - **Configuration and secrets**: a `forgejo` config section (base_url + token) loaded through the existing server-secrets startup/validation path, not a new secrets mechanism.
  > - **Shared abstraction kept minimal**: I'll introduce a narrow trait (e.g., PR creation) only where both forges must be interchangeable for callers, rather than a speculative full forge abstraction. The crates stay separate.
  > - **Sandbox clone-source contract stays GitHub-only** in this change; extending clone-based providers (Docker/Daytona) to Forgejo origins is a follow-up with its own admission/clone semantics. This is the one scope line I'm drawing myself, since it's separable and touches a documented contract.
  > - **Testing**: twin/scripted HTTP tests like other integration crates; no live Forgejo dependency in CI.
  > 
  > Remaining gaps (exact Forgejo API versions, OAuth2 app flow vs PAT-only) don't change the shape of what gets built — PAT-first with the client structured so an OAuth2 flow can be added later.
  > 
  > {"preferred_next_label": "enough"}

## Stage: plan
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response (20.8 KB; full value: `/tmp/fabro/runtime/blobs/2629b1ff29c3eaff078eeb63c23dd8fc1018e1afa026629cd693ec18cba8e129.json`)
  Preview: I have verified every integration point against the current code. Here is the implementation plan.
  
  ---
  
  # Implementation Plan: Forgejo integration (scope B, single instance, PAT-only, additive crate, inert webhooks)
  
  ## Governing decisions (from the human's answers)
  
  - **Scope B**: credentials, clo…

## Stage: devils
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > I attacked the plan against the repository as it actually is. Findings, then the verdict.
  > 
  > ## Defects found
  > 
  > **1. `fabro-manifest` is omitted entirely.** `lib/components/fabro-manifest/src/lib.rs` consumes `normalize_repo_origin_url` and `parse_github_owner_repo` at sites ~382, 418 (`parse...ok()?` — silently drops owner/repo for non-GitHub origins), 449, 517–520, and 1967 when building run manifests from a local checkout. The plan declares the manifest lane (full `git.origin_url`) the *primary entry* for Forgejo runs, yet never audits the crate that constructs those manifests. A local `fabro run` in a Forgejo checkout may produce a manifest with the origin but missing the owner/repo identity downstream PR creation needs. Fix is the same dispatch pattern as every other site — additive, test-catchable, no decision reversal.
  > 
  > **2. Two threading links are unnamed.** `run_manifest.rs:602` (`parse_github_owner_repo` in the git-coordinate path, before preflight) and `fabro-workflow/src/operations/start.rs:114` (`RunServices.github_app`, which feeds `publish.rs`). The plan names `run_options.rs`, `services.rs`, `publish.rs`, and says "thread into worker run options," so the chain is implied — but these two specific links are where a Forgejo run would fail first. Greppable in minutes.
  > 
  > **3. The Gitea→`PullRequestGithubDetail` DTO mapping is never called out.** Forgejo's PR JSON is not GitHub-shaped (different `state` vocabulary, nested head/base objects). `get_pull_request` must convert into the shared `PullRequestGithubDetail`. The plan's wording ("handlers branch identically") implies it, but the conversion is the riskiest parsing code in the crate and deserves a named test fixture.
  > 
  > **4. Twin-forgejo inherits my API assumptions.** Both the mock tests and the twin encode the same possibly-wrong beliefs about the Gitea-lineage REST surface (merge endpoint parameters, auto-merge shape, token-in-URL username). Mock tests would pass even if the real API differs. Mitigations exist in the plan (explicit auto-merge fallback, env-gated live tests, mandatory manual E2E against a real Forgejo docker) — but the live tests are opt-in, so the manual pass is the real gate and must not be skipped.
  > 
  > **5. Web run-creation path unverified.** PR chips are `html_url`-driven (fine), but nothing in Phase 5 confirms the web/playground run-creation form passes a Forgejo URL through the manifest lane rather than validating for github.com. If it validates, a small addition is needed; the plan says "verify" for chips but not for creation input.
  > 
  > **6. Rollback nuance:** adding `[server.integrations.forgejo]` to settings.toml means downgrading to an older binary will reject the section (settings structs are strict). Standard posture for this repo (new keys always do this), not a plan defect.
  > 
  > ## Bar check
  > 
  > None of these forces redoing work or reverses a decision: the architecture (additive crate, single instance, PAT, dispatch points) is unaffected; every gap is an additive call-site audit or a named test within files/patterns the plan already establishes, and the scenario test in Phase 6 would surface each one. Per the stated bar — blocking means wrong work or expensive-to-reverse decisions — these are implementer judgment calls plus review-stage catches, not revision triggers. The `PullRequestLink.forge` wire addition is explicitly flagged, justified, and cheap to remove if unwelcome.
  > 
  > The plan is sound enough to build from, with the six notes above carried into implementation.
  > 
  > {"preferred_next_label": "approve"}

## Stage: refactor_check
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > The threading hop is confirmed: plain struct fields (`github_app: Option<GitHubCredentials>` in `StartOptions`/`StartServices`), `.clone()` at each of five hops, consumed directly by `publish.rs`. That grounds the decision.
  > 
  > ## Reasoning
  > 
  > **1. The central refactoring question was already answered by the human.** The vetted architecture question offered exactly this choice: additive parallel crate vs. shared `Forge` trait refactor across ~100 call sites. The answer was additive, with "GitHub code paths untouched" as the stated benefit. A refactoring stage that extracted a forge abstraction would override an authoritative answer — the main candidate is disqualified before any code reading.
  > 
  > **2. The seams this change needs are part of the change, not pre-work.** `PullRequestHost`, `CloneDecision::Forge`, `ForgejoInstance`, the parallel credential fields — none can exist meaningfully before the `fabro-forgejo` crate and its config types do. They are the plan's own Phase 0–2 content. Doing them "first" as a refactor would just be doing the implementation in the wrong order.
  > 
  > **3. The parallel-field threading is mechanical, not the plan working around the code.** Just verified in `operations/start.rs` and `publish.rs`: each hop is one struct field plus a `.clone()`. The actual dispatch logic is confined to two consumption points (`publish.rs` host selection, `initialize.rs` token injection). Five additive fields is the accepted price of zero GitHub regression risk, and the diff stays proportionate to the idea.
  > 
  > **4. Duplication is bounded and was explicitly accepted.** The `HttpClient`/`HttpMethod`/`HttpResponse` trio and `MockHttpClient` will be duplicated into `fabro-forgejo` (~150 lines). The human saw "some duplicated shape to maintain" in the option they picked. Deduplicating it into `fabro-http` would touch `fabro-github`'s public surface — against the answer — and only becomes clearly worthwhile when a third forge arrives. On record as a future item, not this one.
  > 
  > **5. No test is blocked by the current shape.** Mock-client unit tests, a twin-forgejo harness (the twin-github pattern exists to copy), spec/router conformance tests, and CLI scenario tests all reach the new behaviour with existing tooling.
  > 
  > **6. Names stay honest.** The plan already renames `GitHubRepoLayout` → `CloneRepoLayout`; `GitHubRepositorySlug` remains accurate for its github.com-only remaining uses; the automation lane legitimately stays GitHub-only per the scoping answer.
  > 
  > Refactorings considered and rejected as unrelated-to-this-change (on record): extracting the HTTP trio to `fabro-http`; splitting `pipeline/pull_request.rs` (2,046 lines) and `server.rs` into smaller modules; the forge-host trait (human-rejected — if ever revived it needs its own goal and conversation).
  > 
  > No refactoring stage has run in this workflow; this is the first and only decision point, and the answer is no.
  > 
  > {"preferred_next_label": "none"}

## Stage: implement
- Status: succeeded
- Handler: agent
- Model: glm-5.3-flash
- Files touched: /workspace/fabro/.env.example, /workspace/fabro/AGENTS.md, /workspace/fabro/Cargo.toml, /workspace/fabro/apps/fabro-web/app/components/pull-request-chip.test.tsx, /workspace/fabro/apps/fabro-web/app/install-api.test.ts, /workspace/fabro/apps/fabro-web/app/install-api.ts, /workspace/fabro/apps/fabro-web/app/install-app.test.tsx, /workspace/fabro/apps/fabro-web/app/install-app.tsx, /workspace/fabro/apps/fabro-web/app/install-config.ts, /workspace/fabro/apps/fabro-web/app/install-flow.test.ts, /workspace/fabro/apps/fabro-web/app/install-flow.ts, /workspace/fabro/docs/public/api-reference/fabro-api.yaml, /workspace/fabro/docs/public/changelog/2026-09-11.mdx, /workspace/fabro/docs/public/core-concepts/forgejo.mdx, /workspace/fabro/docs/public/docs.json, /workspace/fabro/lib/apps/fabro-cli/Cargo.toml, /workspace/fabro/lib/apps/fabro-cli/src/commands/doctor.rs, /workspace/fabro/lib/apps/fabro-cli/src/commands/install.rs, /workspace/fabro/lib/apps/fabro-cli/src/commands/pr/link.rs, /workspace/fabro/lib/apps/fabro-cli/src/commands/run/run_progress/event.rs, /workspace/fabro/lib/apps/fabro-cli/src/commands/run/run_progress/mod.rs, /workspace/fabro/lib/apps/fabro-cli/src/commands/run/runner.rs, /workspace/fabro/lib/apps/fabro-cli/src/commands/runs/mod.rs, /workspace/fabro/lib/apps/fabro-cli/src/shared/forgejo.rs, /workspace/fabro/lib/apps/fabro-cli/src/shared/mod.rs, /workspace/fabro/lib/apps/fabro-cli/tests/it/cmd/attach.rs, /workspace/fabro/lib/apps/fabro-cli/tests/it/cmd/inspect.rs, /workspace/fabro/lib/apps/fabro-cli/tests/it/cmd/pr_create.rs, /workspace/fabro/lib/apps/fabro-cli/tests/it/cmd/pr_link.rs, /workspace/fabro/lib/apps/fabro-cli/tests/it/cmd/pr_view.rs, /workspace/fabro/lib/apps/fabro-server/Cargo.toml, /workspace/fabro/lib/apps/fabro-server/src/diagnostics.rs, /workspace/fabro/lib/apps/fabro-server/src/forgejo_webhooks.rs, /workspace/fabro/lib/apps/fabro-server/src/install.rs, /workspace/fabro/lib/apps/fabro-server/src/lib.rs, /workspace/fabro/lib/apps/fabro-server/src/run_manifest.rs, /workspace/fabro/lib/apps/fabro-server/src/serve.rs, /workspace/fabro/lib/apps/fabro-server/src/server.rs, /workspace/fabro/lib/apps/fabro-server/src/server/handler/pull_requests.rs, /workspace/fabro/lib/apps/fabro-server/src/server/handler/system.rs, /workspace/fabro/lib/apps/fabro-server/src/server/pull_request_supervisor.rs, /workspace/fabro/lib/apps/fabro-server/src/server/tests.rs, /workspace/fabro/lib/components/fabro-agent/tests/it/docker_shell.rs, /workspace/fabro/lib/components/fabro-forgejo/Cargo.toml, /workspace/fabro/lib/components/fabro-forgejo/src/lib.rs, /workspace/fabro/lib/components/fabro-forgejo/src/tests.rs, /workspace/fabro/lib/components/fabro-forgejo/src/tests_mock.rs, /workspace/fabro/lib/components/fabro-install/src/lib.rs, /workspace/fabro/lib/components/fabro-sandbox/Cargo.toml, /workspace/fabro/lib/components/fabro-sandbox/src/clone_source.rs, /workspace/fabro/lib/components/fabro-sandbox/src/daytona/mod.rs, /workspace/fabro/lib/components/fabro-sandbox/src/docker.rs, /workspace/fabro/lib/components/fabro-sandbox/src/lib.rs, /workspace/fabro/lib/components/fabro-sandbox/src/provider.rs, /workspace/fabro/lib/components/fabro-sandbox/src/provider/daytona.rs, /workspace/fabro/lib/components/fabro-sandbox/src/push_credentials.rs, /workspace/fabro/lib/components/fabro-sandbox/src/sandbox_spec.rs, /workspace/fabro/lib/components/fabro-sandbox/tests/daytona_streaming_live.rs, /workspace/fabro/lib/components/fabro-store/src/run_state.rs, /workspace/fabro/lib/components/fabro-workflow/Cargo.toml, /workspace/fabro/lib/components/fabro-workflow/src/event/convert.rs, /workspace/fabro/lib/components/fabro-workflow/src/event/events.rs, /workspace/fabro/lib/components/fabro-workflow/src/git_bridge.rs, /workspace/fabro/lib/components/fabro-workflow/src/handler/manager_loop.rs, /workspace/fabro/lib/components/fabro-workflow/src/operations/start.rs, /workspace/fabro/lib/components/fabro-workflow/src/pipeline/initialize.rs, /workspace/fabro/lib/components/fabro-workflow/src/pipeline/mod.rs, /workspace/fabro/lib/components/fabro-workflow/src/pipeline/publish.rs, /workspace/fabro/lib/components/fabro-workflow/src/pipeline/pull_request.rs, /workspace/fabro/lib/components/fabro-workflow/src/pipeline/types.rs, /workspace/fabro/lib/components/fabro-workflow/src/pull_request.rs, /workspace/fabro/lib/components/fabro-workflow/src/run_metadata.rs, /workspace/fabro/lib/components/fabro-workflow/src/run_options.rs, /workspace/fabro/lib/components/fabro-workflow/src/services.rs, /workspace/fabro/lib/components/fabro-workflow/src/test_support.rs, /workspace/fabro/lib/foundation/fabro-api/tests/pull_request_round_trip.rs, /workspace/fabro/lib/foundation/fabro-api/tests/run_integrations_round_trip.rs, /workspace/fabro/lib/foundation/fabro-api/tests/run_summary_round_trip.rs, /workspace/fabro/lib/foundation/fabro-config/src/layers/run.rs, /workspace/fabro/lib/foundation/fabro-config/src/layers/server.rs, /workspace/fabro/lib/foundation/fabro-config/src/resolve/run.rs, /workspace/fabro/lib/foundation/fabro-config/src/resolve/server.rs, /workspace/fabro/lib/foundation/fabro-config/src/tests/resolve_server.rs, /workspace/fabro/lib/foundation/fabro-static/src/env_vars.rs, /workspace/fabro/lib/foundation/fabro-static/src/secret_registry.rs, /workspace/fabro/lib/foundation/fabro-test/Cargo.toml, /workspace/fabro/lib/foundation/fabro-test/src/lib.rs, /workspace/fabro/lib/foundation/fabro-types/src/pull_request.rs, /workspace/fabro/lib/foundation/fabro-types/src/run_event/misc.rs, /workspace/fabro/lib/foundation/fabro-types/src/run_event/mod.rs, /workspace/fabro/lib/foundation/fabro-types/src/settings/run.rs, /workspace/fabro/lib/foundation/fabro-types/src/settings/server.rs, /workspace/fabro/lib/foundation/fabro-types/src/system_integrations.rs, /workspace/fabro/test/twin/forgejo/Cargo.toml, /workspace/fabro/test/twin/forgejo/src/auth.rs, /workspace/fabro/test/twin/forgejo/src/handlers/branches.rs, /workspace/fabro/test/twin/forgejo/src/handlers/git.rs, /workspace/fabro/test/twin/forgejo/src/handlers/mod.rs, /workspace/fabro/test/twin/forgejo/src/handlers/pulls.rs, /workspace/fabro/test/twin/forgejo/src/handlers/repos.rs, /workspace/fabro/test/twin/forgejo/src/handlers/users.rs, /workspace/fabro/test/twin/forgejo/src/lib.rs, /workspace/fabro/test/twin/forgejo/src/server.rs, /workspace/fabro/test/twin/forgejo/src/state.rs, /workspace/fabro/test/twin/forgejo/src/test_support.rs, /workspace/fabro/test/twin/forgejo/tests/client_ops.rs
- Response:
  > The Forgejo integration is complete. All phases of the plan were implemented, verified with a green workspace build, 2,200+ passing tests, clean `fmt`, and clean `clippy -D warnings` on the pinned nightly.
  > 
  > ## What changed, file by file
  > 
  > ### Phase 0 — Foundation
  > - **`lib/components/fabro-forgejo/` (new crate)** — Forgejo (Gitea-lineage) REST v1 client: `ForgejoInstance` (validated HTTPS base URL, subpath support, serde-validated), `ForgejoCredentials` (redacted Debug), `ForgejoConfig`, `ForgejoContext`, URL helpers (`parse_forgejo_owner_repo`, `is_forgejo_origin`, `normalize_forgejo_origin_url`, `embed_token_in_url` on `DisplaySafeUrl`), instance-scoped git credential-helper constants, and ops: `get_authenticated_user`, `get_repository`, `branch_head_sha` (Gitea `commit.id`), `find/create/get/merge/close_pull_request`, `enable_auto_merge` (`Do` + `merge_when_checks_succeed`), returning GitHub-parity `NotFound`/`Other` errors. 31 unit tests incl. the Forgejo→`PullRequestGithubDetail` projection fixture; `ForgejoInstance::new_allowing_http` added later under `test-support` for loopback twins.
  > - **`fabro-static`** — `FORGEJO_TOKEN`/`FORGEJO_URL`/`FORGEJO_WEBHOOK_SECRET` env vars; the two secrets in the vault scrub registry.
  > - **`fabro-types`** — `ForgejoIntegrationSettings` (enabled/url), `RunIntegrationsForgejoSettings { token }` (serde-skip when default so old payloads stay byte-identical), `PullRequestLink.forge: Option<String>` with `from_forgejo_url` (accepts `/pulls/` and `/pull/`, instance-host-checked, omitted from JSON when None), `PullRequestCreatedProps.forge`, `IntegrationProvider::Forgejo`.
  > - **`fabro-config`** — `ForgejoIntegrationLayer`/`RunIntegrationsForgejoLayer` + resolvers.
  > 
  > ### Phase 1 — Sandbox
  > - **`fabro-sandbox`** — `CloneDecision::Forge`, `decide_clone`/`clone_repo_layout` (renamed from `GitHubRepoLayout`) take the instance and dispatch; `SandboxSpec`/`SandboxCreateSpec` gain `forgejo: Option<ForgejoSandboxConfig>` (alias of `ForgejoConfig`); Docker clone embeds the static PAT for Forge origins; Daytona uses PAT as basic-auth for clone + one-time post-clone set-url; push-credential refresh source stays GitHub-only (PATs don't expire).
  > 
  > ### Phase 2 — Workflow
  > - **`fabro-workflow`** — `RunOptions`/`StartServices`/`PublishOptions` carry `ForgejoConfig`; `SandboxEnvSpec.forgejo_requested`; `build_sandbox_env` resolves `FORGEJO_TOKEN` only when the origin is on the instance (fails closed otherwise) and injects instance-scoped git-bridge entries (`git_bridge::merge_forgejo_bridge_env`); `EngineServices`/`WorkflowToolEnvProvider` expose the token as `FORGEJO_TOKEN`; `PullRequestHost` enum dispatches all five PR touch points with a normalized `ExistingPullRequest`; publish picks the host; PR-created events carry `forge`.
  > 
  > ### Phase 3 — Server
  > - **`fabro-server`** — `forgejo_webhooks.rs` (HMAC verify accepting `X-Forgejo-Signature`/`X-Gitea-Signature`/`X-Hub-Signature-256`, inert verify-and-log handler, route mounted only with a secret); startup-resolved `state.forgejo`; `forgejo_config()` resolver; `StartServices.forgejo` threading; preflight Forgejo repository-access check + informational Forgejo-token check; supervisor and PR handlers dispatch get/merge/close/create by `link.forge` with `integration_unavailable` when unconfigured; run-link endpoint accepts instance URLs; install endpoints `PUT /install/forgejo/token` + `/test` with settings/vault persistence via `fabro-install::write_forgejo_settings`; system-integrations forgejo status; diagnostics connectivity check; startup enable/disable log.
  > - **`docs/public/api-reference/fabro-api.yaml`** — new paths/schemas (forgejo install, webhook, `ForgejoIntegrationSettings`, `RunIntegrationsForgejoSettings`, `PullRequestLink.forge`, `IntegrationProvider` enum). TS client regenerated.
  > 
  > ### Phase 4–7 — CLI, Web, Twin, Docs
  > - **CLI** — `shared/forgejo.rs` resolver (env + vault), optional install-wizard step with PAT validation, `pr link` forge URL classification with `forgejo #N` labels, doctor connectivity check.
  > - **Web** — regenerated Axios client, `testInstallForgejoToken`/`putInstallForgejoToken` wrappers, optional Forgejo wizard step (URL+PAT+Test+Skip), chip fixture test for instance URLs.
  > - **`test/twin/forgejo` (new)** — Gitea-lineage axum twin with real bare-repo smart-HTTP fixtures; 17 tests drive the real client ops end-to-end.
  > - **Docs** — `docs/public/core-concepts/forgejo.mdx`, changelog `2026-09-11.mdx`, nav entries, `AGENTS.md` (CLAUDE.md symlink) sandbox/architecture wording, `.env.example` live-test vars.
  > 
  > ### Deviations from the plan (and why)
  > - **Mock-client tests live in-module** (`src/tests.rs`), not `tests/integration.rs` — the repo keeps client tests in `#[cfg(test)]` modules.
  > - **Preflight forge access check uses the instance API** (`get_repository` + `branch_head_sha`) instead of dispatching `git ls-remote` with an embedded-token URL — same guarantee, one code path, and tokens never touch argv.
  > - **`PullRequestHost`/`ForgejoConfig` re-exports** were needed at `fabro_workflow::pull_request` for the server, which the plan's threading implied but didn't name.
  > - **Manifest lane unchanged** (devil's-advocate note): `fabro-manifest` already preserves the full `git.origin_url` for non-GitHub origins, which is exactly what the publish-side dispatch consumes — verified rather than modified.


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

Review the change for security problems only.

Look for: injection of any kind, secrets or credentials committed in the diff,
unvalidated input reaching a dangerous sink, authorization checks that are
missing or bypassable, unsafe deserialization, path traversal, and dependencies
added with known problems.

For each finding give the file, what an attacker does, and what they get.
Impact matters more than category — say what actually goes wrong.

If the diff introduces no security problems, say so plainly. Do not pad.
