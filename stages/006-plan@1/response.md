I have verified every integration point I need. Here is the implementation plan.

---

# Implementation Plan: Forgejo/Gitea Integration (run-path core + install/ops surface)

**Authoritative answers applied:** scope = (b) run-path core + install/ops surface (no OAuth login, no webhooks, no automations/server-side Forgejo run targets); single configured instance per server; dual Forgejo/Gitea branding. Standing decisions from the sweep remain: PAT-only v1, new `fabro-forgejo` crate, host-aware `PullRequestLink`, fail-loud auto-merge error, `WIP:` draft mapping, system-trust TLS.

**Naming convention:** config/env/routes say `forgejo` (`[server.integrations.forgejo]`, `FORGEJO_TOKEN`, `/install/forgejo`, `/repos/forgejo/...`); user-facing text says "Forgejo/Gitea".

---

## Step 1 — Foundation: env vars, secrets registry, types

**Create** `lib/components/fabro-forgejo/` (empty crate scaffold; filled in Step 3). Glob members in root `Cargo.toml:2-8` pick it up automatically; add `fabro-forgejo = { path = "lib/components/fabro-forgejo" }` to `[workspace.dependencies]`.

**Modify `lib/foundation/fabro-static/src/env_vars.rs`** (GitHub block at lines 74-80): add `FORGEJO_URL` and `FORGEJO_TOKEN` constants.

**Modify `lib/foundation/fabro-static/src/secret_registry.rs`**: classify `FORGEJO_TOKEN` as `OptionalVault` (mirrors `GITHUB_TOKEN`, lines 28-31); `FORGEJO_URL` deliberately unclassified (non-secret config, mirrors `GITHUB_BASE_URL`).

