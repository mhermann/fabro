I now have a complete picture of the GitHub integration surface. Here's the report.

# Repository Understanding Report

## 1. Restated goal, concretely

**"Add support for Forgejo similar to the GitHub integration"** means: give Fabro the ability to talk to a Forgejo instance (self-hosted Gitea fork) the way it talks to github.com today. In this codebase, "the GitHub integration" is not one thing — it's a constellation spread across ~10 crates:

| Surface | Where it lives |
|---|---|
| API client (auth, PRs, clone creds, app webhooks) | `lib/components/fabro-github/` (~2000-line `lib.rs`, `access.rs`, `token_source.rs`) |
| Settings schema | `lib/foundation/fabro-types/src/settings/server.rs` (`[server.integrations.github]`, strategy `token\|app`), `settings/run.rs` (`[run.scm]`, `[run.integrations.github]`) |
| Domain types | `fabro-types/src/repository.rs` (`GitHubRepositorySlug`, `RepositoryProvider::{github,git,unknown}`), `pull_request.rs` (`PullRequestLink` — hardcoded `https://github.com/.../pull/N`), `auth.rs` (`AuthMethod::Github`), `run_intent.rs` (targets must be valid GitHub slugs) |
| Webhooks + Tailscale funnel | `fabro-server/src/github_webhooks.rs` (HMAC `X-Hub-Signature-256`, route `/api/v1/webhooks/github`) |
| OAuth browser login | `fabro-server/src/web_auth.rs` (`/login/github`, `/callback/github` using App `client_id`) |
| Auto-PR pipeline | `fabro-workflow/src/pipeline/pull_request.rs` (2046 lines: create/find/merge/close/auto-merge) |
| Sandbox cloning | `fabro-sandbox/src/clone_source.rs` (`CloneDecision::GitHub` — **non-GitHub origins fail** per CLAUDE.md), `push_credentials.rs`, `git_bridge.rs` (credential helper host-scoped to `https://github.com`) |
| Install wizard | `fabro-cli/src/commands/install.rs` (3828 lines; GitHub App manifest registration flow) + `fabro-install` |
| Projects tracker | `fabro-tracker/src/github.rs` (GraphQL Projects V2; `Tracker` trait already exists as a multi-provider abstraction) |
| Secrets/env | `fabro-static` (`EnvVars::GITHUB_*`, secret registry), `fabro-vault`, `fabro-server/server_secrets.rs` |
| HTTP API | OpenAPI spec `docs/public/api-reference/fabro-api.yaml`: `/install/github/*`, `/api/v1/webhooks/github`, `/api/v1/repos/github/{owner}/{name}`, `provider: github` enums |
| Docs | `docs/public/integrations/github.mdx` |

A Forgejo implementation would decide which of these surfaces to build, where a `fabro-forgejo` crate (or a shared SCM abstraction) lands, and how per-instance (self-hosted) addressing replaces the single hard-coded `github.com` host.

## 2. What I know for certain

- **Language/tooling**: Rust workspace (`lib/apps`, `lib/components`, `lib/foundation`) — build with `cargo build --workspace`, test with `cargo nextest`, pinned nightly `rustfmt`/`clippy` (`nightly-2026-04-14`). TypeScript React 19 SPA in `apps/fabro-web` (Bun). Docs are Mintlify MDX under `docs/public`.
- **API contract flow is OpenAPI-first**: edit `fabro-api.yaml` → `cargo build -p fabro-api` regenerates Rust types (progenitor) → handler in `fabro-server/src/server.rs` + route → conformance test catches drift → regenerate TS client in `lib/packages/fabro-api-client`. Any Forgejo API surface must go through this pipeline.
- **No Forgejo/Gitea code exists today** (grep across the whole repo returns nothing).
- **A provider concept already exists** in three places: `RepositoryProvider` enum (`github|git|unknown`), `PullRequestMeta.provider` field (currently always `"github"`), and `IntegrationProvider::{Github, Slack}`. `Sandbox` and `Tracker` are existing trait-based provider abstractions.
- **`GITHUB_BASE_URL` env override already exists** — but only swaps the API endpoint for `fabro-github` calls; host detection (`is_github_origin`, `parse_github_owner_repo`), the git credential helper, and PR link parsing remain hard-coded to `github.com`.
- **GitHub auth is two-strategy**: `token` (PAT from vault/`gh auth token`) and `app` (App JWT → installation tokens scoped per-repo/per-permission). Credentials resolve through `GitHubCredentials::{App, Pat, Installation}` and `GitHubContext` everywhere.
- **Clone-based sandboxes (Docker, Daytona) are contractually GitHub-only**: `clone_source.rs` errors with "Clone-based sandboxes currently support GitHub repository origins only".
- **Conventions that bind the change**: strategy docs must be read before touching events/emitters, server secrets, migrations, error handling, tests (`docs/internal/*.md`); `shell_quote()` for any shell interpolation; strum for enum↔string; no glob imports; `test-support` feature gating for test helpers; insta snapshots for CLI tests.
- **GitHub integration has its own strategy matrix** (docs: token vs app) gating browser sign-in, webhooks, web UI availability — a Forgejo equivalent needs an analogous product definition.
- **Testing patterns to mirror**: `fabro-github` uses a mock `HttpClient` trait for unit tests and live contract tests (`tests/live_access.rs`); server has `server/tests.rs` (~12k+ lines) plus OpenAPI conformance checks.

