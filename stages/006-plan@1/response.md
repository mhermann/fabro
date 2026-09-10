I have the full data-flow map now. Here is the implementation plan.

---

# Implementation Plan: Forgejo integration (run-target parity, single instance, PAT-only, parallel crate)

## Design summary (fixed by the human's answers)

- **Forge marker, not host sniffing**: `GitRunTarget` gains `forge: Option<String>` (`"forgejo"`). The server derives `GitContext.origin_url` from its single configured instance (`[server.integrations.forgejo] url`). The CLI never needs the instance URL; Forgejo runs are **declared** via `[run.scm] provider = "forgejo"` + `owner` + `repository`, not ambient remote detection.
- **`PullRequestLink` carries `forge_base_url: Option<String>`** (normalized, no trailing slash; `None` = github.com). `html_url()` renders `{base}/{owner}/{repo}/pulls/{n}` when present. Self-contained: links stay renderable by any process (CLI, web) without settings.
- **Static PAT everywhere**: vault secret `FORGEJO_TOKEN`; no token minting, no refresh machinery (push-credential refresh stays GitHub-only; the PAT embedded at clone time never expires).
- **`fabro-github` is untouched.** All sharing happens through existing `fabro-types` wire types plus one small sandbox-local extension.

## Files created

| Path | Contents |
|---|---|
| `lib/components/fabro-forgejo/Cargo.toml` | New crate; deps: `anyhow`, `serde`, `serde_json`, `chrono`, `thiserror`, `fabro-http`, `fabro-redact`, `fabro-types` (workspace versions). Auto-included via `lib/components/*` glob. |
| `lib/components/fabro-forgejo/src/lib.rs` | Mirror of the needed `fabro-github` surface: `FORGEJO_API_PREFIX = "/api/v1"`; `ForgejoCredentials { token: SecretString }` (reuse `fabro-github::token_source::SecretString`? No — that would couple the crates; define a local newtype); `ForgejoContext<'a> { creds, base_url, http_client }`; `HttpClient`/`HttpResponse`/`HttpMethod` mirror trait + impl for `fabro_http::HttpClient` (accepted duplication per architecture answer); `forgejo_headers(token)` using `Authorization: token <t>`; `parse_owner_repo(base_url, url) -> (owner, repo)` accepting `https://host[:port]/owner/repo[.git]` and `git@host:owner/repo.git` spellings; `normalize_origin_url`; `branch_head_sha`; `find_open_pull_request`; `create_pull_request`; `get_pull_request -> PullRequestDetails` (field mapping: `has_merged`→`merged`, `mergeable`, `base.ref`/`head.ref`, timestamps); `merge_pull_request` (`POST …/pulls/{n}/merge`, body `Do: "merge"` + `merge_strategy` mapping); `close_pull_request` (`PATCH …/pulls/{n}` state closed); `ForgejoCloneCredentials { token }` + `authenticated_clone_url(base, owner, repo, token) -> DisplaySafeUrl` (basic auth, fixed username `git`, password = PAT). Auto-merge: only if the instance/twin advertises support — otherwise return a typed `Unsupported` the caller warns on. |
| `lib/components/fabro-forgejo/src/tests_mock.rs` | Mock `HttpClient` recording requests/returning scripted JSON (mirror of `fabro-github/src/tests_mock.rs`), `#[cfg(test)]`. |
| `test/twin/forgejo/Cargo.toml` + `src/{lib.rs,server.rs,state.rs,fixtures.rs,handlers/{mod.rs,repos.rs,pulls.rs,branches.rs},test_support.rs}` | Twin Forgejo instance (auto-included via `test/twin/*` glob): token-auth check (`Authorization: token`), `GET /api/v1/repos/{o}/{r}`, `GET …/branches/{b}`, `GET/POST …/pulls`, `GET/POST …/pulls/{n}/merge`, `PATCH …/pulls/{n}`, scripted PR payloads in Gitea shape. Used by `fabro-server` and `fabro-workflow` tests the way `test/twin/github` is. |
| `docs/public/integrations/forgejo.mdx` | Setup doc: settings block, `fabro secret set FORGEJO_TOKEN`, capability matrix vs GitHub (no browser login, no webhooks, no install wizard, auto-merge caveat), strategy note that runs are declared via `[run.scm]`. |
| `docs/public/changelog/2026-09-10.mdx` | Changelog entry (new file; no entry exists for today). |

