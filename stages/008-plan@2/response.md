I re-read every file in the criticized path. Three facts materially shape the revision:

- `observe_git_run_target` has exactly **one** production caller (`fabro-cli/src/commands/run/create.rs:161`), so threading the instance through admission is contained.
- `ValidatedGitRunTarget::repository()` is consumed **only** by `automation_materializer.rs` — the explicitly out-of-scope path — so the forge extension must keep that accessor GitHub-meaningful and guard automations separately.
- `normalize_repo_origin_url`/`ssh_url_to_https` (fabro-github lib.rs:1064-1099) are **host-generic** — verified; forge origins already normalize correctly, no change needed there (criticism resolved, not just accepted).

# Implementation Plan (revised): Forgejo/Gitea Integration — run-path core + install/ops

**Authoritative answers applied:** scope (b) = run core + install/ops surface (no OAuth login, no inbound webhooks, no automations on Forgejo repos); single configured instance; dual "Forgejo/Gitea" branding. Standing decisions: PAT-only v1, new `fabro-forgejo` crate, host-aware `PullRequestLink`, fail-loud auto-merge error, `WIP:` draft mapping, system-trust TLS.

**What changed vs the previous plan and why:** (1) **Added Step 3 — the run-admission layer** (`run_intent.rs`, `fabro-manifest`, `run create.rs`, automation guard): the critique's blocking defect; without it `fabro run` from a Forgejo checkout is rejected before any clone code runs. (2) `run.scm` origin override stays GitHub-only in v1 (avoids `RunScmSettings`/spec churn; forge origin comes from the checkout remote). (3) Twin git smart-HTTP handler explicitly deferred — needed only for optional docker-clone/push e2e, not the core. (4) Draft-prefix doc wording now says the prefix is instance-configurable. (5) Verification now includes admission tests at CLI/workflow level so wrong-but-compiling work fails a test.

---

## Step 1 — Foundation: env vars, secrets, shared types

**Create** `lib/components/fabro-forgejo/` scaffold (root `Cargo.toml` globs `lib/components/*`; add `fabro-forgejo` to `[workspace.dependencies]`).

**Modify `lib/foundation/fabro-static/src/env_vars.rs`** (lines 74-80 block): add `FORGEJO_URL`, `FORGEJO_TOKEN`.

**Modify `lib/foundation/fabro-static/src/secret_registry.rs`**: `FORGEJO_TOKEN` → `OptionalVault` (mirrors `GITHUB_TOKEN`); `FORGEJO_URL` unclassified.

