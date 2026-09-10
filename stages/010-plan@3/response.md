# Implementation Plan (rev 3) — Forgejo integration (tier b, single instance + provider tag)

**Changes from the critiqued revision, and why:**
1. **Token removed from the sandbox spec (critique defect #1).** Verified against the code: Docker's provider receives `github_app: Option<&GitHubCredentials>` as a *runtime parameter* at init (docker.rs:187/219) and embeds the token only into the clone URL at clone time (docker.rs:935, `embed_token_in_url`); the spec/options carry only `clone_origin_url`, and even that is scrubbed for records via `clone_source::clean_clone_origin_for_record` (sandbox_spec.rs:112). Since `RunSpec` is embedded in persisted `RunProjection`s and `RunSandbox` round-trips through serde, a spec-level `token` field would write the PAT into persisted run storage — contradicting the server-secrets strategy. Rev 2's items 25/26 contradicted each other; now the token flows through the runtime credential seam only, mirroring GitHub exactly, and `clean_clone_origin_for_record` is extended to strip embedded forgejo tokens from recorded origins.
2. **`lib/apps/fabro-cli/src/shared/repo.rs` added to the audit (critique defect #2).** `ensure_matching_repo_origin` normalizes the local origin with the github.com-only normalizer; a Forgejo SSH remote would fail the comparison and wrongly refuse fork/resume. Fixed via the instance-aware normalizer; unit test named below.

All prior critique points remain addressed: `[run.scm] provider = "forgejo"` handling (item 37), automated tier-(b) wiring tests + required Docker gate, and the completed audit lists.

**Authoritative decisions:** tier (b) = CLI + server-side (run targets, automations, sandbox clone, repo picker, integration status, doctor; no browser login, no webhooks). Addressing = one configured instance (`[server.integrations.forgejo] url`) + `provider` tag defaulting `github`; GitHub coexists.

**Forgejo API facts the code must respect** (verified against Forgejo docs): REST under `{url}/api/v1`, `Authorization: token <PAT>`, PR URLs use `/pulls/` (plural), PR close via `PATCH /repos/{o}/{r}/issues/{n}`, PR create has no draft flag (WIP title prefix), no GitHub-App/installation-token equivalent.

---

## Files and changes

### A. Foundation types, settings, env

1. `lib/foundation/fabro-types/src/repository.rs` — `pub enum ScmProvider { Github, Forgejo }` (strum `Display/EnumString/IntoStaticStr`, serde lowercase, `#[default] Github`); `Forgejo` added to display-only `RepositoryProvider` via `repository_provider_with(origin, forgejeo_host: Option<&str>)`; existing fn wraps with `None`.
2. `lib/foundation/fabro-types/src/run_intent.rs` — `GitRunTarget` gains `#[serde(default, skip_serializing_if = "is_default")] pub provider: ScmProvider`; validation core becomes `validate_with_scm(forgejo_url: Option<&str>)` (`validate()` = `None` wrapper); forgejo origin = `{forgejo_url}/{owner}/{repo}`; missing URL → `GitCoordinateValidationError::ForgejoUnconfigured`. Slug grammar reuses `GitHubRepositorySlug` (charset-compatible; rename = churn).
3. `lib/foundation/fabro-types/src/pull_request.rs` — `PullRequestLink` gains `provider: ScmProvider` + `origin: Option<String>` (required iff forgejo); `html_url()`: github `.../pull/N`, forgejo `{origin}/.../pulls/N`; conditional serialize (wire-additive); deserialize validates the forgejo `/pulls/` shape from the URL itself (round-trips without settings); `from_forgejo_url`.
4. `lib/foundation/fabro-types/src/settings/server.rs` — `ServerIntegrationsSettings` gains `#[serde(default)] pub forgejo: ForgejoIntegrationSettings`; `ForgejoIntegrationSettings { enabled: bool, url: Option<String> }`, `Default` disabled.
5. `lib/foundation/fabro-types/src/system_integrations.rs` — `IntegrationProvider::Forgejo`.
6. `lib/foundation/fabro-types/src/lib.rs` — re-export `ScmProvider`, `ForgejoIntegrationSettings`.
7. `lib/foundation/fabro-static/src/env_vars.rs` — `EnvVars::FORGEJO_TOKEN` (+ sorted list).
8. `lib/foundation/fabro-static/src/secret_registry.rs` — register `FORGEJO_TOKEN` alongside `GITHUB_TOKEN`.
9. `lib/foundation/fabro-config/src/layers/server.rs` — `ServerIntegrationsLayer.forgejo: Option<ForgejoIntegrationSettings>`.
10. `lib/foundation/fabro-config/src/resolve/server.rs` — resolve `forgejo`; error when `enabled` + missing `url`; normalize URL (trim `/`, require `https` except localhost).

### B. New crate `lib/components/fabro-forgejo`

11. `lib/components/fabro-forgejo/Cargo.toml` (+ root `Cargo.toml` members) — deps `fabro-http`, `fabro-redact`, `fabro-static`, `fabro-types`, `anyhow`, `serde`, `serde_json`, `thiserror`, `tokio`; feature `test-support`.
12. `lib/components/fabro-forgejo/src/lib.rs` — `ForgejoContext { token, base_url }` (token never `Debug`-printed; redacted display only); local `HttpClient` trait mirroring `fabro_github`'s + impl for `fabro_http::HttpClient`; `parse_owner_repo(instance_url, repo_url)`, `repo_https_url`, `normalize_origin_url` (instance-host-aware, converts the instance's SSH spellings → HTTPS), `embed_token_in_url` (username `fabro`, password = token, `DisplaySafeUrl` redaction); ops: `find_open_pull_request` (list open, match base/head client-side), `create_pull_request` (draft → `WIP: ` prefix), `get_pull_request`, `merge_pull_request` (POST `/pulls/{n}/merge`, `Do` from `MergeStrategy`), `close_pull_request` (PATCH `/issues/{n}` `{"state":"closed"}`), `branch_head_sha` (GET `/branches/{b}` → `commit.id`), `get_repo` (default_branch/private/permissions); `NotFound | Other` error shape.
13. `lib/components/fabro-forgejo/src/test_support.rs` + `src/tests_mock.rs` — mock client mirroring `fabro-github`'s `MockHttpClient`.
14. `lib/components/fabro-forgejo/tests/integration.rs` — every op: success, 404/401/403, malformed JSON, URL-parse table (https/ssh/`.git`/credentials), token-embed redaction, `normalize_origin_url` SSH conversion table.
15. `lib/components/fabro-forgejo/tests/live_access.rs` — `#[e2e_test(live("FABRO_FORGEJO_TOKEN"))]` against `FABRO_FORGEJO_URL` (ignored unless live).

### C. OpenAPI + generated clients

16. `docs/public/api-reference/fabro-api.yaml` — `GitRunTarget` + optional `provider` (`enum: [github, forgejo]`); `PullRequestLink` + optional `provider`/`origin`; `IntegrationProvider` + `forgejo`; new `GET /api/v1/repos/forgejo/{owner}/{name}` (same response shape as the github endpoint). Then `cargo build -p fabro-api` and `cd lib/packages/fabro-api-client && bun run generate`. Extend `fabro-api` type-identity/JSON-parity tests for the two modified replaced types.

### D. Workflow engine

17. `lib/components/fabro-workflow/src/pipeline/types.rs` — options struct gains `forgejo: Option<ForgejoContext>` (runtime credential, never serialized into persisted records); new `ScmTarget` enum (`GitHub { owner, repo, ctx }` | `Forgejo { owner, repo, origin, ctx }`).
18. `lib/components/fabro-workflow/src/pipeline/pull_request.rs` — `open_pull_request` resolves `ScmTarget` from origin (forgejo ⇔ host matches instance URL) instead of unconditional `parse_github_owner_repo`; `verify_remote_head`/`reconcile_existing_pull_request` dispatch on it; `build_pr_content` unchanged; `CreatedPullRequest` carries provider+origin; forgejo auto-merge → `warn!` + skip.
19. `lib/components/fabro-workflow/src/pipeline/publish.rs` + `initialize.rs` — context selection via `ScmTarget`; github path untouched for github origins.
20. `lib/components/fabro-workflow/src/run_metadata.rs` — meta-branch gate (~298–325) branches on provider; forgejo writer uses a static-token `CredentialSource` (`StaticTokenSource`; refresh = identity).
21. `lib/components/fabro-workflow/src/git_bridge.rs` — github origins unchanged; forgejo origins get `credential.https://{host}.helper` reading `$FORGEJO_TOKEN` (helper defined in `fabro-forgejo`, same secret-free `!f()` pattern).
22. `lib/components/fabro-workflow/src/services.rs` — `resolve_workflow_env` (~356): forgejo static token → `FORGEJO_TOKEN` env when the run's origin is the instance; options structs (~241/~344) gain the field.
23. **Audit (complete `fabro_github` call-site list):** `run_options.rs`, `operations/start.rs`, `operations/fork.rs`, `operations/retry.rs`, `handler/command.rs`, `handler/llm/acp.rs`, `git.rs` (display sanitization — make normalization provider-aware), `sandbox_git_runtime.rs`. Each site branches on provider or is provably unreachable for forgejo runs.

### E. Sandbox clone (Docker/Daytona) — **token stays in the runtime layer**

24. `lib/components/fabro-sandbox/src/clone_source.rs` — `CloneDecision::Forgejo { origin_url, branch, tag, commit_sha }`; extract `repo_layout(owner, repo, …)` shared with `github_repo_layout`; classification by instance host; **extend `clean_clone_origin_for_record` to strip embedded forgejo tokens** (recorded origins stay credential-free, matching the github behavior at sandbox_spec.rs:112); "GitHub origins only" error names both providers.
25. `lib/components/fabro-sandbox/src/sandbox_spec.rs` + `provider.rs` + `docker.rs` + `daytona/mod.rs` — **(critique fix #1)** the spec/options carry only `clone_origin_url` (forgejo URL, no token). The providers' init functions gain a `forgejo: Option<&ForgejoContext>` runtime parameter exactly parallel to `github_app: Option<&GitHubCredentials>` (docker.rs:187/219); clone embeds the token at clone time via `fabro_forgejo::embed_token_in_url` (parallel to docker.rs:935); exact-commit path unchanged. No `token` field on any serde model.
26. `lib/components/fabro-sandbox/src/push_credentials.rs` — static-PAT remote-credential action for forgejo origins (initial embed + `set-url` before push, same generation-tracking sequence; refresh = no-op success); builders take the runtime `ForgejoContext`, not spec data.

### F. Server

27. `lib/apps/fabro-server/src/server.rs` — `AppState::forgejo_credentials()` (vault `FORGEJO_TOKEN` → `Option<ForgejoContext>`; `None` when disabled); route `GET /api/v1/repos/forgejo/{owner}/{name}`; startup snapshot loads forgejo settings (~2602 area); the runtime `ForgejoContext` is passed into sandbox init and run compilation through the same seams as `github_credentials` today.
28. `lib/apps/fabro-server/src/server/handler/system.rs` — integrations handler appends the forgejo row (disabled / missing `server.integrations.forgejo.url` + `FORGEJO_TOKEN` / configured); `get_forgejo_repo` mirrors `get_github_repo` shape (`install_url` = `{url}/{owner}/{repo}/settings`).
29. `lib/apps/fabro-server/src/server/handler/runs.rs` — ~629: `validate_with_scm(forgejo_url from state)`; forgejo targets rejected 422 (remediation naming settings path + vault key) when integration disabled or token missing.
30. `lib/apps/fabro-server/src/run_manifest.rs` — preflight (~440–520): forgejo-targeted runs get repository-access via `fabro_forgejo::get_repo`, a `FORGEJO_TOKEN` check mirroring `run_github_token_check`, PAT clone-auth; repo-summary parse (~602–607) provider-aware; `check_remote_ref` forgejo branch via `branch_head_sha`; the "GitHub origins only" gate (~807–814) admits forgejo origins when configured.
31. `lib/apps/fabro-server/src/automation_materializer.rs` — resolver carries `forgejo: Option<ForgejoContext>` + `forgejo_url`; forgejo remotes resolve `clone_url = embed_token(repo_https_url)` + static-token `GitAuthConfig`; validation (~235) passes the forgejo URL.
32. `lib/apps/fabro-server/src/git_checkout.rs` — `forgejo_clone_url(instance_url, owner, repo)`; `forgejo_git_auth(token)` (basic-auth `http.extraheader`, existing base64 helper); `resolve_git_read_auth_config` gains a forgejo arm.
33. `lib/apps/fabro-server/src/spawn_env.rs` — `FORGEJO_TOKEN` passed to workers only for forgejo-targeted runs.
34. `lib/apps/fabro-server/src/server/handler/pull_requests.rs` — live-detail/merge/close dispatch on link provider (forgejo → new crate against `link.origin`); payload conversion into `PullRequestDetails`.
35. `lib/apps/fabro-server/src/diagnostics.rs` — when forgejo enabled: PAT probe of `{url}/api/v1/user` in the diagnostics report (mirrors the github token probe at ~351–399); absent when disabled.

### G. Manifest & CLI

36. `lib/components/fabro-manifest/src/lib.rs` —
    - `configured_repo_origin_url` (~434): accept `provider = "forgejo"`; return `{forgejo_url}/{owner}/{repo}` signaling forgejo so downstream targets carry `provider: Forgejo`; **error (not silent `None`)** when forgejo is requested but no instance URL is available (signature → `Result<Option<ConfiguredOrigin>, String>`; callers ~217/332/346/369/405/510 propagate).
    - Manifest-build entry point gains the forgejo instance URL parameter; CLI and server callers pass it from resolved server settings when enabled.
    - Origin observation `github_run_target` (~417): generalize to match the configured instance host, producing forgejo-provider targets.
37. `lib/apps/fabro-cli/src/shared/forgejo.rs` (new) — `build_forgejo_credentials(server_ns, vault)` (env→vault `FORGEJO_TOKEN`, matching `shared/github.rs` order).
38. `lib/apps/fabro-cli/src/shared/repo.rs` — **(critique fix #2)** `ensure_matching_repo_origin` normalizes the current origin with the instance-aware `fabro_forgejo::normalize_origin_url` when the expected origin is the forgejo instance (falls back to today's behavior otherwise), so SSH checkouts of the forgejo repo compare equal instead of failing "wrong checkout".
39. `lib/apps/fabro-cli/src/commands/run/runner.rs` — `maybe_build_github_credentials` (~1102) / `requires_github_credentials` (~1139): forgejo-targeted runs build the forgejo ctx and skip the github requirement (truth-table tests at ~1739 extended); pass the forgejo URL to the manifest builder per item 36.
40. `lib/apps/fabro-cli/src/commands/doctor.rs` — offline forgejo check: not configured → pass; `enabled` + missing `url` → error; `enabled` + url + missing `FORGEJO_TOKEN` → error with remediation. No network.
41. `lib/apps/fabro-cli/src/commands/repo/init.rs` — ~196–197: origin detection branches on the configured instance host → forgejo slug parse + `[run.scm] provider = "forgejo"` in the scaffold.

### H. Web

42. `apps/fabro-web/app/components/automation-form.tsx` (+ test) — provider toggle (shown only when the integrations status endpoint reports forgejo configured) calling the regenerated client's forgejo lookup; default github.
43. `apps/fabro-web/app/components/pull-request-chip.tsx` — verify it renders `html_url` (expected no change; adjust only if it re-parses github URLs).

### I. Docs

44. `docs/public/integrations/forgejo.mdx` (new) — PAT scopes (`write:repository`, `write:issue`), settings, `provider = "forgejo"` on run targets/`[run.scm]`, capability table vs GitHub, limitations (no login/webhooks/auto-merge, WIP-draft, static token blast radius, single instance, system CA only).
45. `docs/public/docs.json` — `integrations/forgejo` after `integrations/github`.
46. `docs/public/changelog/2026-09-10.mdx` — entry.

---

## Order of work

1. **A** → `cargo nextest run -p fabro-types -p fabro-static -p fabro-config`
2. **B** → `cargo nextest run -p fabro-forgejo`
3. **C** (spec → regen → TS client) — before server handlers, per repo API workflow
4. **D** → `cargo nextest run -p fabro-workflow`
5. **E** → `cargo nextest run -p fabro-sandbox`
6. **F** → `cargo nextest run -p fabro-server` (conformance included)
7. **G** → `cargo nextest run -p fabro-cli -p fabro-manifest`
8. **H** → `cd apps/fabro-web && bun test && bun run typecheck`
9. **I** (docs)
10. Full-gate pass after each of steps 4–7 and once at the end.

## Verification

**Automated per stage (as ordered), plus:**
- After each stage: `cargo +nightly-2026-04-14 fmt --check --all`, `cargo +nightly-2026-04-14 clippy --workspace --all-targets -- -D warnings`.
- `cargo build --workspace` (no `test-support` leakage).
- **Backward-compat unit tests (stage A):** existing GitHub JSON fixtures for `GitRunTarget`/`PullRequestLink` deserialize unchanged; `validate()` with no forgejo URL behaves exactly as today for github targets.
- **Secrets-placement test (stage E, guards critique fix #1):** serialize a forgejo-run sandbox/run spec and assert no token appears in the JSON; `clean_clone_origin_for_record` strips an embedded forgejo token from the recorded origin.
- **Tier-(b) wiring tests:**
  - `fabro-server` tests (httpmock/mock pattern): forgejo-targeted RunIntent validates with the URL, fails without; preflight repository-access succeeds against mocked `{url}/api/v1/repos/{o}/{r}`; PAT clone-auth selected (github credential path untouched).
  - `automation_materializer.rs` tests (existing mock-resolver pattern ~546–584): forgejo `GitRunTarget` resolves a token-embedded clone URL; github targets unchanged.
  - `fabro-manifest` test: `[run.scm] provider = "forgejo"` → forgejo-origin git context with URL; explicit error without (guards silent-`None`).
  - `shared/repo.rs` test: SSH forgejo origin compares equal to the expected HTTPS origin (guards critique fix #2).
  - `repo/init.rs` test: scaffold in a forgejo clone writes `provider = "forgejo"`.
- **Required manual gate at end of stage F:** `docker run -d -p 3001:3000 codeberg.org/forgejo/forgejo`, PAT, `[server.integrations.forgejo] url = "http://localhost:3001"`, clone a repo; server-side run → preflight passes, run + meta branch push, auto-PR with WIP prefix; then CLI `fabro run` in the same clone (local path), and fork/resume from that clone (exercises item 38).
- Optional live e2e: `set -a && source .env && set +a && cargo nextest run -p fabro-forgejo --profile e2e --run-ignored only` with `FABRO_FORGEJO_URL`/`FABRO_FORGEJO_TOKEN`.

## Deliberately not doing

- Browser login (`AuthMethod::Forgejo`) — human chose tier (b); instance-wide Forgejo OAuth2 apps are admin-scoped.
- Webhooks — today's GitHub handler only verifies HMAC and logs; nothing to mirror.
- Projects tracker — no Forgejo equivalent of Projects V2 GraphQL.
- Auto-merge on Forgejo — upstream semantics differ; warn-and-skip.
- Gitea branding/support — forgejo naming only.
- Multiple instances / exclusive switch — human chose single instance + provider tag.
- Install wizard changes — URL+token via manual settings + doctor + docs.
- Custom CA bundles — system trust store only, documented.
- Renaming `GitHubRepositorySlug` — reused for slug grammar; churn without behavior.
- `[run.scm.forgejo]` settings leaf — provider + server-level URL covers it.
- Token anywhere in serialized specs/records — runtime credential layer only (critique fix #1).
- Migrations / new event variants — all changes additive.