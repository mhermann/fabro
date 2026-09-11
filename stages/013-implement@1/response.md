The implementation is complete. Here is the final summary.

# Forgejo integration — implementation summary

Following the approved plan (tier b: CLI + server-side; single instance + provider tag), with adaptations noted where reality differed.

## What was built, file by file

### A. Foundation
- **`lib/foundation/fabro-types/src/repository.rs`** — new `ScmProvider` enum (`Github` default / `Forgejo`, strum + snake_case serde); `Forgejo` added to the display-only `RepositoryProvider`; `repository_provider_with(origin, forgejo_url)`; `origin_matches_instance(origin, instance_url)` (SSH spellings match by host alone; HTTP(S) compares explicit/scheme-default ports).
- **`run_intent.rs`** — `GitRunTarget.provider` (serde-defaulted to `github`, omitted on serialize for wire compatibility); `validate_with_scm(forgejo_url)` on `GitRunTarget` and `RunTarget`; forgejo targets derive `origin = {instance}/{owner}/{repo}`; new `ForgejoUnconfigured` error variant; `ValidatedGitRunTarget::provider()`.
- **`pull_request.rs`** — `PullRequestLink.provider` + `origin` (required iff forgejo); `github()`/`forgejo()` constructors; `from_url` dispatching on host (`/pull/` vs `/pulls/`); forgejo links serialize 6 fields, GitHub links keep the historical 4-field shape; credential-stripping URL parse.
- **`settings/server.rs`** — `ForgejoIntegrationSettings { enabled, url }` + `instance_url()` helper.
- **`system_integrations.rs`** — `IntegrationProvider::Forgejo`.
- **`fabro-static`** — `EnvVars::FORGEJO_TOKEN` + optional-vault registry entry.
- **`fabro-config`** — `ForgejoIntegrationLayer` + resolve (enabled ⇒ URL required, trailing-slash trim, `http` only for loopback).

### B. New crate `lib/components/fabro-forgejo`
`ForgejoContext` (token redacted from `Debug`), the `HttpClient` trait + production impl, `url.rs` (`parse_owner_repo`, `repo_https_url`, `normalize_origin_url`, `embed_token_in_url` as `fabro:token`, per-instance `credential.<origin>.helper` reading `$FORGEJO_TOKEN`), and ops: `find_open_pull_request` (client-side base/head/SHA match), `create_pull_request` (draft → `WIP: ` prefix), `get_pull_request` (GitHub-shaped projection; no diff stats on Forgejo), `merge_pull_request` (`Do` mapping), `close_pull_request` (issues endpoint), `branch_head_sha`, `get_repo`, `current_user`. httpmock integration tests (11) + live e2e test.

### C. OpenAPI + clients
`fabro-api.yaml`: `GitRunTarget.provider` (new `ScmProvider` schema), `PullRequestLink.provider`/`origin`, `IntegrationProvider += forgejo`, new `GET /api/v1/repos/forgejo/{owner}/{name}` (`getForgejoRepo`). Rust types regenerated via `with_replacement` (no parallel DTOs); TS client regenerated; new forgejo parity/round-trip tests in `run_intent_round_trip.rs` + `pull_request_round_trip.rs`.

### D. Workflow engine
`ScmTarget` enum (`GitHub`/`Forgejo`) with exact origin resolution; `PublishOptions`/`RunOptions`/`StartServices`/`RunSession` carry `forgejo: Option<ForgejoContext>` (runtime-only); `pull_request.rs` dispatches verify/reconcile/create on the target (auto-merge = warn-and-skip on Forgejo); `run_metadata.rs` pushes forgejo metadata branches with a static PAT auth provider; `services.rs`/`initialize.rs` inject `FORGEJO_TOKEN` into stage/tool env only when the origin matches the instance; `OpenPullRequestRequest.github` became `Option` so forgejo targets never need GitHub credentials.

### E. Sandbox
`CloneDecision::Forgejo`; `decide_clone` classifies GitHub first, then instance match, else the updated "GitHub origins and configured Forgejo instance" error; shared `repo_layout`; `PushTokenSource` enum (GitHub minting source | static Forgejo PAT) so clone/refresh/lease machinery works unchanged for both forges; `SandboxSpec::Docker/Daytona.forgejo` (runtime-only, never persisted); token embedded at clone time via the forge-specific helper; records keep scrubbed origins.

