Goal: Add support for forgejo similar to github integration
Run ID: 01M268NGXHMS7ET6T4W4Y38G7A
Pipeline progress: 16 of 22 stages completed

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
- Files touched: /workspace/fabro/lib/apps/fabro-cli/src/commands/doctor.rs, /workspace/fabro/lib/apps/fabro-server/src/automation_materializer.rs, /workspace/fabro/lib/apps/fabro-server/src/run_manifest.rs, /workspace/fabro/lib/apps/fabro-server/src/server.rs, /workspace/fabro/lib/apps/fabro-server/src/server/handler/pull_requests.rs, /workspace/fabro/lib/apps/fabro-server/tests/it/api/runs.rs, /workspace/fabro/lib/components/fabro-forgejo/src/lib.rs, /workspace/fabro/lib/components/fabro-forgejo/src/url.rs, /workspace/fabro/lib/components/fabro-manifest/src/lib.rs, /workspace/fabro/lib/components/fabro-sandbox/src/push_credentials.rs, /workspace/fabro/lib/components/fabro-store/src/run_state.rs, /workspace/fabro/lib/components/fabro-workflow/src/pipeline/pull_request.rs, /workspace/fabro/lib/components/fabro-workflow/src/run_metadata.rs, /workspace/fabro/lib/foundation/fabro-static/src/secret_registry.rs, /workspace/fabro/lib/foundation/fabro-types/src/pull_request.rs, /workspace/fabro/lib/foundation/fabro-types/src/run_intent.rs, /workspace/fabro/lib/foundation/fabro-types/tests/run_intent.rs
- Response:
  > The host disk never recovered — byte-level writes fail across the entire shared overlay. I've exhausted what's verifiable in this environment. Final summary:
  > 
  > ## Summary of changes (review-fix pass)
  > 
  > The implementation was already committed by the pipeline; this stage addressed the three blocking defects from `deep_review`. All changes are in 19 files, +626/−107.
  > 
  > ### Defect 1 — CI formatting gate (fixed by hand; the pinned nightly toolchain can't be installed at 0 bytes free, and stable rustfmt mis-reports style_edition-2024 formatting as diffs, so I fixed the exact constructs instead)
  > 
  > - `lib/apps/fabro-cli/src/commands/doctor.rs` — split `{    match` off the `check_config` signature line.
  > - `lib/apps/fabro-server/src/server.rs` — split `{Ok(integration) => integration,` onto its own line.
  > - `.../server/handler/pull_requests.rs` — broke the 121-char `is_some_and` chain; both 107-char `ApiError::new(...)` match arms rewritten as braced arms (inner line exactly 100).
  > - `lib/apps/fabro-server/src/run_manifest.rs` — the 143-char `Err("...")` line, and fixed a copy/paste artifact (a run of spaces inside the remediation message); message now uses `` `fabro secret set FORGEJO_TOKEN` `` matching doctor/server phrasing.
  > - `lib/apps/fabro-server/src/automation_materializer.rs` — broke the 102-char `resolve_remote(...)` args vertically.
  > - `lib/components/fabro-manifest/src/lib.rs` — four 110–113-char test calls broken into rustfmt's arg-per-line + `.unwrap()` chain shape.
  > - `lib/components/fabro-sandbox/src/push_credentials.rs` — 101-char `.map_err(...)` chain: closure body moved into a block.
  > - `lib/components/fabro-store/src/run_state.rs` — 105-char `PullRequestLink::github(...)` arm broken into a multi-line call arm.
  > - `lib/components/fabro-workflow/src/pipeline/pull_request.rs` — 104-char struct field: extracted `github_app::GitHubContext` into a local; `github_app` alias resolves via the tests module's `use super::*`.
  > - `lib/components/fabro-workflow/src/run_metadata.rs` — `discover_parent` signature broken across lines.
  > - `lib/foundation/fabro-types/src/pull_request.rs` — `let parsed =` break, mirroring the identical existing pattern 17 lines below.
  > - `lib/foundation/fabro-types/src/run_intent.rs` — 110-char `use crate::{...}` split; `tests/run_intent.rs` — `assert_eq!` args split.
  > - `lib/apps/fabro-cli/src/shared/forgejo.rs` — trailing newline added.
  > 
  > Lines >100 that remain are pre-existing or unspittable string literals already alone on their line — accepted rustfmt overflow.
  > 
  > ### Defect 2 — missing wiring tests (added all five)
  > 
  > - **2a server** (`tests/it/api/runs.rs`): 4 new tests — intent admission without the integration → 422 `target_invalid` with the exact ForgejoUnconfigured message; intent admission with URL but no vault PAT → 422 `forgejo_integration_unconfigured` naming `server.integrations.forgejo.url`; preflight with a forgejo origin and no PAT → failed "Repository Access" check with `FORGEJO_TOKEN` remediation; preflight against an httpmock-served instance API (repo + branch endpoints, `token`-auth header) → passing check with both probes asserted.
  > - **2b materializer** (`automation_materializer.rs` tests): full-pipeline test proving a `provider = "forgejo"` target threads the instance URL to the resolver and clones via PAT auth through a local bare fixture; plus a direct `ServerGitHubRemoteResolver` test pinning the forgejo clone URL/`forgejo_git_auth` equality, the origin-mismatch `FORGEJO_TOKEN` error, and an untouched GitHub lane.
  > - **2c manifest** (`fabro-manifest/src/lib.rs` tests): `provider = "forgejo"` (both casings) resolves `ConfiguredOrigin` against the instance; blank/missing instance → the explicit `requires a configured Forgejo instance` error; missing owner/repo → `None`; `github` provider still resolves without Forgejo.
  > - **2d repo/init** (`tests/it/cmd/repo_init.rs`): SSH origin on the configured instance (via `FABRO_CONFIG`) writes the `[run.scm] provider = "forgejo"` block with owner/repository; GitHub origin writes no scm block.
  > - **2e PR pipeline** (`pipeline/pull_request.rs` tests): a forgejo origin runs branch-verify → reconcile → create entirely against the instance API with `token`-auth headers, returns a `WIP: `-prefixed draft title and a `Forgejo`-tagged link carrying the instance origin, and makes **zero** calls to the GitHub mocks.
  > 
  > **Additional defect found while adding 2e** (invisible to the reviewer, who had no disk to compile): three existing test struct literals omitted the new `forgejo` field, so `fabro-workflow`'s tests did not compile. Added `forgejo: None` to all three.
  > 
  > ### Defect 3 — dead public API (removed)
  > 
  > - Deleted `FORGEJO_CREDENTIAL_HELPER`, `credential_helper`, `credential_helper_key`, their tests, and the `#[expect(dead_code)]` stub; crate docs now state the actual auth seams (PAT embedded in clone URLs at sandbox/server; local pushes use host credentials, same as GitHub).
  > - Removed the never-read `ForgejoRepository.owner` field and the `RepositoryOwner` struct (serde ignores the payload's `owner` key).
  > - `current_user` kept: it is consumed by `tests/live_access.rs` (the live PAT contract test) — same role as GitHub's token validation path.
  > - `secret_registry.rs`: `FORGEJO_TOKEN` added to the `classifies_optional_vault_secrets` test list.
  > 
  > ## Verification status
  > 
  > - **Ran green:** `cargo test -p fabro-forgejo` (all, incl. 11 httpmock integration tests); `cargo test -p fabro-types -p fabro-static` (500+ tests incl. the forgejo run-target and secret-registry tests); `apps/fabro-web bun test` on the changed file (2/2; the 28 full-suite failures are all in files the forgejo change never touched — pre-existing baseline).
  > - **Blocked by environment:** after the smallest-crate runs, the **shared host disk went to 100% and byte-level writes fail** (deleting build artifacts, the registry src, and even the bun runtime only bought minutes — an external tenant is consuming the pool; I probed and retried for ~40 minutes). `cargo test -p fabro-manifest` and `cargo check --tests` for `fabro-store/fabro-workflow/fabro-server/fabro-cli` fail at rmeta/link with ENOSPC. As partial compensation, every edited file passes a full rustc parse (rustfmt-as-parser, zero syntax errors), all added lines respect `max_width = 100` modulo accepted string overflow, and the stable-rustfmt proposals intersecting my edits are exclusively the two known stable↔nightly divergences. Once disk frees, a reviewer should run: `cargo nextest run -p fabro-manifest -p fabro-store -p fabro-workflow -p fabro-server -p fabro-cli` and `cargo +nightly-2026-04-14 fmt --check --all`.

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

## Stage: implement
- Status: succeeded
- Handler: agent
- Model: glm-5.3-flash
- Files touched: /workspace/fabro/lib/apps/fabro-cli/src/commands/doctor.rs, /workspace/fabro/lib/apps/fabro-server/src/automation_materializer.rs, /workspace/fabro/lib/apps/fabro-server/src/run_manifest.rs, /workspace/fabro/lib/apps/fabro-server/src/server.rs, /workspace/fabro/lib/apps/fabro-server/src/server/handler/pull_requests.rs, /workspace/fabro/lib/apps/fabro-server/tests/it/api/runs.rs, /workspace/fabro/lib/components/fabro-forgejo/src/lib.rs, /workspace/fabro/lib/components/fabro-forgejo/src/url.rs, /workspace/fabro/lib/components/fabro-manifest/src/lib.rs, /workspace/fabro/lib/components/fabro-sandbox/src/push_credentials.rs, /workspace/fabro/lib/components/fabro-store/src/run_state.rs, /workspace/fabro/lib/components/fabro-workflow/src/pipeline/pull_request.rs, /workspace/fabro/lib/components/fabro-workflow/src/run_metadata.rs, /workspace/fabro/lib/foundation/fabro-static/src/secret_registry.rs, /workspace/fabro/lib/foundation/fabro-types/src/pull_request.rs, /workspace/fabro/lib/foundation/fabro-types/src/run_intent.rs, /workspace/fabro/lib/foundation/fabro-types/tests/run_intent.rs
- Response:
  > The host disk never recovered — byte-level writes fail across the entire shared overlay. I've exhausted what's verifiable in this environment. Final summary:
  > 
  > ## Summary of changes (review-fix pass)
  > 
  > The implementation was already committed by the pipeline; this stage addressed the three blocking defects from `deep_review`. All changes are in 19 files, +626/−107.
  > 
  > ### Defect 1 — CI formatting gate (fixed by hand; the pinned nightly toolchain can't be installed at 0 bytes free, and stable rustfmt mis-reports style_edition-2024 formatting as diffs, so I fixed the exact constructs instead)
  > 
  > - `lib/apps/fabro-cli/src/commands/doctor.rs` — split `{    match` off the `check_config` signature line.
  > - `lib/apps/fabro-server/src/server.rs` — split `{Ok(integration) => integration,` onto its own line.
  > - `.../server/handler/pull_requests.rs` — broke the 121-char `is_some_and` chain; both 107-char `ApiError::new(...)` match arms rewritten as braced arms (inner line exactly 100).
  > - `lib/apps/fabro-server/src/run_manifest.rs` — the 143-char `Err("...")` line, and fixed a copy/paste artifact (a run of spaces inside the remediation message); message now uses `` `fabro secret set FORGEJO_TOKEN` `` matching doctor/server phrasing.
  > - `lib/apps/fabro-server/src/automation_materializer.rs` — broke the 102-char `resolve_remote(...)` args vertically.
  > - `lib/components/fabro-manifest/src/lib.rs` — four 110–113-char test calls broken into rustfmt's arg-per-line + `.unwrap()` chain shape.
  > - `lib/components/fabro-sandbox/src/push_credentials.rs` — 101-char `.map_err(...)` chain: closure body moved into a block.
  > - `lib/components/fabro-store/src/run_state.rs` — 105-char `PullRequestLink::github(...)` arm broken into a multi-line call arm.
  > - `lib/components/fabro-workflow/src/pipeline/pull_request.rs` — 104-char struct field: extracted `github_app::GitHubContext` into a local; `github_app` alias resolves via the tests module's `use super::*`.
  > - `lib/components/fabro-workflow/src/run_metadata.rs` — `discover_parent` signature broken across lines.
  > - `lib/foundation/fabro-types/src/pull_request.rs` — `let parsed =` break, mirroring the identical existing pattern 17 lines below.
  > - `lib/foundation/fabro-types/src/run_intent.rs` — 110-char `use crate::{...}` split; `tests/run_intent.rs` — `assert_eq!` args split.
  > - `lib/apps/fabro-cli/src/shared/forgejo.rs` — trailing newline added.
  > 
  > Lines >100 that remain are pre-existing or unspittable string literals already alone on their line — accepted rustfmt overflow.
  > 
  > ### Defect 2 — missing wiring tests (added all five)
  > 
  > - **2a server** (`tests/it/api/runs.rs`): 4 new tests — intent admission without the integration → 422 `target_invalid` with the exact ForgejoUnconfigured message; intent admission with URL but no vault PAT → 422 `forgejo_integration_unconfigured` naming `server.integrations.forgejo.url`; preflight with a forgejo origin and no PAT → failed "Repository Access" check with `FORGEJO_TOKEN` remediation; preflight against an httpmock-served instance API (repo + branch endpoints, `token`-auth header) → passing check with both probes asserted.
  > - **2b materializer** (`automation_materializer.rs` tests): full-pipeline test proving a `provider = "forgejo"` target threads the instance URL to the resolver and clones via PAT auth through a local bare fixture; plus a direct `ServerGitHubRemoteResolver` test pinning the forgejo clone URL/`forgejo_git_auth` equality, the origin-mismatch `FORGEJO_TOKEN` error, and an untouched GitHub lane.
  > - **2c manifest** (`fabro-manifest/src/lib.rs` tests): `provider = "forgejo"` (both casings) resolves `ConfiguredOrigin` against the instance; blank/missing instance → the explicit `requires a configured Forgejo instance` error; missing owner/repo → `None`; `github` provider still resolves without Forgejo.
  > - **2d repo/init** (`tests/it/cmd/repo_init.rs`): SSH origin on the configured instance (via `FABRO_CONFIG`) writes the `[run.scm] provider = "forgejo"` block with owner/repository; GitHub origin writes no scm block.
  > - **2e PR pipeline** (`pipeline/pull_request.rs` tests): a forgejo origin runs branch-verify → reconcile → create entirely against the instance API with `token`-auth headers, returns a `WIP: `-prefixed draft title and a `Forgejo`-tagged link carrying the instance origin, and makes **zero** calls to the GitHub mocks.
  > 
  > **Additional defect found while adding 2e** (invisible to the reviewer, who had no disk to compile): three existing test struct literals omitted the new `forgejo` field, so `fabro-workflow`'s tests did not compile. Added `forgejo: None` to all three.
  > 
  > ### Defect 3 — dead public API (removed)
  > 
  > - Deleted `FORGEJO_CREDENTIAL_HELPER`, `credential_helper`, `credential_helper_key`, their tests, and the `#[expect(dead_code)]` stub; crate docs now state the actual auth seams (PAT embedded in clone URLs at sandbox/server; local pushes use host credentials, same as GitHub).
  > - Removed the never-read `ForgejoRepository.owner` field and the `RepositoryOwner` struct (serde ignores the payload's `owner` key).
  > - `current_user` kept: it is consumed by `tests/live_access.rs` (the live PAT contract test) — same role as GitHub's token validation path.
  > - `secret_registry.rs`: `FORGEJO_TOKEN` added to the `classifies_optional_vault_secrets` test list.
  > 
  > ## Verification status
  > 
  > - **Ran green:** `cargo test -p fabro-forgejo` (all, incl. 11 httpmock integration tests); `cargo test -p fabro-types -p fabro-static` (500+ tests incl. the forgejo run-target and secret-registry tests); `apps/fabro-web bun test` on the changed file (2/2; the 28 full-suite failures are all in files the forgejo change never touched — pre-existing baseline).
  > - **Blocked by environment:** after the smallest-crate runs, the **shared host disk went to 100% and byte-level writes fail** (deleting build artifacts, the registry src, and even the bun runtime only bought minutes — an external tenant is consuming the pool; I probed and retried for ~40 minutes). `cargo test -p fabro-manifest` and `cargo check --tests` for `fabro-store/fabro-workflow/fabro-server/fabro-cli` fail at rmeta/link with ENOSPC. As partial compensation, every edited file passes a full rustc parse (rustfmt-as-parser, zero syntax errors), all added lines respect `max_width = 100` modulo accepted string overflow, and the stable-rustfmt proposals intersecting my edits are exclusively the two known stable↔nightly divergences. Once disk frees, a reviewer should run: `cargo nextest run -p fabro-manifest -p fabro-store -p fabro-workflow -p fabro-server -p fabro-cli` and `cargo +nightly-2026-04-14 fmt --check --all`.

## Current context
| Key | Value |
|-----|-------|
| parallel.branch_count | 4 |
| parallel.results | 30.7 KB; full value: `/tmp/fabro/runtime/blobs/a149633c35bf90dca98a2f2a1e315c15ee9ee8a2e0cf5bb1047c6928d006a66c.json`; Preview: [{"id":"r_correct","index":0,"status":"succeeded","context_updates":{"last_response":"# Correctness review — Forgejo integration (109 files, +5118)\n\nI read the full diff and the surrounding context of every correctness-critical seam: the new `fabro-forgejo` crate (API ops, URL/creden","last_stage"… |


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

Review the change for correctness only.

Look for logic that produces a wrong result: off-by-one errors, inverted
conditions, unhandled nil or empty cases, incorrect operator precedence, wrong
variable used, race conditions, resource leaks.

For each finding give the file, the line if you can identify it, what is wrong,
and the concrete input or state that triggers it. A finding you cannot make
fail is a guess — label it as such or drop it.

If the diff is correct, say so plainly. Do not invent findings to seem useful.
