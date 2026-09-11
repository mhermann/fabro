Goal: Add support for forgejo similar to github integration
Run ID: 01M268NGXHMS7ET6T4W4Y38G7A
Pipeline progress: 11 of 22 stages completed

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