## Files modified, by layer, in implementation order

### Phase 1 — Foundation types, settings, secrets

1. **`lib/foundation/fabro-static/src/env_vars.rs`** — add `FORGEJO_TOKEN` const next to `GITHUB_TOKEN`; add to the secret-name list (the block at lines ~225–229).
2. **`lib/foundation/fabro-static/src/secret_registry.rs`** — register `EnvVars::FORGEJO_TOKEN` in both the registry and redaction lists (lines ~28–31, ~87–90).
3. **`lib/foundation/fabro-types/src/settings/server.rs`** — `ServerIntegrationsSettings` gains `pub forgejo: ForgejoIntegrationSettings`; new struct `{ enabled: bool, url: String }` with `Default { enabled: false, url: String::new() }` (mirrors `SlackIntegrationSettings` shape).
4. **`lib/foundation/fabro-types/src/settings/run.rs`** — doc-only clarification on `RunScmSettings.provider` that `"forgejo"` is accepted; no struct change (provider stays `Option<String>`; `ScmGitHubSettings` stays as-is for github-declared repos).
5. **`lib/foundation/fabro-types/src/run_intent.rs`** — `GitRunTarget` gains `#[serde(default, skip_serializing_if = "Option::is_none")] pub forge: Option<String>`; `validate()` keeps grammar checks unchanged and, when `forge == Some("forgejo")`, skips the github-derived `GitContext` (returns slug-only projection; `origin_url` derivation moves to the admission step) — `github`/`None` behavior byte-identical. Unknown forge strings → `GitCoordinateValidationError`-style error (new variant `Forge`).
6. **`lib/foundation/fabro-types/src/pull_request.rs`** — `PullRequestLink` gains `forge_base_url: Option<String>`; `html_url()` branches (`/pull/` vs `/pulls/`); custom `Serialize`/`Deserialize` updated to round-trip and cross-check the field against `html_url`; new pure helper `pull_request_link_from_url(url, expected_forge_base: Option<&str>)` — github.com rules when the host isn't the expected base, `{base}/{owner}/{repo}/pulls/{n}` when it is. Existing tests keep passing; add forge-shape tests. `PartialEq`/`Eq` derive covers the new field.
7. **`lib/foundation/fabro-types/src/repository.rs`** — `RepositoryProvider` gains `Forgejo` variant; `repository_provider()` sniffer unchanged (URL-based detection stays github-only); forgejo classification happens at the server call sites that know the configured base.
8. **`lib/foundation/fabro-types/src/system_integrations.rs`** — `IntegrationProvider` gains `Forgejo`.
9. **`lib/foundation/fabro-types/src/lib.rs`** — re-export any new public items (`ForgejoIntegrationSettings`, error variant).

### Phase 2 — `fabro-forgejo` crate + twin (files above)

### Phase 3 — API contract

10. **`docs/public/api-reference/fabro-api.yaml`** — additive only: `GitRunTarget.forge` (nullable string, enum `[forgejo]`), `PullRequestLink.forge_base_url` (nullable string, only when present; `html_url` description updated to mention instance URLs), `RepositoryProvider` enum add `forgejo`, integration provider enum add `forgejo`. No new paths.
11. **Regenerate**: `cargo build -p fabro-api` (build.rs/progenitor picks up `with_replacement` types automatically since they are unified), then `cd lib/packages/fabro-api-client && bun run generate`.
12. **`lib/foundation/fabro-api/tests/pull_request_round_trip.rs`** (and `run_summary_round_trip.rs` if it asserts the target shape) — extend round-trip tests with a forge-shaped link/target proving spec/domain parity (CLAUDE.md requires a parity test for touched unified types).

