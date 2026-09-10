All verification is green. Here's the complete summary:

## Forgejo/Gitea Integration — Implementation Summary

### What was built (by plan step)

**Step 1 — Foundation**
- `lib/foundation/fabro-static/src/env_vars.rs` — added `FORGEJO_TOKEN`, `FORGEJO_URL`
- `lib/foundation/fabro-static/src/secret_registry.rs` — `FORGEJO_TOKEN` → OptionalVault; `FORGEJO_URL` unclassified
- `lib/foundation/fabro-types/src/pull_request.rs` — `PullRequestLink` gained `instance_url: Option<String>`; `html_url()` renders `{instance}/{owner}/{repo}/pulls/{n}` for forge links; serialize omits `instance_url` for github.com (old data unchanged); deserialize validates `html_url` against the stored instance; new `from_forge_url()`
- `lib/foundation/fabro-types/src/settings/server.rs` — `ForgejoIntegrationSettings { enabled, url }` (default disabled) + shared `validate_instance_url()`; `ServerIntegrationsSettings` gained `forgejo`
- `lib/foundation/fabro-config` — `ForgejoIntegrationLayer`, resolve (presence ⇒ enabled, URL validated), forge tests
- `lib/foundation/fabro-types/src/system_integrations.rs` — `IntegrationProvider::Forgejo`

**Step 2 — New `lib/components/fabro-forgejo` crate** (mirrors fabro-github, independent): own `HttpClient`/`HttpResponse` + impl for `fabro_http::HttpClient`, own `SecretString`, `ForgejoCredentials::Pat` enum, `ForgejoContext`, endpoints (`validate_token`, `server_version`, `get_repository`, `branch_head_sha`, paginated `find_open_pull_request` with client-side SHA filter, `create_pull_request` with `WIP: ` draft prefix, `get_pull_request` → `PullRequestDetails` with zero diff stats, `merge_pull_request` with shared strategies, `close_pull_request`), `PullRequestApiError`, URL helpers (`instance_host`, `is_instance_origin`, `parse_owner_repo`, `ssh_url_to_https`, `normalize_origin_url`, `embed_token_in_url` with `oauth2` username), `FORGEJO_URL` env override. 14 tests passing (13 inline/integration vs twin + live-gated).

**Step 3 — Run admission**
- `fabro-types/src/run_intent.rs` — `GitRunTarget.instance_url` (optional, `deny_unknown_fields`-safe); `validate()` checks instance URL grammar and builds `origin_url = {instance}/{owner}/{repo}`; `ValidatedGitRunTarget::instance_url()` accessor; GitHub path byte-identical
- `fabro-manifest` — `observe_git_run_target` gained `forge_instance_url`; `run_target_for_origin` classifies github.com vs configured instance vs unsupported; forge tests (forge target, github-with-forge-configured, host mismatch)
- `fabro-automation` — forge targets rejected at `validate_target` with an explicit "Forgejo/Gitea automation targets are not supported" error (chokepoint guard, stronger than the materializer-only check the plan proposed)
- `fabro-cli` `commands/run/create.rs` — threads the instance through observation; provider-aware error messages (`canonical_forge_name`)

**Step 4 — OpenAPI + clients** — spec: forgejo in `ServerIntegrationsSettings`, `IntegrationProvider`, `PullRequestLink.instance_url`, `GitRunTarget.instance_url`, link-request description, `POST /install/forgejo/test`, `PUT /install/forgejo`, `GET /repos/forgejo/{owner}/{name}`, `InstallForgejo*` schemas, install-session `forgejo` summary; `fabro-api/build.rs` replacement; extended round-trip tests; TS client regenerated (clean additive diff).

**Step 5 — `test/twin/forgejo`** — new workspace member: in-memory AppState (tokens, repos, branch SHAs, PRs), axum TestServer, handlers for version/user/repos/branches/pulls (list+paginate, create with duplicate 409, get, edit→201, merge with 405/409), `Authorization: token X` auth; self-tests; `fabro-test` exports `TwinForgejo`/`ForgejoAppState`/`ForgejoTwinPullRequest`.

