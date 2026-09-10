# Implementation Plan (rev 2) — Forgejo integration (tier b, single instance + provider tag)

**Changes from the critiqued revision, and why:**
1. **Added the `[run.scm] provider` coupling (blocking defect #1).** `fabro-manifest/src/lib.rs:434` (`configured_repo_origin_url`) returns `None` for any `provider` other than `"github"` — silently. This function feeds `build_legacy_git_context` (line 217) and the repo-info builders, i.e. the settings-pinned repo path for CLI and server runs. The plan now extends it to `"forgejo"`, threads the instance URL through the manifest-build entry point, and errors (instead of returning `None`) when `provider = "forgejo"` but no instance URL is configured.
2. **Made the tier-(b) server path automatically verified (blocking defect #2).** Added mock-based integration tests for `run_manifest` preflight with a forgejo target and for the automation materializer's forgejo remote resolution, and promoted the Docker Forgejo round-trip from "optional" to a required gate at the end of the server stage.
3. **Audit list completed (advisory).** Added `workflow/src/git.rs` (verified: only `normalize_repo_origin_url` for display sanitization — push functions are URL-generic) and `fabro-cli/src/commands/repo/init.rs` (verified: lines 196–197 hard-fail `fabro repo init` in a forgejo clone). `sandbox_git.rs` verified to have **no** `fabro_github` usage — removed from the audit. `diagnostics.rs` added as a small named step (PAT connectivity probe when enabled).

**Authoritative decisions:** tier (b) = CLI + server-side (run targets, automations, sandbox clone, repo picker, integration status, doctor; no browser login, no webhooks). Addressing = one configured instance (`[server.integrations.forgejo] url`) + `provider` tag defaulting `github`; GitHub coexists.

**Forgejo API facts the code must respect** (verified against Forgejo docs): REST under `{url}/api/v1`, `Authorization: token <PAT>`, PR URLs use `/pulls/` (plural), PR close via the issues endpoint (`PATCH /repos/{o}/{r}/issues/{n}`), PR create has no draft flag (WIP title prefix is the convention), no GitHub-App/installation-token equivalent.

---

## Files and changes

### A. Foundation types, settings, env

1. `lib/foundation/fabro-types/src/repository.rs` — add `pub enum ScmProvider { Github, Forgejo }` (strum `Display/EnumString/IntoStaticStr`, serde lowercase, `#[default] Github`); add `Forgejo` to display-only `RepositoryProvider` with `repository_provider_with(origin, forgejo_host: Option<&str>)`; existing `repository_provider` wraps it with `None`.
2. `lib/foundation/fabro-types/src/run_intent.rs` — `GitRunTarget` gains `#[serde(default, skip_serializing_if = "is_default")] pub provider: ScmProvider` (github payloads stay byte-identical); validation core becomes `validate_with_scm(forgejo_url: Option<&str>)` with `validate()` as a `None`-passing wrapper; forgejo origin_url = `{forgejo_url}/{owner}/{repo}`; `None` URL → new `GitCoordinateValidationError::ForgejoUnconfigured`. Slug grammar reuses `GitHubRepositorySlug` (charset-compatible; rename = ~40 files of churn).
3. `lib/foundation/fabro-types/src/pull_request.rs` — `PullRequestLink` gains `provider: ScmProvider` + `origin: Option<String>` (required iff forgejo); `html_url()`: github → `.../pull/N`, forgejo → `{origin}/.../pulls/N`; conditional serialize (wire-additive), deserialize validates the forgejo `/pulls/` shape parsed from the URL itself (round-trips with no settings); `from_forgejo_url`.
4. `lib/foundation/fabro-types/src/settings/server.rs` — `ServerIntegrationsSettings` gains `#[serde(default)] pub forgejo: ForgejoIntegrationSettings`; new `ForgejoIntegrationSettings { enabled: bool, url: Option<String> }`, `Default` disabled.
5. `lib/foundation/fabro-types/src/system_integrations.rs` — `IntegrationProvider::Forgejo`.
6. `lib/foundation/fabro-types/src/lib.rs` — re-export `ScmProvider`, `ForgejoIntegrationSettings`.
7. `lib/foundation/fabro-static/src/env_vars.rs` — `EnvVars::FORGEJO_TOKEN` (+ sorted list entry).
8. `lib/foundation/fabro-static/src/secret_registry.rs` — register `FORGEJO_TOKEN` alongside `GITHUB_TOKEN` (token-secret set; scrubbing/redaction).
9. `lib/foundation/fabro-config/src/layers/server.rs` — `ServerIntegrationsLayer` gains `pub forgejo: Option<ForgejoIntegrationSettings>`.
10. `lib/foundation/fabro-config/src/resolve/server.rs` — resolve `forgejo`; error when `enabled` + missing `url`; normalize url (trim trailing `/`, require `https` except localhost).

### B. New crate `lib/components/fabro-forgejo`

11. `lib/components/fabro-forgejo/Cargo.toml` (+ root `Cargo.toml` members) — deps: `fabro-http`, `fabro-redact`, `fabro-static`, `fabro-types`, `anyhow`, `serde`, `serde_json`, `thiserror`, `tokio`; feature `test-support`.
12. `lib/components/fabro-forgejo/src/lib.rs` — `ForgejoContext { token, base_url }`; local `HttpClient` trait mirroring `fabro_github`'s (deliberate small duplication per parallel-crate decision) + impl for `fabro_http::HttpClient`; `parse_owner_repo(instance_url, repo_url)`, `repo_https_url`, `normalize_origin_url`, `embed_token_in_url` (username `fabro`, password = token; redacted display); PR ops: `find_open_pull_request` (list open PRs, match base/head client-side — do not rely on a `head` query param), `create_pull_request` (draft → `WIP: ` prefix), `get_pull_request`, `merge_pull_request` (POST `/pulls/{n}/merge`, `Do` mapped from `MergeStrategy`), `close_pull_request` (PATCH `/issues/{n}` `{"state":"closed"}`), `branch_head_sha` (GET `/branches/{b}` → `commit.id`), `get_repo` (default_branch/private/permissions); `PullRequestApiError`-shaped error (`NotFound` | `Other`).
13. `lib/components/fabro-forgejo/src/test_support.rs` + `src/tests_mock.rs` — mock client mirroring `fabro-github`'s `MockHttpClient`.
14. `lib/components/fabro-forgejo/tests/integration.rs` — every op: success, 404/401/403 mapping, malformed JSON, URL-parse table, token-embed redaction.
15. `lib/components/fabro-forgejo/tests/live_access.rs` — `#[e2e_test(live("FABRO_FORGEJO_TOKEN"))]` against `FABRO_FORGEJO_URL` (ignored unless live mode).

### C. OpenAPI + generated clients

16. `docs/public/api-reference/fabro-api.yaml` — `GitRunTarget` + optional `provider` (`enum: [github, forgejo]`); `PullRequestLink` + optional `provider`/`origin`; `IntegrationProvider` + `forgejo`; new `GET /api/v1/repos/forgejo/{owner}/{name}` (same response shape as the github endpoint). Then `cargo build -p fabro-api` (progenitor; both modified types are already `with_replacement`) and `cd lib/packages/fabro-api-client && bun run generate`. Extend the `fabro-api` type-identity/JSON-parity tests for the two modified replaced types.

### D. Workflow engine

17. `lib/components/fabro-workflow/src/pipeline/types.rs` — options struct gains `forgejo: Option<ForgejoContext>`; new `ScmTarget` enum (`GitHub { owner, repo, ctx }` | `Forgejo { owner, repo, origin, ctx }`).
18. `lib/components/fabro-workflow/src/pipeline/pull_request.rs` — `open_pull_request` resolves `ScmTarget` from origin (forgejo ⇔ host matches instance URL) instead of unconditional `parse_github_owner_repo`; `verify_remote_head`/`reconcile_existing_pull_request` dispatch on it; `build_pr_content` (LLM) unchanged; `CreatedPullRequest` carries provider+origin for the link; forgejo auto-merge → `warn!` + skip.
19. `lib/components/fabro-workflow/src/pipeline/publish.rs` + `initialize.rs` — context selection via `ScmTarget`; github path untouched for github origins.
20. `lib/components/fabro-workflow/src/run_metadata.rs` — meta-branch gate (~298–325) branches on provider; forgejo writer uses a static-token `CredentialSource` (new `StaticTokenSource`; refresh = identity).
21. `lib/components/fabro-workflow/src/git_bridge.rs` — github origins unchanged; forgejo origins get `credential.https://{host}.helper` reading `$FORGEJO_TOKEN` (helper definition exported from `fabro-forgejo`, same secret-free `!f()` pattern).
22. `lib/components/fabro-workflow/src/services.rs` — `resolve_workflow_env` (~356): forgejo static token → `FORGEJO_TOKEN` env when the run's origin is the instance; options structs (~241/~344) gain the field.
23. **Audit step (complete `fabro_github` call-site list):** `run_options.rs`, `operations/start.rs`, `operations/fork.rs`, `operations/retry.rs`, `handler/command.rs`, `handler/llm/acp.rs`, `git.rs` (display sanitization only — make normalization provider-aware for correct origin display), `sandbox_git_runtime.rs`. Each site branches on provider or is proven unreachable for forgejo runs; GitHub-specific paths (installation-token minting) must be unreachable for forgejo runs.

### E. Sandbox clone (Docker/Daytona)

24. `lib/components/fabro-sandbox/src/clone_source.rs` — `CloneDecision::Forgejo { origin_url, branch, tag, commit_sha }`; extract `repo_layout(owner, repo, …)` shared with `github_repo_layout`; origin classification by instance host (threaded via spec); update the "GitHub origins only" error to name both providers.
25. `lib/components/fabro-sandbox/src/sandbox_spec.rs` + `provider.rs` + `docker.rs` + `daytona/mod.rs` — spec carries `forgejo: Option<ForgejoCloneConfig { instance_url, token }>`; clone via `embed_token_in_url`; exact-commit path unchanged.
26. `lib/components/fabro-sandbox/src/push_credentials.rs` — static-PAT remote-credential action for forgejo origins (initial embed + `set-url` before push, same generation-tracking sequence; refresh = no-op success).

### F. Server

27. `lib/apps/fabro-server/src/server.rs` — `AppState::forgejo_credentials()` (vault `FORGEJO_TOKEN` → `Option<ForgejoContext>`; `None` when disabled); route `GET /api/v1/repos/forgejo/{owner}/{name}`; startup snapshot loads forgejo settings alongside github (~2602 area).
28. `lib/apps/fabro-server/src/server/handler/system.rs` — integrations handler appends the forgejo row (disabled / missing `server.integrations.forgejo.url` + `FORGEJO_TOKEN` / configured); `get_forgejo_repo` mirrors `get_github_repo` shape (`install_url` = `{url}/{owner}/{repo}/settings`).
29. `lib/apps/fabro-server/src/server/handler/runs.rs` — line ~629: `validate_with_scm(forgejo_url from state)`; forgejo targets rejected 422 (with remediation naming settings path + vault key) when integration disabled or token missing.
30. `lib/apps/fabro-server/src/run_manifest.rs` — preflight (~440–520): forgejo-targeted runs get repository-access via `fabro_forgejo::get_repo`, a `FORGEJO_TOKEN` check mirroring `run_github_token_check`, PAT clone-auth; repo-summary parse (~602–607) provider-aware; `check_remote_ref` forgejo branch via `branch_head_sha`; the "GitHub origins only" gate (~807–814) admits forgejo origins when configured.
31. `lib/apps/fabro-server/src/automation_materializer.rs` — resolver carries `forgejo: Option<ForgejoContext>` + `forgejo_url`; forgejo remotes resolve `clone_url = embed_token(repo_https_url)` + static-token `GitAuthConfig`; validation (~235) passes the forgejo URL.
32. `lib/apps/fabro-server/src/git_checkout.rs` — `forgejo_clone_url(instance_url, owner, repo)`; `forgejo_git_auth(token)` (basic-auth `http.extraheader`, existing base64 helper); `resolve_git_read_auth_config` gains a forgejo arm.
33. `lib/apps/fabro-server/src/spawn_env.rs` — `FORGEJO_TOKEN` passed to workers only for forgejo-targeted runs.
34. `lib/apps/fabro-server/src/server/handler/pull_requests.rs` — live-detail/merge/close dispatch on link provider (forgejo → the new crate against `link.origin`); payload conversion into `PullRequestDetails`.
35. `lib/apps/fabro-server/src/diagnostics.rs` — when forgejo enabled: PAT probe of `{url}/api/v1/user` in the diagnostics report (mirrors the github token probe at ~351–399); absent when disabled.

### G. Manifest & CLI

36. `lib/components/fabro-manifest/src/lib.rs` — **(critique fix #1)**
    - `configured_repo_origin_url` (~434): accept `provider = "forgejo"`; return `{forgejo_url}/{owner}/{repo}` and signal forgejo so downstream targets carry `provider: Forgejo`; **error (not silent `None`)** when provider is forgejo and no instance URL is available — signature becomes `Result<Option<ConfiguredOrigin>, String>` (or equivalent), callers surface the error through manifest building.
    - The manifest-build entry point (~217 caller chain) gains the forgejo instance URL parameter; CLI and server callers pass it from resolved server settings when the integration is enabled.
    - Origin observation `github_run_target` (~417): generalize to match the configured instance host (URL param threaded from the CLI caller), producing forgejo-provider targets for local forgejo clones.
37. `lib/apps/fabro-cli/src/shared/forgejo.rs` (new) — `build_forgejo_credentials(server_ns, vault)` (env→vault `FORGEJO_TOKEN`, matching `shared/github.rs` order).
38. `lib/apps/fabro-cli/src/commands/run/runner.rs` — `maybe_build_github_credentials` (~1102) / `requires_github_credentials` (~1139): forgejo-targeted runs build the forgejo ctx and skip the github requirement (truth-table tests at ~1739 extended); pass the forgejo URL through to the manifest builder per item 36.
39. `lib/apps/fabro-cli/src/commands/doctor.rs` — offline forgejo check: not configured → pass; `enabled` + missing `url` → error; `enabled` + url + missing `FORGEJO_TOKEN` → error with remediation. No network (matches github doctor convention).
40. `lib/apps/fabro-cli/src/commands/repo/init.rs` — **(audit addition)** lines ~196–197: origin detection branches on the configured instance host → forgejo slug parse + `[run.scm] provider = "forgejo"` written into the scaffold, instead of failing `parse_github_owner_repo`.

### H. Web

41. `apps/fabro-web/app/components/automation-form.tsx` (+ test) — provider toggle (shown only when the integrations status endpoint reports forgejo configured) calling the regenerated client's forgejo repo lookup; default github.
42. `apps/fabro-web/app/components/pull-request-chip.tsx` — verify it renders `html_url` (expected no change; adjust only if it re-parses github URLs).

### I. Docs

43. `docs/public/integrations/forgejo.mdx` (new) — PAT scopes (`write:repository`, `write:issue`), settings, `provider = "forgejo"` on run targets/`[run.scm]`, capability table vs GitHub, limitations (no login/webhooks/auto-merge, WIP-draft, static token blast radius, single instance, system CA only).
44. `docs/public/docs.json` — `integrations/forgejo` after `integrations/github`.
45. `docs/public/changelog/2026-09-10.mdx` — entry.

---

## Order of work

1. **A** (types/settings/env) → `cargo nextest run -p fabro-types -p fabro-static -p fabro-config`
2. **B** (fabro-forgejo) → `cargo nextest run -p fabro-forgejo`
3. **C** (OpenAPI → regen → TS client) — before server handlers, per repo API workflow
4. **D** (workflow pipeline) → `cargo nextest run -p fabro-workflow`
5. **E** (sandbox) → `cargo nextest run -p fabro-sandbox`
6. **F** (server) → `cargo nextest run -p fabro-server` (conformance included)
7. **G** (manifest + CLI) → `cargo nextest run -p fabro-cli -p fabro-manifest`
8. **H** (web) → `bun test && bun run typecheck`
9. **I** (docs)
10. Full-gate pass (below) after each of steps 4–7 and once at the end.

## Verification

**Automated (per stage, as ordered above), plus:**
- Gates after each stage: `cargo +nightly-2026-04-14 fmt --check --all` and `cargo +nightly-2026-04-14 clippy --workspace --all-targets -- -D warnings`.
- `cargo build --workspace` (no `test-support` leakage).
- **Backward-compat unit tests written with stage A:** existing GitHub JSON fixtures for `GitRunTarget`/`PullRequestLink` deserialize unchanged; `validate()` with no forgejo URL is byte-identical to today for github targets.
- **(Critique fix #2) Automated tier-(b) wiring tests:**
  - `lib/apps/fabro-server/src/server/tests.rs` (alongside existing preflight tests, using the crate's httpmock/mock-client pattern): a forgejo-targeted RunIntent validates with the URL configured, fails validation without it; preflight repository-access check succeeds against a mocked `{url}/api/v1/repos/{o}/{r}`; PAT clone-auth is selected (no github credential path taken).
  - `automation_materializer.rs` tests (existing `with_remote_resolver` mock pattern at ~546–584): a forgejo `GitRunTarget` resolves a token-embedded clone URL; github targets keep resolving exactly as before.
  - `fabro-manifest` test: `[run.scm] provider = "forgejo"` produces a forgejo-origin git context when the URL is configured and an explicit error when not (guards the silent-`None` regression the critique found).
  - `fabro-cli/src/commands/repo/init.rs` test: scaffolding in a forgejo clone writes `provider = "forgejo"`.
- **Required manual gate at the end of stage F (no longer optional):** `docker run -d -p 3001:3000 codeberg.org/forgejo/forgejo`, PAT, `[server.integrations.forgejo] url = "http://localhost:3001"`, clone a repo, server-side run → assert preflight passes, run branch + meta branch push, auto-PR with WIP prefix; then CLI `fabro run` in the same clone for the local path.
- Optional live e2e: `set -a && source .env && set +a && cargo nextest run -p fabro-forgejo --profile e2e --run-ignored only` with `FABRO_FORGEJO_URL`/`FABRO_FORGEJO_TOKEN`.

## Deliberately not doing

- Browser login (`AuthMethod::Forgejo`) — human chose tier (b); instance-wide Forgejo OAuth2 apps are admin-scoped.
- Webhooks — today's GitHub handler only verifies HMAC and logs; nothing to mirror.
- Projects tracker — no Forgejo equivalent of Projects V2 GraphQL.
- Auto-merge on Forgejo — upstream semantics differ; warn-and-skip.
- Gitea branding/support — forgejo naming only.
- Multiple instances / exclusive switch — human chose single instance + provider tag.
- Install wizard changes — URL+token setup via manual settings + doctor + docs.
- Custom CA bundles — system trust store only, documented.
- Renaming `GitHubRepositorySlug` — reused for slug grammar; churn without behavior.
- `[run.scm.forgejo]` settings leaf — provider + server-level URL covers it; no per-run forgejo knobs exist.
- Migrations / new event variants — all changes additive; existing PR/git events carry provider-neutral payloads.