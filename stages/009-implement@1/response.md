The Forgejo integration is complete. All phases of the plan were implemented, verified with a green workspace build, 2,200+ passing tests, clean `fmt`, and clean `clippy -D warnings` on the pinned nightly.

## What changed, file by file

### Phase 0 — Foundation
- **`lib/components/fabro-forgejo/` (new crate)** — Forgejo (Gitea-lineage) REST v1 client: `ForgejoInstance` (validated HTTPS base URL, subpath support, serde-validated), `ForgejoCredentials` (redacted Debug), `ForgejoConfig`, `ForgejoContext`, URL helpers (`parse_forgejo_owner_repo`, `is_forgejo_origin`, `normalize_forgejo_origin_url`, `embed_token_in_url` on `DisplaySafeUrl`), instance-scoped git credential-helper constants, and ops: `get_authenticated_user`, `get_repository`, `branch_head_sha` (Gitea `commit.id`), `find/create/get/merge/close_pull_request`, `enable_auto_merge` (`Do` + `merge_when_checks_succeed`), returning GitHub-parity `NotFound`/`Other` errors. 31 unit tests incl. the Forgejo→`PullRequestGithubDetail` projection fixture; `ForgejoInstance::new_allowing_http` added later under `test-support` for loopback twins.
- **`fabro-static`** — `FORGEJO_TOKEN`/`FORGEJO_URL`/`FORGEJO_WEBHOOK_SECRET` env vars; the two secrets in the vault scrub registry.
- **`fabro-types`** — `ForgejoIntegrationSettings` (enabled/url), `RunIntegrationsForgejoSettings { token }` (serde-skip when default so old payloads stay byte-identical), `PullRequestLink.forge: Option<String>` with `from_forgejo_url` (accepts `/pulls/` and `/pull/`, instance-host-checked, omitted from JSON when None), `PullRequestCreatedProps.forge`, `IntegrationProvider::Forgejo`.
- **`fabro-config`** — `ForgejoIntegrationLayer`/`RunIntegrationsForgejoLayer` + resolvers.

### Phase 1 — Sandbox
- **`fabro-sandbox`** — `CloneDecision::Forge`, `decide_clone`/`clone_repo_layout` (renamed from `GitHubRepoLayout`) take the instance and dispatch; `SandboxSpec`/`SandboxCreateSpec` gain `forgejo: Option<ForgejoSandboxConfig>` (alias of `ForgejoConfig`); Docker clone embeds the static PAT for Forge origins; Daytona uses PAT as basic-auth for clone + one-time post-clone set-url; push-credential refresh source stays GitHub-only (PATs don't expire).

### Phase 2 — Workflow
- **`fabro-workflow`** — `RunOptions`/`StartServices`/`PublishOptions` carry `ForgejoConfig`; `SandboxEnvSpec.forgejo_requested`; `build_sandbox_env` resolves `FORGEJO_TOKEN` only when the origin is on the instance (fails closed otherwise) and injects instance-scoped git-bridge entries (`git_bridge::merge_forgejo_bridge_env`); `EngineServices`/`WorkflowToolEnvProvider` expose the token as `FORGEJO_TOKEN`; `PullRequestHost` enum dispatches all five PR touch points with a normalized `ExistingPullRequest`; publish picks the host; PR-created events carry `forge`.

### Phase 3 — Server
- **`fabro-server`** — `forgejo_webhooks.rs` (HMAC verify accepting `X-Forgejo-Signature`/`X-Gitea-Signature`/`X-Hub-Signature-256`, inert verify-and-log handler, route mounted only with a secret); startup-resolved `state.forgejo`; `forgejo_config()` resolver; `StartServices.forgejo` threading; preflight Forgejo repository-access check + informational Forgejo-token check; supervisor and PR handlers dispatch get/merge/close/create by `link.forge` with `integration_unavailable` when unconfigured; run-link endpoint accepts instance URLs; install endpoints `PUT /install/forgejo/token` + `/test` with settings/vault persistence via `fabro-install::write_forgejo_settings`; system-integrations forgejo status; diagnostics connectivity check; startup enable/disable log.
- **`docs/public/api-reference/fabro-api.yaml`** — new paths/schemas (forgejo install, webhook, `ForgejoIntegrationSettings`, `RunIntegrationsForgejoSettings`, `PullRequestLink.forge`, `IntegrationProvider` enum). TS client regenerated.

### Phase 4–7 — CLI, Web, Twin, Docs
- **CLI** — `shared/forgejo.rs` resolver (env + vault), optional install-wizard step with PAT validation, `pr link` forge URL classification with `forgejo #N` labels, doctor connectivity check.
- **Web** — regenerated Axios client, `testInstallForgejoToken`/`putInstallForgejoToken` wrappers, optional Forgejo wizard step (URL+PAT+Test+Skip), chip fixture test for instance URLs.
- **`test/twin/forgejo` (new)** — Gitea-lineage axum twin with real bare-repo smart-HTTP fixtures; 17 tests drive the real client ops end-to-end.
- **Docs** — `docs/public/core-concepts/forgejo.mdx`, changelog `2026-09-11.mdx`, nav entries, `AGENTS.md` (CLAUDE.md symlink) sandbox/architecture wording, `.env.example` live-test vars.

### Deviations from the plan (and why)
- **Mock-client tests live in-module** (`src/tests.rs`), not `tests/integration.rs` — the repo keeps client tests in `#[cfg(test)]` modules.
- **Preflight forge access check uses the instance API** (`get_repository` + `branch_head_sha`) instead of dispatching `git ls-remote` with an embedded-token URL — same guarantee, one code path, and tokens never touch argv.
- **`PullRequestHost`/`ForgejoConfig` re-exports** were needed at `fabro_workflow::pull_request` for the server, which the plan's threading implied but didn't name.
- **Manifest lane unchanged** (devil's-advocate note): `fabro-manifest` already preserves the full `git.origin_url` for non-GitHub origins, which is exactly what the publish-side dispatch consumes — verified rather than modified.