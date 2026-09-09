# Implementation Plan: Forgejo integration (core run path)

All decisions below follow the human's answers: **core run path only** (clone + push + PR lifecycle), **PAT-only auth**, **sibling crate `fabro-forgejo`**, **single instance via server config**, **no web wizard** (status card included only because the existing generic endpoint makes it cheap).

Forgejo facts the design relies on: REST API at `<instance>/api/v1` (Gitea-compatible), auth via `Authorization: token <PAT>`, PR create `POST /api/v1/repos/{owner}/{repo}/pulls`, merge `POST .../pulls/{index}/merge` with `{"Do": "merge"|"rebase"|"squash"}`, close `PATCH .../pulls/{index}` `{"state":"closed"}`, branch head `GET /api/v1/repos/{owner}/{repo}/branches/{branch}`, token check `GET /api/v1/user`. Git-over-HTTPS accepts basic auth with any username + PAT as password.

## Files and changes

### Phase 1 — new crate `lib/components/fabro-forgejo/` (created)

| File | Change |
|---|---|
| `Cargo.toml` (new) | Mirror `fabro-github`'s manifest: deps `anyhow, serde, serde_json, strum, thiserror, chrono, tracing, fabro-http, fabro-redact, fabro-types`; `test-support` feature; no `jsonwebtoken` (no app JWT in PAT-only v1). |
| `src/lib.rs` (new) | Core client: `ForgejoCredentials::Pat(String)` (with `SecretString`-style handling mirroring `token_source::SecretString`); `ForgejoContext<'a> { creds, base_url, http_client }`; `instance_api_base(url) -> String` (normalizes instance URL, appends `/api/v1`); `origin_matches_instance(instance_url, origin_url) -> bool` (https + ssh spellings via a local `ssh_url_to_https`, credentials stripped, host compared case-insensitively, port preserved); `parse_forgejo_owner_repo(instance_url, origin_url)` (path must be exactly `{owner}/{repo}`); `embed_token_in_url(url, token)` (username `git`, `DisplaySafeUrl` redaction — copy `fabro-github`'s pattern and tests); `PullRequestApiError { NotFound, Other }` mirror; `MergeStrategy → "merge"/"squash"/"rebase"` mapping (`MergeStrategy` has exactly these 3 variants). API fns taking `&impl HttpClient` (re-use `fabro-github`'s `HttpClient`/`HttpResponse`/`HttpMethod` trait shape by defining a local mirror — a *local* mirror, not a dep on fabro-github, keeping the sibling-crate decision honest): `get_current_user` (200→ok, 401→auth error), `branch_head_sha`, `find_open_pull_request` (list `?state=open`, filter `head.ref == branch`), `create_pull_request` (`{title, body, base, head}`), `get_pull_request`, `merge_pull_request`, `close_pull_request`. Status-code mapping mirrors github's (404 NotFound, 401/403 auth, 405/409 merge-not-possible/conflict). Local `ForgejoPullRequest` deserialize struct → `PullRequestDetails` conversion (`fabro-types` type is provider-neutral enough: title/state/base/head/user; `draft` absent in Gitea payloads → `false`). |
| `src/test_support.rs` (new, `#[cfg(any(test, feature = "test-support"))]`) | `MockHttpClient` mirror of `fabro-github::tests_mock` (route/status/body matcher, request-body assertions). |
| `src/lib.rs` inline `#[cfg(test)] mod tests` | Unit tests: URL parsing/embedding/redaction, `origin_matches_instance` (host, port, ssh, wrong-host rejection), each API fn via mock (success, 404, 401, 409), merge-strategy mapping, PR→`PullRequestDetails` conversion. |

### Phase 2 — static + types + config

