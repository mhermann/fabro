Goal: Add support for forgejo similar to github integration
Run ID: 01M23NSPQB9EAM7VBAZQDAYYHX
Pipeline progress: 6 of 17 stages completed

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

## Stage: implement
- Status: succeeded
- Handler: agent
- Model: glm-5.3-flash
- Files touched: /workspace/fabro/apps/fabro-web/app/routes/settings-integrations.test.tsx, /workspace/fabro/apps/fabro-web/app/routes/settings-integrations.tsx, /workspace/fabro/docs/public/api-reference/fabro-api.yaml, /workspace/fabro/docs/public/docs.json, /workspace/fabro/docs/public/integrations/forgejo.mdx, /workspace/fabro/lib/apps/fabro-cli/Cargo.toml, /workspace/fabro/lib/apps/fabro-cli/src/args.rs, /workspace/fabro/lib/apps/fabro-cli/src/commands/repo/init.rs, /workspace/fabro/lib/apps/fabro-cli/src/shared/forgejo.rs, /workspace/fabro/lib/apps/fabro-cli/tests/it/cmd/install.rs, /workspace/fabro/lib/apps/fabro-server/Cargo.toml, /workspace/fabro/lib/apps/fabro-server/src/run_manifest.rs, /workspace/fabro/lib/apps/fabro-server/src/server.rs, /workspace/fabro/lib/apps/fabro-server/src/server/handler/pull_requests.rs, /workspace/fabro/lib/apps/fabro-server/src/server/handler/system.rs, /workspace/fabro/lib/apps/fabro-server/src/server/pull_request_supervisor.rs, /workspace/fabro/lib/components/fabro-forgejo/Cargo.toml, /workspace/fabro/lib/components/fabro-forgejo/src/lib.rs, /workspace/fabro/lib/components/fabro-forgejo/src/tests.rs, /workspace/fabro/lib/components/fabro-forgejo/src/tests_mock.rs, /workspace/fabro/lib/components/fabro-install/src/lib.rs, /workspace/fabro/lib/components/fabro-manifest/Cargo.toml, /workspace/fabro/lib/components/fabro-manifest/src/lib.rs, /workspace/fabro/lib/components/fabro-sandbox/Cargo.toml, /workspace/fabro/lib/components/fabro-sandbox/src/clone_source.rs, /workspace/fabro/lib/components/fabro-sandbox/src/daytona/mod.rs, /workspace/fabro/lib/components/fabro-sandbox/src/lib.rs, /workspace/fabro/lib/components/fabro-sandbox/src/push_credentials.rs, /workspace/fabro/lib/components/fabro-sandbox/src/sandbox_spec.rs, /workspace/fabro/lib/components/fabro-sandbox/tests/docker_streaming.rs, /workspace/fabro/lib/components/fabro-store/src/run_state.rs, /workspace/fabro/lib/components/fabro-workflow/src/git_bridge.rs, /workspace/fabro/lib/components/fabro-workflow/src/operations/fork.rs, /workspace/fabro/lib/components/fabro-workflow/src/operations/mod.rs, /workspace/fabro/lib/components/fabro-workflow/src/pipeline/initialize.rs, /workspace/fabro/lib/components/fabro-workflow/src/pipeline/mod.rs, /workspace/fabro/lib/components/fabro-workflow/src/pipeline/publish.rs, /workspace/fabro/lib/components/fabro-workflow/src/pipeline/types.rs, /workspace/fabro/lib/components/fabro-workflow/src/pull_request.rs, /workspace/fabro/lib/components/fabro-workflow/src/run_metadata.rs, /workspace/fabro/lib/components/fabro-workflow/src/run_options.rs, /workspace/fabro/lib/foundation/fabro-api/build.rs, /workspace/fabro/lib/foundation/fabro-api/tests/pull_request_round_trip.rs, /workspace/fabro/lib/foundation/fabro-api/tests/system_integrations_round_trip.rs, /workspace/fabro/lib/foundation/fabro-client/src/client.rs, /workspace/fabro/lib/foundation/fabro-config/Cargo.toml, /workspace/fabro/lib/foundation/fabro-config/src/layers/mod.rs, /workspace/fabro/lib/foundation/fabro-config/src/layers/server.rs, /workspace/fabro/lib/foundation/fabro-config/src/lib.rs, /workspace/fabro/lib/foundation/fabro-config/src/resolve/run.rs, /workspace/fabro/lib/foundation/fabro-config/src/resolve/server.rs, /workspace/fabro/lib/foundation/fabro-config/src/tests/resolve_run.rs, /workspace/fabro/lib/foundation/fabro-config/src/tests/resolve_server.rs, /workspace/fabro/lib/foundation/fabro-static/src/env_vars.rs, /workspace/fabro/lib/foundation/fabro-types/src/lib.rs, /workspace/fabro/lib/foundation/fabro-types/src/pull_request.rs, /workspace/fabro/lib/foundation/fabro-types/src/repository.rs, /workspace/fabro/lib/foundation/fabro-types/src/settings/server.rs, /workspace/fabro/lib/foundation/fabro-types/src/system_integrations.rs
- Response:
  > All verification gates pass. Here's the complete summary of the implementation.
  > 
  > ## Summary
  > 
  > Forgejo is now a second first-class git forge in Fabro: single configured instance (`FORGEJO_URL` + `FORGEJO_TOKEN`), token-only auth, covering clone/push, auto-PR, integration status, repo-access check, `run.scm.forgejo`, `fabro install forgejo`, web UI row, OpenAPI + generated clients, and docs.
  > 
  > **Verification:** `cargo build --workspace` clean; `cargo nextest run --workspace` **8,464 passed** (incl. 995 fabro-cli and 903 fabro-server tests, OpenAPI conformance green); `fmt --check` clean; `clippy -D warnings` clean; TS client regenerated with `typecheck` clean and new web tests passing. Docker isn't available in this sandbox, so the plan's optional live Forgejo wire check wasn't run — fixtures encode the wire shapes (`Authorization: token`, `commit.id`, capital-`Do`, `WIP:` drafts, 201 on edit) as the plan's fallback anticipated.
  > 
  > ### Changed, file by file
  > 
  > **Foundation**
  > - `fabro-static/src/env_vars.rs` — `FORGEJO_URL`, `FORGEJO_TOKEN` consts + registry list.
  > - `fabro-types/src/repository.rs` — `RepositoryProvider::Forgejo`; `is_forgejo_origin()` (host + base-path match, scheme-agnostic); `RepositoryRef::from_origin_and_source_with_forge()`.
  > - `fabro-types/src/pull_request.rs` — `PullRequestLink.forge: Option<ForgeInstanceRef>` (skipped when `None` → legacy JSON byte-identical); `PullRequestLink::github/on_forge/from_forgejo_url/from_stored_pr_url`; forge parser accepts `/pulls/{n}` and `/pull/{n}`.
  > - `fabro-types/src/system_integrations.rs` — `IntegrationProvider::Forgejo`.
  > - `fabro-types/src/settings/server.rs` — `ForgejoIntegrationSettings { enabled, url }`; added to `ServerIntegrationsSettings`.
  > - `fabro-config` layers/resolve — `ForgejoIntegrationLayer`; resolution validates https/credential-free URL and trims trailing slash; `run.scm.provider="forgejo"` + `auto_merge` rejected at resolve.
  > 
  > **New crate `lib/components/fabro-forgejo`** — Forgejo API client: `ForgejoCredentials`, `ForgejoContext`, URL parse/normalize/`embed_token_in_url`, `credential_helper_key(base_url)`, endpoints (`user`, `repo`, `branch_head_sha` via `commit.id`, find/create/get/merge/close PR with capital-`Do` and `WIP:` draft convention), `PullRequestForgejoDetail → PullRequestDetails`. 22 tests with a scripted mock client.
  > 
  > **Manifest** — `configured_repo_origin_url` builds `{instance}/{owner}/{repo}` for forgejo (errors when no instance configured); GitHub path unchanged.
  > 
  > **Sandbox** — `CloneDecision::Forgejo`; `decide_clone` takes the instance URL; shared `repo_layout` extracted with `repo_layout_for_origin` dispatch; `ForgejoSandboxCreds` (redacting `Debug`, reuses `SecretString`); `build_token_source` maps forgejo origins to a static-PAT source; Docker + Daytona constructors/clone paths threaded (`git_origin` accessor dropped as unused — daytona uses `is_forgejo()`).
  > 
  > **Workflow engine** — `PullRequestRemote { Github, Forgejo }` enum replaces `OpenPullRequestRequest.github`; `PublishOptions/RunOptions/StartServices` gain `forgejo: ForgejoRunCreds` (redacting `Debug`); publish selects remote by origin; `merge_forgejo_bridge_env` + `FORGEJO_TOKEN` env injection at sandbox-env build; fork of forgejo origins rejected explicitly; `ForgejoRunCreds` re-exported.
  > 
  > **Server** — `AppState.forgejo_base_url()/forgejo_credentials()` (settings → vault/env token); worker `StartServices` wiring; preflight Forgejo-access check + API-based remote-ref probe + sandbox-spec creds; PR get/merge/close route through `server_forgejo_parts` when the stored link carries `forge`; `/repos/forgejo/{owner}/{name}` route (+demo stub); forgejo row in system integrations.
  > 
  > **OpenAPI/clients** — `getForgejoRepo` path, `IntegrationProvider`/`RepositoryProvider` enums, `PullRequestLink.forge`, `ForgejoIntegrationSettings` (all as replacement types with parity tests); progenitor + axios clients regenerated; `fabro-client.get_forgejo_repo`.
  > 
  > **CLI** — `fabro install forgejo` (prompts/`--url`/`FORGEJO_TOKEN` env, validates via `GET /user`, writes vault + settings); `repo init` classifies remotes provider-aware and probes the right endpoint; run-start builds forgejo creds from the vault.
  > 
  > **Web/docs** — integrations page renders a Forgejo row when the server reports one (older-server tolerant) with tests; new `docs/public/integrations/forgejo.mdx` + nav entry.
  > 
  > **Deviations from the plan:** (1) spec edits pulled forward into Phase 1 since replacement-type parity tests need schema and types together; (2) `run_metadata.rs` needed no change — non-GitHub origins already skip the GitHub metadata writer gracefully rather than erroring; (3) `FORGEJO_TOKEN` is baked into the sandbox base env at initialize (static PAT needs no per-stage token source), avoiding a wide `EngineServices` ripple; (4) `runs/mod.rs` picked up a mechanical `Box::pin` (clippy `large_futures` crossed the threshold from the new field).


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

Review the change for unnecessary complexity only.

Look for: abstraction with a single caller, configuration nobody sets, dead or
unreachable code, duplicated logic that already exists elsewhere in this
repository, indirection that makes the change harder to follow, and scope the
change added beyond what the goal asked for.

For each finding say what to delete or collapse, and what the simpler version
looks like.

Do not report style preferences, formatting, or naming you merely dislike. Only
report complexity that will cost a future reader real time.

If the diff is appropriately simple, say so plainly.
