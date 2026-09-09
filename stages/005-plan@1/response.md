I now have the complete map of the GitHub vertical. Here is the implementation plan.

---

# Implementation Plan: Forgejo Integration (v1 — core SCM + operator surface)

**Authoritative constraints from the human:** PAT/token-only auth (no OAuth2, no GitHub-App-equivalent); single configured instance (`FORGEJO_URL` + `FORGEJO_TOKEN`); new parallel crate `lib/components/fabro-forgejo` (no refactor of `fabro-github`); scope = clone/push/auto-PR (`run.scm`), integration status, repo-access check endpoint, `fabro install` support. **No webhooks/automations, no browser sign-in.**

Prior decisions that stand: `FORGEJO_TOKEN` in sandboxes (never overload `GITHUB_TOKEN`); naming `run.scm.provider = "forgejo"`, `integrations.forgejo`, `RepositoryProvider::Forgejo`, provider string `"forgejo"`; PR links accept `{instance}/owner/repo/pulls/{n}` (and `/pull/{n}` alias); Forgejo API only (Gitea not promised); no tracker.

**Key wire-format facts the plan relies on** (verified against Forgejo/Gitea API v1 knowledge; re-verified in Phase 2 tests and the optional live check in Verification):
- Auth header: `Authorization: token <PAT>` (Bearer also accepted; use `token`).
- Create PR: `POST /api/v1/repos/{owner}/{repo}/pulls` body `{title, body, base, head}`. No `draft` field — draft is the `WIP:` title prefix convention.
- List PRs: `GET /api/v1/repos/{owner}/{repo}/pulls?state=open` (no `base` filter param — filter client-side). Items carry `html_url`, `number`, `title`, `head.sha`; **no `node_id`** (GitHub-only).
- Get PR: `GET .../pulls/{index}`. Close: `PATCH .../pulls/{index}` `{"state":"closed"}`.
- Merge: `POST .../pulls/{index}/merge` body `{"Do":"merge"|"rebase"|"rebase-merge"|"squash"|"fast-forward-only"}`.
- Branch head: `GET /api/v1/repos/{owner}/{repo}/branches/{branch}` → `commit.id`. Repo check: `GET /api/v1/repos/{owner}/{repo}`. Token check: `GET /api/v1/user`.
- Git HTTPS auth: basic auth with token as password (arbitrary username — same `x-access-token:{token}` form `fabro_github::embed_token_in_url` already produces) works on Forgejo.
- `MergeStrategy` maps: `Merge→"merge"`, `Squash→"squash"`, `Rebase→"rebase"`.

**Design decisions made to keep the diff small and honest:**
- `CloneDecision` gains a `Forgejo` sibling variant (same fields as `GitHub`) instead of reshaping the existing variant — existing GitHub tests stay untouched; providers match both via a shared accessor.
- Sandbox push credentials reuse `InstallationTokenSource::pat(...)` from `fabro-github` (a static-token cache — exactly what a PAT is). `fabro-forgejo` owns all API/URL logic; the PAT crosses into `fabro-sandbox` as an opaque string. This is plumbing reuse, not API duplication.
- `PullRequestLink` (persisted, `with_replacement` API type) gains an **optional** `forge` field, skipped when `None`, so all existing stored runs and wire JSON stay byte-identical. No data migration (serde default; old JSON deserializes unchanged). Per `migrations-strategy.md` this is a backward-compatible additive change, no migration runner needed.
- Forgejo sandbox env: inject `FORGEJO_TOKEN` + a host-scoped git credential helper for the instance host (forgejo counterpart of `git_bridge.rs`), applied when the run origin is the configured Forgejo instance. No `run.integrations.forgejo.permissions` table — PAT scopes are instance-side; minting doesn't exist.
- `[run.pull_request] auto_merge = true` + `provider = "forgejo"` → config-resolution **error** ("not supported for Forgejo runs") rather than silent skipping. Instance auto-merge support is version-dependent; explicit rejection beats silent divergence.
- Server-submitted `GitRunTarget` runs stay github.com (API contract says so). Forgejo runs reach the server via workflow `run.scm` config or CLI-local git origins.
- **Before writing code**, read (per CLAUDE.md): `docs/internal/error-handling-strategy.md`, `docs/internal/testing-strategy.md`, `docs/internal/server-secrets-strategy.md`, `docs/internal/events-strategy.md` (we add no new `Event` variants — existing PR events carry the link), `docs/internal/react-effects-policy.md` (web change adds no effects).