| File | Change |
|---|---|
| `lib/foundation/fabro-static/src/env_vars.rs` | Add `FORGEJO_TOKEN` const; add to the aggregate lists (same lists `GITHUB_TOKEN` is in, lines ~225). |
| `lib/foundation/fabro-static/src/secret_registry.rs` | Add `EnvVars::FORGEJO_TOKEN` to `OPTIONAL_VAULT_SECRETS` (both the const array and any duplicate list around line 87). |
| `lib/foundation/fabro-types/src/settings/server.rs` | `ForgejoIntegrationSettings { enabled: bool, url: Option<String> }` (serde kebab/lowercase consistent with neighbors); add `forgejo: ForgejoIntegrationSettings` to `ServerIntegrationsSettings`. |
| `lib/foundation/fabro-config/src/layers/server.rs` | `ForgejoIntegrationLayer { enabled: Option<bool>, url: Option<String> }`; field on `ServerIntegrationsLayer`. |
| `lib/foundation/fabro-config/src/resolve/server.rs` | `resolve_integrations`: resolve forgejo (`enabled` default `false`, url trimmed); validation: `enabled = true` requires `url` set and `https` (error path `server.integrations.forgejo.url`). |
| `lib/foundation/fabro-config/src/tests/resolve_server.rs` | Cases: defaults (disabled/absent url), enabled-without-url error, non-https url error. |
| `lib/foundation/fabro-types/src/pull_request.rs` | `PullRequestLink` gains `host: Option<String>` (`None` = github.com). `html_url()` formats `https://{host}/{owner}/{repo}/pull/{n}` when host set. Serialize emits `host` only when `Some`; `Deserialize` accepts optional `host`; the `html_url` cross-check accepts instance-host URLs via new `PullRequestLink::from_url_for_hosts(url, allowed_hosts: &[&str])`; existing `from_github_url` unchanged (github.com only). All existing github records deserialize identically (no `host` field present). |
| `lib/foundation/fabro-types/src/repository.rs` | No change (see Not doing). |

### Phase 3 — sandbox clone support (Docker)

| File | Change |
|---|---|
| `lib/components/fabro-sandbox/src/clone_source.rs` | Generalize `github_repo_layout` → `repo_layout(origin_url, …)` accepting any https host (owner/repo path-safety checks unchanged); keep `CloneDecision::GitHub` and add `CloneDecision::Forgejo { origin_url, branch, tag, commit_sha }`; the classifier (caller-supplied instance match) decides which. Update the two `parse_github_owner_repo` guard sites (lines 31–36, 318–344) to accept a forgejo origin when it matches the configured instance. |
| `lib/components/fabro-sandbox/src/sandbox_spec.rs` | Docker variant gains `forgejo: Option<ForgejoCloneCredentials>` where `ForgejoCloneCredentials { instance_url: String, token: String }` (owned struct defined in `fabro-sandbox`, constructed from `fabro_forgejo::ForgejoCredentials`). Daytona variant: unchanged (rejected later with explicit error). |
| `lib/components/fabro-sandbox/src/docker.rs` | `clone_github_repo`: when `forgejo` creds present and `origin_matches_instance` → layout via generalized fn, embed PAT in clone URL (`fabro_forgejo::embed_token_in_url`), **no** `InstallationTokenSource` (PAT never expires; `PushCredentialState::new(None)` → refresh no-op, pushes reuse the origin-embedded PAT), record nothing in the token source. Error messages say "GitHub or Forgejo" where the origin check fails. Clone-failure classification: forgejo auth failures are non-retryable (static credential), mirror `CredentialContext::static` handling. |
| `lib/components/fabro-sandbox/src/push_credentials.rs` | `build_token_source`: unchanged for github; document that forgejo runs construct `PushCredentialState` with no source. |
| `lib/components/fabro-sandbox/src/daytona/mod.rs` | Admission guard: forgejo origin + Daytona provider → error "Forgejo origins are supported on the Docker sandbox provider only" at spec construction (one match arm). |
| `lib/components/fabro-sandbox/Cargo.toml` | Add `fabro-forgejo` dep (both normal and dev/test-support listing, mirroring the `fabro-github` dual listing). |

### Phase 4 — workflow pipeline threading + PR creation

