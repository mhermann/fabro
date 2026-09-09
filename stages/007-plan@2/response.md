# Implementation Plan (rev 2): Forgejo integration (core run path)

**Changes from the critique and why:**
1. **Event-host threading added** (was missing): the critique proved `Event::PullRequestCreated` → `PullRequestCreatedProps` → `run_state.rs` projection drops any host, so server-side merge/close/get would mis-dispatch forgejo PRs to `api.github.com`. The plan now adds an optional `host` property end-to-end. Verified while revising: the `Event::pull_request_created(record: &PullRequestLink, …)` constructor (`fabro-workflow/src/event/events.rs:786`) already receives the full link, and `PullRequestCreatedProps.head_sha` (`fabro-types/src/run_event/misc.rs:372`) is the exact serde precedent (`Option` + `default` + `skip_serializing_if`) to copy. `docs/internal/events-strategy.md` read as required; the addition follows its "optional properties live inside `properties`" rule.
2. **URL grammar fixed**: host-carrying `PullRequestLink::html_url()` now uses Forgejo's `/pulls/{n}` route (was wrongly `/pull/{n}`), and the forgejo PR path prefers the API-returned `html_url` instead of synthesizing one.
3. Minor points folded in: serde round-trip validates against the record's *own* `host` (config-free); pagination comment on `find_open_pull_request`; `SandboxSpec::Docker` construction site named (`operations/start.rs:535`); explicit note that `spawn_env.rs` needs **no** change (verified: worker tokens flow via the shared vault under `FABRO_HOME`, settings via `FABRO_CONFIG` at `worker_runtime.rs:101`).

Decisions (human-confirmed): core run path only; PAT-only auth; sibling crate `fabro-forgejo`; single instance via `[server.integrations.forgejo]`; no web wizard. Forgejo facts used: API at `<instance>/api/v1`, `Authorization: token <PAT>`, PR create `POST /repos/{owner}/{repo}/pulls`, merge `POST …/pulls/{index}/merge` `{"Do": …}`, close `PATCH …/pulls/{index}`, branch head `GET …/branches/{branch}`, token check `GET /user`; web URLs are `https://host/owner/repo/pulls/N`.

## Files and changes

### Phase 1 — new crate `lib/components/fabro-forgejo/` (created)

