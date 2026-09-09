All verification gates pass. Here's the complete summary of the implementation.

## Summary

Forgejo is now a second first-class git forge in Fabro: single configured instance (`FORGEJO_URL` + `FORGEJO_TOKEN`), token-only auth, covering clone/push, auto-PR, integration status, repo-access check, `run.scm.forgejo`, `fabro install forgejo`, web UI row, OpenAPI + generated clients, and docs.

**Verification:** `cargo build --workspace` clean; `cargo nextest run --workspace` **8,464 passed** (incl. 995 fabro-cli and 903 fabro-server tests, OpenAPI conformance green); `fmt --check` clean; `clippy -D warnings` clean; TS client regenerated with `typecheck` clean and new web tests passing. Docker isn't available in this sandbox, so the plan's optional live Forgejo wire check wasn't run — fixtures encode the wire shapes (`Authorization: token`, `commit.id`, capital-`Do`, `WIP:` drafts, 201 on edit) as the plan's fallback anticipated.

### Changed, file by file

**Foundation**
- `fabro-static/src/env_vars.rs` — `FORGEJO_URL`, `FORGEJO_TOKEN` consts + registry list.
- `fabro-types/src/repository.rs` — `RepositoryProvider::Forgejo`; `is_forgejo_origin()` (host + base-path match, scheme-agnostic); `RepositoryRef::from_origin_and_source_with_forge()`.
- `fabro-types/src/pull_request.rs` — `PullRequestLink.forge: Option<ForgeInstanceRef>` (skipped when `None` → legacy JSON byte-identical); `PullRequestLink::github/on_forge/from_forgejo_url/from_stored_pr_url`; forge parser accepts `/pulls/{n}` and `/pull/{n}`.
- `fabro-types/src/system_integrations.rs` — `IntegrationProvider::Forgejo`.
- `fabro-types/src/settings/server.rs` — `ForgejoIntegrationSettings { enabled, url }`; added to `ServerIntegrationsSettings`.
- `fabro-config` layers/resolve — `ForgejoIntegrationLayer`; resolution validates https/credential-free URL and trims trailing slash; `run.scm.provider="forgejo"` + `auto_merge` rejected at resolve.

**New crate `lib/components/fabro-forgejo`** — Forgejo API client: `ForgejoCredentials`, `ForgejoContext`, URL parse/normalize/`embed_token_in_url`, `credential_helper_key(base_url)`, endpoints (`user`, `repo`, `branch_head_sha` via `commit.id`, find/create/get/merge/close PR with capital-`Do` and `WIP:` draft convention), `PullRequestForgejoDetail → PullRequestDetails`. 22 tests with a scripted mock client.

**Manifest** — `configured_repo_origin_url` builds `{instance}/{owner}/{repo}` for forgejo (errors when no instance configured); GitHub path unchanged.

**Sandbox** — `CloneDecision::Forgejo`; `decide_clone` takes the instance URL; shared `repo_layout` extracted with `repo_layout_for_origin` dispatch; `ForgejoSandboxCreds` (redacting `Debug`, reuses `SecretString`); `build_token_source` maps forgejo origins to a static-PAT source; Docker + Daytona constructors/clone paths threaded (`git_origin` accessor dropped as unused — daytona uses `is_forgejo()`).

**Workflow engine** — `PullRequestRemote { Github, Forgejo }` enum replaces `OpenPullRequestRequest.github`; `PublishOptions/RunOptions/StartServices` gain `forgejo: ForgejoRunCreds` (redacting `Debug`); publish selects remote by origin; `merge_forgejo_bridge_env` + `FORGEJO_TOKEN` env injection at sandbox-env build; fork of forgejo origins rejected explicitly; `ForgejoRunCreds` re-exported.

**Server** — `AppState.forgejo_base_url()/forgejo_credentials()` (settings → vault/env token); worker `StartServices` wiring; preflight Forgejo-access check + API-based remote-ref probe + sandbox-spec creds; PR get/merge/close route through `server_forgejo_parts` when the stored link carries `forge`; `/repos/forgejo/{owner}/{name}` route (+demo stub); forgejo row in system integrations.

**OpenAPI/clients** — `getForgejoRepo` path, `IntegrationProvider`/`RepositoryProvider` enums, `PullRequestLink.forge`, `ForgejoIntegrationSettings` (all as replacement types with parity tests); progenitor + axios clients regenerated; `fabro-client.get_forgejo_repo`.

**CLI** — `fabro install forgejo` (prompts/`--url`/`FORGEJO_TOKEN` env, validates via `GET /user`, writes vault + settings); `repo init` classifies remotes provider-aware and probes the right endpoint; run-start builds forgejo creds from the vault.

**Web/docs** — integrations page renders a Forgejo row when the server reports one (older-server tolerant) with tests; new `docs/public/integrations/forgejo.mdx` + nav entry.

**Deviations from the plan:** (1) spec edits pulled forward into Phase 1 since replacement-type parity tests need schema and types together; (2) `run_metadata.rs` needed no change — non-GitHub origins already skip the GitHub metadata writer gracefully rather than erroring; (3) `FORGEJO_TOKEN` is baked into the sandbox base env at initialize (static PAT needs no per-stage token source), avoiding a wide `EngineServices` ripple; (4) `runs/mod.rs` picked up a mechanical `Box::pin` (clippy `large_futures` crossed the threshold from the new field).