| File | Change |
|---|---|
| `lib/components/fabro-workflow/Cargo.toml` | Add `fabro-forgejo` (normal + test-support dev listing, mirroring fabro-github). |
| `lib/components/fabro-workflow/src/run_options.rs` / `pipeline/types.rs` | `RunSpec`/pipeline options gain `forgejo: Option<ForgejoIntegration>` where `ForgejoIntegration { url: String, creds: ForgejoCredentials }` (owned, thread-safe). Existing `github_app` fields untouched. |
| `lib/components/fabro-workflow/src/operations/start.rs` | `StartServices` gains `forgejo: Option<ForgejoIntegration>`; copy into `RunSession`. |
| `lib/components/fabro-workflow/src/pipeline/initialize.rs` | When run origin matches the forgejo instance: skip github `GITHUB_TOKEN` minting; inject `FORGEJO_TOKEN` + `FORGEJO_URL` (instance base) into the sandbox env spec. `[run.integrations.github]` settings are ignored for forgejo-origin runs. |
| `lib/components/fabro-workflow/src/pipeline/pull_request.rs` | Refactor: extract the LLM title/body generation + truncation caps into a provider-neutral `generate_pr_content(...)` (it takes model/catalog/diff/goal — no github types). Add `open_forgejo_pull_request(OpenForgejoPullRequestRequest)` mirroring `open_pull_request`: verify remote head via `fabro_forgejo::branch_head_sha`, adopt existing PR via `find_open_pull_request`, else create; `CreatedPullRequest.link` carries `host = Some(instance_host)`. `auto_merge = true` + forgejo → return error string "auto-merge is not supported for Forgejo pull requests" (fails publish with `PullRequestFailed`, not a silent skip). |
| `lib/components/fabro-workflow/src/pipeline/publish.rs` | Branch in `publish`: if `options.forgejo` present and origin matches instance → `open_forgejo_pull_request` with the same `pr_config`/outcome plumbing; else existing github path. `outcome.pr_url` uses the link's `html_url()` (already host-aware). |
| `lib/components/fabro-workflow/src/pipeline/initialize.rs` tests + `pull_request.rs` tests | Mock-based tests for the forgejo path: content generation shared, create-on-empty, adopt-existing, head-SHA mismatch, auto-merge rejection, host-carrying link. |

### Phase 5 — server

| File | Change |
|---|---|
| `lib/apps/fabro-server/Cargo.toml` | Add `fabro-forgejo` dep. |
| `lib/apps/fabro-server/src/server.rs` | `AppState::forgejo_integration(&self) -> Option<ForgejoIntegration>`: enabled + url from settings, `FORGEJO_TOKEN` from vault (missing token when enabled → the same style of actionable error string as `github_credentials`, i.e. "run `fabro secret set FORGEJO_TOKEN`"); cache instance api base like `github_api_base_url` (field at line 1143 pattern). |
| `lib/apps/fabro-server/src/run_manifest.rs` | Admission (`run_repository_access_check_with`, line ~806): if forgejo integration configured and origin matches → forgejo preflight (embed PAT, `git ls-remote`, same redaction) instead of the GitHub-parse error branch; `check_git_remote_ref` gains the forgejo URL-embedding branch. Thread `ForgejoIntegration` into the credential bundle passed toward `StartServices` construction sites (`automation_materializer.rs`, `worker_runtime.rs`, `server/session_runtime.rs` where `github_app` is currently threaded — same plumbing, new field). |
| `lib/apps/fabro-server/src/server/handler/pull_requests.rs` | Link/merge/close/get: when the stored `PullRequestLink.host` matches the configured instance → forgejo client calls; `pr link` URL validation accepts `https://<instance-host>/owner/repo/pulls/{n}` (Gitea path is `/pulls/`, accept both `/pull/` and `/pulls/`). NotFound mapping unchanged. |
| `lib/apps/fabro-server/src/server/pull_request_supervisor.rs` | PR-creation worker picks forgejo vs github per run origin (reads the same threaded integration). |
| `lib/apps/fabro-server/src/server/handler/system.rs` | Integrations status: add `IntegrationProvider::Forgejo` entry — `configured` = url+token present, connection check = `get_current_user` (mirror the Slack card builder at lines 201–246). |
| `lib/apps/fabro-server/src/server/tests.rs` | Integration tests: admission pass/fail with forgejo origin + token; PR link accept/reject; merge via forgejo twin; system integrations payload includes forgejo. |

### Phase 6 — manifest + CLI + install

