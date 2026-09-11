I have verified every integration point against the current code. Here is the implementation plan.

---

# Implementation Plan: Forgejo integration (scope B, single instance, PAT-only, additive crate, inert webhooks)

## Governing decisions (from the human's answers)

- **Scope B**: credentials, clone into Docker/Daytona, PR lifecycle (API + web), settings, sandbox token (`run.integrations.forgejo` → `FORGEJO_TOKEN`), inert webhook route, install flow (CLI + web), diagnostics. No OAuth2 login, no automations, no tracker.
- **Single instance per server** — `[server.integrations.forgejo] url` + one PAT. `owner/repo` slugs are instance-scoped; no host-qualified identity; **no change to `GitRunTarget`/`RunTarget` wire format**. Forgejo runs enter through the legacy manifest lane (full `git.origin_url`) and local working copies — the intent lane's GitHub-slug grammar stays GitHub-only.
- **PAT only** — `Authorization: token <PAT>`; no refresh machinery needed (static token).
- **Additive** — new `fabro-forgejo` crate, explicit dispatch; GitHub code paths untouched.
- **Webhooks inert** — verify HMAC + log, exactly like `github_webhook` today.

One deliberate wire addition, justified: `PullRequestLink` gains an optional `forge` field (instance base URL) so Forgejo PR links render correct URLs in CLI/web. It is `#[serde(default, skip_serializing_if = "Option::is_none")]`; existing persisted runs (no field) are unaffected, and `html_url()` keeps producing `github.com/...` for them. Without this, every Forgejo PR link in the UI would point at github.com. This does not touch run-target identity, which is what the "no wire change" answer was about.

## Phase 0 — Foundations

1. **Create `lib/components/fabro-forgejo/Cargo.toml`** — mirror `fabro-github`'s manifest (deps: anyhow, serde, serde_json, fabro-http, fabro-redact, fabro-static, fabro-types, chrono, tracing, thiserror, tokio; dev: fabro-macros, fabro-test, tokio test-util; `test-support` feature). Workspace membership is automatic (`lib/components/*` glob).
2. **Create `lib/components/fabro-forgejo/src/lib.rs`**:
   - `ForgejoInstance` — validated HTTPS base URL (reject non-HTTPS, credentials, query/fragment; normalize trailing slash, optional subpath). Constructor returns `Result`; `Display`/serde.
   - `ForgejoCredentials` — PAT wrapper with redaction-safe `Debug` (follow `token_source::SecretString` pattern).
   - `ForgejoContext<'a>` — `{ creds, instance }`; API base = `{instance}/api/v1`.
   - URL helpers: `parse_forgejo_owner_repo(instance, url)` (accepts `https://instance[/subpath]/owner/repo[.git]`, SSH `git@instance:owner/repo`, credential-bearing forms — mirrors `parse_github_owner_repo` semantics), `normalize_forgejo_origin_url`, `is_forgejo_origin(instance, url)`, `embed_token_in_url` (fixed marker username, PAT as password, `DisplaySafeUrl` so tokens never render — mirror `fabro_github::embed_token_in_url` tests).
   - REST ops (all take `&impl HttpClient`, port the `HttpMethod`/`HttpResponse`/`HttpClient` trio): `get_authenticated_user` (token validation), `get_repository`, `branch_head_sha`, `find_open_pull_request`, `create_pull_request`, `get_pull_request`, `merge_pull_request` (merge/squash/rebase), `close_pull_request`, `enable_auto_merge` — return `PullRequestApiError`-style `NotFound` vs `Other` enum so `pull_requests.rs` handlers branch identically. Gitea-lineage endpoints: `GET/POST /repos/{owner}/{repo}/pulls`, `GET/PATCH /repos/{owner}/{repo}/pulls/{index}`, `PUT /repos/{owner}/{repo}/pulls/{index}/merge`, `GET /repos/{owner}/{repo}/branches/{branch}`. Auto-merge: Gitea exposes it as a merge-endpoint parameter (`merge_when_checks_succeed`) rather than GitHub's separate endpoint — implement against that shape and prove it with the twin; if the twin run shows it unsupported on current stable, return a clear "auto-merge unsupported" error instead of failing the PR creation.
