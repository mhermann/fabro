Re-reading surfaced two additional facts that sharpen the fix: the server's intent-validation point is **`handler/runs.rs:629`** (`intent.target.validate()`), not `run_compiler.rs`; and **`GitRepoCache.bare_dir` keys bare clones by `owner/repo` only** (`git_checkout.rs:101-106`), so a forgejo repo would collide with a same-named github repo in the checkout cache. Both are now in the plan.

**What changed from the critique, in brief:**
1. **Worker credential layer fixed**: the plan now names `lib/apps/fabro-cli/src/commands/run/runner.rs` (`maybe_build_github_credentials`, lines 1102–1132; `load_worker_vault`, line 139) and adds its forgejo twin — the actual place PATs reach the worker, not `spawn_env.rs`.
2. **`automation_materializer.rs` added**: forge-aware remote resolution, endpoint-aware validation at line 241, and the `GitRepoCache` collision fix.
3. **The forge→origin seam is now compilable**: `ValidatedGitRunTarget.git` stays non-optional; a new `validate_with_endpoints(ForgeEndpoints)` fills the projection where settings exist (`handler/runs.rs`, `automation_materializer.rs`), while bare `validate()` stays grammar-only for `forge: Some(...)` and defers projection to server admission. The "sentinel descriptor" idea is dropped — declared-forgejo manifests keep the local remote's real URL and carry the `forge` marker.

---

# Implementation Plan: Forgejo integration (run-target parity, single instance, PAT-only, parallel crate)

## Design summary

- **Wire**: `GitRunTarget.forge: Option<String>` (`"forgejo"`); `PullRequestLink.forge_base_url: Option<String>` (normalized base, `None` = github.com, `/pulls/N` URLs). Additive serde defaults; no migrations.
- **Seam**: `ForgeEndpoints { forgejo: Option<String> }` in `fabro-types`. `validate()` = grammar (unchanged for `forge: None`; defers projection for forge targets). `validate_with_endpoints()` = full projection, called only where settings exist. Server admission and automation materialization are the only endpoint-aware validators.
- **Single instance**: `[server.integrations.forgejo] { enabled, url }`; PAT in vault as `FORGEJO_TOKEN`. Forge detection everywhere = origin URL host matches the configured base. Other hosts rejected at admission with an error naming the configured URL.
- **Static PAT, no refresh**: token embedded at clone; push-credential refresh machinery stays GitHub-only.
- **`fabro-github` untouched** except nothing — all sharing via `fabro-types` plus one sandbox-internal variant rename.

## Files created

