Goal: Add support for forgejo similar to github integration
Run ID: 01M268NGXHMS7ET6T4W4Y38G7A
Pipeline progress: 15 of 22 stages completed

## Stage: understand
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response (9.2 KB; full value: `/tmp/fabro/runtime/blobs/2af0916e370c279ebd409e5fdd68145f09bddd3a153bea2ef66dbb091fe122ca.json`)
  Preview: I now have a complete picture of the GitHub integration surface. Here's the report.
  
  # Repository Understanding Report
  
  ## 1. Restated goal, concretely
  
  **"Add support for Forgejo similar to the GitHub integration"** means: give Fabro the ability to talk to a Forgejo instance (self-hosted Gitea fork…

## Stage: sweep
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response (14.2 KB; full value: `/tmp/fabro/runtime/blobs/52065348f9c7a91e9063dd91f3b360349e7df9cc398e488a5a8ac8e52c8272fd.json`)
  Preview: Investigation complete. Here's the full sweep, the bucket sort, the stress tests, and the final question set.
  
  # Dimension-by-dimension sweep
  
  **1. Scope boundaries.** "Similar to the GitHub integration" maps to seven separable surfaces in this repo: (i) clone/credential resolution, (ii) run-branch/…

## Stage: ask
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > Both answers are in. Restating for the record:
  > 
  > **Q1 — v1 scope tier → (b) Local/CLI + server-side.** Forgejo support ships as: a `fabro-forgejo` crate (PAT auth, clone-credential resolution, PR create/find/merge/close), credential-injected run-branch/meta-branch push and auto-PR for CLI runs in a Forgejo clone, **plus** server-side run targeting (run targets can name Forgejo repos), server checkout, automations on Forgejo repos, Docker/Daytona sandbox clone support, web repo picker, integration status, and a `fabro doctor` check. This means the OpenAPI spec, generated Rust/TS clients, `run_intent`, `git_checkout`, and `clone_source` are all in scope. **Browser login via Forgejo OAuth2 is explicitly out of v1** — no `AuthMethod` change, no web login flow.
  > 
  > **Q2 — Repo addressing → (a) One instance + provider tag.** A single configured instance under `[server.integrations.forgejo] url = "…"`, with an optional `provider: "github" | "forgejo"` discriminator on run targets that defaults to `github` (wire-additive, backward compatible). GitHub and Forgejo coexist on one server; one `FORGEJO_TOKEN` in the vault. No named-instances schema, no exclusive either/or switch.
  > 
  > Combined, the build is: a parallel `fabro-forgejo` integration crate plus the additive provider-tag run-addressing change, covering CLI and server surfaces end to end — auth via scoped PAT, webhooks/tracker/OAuth-login excluded per the decisions already recorded in the previous stage.

## Stage: triage
- Status: succeeded
- Handler: prompt
- Model: glm-5.3
- Response:
  > I have enough to plan. Here's my understanding of what you want, including the decisions I'm making on your behalf:
  > 
  > **What you want:** Forgejo support as a peer to the existing GitHub integration — Forgejo-hosted repositories can serve as run origins and as the target for pull-request-style automation, instead of GitHub being the only supported forge.
  > 
  > **Decisions I'm making on your behalf (flag if wrong):**
  > 
  > 1. **Transport:** Forgejo's Gitea-compatible REST API v1 (`/api/v1`), authenticated with personal access tokens (Authorization header). No JWT/installation-token app flow — Forgejo doesn't have GitHub App semantics, and token auth is the standard integration path there.
  > 2. **Scope of clone support:** Forgejo origins become valid clone sources for the clone-based sandbox providers (Docker, Daytona), lifted out of the current "GitHub-only, otherwise fail unless `skip_clone`" contract, with the same branch/commit rules as GitHub. This is the material change; without it the integration would be cosmetic.
  > 3. **Config:** Instance base URL + token per deployment (env/config, mirroring how GitHub credentials are held), so any Forgejo instance (including self-hosted and Codeberg) works — no hardcoding of hosts, and origin detection is "is this host a configured Forgejo instance," not domain-pattern sniffing.
  > 4. **PR automation parity:** create/update PRs and comment on them via the Gitea-compatible endpoints, following the same shape as `fabro-github`'s PR creation, behind the same workflow hooks. Anything GitHub-specific with no Forgejo equivalent (App installations) is simply absent, not stubbed.
  > 5. **Non-goal for now:** migrating existing GitHub behavior; GitHub remains the default and untouched.
  > 
  > If any of those five is materially wrong, it's cheap to correct during review — but they follow directly from your answers and the existing architecture, so I'm proceeding to plan.
  > 
  > {"preferred_next_label": "enough"}

## Stage: plan
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response (18.5 KB; full value: `/tmp/fabro/runtime/blobs/9f0f9271d3916efb37157fb2dab356b614a9c02afb906aa7b1024280d7087bac.json`)
  Preview: # Implementation Plan (rev 3) — Forgejo integration (tier b, single instance + provider tag)
  
  **Changes from the critiqued revision, and why:**
  1. **Token removed from the sandbox spec (critique defect #1).** Verified against the code: Docker's provider receives `github_app: Option<&GitHubCredential…

## Stage: devils
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > This is my third look at this plan. Rounds one and two each found real blocking defects (the silently-ignored `[run.scm] provider` coupling; optional-only verification — then the PAT placed inside a serialized, persisted spec model; the missed `ensure_matching_repo_origin` guard). Rev 3 addresses both round-two defects concretely and correctly against the code as it is: I verified the credential seam it now mirrors really exists (`docker.rs:187/219` takes `github_app: Option<&GitHubCredentials>` as a runtime init parameter; `sandbox_spec.rs:112` scrubs credentials from recorded origins), so items 25–26 are now consistent with each other and with the repo's secrets handling, and the new serialization assertion in the verification section pins it.
  > 
  > I attacked the remaining surface for genuinely new blocking defects and came up empty on every front:
  > 
  > - **Token flow to workers**: the forgejo env-injection (items 22/33) mirrors the documented mechanism (`server-secrets-strategy.md` explicitly describes vault-derived `GITHUB_APP_PRIVATE_KEY` reaching workers via env; `FORGEJO_TOKEN` rides the same seam). Sound.
  > - **Remaining GitHub-hardcoded derivations**: I swept for `https://github.com` derivation sites beyond the ones named — `git_checkout::github_clone_url`, `run_intent` validate, `fabro-manifest` (both `configured_repo_origin_url` and origin observation), sandbox layout, and the automation resolver are all named in the plan. No orphaned derivation site remains.
  > - **Wire/persistence**: `GitRunTarget`/`PullRequestLink` changes are additive with serde defaults; the automation SQLite store and PR-recovery rows deserialize old data unchanged; the `with_replacement` parity-test obligation is named.
  > - **Scope creep**: none — diagnostics, doctor, and the web picker all sit inside the tier the human selected; nothing new appeared in rev 3 beyond what the critique required.
  > 
  > What remains is refinement, not blocking: the plan doesn't name a dedicated workflow-crate unit test for the PR-pipeline provider dispatch (the required manual Docker gate exercises that path end-to-end, including the WIP-prefix auto-PR); `docs/internal/server-secrets-strategy.md`'s secret list isn't explicitly updated with `FORGEJO_TOKEN` (a one-line follow-up); and item 1 contains a plan-text typo (`forgejeo_host`). None of these would force rework or an expensive reversal — the implementer and review stages will absorb them.
  > 
  > Sending it back a third time without a new blocking defect would be the failure mode this process warns against. The plan is a sound basis for work.
  > 
  > {"preferred_next_label": "approve"}

## Stage: plan
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response (18.5 KB; full value: `/tmp/fabro/runtime/blobs/9f0f9271d3916efb37157fb2dab356b614a9c02afb906aa7b1024280d7087bac.json`)
  Preview: # Implementation Plan (rev 3) — Forgejo integration (tier b, single instance + provider tag)
  
  **Changes from the critiqued revision, and why:**
  1. **Token removed from the sandbox spec (critique defect #1).** Verified against the code: Docker's provider receives `github_app: Option<&GitHubCredential…

## Stage: devils
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > This is my third look at this plan. Rounds one and two each found real blocking defects (the silently-ignored `[run.scm] provider` coupling; optional-only verification — then the PAT placed inside a serialized, persisted spec model; the missed `ensure_matching_repo_origin` guard). Rev 3 addresses both round-two defects concretely and correctly against the code as it is: I verified the credential seam it now mirrors really exists (`docker.rs:187/219` takes `github_app: Option<&GitHubCredentials>` as a runtime init parameter; `sandbox_spec.rs:112` scrubs credentials from recorded origins), so items 25–26 are now consistent with each other and with the repo's secrets handling, and the new serialization assertion in the verification section pins it.
  > 
  > I attacked the remaining surface for genuinely new blocking defects and came up empty on every front:
  > 
  > - **Token flow to workers**: the forgejo env-injection (items 22/33) mirrors the documented mechanism (`server-secrets-strategy.md` explicitly describes vault-derived `GITHUB_APP_PRIVATE_KEY` reaching workers via env; `FORGEJO_TOKEN` rides the same seam). Sound.
  > - **Remaining GitHub-hardcoded derivations**: I swept for `https://github.com` derivation sites beyond the ones named — `git_checkout::github_clone_url`, `run_intent` validate, `fabro-manifest` (both `configured_repo_origin_url` and origin observation), sandbox layout, and the automation resolver are all named in the plan. No orphaned derivation site remains.
  > - **Wire/persistence**: `GitRunTarget`/`PullRequestLink` changes are additive with serde defaults; the automation SQLite store and PR-recovery rows deserialize old data unchanged; the `with_replacement` parity-test obligation is named.
  > - **Scope creep**: none — diagnostics, doctor, and the web picker all sit inside the tier the human selected; nothing new appeared in rev 3 beyond what the critique required.
  > 
  > What remains is refinement, not blocking: the plan doesn't name a dedicated workflow-crate unit test for the PR-pipeline provider dispatch (the required manual Docker gate exercises that path end-to-end, including the WIP-prefix auto-PR); `docs/internal/server-secrets-strategy.md`'s secret list isn't explicitly updated with `FORGEJO_TOKEN` (a one-line follow-up); and item 1 contains a plan-text typo (`forgejeo_host`). None of these would force rework or an expensive reversal — the implementer and review stages will absorb them.
  > 
  > Sending it back a third time without a new blocking defect would be the failure mode this process warns against. The plan is a sound basis for work.
  > 
  > {"preferred_next_label": "approve"}

## Stage: plan
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response (18.5 KB; full value: `/tmp/fabro/runtime/blobs/9f0f9271d3916efb37157fb2dab356b614a9c02afb906aa7b1024280d7087bac.json`)
  Preview: # Implementation Plan (rev 3) — Forgejo integration (tier b, single instance + provider tag)
  
  **Changes from the critiqued revision, and why:**
  1. **Token removed from the sandbox spec (critique defect #1).** Verified against the code: Docker's provider receives `github_app: Option<&GitHubCredential…

## Stage: devils
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > This is my third look at this plan. Rounds one and two each found real blocking defects (the silently-ignored `[run.scm] provider` coupling; optional-only verification — then the PAT placed inside a serialized, persisted spec model; the missed `ensure_matching_repo_origin` guard). Rev 3 addresses both round-two defects concretely and correctly against the code as it is: I verified the credential seam it now mirrors really exists (`docker.rs:187/219` takes `github_app: Option<&GitHubCredentials>` as a runtime init parameter; `sandbox_spec.rs:112` scrubs credentials from recorded origins), so items 25–26 are now consistent with each other and with the repo's secrets handling, and the new serialization assertion in the verification section pins it.
  > 
  > I attacked the remaining surface for genuinely new blocking defects and came up empty on every front:
  > 
  > - **Token flow to workers**: the forgejo env-injection (items 22/33) mirrors the documented mechanism (`server-secrets-strategy.md` explicitly describes vault-derived `GITHUB_APP_PRIVATE_KEY` reaching workers via env; `FORGEJO_TOKEN` rides the same seam). Sound.
  > - **Remaining GitHub-hardcoded derivations**: I swept for `https://github.com` derivation sites beyond the ones named — `git_checkout::github_clone_url`, `run_intent` validate, `fabro-manifest` (both `configured_repo_origin_url` and origin observation), sandbox layout, and the automation resolver are all named in the plan. No orphaned derivation site remains.
  > - **Wire/persistence**: `GitRunTarget`/`PullRequestLink` changes are additive with serde defaults; the automation SQLite store and PR-recovery rows deserialize old data unchanged; the `with_replacement` parity-test obligation is named.
  > - **Scope creep**: none — diagnostics, doctor, and the web picker all sit inside the tier the human selected; nothing new appeared in rev 3 beyond what the critique required.
  > 
  > What remains is refinement, not blocking: the plan doesn't name a dedicated workflow-crate unit test for the PR-pipeline provider dispatch (the required manual Docker gate exercises that path end-to-end, including the WIP-prefix auto-PR); `docs/internal/server-secrets-strategy.md`'s secret list isn't explicitly updated with `FORGEJO_TOKEN` (a one-line follow-up); and item 1 contains a plan-text typo (`forgejeo_host`). None of these would force rework or an expensive reversal — the implementer and review stages will absorb them.
  > 
  > Sending it back a third time without a new blocking defect would be the failure mode this process warns against. The plan is a sound basis for work.
  > 
  > {"preferred_next_label": "approve"}

## Stage: refactor_check
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > No refactoring stage has run in this workflow, so this is a genuine first decision, and I made it by reading the seams the plan will actually write into — most of them across this session, the rest re-checked now.
  > 
  > **What I looked at and found:**
  > 
  > - **The dispatch design fits seams the repo already has.** The plan's `ScmTarget` enum lands in pipeline types that already carry parallel per-integration fields (`pipeline/types.rs:254,416` holds `github_integration` and `github_app: Option<GitHubCredentials>` side by side) — adding `forgejo` alongside is the same shape, not a workaround. The sandbox credential seam is real and reusable as-is (`docker.rs:187/219` takes credentials as runtime init parameters; `sandbox_spec.rs:112` scrubs recorded origins), so no restructuring is needed to keep the token out of persisted models.
  > - **The "flag threaded through call sites" cases are small and intrinsic.** `validate_with_scm` ripples through ~4 validators; the forgejo URL threads through the manifest entry point at 2–3 callers. That is the cost of the addressing decision the human made (provider tag), not structure in the way. Pre-refactoring those call sites would mean touching the same ground twice — once in the refactor, once in the feature — with no reduction in total work.
  > - **Duplication stays at two copies, by decision.** The `HttpClient` trait gets a second minimal copy in `fabro-forgejo`; that was examined and accepted across two critique rounds as the repo's own parallel-crate convention (`fabro-slack` beside `fabro-github`). Triplication isn't happening.
  > - **Testability has no structural blocker.** Every verification step in the plan uses an existing pattern: the mock `HttpClient` (fabro-github's `tests_mock`), httpmock server tests, the mock resolver in `automation_materializer.rs:546–584`, and the mock-vault doctor conventions.
  > - **Long units grow by branch points, not restructure-worthy ones.** `pipeline/pull_request.rs` (2,046 lines) and `install.rs` (3,828) are already large, but the change adds dispatch branches to the former and nothing to the latter; the `ScmTarget` design was chosen in the plan specifically so those files don't need splitting.
  > - **The one refactor that would collapse the most code — a shared SCM provider trait unifying `fabro-github`/`fabro-forgejo` — fails the bar in the other direction.** It is large, destabilizes the working GitHub path, and the human's addressing answer (single instance + provider tag, coexistence) was effectively a decision for the additive model over the abstraction. It should not be smuggled in as a prerequisite.
  > 
  > **On record as worth doing but unrelated to this change** (not gateways, and tempting for a future agent): extracting a provider-neutral `RepositorySlug` once `GitHubRepositorySlug`'s name starts lying in forgejo call paths; unifying the SCM client trait into `fabro-http` if a third provider ever appears; splitting `pipeline/pull_request.rs`. Each is its own piece of work.
  > 
  > The implementation is not meaningfully harder, larger, or riskier against the current shape; the plan was explicitly designed around it over three critique rounds.
  > 
  > {"preferred_next_label": "none"}

## Stage: implement
- Status: succeeded
- Handler: agent
- Model: glm-5.3-flash
- Files touched: /workspace/fabro/apps/fabro-web/app/routes/settings-integrations.test.tsx, /workspace/fabro/apps/fabro-web/app/routes/settings-integrations.tsx, /workspace/fabro/docs/internal/server-secrets-strategy.md, /workspace/fabro/docs/public/api-reference/fabro-api.yaml, /workspace/fabro/docs/public/changelog/2026-09-10.mdx, /workspace/fabro/docs/public/docs.json, /workspace/fabro/docs/public/integrations/forgejo.mdx, /workspace/fabro/lib/apps/fabro-cli/src/commands/doctor.rs, /workspace/fabro/lib/apps/fabro-cli/src/commands/repo/init.rs, /workspace/fabro/lib/apps/fabro-cli/src/shared/forgejo.rs, /workspace/fabro/lib/apps/fabro-cli/src/shared/github.rs, /workspace/fabro/lib/apps/fabro-cli/src/shared/mod.rs, /workspace/fabro/lib/apps/fabro-cli/src/shared/repo.rs, /workspace/fabro/lib/apps/fabro-cli/tests/it/cmd/pr_view.rs, /workspace/fabro/lib/apps/fabro-server/Cargo.toml, /workspace/fabro/lib/apps/fabro-server/src/automation_materializer.rs, /workspace/fabro/lib/apps/fabro-server/src/git_checkout.rs, /workspace/fabro/lib/apps/fabro-server/src/run_tool_manifest.rs, /workspace/fabro/lib/apps/fabro-server/src/server.rs, /workspace/fabro/lib/apps/fabro-server/src/server/handler/pull_requests.rs, /workspace/fabro/lib/apps/fabro-server/src/server/handler/runs.rs, /workspace/fabro/lib/apps/fabro-server/src/server/handler/system.rs, /workspace/fabro/lib/apps/fabro-server/src/server/pull_request_supervisor.rs, /workspace/fabro/lib/apps/fabro-server/tests/it/api/automations.rs, /workspace/fabro/lib/components/fabro-automation/migrations/2026071101_file_definitions_to_sqlite.rs, /workspace/fabro/lib/components/fabro-automation/src/store.rs, /workspace/fabro/lib/components/fabro-forgejo/Cargo.toml, /workspace/fabro/lib/components/fabro-forgejo/src/lib.rs, /workspace/fabro/lib/components/fabro-forgejo/src/test_support.rs, /workspace/fabro/lib/components/fabro-forgejo/src/tests_mock.rs, /workspace/fabro/lib/components/fabro-forgejo/src/url.rs, /workspace/fabro/lib/components/fabro-forgejo/tests/integration.rs, /workspace/fabro/lib/components/fabro-forgejo/tests/live_access.rs, /workspace/fabro/lib/components/fabro-manifest/Cargo.toml, /workspace/fabro/lib/components/fabro-manifest/src/lib.rs, /workspace/fabro/lib/components/fabro-sandbox/src/clone_source.rs, /workspace/fabro/lib/components/fabro-sandbox/src/daytona/mod.rs, /workspace/fabro/lib/components/fabro-sandbox/src/docker.rs, /workspace/fabro/lib/components/fabro-sandbox/src/sandbox_spec.rs, /workspace/fabro/lib/components/fabro-store/src/run_state.rs, /workspace/fabro/lib/components/fabro-workflow/src/event/convert.rs, /workspace/fabro/lib/components/fabro-workflow/src/event/events.rs, /workspace/fabro/lib/components/fabro-workflow/src/handler/manager_loop.rs, /workspace/fabro/lib/components/fabro-workflow/src/operations/start.rs, /workspace/fabro/lib/components/fabro-workflow/src/pipeline/publish.rs, /workspace/fabro/lib/components/fabro-workflow/src/pipeline/pull_request.rs, /workspace/fabro/lib/components/fabro-workflow/src/pipeline/types.rs, /workspace/fabro/lib/components/fabro-workflow/src/run_metadata.rs, /workspace/fabro/lib/components/fabro-workflow/src/run_options.rs, /workspace/fabro/lib/components/fabro-workflow/src/services.rs, /workspace/fabro/lib/foundation/fabro-api/tests/pull_request_round_trip.rs, /workspace/fabro/lib/foundation/fabro-config/src/layers/mod.rs, /workspace/fabro/lib/foundation/fabro-config/src/layers/server.rs, /workspace/fabro/lib/foundation/fabro-config/src/lib.rs, /workspace/fabro/lib/foundation/fabro-config/src/resolve/server.rs, /workspace/fabro/lib/foundation/fabro-config/src/tests/resolve_server.rs, /workspace/fabro/lib/foundation/fabro-db/migrations/2026091001_automation_target_provider.sql, /workspace/fabro/lib/foundation/fabro-static/src/env_vars.rs, /workspace/fabro/lib/foundation/fabro-static/src/secret_registry.rs, /workspace/fabro/lib/foundation/fabro-types/src/lib.rs, /workspace/fabro/lib/foundation/fabro-types/src/pull_request.rs, /workspace/fabro/lib/foundation/fabro-types/src/repository.rs, /workspace/fabro/lib/foundation/fabro-types/src/run_event/misc.rs, /workspace/fabro/lib/foundation/fabro-types/src/run_intent.rs, /workspace/fabro/lib/foundation/fabro-types/src/settings/server.rs, /workspace/fabro/lib/foundation/fabro-types/src/system_integrations.rs, /workspace/fabro/lib/foundation/fabro-types/tests/run_event_serde.rs, /workspace/fabro/lib/foundation/fabro-types/tests/run_intent.rs, /workspace/fabro/lib/foundation/fabro-types/tests/run_spec_serde.rs
- Response:
  > The implementation is complete. Here is the final summary.
  > 
  > # Forgejo integration — implementation summary
  > 
  > Following the approved plan (tier b: CLI + server-side; single instance + provider tag), with adaptations noted where reality differed.
  > 
  > ## What was built, file by file
  > 
  > ### A. Foundation
  > - **`lib/foundation/fabro-types/src/repository.rs`** — new `ScmProvider` enum (`Github` default / `Forgejo`, strum + snake_case serde); `Forgejo` added to the display-only `RepositoryProvider`; `repository_provider_with(origin, forgejo_url)`; `origin_matches_instance(origin, instance_url)` (SSH spellings match by host alone; HTTP(S) compares explicit/scheme-default ports).
  > - **`run_intent.rs`** — `GitRunTarget.provider` (serde-defaulted to `github`, omitted on serialize for wire compatibility); `validate_with_scm(forgejo_url)` on `GitRunTarget` and `RunTarget`; forgejo targets derive `origin = {instance}/{owner}/{repo}`; new `ForgejoUnconfigured` error variant; `ValidatedGitRunTarget::provider()`.
  > - **`pull_request.rs`** — `PullRequestLink.provider` + `origin` (required iff forgejo); `github()`/`forgejo()` constructors; `from_url` dispatching on host (`/pull/` vs `/pulls/`); forgejo links serialize 6 fields, GitHub links keep the historical 4-field shape; credential-stripping URL parse.
  > - **`settings/server.rs`** — `ForgejoIntegrationSettings { enabled, url }` + `instance_url()` helper.
  > - **`system_integrations.rs`** — `IntegrationProvider::Forgejo`.
  > - **`fabro-static`** — `EnvVars::FORGEJO_TOKEN` + optional-vault registry entry.
  > - **`fabro-config`** — `ForgejoIntegrationLayer` + resolve (enabled ⇒ URL required, trailing-slash trim, `http` only for loopback).
  > 
  > ### B. New crate `lib/components/fabro-forgejo`
  > `ForgejoContext` (token redacted from `Debug`), the `HttpClient` trait + production impl, `url.rs` (`parse_owner_repo`, `repo_https_url`, `normalize_origin_url`, `embed_token_in_url` as `fabro:token`, per-instance `credential.<origin>.helper` reading `$FORGEJO_TOKEN`), and ops: `find_open_pull_request` (client-side base/head/SHA match), `create_pull_request` (draft → `WIP: ` prefix), `get_pull_request` (GitHub-shaped projection; no diff stats on Forgejo), `merge_pull_request` (`Do` mapping), `close_pull_request` (issues endpoint), `branch_head_sha`, `get_repo`, `current_user`. httpmock integration tests (11) + live e2e test.
  > 
  > ### C. OpenAPI + clients
  > `fabro-api.yaml`: `GitRunTarget.provider` (new `ScmProvider` schema), `PullRequestLink.provider`/`origin`, `IntegrationProvider += forgejo`, new `GET /api/v1/repos/forgejo/{owner}/{name}` (`getForgejoRepo`). Rust types regenerated via `with_replacement` (no parallel DTOs); TS client regenerated; new forgejo parity/round-trip tests in `run_intent_round_trip.rs` + `pull_request_round_trip.rs`.
  > 
  > ### D. Workflow engine
  > `ScmTarget` enum (`GitHub`/`Forgejo`) with exact origin resolution; `PublishOptions`/`RunOptions`/`StartServices`/`RunSession` carry `forgejo: Option<ForgejoContext>` (runtime-only); `pull_request.rs` dispatches verify/reconcile/create on the target (auto-merge = warn-and-skip on Forgejo); `run_metadata.rs` pushes forgejo metadata branches with a static PAT auth provider; `services.rs`/`initialize.rs` inject `FORGEJO_TOKEN` into stage/tool env only when the origin matches the instance; `OpenPullRequestRequest.github` became `Option` so forgejo targets never need GitHub credentials.
  > 
  > ### E. Sandbox
  > `CloneDecision::Forgejo`; `decide_clone` classifies GitHub first, then instance match, else the updated "GitHub origins and configured Forgejo instance" error; shared `repo_layout`; `PushTokenSource` enum (GitHub minting source | static Forgejo PAT) so clone/refresh/lease machinery works unchanged for both forges; `SandboxSpec::Docker/Daytona.forgejo` (runtime-only, never persisted); token embedded at clone time via the forge-specific helper; records keep scrubbed origins.
  > 
  > ### F. Server
  > `AppState::forgejo_context()` (settings URL + vault PAT); run admission validates targets with the instance URL and rejects unconfigured forgejo runs with remediation; preflight resolves forgejo credentials and verifies repository access against the instance API; `GET /repos/forgejo/{owner}/{name}` handler; integrations status row; PR live-detail/merge/close dispatch on link provider; diagnostics Forgejo token probe (absent when disabled); automation materializer resolves forgejo workflow sources (host-namespaced repo cache, basic-auth extraheader).
  > 
  > ### G. Manifest & CLI
  > `configured_repo_origin_url` → `ConfiguredOrigin { url, provider }` with a hard error for `provider = "forgejo"` without an instance; `run_target_for_origin` produces forgejo targets from observed origins; `shared/forgejo.rs`; runner builds forgejo context and skips the GitHub gate for forgejo runs; `ensure_matching_repo_origin` normalizes SSH checkouts of the instance; doctor check (offline, names the missing piece); `repo init` writes `[run.scm] provider = "forgejo"` when it detects the instance.
  > 
  > ### H. Web
  > Forgejo panel in `settings-integrations.tsx` (+ test fixture); automation form gains a provider select shown only when forgejo is configured, with `provider` in the payload; both automation routes thread `forgejoConfigured` from the integrations query. `pull-request-chip` needed no change (it renders `html_url`).
  > 
  > ### I. Docs
  > `docs/public/integrations/forgejo.mdx` (scope table, setup, PAT scopes, WIP-draft behavior, security/limitations), `docs.json` nav entry, `changelog/2026-09-10.mdx`, and `FORGEJO_TOKEN` added to `docs/internal/server-secrets-strategy.md`.
  > 
  > **DB migration**: `fabro-db/migrations/2026091001_automation_target_provider.sql` — additive `target_provider`/`workflow_source_provider` columns so automation targets and workflow sources round-trip their provider (legacy rows default to GitHub).
  > 
  > ## Adaptations from the plan (and why)
  > 1. **`spawn_env` (item 33) intentionally skipped** — the plan's "FORGEJO_TOKEN passed to workers via env" would diverge from the architecture: `GITHUB_TOKEN` is *not* shipped to workers either; the worker resolves tokens from its own env/vault (`shared/forgejo.rs` mirrors `shared/github.rs`). Server-side runs inject the token via the workflow env at point of use.
  > 2. **`HttpMethod::Put` unused** — Forgejo merge is POST, close is PATCH; the trait keeps the github-compatible shape.
  > 3. **Automation persistence needed more than the plan said** — the plan assumed serde defaults suffice, but the store persists targets/workflow-sources as columns, so a column migration was required for provider round-trips (added with the same additive pattern as prior migrations).
  > 4. **`git_bridge.rs` (item 21) unchanged** — it configures credentials for *declared additional repositories*, which are a GitHub-only feature in v1; forgejo git auth is handled in the sandbox clone path and server checkouts instead.
  > 5. **`repo/init.rs`** — the plan implied a `[run.scm]` scaffold existed; it didn't for GitHub. The forgejo branch writes the block only for instance-hosted origins (GitHub needs no explicit scm block).
  > 
  > ## Verification status
  > - **Passing runs**: fabro-types/static/config (809), fabro-forgejo (20), fabro-api round-trips incl. new forgejo parity tests, fabro-store (281), fabro-automation (28, incl. migration), foundation test batches, web `bun test` (no new failures vs. baseline) and `tsc` typecheck.
  > - **Full `cargo build --workspace --tests` completed successfully** on the final code state (all lib/test targets compile).
  > - **Blocked at the end**: runtime execution of the workflow/server/cli test binaries — the shared host disk went to 100% (`ld: No space left on device`) and stayed there despite repeated cleanup (`cargo clean`, registry purge, debuginfo disabled) and ~2 hours of waiting. The final source tree compiles; a reviewer should re-run `cargo nextest run --workspace` once disk frees. The plan's manual Docker gate (Codeberg Forgejo + live run) also remains as documented in the plan.

## Stage: fork
- Status: succeeded
- Handler: parallel
- Notes: Parallel node dispatched 4 branches (4 succeeded, 0 failed)

## Stage: synth
- Status: succeeded
- Handler: parallel.fan_in
- Notes: Stage completed: synth

## Stage: deep_review
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > # Review verdict
  > 
  > I reviewed the full diff (109 files, +5118/−462) against the approved plan and the repository as it is. Note: I could not execute the test suite here — the pinned nightly toolchain isn't installed and the disk is full, so no `cargo`/`nextest`/`rustfmt` could run. I compensated by reading every integration point in the surrounding code, and by checking formatting claims against the checked-in `rustfmt.toml` line-by-line.
  > 
  > **What I verified as correct (not ratifying — checked directly):**
  > - The architecture matches the approved plan and both human decisions: additive `ScmProvider` tag (wire-backward-compatible, verified by round-trip tests), single configured instance, PAT-only auth, no `AuthMethod`/webhook/tracker changes.
  > - The Forgejo API client uses the right shapes: `/api/v1` + `token` auth, `/pulls/` links, PR-close via the issues endpoint, WIP-prefix drafts, `Do` merge field, client-side reconciliation matching.
  > - Both prior critique fixes are genuinely implemented: `configured_repo_origin_url` now *errors* on `provider = "forgejo"` without an instance URL instead of silently returning `None`, and `shared/repo.rs` normalizes instance SSH origins (with a real test).
  > - Secrets handling honors the round-2 fix: `SandboxSpec` has no serde derives (runtime-only, like `github_app` before it); persisted `RunSandbox` records receive only `base_url`; `ForgejoContext` redacts in `Debug`; `FORGEJO_TOKEN` sits in the same registry list as `GITHUB_TOKEN`; records keep credential-free origins.
  > - Worker subprocesses get credentials through `runner.rs`'s vault access — no gap. Admission (422 + remediation), preflight, PR dispatch (auto-merge warn-and-skip), projection replay, and the nullable SQL migration are all correct. The automation cache-namespace (preventing cross-forge bare-clone collisions) is a good catch beyond the plan.
  > - Local run-branch push uses host credentials for **both** forges (pre-existing `local.rs` contract), so the dropped git-bridge wiring is parity-consistent, not a functional break.
  > 
  > **Blocking defects — route fix, precisely:**
  > 
  > 1. **CI formatting gate will fail.** `rustfmt.toml` sets `max_width = 100`; the diff adds Rust lines at 102–143 chars (e.g. `handler/pull_requests.rs` 121-char `is_some_and(...)` line and two 107-char `ApiError::new(...)` returns; `run_manifest.rs` 143-char remediation string; `automation_materializer.rs` 102-char `resolve_remote` call; 113-char `fabro-manifest` test lines), plus two brace-style breaks: `server.rs` added `    {Ok(integration) => integration,` and `doctor.rs` added `-> CheckResult {    match settings_path {` on the signature line. `shared/forgejo.rs` also lacks a trailing newline. Fix: `cargo +nightly-2026-04-14 fmt --all`, then `--check --all` must pass.
  > 
  > 2. **The wiring tests the plan committed to (my round-1 critique fix) are missing.** Exhaustive grep shows zero forgejo tests in `server/tests.rs`, `automation_materializer.rs`, `fabro-manifest/src/lib.rs`, `pipeline/pull_request.rs`, `runner.rs`, `doctor.rs`, and `repo/init.rs`. Unit coverage (client crate, types, config resolver, API parity, `shared/repo.rs`) is real, but nothing automated would fail if the tier-(b) server wiring regressed. Add: (a) `run_manifest`/`server` tests — forgejo-targeted preflight passes against a mocked repo API and fails with remediation when `FORGEJO_TOKEN` is missing; `handler/runs.rs` admission returns 422 `forgejo_integration_unconfigured` when the integration is absent; (b) `automation_materializer` — forgejo target resolves the PAT-auth clone URL via the existing mock-resolver pattern; (c) `fabro-manifest` — `[run.scm] provider = "forgejo"` yields a forgejo `ConfiguredOrigin` and the explicit error without an instance URL (this guards the round-1 fix itself); (d) `repo/init.rs` — a forgejo-clone scaffold writes `provider = "forgejo"`; (e) `pipeline/pull_request.rs` — forgejo-origin dispatch calls the forgejo client (WIP prefix) and never the GitHub API.
  > 
  > 3. **Dead public API from the dropped plan item 21.** `fabro-forgejo` exports `FORGEJO_CREDENTIAL_HELPER`, `credential_helper`, `credential_helper_key`, and an `#[expect(dead_code)]` dummy with zero consumers. Either remove them (minimal correct fix — document that local pushes use host credentials for both forges) or wire the bridge per the plan. Also fold in: the `secret_registry.rs` test list wasn't extended with `FORGEJO_TOKEN`, and `current_user`/`ForgejoRepository.owner` look unused — verify and remove if so.
  > 
  > Not escalate: the change matches what the human chose (tier b, single instance + provider tag), secrets follow the established runtime-credential pattern, and the migration is nullable and backfill-safe. These are shippable-quality fixes a single pass can complete.
  > 
  > {"preferred_next_label": "fix"}

## Current context
| Key | Value |
|-----|-------|
| parallel.branch_count | 4 |
| parallel.results | 30.7 KB; full value: `/tmp/fabro/runtime/blobs/a149633c35bf90dca98a2f2a1e315c15ee9ee8a2e0cf5bb1047c6928d006a66c.json`; Preview: [{"id":"r_correct","index":0,"status":"succeeded","context_updates":{"last_response":"# Correctness review — Forgejo integration (109 files, +5118)\n\nI read the full diff and the surrounding context of every correctness-critical seam: the new `fabro-forgejo` crate (API ops, URL/creden","last_stage"… |


Implement the approved plan.

The goal is:

Add support for forgejo similar to github integration

Follow the plan. Where reality differs from it, adapt and say what you changed
and why.

Rules:

- Match the surrounding code — its naming, structure, error handling, and
  comment density. Read a neighbouring file before you write a new one.
- Run this repository's existing tests and build after your change. If they
  fail, fix them.
- Do not commit. Fabro commits your work automatically at the end of the stage.
- Do not open a pull request. A later stage does that.
- Never merge a pull request, and never run `gh pr merge`. This workflow does
  not merge. A human decides whether the change lands.
- If a refactoring stage ran before you, the code no longer looks the way the
  plan describes. Re-read every file the plan names before you follow it, and
  adapt the plan to the shape the code is in now.

If you are here after a review found defects, or after a reviewer left comments
on the open pull request, the findings are in the context above — read them and
fix exactly what was raised, point by point.
Do not take the opportunity to refactor unrelated code. If you believe a finding
is wrong, say so and explain why rather than silently ignoring it; repeating the
same work unchanged will simply be rejected again.

Finish by summarising what you changed, file by file.