3. **Create `lib/components/fabro-forgejo/src/tests_mock.rs`** (port `MockHttpClient`), **`src/test_support.rs`** (feature-gated fixture creds/instance), **`tests/integration.rs`** (mock-client tests for every op: happy path, 404, 401/403, unexpected status, URL parsing table incl. subpath + SSH + credential-bearing forms, redaction proofs).
4. **Modify `lib/foundation/fabro-static/src/env_vars.rs`** — add `FORGEJO_URL`, `FORGEJO_TOKEN`, `FORGEJO_WEBHOOK_SECRET`; `FORGEJO_URL` goes in the non-secret list (alongside `GITHUB_BASE_URL`), the two secrets in the env-registry list.
5. **Modify `lib/foundation/fabro-static/src/secret_registry.rs`** — add `FORGEJO_TOKEN`, `FORGEJO_WEBHOOK_SECRET` to scrub lists (lines ~28–31, ~87–90).
6. **Modify `lib/foundation/fabro-types/src/settings/server.rs`** — `ServerIntegrationsSettings` gains `forgejo: ForgejoIntegrationSettings { enabled: bool /* default false */, url: Option<String> }` with `Default`. No `strategy` field (PAT-only).
7. **Modify `lib/foundation/fabro-types/src/settings/run.rs`** — `RunIntegrationsSettings` gains `forgejo: RunIntegrationsForgejoSettings { token: bool }` (bool, not a permissions map — PAT scopes are fixed at token creation; the run config only decides whether the token is exposed). Update the adjacent doc comments and the `#[cfg(test)] mod run_integrations_github_tests` style tests with forge siblings.
8. **Modify `lib/foundation/fabro-types/src/pull_request.rs`** — `PullRequestLink { owner, repo, number, forge: Option<String> }`; `html_url()` → forge ? `{forge}/{owner}/{repo}/pulls/{number}` : existing github.com form; extend the `Wire` struct (which is `deny_unknown_fields`) with `#[serde(default)] forge: Option<String>`; add `from_forgejo_url(instance, url)`. Keep `from_github_url` intact.

## Phase 1 — Sandbox clone support (Docker/Daytona)

9. **Modify `lib/components/fabro-sandbox/Cargo.toml`** — add `fabro-forgejo` (and dev `test-support`).
10. **Modify `lib/components/fabro-sandbox/src/clone_source.rs`** — add `CloneDecision::Forge { origin_url, branch, tag, commit_sha }`; `decide_clone` gains a `forgejo_instance: Option<&ForgejoInstance>` param: origin host matches instance → `Forge`; github.com → existing `GitHub` arm; otherwise the existing error message is extended to "support GitHub and the configured Forgejo instance origins only". Rename `GitHubRepoLayout` → `CloneRepoLayout` (crate-private) and generalize `github_repo_layout` to take the instance so owner/repo parse dispatches; layout math (owner dir, repo link) is unchanged. Update `clean_clone_for_record`/`repo_cloned_for_record` matching.
11. **Modify `lib/components/fabro-sandbox/src/sandbox_spec.rs`** — `SandboxSpec` gains `forgejo: Option<ForgejoSandboxConfig { instance, token }>` threaded beside `github_app` (constructor + all call sites in this file).
12. **Modify `lib/components/fabro-sandbox/src/provider.rs`, `src/from_environment.rs`, `src/config.rs`** — thread forgejo config from server settings/env into specs.
13. **Modify `lib/components/fabro-sandbox/src/docker.rs`** — in the clone-command builder, `CloneDecision::Forge` produces the same init/fetch/checkout sequence (reuse `PinnedRevision`, `exact_repository_init_command`) with the clone URL authenticated via `fabro_forgejo::embed_token_in_url` using the spec's PAT (site ~line 935 mirrors the github one).
14. **Modify `lib/components/fabro-sandbox/src/daytona/mod.rs`** — same dispatch at the `CloneDecision` match (~lines 1557–1577) and the authenticated-URL site (~line 1789).
15. **Modify `lib/components/fabro-sandbox/src/push_credentials.rs`** — `build_token_source` returns `None` for Forge origins (no expiry → no refresh); providers embed the spec PAT directly via a new `forgejo_auth_url` helper; the compare→`set-url`→record flow is bypassed for static PATs (first-embed only). Doc comment updated.

## Phase 2 — Workflow pipeline (token injection, git bridge, PR lifecycle)