---

## Phase 1 — Foundation: env vars, settings, provider classification

| # | File | Change |
|---|---|---|
| 1 | `lib/foundation/fabro-static/src/env_vars.rs` | Add `FORGEJO_URL` and `FORGEJO_TOKEN` consts in the integration section (~line 76) and add both to the registry list (~line 225) so scrubbing/redaction covers them. |
| 2 | `lib/foundation/fabro-types/src/settings/server.rs` | Add `ForgejoIntegrationSettings { enabled: bool, url: Option<String> }` (derive Default/Serialize/Deserialize like `SlackIntegrationSettings`); add `pub forgejo: ForgejoIntegrationSettings` to `ServerIntegrationsSettings` (~line 243). |
| 3 | `lib/foundation/fabro-config/src/layers/server.rs` | Add `ForgejoIntegrationLayer { enabled: Option<bool>, url: Option<String> }` (`Combine`, `deny_unknown_fields`); add `forgejo` to `ServerIntegrationsLayer` (~line 206). |
| 4 | `lib/foundation/fabro-config/src/resolve/server.rs` | Resolve `forgejo` layer → settings in `resolve_integrations`; validate: when `enabled`, `url` must be an https URL with host (optional subpath allowed — Forgejo supports subpath installs); push resolve errors otherwise. |
| 5 | `lib/foundation/fabro-types/src/repository.rs` | Add `Forgejo` variant to `RepositoryProvider` (serde `snake_case` already → `"forgejo"`). Add `fn is_forgejo_origin(origin, base_url) -> bool` (host + base-path match). Add `RepositoryRef::from_origin_and_source_with_forge(origin, source, forge_base: Option<&str>)`; existing `from_origin_and_source` delegates with `None` (legacy callers unchanged). |
| 6 | `lib/foundation/fabro-types/src/system_integrations.rs` | Add `Forgejo` to `IntegrationProvider` (strum + serde snake_case). |
| 7 | `lib/foundation/fabro-config/src/resolve/run.rs` | Cross-validation: `scm.provider == "forgejo"` + `pull_request.auto_merge == true` → resolve error. |
| 8 | `lib/foundation/fabro-types/src/pull_request.rs` | `PullRequestLink` gains `pub forge: Option<ForgeInstanceRef>` where `ForgeInstanceRef { base_url: String }` (serializes `{"base_url": ...}`); `html_url()` returns `{base_url}/{owner}/{repo}/pulls/{number}` when `forge` is set, existing github.com form otherwise (legacy output byte-identical). Custom `Serialize` emits `forge` only when `Some`. `Deserialize` defaults `forge: None`; `html_url` consistency check branches per provider. Add `PullRequestLink::from_forgejo_url(url, base_url)` accepting `/pulls/{n}` and `/pull/{n}`; generalize the error messages to name both forms for forge URLs. |
| 9 | `lib/foundation/fabro-api/tests/pull_request_round_trip.rs` | Extend: round-trip a forge link; assert legacy JSON (no `forge` key) still deserializes and re-serializes without the key (parity with updated schema). |
| 10 | `lib/foundation/fabro-api/tests/system_integrations_round_trip.rs`, `server_settings_round_trip.rs` | Update for the new enum variant / settings field as the parity tests require. |

## Phase 2 — New crate `fabro-forgejo`