**Step 6 — Sandbox** — `CloneDecision::Forge` arm; `decide_clone`/`repo_cloned_for_record` take the instance; `github_repo_layout`→`repo_layout`, `GitHubRepoLayout`→`RepoLayout`; `build_token_source`→`build_credential_source` returning `CredentialSource { GitHub, Forgejo }`; provider-aware lease/refresh (static generation-0 PAT, forge username embed); `DockerSandbox`/`DaytonaSandbox` gained a `forgejo: Option<ForgejoSandboxCredentials>` constructor param (mirroring `github_app`, instead of stuffing credentials into options); `SandboxSpec`/`SandboxCreateSpec` thread it; `clone_github_repo`→`clone_git_repo` dispatches.

**Step 7 — Workflow engine** — `RunOptions.forgejo`, `StartServices.forgejo`, `RunSession`/`PublishOptions` plumbing; `open_pull_request` dispatches on origin host — new forge path (branch verify → reconcile → shared LLM content → create → link with instance) and fail-loud auto-merge error; internal `Event::PullRequestCreated` and `PullRequestCreatedProps` gained optional `instance_url` (serde-default, additive); `EngineServices`/`WorkflowToolEnvProvider` inject `FORGEJO_TOKEN` when origin matches; git bridge gained `merge_forge_bridge_env` (host-scoped helper reading `$FORGEJO_TOKEN` + SSH→HTTPS `insteadOf`); `run_metadata` meta-branch pushes use a static-PAT auth provider for forge origins.

**Step 8 — Install writers** — `FORGEJO_INSTALL_SECRET_KEYS`/`FORGEJO_VAULT_KEYS`, `write_forgejo_settings` + test.

**Step 9 — Server** — `AppState::forgejo_settings/forgejo_credentials/forgejo_sandbox_credentials`; `GET /repos/forgejo/{owner}/{name}`; system-integrations forgejo status; diagnostics forgejo health (`/api/v1/version` probe); install wizard test/put/finish/session coverage; `RunPrInputs::extract` and the PR link handler accept instance-host URLs (`from_forge_url`); GET/merge/close dispatch on stored `record.instance_url`; `FORGEJO_TOKEN` env-override preflight warning.

**Step 10 — CLI** — `shared/forgejo.rs` (instance resolution env→settings, credentials env→vault); `maybe_build_forgejo_credentials` feeding `StartServices`; doctor gained an async local Forgejo check (URL validation, vault token, best-effort version probe); `repo init` probes `GET /repos/forgejo/...` for instance-host origins.

**Step 11 — Web** — install wizard gained a skippable "Forgejo / Gitea" step (URL + token form, session-prefill, test-then-save), `install-api.ts` client functions.

**Step 12 — Docs** — `docs/public/integrations/forgejo.mdx` (setup, scopes, limitations), `changelog/2026-09-10.mdx`, `docs.json` nav entries.

### Adaptations from the plan (and why)
1. **Instance-URL validation lives in fabro-types** (`validate_instance_url`), not fabro-config — config, CLI, server, and admission all reuse it; loopback `http://` is allowed so the twin and local instances work.
2. **Automation guard placed in `fabro-automation::validate_target`** (the real validation chokepoint) instead of only `automation_materializer.rs` — it rejects forge automation targets at creation time across all callers.
3. **`ForgejoSandboxCredentials` is one bundled constructor param** on the sandboxes (mirroring the existing `github_app` pattern) rather than two fields added to `DockerSandboxOptions`, which is a Clone-able config struct that never held credentials.
4. **CLI `fabro install` non-interactive flags were trimmed**: the forge step is delivered via the browser install wizard (server routes + web UI, both done), which `fabro install` orchestrates; the token can also be set via `fabro secret set FORGEJO_TOKEN` + settings.
5. **Twin git smart-HTTP handler deferred** per plan — REST endpoints cover every client call in v1.
6. **Deviation forced by environment**: the manual dual-instance live smoke (`FORGEJO_LIVE_URL`/`FORGEJO_LIVE_TOKEN` e2e) is implemented but requires real containers, which this environment lacks — it is env-gated and skips cleanly.

### Verification
- `cargo nextest run --workspace`: **8468 passed, 0 failed** (224 skipped, live/env-gated)
- `cargo +nightly-2026-04-14 clippy --workspace --all-targets -- -D warnings`: clean
- `cargo +nightly-2026-04-14 fmt --all -- --check`: clean
- `cargo build -p fabro-api` regeneration + all 226 fabro-api tests pass; TS client regenerated; `bun run typecheck` clean; web `bun test` shows only the 27 failures that also fail on the untouched tree (environmental, pre-existing)
- No commits made; nothing merged