| Path | Contents |
|---|---|
| `lib/components/fabro-forgejo/Cargo.toml` | Deps: `anyhow`, `serde`, `serde_json`, `chrono`, `thiserror`, `fabro-http`, `fabro-redact`, `fabro-types` (workspace versions). Picked up by the `lib/components/*` glob. |
| `lib/components/fabro-forgejo/src/lib.rs` | `ForgejoCredentials { token }` (local `SecretString` newtype — no cross-crate reuse of fabro-github's); `ForgejoContext { creds, base_url, http_client }`; `HttpClient`/`HttpResponse`/`HttpMethod` mirror trait + `fabro_http::HttpClient` impl (accepted duplication); `forgejo_headers` → `Authorization: token <t>`; `parse_owner_repo(base, url)` for `https://host[:port]/owner/repo[.git]` and `git@host:owner/repo.git`; `normalize_origin_url`; `branch_head_sha` (`GET /api/v1/repos/{o}/{r}/branches/{b}`); `find_open_pull_request`, `create_pull_request`, `get_pull_request → PullRequestDetails` (Gitea field map: `has_merged`→`merged`, `mergeable`, `base.ref`/`head.ref`), `merge_pull_request` (`POST …/pulls/{n}/merge`), `close_pull_request` (`PATCH …/pulls/{n}`); `ForgejoCloneCredentials` + `authenticated_clone_url(base, owner, repo, token) -> DisplaySafeUrl` (basic auth, username `git`); auto-merge attempted via the create payload's auto-merge field when the twin models it, else typed `Unsupported`. |
| `lib/components/fabro-forgejo/src/tests_mock.rs` | `#[cfg(test)]` mock `HttpClient` (mirror of `fabro-github/src/tests_mock.rs`). |
| `test/twin/forgejo/Cargo.toml`, `src/{lib.rs,server.rs,state.rs,fixtures.rs,handlers/{mod.rs,repos.rs,pulls.rs,branches.rs},test_support.rs}` | Twin instance (picked up by `test/twin/*` glob): `Authorization: token` check, repo/branch/pulls endpoints in Gitea payload shape, scripted merge/close behavior. |
| `docs/public/integrations/forgejo.mdx` | Settings block, `fabro secret set FORGEJO_TOKEN`, declared-runs via `[run.scm] provider = "forgejo"`, capability matrix vs GitHub (no login/webhooks/installer; auto-merge caveat; single-instance contract: the configured instance is authoritative for local runs). |
| `docs/public/changelog/2026-09-10.mdx` | Entry noting twin-verified status. |

## Files modified, in order

### Phase 1 — Foundation (fabro-types, fabro-static)

1. **`lib/foundation/fabro-static/src/env_vars.rs`** — `FORGEJO_TOKEN` const; add to the secret list (~lines 225–229).
2. **`lib/foundation/fabro-static/src/secret_registry.rs`** — register in both lists (~28–31, ~87–90).
3. **`lib/foundation/fabro-types/src/settings/server.rs`** — `ServerIntegrationsSettings.forgejo: ForgejoIntegrationSettings { enabled: bool, url: String }`, `Default { enabled: false, url: String::new() }`.
4. **`lib/foundation/fabro-types/src/run_intent.rs`** — `GitRunTarget.forge` field; `ForgeEndpoints`; `ValidatedGitRunTarget.forge_base: Option<String>`; `validate_with_endpoints()` (forgejo requires `endpoints.forgejo`, else `GitCoordinateValidationError::ForgeEndpointMissing` naming `server.integrations.forgejo.url`; sets `git.origin_url = {base}/{owner}/{repo}.git`); bare `validate()` on forge targets = grammar-only with empty `git.origin_url` and a doc contract that server admission re-validates; `RunTarget::validate_with_endpoints` wrapper; new error variants `Forge`, `ForgeEndpointMissing`. `forge: None` paths byte-identical.
5. **`lib/foundation/fabro-types/src/pull_request.rs`** — `PullRequestLink.forge_base_url`; `html_url()` branch (`/pull/` vs `/pulls/`); custom Serde round-trip + cross-check; `pull_request_link_from_url(url, expected_base: Option<&str>)`.
6. **`lib/foundation/fabro-types/src/repository.rs`** — `RepositoryProvider::Forgejo` (sniffer unchanged; classification at settings-aware call sites).
7. **`lib/foundation/fabro-types/src/system_integrations.rs`** — `IntegrationProvider::Forgejo`.
8. **`lib/foundation/fabro-types/src/lib.rs`** — re-exports (`ForgejoIntegrationSettings`, `ForgeEndpoints`).

### Phase 2 — `fabro-forgejo` + twin (created above)

### Phase 3 — API contract

9. **`docs/public/api-reference/fabro-api.yaml`** — additive: `GitRunTarget.forge`, `PullRequestLink.forge_base_url`, `RepositoryProvider` + integration provider enum entries. No new paths.
10. **Regenerate**: `cargo build -p fabro-api`; `cd lib/packages/fabro-api-client && bun run generate`.
11. **`lib/foundation/fabro-api/tests/pull_request_round_trip.rs`** — forge-shaped round-trip/parity cases.

### Phase 4 — Server

12. **`lib/apps/fabro-server/src/server.rs`** — `forgejo_credentials(&self, settings)` reading vault `FORGEJO_TOKEN` (mirrors `github_credentials`, ~1575–1615); `ForgeEndpoints::from_settings` helper.
13. **`lib/apps/fabro-server/src/server/handler/runs.rs`** — line 629: `intent.target.validate()` → `validate_with_endpoints(&endpoints)`; a forge target with no configured/enabled integration → the existing target-validation error shape carrying `ForgeEndpointMissing`.
14. **`lib/apps/fabro-server/src/run_compiler.rs`** — when the validated target's `forge_base` is set: `RepositoryRef` gets `RepositoryProvider::Forgejo`; compiled `git` uses the endpoint-derived `origin_url` (already on `ValidatedGitRunTarget.git`).
15. **`lib/apps/fabro-server/src/automation_materializer.rs`** — `AutomationGitRemoteResolver::resolve(repo, forge_base: Option<&str>)`; `ServerGitHubRemoteResolver` holds `ForgeEndpoints` + forgejo PAT (resolved via `state.forgejo_credentials`); forge branch → `git_checkout::forgejo_clone_url(base, repo)` + PAT `GitAuthConfig`; line 241 (`input.target.validate()`) and the `workflow_source` validation → `validate_with_endpoints`.
16. **`lib/apps/fabro-server/src/git_checkout.rs`** — `forgejo_clone_url(base, repo)`; `GitRepoCache.bare_dir`/lock key gain a forge discriminant: github keeps `<cache>/<owner>/<repo>.git` unchanged, forgejo uses `<cache>/forgejo/<host>/<owner>/<repo>.git` (avoids same-slug collision, preserves existing caches); `resolve_git_read_auth_config` forge branch (PAT basic auth, no API token-exchange call — Forgejo PATs are used directly).
17. **`lib/apps/fabro-server/src/run_manifest.rs`** — preflight: `needs_forgejo_credentials` when the run origin matches the configured base; remote-ref check (~800–830) accepts forgejo origins (parse against base; authenticated URL from PAT).
18. **`lib/apps/fabro-server/src/server/handler/system.rs`** — `forgejo_integration_status` appended to the integrations response.
19. **`lib/apps/fabro-server/src/server/handler/pull_requests.rs`** — link endpoint parses with the settings-aware parser; details fetch branches on `link.forge_base_url` → `ForgejoContext` + `forgejo_get_pull_request`; supervisor path in **`server/pull_request_supervisor.rs`** gets `server_forgejo_context`.
20. **`lib/apps/fabro-server/src/spawn_env.rs`** — unchanged (worker resolves the PAT from the vault itself via storage paths already allowlisted; the GitHub PEM env path is App-specific and has no forgejo counterpart).

### Phase 5 — Worker + workflow

21. **`lib/apps/fabro-cli/src/commands/run/runner.rs`** — `maybe_build_forgejo_credentials(settings, vault)` next to `maybe_build_github_credentials` (1102–1132): resolved `server.integrations.forgejo` + vault `FORGEJO_TOKEN`; required when the run's git origin matches the configured base (clone/push) or pull-request-enabled forgejo run; feeds a new `StartServices.forgejo` field.
22. **`lib/components/fabro-workflow/src/operations/start.rs`** + **`pipeline/types.rs`** — `StartServices`/options carry `forgejo: Option<ForgejoCredentials>` + the forgejo base from the settings snapshot; `SandboxEnvSpec.origin_url` doc update only.
23. **`lib/components/fabro-workflow/src/pipeline/initialize.rs`** — accept forgejo creds; no repository-access validation (single PAT).
24. **`lib/components/fabro-workflow/src/services.rs`** — `resolve_workflow_env` inserts `FORGEJO_TOKEN` for forgejo runs (parallel to the `GITHUB_TOKEN` block, ~356–368).
25. **`lib/components/fabro-workflow/src/git_bridge.rs`** — `FORGEJO_CREDENTIAL_HELPER` + `credential.https://<instance-host>.helper` applied when the run's forge matches; SSH→HTTPS rewrite for the instance host; github paths untouched.
26. **`lib/components/fabro-workflow/src/pipeline/publish.rs`** + **`pipeline/pull_request.rs`** — client enum (`Forge::Github(..) | Forge::Forgejo(..)`) selected by origin-vs-base match; forgejo branch calls fabro-forgejo ops; auto-merge `Unsupported` → `warn!` + continue.
27. **`lib/components/fabro-workflow/src/run_metadata.rs`**, **`lifecycle/git.rs`** — audit-only (consume `origin_url` opaquely); comment changes if any.

### Phase 6 — Sandbox

28. **`lib/components/fabro-sandbox/src/clone_source.rs`** — `github_repo_layout` → `repo_layout(origin_url, forgejo_base: Option<&str>)` (parses against github.com or the base; SSH spellings of either); `CloneDecision::GitHub` → `CloneDecision::Git` (mechanical rename); error text names both forges.
29. **`lib/components/fabro-sandbox/src/sandbox_spec.rs`** — builders take `forgejo: Option<ForgejoRemote { base_url, token }>`; forge origins use `authenticated_clone_url`.
30. **`lib/components/fabro-sandbox/src/push_credentials.rs`** — `build_token_source` returns `Ok(None)` for forgejo origins (static PAT embedded at clone; refresh is a GitHub-app concern).
31. **`lib/components/fabro-sandbox/src/docker.rs`**, **`daytona/mod.rs`** — clone/fetch URLs from the forgejo-authenticated URL; docker adds the host-scoped `FORGEJO_TOKEN` credential helper for forgejo runs; Daytona SDK clone gets the authenticated URL.
32. **`lib/components/fabro-sandbox/src/error.rs`** — line ~143 message generalized.

### Phase 7 — Manifest & CLI

33. **`lib/components/fabro-manifest/src/lib.rs`** — declared-forgejo path: when `run.scm.provider == "forgejo"` (+ owner/repository, or local remote path segments), emit `GitRunTarget { repo: slug, forge: Some("forgejo") }`; `inspect_local_git` keeps the local remote's real URL as `origin_url` (no github normalization on it — use a pass-through normalize for non-github hosts); ambient github detection unchanged; `github_run_target` untouched.
34. **`lib/apps/fabro-cli/src/commands/pr/link.rs`** — `record_label` picks `github #N` / `forgejo #N` from `link.forge_base_url`.

### Phase 8 — Web, docs, wiring

35. **`apps/fabro-web/app/routes/settings-integrations.tsx`** — `ForgejoPanel` (status display only).
36. **`docs/public/docs.json`** — nav entry after `integrations/github` (~line 95).
37. **Cargo.toml edits**: `fabro-forgejo` dep in `fabro-server`, `fabro-workflow`, `fabro-sandbox`, `fabro-manifest`, `fabro-cli`; `test/twin/forgejo` dev-dep in `fabro-server` and `fabro-workflow`. Root `Cargo.toml` untouched (globs).

## Verification

- **Phase 1**: `cargo nextest run -p fabro-types -p fabro-static` — serde round-trips incl. old github-shaped JSON (no forge fields) deserializing identically; `deny_unknown_fields` still rejects junk; `validate_with_endpoints` error cases.
- **Phase 2**: `cargo nextest run -p fabro-forgejo` — header scheme, URL matrix (https/http/port/SSH/`.git`), `has_merged` mapping, merge/close bodies, NotFound→`PullRequestApiError::NotFound`.
- **Phase 3**: `cargo build -p fabro-api`; `cargo nextest run -p fabro-api`; `bun run generate` + `bun run typecheck` in fabro-web.
- **Phases 4–6**: `cargo nextest run -p fabro-server -p fabro-sandbox -p fabro-workflow -p fabro-manifest` — spec/router conformance test (fabro-server) catches OpenAPI drift; new tests: admission rejects forge target without integration; automation resolver returns the forgejo URL and the cache discriminates `forgejo/<host>/…`; docker command tests assert the authenticated clone URL + host-scoped helper; PR pipeline against `test/twin/forgejo`; worker credential builder unit tests mirroring the github ones in `runner.rs`'s test module.
- **Phase 7**: `cargo nextest run -p fabro-cli --test it` (pr link).
- **Final**: `cargo build --workspace`; `cargo nextest run --workspace`; `cargo +nightly-2026-04-14 fmt --all` + `--check`; `cargo +nightly-2026-04-14 clippy --workspace --all-targets -- -D warnings`; `cd apps/fabro-web && bun test && bun run typecheck`.
- **Manual, no tooling substitute**: real-instance clone/PR is unverifiable here; the twin is the stated substitute and the doc/changelog say so.

## Deliberately not doing

- No OAuth/browser login, webhooks, install-wizard UI, tracker (human chose PAT + run-target tier; webhooks were tier-C; GitHub's tracker is GraphQL, Forgejo has none).
- No multi-instance/Codeberg: single configured instance is authoritative — for local declared-forgejo runs the configured instance overrides the remote's own host (documented in forgejo.mdx).
- No `fabro-github` changes; only the mechanical `CloneDecision::GitHub`→`Git` rename inside fabro-sandbox.
- No migrations (additive serde fields; existing data all github-shaped). No `FORGEJO_BASE_URL` env (settings-only; server-secrets-strategy discourages bespoke env fallbacks).
- Auto-merge best-effort with a warning if the API lacks support.
- No breaking wire changes; old clients/servers interoperate.