**Modify `lib/foundation/fabro-types/src/pull_request.rs`**: `PullRequestLink` (line 62) gains `pub instance_url: Option<String>`; `html_url()` (line 70) renders `{instance_url}/{owner}/{repo}/pulls/{number}` when set (Gitea path is `/pulls/`, GitHub's `/pull/`), else the existing github.com form; `Serialize` (line 82) emits `instance_url` only when `Some` (old events/data unchanged); `Deserialize` (line 96) validates `html_url` against the computed form for the stored instance; add `from_forge_url(url, expected_instance)` parsing `/pulls/{n}` with host match. Extend in-file tests.

**Modify `lib/foundation/fabro-types/src/settings/server.rs`**: `ForgejoIntegrationSettings { enabled: bool, url: Option<String> }` (`Default` disabled); add `pub forgejo: ForgejoIntegrationSettings` to `ServerIntegrationsSettings` (line 242).

**Modify `lib/foundation/fabro-types/src/system_integrations.rs`**: `IntegrationProvider::Forgejo` (line 29).

**Modify `lib/foundation/fabro-config/src/layers/server.rs`**: `ForgejoIntegrationLayer { enabled: Option<bool>, url: Option<String> }`; `pub forgejo: Option<...>` on `ServerIntegrationsLayer` (line 202).

**Modify `lib/foundation/fabro-config/src/resolve/server.rs`**: `resolve_integrations` (line 336): presence ⇒ enabled; validation error at path `server.integrations.forgejo.url` when enabled without a valid `https://` origin (host required, no credentials, no path). Tests in `src/tests/resolve_server.rs`.

**Verify:** `cargo nextest run -p fabro-types -p fabro-config -p fabro-static`.

## Step 2 — `fabro-forgejo` crate (moved before admission; manifest needs its helpers)

**Create `lib/components/fabro-forgejo/src/lib.rs`** — mirrors `fabro-github` (own local `HttpClient`/`HttpResponse` traits + impl for `fabro_http::HttpClient`; own minimal `SecretString`; crates independent):
- `ForgejoCredentials::Pat(String)` (enum reserved for future OAuth2); `ForgejoContext<'a> { creds, instance_url, http_client }`.
- `forgejo_api_url(instance) -> "{instance}/api/v1"`; auth header `Authorization: token {pat}`.
- Endpoints: `validate_token` (GET `/api/v1/user`), `server_version` (GET `/api/v1/version`), `get_repository` (GET `/api/v1/repos/{owner}/{repo}`), `branch_head_sha` (GET `.../branches/{branch}` → `commit.id`), `find_open_pull_request` (GET `.../pulls?state=open`, paginate `limit`/`page`, **client-side filter** by `head.sha` — Gitea lacks GitHub's `head=` param), `create_pull_request` (POST `.../pulls` `{title, body, base, head}`; `draft` ⇒ `"WIP: "` title prefix), `get_pull_request` (GET `.../pulls/{index}` → `ForgejoPullDetail`; `From<ForgejoPullDetail> for PullRequestDetails` with `additions/deletions/changed_files = 0`, `draft` from title prefix), `merge_pull_request` (POST `.../pulls/{index}/merge` `{"Do": "merge"|"squash"|"rebase"}`), `close_pull_request` (PATCH `.../pulls/{index}` `{"state":"closed"}`).
- `PullRequestApiError { NotFound, Other }` with 404/401/403/405/409 mapping (405 ⇒ not mergeable/method disallowed, 409 ⇒ conflict).
- URL helpers: `instance_host`, `is_instance_origin(origin, instance)` (host equality, scheme-agnostic), `parse_owner_repo(origin, instance)`, `ssh_url_to_https`/`normalize_origin_url`/`embed_token_in_url` (token as password; `DisplaySafeUrl` redaction).
- `FORGEJO_URL` env override for the instance, mirroring `github_api_base_url()`'s documented-override pattern.

**Create** `src/tests_mock.rs` (`#[cfg(test)]`), `src/test_support.rs` (feature-gated), `tests/integration.rs` (every endpoint happy path + error statuses + draft prefix + pagination + host mismatch), `tests/live_access.rs` (env-gated).

**Verify:** `cargo nextest run -p fabro-forgejo`.

## Step 3 — Run admission (NEW — the critique's blocking defect)

**Modify `lib/foundation/fabro-types/src/run_intent.rs`**:
- `GitRunTarget` (line 69) gains `#[serde(default, skip_serializing_if = "Option::is_none")] pub instance_url: Option<String>` (follows the existing `tag`/`sha` pattern under `deny_unknown_fields`).
- `GitRunTarget::validate()` (line 83): when `instance_url` is `Some`, validate it as an https origin (host, no path/credentials — failure maps to `GitCoordinateValidationError::Repository`); slug grammar still enforced via `GitHubRepositorySlug` (documented: GitHub 39/100-char rules apply — acceptable for Forgejo/Gitea names, noted in docs); built `GitContext.origin_url` becomes `{instance_url}/{owner}/{repo}` (currently `repository.https_url()`, line 106). When `None`, byte-identical behavior. `ValidatedGitRunTarget` keeps its shape; `.repository()` remains the slug (its only consumer is automations — see guard below); add `instance_url()` accessor.
- Extend `lib/foundation/fabro-types/tests/run_intent.rs`: forge validate round-trip, invalid instance URL rejected, github targets unaffected.

**Modify `lib/components/fabro-manifest/src/lib.rs`**:
- `observe_git_run_target` (line ~329) gains `forge_instance_url: Option<&str>` (its single production caller threads it; test callers updated). `github_run_target` (line ~417) becomes origin-classifying: host `github.com` ⇒ existing path; host == instance (via `fabro_forgejo::is_instance_origin`) ⇒ `GitRunTarget { repo: slug, instance_url: Some(instance), .. }` then the same `RunTarget::validate()` proof; otherwise `None`. The best-effort branch push (`publish_manifest_branch_best_effort`) and `remotely_available_sha` work unchanged — they push to the checkout's own remote; for private instances the create-time push is best-effort exactly as for GitHub, and the existing "push the commit and try again" error covers failure. `configured_repo_origin_url` (line ~434) is left GitHub-only in v1.
- Manifest tests: forge-origin observation produces a forge target; host-mismatch origin yields `None`; github observation unchanged.

**Modify `lib/apps/fabro-cli/src/commands/run/create.rs`**: pass the instance (resolved settings `[server.integrations.forgejo].url`, `FORGEJO_URL` env override) into `observe_git_run_target` (line 161); make the two error messages provider-aware (lines ~166-174: "canonical GitHub run target" / "canonical GitHub origin" → name the configured forge when the origin matched it, else keep GitHub wording).

**Modify `lib/apps/fabro-server/src/automation_materializer.rs`**: reject automation targets with `instance_url.is_some()` at validation (line ~241) with an explicit "Forgejo/Gitea automation targets are not supported" error — keeps automations GitHub-only per scope without narrowing `RunTarget` for local runs.

**Verify:** `cargo nextest run -p fabro-types -p fabro-manifest`; CLI test in Step 10.

## Step 4 — OpenAPI spec and generated clients

**Modify `docs/public/api-reference/fabro-api.yaml`**: optional `forgejo` on `ServerIntegrationsSettings` (line 14552) → new `ForgejoIntegrationSettings`; `IntegrationProvider` enum (line 15480) + `forgejo`; `PullRequestLink` (line 12576) optional `instance_url`; `LinkRunPullLinkRequest` (line 12801) description accepts forge `/pulls/{n}` URLs matching the configured instance; `GitRunTarget` (line 9444) optional `instance_url` (and `RunIntent`-adjacent description notes local runs only); new paths `POST /install/forgejo/test`, `PUT /install/forgejo`, `GET /repos/forgejo/{owner}/{name}` (reuse `RepoCheckResponse`); new `InstallForgejo*` schemas; extend install summary.

**Modify `lib/foundation/fabro-api/build.rs`**: `with_replacement` for `ForgejoIntegrationSettings` → fabro-types type (existing whole-struct replacements for `ServerIntegrationsSettings`, `IntegrationProvider`, `GitRunTarget`, `PullRequestLink` already route through fabro-types).

**Extend** `lib/foundation/fabro-api/tests/{server_settings,system_integrations,pull_request}_round_trip.rs` + `run_intent`-related parity test for the new optional fields. **Regenerate:** `cargo build -p fabro-api`; `cd lib/packages/fabro-api-client && bun run generate`.

**Verify:** `cargo nextest run -p fabro-api`.

## Step 5 — Forgejo twin (REST core; git endpoint deferred)

**Create `test/twin/forgejo/`** mirroring `test/twin/github/`: `state.rs`, `server.rs` (`.no_proxy()` TestServer), handlers `version.rs`, `users.rs`, `repos.rs`, `branches.rs`, `pulls.rs` (list/create/get/patch/merge); token auth accepting `Authorization: token X`. **Deferred (optional, after core lands):** git smart-HTTP handler for docker-clone/push e2e — only needed for optional live-ish tests, per critique; twin-github's `handlers/git.rs` is the pattern if added.

**Modify `lib/foundation/fabro-test/src/lib.rs`** (mirror `twin_github` exports at lines 2094-2115) + fabro-test Cargo.toml.

**Verify:** `cargo nextest run -p fabro-test -p fabro-forgejo`.

## Step 6 — Sandbox clone support

**Modify `lib/components/fabro-sandbox/src/clone_source.rs`**: `CloneDecision::Forge { origin_url, branch, tag, commit_sha }` (enum line 3); `decide_clone` (line 266) gains `forgejo_instance: Option<&str>` — host-matching origins classify `Forge` with identical precedence and pinned-SHA rules (`skip_clone` still overrides; absent origin still empty; unknown hosts still error, message updated to name Forgejo/Gitea and the configuration knob); `github_repo_layout` (line 26) → `repo_layout(origin, instance)` via `fabro_forgejo::parse_owner_repo` for forge origins, same `/repos/<owner>/<repo>` layout and path validation; update constructor pre-checks (`DockerSandbox::new` ~195, `DaytonaSandbox::new` ~535) and unit tests (forge accept, mismatch reject, pinned SHA on forge).

**Modify `src/push_credentials.rs`**: `build_token_source` (line 27) → `build_credential_source(github_app, forgejo: Option<&ForgejoCredentials>, clone_origin_url, forgejo_instance)` returning `enum CredentialSource { GitHub(Arc<InstallationTokenSource>), Forgejo(SecretString) }`; `PushCredentialState` holds the enum; PAT arm returns the static token (`expires_at() == None` ⇒ refresh no-ops, matching `is_static()` handling); compare→set-url→record semantics preserved.

**Modify `src/docker.rs`**: options gain `forgejo_credentials` + `forgejo_instance`; `clone_github_repo` (line 909) dispatches the `Forge` arm (PAT-embedded URL, generalized layout, pinned-SHA sequence lines 988-1042 reused); **`src/daytona/mod.rs`** same at init match (lines 1547-1585) with SDK clone username/password (lines 1690-1697) and `attach_pinned_branch` unchanged; **`src/provider.rs`, `provider/docker.rs`, `provider/daytona.rs`** thread the options.

**Verify:** `cargo nextest run -p fabro-sandbox`.

## Step 7 — Workflow engine plumbing

**Modify `src/run_options.rs`** (`RunOptions` line 34): `forgejo: Option<ForgejoRunCredentials>` (owned `{ instance_url, pat }`). **`src/operations/start.rs`**: `RunSession`/`StartServices` gain the same. **`src/services.rs`**: `EngineServices.forgejo_token`; inject `FORGEJO_TOKEN` at point of use (mirror lines 354-364) when the run origin matches. **`src/git_bridge.rs`**: host-scoped credential helper `credential.https://{instance_host}.helper` reading `$FORGEJO_TOKEN` + SSH→HTTPS `insteadOf` for the instance host (this is what authenticates run-branch pushes). **`src/pipeline/initialize.rs`**: pass forgejo credentials to sandbox options + env seed when the spec's origin matches. **`src/run_metadata.rs`** (lines 277-325): owner/repo via forge parser + PAT embed for forge origins. **`src/pipeline/pull_request.rs`**: `OpenPullRequestRequest` (line 447) gains `forgejo: Option<ForgejoContext>`; `open_pull_request` (line 592) dispatches by origin host — github path untouched; forge path: `branch_head_sha` verify → `find_open_pull_request` reconcile → shared `build_pr_content` → `create_pull_request` with `WIP:` prefix → **auto-merge requested ⇒ explicit error** `"auto-merge is not supported for Forgejo/Gitea pull requests; disable run.pull_request.auto_merge"` → link with `instance_url`. **`src/pipeline/publish.rs`** (lines 152-170): forge context selection by origin; GitHub requirement error stays on the github path only.

**Verify:** `cargo nextest run -p fabro-workflow`; forge PR-path tests against the twin (mirror `tests/it/git_integration.rs`), including the auto-merge failure message and a github-path regression test.

## Step 8 — Install writers

**Modify `lib/components/fabro-install/src/lib.rs`**: `FORGEJO_VAULT_KEYS`/`FORGEJO_INSTALL_SECRET_KEYS`; `write_forgejo_settings` writing `[server.integrations.forgejo] url` + vault `FORGEJO_TOKEN` (mirror `write_token_settings`, line 279); unit tests.

## Step 9 — Server

**Modify `src/server.rs`**: `AppState` resolved forgejo settings + `forgejo_credentials()` (mirror lines 1575-1622; errors point at `fabro install` / `fabro secret set FORGEJO_TOKEN`). **`src/server/handler/pull_requests.rs`**: `parse_github_owner_repo_from_url` (line 45) accepts instance hosts; link handler (line 70) uses `from_forge_url`; forgejo credential/context helpers alongside lines 80/108; GET/merge/close dispatch on `record.instance_url`; create validation mirrors hard/soft semantics (origin matches + no token ⇒ warn + skip PR). **`src/server/handler/system.rs`**: `forgejo_integration_status` (mirror lines 122-190); `GET /repos/forgejo/{owner}/{name}` route. **`src/install.rs`**: routes (lines 633-645 area) `POST /install/forgejo/test`, `PUT /install/forgejo`; state; `post_install_finish` writes via Step 8 (coexists with GitHub). **`src/diagnostics.rs`**: forgejo health (`/api/v1/version` + `/user` probe, remediation strings). **`src/run_manifest.rs`** (lines 1284-1300): warn on user-env `FORGEJO_TOKEN` override.

**Verify:** `cargo nextest run -p fabro-server` (openapi conformance included; handler tests against the twin).

## Step 10 — CLI

**Create `src/shared/forgejo.rs`**: `build_forgejo_credentials` (env `FORGEJO_TOKEN` → vault; instance from settings/`FORGEJO_URL`). **`src/commands/install.rs`**: `--forgejo-url`/`--forgejo-token` non-interactive (validate via direct `/api/v1/user`); interactive skippable step; persist via Step 8. **`src/commands/doctor.rs`**: enabled ⇒ valid https URL, vault token present, best-effort version probe. **`src/commands/repo/init.rs`** (lines 156-235): instance-host origins probe `GET /repos/forgejo/{owner}/{name}`. **`src/commands/run/runner.rs`**: `maybe_build_forgejo_credentials` (mirror lines 1102-1139) feeding `StartServices`. **New CLI test** (extend `tests/it/cmd/runner.rs` or a `create` scenario): a fixture git checkout with a forge-hosted origin (matched to a test instance) is admitted as a run target with `instance_url` set; a mismatched host still fails with the provider-aware error.

**Verify:** `cargo nextest run -p fabro-cli` (insta snapshots updated only after `cargo insta pending-snapshots` review).

## Step 11 — Web install wizard

**Modify `apps/fabro-web/app/install-app.tsx`**: step `{ id: "forgejo", label: "Forgejo / Gitea", href: "/install/forgejo" }` (line 67 pattern) — URL + token form, skippable; **`app/install-api.ts`**, **`app/install-router.tsx`**: `testForgejo`/`putForgejo` + route; extend `install-app.test.tsx`, `install-api.test.ts`.

**Verify:** `cd apps/fabro-web && bun test && bun run typecheck && bun run build`.

## Step 12 — Docs and changelog

**Create `docs/public/integrations/forgejo.mdx`**: setup, token scopes (`write:repository`, `read:user`), Gitea compatibility, limitations — single instance, no auto-merge (exact error), draft is **instance-configurable prefix-based** (`WORK_IN_PROGRESS_PREFIXES`; Fabro uses `WIP: `, matching Forgejo/Gitea defaults but may differ on customized instances), no per-run permission scoping, no additions/deletions in PR details, GitHub slug name rules apply, `[run.scm]` override stays GitHub-only. **Modify `docs/public/docs.json`** nav. **Create `docs/public/changelog/2026-09-10.mdx`**. Check README for an integrations list; update only if present.

## Order

Steps 1 → 12 as listed (foundation → client crate → admission → spec → twin → sandbox → workflow → install lib → server → CLI → web → docs). Admission (3) precedes sandbox (6) and workflow (7) so the clone/PR steps are exercised by tests rather than reachable only via hand-built specs.

## Final verification

1. `cargo build --workspace` → 2. `cargo nextest run --workspace` → 3. `cargo +nightly-2026-04-14 fmt --all` + `clippy --workspace --all-targets -- -D warnings` → 4. `cd lib/packages/fabro-api-client && bun run generate` (clean additive diff) → 5. `cd apps/fabro-web && bun test && bun run typecheck`.
6. **Manual live smoke (only unautomatable part):** local Forgejo **and** local Gitea containers; PAT + repo on each; `cargo nextest run -p fabro-forgejo --profile e2e --run-ignored only` with `FORGEJO_LIVE_URL/TOKEN` against both; then `fabro run` a trivial workflow from a Forgejo checkout (admission → clone/push via credential helper → PR with `WIP:` draft) and `fabro pr view/merge/close`. This is the only place dual-forge behavior meets real instances.

## Deliberately not doing, and why

- **Forgejo OAuth login, inbound webhooks, automations/server-submitted forge targets** — excluded by scope answer; `automation_materializer` actively rejects forge targets (Step 3 guard).
- **Multiple instances; per-run permission scoping** — single instance + PAT per answers; `PullRequestLink.instance_url`/`GitRunTarget.instance_url` keep widening non-breaking.
- **`RepositoryProvider::Forgejo`** — origin-only detection can't classify forge hosts without config; forgejo repos report provider `git`.
- **Auto-merge on Forgejo** — no API equivalent; fail-loud error.
- **`[run.scm]` forgejo override** — stays GitHub-only v1; origin comes from the checkout remote (avoids `RunScmSettings` + spec churn for a rare case).
- **Twin git smart-HTTP** — deferred; only optional docker-clone/push e2e needs it.
- **Custom-CA/self-signed TLS escape hatch** — system trust store only.
- **`fabro-github` refactor** — GitHub paths byte-identical; shared seams are fabro-types and the small `CredentialSource` enum.
- **README changes** — only if a verified integrations list exists there.

{"preferred_next_label": "approve"}