| File | Change |
|---|
| `Cargo.toml` | Mirror `fabro-github` minus app-JWT deps: `anyhow, serde, serde_json, strum, thiserror, chrono, tracing, fabro-http, fabro-redact, fabro-types`; `test-support` feature; no `jsonwebtoken`. |
| `src/lib.rs` | `ForgejoCredentials::Pat(String)` (secret handling mirrors `token_source::SecretString`); `ForgejoContext<'a> { creds, base_url, http_client }`; `instance_api_base(url)` (normalize, append `/api/v1`); local `ssh_url_to_https`/normalize helpers; `origin_matches_instance(instance_url, origin_url) -> bool` (https+ssh spellings, credentials stripped, host case-insensitive, port preserved); `parse_forgejo_owner_repo(instance_url, origin_url)` (path exactly `{owner}/{repo}`); `embed_token_in_url(url, token)` (username `git`, `DisplaySafeUrl` redaction, with fabro-github's redaction tests copied); `PullRequestApiError { NotFound, Other }`; local `HttpClient`/`HttpResponse`/`HttpMethod` mirror (deliberately not a dep on fabro-github). API fns: `get_current_user`, `branch_head_sha`, `find_open_pull_request` (`?state=open` list + client-side `head.ref` filter; doc-comment the un-paginated first-page assumption), `create_pull_request`, `get_pull_request`, `merge_pull_request` (`MergeStrategy::{Merge,Squash,Rebase}` → `"merge"/"squash"/"rebase"`), `close_pull_request`. Status mapping mirrors github (404 NotFound, 401/403 auth, 405/409 not-mergeable/conflict). `ForgejoPullRequest` deserialize struct (reads API `html_url` — `/pulls/N`) → `PullRequestDetails`. |
| `src/test_support.rs` | `MockHttpClient` mirror of `fabro-github::tests_mock`. |
| inline `#[cfg(test)] mod tests` | URL parse/embed/redaction; `origin_matches_instance` (host/port/ssh/wrong-host); each API fn (success/404/401/409); merge mapping; PR→details conversion. |

### Phase 2 — static, types, config, PR link

| File | Change |
|---|
| `lib/foundation/fabro-static/src/env_vars.rs` | `FORGEJO_TOKEN` const + the same aggregate lists `GITHUB_TOKEN` is in. |
| `lib/foundation/fabro-static/src/secret_registry.rs` | `FORGEJO_TOKEN` in `OPTIONAL_VAULT_SECRETS` (both arrays). |
| `lib/foundation/fabro-types/src/settings/server.rs` | `ForgejoIntegrationSettings { enabled: bool, url: Option<String> }`; field on `ServerIntegrationsSettings`. |
| `lib/foundation/fabro-config/src/layers/server.rs` | `ForgejoIntegrationLayer { enabled: Option<bool>, url: Option<String> }`. |
| `lib/foundation/fabro-config/src/resolve/server.rs` | Resolve forgejo (`enabled` default `false`); validation: enabled ⇒ `url` present and `https` (error path `server.integrations.forgejo.url`). |
| `lib/foundation/fabro-config/src/tests/resolve_server.rs` | Defaults / enabled-without-url / non-https-url cases. |
| `lib/foundation/fabro-types/src/pull_request.rs` | `PullRequestLink` gains `host: Option<String>` (None = github.com). `html_url()`: no host → `https://github.com/…/pull/{n}` (unchanged); host → `https://{host}/{owner}/{repo}/pulls/{n}` (**Forgejo grammar**). Serialize emits `host` only when `Some`. `Deserialize`: `Wire` gains `host: Option<String>`; the `html_url` cross-check validates against the record's **own** `host` (github.com when absent) — config-free, no allowed-hosts param on the serde path. New `from_url_for_hosts(url, allowed_hosts)` used only by the config-aware `pr link` handler; accepts both `/pull/N` and `/pulls/N` segments. Existing `from_github_url` unchanged. |

### Phase 3 — sandbox clone (Docker)

| File | Change |
|---|
| `lib/components/fabro-sandbox/src/clone_source.rs` | Generalize `github_repo_layout` → `repo_layout` (any https host; owner/repo path-safety unchanged); add `CloneDecision::Forgejo { origin_url, branch, tag, commit_sha }`; both `parse_github_owner_repo` guard sites (lines ~31–36 and ~318–344) accept a forgejo origin that matches the configured instance. |
| `lib/components/fabro-sandbox/src/sandbox_spec.rs` | Docker variant gains `forgejo: Option<ForgejoCloneCredentials { instance_url, token }>` (struct defined here). Daytona variant unchanged. |
| `lib/components/fabro-sandbox/src/docker.rs` | `clone_github_repo`: when `forgejo` present and origin matches instance → generalized layout, clone URL embeds PAT via `fabro_forgejo::embed_token_in_url`, **no** `InstallationTokenSource` (static PAT never expires → `PushCredentialState::new(None)`, refresh no-op, pushes reuse the origin-embedded PAT); clone-failure classification treats auth failures as non-retryable (static credential). Error strings updated to "GitHub or Forgejo". |
| `lib/components/fabro-sandbox/src/push_credentials.rs` | Comment only: forgejo runs build `PushCredentialState` with no source. |
| `lib/components/fabro-sandbox/src/daytona/mod.rs` | Admission guard: forgejo origin + Daytona → error "Forgejo origins are supported on the Docker sandbox provider only". |
| `lib/components/fabro-sandbox/Cargo.toml` | `fabro-forgejo` dep (normal + test-support dev listing, mirroring the `fabro-github` dual listing). |

### Phase 4 — workflow: threading, env injection, PR creation, events

| File | Change |
|---|
| `lib/components/fabro-workflow/Cargo.toml` | `fabro-forgejo` (normal + test-support dev listing). |
| `lib/components/fabro-workflow/src/run_options.rs`, `src/pipeline/types.rs` | Owned `ForgejoIntegration { url, creds }` added where `github_app` lives (`RunSpec`/options). |
| `lib/components/fabro-workflow/src/operations/start.rs` | `StartServices` gains `forgejo: Option<ForgejoIntegration>`; copied into `RunSession`; **populated at the `SandboxSpec::Docker { … }` construction (line 535, start path, and the resume-path construction sharing it)**. |
| `lib/components/fabro-workflow/src/pipeline/initialize.rs` | Forgejo-origin runs: skip `GITHUB_TOKEN` minting; inject `FORGEGO_TOKEN`→`FORGEJO_TOKEN` + `FORGEJO_URL` into the sandbox env spec. |
| `lib/components/fabro-workflow/src/pipeline/pull_request.rs` | Extract LLM title/body generation into provider-neutral `generate_pr_content(...)` (no github types in signature). Add `open_forgejo_pull_request(...)`: verify head via `fabro_forgejo::branch_head_sha`, adopt existing via `find_open_pull_request`, else create; build `PullRequestLink` from the **API-returned `html_url`** (host from instance, `/pulls/N`); `auto_merge = true` → error "auto-merge is not supported for Forgejo pull requests" (fails publish via `PullRequestFailed`, no silent skip). |
| `lib/components/fabro-workflow/src/pipeline/publish.rs` | Branch: forgejo integration present and origin matches → `open_forgejo_pull_request` with identical `pr_config`/outcome plumbing; `outcome.pr_url` uses host-aware `html_url()`. |
| `lib/components/fabro-workflow/src/event/events.rs` | **(critique fix)** `Event::PullRequestCreated` variant + `pull_request_created()` constructor gain `host: Option<String>` (from `record.host`; constructor signature otherwise unchanged). |
| `lib/components/fabro-workflow/src/event/convert.rs` | **(critique fix)** Map `host` into `PullRequestCreatedProps` (line ~1400). |
| tests (`pull_request.rs`, `initialize.rs`, `events.rs`) | Forgejo create/adopt/SHA-mismatch/auto-merge-reject; round-trip test proving a forgejo `PullRequestLink` survives `Event → RunEvent → projection` **with host intact** (the exact gap the critique found). |

### Phase 5 — types events + store + server

| File | Change |
|---|
| `lib/foundation/fabro-types/src/run_event/misc.rs` | **(critique fix)** `PullRequestCreatedProps` gains `host: Option<String>` with `#[serde(default, skip_serializing_if = "Option::is_none")]` — the existing `head_sha` precedent; old events (no host) deserialize as github links. |
| `lib/components/fabro-store/src/run_state.rs` | **(critique fix)** Projection at line 386: `PullRequestLink { owner, repo, number, host: props.host.clone() }` so stored run records keep the forgejo host. |
| `lib/apps/fabro-server/Cargo.toml` | `fabro-forgejo` dep. |
| `lib/apps/fabro-server/src/server.rs` | `AppState::forgejo_integration()`: settings (enabled+url) + vault `FORGEJO_TOKEN`; enabled-with-url-but-no-token → actionable error ("run `fabro secret set FORGEJO_TOKEN`"). |
| `lib/apps/fabro-server/src/run_manifest.rs` | Admission (`run_repository_access_check_with` ~806): forgejo origin → PAT-embedded `ls-remote` preflight branch in `check_git_remote_ref`; thread `ForgejoIntegration` into worker-run credential plumbing (`automation_materializer.rs`, `worker_runtime.rs`, `server/session_runtime.rs` — same sites `github_app` uses). **No `spawn_env.rs` change** (verified: tokens reach workers via the shared vault under `FABRO_HOME`; the only github env passed explicitly is `GITHUB_APP_PRIVATE_KEY` for the App strategy, which has no forgejo analogue). |
| `lib/apps/fabro-server/src/server/handler/pull_requests.rs` | Dispatch in `get/merge/close_run_pull_request` (lines ~498/551/582) and `link` (~69): `record.host` (or link-URL host) matching the configured instance → forgejo client + creds; `github_coordinates_for_record` stays for github records. `pr link` validation via `from_url_for_hosts(&[instance_host])`, `/pull/` and `/pulls/` both accepted. |
| `lib/apps/fabro-server/src/server/pull_request_supervisor.rs` | Creation worker picks forgejo vs github per run origin from the same threaded integration. |
| `lib/apps/fabro-server/src/server/handler/system.rs` | `IntegrationProvider::Forgejo` status card: `configured` = url+token; connection = `get_current_user` (mirrors the Slack builder). |
| `lib/apps/fabro-server/src/server/tests.rs` | Admission pass/fail; link accept/reject; merge via forgejo twin; integrations payload includes forgejo; **projection test: forgejo-created event → stored link retains host → merge dispatches to forgejo**. |

### Phase 6 — manifest guard, install, CLI

| File | Change |
|---|
| `lib/components/fabro-manifest/src/lib.rs` | Guard + test only: forgejo remote survives `inspect_local_git`/`normalize_repo_origin_url` into `GitContext.origin_url` and yields no GitHub run target (`github_run_target` returns None — assert no error). |
| `lib/components/fabro-install/src/lib.rs` | `write_forgejo_settings(url)` (settings.toml table, mirror `github_integration_table`); `FORGEJO_INSTALL_SECRET_KEYS`. |
| `lib/apps/fabro-cli/src/args.rs` | `InstallCommand::Forgejo(InstallForgejoArgs { url: String, token: Option<String> })`; telemetry label `"install forgejo"` (~1399). |
| `lib/apps/fabro-cli/src/commands/install.rs` | `install forgejo`: validate https URL, write settings, store token via the same path the github token strategy uses for `GITHUB_TOKEN`. Non-interactive. |
| `lib/apps/fabro-cli/src/cli_reference.rs` | `install forgejo` row; insta snapshots via `cargo insta pending-snapshots` → review → accept. |
| `lib/apps/fabro-cli/src/commands/run/runner.rs` | `maybe_build_forgejo_credentials(settings, vault, run_spec)` sibling of `maybe_build_github_credentials` (~1102): attach when pull-request enabled or run git origin matches instance; worker reads `FORGEJO_TOKEN` from the shared vault and `[server.integrations.forgejo]` from `FABRO_CONFIG`-pointed settings. |

### Phase 7 — API surface + docs

| File | Change |
|---|
| `docs/public/api-reference/fabro-api.yaml` | `IntegrationProvider` enum += `forgejo`; `PullRequestLink` (line 12576) optional `host` property, `html_url` description notes instance-host `/pulls/N` form. No new endpoints; `RunScmSettings` untouched. |
| `lib/foundation/fabro-api/build.rs` | Verify `IntegrationProvider`/`PullRequestLink` replacement types still align; if a new `with_replacement` is needed, add the required type-identity/JSON-parity test per AGENTS rules. |
| `lib/packages/fabro-api-client/` | `bun run generate`. |
| `docs/public/integrations/forgejo.mdx` (new), `docs/public/docs.json`, `docs/public/administration/server-configuration.mdx`, `docs/public/reference/cli.mdx`, `.env.example` | Setup (instance URL, PAT, scopes), capabilities (Docker clone runs, PR lifecycle, `FORGEJO_TOKEN`/`FORGEJO_URL` in stages), limitations; nav + `[server.integrations.forgejo]` reference + CLI section + `FORGEJO_TOKEN` entry. |

### Phase 8 — twin + e2e

| File | Change |
|---|
| `test/twin/forgejo/` (new crate: `Cargo.toml`, `src/{lib,server,state,fixtures,test_support}.rs`, `src/handlers/{mod,user,branches,pulls,git}.rs`) + root `Cargo.toml` member | Lean mirror of `test/twin/github`: `/api/v1/user`, `/api/v1/repos/{o}/{r}/branches/{branch}`, pulls CRUD + merge, git smart-HTTP (`info/refs`, `upload-pack`, `receive-pack`); `Authorization: token` check; fixture-driven state; PR `html_url` values use `/pulls/N`. |
| E2E (`fabro-workflow/tests/it/` and/or `fabro-server`, `#[e2e_test]` twin style) | Clone forgejo origin in Docker → push → PR created with instance-host link; **merge via server handler dispatches to twin forgejo**; adopt-existing; missing-token admission failure. |

## Order

1 → 2 → 3 → 4 → 5 → 6 → 7 → 8, with `cargo nextest run -p <touched crate>` after each phase. Phase 4's event-host work lands **before** Phase 5's dispatch tests so the projection carries host when the server tests exercise it. OpenAPI (7) is after handlers exist because the wire shape settles then.

## Verification

- Per-crate nextest for every touched crate (`fabro-forgejo`, `fabro-static`, `fabro-types`, `fabro-config`, `fabro-sandbox`, `fabro-workflow`, `fabro-store`, `fabro-server`, `fabro-cli`, `fabro-manifest`, `fabro-install`).
- The critique-regression test: `Event → RunEvent → run_state projection` round-trip keeps `host`; server merge test uses the twin forgejo, proving dispatch is not silently hitting `api.github.com` (a wrong dispatch fails here, not in production).
- API regen: `cargo build -p fabro-api`; `fabro-server` conformance test (spec/router drift).
- TS: `bun run generate` in `lib/packages/fabro-api-client`, then `cd apps/fabro-web && bun run typecheck && bun test`.
- Snapshots: `cargo insta pending-snapshots`, review, targeted `cargo insta accept`.
- Full gate: `cargo build --workspace`; `cargo nextest run --workspace`; `cargo +nightly-2026-04-14 fmt --check --all`; `cargo +nightly-2026-04-14 clippy --workspace --all-targets -- -D warnings`; e2e twin profile for the new tests, then full e2e profile.
- Manual one-time smoke (documented in PR): official Forgejo docker image — `fabro install forgejo --url … --token …`, run in a repo whose `origin` points at it, verify the PR URL opens on the instance (`/pulls/N`), merge via the `fabro pr` flow. Only a real Gitea git-over-HTTPS server proves the twin can't.

## Deliberately not doing

- Webhooks/automations, browser OAuth login, tracker — out of scope (scope answer); `github_webhooks.rs`, `auth/github_endpoints.rs`, `fabro-tracker` untouched.
- Provider-trait refactor of `fabro-github` — sibling crate per architecture answer.
- Multi-instance / auto-detection — single configured instance; `PullRequestLink.host` keeps per-record hosts so a later map is additive, not a schema break.
- Daytona Forgejo clones — SDK clone is GitHub-shaped; explicit admission error, documented.
- API/RunIntent git targets for Forgejo — `GitRunTarget.repo` is a host-ambiguous GitHub slug; changing that grammar is a wire break out of scope. Forgejo runs originate from a workspace git remote.
- Auto-merge on Forgejo — no API; explicit `PullRequestFailed` error.
- Per-run scoped minting / `[run.integrations.forgejo]` — PAT-only; scopes live on the Forgejo token; PAT forwarded verbatim as `FORGEJO_TOKEN`.
- `RepositoryProvider::Forgejo` — needs configured host in config-free `fabro-types`; forgejo origins stay `Git` in display metadata.
- `spawn_env.rs` changes — verified unnecessary (vault-based flow).
- Web wizard/settings-card UI — lean reading of the last answer; only the system-integrations *endpoint* grows a forgejo entry.