| File | Change |
|---|---|
| `lib/components/fabro-manifest/src/lib.rs` | Guard only: local-origin detection (`inspect_local_git`, `github_run_target`) must pass forgejo origins through `GitContext.origin_url` unchanged (they already survive `normalize_repo_origin_url`) and produce no GitHub run target — add a test pinning a forgejo remote round-trip. No new `[run.scm]` grammar. |
| `lib/components/fabro-install/src/lib.rs` | `write_forgejo_settings(url)`: write `[server.integrations.forgejo] enabled = true, url = ...` into settings.toml (mirror `github_integration_table` helper); `FORGEJO_INSTALL_SECRET_KEYS = [FORGEJO_TOKEN]` for cleanup. |
| `lib/apps/fabro-cli/src/args.rs` | `InstallCommand::Forgejo(InstallForgejoArgs { #[arg(long)] url: String, #[arg(long)] token: Option<String> })`; telemetry label `"install forgejo"` (line ~1399). |
| `lib/apps/fabro-cli/src/commands/install.rs` | `install forgejo`: validate https URL, write settings via fabro-install, store token in vault/server secret (same path the github token strategy uses for `GITHUB_TOKEN`). Non-interactive only. |
| `lib/apps/fabro-cli/src/cli_reference.rs` | Add `install forgejo` row (insta snapshots updated via `cargo insta pending-snapshots` → review → `cargo insta accept`). |
| `lib/apps/fabro-cli/src/commands/run/runner.rs` | `maybe_build_forgejo_credentials(settings, vault, run_spec)` sibling of `maybe_build_github_credentials` (line 1102): attach when pull-request enabled or the run git origin matches the configured instance; feeds `StartServices.forgejo`. |

### Phase 7 — API surface + docs

| File | Change |
|---|---|
| `docs/public/api-reference/fabro-api.yaml` | `IntegrationProvider` enum: add `forgejo`. `PullRequestLink` schema (line 12576): optional `host` property (absent = github.com), and `html_url` description notes instance-host URLs. No new endpoints, `RunScmSettings` untouched. |
| `lib/foundation/fabro-api/build.rs` | No new `with_replacement` needed if `IntegrationProvider`/`PullRequestLink` are already replacement types — verify during implementation; if not, add replacements + the required type-identity/JSON-parity test per AGENTS rules. |
| `lib/packages/fabro-api-client/` | `bun run generate` (regenerated TS models, no hand edits). |
| `docs/public/api-reference/… conformance` | Existing fabro-server conformance test catches router/spec drift automatically. |
| `docs/public/integrations/forgejo.mdx` (new) | Setup (instance URL, PAT with `write:repository`/`write:issue` scopes guidance), what works (Docker clone runs, PR create/merge/close, `FORGEJO_TOKEN`/`FORGEJO_URL` in stages), limitations (Docker-only, no auto-merge, no webhooks/OAuth/tracker). |
| `docs/public/docs.json` | Nav entry under integrations. |
| `docs/public/administration/server-configuration.mdx` | `[server.integrations.forgejo]` table + `FORGEJO_TOKEN` vault row. |
| `docs/public/reference/cli.mdx` | `fabro install forgejo` section. |
| `.env.example` | `FORGEJO_TOKEN=` with vault-not-env comment matching the GITHUB block style. |

### Phase 8 — twin test server + e2e

| File | Change |
|---|---|
| `test/twin/forgejo/` (new crate: `Cargo.toml`, `src/lib.rs`, `src/server.rs`, `src/state.rs`, `src/fixtures.rs`, `src/handlers/{mod,user,branches,pulls,git}.rs`, `src/test_support.rs`) | Lean mirror of `test/twin/github`: axum router with `GET /api/v1/user`, `GET /api/v1/repos/{owner}/{repo}/branches/{branch}`, `GET|POST /api/v1/repos/{owner}/{repo}/pulls`, `GET|PATCH /api/v1/repos/{owner}/{repo}/pulls/{index}`, `POST .../pulls/{index}/merge`, and git smart-HTTP (`info/refs`, `upload-pack`, `receive-pack`) for clone/push coverage. Fixture-driven in-memory state; `Authorization: token` check. Workspace member in root `Cargo.toml`. |
| E2E tests (in `fabro-workflow/tests/it/` or `fabro-server`, following `#[e2e_test]` twin usage) | Scripted flows: clone forgejo origin in Docker → push branch → PR created with instance-host link; merge via server handler; adopt-existing PR; token-missing admission failure. |