### Phase 4 — Server

13. **`lib/apps/fabro-server/src/server.rs`** — `AppState`/builder: add `forgejo_credentials(&self, settings) -> anyhow::Result<Option<ForgejoCredentials>>` reading vault `FORGEJO_TOKEN` (mirrors `github_credentials`, lines ~1575–1615); thread nothing else — resolution stays lazy.
14. **`lib/apps/fabro-server/src/run_compiler.rs`** — admission: when `RunTarget::Git` has `forge == Some("forgejo")`, require `server.integrations.forgejo.enabled && !url.is_empty()`, else a typed admission error naming the missing setting; derive `GitContext.origin_url = normalize({url}/{owner}/{repo}.git)`; build `RepositoryRef` with `RepositoryProvider::Forgejo`. GitHub path untouched.
15. **`lib/apps/fabro-server/src/run_manifest.rs`** — preflight (`~lines 440–520`): add `needs_forgejo_credentials = run target forge is forgejo`; resolve PAT; remote-ref check (`~lines 800–830`): accept forgejo origins by parsing against the configured base and build the authenticated URL from the PAT instead of `fabro_github::resolve_authenticated_url`; keep the GitHub branch verbatim.
16. **`lib/apps/fabro-server/src/git_checkout.rs`** (Local provider checkout) — `checkout` gains a forgejo branch: clone URL from the configured base + PAT basic auth; `github_clone_url` untouched.
17. **`lib/apps/fabro-server/src/server/handler/system.rs`** — `forgejo_integration_status(settings, vault)` mirroring `github_integration_status` (line 122 area): `enabled` from settings, `configured` = url non-empty, `missing_credentials` lists `FORGEJO_TOKEN`; append to the response `data` vec.
18. **`lib/apps/fabro-server/src/server/handler/pull_requests.rs`** — link endpoint: parse submitted URLs with the settings-aware parser (accepts github.com or the configured instance host); details fetch: when `link.forge_base_url.is_some()`, build `ForgejoContext` from vault token + settings URL and call `forgejo_get_pull_request`, mapping to `PullRequestDetails`; `IntegrationUnavailable`/`NotFound` semantics preserved.
19. **`lib/apps/fabro-server/src/server/pull_request_supervisor.rs`** — `server_forgejo_context(state)` helper next to `server_github_context`; the supervisor's explicit PR-creation path branches on the run's origin/link forge. (Note: supervisor currently uses GitHub credentials — a forgejo run's PR creation primarily flows through the worker's publish pipeline; the supervisor branch covers server-side re-creation.)
20. **`lib/apps/fabro-server/src/spawn_env.rs`** — **no change**: the worker reads `FORGEJO_TOKEN` from the vault via `FABRO_HOME`/storage paths already allowlisted (matches server-secrets-strategy: optional integration secrets are vault-resolved at use). Stated here so the reviewer sees it was considered.

### Phase 5 — Sandbox

21. **`lib/components/fabro-sandbox/src/clone_source.rs`** — generalize `github_repo_layout` → `repo_layout(origin_url, forgejo_base: Option<&str>)`: parse owner/repo against github.com **or** the configured base (normalized, `.git` stripped, SSH spellings of either host); rename `CloneDecision::GitHub` → `CloneDecision::Git` (mechanical rename of variant + call sites; `GitHubRepoLayout` → `RepoLayout`). Error message becomes "Clone-based sandboxes support GitHub and the configured Forgejo instance only: …".
22. **`lib/components/fabro-sandbox/src/sandbox_spec.rs`** — spec builders gain `forgejo: Option<ForgejoRemote>` param (`{ base_url, token }` from worker-resolved vault PAT); pass `forgejo_base` into layout parsing; use `authenticated_clone_url` for the clone URL when the origin matches the base; `github_app` path untouched.
23. **`lib/components/fabro-sandbox/src/push_credentials.rs`** — `build_token_source` returns `Ok(None)` for forgejo origins (static PAT embedded at clone; no refresh). One comment explaining why refresh is a GitHub-app-token concern only.
24. **`lib/components/fabro-sandbox/src/docker.rs`** — clone/fetch/checkout command construction uses the forgejo-authenticated URL; in-sandbox credential helper: add `FORGEJO_CREDENTIAL_HELPER` config keyed `credential.https://<host>.helper` reading `$FORGEJO_TOKEN` when the run is forgejo (mirrors the github probe env pattern from `fabro-github/src/lib.rs:42-51`, defined locally in fabro-sandbox or fabro-forgejo and imported).
25. **`lib/components/fabro-sandbox/src/daytona/mod.rs`** — pass the authenticated forgejo URL to the SDK clone (same basic-auth URL form the GitHub path uses); exact-commit fetch path unchanged apart from URL.
26. **`lib/components/fabro-sandbox/src/error.rs`** — message at line ~143 generalized to name either credential source.

