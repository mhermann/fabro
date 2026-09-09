Goal: Add support for forgejo similar to github integration
Run ID: 01M23NSPQB9EAM7VBAZQDAYYHX
Pipeline progress: 5 of 17 stages completed

## Stage: understand
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > Exploration complete. Here is my understanding.
  > 
  > ## 1. Repository structure and where the change lands
  > 
  > **Fabro** is an AI workflow orchestration platform: agents run as stages of Graphviz-defined workflows, executing inside clone-based sandboxes (Docker default, Daytona, Local) against git repositories, pushing branches/checkpoints and opening PRs.
  > 
  > - **Language/tooling**: Rust workspace under `lib/` (cargo + nextest, pinned nightly fmt/clippy, insta snapshots), React 19 web UI in `apps/fabro-web` (Bun), OpenAPI-first HTTP contract (`docs/public/api-reference/fabro-api.yaml` → progenitor Rust types + generated TS client), docs as MDX in `docs/public/`.
  > - **Layout**: `lib/foundation/` (fabro-types, fabro-config, fabro-static, fabro-http, fabro-vault, fabro-oauth…), `lib/components/` (fabro-github, fabro-workflow, fabro-sandbox, fabro-tracker, fabro-slack…), `lib/apps/` (fabro-cli, fabro-server).
  > 
  > The GitHub integration is **not one module — it's a vertical slice through ~10,000+ lines across six crates**:
  > 
  > | Layer | Location | Role |
  > |---|---|---|
  > | API client crate | `lib/components/fabro-github/` (~4,400 LOC) | GitHub App JWT + installation tokens, PAT, PR CRUD (`create/find/get/merge/close_pull_request`, `enable_auto_merge`, `branch_head_sha`), app webhook config, URL parsing (`parse_github_owner_repo` hardcodes `https://github.com/`), git credential helper, `resolve_clone_credentials`, cached token source with provenance |
  > | Server | `lib/apps/fabro-server/` | Webhook route `/api/v1/webhooks/github` (HMAC `X-Hub-Signature-256`), OAuth login (`ServerAuthMethod::Github`, `auth/github_endpoints.rs`), install flow (`install.rs` + `fabro-install`: App manifest, vault keys), `git_checkout.rs`, automations (`automation_materializer.rs`), integration status |
  > | Workflow engine | `lib/components/fabro-workflow/` | Auto-PR pipeline (`pipeline/pull_request.rs`, 2,046 LOC), publish, `git_bridge.rs` (injects `GITHUB_TOKEN` + git credential-helper env into sandboxes) |
  > | Sandbox | `lib/components/fabro-sandbox/` | `clone_source.rs`: **explicitly GitHub-only** clone contract ("Clone-based sandboxes currently support GitHub repository origins only"), `push_credentials.rs`, Docker/Daytona transports |
  > | Types/config | `lib/foundation/fabro-types`, `fabro-config` | `GitHubRepositorySlug`, `RepositoryProvider { Github, Git, Unknown }`, `PullRequestGithubDetail`, settings: server `integrations.github { strategy: token\|app, app_id, slug, webhooks }`, `run.integrations.github { permissions, additional_repositories }`, and `run.scm { provider, owner, repository, github }` |
  > | CLI + web + docs | `fabro-cli` (`gh.rs`, `shared/github.rs`, install cmd), `apps/fabro-web` (integrations panel, auth, install API), `docs/public/integrations/github.mdx` | Setup UX, credentials from vault/`gh auth token`, UI panels, user docs |
  > 
  > Notably, `run.scm` already exists as a **designed extension point**: `RunScmLayer` carries `provider: Option<String>` with a comment "Provider-specific SCM leaves. First-pass providers", and `ScmGitHubLayer` is "Intentionally minimal in the first pass." The codebase anticipated non-GitHub SCMs. There is **zero existing Forgejo/Gitea code** (one incidental mention in a design doc).
  > 
  > ## 2. Goal restated concretely
  > 
  > Add Forgejo (self-hostable Gitea fork) as a second first-class SCM/forge provider alongside GitHub, following the same vertical shape: a client crate for Forgejo's API (token auth, PR create/get/merge/close, clone credentials), server settings + install path, webhook ingestion, sandbox cloning/pushing from Forgejo origins, auto-PR from workflows, `run.scm.provider = "forgejo"` config, `RepositoryProvider::Forgejo`, integration status surface, web UI panel, OpenAPI updates, and docs — parameterized by an instance base URL since Forgejo has no fixed host.
  > 
  > ## 3. What I know for certain
  > 
  > - The GitHub slice is deep: credentials → token minting/caching → git bridge env → sandbox clone/push → PR pipeline → webhooks → OAuth → install → UI/docs, with a test harness (mock `HttpClient` trait, httpmock-based PR pipeline tests, twin/e2e patterns).
  > - `fabro-github` already parameterizes the **API base URL** (`GITHUB_BASE_URL` env, `GitHubContext`), but URL parsing, origin detection (`is_github_origin`), the credential helper host (`credential.https://github.com.helper`), and clone-source validation are hardcoded to github.com.
  > - Clone-based sandboxes (Docker/Daytona) reject non-GitHub origins today; `GitHubRepoLayout` builds owner/repo paths and symlinks from the origin.
  > - `run.scm` with `provider` + per-provider leaves, and `RepositoryRef.provider` enum `[github, git, unknown]`, are deliberate seams for this exact change.
  > - Server settings support two GitHub strategies: `token` (PAT via vault/`gh auth token`) and `app` (GitHub App + installation tokens + OAuth + webhooks).
  > - Env-var registry (`fabro-static`), vault secret types, redaction (`DisplaySafeUrl`), shell quoting, strum conventions, OpenAPI conformance tests, and strategy docs (events, migrations, secrets, error-handling) all constrain how new integration code must be written.
  > - Forgejo exposes a GitHub-*ish* REST API under `/api/v1` but has **no GitHub Apps**: auth is PAT/scoped tokens and OAuth2 apps; webhooks use `X-Gitea-Event`/`X-Gub-Signature`-family headers (HMAC-SHA256 with a per-hook secret); web PR URLs use `/pulls/{n}` not `/pull/{n}`.
  > 
  > ## 4. Genuinely ambiguous (two reasonable engineers would build differently)
  > 
  > 1. **Auth model parity**: GitHub has `token` and `app` strategies. Forgejo has no App/installation-token equivalent. PAT-only? PAT + OAuth2-app browser login? Is browser sign-in (currently GitHub-OAuth-gated) in scope at all?
  > 2. **One instance or many**: Forgejo is self-hosted. Does Fabro support a single configured instance (env/settings like `FORGEJO_URL` + `FORGEJO_TOKEN`, mirroring `GITHUB_*`), or arbitrary per-run origins detected by URL host? This ripples through credential lookup, `additional_repositories` slugs (no host component today), and the credential-helper host scoping.
  > 3. **Crate architecture**: new `fabro-forgejo` crate mirroring `fabro-github` vs. generalizing `fabro-github` into a provider-abstracted forge crate (the existing `GITHUB_BASE_URL` override shows partial precedent). The API shapes are close but not identical (e.g., merge endpoint payload differs), so "reuse with base URL" vs "separate DTOs" is a real fork.
  > 4. **Surface scope for v1**: minimum viable = clone/push/auto-PR? Or also webhooks→automations, repo-access check endpoint (`/api/v1/repos/forgejo/...`), sandbox token injection (`FORGEJO_TOKEN` vs reusing `GITHUB_TOKEN` since Forgejo tooling reads it), integration status, tracker (Forgejo has no Projects V2 GraphQL)?
  > 5. **Webhook ingestion**: separate route `/api/v1/webhooks/forgejo` with Forgejo's header/signature scheme, and which events feed the automation scheduler?
  > 6. **Config naming**: `run.scm.forgejo` leaf (empty like GitHub's?), `integrations.forgejo` server settings shape, `RepositoryProvider::Forgejo` vs reclassifying as generic `Git` with a forge type, whether `gitea` is an alias.
  > 7. **PR link handling**: existing validation only accepts `https://github.com/owner/repo/pull/N`; Forgejo uses instance-host URLs and `/pulls/N` — new validation, and does `PullRequestGithubDetail` get a Forgejo sibling or a provider-neutral rename (OpenAPI/type-ownership rules apply)?
  > 8. **Install UX**: `fabro install` flow for Forgejo — PAT prompt only, or OAuth2 app creation helper? Affects web install API endpoints and the install-flow UI.
  > 
  > No files were modified; this stage was exploration only.

## Stage: ask
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > All four questions answered. Restatement for the planning stage:
  > 
  > **1. V1 scope → Core + operator surface**
  > Forgejo support covers: clone from Forgejo origins, push branches/checkpoints, auto-PR create/merge/close, `run.scm.forgejo`, integration status in the web UI + system endpoint, a repo-access check endpoint (`/api/v1/repos/forgejo/...`), and `fabro install` support for a Forgejo token strategy. **Webhook ingestion and automation triggers are explicitly out of scope for v1** — no new webhook route, no automation-materializer changes.
  > 
  > **2. Auth model → PAT/token only**
  > A scoped API token stored like GitHub's `token` strategy (env/vault, e.g. `FORGEJO_TOKEN`). No OAuth2 app, no browser sign-in via Forgejo — web UI auth is untouched. No GitHub-App-equivalent flows, so no installation-token machinery for Forgejo.
  > 
  > **3. Instance model → Single configured instance**
  > One instance URL + one token (e.g. `FORGEJO_URL`, `FORGEJO_TOKEN`). `owner/repo` slugs remain host-free and unambiguous, credential lookup stays simple, and the git credential helper scopes to that single configured host. Multi-instance/host-aware slugs are out of scope.
  > 
  > **4. Crate architecture → New fabro-forgejo crate**
  > A parallel component crate (`lib/components/fabro-forgejo`) with its own client and DTOs, following the `fabro-github`/`fabro-slack` pattern. The existing GitHub path is not refactored; shared behavior is limited to foundation helpers. Duplication of URL/token-caching patterns is an accepted cost.
  > 
  > Decisions I had already made and that stand unless overridden: no tracker support; naming follows convention (`run.scm.forgejo`, `integrations.forgejo`, `RepositoryProvider::Forgejo`, provider string `"forgejo"`); PR links accept `{instance}/owner/repo/pulls/{n}` with `/pull/{n}` as alias; sandbox token env var is `FORGEJO_TOKEN` (no `GITHUB_TOKEN` overload); target is the Forgejo API — Gitea instances likely work but are not promised.
  > 
  > The planning stage can now treat the goal as fully determined: a single-instance, PAT-only, new-crate Forgejo integration covering core SCM plus the operator surface, with webhooks/automations and OAuth left for a later pass.

## Stage: triage
- Status: succeeded
- Handler: prompt
- Model: glm-5.3
- Response:
  > I have enough to plan. Here's my understanding of what's wanted, plus the decisions I'm making on the human's behalf:
  > 
  > **What they want:** Forgejo support comparable to the existing GitHub integration — the ability to point Fabro at Forgejo-hosted repositories (codeberg.org out of the box, self-hosted instances via configured base URL) for the same jobs GitHub does today: authenticated API access, PR creation/comments, and clone-based run workspaces.
  > 
  > **Decisions I'm making myself:**
  > 
  > - **Structure:** a new sibling crate `lib/components/fabro-forgejo` mirroring `fabro-github`'s surface (auth, API client, PR operations), rather than prematurely generalizing `fabro-github` into a generic "git forge" abstraction. This matches the existing one-integration-per-crate pattern (`fabro-github`, `fabro-slack`, `fabro-mcp`).
  > - **Auth:** Forgejo API tokens (personal access tokens) — Forgejo has no GitHub-App JWT/installation-token equivalent, so token auth is the honest parity point, with per-instance token config in server/CLI config mirroring how GitHub creds are supplied.
  > - **Clone support:** extend the sandbox clone-source contract so run manifests with a Forgejo origin (host + owner + repo) clone successfully in both Docker and Daytona providers, reusing the exact branch/SHA semantics and never-fallback rules already documented for GitHub origins. GitHub-only behavior is untouched for GitHub origins.
  > - **Webhooks:** treat Forgejo payloads as Gitea-compatible; parse the event types needed for run triggering, mapping them onto the same internal events GitHub webhooks produce.
  > - **Scope boundary:** issue timelines, releases, and other GitHub features with no clear role in Fabro's run flow stay out of the first pass.
  > 
  > Nothing in their answers opened a gap that would change *what* gets built — the rest is implementation detail I can settle while planning.
  > 
  > {"preferred_next_label": "enough"}

## Stage: plan
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response (22.4 KB; full value: `/tmp/fabro/runtime/blobs/bfab6aefe67347a003feb1e9ffd98b06401a4c0edf78487cb496b80d8cf722fc.json`)
  Preview: I now have the complete map of the GitHub vertical. Here is the implementation plan.
  
  ---
  
  # Implementation Plan: Forgejo Integration (v1 — core SCM + operator surface)
  
  **Authoritative constraints from the human:** PAT/token-only auth (no OAuth2, no GitHub-App-equivalent); single configured instanc…

## Stage: devils
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > I attacked the plan against the repository as it actually is. Findings, ordered by severity:
  > 
  > **1. Missed call site: PR-URL association on run submission (non-blocking).** `fabro-types/src/pull_request.rs`'s `github_pull_request_link_from_url` gates the `pull_request_url` parameter on run submission (OpenAPI line ~12810: "Must use the form `https://github.com/{owner}/{repo}/pull/{number}`"). The plan adds `PullRequestLink::from_forgejo_url` (Phase 1 #8) and covers PR get/create/merge/close (Phase 6 #35) but never names the association call site, so an implementer following the file list literally would leave Forgejo PR URLs rejected at run submission. However, the `PullRequestLink.forge` design already accommodates the fix — it's a one-branch addition at an existing parser call site, not rework. The implementer should treat #8's parser as consumed there too.
  > 
  > **2. Fixture-circular wire verification (acknowledged, acceptable).** Every Phase 2 test encodes my Forgejo wire-format claims as fixtures, so all tests pass even if the real API disagrees (`Do` casing, `Authorization: token`, token-as-password clone auth, `WIP:` draft). The plan names this honestly and specifies an exact live checklist with a Docker fallback. Since the host is linuxkit (Docker likely available), the implementer should prefer running the live check rather than defaulting to the fallback — but the failure mode is a localized constant/fixture fix, not structural rework.
  > 
  > **3. Unverified semantics of the `env_vars.rs` registry list (non-blocking).** The plan says "add both to the registry list (~line 225) so scrubbing/redaction covers them" without establishing what that list actually drives (it mixes secrets like `GITHUB_APP_PRIVATE_KEY` with non-secrets like `SLACK_BASE_URL`, so it's not obviously a scrub list — it may feed `server_secrets.rs`'s `process_env_snapshot` or env-file persistence). Wrong membership could either leak `FORGEJO_TOKEN` into places it shouldn't reach or prevent it reaching workers. The edit itself forces reading the const's name and consumers; discoverable in minutes, but worth flagging since the plan asserted a specific behavior it didn't verify.
  > 
  > **4. `fabro-forgejo`'s relationship to `normalize_repo_origin_url` is ambiguous (non-blocking).** `ssh_url_to_https`/`normalize_repo_origin_url` in `fabro-github` are host-generic (verified), and `fabro-sandbox`/`fabro-manifest` call them today. The new crate either duplicates ~40 lines or takes a `fabro-github` dependency — the plan's Cargo.toml doesn't list it and the prose says "reuses the same normalization semantics," which is not a decision. Either answer works; the human's "sharing only foundation helpers" nudges toward duplication, but `fabro-sandbox` will still call `fabro_github::normalize_repo_origin_url` for both providers. Ordinary judgment, no rework either way.
  > 
  > **5. Reconcile path lists all open PRs (behavior note).** Forgejo's list-pulls endpoint has no `base`/`head` filters like GitHub's, so `find_open_pull_request` fetches every open PR and filters client-side. Correct but O(open PRs); fine for v1, worth a comment in code.
  > 
  > **6. Auto-merge hard-error interaction (accepted design cost).** A user with `auto_merge = true` at the user layer cannot run any `provider = "forgejo"` workflow without per-layer override. The plan chose explicit rejection over silent divergence — defensible and cheap to soften later (warning instead of error is a one-line change at the validation site, not a type change).
  > 
  > Things I checked that the plan gets right: the demo-router mirror at `handler/mod.rs:173` is covered; `PullRequestLink` is an existing `with_replacement` type with a parity test to update; `InstallationTokenSource::pat` exists and fits the static-PAT push-credential reuse; `CloneDecision` sibling-variant avoids breaking existing GitHub sandbox tests; `ServerIntegrationsLayer`/`deny_unknown_fields` edit is consistent; conformance-test ordering (Phases 6+7 together) is stated; no root `Cargo.toml` edit needed (glob members); `GitRepoCache`/`git_checkout.rs` are automation-only and correctly excluded.
  > 
  > No finding forces redoing planned work or reverses an expensive decision — the load-bearing designs (new crate boundary, `PullRequestLink.forge` as an optional additive field, PAT-through-`InstallationTokenSource::pat`, `run.scm` provider routing) are sound against the actual code and the human's answers.
  > 
  > {"preferred_next_label": "approve"}


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

If you are here after a review found defects, its findings are in the
context above — read them and fix exactly what was raised, point by point.
Do not take the opportunity to refactor unrelated code. If you believe a finding
is wrong, say so and explain why rather than silently ignoring it; repeating the
same work unchanged will simply be rejected again.

Finish by summarising what you changed, file by file.