**Modify `lib/foundation/fabro-types/src/pull_request.rs`**:
- `PullRequestLink` (line 62) gains `pub instance_url: Option<String>` — the Forge instance origin (e.g. `https://forgejo.example.com`), absent ⇒ github.com.
- `html_url()` (line 70): `Some(instance)` ⇒ `format!("{instance}/{owner}/{repo}/pulls/{number}")` (note Gitea path is `/pulls/`, GitHub's is `/pull/`); `None` ⇒ existing github.com form.
- Custom `Serialize` (line 82): emit `instance_url` only when `Some` (wire stays additive; old events/data deserialize unchanged).
- Custom `Deserialize` (line 96): when `html_url` present, validate against the computed URL for the stored `instance_url` (github or forge form); add `from_forge_url(url: &str, expected_instance: Option<&str>)` parsing `/pulls/{n}` shape (host must match `expected_instance` when given).
- Extend in-file tests: forge serialization round-trip, mixed-shape rejection, old records (no `instance_url`) unchanged.

**Modify `lib/foundation/fabro-types/src/settings/server.rs`**: add `ForgejoIntegrationSettings { enabled: bool, url: Option<String> }` (serde snake_case, `url` optional at wire level); add `pub forgejo: ForgejoIntegrationSettings` to `ServerIntegrationsSettings` (line 242) with `Default { enabled: false, url: None }` so existing configs resolve to disabled.

**Modify `lib/foundation/fabro-types/src/system_integrations.rs`**: add `Forgejo` to `IntegrationProvider` (line 29; strum/serde conventions per existing variants).

**Modify `lib/foundation/fabro-config/src/layers/server.rs`**: add `ForgejoIntegrationLayer { enabled: Option<bool>, url: Option<String> }` and `pub forgejo: Option<ForgejoIntegrationLayer>` on `ServerIntegrationsLayer` (line 202), mirroring the Slack/GitHub layer combine behavior.

**Modify `lib/foundation/fabro-config/src/resolve/server.rs`**: in `resolve_integrations` (line 336): section presence ⇒ `enabled: true`; `url` required when enabled — emit a `ConfigError` with path `server.integrations.forgejo.url` when enabled without a valid `https://` URL (host required, no credentials in URL). Add resolve tests to `lib/foundation/fabro-config/src/tests/resolve_server.rs`.

**Verify:** `cargo nextest run -p fabro-types -p fabro-config -p fabro-static`.

## Step 2 — OpenAPI spec and generated clients

**Modify `docs/public/api-reference/fabro-api.yaml`**:
- `ServerIntegrationsSettings` (line 14552): optional `forgejo` property → new `ForgejoIntegrationSettings { enabled: boolean, url: ["string","null"] }`.
- `IntegrationProvider` (line 15480): enum `[github, slack, forgejo]`.
- `PullRequestLink` (line 12576): optional `instance_url`; `LinkRunPullLinkRequest` (line 12801): description updated to accept `https://github.com/.../pull/{n}` **or** a `{forgejo-instance}/{owner}/{repo}/pulls/{n}` URL matching the configured instance.
- New paths: `POST /install/forgejo/test` (url+token → validated `{ login, version }`), `PUT /install/forgejo` (url+token), `GET /repos/forgejo/{owner}/{name}` (reuse `RepoCheckResponse`, line 16057). New schemas `InstallForgejoTestInput/TestResponse/Input`; extend `InstallSessionResponse` (line 6416) and `InstallGithubSummary`-adjacent summary with an optional forgejo field.

**Modify `lib/foundation/fabro-api/build.rs`**: `with_replacement` entries for `ForgejoIntegrationSettings` → `fabro_types::settings::server::ForgejoIntegrationSettings` (the existing whole-struct replacements for `ServerIntegrationsSettings` and `IntegrationProvider` already route through fabro-types, so no other entries change).

**Extend round-trip tests** `lib/foundation/fabro-api/tests/server_settings_round_trip.rs`, `system_integrations_round_trip.rs`, `pull_request_round_trip.rs`: prove type identity + JSON parity including the new optional fields.

**Regenerate:** `cargo build -p fabro-api` (progenitor), then `cd lib/packages/fabro-api-client && bun run generate`.

**Verify:** `cargo nextest run -p fabro-api`; the generated TS client diff shows only additive shapes.

## Step 3 — `fabro-forgejo` crate (the client)

**Create `lib/components/fabro-forgejo/src/lib.rs`** — mirrors `fabro-github` structure (own local `HttpClient` trait + `HttpResponse` + impl for `fabro_http::HttpClient`; own minimal `SecretString` — crates stay independent):
- `ForgejoCredentials::Pat(String)` (enum shape reserved for future OAuth2); `ForgejoContext<'a> { creds, instance_url, http_client }`.
- `pub fn forgejo_api_url(instance: &str) -> String` → `{instance}/api/v1`.
- Auth header: `Authorization: token {pat}` (Forgejo/Gitea canonical form).
- Endpoints (all under `/api/v1`): `validate_token` (GET `/user`), `server_version` (GET `/version`), `get_repository` (GET `/repos/{owner}/{repo}`), `branch_head_sha` (GET `/repos/{owner}/{repo}/branches/{branch}` → `commit.id`), `find_open_pull_request` (GET `/repos/{owner}/{repo}/pulls?state=open&type=pulls`, paginate `limit`/`page`, **client-side filter** by `head.sha` — Gitea lacks GitHub's `head=` param), `create_pull_request` (POST `/repos/{owner}/{repo}/pulls` body `{title, body, base, head}`; `draft` ⇒ prepend `"WIP: "` to title — Gitea draft semantics), `get_pull_request` (GET `.../pulls/{index}` → `ForgejoPullDetail`; `From<ForgejoPullDetail> for PullRequestDetails`, with `additions/deletions/changed_files = 0` and `draft` derived from title prefix — fields absent from Gitea payloads), `merge_pull_request` (POST `.../pulls/{index}/merge` body `{"Do": "merge"|"squash"|"rebase"}` from `MergeStrategy`), `close_pull_request` (PATCH `.../pulls/{index}` `{"state":"closed"}`).
- `PullRequestApiError { NotFound { owner, repo, number }, Other }` analog with 404/401/403/405/409 status mapping (405 ⇒ not mergeable/method disallowed, 409 ⇒ conflict, mirroring Gitea semantics).
- URL helpers: `instance_host(instance_url)`, `is_instance_origin(origin_url, instance_url) -> bool` (host equality, scheme-agnostic), `parse_owner_repo(origin_url, instance_url) -> Result<(String,String)>` (strips credentials/`.git`/trailing slash, host must equal instance host), `ssh_url_to_https(url)` host-generic, `normalize_origin_url(url, instance_url)`, `embed_token_in_url(origin, token) -> DisplaySafeUrl` (token as password, fixed username — accepted by Gitea basic auth; redacted display via `DisplaySafeUrl`).
- `FORGEJO_URL` env override for the instance URL, mirroring `github_api_base_url()`'s documented-process-env pattern (`#[expect(clippy::disallowed_methods)]` with reason).

**Create** `src/tests_mock.rs` (`#[cfg(test)]` MockHttpClient), `src/test_support.rs` (`#[cfg(any(test, feature = "test-support"))]`, mirroring fabro-github's gating), `tests/integration.rs` (mock-based: every endpoint's happy path + each error status + draft-prefix + pagination + host-mismatch rejection), `tests/live_access.rs` (env-gated `FORGEJO_LIVE_URL`/`FORGEJO_LIVE_TOKEN`, mirroring fabro-github's live_access pattern).

**Verify:** `cargo nextest run -p fabro-forgejo`.

## Step 4 — Forgejo twin (fake server for tests)

**Create `test/twin/forgejo/`** mirroring `test/twin/github/` (auto-included by workspace glob): `src/lib.rs`, `state.rs` (in-memory repos/PRs/tokens), `server.rs` (TestServer with `.no_proxy()` client), handlers: `version.rs`, `users.rs` (`GET /api/v1/user`), `repos.rs` (`GET /api/v1/repos/{owner}/{repo}`), `branches.rs`, `pulls.rs` (list/create/get/patch/merge), plus a minimal git smart-HTTP handler for clone tests (reuse twin-github's `handlers/git.rs` approach). Token auth middleware accepting `Authorization: token X` against configured test tokens.

**Modify `lib/foundation/fabro-test/src/lib.rs`** (mirror the `twin_github` exports at lines 2094-2115): export `twin_forgejo::{AppState, TestServer}` helpers; add twin-forgejo to fabro-test's Cargo.toml.

**Verify:** `cargo nextest run -p fabro-test`; `cargo nextest run -p fabro-forgejo` gains twin-backed tests.

## Step 5 — Sandbox clone support

**Modify `lib/components/fabro-sandbox/src/clone_source.rs`**:
- Add `CloneDecision::Forge { origin_url, branch, tag, commit_sha }` (enum at line 3).
- `decide_clone` (line 266) gains the instance as a parameter — new signature `decide_clone(origin_url, branch, tag, sha, skip_clone, forgejo_instance: Option<&str>)`: origins whose host matches the instance classify as `Forge` (same pinned-SHA rules and precedence as `GitHub`: skip_clone still overrides, absent origin still empty-workspace); github.com origins unchanged; everything else still errors "Clone-based sandboxes currently support GitHub repository origins only" → message updated to name Forgejo/Gitea support and the `run.scm` option.
- Generalize `github_repo_layout` (line 26) → `repo_layout(origin_url, instance: Option<&str>)` producing the same `/repos/<owner>/<repo>` layout via `fabro_forgejo::parse_owner_repo` for forge origins (path validation and symlink commands unchanged).
- Update constructor pre-check call sites (`DockerSandbox::new` line ~195, `DaytonaSandbox::new` line ~535) and the unit tests (add forge-origin accept, host-mismatch reject, pinned-SHA-on-forge cases).

**Modify `lib/components/fabro-sandbox/src/push_credentials.rs`**: generalize `build_token_source` (line 27) to `build_credential_source(github_app, forgejo: Option<&fabro_forgejo::ForgejoCredentials>, clone_origin_url, forgejo_instance) -> Option<CredentialSource>` where `enum CredentialSource { GitHub(Arc<InstallationTokenSource>), Forgejo(SecretString) }`; `PushCredentialState` holds `CredentialSource`; `mint_for_clone()` returns the static PAT for the Forgejo arm (PATs don't expire ⇒ refresh path no-ops on `expires_at() == None`, matching existing `is_static()` handling). Keep the module doc's compare→set-url→record semantics.

**Modify `lib/components/fabro-sandbox/src/docker.rs`**: `DockerSandboxOptions` (line ~130) gains `forgejo_credentials: Option<fabro_forgejo::ForgejoCredentials>` and `forgejo_instance: Option<String>`; `clone_github_repo` (line 909) dispatches: `CloneDecision::Forge` arm clones with the embedded PAT URL (layout via generalized `repo_layout`), pinned-SHA path (init/fetch/checkout -B/verify, lines 988-1042) reused verbatim; `GitCloneStarted` event unchanged.

**Modify `lib/components/fabro-sandbox/src/daytona/mod.rs`**: same dispatch at the init match (lines 1547-1585) — SDK clone with `username/password` from the PAT (lines 1690-1697), `attach_pinned_branch` (line 706) unchanged.

**Modify `lib/components/fabro-sandbox/src/provider.rs`, `provider/docker.rs`, `provider/daytona.rs`**: thread the new options through the factories (docker.rs lines 92-106).

**Verify:** `cargo nextest run -p fabro-sandbox`.

## Step 6 — Workflow engine plumbing

**Modify `lib/components/fabro-workflow/src/run_options.rs`**: `RunOptions` (line 34) gains `forgejo: Option<fabro_forgejo::ForgejoContext<'static>>`-shaped credentials (owned `{ instance_url, pat }` struct to keep `RunOptions` owned).

**Modify `src/operations/start.rs`**: `RunSession` (line 82) and `StartServices` gain `forgejo: Option<ForgejoRunCredentials>`.

**Modify `src/services.rs`**: `EngineServices` (line ~240) gains `forgejo_token: Option<...>`; inject `FORGEJO_TOKEN` into stage env at point of use (mirror `GITHUB_TOKEN` in `resolve_workflow_env`, lines 354-364) when the run origin matches the instance.

**Modify `src/git_bridge.rs`**: add a host-scoped credential helper for the instance (`credential.https://{instance_host}.helper` reading `$FORGEJO_TOKEN`, same secret-free pattern as `GITHUB_CREDENTIAL_HELPER`, line ~38) and SSH→HTTPS `insteadOf` rewrites for the instance host. This is what makes run-branch pushes (`push_run_branch`) authenticate.

**Modify `src/pipeline/initialize.rs`**: when `RunSpec` origin matches the configured instance, pass forgejo credentials into sandbox provider options (Step 5) and set `FORGEJO_TOKEN` in the sandbox env seed.

**Modify `src/run_metadata.rs`** (lines 277-325): meta-branch push parses owner/repo via the forgejo parser and embeds the PAT when origin is a forge origin.

**Modify `src/pipeline/pull_request.rs`**: `OpenPullRequestRequest` (line 447) gains `forgejo: Option<fabro_forgejo::ForgejoContext<'a>>`; `open_pull_request` (line 592) dispatches by origin: github.com ⇒ existing path untouched; instance origin ⇒ forgejo sequence (verify_remote_head via `branch_head_sha` → reconcile via `find_open_pull_request` → `build_pr_content` shared as-is → `create_pull_request` with `WIP:` prefix → **auto_merge: return explicit error** `"auto-merge is not supported for Forgejo/Gitea pull requests; disable run.pull_request.auto_merge"` (fail-loud per decision) → link with `instance_url` set).

**Modify `src/pipeline/publish.rs`** (lines 152-170): select the forgejo context when the origin host matches; keep the `github_app` requirement error only on the github path.

**Verify:** `cargo nextest run -p fabro-workflow`; new tests in `tests/it/` using the twin (PR create happy path, reconcile, auto-merge failure message, github path regression).

## Step 7 — Install writers

**Modify `lib/components/fabro-install/src/lib.rs`**: add `FORGEJO_VAULT_KEYS` (line ~51 area) = `["FORGEJO_TOKEN"]`, `FORGEJO_INSTALL_SECRET_KEYS`, and `write_forgejo_settings(vault, ForgejoSettings { url })` writing `[server.integrations.forgejo] url` (presence ⇒ enabled) + vault secret, mirroring `write_token_settings` (line 279). Unit tests alongside the existing ones.

## Step 8 — Server

**Modify `lib/apps/fabro-server/src/server.rs`**: `AppState` gains resolved forgejo settings + `forgejo_credentials()` (mirror `github_credentials`, lines 1575-1622: vault `FORGEJO_TOKEN`, errors pointing at `fabro install` / `fabro secret set FORGEJO_TOKEN`); register new routes (Step 8b).

**Modify `src/server/handler/pull_requests.rs`**:
- `parse_github_owner_repo_from_url` (line 45): accept forgejo PR URLs when host == configured instance (error message updated).
- Link handler (line 70): `PullRequestLink::from_forge_url` for instance hosts.
- `load_server_github_credentials` (line 80) / `server_github_context` (line 108): add forgejo analogs; GET/merge/close handlers dispatch on `record.instance_url` (merge maps `MergeStrategy` → `Do`; error mapping per Step 3).
- PR create validation (line 339 area): mirror hard/soft credential semantics for forge-origin runs (origin matches instance + no token ⇒ warn + skip PR; clone failure surfaces naturally).

**Modify `src/server/handler/system.rs`**: `forgejo_integration_status` (mirror `github_integration_status`, lines 122-190: enabled/configured/missing `[FORGEJO_TOKEN]`); route `GET /repos/forgejo/{owner}/{name}` (line 31 area) — validates token via `get_repository`, returns `RepoCheckResponse` with the forgejo HTML URL.

**Modify `src/install.rs`**: routes (lines 633-645 area) `POST /install/forgejo/test`, `PUT /install/forgejo`; `InstallAppState` forgejo state; `post_install_finish` writes `write_forgejo_settings` + vault and removes nothing (coexists with GitHub); summary includes forgejo.

**Modify `src/diagnostics.rs`**: forgejo health (mirror lines 362-456): probe `GET {url}/api/v1/version` + `/user` with vault token; remediation strings.

**Modify `src/run_manifest.rs`** (lines 1284-1300): warn when user env defines `FORGEJO_TOKEN` overriding the managed value.

**Verify:** `cargo nextest run -p fabro-server` (includes `tests/it/openapi_conformance.rs` catching any spec/router drift, plus new handler tests against the twin).

## Step 9 — CLI

**Create `lib/apps/fabro-cli/src/shared/forgejo.rs`**: `build_forgejo_credentials(settings) -> Option<ForgejoRunCredentials>` — env `FORGEJO_TOKEN` → vault `FORGEJO_TOKEN`, instance URL from settings (env `FORGEJO_URL` override).

**Modify `src/commands/install.rs`**: non-interactive `--forgejo-url <URL>` `--forgejo-token <TOKEN>` (validate via direct `/api/v1/user` call before persisting, mirroring the GitHub token test); interactive optional "Forgejo/Gitea (self-hosted)" step (URL + PAT prompts, skippable); persist via `write_forgejo_settings` + vault (lines 1631-1691 pattern).

**Modify `src/commands/doctor.rs`**: forgejo checks — `url` is valid https when enabled, vault token present, best-effort `/api/v1/version` reachability with remediation text.

**Modify `src/commands/repo/init.rs`** (lines 156-235): when origin host == instance host, probe `GET /repos/forgejo/{owner}/{name}` instead of the GitHub App installation check.

**Modify `src/commands/run/runner.rs`**: `maybe_build_forgejo_credentials` (mirror lines 1102-1139 hard/soft semantics) feeding `StartServices.forgejo`.

**Verify:** `cargo nextest run -p fabro-cli` (extend `tests/it/cmd/install.rs`, `doctor.rs`, `repo/init` scenarios; insta snapshots updated only after `cargo insta pending-snapshots` review).

## Step 10 — Web install wizard

**Modify `apps/fabro-web/app/install-app.tsx`**: add step `{ id: "forgejo", label: "Forgejo / Gitea", href: "/install/forgejo" }` (line 67 pattern) — instance URL + token form with test button, skippable; state/actions following the existing reducer shape.

**Modify `apps/fabro-web/app/install-api.ts`** + **`app/install-router.tsx`**: `testForgejo`/`putForgejo` calls to the Step 8 routes; route entry.

**Extend** `install-app.test.tsx`, `install-api.test.ts`.

**Verify:** `cd apps/fabro-web && bun test && bun run typecheck && bun run build`.

## Step 11 — Docs and changelog

**Create `docs/public/integrations/forgejo.mdx`**: setup (settings.toml, `fabro secret set FORGEJO_TOKEN`, install wizard), token scope guidance (`write:repository` + `read:user`), Gitea compatibility statement, limitations (single instance, no auto-merge — with the exact error, draft = `WIP:` prefix, no per-run permission scoping, additions/deletions not reported). **Modify `docs/public/docs.json`** (Integrations nav). **Create `docs/public/changelog/2026-09-10.mdx`**. Check `README.md` for an integrations list; update only if one exists.

## Final verification (in order)

1. `cargo build --workspace`
2. `cargo nextest run --workspace`
3. `cargo +nightly-2026-04-14 fmt --all` then `cargo +nightly-2026-04-14 clippy --workspace --all-targets -- -D warnings`
4. `cargo nextest run -p fabro-server` already covers OpenAPI conformance; `cd lib/packages/fabro-api-client && bun run generate` leaves a clean diff
5. `cd apps/fabro-web && bun test && bun run typecheck`
6. **Manual live check (not automated):** run a local Forgejo and a local Gitea container (`docker run codeberg.org/forgejo/forgejo`, `docker run gitea/gitea`), create a user + PAT + repo, then `FABRO_TEST_MODE` live tests: `cargo nextest run -p fabro-forgejo --profile e2e --run-ignored only` with `FORGEJO_LIVE_URL/FORGEJO_LIVE_TOKEN`; end-to-end smoke: configure settings, `fabro run` a trivial workflow on a Forgejo-hosted repo (verifies clone + push via credential helper + PR creation with `WIP:` draft), then `fabro pr view/merge/close` against it. This is the only place the dual-forge claim (Forgejo **and** Gitea) is verified against real software.

## Deliberately not doing, and why

- **Forgejo OAuth login / web auth** — out of scope per answer (a); `server.auth`, `web_auth.rs`, `fabro-oauth` untouched.
- **Inbound webhooks** — out of scope per answer (a); `github_webhooks.rs` untouched, no `/api/v1/webhooks/forgejo` route.
- **Automations / server-submitted run targets on Forgejo repos** — out of scope; `GitRunTarget` validation, `automation_materializer.rs`, `git_checkout.rs` untouched (Forgejo runs originate from local checkouts via CLI).
- **Multiple instances, per-run permission scoping** — single instance + PAT per answers; `PullRequestLink.instance_url` keeps the later widening non-breaking.
- **`RepositoryProvider::Forgejo`** — origin-based detection can't distinguish a Forgejo host from any other git host without config; threading config into run-summary assembly is disproportionate. Forgejo repos report provider `git`.
- **Auto-merge on Forgejo** — no GraphQL API equivalent; fails loudly with a remediation message instead of silently skipping.
- **Custom-CA/self-signed TLS escape hatch** — system trust store only; a documented security default, reversible later.
- **`fabro-github` refactor into a shared forge abstraction** — GitHub paths stay byte-identical; the only shared seams are `fabro-types` (link/settings) and the small `CredentialSource` enum in `push_credentials.rs`.
- **README changes** — only if a verified integrations list exists there.