## 3. What is genuinely ambiguous

1. **Scope of "the integration".** GitHub parity spans cloning, auto-PR/auto-merge, OAuth login, webhooks, sandbox token injection, Projects tracking, install wizard, doctor checks. Two engineers would ship very different v1s: "clone + PR against a Forgejo instance" vs "full parity including auth and webhooks." Nothing in the instruction picks a subset.

2. **Auth model.** Forgejo has **no GitHub-App equivalent** — no JWT app auth, no scoped installation tokens, no manifest-registration flow. Does Forgejo support mean PAT-only (`Authorization: token …`), OAuth2 apps for login, or both? And how is the loss of per-run scoped token minting (`[run.integrations.github.permissions]` → sandbox `GITHUB_TOKEN`) handled — unsandboxed full PAT, or omitted?

3. **Single instance vs. multi-instance.** Forgejo is self-hosted. Is exactly one Forgejo instance configured per Fabro server (`[server.integrations.forgejo] url = …`), with a hard either/or against github.com? Or multiple instances (breaking the single-`github.com` assumption in `GitHubRepositorySlug`, `RunIntent`, and `/api/v1/repos/github/…`)? This choice reshapes the domain model.

4. **Architecture: mirror crate vs. abstraction refactor.** Build a parallel `fabro-forgejo` crate and thread a second provider through call sites (minimal, matches how `fabro-slack` sits beside it), or first extract a `ScmProvider`/`ForgeCapability` trait and make GitHub the first implementation? The latter touches `GitHubRepositorySlug`, `PullRequestLink`, `RunIntent` validation — a much larger, riskier change. Both are defensible readings of "similar to the github integration."

5. **Run admission and API addressing.** Run targets must currently be "a valid GitHub owner/name slug" (`run_intent.rs`), and the API has `/api/v1/repos/github/{owner}/{name}`. How is a Forgejo repo named — new `provider` field on run targets (wire change), a URL, or a `forgejo` enum variant in existing types? OpenAPI/TS-client regeneration is forced either way.

6. **Webhooks.** Separate `/api/v1/webhooks/forgejo` route? Gitea/Forgejo webhook payloads differ from GitHub's (and offer GitHub-compat webhook type). What triggers exist — the current GitHub webhook path appears to be receive-and-verify with funnel-based exposure; does Forgejo need automation triggers (push/PR-comment → run) from day one?

7. **OAuth/browser login.** Does `AuthMethod` gain `forgejo` (an auth-settings + OpenAPI + `web_auth.rs` change), or is login out of scope for v1? GitHub's strategy matrix couples web-UI availability to App-strategy login; Forgejo's equivalent coupling is undefined.

8. **Forgejo vs. Gitea.** Forgejo is a Gitea fork with a compatible API. Support `forgejo` only, or name things so Gitea works too (affects crate name, settings keys, docs)?

9. **Sandbox clone support.** Do Docker/Daytona sandboxes clone Forgejo origins (requires per-host credential plumbing in `clone_source.rs`, `push_credentials.rs`, `git_bridge.rs` — all currently github.com-scoped), or is v1 server-side/CLI only?

10. **Install/doctor surface.** GitHub has a guided App-manifest install flow and a 5-field doctor check. Forgejo's install is presumably "enter instance URL + token." New wizard step, new `fabro install forgejo`-style flow, or manual settings only? Plus: secrets naming (`FORGEJO_TOKEN`? `FORGEJO_URL`?) across `EnvVars`, secret registry, and vault.

11. **Auto-merge & Projects parity.** GitHub auto-merge enablement and Projects V2 GraphQL tracking have no guaranteed Forgejo equivalent (REST-only, different merge endpoint semantics). Include degraded/absent, or explicitly out of scope?

**Recommendation to resolve before coding**: the highest-leverage decisions are #3 (instance model), #4 (mirror vs. trait abstraction), and #2 (auth). I'd suggest asking the requester to confirm scope — my reading of "similar to the github integration" with minimal blast radius is: a `fabro-forgejo` component crate (PAT + optional OAuth2), single configured instance under `[server.integrations.forgejo]`, covering clone-credential resolution + auto-PR for CLI/server runs, with webhooks/login/tracker deferred — but that is a choice, not a derivation.