| # | File | Change |
|---|---|---|
| 11 | `lib/components/fabro-forgejo/Cargo.toml` | New; mirror `fabro-github` deps (anyhow, async-trait, serde, serde_json, strum, thiserror, tracing, chrono, fabro-http, fabro-redact, fabro-static, fabro-types; dev: fabro-test, tokio, tracing-subscriber). No `test-support` feature needed v1 (mock is `#[cfg(test)]`). |
| 12 | `lib/components/fabro-forgejo/src/lib.rs` | (a) `ForgejoCredentials::Pat(String)` + `valid_token()`; (b) `forgejo_base_url()` — settings-style resolution helper reading a passed-in URL or `EnvVars::FORGEJO_URL`, normalized (`https://host[/path]`, no trailing slash); (c) `ForgejoContext<'a> { creds, base_url }` + `with_http_client`; (d) URL helpers: `parse_forgejo_owner_repo(url, base_url)` (host/base-path must match instance; strips credentials/`.git`; reuses the same normalization semantics as `fabro_github`), `ssh_url_to_https` behavior comes free (host-generic upstream); (e) local `HttpClient` trait (request(method,url,headers,body)) + impl for `fabro_http::HttpClient`, `forgejo_headers(token)` → `Authorization: token <PAT>`; (f) API fns: `get_repo`, `get_authenticated_user`, `branch_head_sha`, `find_open_pull_request` (list+filter by base/head-sha), `create_pull_request` (draft → `WIP: ` title prefix), `get_pull_request` → new `PullRequestForgejoDetail` with `From<PullRequestForgejoDetail> for fabro_types::PullRequestDetails`, `merge_pull_request` (`Do` mapping, 409/405 handling mirroring `PullRequestApiError` semantics — reuse `fabro_types`-level error shape via a local `PullRequestApiError`), `close_pull_request` (PATCH state=closed), `enable_auto_merge` — **not provided** (absent, by decision); (g) `embed_token_in_url` — reuse `fabro_github::embed_token_in_url` shape via a thin local fn (x-access-token:token; verify acceptance in live check); (h) unit tests: mock-client tests per endpoint (fixtures in-crate), URL parsing/redaction tests mirroring `fabro-github`'s. |
| 13 | `lib/components/fabro-forgejo/src/tests_mock.rs` | `#[cfg(test)]` mock `HttpClient` (port of `fabro-github`'s pattern). |

## Phase 3 — Manifest origin from `run.scm`

| # | File | Change |
|---|---|---|
| 14 | `lib/components/fabro-manifest/src/lib.rs` | `configured_repo_origin_url`: accept `provider == "forgejo"` → require configured base URL (env `FORGEJO_URL`) → `{base}/{owner}/{repo}` normalized; error when provider is forgejo but no instance URL is configured. Update the `[run.scm]` doc block (~line 1698). |

## Phase 4 — Sandbox: clone, layout, push credentials

| # | File | Change |
|---|---|---|
| 15 | `lib/components/fabro-sandbox/src/clone_source.rs` | Add `CloneDecision::Forgejo { origin_url, branch, tag, commit_sha }`; `decide_clone` gains `forgejo_base_url: Option<&str>` param — origins whose host/base-path match the instance parse via `fabro_forgejo::parse_forgejo_owner_repo`, others keep the current GitHub-only error text (reworded to name both). Extract shared layout builder from `github_repo_layout` → `repo_layout(owner, repo, …)`; `github_repo_layout` delegates; add `forgejo_repo_layout`. Add accessor `CloneDecision::git_origin()` for providers. |
| 16 | `lib/components/fabro-sandbox/src/push_credentials.rs` | `build_token_source` gains forgejo inputs (`forgejo_pat: Option<&str>`, `forgejo_base_url`): for a forgejo origin build `InstallationTokenSource::pat(token)` (static; refresh is a no-op reuse). Keep GitHub path byte-identical. |
| 17 | `lib/components/fabro-sandbox/src/sandbox_spec.rs` | Add `ForgejoSandboxCreds { base_url, token }` (`Debug`-safe: token behind the existing redaction pattern — reuse `SecretString`-style wrapper); spec fields gain `forgejo: Option<ForgejoSandboxCreds>` alongside `github_app`; clone decision call sites pass the instance URL. |
| 18 | `lib/components/fabro-sandbox/src/docker.rs` | Match `CloneDecision::Forgejo` in clone steps (same commands — host-agnostic git); token embedding flows through the existing `PushCredentialState` with the PAT source. |
| 19 | `lib/components/fabro-sandbox/src/daytona/mod.rs` | Same: forgejo decision accepted; SDK clone with the forgejo URL + branch/commit (same transport as GitHub path). |
| 20 | `lib/components/fabro-sandbox/src/provider.rs`, `src/lib.rs` | Match/exhaustiveness updates for the new variant; exports. |

## Phase 5 — Workflow engine: env, bridge, PR pipeline, publish

| # | File | Change |
|---|---|---|
| 21 | `lib/components/fabro-workflow/src/git_bridge.rs` | Add `forgejo_bridge_entries(base_url)` → `credential.https://{host}[/path].helper` reading `$FORGEJO_TOKEN` (same shell shape as `GITHUB_CREDENTIAL_HELPER`, username `x-access-token`); `merge_forgejo_bridge_env(env, base_url)` mirroring the count-merge logic. Applied when the run origin is the configured instance. |
| 22 | `lib/components/fabro-workflow/src/services.rs` | Env providers gain `forgejo_token: Option<String>`; merged into command/tool/ACP env as `FORGEJO_TOKEN` at the point of use (static PAT — no token source needed). |
| 23 | `lib/components/fabro-workflow/src/run_options.rs`, `src/pipeline/types.rs` | Run/publish options gain `forgejo: Option<ForgejoRunCreds { base_url, token }>` next to `github_app`. |
| 24 | `lib/components/fabro-workflow/src/pipeline/pull_request.rs` | `OpenPullRequestRequest.github: GitHubContext` → `scm: PullRequestRemote` (new enum `Github(GitHubContext) | Forgejo(ForgejoContext)`). `open_pull_request` parses owner/repo per provider; `verify_remote_head`, `reconcile_existing_pull_request`, `create`, and link construction branch on the enum; forgejo `CreatedPullRequest` has no `node_id` (auto-merge branch is unreachable — config already rejects it). Tests: add httpmock-based forgejo fixtures (create/find/merge) next to the GitHub ones. |
| 25 | `lib/components/fabro-workflow/src/pipeline/publish.rs` | Select `PullRequestRemote` by run-origin provider (github.com → GitHub creds; instance match → Forgejo creds); clear error when a forgejo origin has no configured token. |
| 26 | `lib/components/fabro-workflow/src/pipeline/initialize.rs` | Apply forgejo bridge env + `FORGEJO_TOKEN` when origin is forgejo; skip the GitHub `GitHubRepositoryAccess` path for forgejo origins (no additional-repos support v1). |
| 27 | `lib/components/fabro-workflow/src/run_metadata.rs` | Origin parse at ~line 298: when origin is a forgejo origin, use the forgejo parser (and `RepositoryRef` classification with the instance URL) instead of erroring through `parse_github_owner_repo`. |
| 28 | `lib/components/fabro-workflow/src/operations/fork.rs` | Guard: forgejo origin + fork requested → explicit "fork is not supported for Forgejo runs" error (no silent fallthrough). |

## Phase 6 — Server: credentials, preflight, handlers, supervisor

| # | File | Change |
|---|---|---|
| 29 | `lib/apps/fabro-server/src/server.rs` | `AppState`: add `forgejo_base_url: Option<String>` + `forgejo_credentials()` (vault/env `FORGEJO_TOKEN` → `ForgejoCredentials::Pat`, respecting `integrations.forgejo.enabled`). Wire both AppState construction sites (`server.rs` ~2523, `serve.rs` ~824). |
| 30 | `lib/apps/fabro-server/src/serve.rs` | AppState wiring from #29. |
| 31 | `lib/apps/fabro-server/src/run_manifest.rs` | Preflight: when the prepared manifest origin is the forgejo instance → require token (check row mirroring `run_github_token_check`), remote-ref check via `fabro_forgejo::branch_head_sha`, and pass `ForgejoSandboxCreds` into the sandbox spec for clone-based providers. GitHub path untouched. |
| 32 | `lib/apps/fabro-server/src/server/handler/system.rs` | (a) `forgejo_integration_status(settings, vault)` — disabled/missing-credentials/configured, metadata `url`; (b) include in `get_system_integrations` (`data: vec![github, forgejo, slack]`); (c) `get_forgejo_repo` handler + route `/repos/forgejo/{owner}/{name}` reusing `validate_github_slug` (rename to `validate_repo_slug` mechanically or keep name) and `RepoCheckResponse` (accessible/default_branch/private via `get_repo`; no `install_url`). |
| 33 | `lib/apps/fabro-server/src/server/handler/mod.rs` | Mirror the demo route (`demo::get_forgejo_repo`) at ~line 173. |
| 34 | `lib/apps/fabro-server/src/demo/mod.rs` | Demo `get_forgejo_repo` stub mirroring `get_github_repo` (~line 639). |
| 35 | `lib/apps/fabro-server/src/server/handler/pull_requests.rs` | `load_server_github_credentials`/`server_github_context` gain forgejo counterparts; run PR get/create/merge/close branch on the run's repository provider; forgejo details map `PullRequestForgejoDetail` → `PullRequestDetails`. |
| 36 | `lib/apps/fabro-server/src/server/pull_request_supervisor.rs` | Uses the #35 helpers — pass through the selected provider context (no logic change beyond plumbing). |
| 37 | `lib/apps/fabro-server/src/server/tests.rs` | Update/add: integration-status includes forgejo; `/repos/forgejo/...` handler test with mock; PR endpoint test for a forgejo-origin run. |

## Phase 7 — OpenAPI + generated clients

| # | File | Change |
|---|---|---|
| 38 | `docs/public/api-reference/fabro-api.yaml` | (a) `/api/v1/repos/forgejo/{owner}/{name}` (operationId `getForgejoRepo`, same params/`RepoCheckResponse`); (b) `IntegrationProvider` enum += `forgejo`; (c) `RepositoryRef.provider` enum += `forgejo`; (d) `PullRequestLink`: optional `forge: {base_url: string}` property + description/example updates; (e) any `github.com`-only wording touched by these schemas. |
| 39 | `lib/packages/fabro-api-client/**` (generated) | `cd lib/packages/fabro-api-client && bun run generate` after the spec edit (no hand edits). |

## Phase 8 — CLI: install + run

| # | File | Change |
|---|---|---|
| 40 | `lib/components/fabro-install/src/lib.rs` | `write_forgejo_settings(doc, url)` (writes `[server.integrations.forgejo] enabled/url`); add `FORGEJO_TOKEN` to the install secret-key consts pattern. |
| 41 | `lib/apps/fabro-cli/src/commands/install.rs` | New Forgejo branch in the install flow: prompt instance URL + PAT (hidden input; honor pre-set `FORGEJO_TOKEN` env), validate via `get_authenticated_user`, write vault + settings through #40. Flags `--forgejo-url <URL>` for non-interactive use. |
| 42 | `lib/apps/fabro-cli/src/shared/forgejo.rs` (new) + `src/shared/mod.rs` | `build_forgejo_credentials(settings, vault) -> Option<ForgejoRunCreds>` (env → vault lookup, mirroring `shared/github.rs` shape). |
| 43 | `lib/apps/fabro-cli/src/commands/run/runner.rs` | `maybe_build_forgejo_credentials` alongside `maybe_build_github_credentials` (~1102); feed run options + `StartServices` forgejo token. |
| 44 | `lib/apps/fabro-cli/src/commands/repo/init.rs` | Inspect: it parses the git remote with GitHub assumptions — make remote classification provider-aware so a Forgejo remote fills `run.scm` correctly. |
| 45 | `lib/apps/fabro-cli/src/commands/cli_reference.rs` | Only if it enumerates install flags — add `--forgejo-url`. |

## Phase 9 — Web UI + docs

| # | File | Change |
|---|---|---|
| 46 | `apps/fabro-web/app/routes/settings-integrations.tsx` | Row under the "Version Control" `Panel` for provider `"forgejo"` (instance URL from `metadata.url`); render when present; adjust the `github && slack` gate to tolerate a missing forgejo entry (older servers). |
| 47 | `apps/fabro-web/app/routes/settings-integrations.test.tsx` | Tests for the forgejo row (present/absent/disabled states). |
| 48 | `docs/public/integrations/forgejo.mdx` (new) | PAT-only setup (token creation with scopes `read:repository`, `write:repository`, `write:issue` — verified in Phase 2), single-instance config, `run.scm` example, capability matrix vs GitHub (no webhooks/OAuth/auto-merge/additional-repositories/fork), clone/sandbox `FORGEJO_TOKEN` notes. |
| 49 | `docs/public/docs.json` | Nav entry `integrations/forgejo` after `integrations/github` (~line 95). |

**Order of execution:** Phases 1→9 strictly in order; each phase ends with the phase's crate building and its tests green before the next begins (Phase 2 unblocks 3–6; Phase 7 must land with 6 so the conformance test passes in the same commit; 8–9 are leaf UX).

---

## Verification

Existing tooling only (no new harness):

1. **Per-crate unit tests**, run after each phase:
   - `cargo nextest run -p fabro-forgejo` (new: endpoint fixtures, URL parse/redaction, `PullRequestApiError` chains)
   - `cargo nextest run -p fabro-types -p fabro-config -p fabro-static`
   - `cargo nextest run -p fabro-sandbox` (clone decision forgejo origin/layout/PAT source; existing GitHub tests unchanged)
   - `cargo nextest run -p fabro-workflow -- pull_request` (httpmock forgejo fixtures) then full crate
   - `cargo nextest run -p fabro-server` (includes the **OpenAPI conformance test** that catches spec/router drift — must pass only after Phase 6+7 land together)
   - `cargo nextest run -p fabro-api` (updated round-trip parity tests for `PullRequestLink`, `IntegrationProvider`, server settings)
   - `cargo nextest run -p fabro-cli -p fabro-install`
2. **Generated clients**: `cargo build -p fabro-api` (progenitor regen), then `cd lib/packages/fabro-api-client && bun run generate`.
3. **Web**: `cd apps/fabro-web && bun test && bun run typecheck`.
4. **Workspace gates**: `cargo build --workspace`; `cargo nextest run --workspace`; `cargo +nightly-2026-04-14 fmt --check --all`; `cargo +nightly-2026-04-14 clippy --workspace --all-targets -- -D warnings`.
5. **Wire-format risk (the one thing mocks can't prove)** — optional live check if Docker is available in the environment: `docker run -d -p 3000:3000 codeberg.org/forgejo/forgejo`, create admin + PAT + repo, then verify (a) clone with `https://x-access-token:{token}@localhost:3000/...` succeeds, (b) PR create with `WIP:` draft prefix, (c) merge with each `Do` value, (d) close via PATCH, (e) `GET /api/v1/user` with `Authorization: token`. If Docker is unavailable, fixtures encode these shapes and the assumption is flagged in the PR description. (`test/twin/github`-style Forgejo twin is deliberately not built in v1.)

## Deliberately not doing

- **Webhooks + automation triggers** — excluded by the chosen scope; `automation_materializer.rs`, `github_webhooks.rs`, `GitRepoCache` untouched.
- **OAuth2 browser sign-in / `ServerAuthMethod` changes** — PAT-only decision; web auth paths untouched.
- **Multi-instance support, host-aware slugs** — single-instance decision; `GitHubRepositorySlug` semantics unchanged.
- **`run.integrations.forgejo` permissions/additional-repositories** — a GitHub App minting concept; a PAT's scopes live instance-side. Forgejo runs get a static `FORGEJO_TOKEN`.
- **Auto-merge on Forgejo** — rejected at config validation with a clear error; instance support is version-dependent and the GitHub GraphQL `node_id` flow doesn't map.
- **Fork operation on Forgejo** — explicit unsupported error; GitHub-only feature.
- **`GitRunTarget` forge targets in the run-intent API** — contract says github.com; Forgejo origins arrive via workflow `run.scm` or CLI-local remotes. Extending it would drag admission checks into scope.
- **Tracker support** — Forgejo has no Projects V2 GraphQL equivalent.
- **Refactoring `fabro-github` into a generic provider layer** — human chose a parallel crate; the only reuse is the static-token cache (`InstallationTokenSource::pat`) and `embed_token_in_url`'s URL form.
- **Gitea compatibility promise** — likely works (API-compatible fork); not tested, not documented as supported.
- **Forgejo twin server (`test/twin/forgejo`)** — mock-client + httpmock tests cover v1 shapes; a twin is e2e infrastructure for later.