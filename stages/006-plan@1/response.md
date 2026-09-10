# Implementation Plan — Forgejo integration (tier b, single instance + provider tag)

**Authoritative decisions from the human:** v1 covers CLI + server-side surfaces (run targets, automations, sandbox clone, repo picker, integration status, doctor). Browser login is out. Addressing is one configured instance (`[server.integrations.forgejo] url`) plus a `provider` tag on run targets defaulting to `github`; GitHub and Forgejo coexist.

**Design spine:** a parallel `fabro-forgejo` crate (PAT-only, following the `fabro-github`/`fabro-slack` side-by-side convention — not a shared-trait refactor), an additive `provider` discriminator on `GitRunTarget`, and a provider-aware `PullRequestLink`. No new event variants, no migrations, no auth-method changes.

Key Forgejo API facts the code must respect (verified against Forgejo docs): REST under `{url}/api/v1`, `Authorization: token <PAT>`, PR URLs use `/pulls/` (plural, unlike GitHub's `/pull/`), PR close goes through the issues endpoint (`PATCH /repos/{o}/{r}/issues/{n}`), no GitHub-App/installation-token equivalent, no draft flag on PR creation (WIP title prefix is the convention).

---

## Files and changes

### A. Foundation types, settings, env (no behavior change for GitHub)

**Create: none. Modify:**

1. `lib/foundation/fabro-types/src/repository.rs`
   - Add `pub enum ScmProvider { Github, Forgejo }` — strum `Display/EnumString/IntoStaticStr`, serde `rename_all = "lowercase"`, `#[default] Github`. This is the wire/storage discriminator (distinct from the display-only `RepositoryProvider`).
   - Add `Forgejo` variant to `RepositoryProvider` and extend `repository_provider(origin_url)` → `repository_provider_with(origin_url, forgejo_host: Option<&str>)`: origins on the configured instance host classify as `Forgejo`; existing callers keep a wrapper passing `None`. Display metadata only.

2. `lib/foundation/fabro-types/src/run_intent.rs`
   - `GitRunTarget` gains `#[serde(default, skip_serializing_if = "is_default")] pub provider: ScmProvider` (omitted on the wire when `github`, so existing payloads are byte-identical).
   - Rename the validation core: `validate()` → `validate_with_scm(forgejo_url: Option<&str>)`. `validate()` stays as a wrapper passing `None`. Slug grammar validation reuses `GitHubRepositorySlug` (charset-compatible; renaming the type is churn — documented compromise). For `provider = Forgejo`: origin_url = `{forgejo_url}/{owner}/{repo}`; `None` URL → new `GitCoordinateValidationError::ForgejoUnconfigured` ("provider \"forgejo\" requires server.integrations.forgejo.url"). GitHub path byte-identical to today.
   - `ValidatedGitRunTarget` exposes `provider()`.

3. `lib/foundation/fabro-types/src/pull_request.rs`
   - `PullRequestLink` gains `provider: ScmProvider` and `origin: Option<String>` (forgejo instance origin; `None` for github). `html_url()`: github → current `https://github.com/.../pull/N`; forgejo → `{origin}/{owner}/{repo}/pulls/{N}`.
   - Custom Serialize emits `provider`/`origin` only when non-default (wire-additive). Deserialize: with `origin` present, validate `html_url` against the forgejo `/pulls/` shape parsed from the URL itself (round-trips with no settings); else the existing github validation. `from_forgejo_url(url)` mirrors `from_github_url`.

4. `lib/foundation/fabro-types/src/settings/server.rs`
   - `ServerIntegrationsSettings` gains `#[serde(default)] pub forgejo: ForgejoIntegrationSettings` (field order after `slack`).
   - New `pub struct ForgejoIntegrationSettings { pub enabled: bool, pub url: Option<String> }`, `Default` = disabled. Mirrors `GithubIntegrationSettings` shape.

5. `lib/foundation/fabro-types/src/system_integrations.rs` — `IntegrationProvider` gains `Forgejo` (strum derives already present).

6. `lib/foundation/fabro-types/src/lib.rs` — re-export the new types (`ScmProvider`, `ForgejoIntegrationSettings`).

7. `lib/foundation/fabro-static/src/env_vars.rs` — add `pub const FORGEJO_TOKEN: &'static str = "FORGEJO_TOKEN"` to `EnvVars` and its sorted list.

8. `lib/foundation/fabro-static/src/secret_registry.rs` — register `FORGEJO_TOKEN` in the token-secret set (alongside `GITHUB_TOKEN`), so scrubbing/redaction covers it.

9. `lib/foundation/fabro-config/src/layers/server.rs` — `ServerIntegrationsLayer` gains `pub forgejo: Option<ForgejoIntegrationSettings>` (reuses the types-layer struct; matches how `github` is layered).

10. `lib/foundation/fabro-config/src/resolve/server.rs` — `resolve_integrations()` resolves the `forgejo` field; validation error when `enabled = true` and `url` missing (`server.integrations.forgejo.url` "must be set when server.integrations.forgejo is enabled"); normalize `url` (trim trailing `/`, require `https` scheme unless localhost for dev/tests — error otherwise).

### B. New crate `lib/components/fabro-forgejo`

**Create:**

11. `lib/components/fabro-forgejo/Cargo.toml` — deps: `fabro-http`, `fabro-redact`, `fabro-static`, `fabro-types` (`ScmProvider`, `MergeStrategy`), `anyhow`, `serde`, `serde_json`, `thiserror`, `tokio`; dev/test: `tokio-test`, `fabro-test`. Features: `test-support`. Add crate to workspace members in root `Cargo.toml` (components list, alphabetical).

12. `lib/components/fabro-forgejo/src/lib.rs` — PAT-only client, ~1/4 the size of `fabro-github`:
    - `ForgejoContext { token, base_url }` (`base_url` = instance root; API root derived as `{base_url}/api/v1`). Clone + Debug-with-redaction.
    - Local `HttpClient` trait mirroring `fabro_github::HttpClient` (same method shape; deliberate small duplication per the parallel-crate decision) + impl for `fabro_http::HttpClient`. Auth header `Authorization: token {t}`, `User-Agent: fabro`.
    - URL handling: `parse_owner_repo(instance_url, repo_url) -> Result<(String, String)>` (accepts https with/without `.git`, ssh `git@host:o/r` and `ssh://git@host/o/r` for the instance host); `repo_https_url(instance_url, owner, repo)`; `normalize_origin_url(instance_url, origin)`; `embed_token_in_url(url, token) -> DisplaySafeUrl` (username `fabro`, password = token — Forgejo accepts basic auth with token as password).
    - PR ops (all return/accept a small `ForgejoPullRequest` mirroring the fields `PullRequestGithubDetail` needs — number/title/body/state/merged/draft/head/base/html_url): `find_open_pull_request` (GET `/repos/{o}/{r}/pulls?state=open&head={owner}:{branch}`, match base), `create_pull_request` (POST `/repos/{o}/{r}/pulls`; `draft: true` maps to `WIP: ` title prefix — Forgejo has no draft flag), `get_pull_request`, `merge_pull_request` (POST `/repos/{o}/{r}/pulls/{n}/merge` with `Do: "merge"|"rebase"|"squash"` mapped from `MergeStrategy`), `close_pull_request` (PATCH `/repos/{o}/{r}/issues/{n}` `{"state":"closed"}`), `branch_head_sha` (GET `/repos/{o}/{r}/branches/{b}` → `commit.id`), `get_repo` (GET `/repos/{o}/{r}` → default_branch/private/permissions — backs the repo-access endpoint and preflight).
    - Status-code error mapping mirroring `PullRequestApiError` (`NotFound { owner, repo, number }` | `Other`).
    - `ForgejoCredentials` = newtype over `SecretString` token; `from_env_or_vault(vault)` helper lives in the CLI (C/G), not here — this crate takes a resolved token.

13. `lib/components/fabro-forgejo/src/test_support.rs` (`#[cfg(any(test, feature = "test-support"))]`) and `src/tests_mock.rs` (`#[cfg(test)]`, `pub(crate)`) — mock `HttpClient` mirroring `fabro-github`'s `MockHttpClient` (`.on(method, path, status, body)`).

14. `lib/components/fabro-forgejo/tests/integration.rs` — mock-based tests for every op: success, 404 mapping, 401/403, malformed JSON, URL parsing table, token-embed redaction (`redacted_string()` hides the token).

15. `lib/components/fabro-forgejo/tests/live_access.rs` — `#[e2e_test(live("FABRO_FORGEJO_TOKEN"))]` contract test against `FABRO_FORGEJO_URL` (repo/branch/PR round-trip in a disposable repo); ignored unless `FABRO_TEST_MODE=live`.

### C. OpenAPI spec + generated clients (before server handlers, per repo API workflow)

16. `docs/public/api-reference/fabro-api.yaml`
    - `GitRunTarget` schema: add optional `provider` (`enum: [github, forgejo]`, `default: github`).
    - `PullRequestLink` schema: add optional `provider` + optional `origin` (forgejo instance origin, required when `provider = forgejo`).
    - `IntegrationProvider` enum: add `forgejo`.
    - New path `GET /api/v1/repos/forgejo/{owner}/{name}` — same response schema as the github repo-lookup endpoint (accessible/name/default_branch/private/permissions), `install_url` → repo settings URL on the instance. No new replaceable schema types (handler returns `serde_json::json!` like the github one — mirror exactly).
    - Then `cargo build -p fabro-api` (progenitor regen; `GitRunTarget`/`PullRequestLink` are `with_replacement` types so the new fields flow through without new replacements) and `cd lib/packages/fabro-api-client && bun run generate`.
    - `lib/foundation/fabro-api` tests: extend the existing type-identity/JSON-parity test list for the two modified replaced types (per CLAUDE.md `with_replacement` rule).

### D. Workflow engine (run execution: push, meta-branch, auto-PR, sandbox env)

17. `lib/components/fabro-workflow/src/pipeline/types.rs` — the options struct carrying `github_app: Option<GitHubCredentials>` (line ~416) and `github_integration` (~254) gains `forgejo: Option<ForgejoContext>`; add `ScmTarget` enum (`GitHub { owner, repo, ctx }` | `Forgejo { owner, repo, origin, ctx }`) used by the PR pipeline.

18. `lib/components/fabro-workflow/src/pipeline/pull_request.rs`
    - `open_pull_request`: resolve `ScmTarget` from origin URL (forgejo origin ⇔ host matches configured instance URL, threaded in via options) instead of unconditionally `parse_github_owner_repo`; `verify_remote_head` and `reconcile_existing_pull_request` dispatch on the enum (forgejo uses `branch_head_sha`/`find_open_pull_request` from the new crate).
    - LLM PR-content generation (`build_pr_content`) unchanged — provider-agnostic.
    - `CreatedPullRequest` carries provider + origin so the stored `PullRequestLink` is built correctly; draft handling for forgejo = WIP prefix (strip it when reconciling).
    - Auto-merge: forgejo path logs `warn!` "auto-merge is not supported by the Forgejo integration" and skips (no `enable_auto_merge` call).

19. `lib/components/fabro-workflow/src/pipeline/publish.rs` + `initialize.rs` — context selection: `ScmTarget::from(origin_url, github_ctx, forgejo_ctx)`; github path untouched when origin is github.com.

20. `lib/components/fabro-workflow/src/run_metadata.rs` — `meta_branch` gate (lines ~298–325): branch on provider. GitHub: unchanged (installation-token auth). Forgejo: writer with a static-token credential source (new `StaticTokenSource` implementing the `fabro_auth::CredentialSource` trait the file already uses via `GitHubAuthProvider` — token never expires mid-run, refresh is identity).

21. `lib/components/fabro-workflow/src/git_bridge.rs` — bridge config stays github-scoped for github origins; for forgejo origins emit `credential.https://{forgejo_host}.helper` with a helper reading `$FORGEJO_TOKEN` (same secret-free `!f()` pattern as `GITHUB_CREDENTIAL_HELPER`, definition in `fabro-forgejo`, not duplicated inline). No `insteadOf` rewriting for forgejo (that rewrites Fabro's own repo).

22. `lib/components/fabro-workflow/src/services.rs` — `resolve_workflow_env` (line ~356): alongside `github_token` → `GITHUB_TOKEN`, add static forgejo token → `FORGEJO_TOKEN` when the run's origin is the configured instance. Options structs (~241, ~344) gain the forgejo token field.

23. `lib/components/fabro-workflow/src/run_options.rs`, `operations/start.rs`, `operations/fork.rs`, `operations/retry.rs`, `handler/command.rs`, `handler/llm/acp.rs` — audit step: each `fabro_github` call site in these files branches on provider (mostly credential construction and origin normalization; `start.rs` target fixtures stay github). Where a call is GitHub-specific by nature (e.g. installation-token minting), forgejo runs must not reach it.

### E. Sandbox clone (Docker/Daytona)

24. `lib/components/fabro-sandbox/src/clone_source.rs`
    - `CloneDecision` gains `Forgejo { origin_url, branch, tag, commit_sha }` (mirror of the `GitHub` variant).
    - Extract the layout math from `github_repo_layout` into `repo_layout(owner, repo, workspace_root, repos_root)`; both providers call it. Origin classification: host matches the configured forgejo instance URL (threaded from spec — `sandbox_spec.rs` already carries clone origin; add the instance URL to the spec).
    - Update the "GitHub repository origins only" error to name both providers.

25. `lib/components/fabro-sandbox/src/sandbox_spec.rs` + `provider.rs` + `docker.rs` + `daytona/mod.rs` — spec carries `forgejo: Option<ForgejoCloneConfig { instance_url, token }>`; clone commands use `fabro_forgejo::embed_token_in_url` for forgejo origins; exact-commit path reuses the existing init/fetch flow unchanged.

26. `lib/components/fabro-sandbox/src/push_credentials.rs` — `build_token_source` stays github (installation tokens); add the static-PAT remote-credential action for forgejo origins: initial embed at clone, `set-url` before push reuses the same token, refresh is a no-op success (PATs don't expire on the run timescale) — keep the generation-tracking sequence identical.

### F. Server

27. `lib/apps/fabro-server/src/server.rs`
    - `AppState` gains `forgejo_credentials()` accessor mirroring `github_credentials` (line ~1575): vault `FORGEJO_TOKEN` → `Option<ForgejoContext>`, `None` when integration disabled/absent.
    - Route: `GET /api/v1/repos/forgejo/{owner}/{name}` (handler in `handler/system.rs`).
    - `AppState` startup snapshot: load forgejo settings alongside github (`serve.rs` construction site, ~line 2602 area).

28. `lib/apps/fabro-server/src/server/handler/system.rs`
    - `integrations` handler: append forgejo status row (`IntegrationProvider::Forgejo`): disabled / missing-credentials (`server.integrations.forgejo.url`, `FORGEJO_TOKEN`) / configured.
    - `get_forgejo_repo` handler: validate slugs with the existing `validate_github_slug` (rename usage only), call `fabro_forgejo::get_repo`, respond in the same shape as `get_github_repo` (accessible/default_branch/private/permissions; `install_url` = `{url}/{owner}/{repo}/settings`).

29. `lib/apps/fabro-server/src/server/handler/runs.rs` — line ~629: `intent.target.validate()` → `validate_with_scm(forgejo_url)` where `forgejo_url` comes from `state.server_settings()`. Forgejo-targeted runs proceed only when integration enabled + token present (else 422 with remediation naming the settings path and vault key).

30. `lib/apps/fabro-server/src/run_manifest.rs`
    - Preflight (lines ~440–520): when the resolved target's provider is forgejo — repository-access check via `fabro_forgejo::get_repo` (replaces the github parse at ~602–607 for those runs), credential check mirrors `run_github_token_check` (`FORGEJO_TOKEN` present, one API call), clone-auth via PAT instead of `resolve_authenticated_url` (~856).
    - `check_remote_ref` forgejo branch: `fabro_forgejo::branch_head_sha`.
    - The "Clone-based sandboxes currently support GitHub repository origins only" gate (~807–814) becomes provider-aware: forgejo origins allowed when the integration is configured.

31. `lib/apps/fabro-server/src/automation_materializer.rs` — `AutomationGitRemoteResolver::resolve` takes the provider + instance URL (resolver struct gains `forgejo: Option<ForgejoContext>` and `forgejo_url: Option<String>` from settings at construction); forgejo repos resolve `clone_url = embed_token(repo_https_url)` and a static-token `GitAuthConfig`. `prepare_worktree` input carries owner/repo (already generic strings). Validation at line ~235 passes the forgejo URL.

32. `lib/apps/fabro-server/src/git_checkout.rs` — add `forgejo_clone_url(instance_url, owner, repo)` and `forgejo_git_auth(token)` (basic-auth `http.extraheader`, same base64 helper as the github path); `resolve_git_read_auth_config` gains a forgejo arm.

33. `lib/apps/fabro-server/src/spawn_env.rs` — worker env passes `FORGEJO_TOKEN` only for forgejo-targeted runs (registry addition in A8 keeps scrubbing correct elsewhere).

34. `lib/apps/fabro-server/src/server/handler/pull_requests.rs` — live-detail fetch dispatch: link provider forgejo → `fabro_forgejo::get_pull_request` against the link's `origin`; merge/close handlers likewise. `PullRequestDetails` conversion from the forgejo payload type.

### G. CLI

35. `lib/apps/fabro-cli/src/shared/forgejo.rs` (new) — `build_forgejo_credentials(server_ns, vault) -> Result<Option<ForgejoContext>>`: enabled + url → vault/env `FORGEJO_TOKEN` (env→vault order matching `lookup_env_or_vault` in `shared/github.rs`).

36. `lib/apps/fabro-cli/src/commands/run/runner.rs` — `maybe_build_github_credentials` (~1102) and `requires_github_credentials` (~1139): when the resolved target/origin is a forgejo repo, build forgejo ctx instead and skip the github requirement (truth-table test module at ~1739 extended).

37. `lib/apps/fabro-cli/src/commands/doctor.rs` — new check "Forgejo integration": not configured → pass/silent; `enabled` + missing `url` → error (name the settings path); `enabled` + url + missing `FORGEJO_TOKEN` → error with remediation; configured+token → one API call `get_repo` on... no, doctor is offline by convention for github (it checks fields only) — match that: field presence only, no network.

38. `lib/components/fabro-manifest/src/lib.rs` — origin→target observation (`github_run_target`, line ~417): generalize to match the configured forgejo instance host (URL param threaded from the CLI caller, which has resolved settings); produce `GitRunTarget { provider: Forgejo, .. }` so local runs in a forgejo clone carry the right target.

### H. Web

39. `apps/fabro-web/app/components/automation-form.tsx` (+ its test) — repo picker gains a provider toggle shown only when the integrations status endpoint reports forgejo configured; forgejo selection calls the new `getForgejoRepo` client method (regenerated client in C). Default stays github.

40. `apps/fabro-web/app/components/pull-request-chip.tsx` — verify it renders `html_url` from the link (expected: no change needed since the link now round-trips the forgejo URL); adjust only if it hard-codes github parsing.

### I. Docs

41. `docs/public/integrations/forgejo.mdx` (new) — setup (PAT with `write:repository` + `write:issue` scopes), settings table, `provider = "forgejo"` usage on run targets/automations, strategy-matrix-style capability table vs GitHub, explicit limitations (no browser login, no webhooks, no auto-merge, draft = WIP prefix, static token blast radius, single instance, system CA trust only).

42. `docs/public/docs.json` — nav entry `integrations/forgejo` after `integrations/github`.

43. `docs/public/changelog/2026-09-10.mdx` — entry following the dated-file convention.

---

## Order of work

1. **A (types/settings/env)** — everything depends on `ScmProvider` + settings shape. Verify: `cargo nextest run -p fabro-types -p fabro-static -p fabro-config`.
2. **B (fabro-forgejo crate)** — the client everything calls. Verify: `cargo nextest run -p fabro-forgejo`.
3. **C (OpenAPI + regen)** — spec edit → `cargo build -p fabro-api` → TS client regen, before server handlers exist (repo's stated API workflow). Verify: fabro-api type-parity tests.
4. **D (workflow pipeline)** — push/meta-branch/auto-PR/provider dispatch. Verify: `cargo nextest run -p fabro-workflow`.
5. **E (sandbox)** — clone decision, token embed, push creds. Verify: `cargo nextest run -p fabro-sandbox` (docker-gated tests run where Docker is available).
6. **F (server)** — validation, preflight, repos endpoint, automations, PR supervision. Verify: `cargo nextest run -p fabro-server` (includes OpenAPI conformance, which will fail until the spec/routes agree — that's the check working).
7. **G (CLI)** — credentials, run gating, doctor, manifest observation. Verify: `cargo nextest run -p fabro-cli`.
8. **H (web)** — automation form provider toggle. Verify: `cd apps/fabro-web && bun test && bun run typecheck`.
9. **I (docs)** — page, nav, changelog.
10. **Full-gate pass** (below) after each of steps 4–7 as they land, and once at the end.

## Verification

- **Per-crate as listed above** with `cargo nextest run -p <crate>`.
- **Formatting/lint gates** (CI parity): `cargo +nightly-2026-04-14 fmt --check --all` and `cargo +nightly-2026-04-14 clippy --workspace --all-targets -- -D warnings` after each stage; fix immediately, not at the end.
- **Workspace builds without test-support**: `cargo build --workspace` (per testing-strategy boundary rules).
- **Backward-compat assertions** as unit tests, written with the code: existing GitHub JSON fixtures for `GitRunTarget`/`PullRequestLink` deserialize unchanged (`provider` absent → github); `validate()` with no forgejo URL behaves exactly as today for github targets.
- **Web**: `bun test`, `bun run typecheck`.
- **Live verification (manual, optional)**: `docker run -d -p 3001:3000 codeberg.org/forgejo/forgejo`, create PAT, set `[server.integrations.forgejo] url="http://localhost:3001"`, clone a repo from it, `fabro run` → assert run branch push, meta-branch push, auto-PR with WIP prefix; and `set -a && source .env && set +a && cargo nextest run -p fabro-forgejo --profile e2e --run-ignored only` with `FABRO_FORGEJO_URL`/`FABRO_FORGEJO_TOKEN`.
- **Regression signal**: any test touching `parse_github_owner_repo`, `clone_source`, `run_manifest` preflight, or the github integration status must pass unmodified except where provider-dispatch was explicitly added.

## Deliberately not doing

- **Browser login (`AuthMethod::Forgejo`)** — human chose tier (b), and instance-wide Forgejo OAuth2 apps are admin-scoped; revisiting needs its own design.
- **Webhooks** — the existing GitHub webhook handler only verifies HMAC and logs; there is no behavior to mirror.
- **Projects tracker** — Forgejo has no Projects V2 GraphQL API.
- **Auto-merge on Forgejo** — upstream merge-API semantics differ; log-and-skip rather than guess.
- **Gitea branding/support** — forgejo naming only; the client is API-compatible in practice but nothing claims or tests Gitea.
- **Multiple instances / exclusive switch** — human chose single-instance + provider tag.
- **Install wizard changes** — setup is URL + token; manual settings + doctor check + docs page (avoids churning the wizard for everyone else).
- **Custom CA bundle support** — system trust store only; documented as a limitation.
- **Renaming `GitHubRepositorySlug`** — reused for forgejo slug grammar; a rename touches ~40 files for zero behavior.
- **Migrations** — all wire/settings changes are additive with serde defaults.
- **New event variants** — existing PR/git events carry provider-neutral payloads; the events strategy doc's cost of new variants isn't justified here.