16. **Modify `lib/components/fabro-workflow/Cargo.toml`** — add `fabro-forgejo` (+ dev `test-support`).
17. **Modify `lib/components/fabro-workflow/src/run_options.rs`** — `RunOptions` gains `forgejo: Option<(ForgejoInstance, ForgejoCredentials)>`.
18. **Modify `lib/components/fabro-workflow/src/services.rs`** — `EngineServices` gains `forgejo_token: Option<SecretString>` (+ instance); `resolve_workflow_env` inserts `FORGEJO_TOKEN` when present. Default constructors set `None`.
19. **Modify `lib/components/fabro-workflow/src/pipeline/initialize.rs`** — when `[run.integrations.forgejo].token` is true and the run origin matches the configured instance, populate `forgejo_token`; when true but credentials/instance are missing, fail with the same shape as the GitHub "requires credentials" error (~line 194).
20. **Modify `lib/components/fabro-workflow/src/git_bridge.rs`** — add forge bridging entries: `credential.https://<instance>.helper` reading `$FORGEJO_TOKEN` (forgejo-scoped helper constant in `fabro-forgejo`, mirroring `GITHUB_CREDENTIAL_HELPER`) plus `url.<instance>.insteadOf` SSH rewrites for the instance host. Primary origin only — `additional_repositories` stays GitHub-only (documented limitation).
21. **Modify `lib/components/fabro-workflow/src/pipeline/pull_request.rs`** — `OpenPullRequestRequest.github: GitHubContext` becomes `host: PullRequestHost<'_>` where `enum PullRequestHost { GitHub(GitHubContext<'a>), Forgejo(ForgejoContext<'a>) }`; dispatch at the five API touch points (`verify_remote_head`, `find_open_pull_request`, `create_pull_request`, `enable_auto_merge`, get/reconcile); forge path sets `CreatedPullRequest.link.forge = Some(instance)`. `build_pr_content` and everything else is host-agnostic — untouched.
22. **Modify `lib/components/fabro-workflow/src/pipeline/publish.rs`** — at the credential-resolution site (~lines 163–169): if the run origin matches the configured instance, build `PullRequestHost::Forgejo`; else the existing GitHub path unchanged.
23. **Modify `lib/components/fabro-workflow/src/pipeline/types.rs`** — doc-comment fix (~line 253: "no `GITHUB_TOKEN`" → also mention `FORGEJO_TOKEN`).

## Phase 3 — Server surface