### Phase 6 — Workflow engine

27. **`lib/components/fabro-workflow/src/pipeline/initialize.rs`** — accept `forgejo: Option<&ForgejoCredentials>` (worker-resolved) plus the settings snapshot's `server.integrations.forgejo`; no access-validation (single PAT, no repository-set semantics).
28. **`lib/components/fabro-workflow/src/services.rs`** — `resolve_workflow_env` gains an optional static `forgejo_token` insert of `FORGEJO_TOKEN` (parallel to the `GITHUB_TOKEN` block at lines ~356–368); `WorkflowToolEnvProvider`/`SandboxToolEnvProvider` carry the extra field.
29. **`lib/components/fabro-workflow/src/git_bridge.rs`** — add `FORGEJO_CREDENTIAL_HELPER` + host-parameterized config key `credential.https://<instance-host>.helper`; bridge activation takes the run's forgejo descriptor and applies the helper + SSH→HTTPS rewrite for that host only; github behavior unchanged (all existing tests at lines ~200–420 must pass verbatim).
30. **`lib/components/fabro-workflow/src/pipeline/publish.rs`** + **`pipeline/pull_request.rs`** — `PullRequestRequest`/options carry a client enum (`Forge::Github(GitHubContext) | Forge::Forgejo(ForgejoContext)`) selected by matching the run origin against the configured forgejo base; forgejo branch calls the fabro-forgejo ops; auto-merge `Unsupported` → `warn!` + continue (per logging-strategy: structured fields, no secrets); the existing `origin_url: Some("https://github.com/...")` tests stay green, add forge-shaped twins using `test/twin/forgejo` + httpmock where the existing harness does.
31. **`lib/components/fabro-workflow/src/pipeline/types.rs`** — `SandboxEnvSpec` doc update (`origin_url` may be a Forgejo instance URL); no shape change.
32. **`lib/components/fabro-workflow/src/run_metadata.rs` / `lifecycle/git.rs`** — audit only: these consume `GitContext.origin_url` opaquely; expected change is none or comment-only. Listed so the auditor sees they were checked.

### Phase 7 — Manifest & CLI

33. **`lib/components/fabro-manifest/src/lib.rs`** — `configured_repo_origin_url` (lines ~428–451): when `provider == "forgejo"` and owner/repository set, return a sentinel descriptor (not a URL) so `observe_git_run_target` emits `GitRunTarget { repo: slug, forge: Some("forgejo") }`; ambient non-github remotes keep failing unless declared. `github_run_target` unchanged; new `declared_forge_run_target`.
34. **`lib/apps/fabro-cli/src/commands/pr/link.rs`** — `record_label` prints `forgejo #N` vs `github #N` based on `link.forge_base_url`.
35. **`lib/apps/fabro-cli/src/shared/github.rs`** — unchanged (GitHub credential building); noted to preempt "why not here".

### Phase 8 — Web, docs

36. **`apps/fabro-web/app/routes/settings-integrations.tsx`** — add `ForgejoPanel` rendering the `forgejo` provider status (mirror of `GithubPanel`, no actions — config is file/vault-driven).
37. **`docs/public/docs.json`** — nav: `"integrations/forgejo"` after `"integrations/github"` (line ~95).
38. Docs/changelog files from the "created" table.