## Order of work

1. **Phase 1** (fabro-forgejo crate + unit tests) — foundation, nothing else depends on decisions above it.
2. **Phase 2** (static/types/config) — enables settings everywhere.
3. **Phase 3** (sandbox clone) — first end-to-end slice (clone works).
4. **Phase 4** (workflow threading + PR) — completes the run path.
5. **Phase 5** (server) — admission, PR lifecycle handlers, status.
6. **Phase 6** (manifest guard, install, CLI).
7. **Phase 7** (OpenAPI → `cargo build -p fabro-api` → TS client → docs) — API is last among code because the wire shape settles only after handlers exist.
8. **Phase 8** (twin + e2e), then full-workspace verification.

Each phase ends with `cargo nextest run -p <touched crate>` so failures stay local.

## Verification

- Per crate: `cargo nextest run -p fabro-forgejo`, `-p fabro-static`, `-p fabro-types`, `-p fabro-config`, `-p fabro-sandbox`, `-p fabro-workflow`, `-p fabro-server`, `-p fabro-cli`.
- API regen check: `cargo build -p fabro-api` (progenitor) and `fabro-server` conformance test (spec/router drift) must pass after the yaml change.
- TS client: `cd lib/packages/fabro-api-client && bun run generate`, then `cd apps/fabro-web && bun run typecheck && bun test` (web app must compile against regenerated models).
- Snapshots: `cargo insta pending-snapshots` → review each (cli_reference, any settings snapshots) → `cargo insta accept` only for reviewed ones.
- Full gate: `cargo build --workspace`, `cargo nextest run --workspace`, `cargo +nightly-2026-04-14 fmt --check --all`, `cargo +nightly-2026-04-14 clippy --workspace --all-targets -- -D warnings`.
- E2E (twin, no external credentials needed): `cargo nextest run --workspace --profile e2e --run-ignored only` restricted to the new forgejo tests first, then the full e2e profile.
- Manual smoke (one-time, documented in the PR): real Forgejo instance via `docker run` from the official image — `fabro install forgejo --url ... --token ...`, `fabro run` in a repo whose `origin` points at it, verify PR appears, merge via `fabro pr` flow. This is the only check no twin can fully prove (real git-over-HTTPS against Gitea's server).

## Deliberately not doing

- **Webhooks/automations, browser OAuth login, tracker** — out of scope per the scope answer; `github_webhooks.rs`, `auth/github_endpoints.rs`, `fabro-tracker` untouched.
- **Provider-trait refactor of `fabro-github`** — sibling crate per the architecture answer; branching is explicit at the ~6 call sites listed above.
- **Multi-instance / auto-detection** — single instance per the instance answer; `origin_matches_instance` takes the one configured URL. Extending to a map later is a settings + lookup change, not a schema break (`PullRequestLink.host` already carries per-record hosts).
- **Daytona clones for Forgejo** — Daytona's clone is GitHub-shaped via its SDK; v1 rejects with an explicit error (daytona admission guard). Documented in forgejo.mdx.
- **API/RunIntent git targets for Forgejo** — `GitRunTarget.repo` is a host-ambiguous `owner/name` GitHub slug; changing that grammar is a wire-contract break out of scope. Forgejo runs originate from a workspace whose git remote points at the instance.
- **Auto-merge on Forgejo** — no API equivalent; explicit `PullRequestFailed` error, per the "not a silent no-op" rule.
- **Per-run scoped token minting / `[run.integrations.forgejo]`** — PAT-only answer; token scopes are configured on the token in Forgejo itself. The PAT is forwarded verbatim as `FORGEJO_TOKEN`.
- **`RepositoryProvider::Forgejo` variant** — classification requires the configured host, which `fabro-types` (config-free) can't know; forgejo origins stay `Git` in display metadata. Purely cosmetic; avoids threading config into a foundation crate.
- **Web install wizard step / settings-integrations card UI** — lean reading of the last answer; the system-integrations *endpoint* includes forgejo because it's one enum variant + one card-builder function, but no `apps/fabro-web` component work beyond the regenerated client.