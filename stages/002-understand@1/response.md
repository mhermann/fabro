I have enough understanding now. Here is my report.

---

## 1. Repository structure, language, tooling

**Fabro** is an AI-powered workflow orchestration platform. Rust workspace (edition 2021, v0.345.0-nightly.0) + TypeScript apps, built with `cargo`/`nextest`/`clippy`/`rustfmt` (pinned nightly) and `bun` for TS.

| Layer | Location | Contents |
|---|---|---|
| Apps | `lib/apps/` | `fabro-cli`, `fabro-server` (Axum), `fabro-mcp-server`, `fabro-spa` |
| Components | `lib/components/` | 21 crates incl. **`fabro-github`** (the integration in question), `fabro-sandbox`, `fabro-workflow`, `fabro-tracker`, `fabro-slack`, `fabro-manifest`, `fabro-install` |
| Foundation | `lib/foundation/` | `fabro-types` (shared domain types/settings), `fabro-static` (env-var registry + secret registry), `fabro-config`, `fabro-http`, `fabro-oauth`, `fabro-vault`, etc. |
| TS | `apps/fabro-web` (React 19 SPA), `lib/packages/fabro-api-client` (generated from OpenAPI) | |
| Contract | `docs/public/api-reference/fabro-api.yaml` | OpenAPI-first: spec → progenitor Rust types → TS client |
| Tests | `test/twin/github` | A **mock GitHub server harness** ("twin") used by dual-mode e2e/unit tests |

Governance rules that matter: AGENTS.md/CLAUDE.md mandate reading internal strategy docs before touching events/errors/tests/server-secrets; OpenAPI is source of truth with `with_replacement` type-unification rules; strum for enum conversions; test helpers must be feature-gated (`test-support`).

## 2. Goal restated in this codebase's terms

"Add Forgejo support similar to the GitHub integration" means introducing a second Git-forge integration alongside `fabro-github` such that Fabro can talk to a (self-hosted) Forgejo instance the way it today talks exclusively to github.com. In practice the GitHub integration spans **eight distinct surfaces**, each of which a Forgejo equivalent would have to decide about:

1. **Client crate** — `lib/components/fabro-github` (~4.4k lines): credentials (`Pat` / `App` / `Installation`), JWT signing, installation-token minting, `HttpClient` trait, PR create/find/merge/close/auto-merge, branch HEAD SHA, app-install checks, webhook config updates, URL parsing (`parse_github_owner_repo`, `ssh_url_to_https`, `normalize_repo_origin_url` — all hardcode `github.com`), git credential helper hardwired to `credential.https://github.com.helper`, `InstallationTokenSource` (cached, single-flight), `GitHubRepositoryAccess`.
2. **Domain types** (`fabro-types`): `PullRequestLink` (serializes a computed `https://github.com/.../pull/N` URL and *rejects non-github.com URLs on deserialize*), `PullRequestGithubDetail`→`PullRequestDetails`, `GitContext`, `RepositoryProvider::{Github,Git,Unknown}`, `GitHubRepositorySlug` (GitHub name-syntax validation), `SystemIntegrationStatus`/`IntegrationProvider::{Github,Slack}`, `AuthMethod::Github`, `IdpIdentity` issuer `https://github.com`.
3. **Settings** — `[server.integrations.github]` (`GithubIntegrationStrategy::{Token,App}`, app_id/client_id/slug/webhooks), `[run.integrations.github.permissions]` (sandbox token scopes), `[run.scm] provider/owner/repository/github`, `[server.auth] methods/github.allowed_usernames`; env vars `GITHUB_TOKEN`, `GITHUB_BASE_URL`, `GITHUB_APP_PRIVATE_KEY`, `GITHUB_APP_CLIENT_SECRET`, `GITHUB_APP_WEBHOOK_SECRET` registered in `fabro-static`'s EnvVars **and** secret/redaction registries; vault keys managed by `fabro-install`.
4. **Server** — `install.rs` wizard (token strategy via `gh auth token`; app strategy via manifest flow + localhost callback), browser OAuth login (`auth/github_endpoints.rs`, `fabro-oauth` PKCE), inbound webhooks (`github_webhooks.rs`, route `/api/v1/webhooks/github`, HMAC `X-Hub-Signature-256`, Tailscale-funnel webhook strategy), `GET /repos/github/{owner}/{name}`, PR supervisor, automation materializer, `git_checkout.rs`, run compiler.
5. **Sandbox** — Docker/Daytona are *clone-based GitHub-only*: `clone_source.rs` errors "Clone-based sandboxes currently support GitHub repository origins only"; `push_credentials.rs` embeds/refreshes tokens in the origin remote.
6. **Workflow engine** — `git_bridge.rs` (github.com-scoped credential helper, SSH→HTTPS rewrites, `insteadOf`), auto-PR pipeline (`pipeline/pull_request.rs` driven by `GitHubContext`), `Initialize`/`services.rs` env injection of `GITHUB_TOKEN` into sandboxes.
7. **Tracker** — `fabro-tracker/src/github.rs` (GitHub Projects V2 GraphQL tracker; there's also a Linear tracker as the multi-tracker precedent).
8. **CLI, web UI, docs, OpenAPI, twin tests** — `gh.rs`, install web flow, `settings-integrations.tsx` GithubPanel, `docs/public/integrations/github.mdx`, ~15 OpenAPI paths/schemas referencing github.com URLs, `test/twin/github`.

So the concrete task is: design where a Forgejo (self-hosted, Gitea-lineage, `/api/v1` REST, token/OAuth2 auth) forge fits into each of these surfaces, choosing between a parallel `fabro-forgejo` crate and a generalized forge abstraction.

## 3. What I know for certain

- There is **zero existing Forgejo/Gitea code or config** anywhere in the repo (only an incidental mention of "Gitea" in a design doc's prior-art list).
- GitHub is integrated **deeply and by name**, not behind an abstraction: `github.com` is hardcoded in URL parsing, PR-link (de)serialization, the git credential-helper key, sandbox clone decisions, and the OpenAPI schema; `GITHUB_BASE_URL` exists but only retargets the *API* endpoint (used by twins/tests), never the forge host identity.
- The settings/config surface is TOML-driven with layered resolution (workflow/project/user), migration-aware parsing (`fabro-config/parse.rs` contains deprecation-split messages for the existing github keys), and a vault (`fabro-vault`) plus a central env/secret registry (`fabro-static`) that redacts secrets.
- Multi-provider precedent exists in adjacent domains: `SandboxProviderKind::{Local,Docker,Daytona}` with per-provider settings; `Tracker` trait with `GitHubTracker` and `LinearTracker`; `IntegrationProvider::{Github,Slack}` for status reporting. These show the house style for adding a peer provider.
- Auth strategy is a first-class choice (`GithubIntegrationStrategy::{Token,App}`) with capability consequences (token ⇒ no web login/webhooks; app ⇒ OAuth + webhooks), documented in `docs/public/integrations/github.mdx`.
- The OpenAPI spec is regenerated into Rust (`fabro-api/build.rs` progenitor, with `with_replacement` for type unification) and TS (`fabro-api-client`), and a conformance test catches spec/router drift — any new endpoint or enum variant must flow spec-first.
- Testing infrastructure includes a full fake GitHub twin (`test/twin/github`) with fixtures/handlers/auth, and mock-HTTP unit tests inside `fabro-github` (`tests_mock.rs`) and `fabro-workflow`.
- Forge facts that constrain design: Forgejo is self-hostable (no single fixed host), exposes a Gitea-compatible REST API under `/api/v1` with GitHub-shaped-but-not-identical payloads (e.g. `has_merged` vs `merged`, `/pulls/` vs `/pull/` in web URLs, token-style `Authorization: token …`), supports OAuth2 apps and scoped PATs, and has **no equivalent of GitHub App installation tokens**. Webhooks can be delivered in Gitea or GitHub-compatible formats.
- Repo conventions that would apply: read `docs/internal/{events,error-handling,testing,server-secrets,migrations}-strategy.md` before implementation; strum derives for new enums; feature-gated `test_support`; `shell_quote()` for any shell interpolation; shell command construction in sandbox code.

## 4. What is genuinely ambiguous

1. **Scope of parity.** Does "similar to github integration" mean full parity (auth/login, install wizard, webhooks, sandbox cloning, auto-PR/auto-merge, tracker, integrations status, web UI) or a minimal viable slice (PAT + clone + PR creation)? The GitHub integration is ~8 surfaces; two engineers would almost certainly draw the v1 line differently.
2. **Auth model.** GitHub has three credential kinds (PAT / App / installation token) and the whole token-refresh machinery (`InstallationTokenSource`, `push_credentials.rs`) exists because installation tokens expire. Forgejo has no App/installation-token equivalent. PAT-only? OAuth2 app with refresh? Scoped tokens mapped from `[run.integrations.github.permissions]`? Each changes how much of the token-source architecture is reused vs. bypassed.
3. **Instance topology.** github.com is one host; Forgejo is N self-hosted instances. Is Forgejo configured **once server-wide** (a single `[server.integrations.forgejo]` with a base URL), or resolved **per origin/per run** (any Forgejo remote works, credentials matched by host)? This single decision drives config schema, credential storage, URL parsing, and security posture (token exfiltration across instances).
4. **Architecture: parallel crate vs. forge abstraction.** New `fabro-forgejo` mirroring `fabro-github`, or extract a `forge`-provider trait (auth, PR ops, URL math, credential helper) with GitHub and Forgejo implementations? The repo's own "API type ownership / avoid parallel duplicate types" guidance pushes toward unification, but the GitHub API surface in the crate is large and the two APIs are *similar, not identical* — where the similarities lie is a judgment call (Forgejo's Gitea-lineage API means partial compat, e.g. `merged` vs `has_merged`).
5. **Wire/format compatibility and migrations.** `PullRequestLink` currently *rejects* non-github.com URLs; supporting Forgejo PR URLs (`…/pulls/N`) changes a persisted, wire-visible type (runs store this). Do we extend `PullRequestLink` with a host/provider field, add a `PullRequestForgejoDetail`, and migrate stored runs — or treat Forgejo links as a new variant? Same question for `RepositoryProvider` (new `Forgejo` variant ripples into OpenAPI enum + DB), `RunScmSettings` (`[run.scm.forgejo]`?), and `IntegrationProvider`.
6. **Webhooks.** One shared inbound route or `/api/v1/webhooks/forgejo`? Which payload format (Gitea-native vs GitHub-compatible) and signature header (`X-Gitea-Signature` vs `X-Hub-Signature-256`)? Do webhook strategies (Tailscale funnel / server URL) carry over for self-hosted instances where the server may be on the same network? Do webhooks drive the automation materializer?
7. **Sandbox clone support.** Must Docker/Daytona accept Forgejo origins (changing the explicit "GitHub origins only" contract in `clone_source.rs` and the credential-embedding model), or is Forgejo local-sandbox-only in v1? CLAUDE.md documents the GitHub-only clone-source contract as intentional.
8. **Browser login / identity.** Does Forgejo become a `ServerAuthMethod` (OAuth2 against the instance, `AuthMethod::Forgejo`, per-instance `IdpIdentity` issuer), or is auth out of scope? The token-vs-app capability matrix (web UI enabled only with app strategy) would need a Forgejo column.
9. **Does Gitea compatibility come along?** Forgejo is a Gitea fork with a compatible API — build "Gitea-compatible" and name it Forgejo, or strictly Forgejo? Affects naming, docs, and any version-specific API behavior.
10. **Install/UX surface.** Should `fabro install` grow a Forgejo path (base-URL prompt, token/OAuth2-app creation), and does the web install UI + `docs/public/integrations/forgejo.mdx` + twin test harness (`test/twin/forgejo`) belong in the same change? Nothing in the repo pins this down.

**Recommended clarifications before coding:** scope (which of the 8 surfaces), instance topology (single configured instance vs. per-origin), auth model (PAT/OAuth2), and crate architecture (parallel vs. abstracted) — these four determine nearly everything else.