### Phase 9 — Cargo wiring

39. **`lib/apps/fabro-server/Cargo.toml`, `lib/components/fabro-workflow/Cargo.toml`, `lib/components/fabro-sandbox/Cargo.toml`, `lib/components/fabro-manifest/Cargo.toml`** — add `fabro-forgejo` dependency. **`lib/apps/fabro-server/Cargo.toml` (dev-dependencies)** and **`lib/components/fabro-workflow/Cargo.toml` (dev-dependencies)** — add `test/twin/forgejo`. Workspace `Cargo.toml` needs no edit (globs).

## Verification

Per phase, narrowest first, using repo tooling only:

- **Phase 1**: `cargo nextest run -p fabro-types -p fabro-static` — new serde round-trip tests (forge link/target old-data compat: github-shaped JSON with no forge fields must deserialize identically; deny_unknown_fields still rejects junk).
- **Phase 2**: `cargo nextest run -p fabro-forgejo` — mock-client tests for header scheme (`Authorization: token`), URL parsing matrix (https/http/custom-port/SSH/`.git`), payload mapping (`has_merged`→`merged`), merge/close bodies, NotFound mapping to `PullRequestApiError::NotFound`.
- **Phase 3**: `cargo build -p fabro-api` regen compiles; `cargo nextest run -p fabro-api` round-trip/parity tests; `cd lib/packages/fabro-api-client && bun run generate` then `cd apps/fabro-web && bun run typecheck`.
- **Phase 4–6**: `cargo nextest run -p fabro-server -p fabro-sandbox -p fabro-workflow -p fabro-manifest` — including the existing OpenAPI/router conformance test in fabro-server (catches spec drift), extended clone_source/docker command-construction tests asserting the forgejo clone URL + credential-helper config, PR-pipeline tests against the twin.
- **Phase 7**: `cargo nextest run -p fabro-cli --test it` (pr link tests).
- **Final sweep**: `cargo build --workspace`; `cargo nextest run --workspace`; `cargo +nightly-2026-04-14 fmt --all` then `--check`; `cargo +nightly-2026-04-14 clippy --workspace --all-targets -- -D warnings`; `cd apps/fabro-web && bun test && bun run typecheck`. No live/E2E profile runs — no credentials, per standing decision.
- **Manual check (no tooling covers it)**: end-to-end clone/PR against a real Forgejo instance is **not** verifiable in this environment; the twin is the substitute. I will state this limitation in the changelog/doc ("tested against the twin").

## Deliberately not doing

- **No OAuth/browser login, webhooks, install-wizard UI, tracker** — the human chose PAT-only and the run-target tier; webhooks were tier-C and tracker has no Forgejo analogue (GitHub's is GraphQL).
- **No multi-instance/Codeberg support** — single configured instance; other hosts are rejected at admission with an error naming the configured URL.
- **No `fabro-github` refactor** — parallel crate per the architecture answer; the only shared-code touch is the mechanical `CloneDecision::GitHub`→`Git` rename inside fabro-sandbox (variant semantics widen, GitHub behavior identical).
- **No migrations** (file or SQL) — all persisted data is github-shaped; additive serde fields keep it valid (migrations-strategy: no migration for additive parsing).
- **No `FORGEJO_BASE_URL` env override** — the instance URL is settings-only; twins inject via settings. GitHub's `GITHUB_BASE_URL` exists for the github twin; the forgejo twin gets its URL the same way tests construct it, without a production env path (server-secrets-strategy discourages bespoke env fallbacks).
- **No auto-merge guarantee** — attempted only if the API/tin supports it; otherwise a warning, documented.
- **No breaking wire changes** — additive optional fields only; old clients/servers interoperate.
- **No Local-provider ambient forgejo detection** — Forgejo runs are declared via `[run.scm]`; ambient remote detection stays github-only (the CLI cannot know the instance host).