24. **Modify `lib/apps/fabro-server/Cargo.toml`** — add `fabro-forgejo`.
25. **Modify `lib/apps/fabro-server/src/server.rs`** — (a) `forgejo_credentials()` resolver beside `github_credentials()` (~line 1577): reads settings `url` + vault `FORGEJO_TOKEN`, validates non-empty, returns `Option<(ForgejoInstance, ForgejoCredentials)>`; (b) app state gains resolved forgejo config + `forgejo_webhook_secret` (vault `FORGEJO_WEBHOOK_SECRET`, startup snapshot beside `github_webhook_secret` ~line 2602); (c) mount `/api/v1/webhooks/forgejo` when the secret exists (~line 1906 pattern); (d) thread forgejo config into sandbox-spec construction for run admission and into worker run options.
26. **Create `lib/apps/fabro-server/src/forgejo_webhooks.rs`** — `FORGEJO_WEBHOOK_ROUTE = "/api/v1/webhooks/forgejo"`; `verify_signature` accepting `X-Forgejo-Signature` / `X-Gitea-Signature` (raw hex) and `X-Hub-Signature-256` (`sha256=`-prefixed) — both HMAC-SHA256 over the raw body; handler mirrors `github_webhook`: unauthorized without a valid signature, auth-slot `Principal::Webhook`, log event + delivery id from `X-Forgejo-Event`/`X-Gitea-Event`/`X-GitHub-Event`, return `200`. Inert by decision.
27. **Modify `lib/apps/fabro-server/src/run_manifest.rs`** — (a) origin-admission/preflight (~line 806): origin matching the instance passes with a Forgejo "Repository Access" check (`get_repository` + `branch_head_sha` with PAT); non-matching non-GitHub origins keep today's error with the extended message; (b) authenticated clone URL resolution (~line 852) dispatches to `fabro_forgejo::embed_token_in_url` for instance origins; (c) add an inert informational preflight check when `run.integrations.forgejo.token` is requested (token present / missing credentials warning), mirroring the primary-only GitHub token check shape.
28. **Modify `lib/apps/fabro-server/src/server/handler/pull_requests.rs`** — context resolution (~lines 82–119): `PullRequestLink.forge` present → build `ForgejoContext` from state's instance + vault PAT; owner/repo come from the link (no GitHub URL parse needed); merge/close/get call the forgejo ops with the same `NotFound` mapping (~lines 530–605).
29. **Modify `lib/apps/fabro-server/src/install.rs`** — add `PUT /install/forgejo/token` (body: url + token; validates via `get_authenticated_user` against `{url}/api/v1/user`, persists vault secret + writes `server.integrations.forgejo.{enabled,url}` into settings) and `POST /install/forgejo/token/test` (validate-only, nothing persisted) — mirroring the `/install/github/token*` handlers (~line 634 route block).
30. **Modify `lib/apps/fabro-server/src/server/handler/system.rs`** — `get_system_integrations` (~line 105) adds a `forgejo` status object (`enabled`, `url_configured`, `token_configured`) via a `forgejo_integration_status` sibling of `github_integration_status`.
31. **Modify `lib/apps/fabro-server/src/diagnostics.rs`** — when configured: token check (`GET {instance}/api/v1/user`) + repo-reachability line, mirroring the GitHub blocks (~lines 351, 399, 530).
32. **Modify `lib/apps/fabro-server/src/serve.rs`** — startup resolution + `info!` "Forgejo integration enabled/disabled" beside the Slack/GitHub logs (~line 282 area). No Tailscale funnel work (that is GitHub-App-specific webhook plumbing).
33. **Modify `docs/public/api-reference/fabro-api.yaml`** — add paths `/api/v1/webhooks/forgejo`, `/install/forgejo/token`, `/install/forgejo/token/test` with request/response schemas; extend `PullRequestLink` with optional `forge` (uri-format string, omitted for GitHub links); extend the system-integrations response schema. Then run `cargo build -p fabro-api` (build.rs regenerates Rust types; confirm `PullRequestLink` resolves to the `fabro-types` type via existing `with_replacement` — add one if it isn't already replaced, plus the type-identity/JSON-parity test the repo conventions require).
34. **Regenerate TS client** — `cd lib/packages/fabro-api-client && bun run generate`.

## Phase 4 — CLI

35. **Modify `lib/apps/fabro-cli/src/commands/install.rs`** — optional Forgejo step after the GitHub step: "Configure a Forgejo instance? (y/N)"; prompts for instance URL + PAT (password prompt), validates via the same `GET /api/v1/user` call (or the install endpoint in server mode), writes settings/vault per install mode; non-interactive flag support following the existing pattern.
36. **Create `lib/apps/fabro-cli/src/shared/forgejo.rs`** — resolve instance+PAT from settings/env for local runs (sibling of `shared/github.rs`); wire into `commands/run/runner.rs` (~line 1105) so local runs against a Forgejo checkout get credentials.
37. **Modify `lib/apps/fabro-cli/src/commands/pr/link.rs`** — accept instance PR URLs via `PullRequestLink::from_forgejo_url` when the URL matches the configured instance; label renders `forgejo #N` (~line 35).
38. **Modify `lib/apps/fabro-cli/src/commands/doctor.rs`** — Forgejo connectivity check when configured (reuse `fabro-forgejo` probe).

## Phase 5 — Web

39. **Modify `apps/fabro-web/app/install-api.ts`** — typed wrappers for the two new install endpoints (following `testInstallGithubToken`/`putInstallGithubToken`).
40. **Modify `apps/fabro-web/app/install-flow.ts`, `install-config.ts`, `install-app.tsx`** — optional Forgejo step (URL + PAT + test button) in the wizard; state/validation mirroring the GitHub PAT step.
41. **Verify (no change expected) `apps/fabro-web/app/components/pull-request-chip.tsx` and run-summary components** — they are `html_url`-driven, so Forgejo links render once the link carries the instance. Extend their tests with a forge fixture.

## Phase 6 — Twin and tests (written alongside each phase, consolidated here)

42. **Create `test/twin/forgejo/`** — crate `twin-forgejo` mirroring `test/twin/github`: axum router serving Gitea-lineage REST v1 (`/api/v1/user`, `/api/v1/repos/...`, branches, pulls CRUD, merge PUT) with `Authorization: token` checking, fixture repos backed by real bare git dirs served over smart HTTP (reuse the twin-github git-handler approach), plus webhook-signature helper for tests.
43. **Modify root `Cargo.toml`** — add `"test/twin/forgejo"` to members (the list is explicit for twins).
44. **Modify `lib/foundation/fabro-test/Cargo.toml` + `src/lib.rs`** — re-export twin-forgejo helpers beside `twin_github` (~line 2094).
45. Test additions (all through existing tooling):
   - `fabro-forgejo` unit + integration tests (Phase 0).
   - `fabro-sandbox` `clone_source.rs` tests: forge origins accepted/subpath/ssh/rejected-host; Docker clone-command builder golden tests.
   - `fabro-workflow` `pipeline/pull_request.rs` tests with a mocked `PullRequestHost::Forgejo` (create/find/merge/auto-merge-unsupported paths); `git_bridge` env-shape tests for the instance host.
   - `fabro-server` `server/tests.rs`: webhook route (valid/invalid signature, all three header spellings, inert 200), install endpoints (persist + test), PR handler forge dispatch, preflight check for a forge origin against twin-forgejo, system-integrations response shape.
   - CLI scenario test using twin-forgejo: install → create run against a twin repo → clone (Docker or local) → PR create/merge via CLI/web API (model `scenario/auth.rs`).
   - Live (opt-in, env-gated): `.env.example` gains `FABRO_TEST_FORGEJO_URL`/`FABRO_TEST_FORGEJO_TOKEN` for `--profile e2e --run-ignored only` runs against a real instance.

## Phase 7 — Docs and metadata

46. **Create `docs/public/core-concepts/forgejo.mdx`** (setup: settings keys, PAT scopes needed — `read:repository`, `write:repository`, `write:issue`/PR scopes per op — webhook configuration with all accepted headers, run-integrations flag) + nav entry in `docs/public/docs.json`.
47. **Modify `CLAUDE.md`** — sandbox section: "GitHub repository origins only" now names GitHub + the configured Forgejo instance.
48. **Create `docs/public/changelog/2026-09-11.mdx`** (dated-file convention) announcing the integration.

## Order of execution

Phase 0 → 1 → 2 → 3 → 4 → 5, with Phase 6 tests written in the same commit-sized step as the code they cover (twin-forgejo built before the server/CLI scenario tests that need it), and Phase 7 last. Within Phase 3, the OpenAPI edit precedes handler wiring (repo's API workflow: spec → `cargo build -p fabro-api` → handler → conformance test). Each phase compiles and passes tests independently before the next begins.

## Verification

- `cargo build --workspace` after every phase.
- `cargo nextest run -p fabro-forgejo`, `-p fabro-sandbox`, `-p fabro-workflow`, `-p fabro-server`, `-p fabro-cli` as each phase lands; `cargo nextest run --workspace` at the end.
- `cargo +nightly-2026-04-14 fmt --check --all` and `cargo +nightly-2026-04-14 clippy --workspace --all-targets -- -D warnings`.
- Server spec/router conformance: `cargo nextest run -p fabro-server` (existing drift test covers the new routes).
- Web: `cd apps/fabro-web && bun test && bun run typecheck`; `cd lib/packages/fabro-api-client && bun run generate` and diff review.
- `cargo insta pending-snapshots` review for any CLI snapshot changes (install flow wording) before `cargo insta accept` — never blind-accept.
- Manual end-to-end (documented in the PR, not CI): `docker run codeberg.org/forgejo/forgejo:X` + local docker-compose; `fabro install` Forgejo step; run against a repo on the instance; PR create/merge from web + CLI; `curl` a signed webhook delivery to `/api/v1/webhooks/forgejo` and observe verify+log only.
- Live tests: `set -a && source .env && set +a && cargo nextest run --profile e2e --run-ignored only` with the Forgejo vars set.

## Deliberately not doing

- **Multi-instance identity / `GitRunTarget` wire changes** — human chose single-instance; a second configured instance is a future feature.
- **OAuth2 login, Forgejo Apps, token refresh machinery** — PAT-only per answer; PATs don't expire so `InstallationTokenSource`-style refresh is unneeded for Forgejo.
- **Tracker integration** — Forgejo has no GraphQL API; there is no Projects-V2 analog to mirror.
- **Event-driven webhooks** — inert parity per answer.
- **Automation-target support** — automations use the GitHub-slug intent lane; Forgejo automations would require the identity work explicitly declined. Automations targeting non-GitHub repos fail exactly as today.
- **`additional_repositories` for Forgejo** — the multi-repo scoped-token model is GitHub-App-specific; the Forgejo bridge covers the primary origin only.
- **Refactoring `fabro-github` behind a shared trait** — additive per answer; accepted duplication.
- **Plain-HTTP instances** — rejected at `ForgejoInstance` validation (matches `embed_token_in_url`'s HTTPS requirement; avoids cleartext token transport).
- **Gitea support advertising** — the client targets the shared Gitea-lineage REST shape (so Gitea likely works incidentally) but docs/UI name Forgejo only, per the goal's wording.