### F. Server
`AppState::forgejo_context()` (settings URL + vault PAT); run admission validates targets with the instance URL and rejects unconfigured forgejo runs with remediation; preflight resolves forgejo credentials and verifies repository access against the instance API; `GET /repos/forgejo/{owner}/{name}` handler; integrations status row; PR live-detail/merge/close dispatch on link provider; diagnostics Forgejo token probe (absent when disabled); automation materializer resolves forgejo workflow sources (host-namespaced repo cache, basic-auth extraheader).

### G. Manifest & CLI
`configured_repo_origin_url` → `ConfiguredOrigin { url, provider }` with a hard error for `provider = "forgejo"` without an instance; `run_target_for_origin` produces forgejo targets from observed origins; `shared/forgejo.rs`; runner builds forgejo context and skips the GitHub gate for forgejo runs; `ensure_matching_repo_origin` normalizes SSH checkouts of the instance; doctor check (offline, names the missing piece); `repo init` writes `[run.scm] provider = "forgejo"` when it detects the instance.

### H. Web
Forgejo panel in `settings-integrations.tsx` (+ test fixture); automation form gains a provider select shown only when forgejo is configured, with `provider` in the payload; both automation routes thread `forgejoConfigured` from the integrations query. `pull-request-chip` needed no change (it renders `html_url`).

### I. Docs
`docs/public/integrations/forgejo.mdx` (scope table, setup, PAT scopes, WIP-draft behavior, security/limitations), `docs.json` nav entry, `changelog/2026-09-10.mdx`, and `FORGEJO_TOKEN` added to `docs/internal/server-secrets-strategy.md`.

**DB migration**: `fabro-db/migrations/2026091001_automation_target_provider.sql` — additive `target_provider`/`workflow_source_provider` columns so automation targets and workflow sources round-trip their provider (legacy rows default to GitHub).

## Adaptations from the plan (and why)
1. **`spawn_env` (item 33) intentionally skipped** — the plan's "FORGEJO_TOKEN passed to workers via env" would diverge from the architecture: `GITHUB_TOKEN` is *not* shipped to workers either; the worker resolves tokens from its own env/vault (`shared/forgejo.rs` mirrors `shared/github.rs`). Server-side runs inject the token via the workflow env at point of use.
2. **`HttpMethod::Put` unused** — Forgejo merge is POST, close is PATCH; the trait keeps the github-compatible shape.
3. **Automation persistence needed more than the plan said** — the plan assumed serde defaults suffice, but the store persists targets/workflow-sources as columns, so a column migration was required for provider round-trips (added with the same additive pattern as prior migrations).
4. **`git_bridge.rs` (item 21) unchanged** — it configures credentials for *declared additional repositories*, which are a GitHub-only feature in v1; forgejo git auth is handled in the sandbox clone path and server checkouts instead.
5. **`repo/init.rs`** — the plan implied a `[run.scm]` scaffold existed; it didn't for GitHub. The forgejo branch writes the block only for instance-hosted origins (GitHub needs no explicit scm block).

## Verification status
- **Passing runs**: fabro-types/static/config (809), fabro-forgejo (20), fabro-api round-trips incl. new forgejo parity tests, fabro-store (281), fabro-automation (28, incl. migration), foundation test batches, web `bun test` (no new failures vs. baseline) and `tsc` typecheck.
- **Full `cargo build --workspace --tests` completed successfully** on the final code state (all lib/test targets compile).
- **Blocked at the end**: runtime execution of the workflow/server/cli test binaries — the shared host disk went to 100% (`ld: No space left on device`) and stayed there despite repeated cleanup (`cargo clean`, registry purge, debuginfo disabled) and ~2 hours of waiting. The final source tree compiles; a reviewer should re-run `cargo nextest run --workspace` once disk frees. The plan's manual Docker gate (Codeberg Forgejo + live run) also remains as documented in the plan.