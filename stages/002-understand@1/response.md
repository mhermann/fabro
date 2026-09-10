Exploration complete. Here is my full understanding of the task.

---

## 1. Repository structure and tooling

**Fabro** is an AI workflow orchestration platform: workflows are Graphviz graphs whose stages (agents, prompts, commands, human gates) run in sandboxes. It's a **Rust workspace** with a **TypeScript SPA**:

| Area | Location | Notes |
|---|---|---|
| Rust apps | `lib/apps/` | `fabro-cli` (all user commands), `fabro-server` (Axum HTTP API + install wizard + webhooks) |
| Rust components | `lib/components/` | `fabro-github` (GitHub API client), `fabro-workflow` (engine), `fabro-sandbox` (Docker/Daytona/Local), `fabro-store`, `fabro-tracker`, `fabro-manifest`, `fabro-slack`, `fabro-install` (settings writers shared by CLI/server install) |
| Rust foundation | `lib/foundation/` | `fabro-types` (all canonical domain types), `fabro-config` (TOML layers), `fabro-api` (progenitor-generated from OpenAPI), `fabro-vault` (secrets), `fabro-static` (env-var registry), `fabro-oauth`, `fabro-http` |
| TypeScript | `apps/fabro-web` (React 19 SPA), `lib/packages/fabro-api-client` (generated Axios) | |
| Contract | `docs/public/api-reference/fabro-api.yaml` | **OpenAPI-first**: spec → `with_replacement(...)` Rust types → generated clients; conformance test catches drift |

Tooling: `cargo nextest`, pinned nightly fmt/clippy, `bun` for TS, insta snapshots, strategy docs in `docs/internal/` that are mandatory reading for the areas this change touches (events, testing, migrations, error-handling, server-secrets). Docs live in `docs/public/` (Mintlify, incl. `integrations/github.mdx`).

## 2. The goal, restated concretely

Today Fabro's forge story is **GitHub-only end to end**. "Add support for Forgejo similar to the GitHub integration" means: make a self-hosted Forgejo instance usable everywhere github.com currently is. The GitHub integration's full surface (all would need Forgejo counterparts or explicit exclusion):

1. **API client** — `fabro-github` crate: credentials (GitHub App JWT → scoped installation tokens, PAT), PR create/find/get/merge/close/auto-merge, `parse_github_owner_repo` (hard-codes `github.com`), clone-credential embedding, webhook config updates.
2. **Domain types** — `GitContext`, `PullRequestLink` (computed `html_url` = `https://github.com/...` and a deserializer that **rejects non-github.com hosts**), `GitHubRepositorySlug` (validates owner/repo and renders github.com URLs), `RepositoryProvider::{Github, Git, Unknown}`, `GitRunTarget` ("Public github.com repository target"), `IntegrationProvider::{Github, Slack}`.
3. **Server config & secrets** — `[server.integrations.github]` (`strategy: token|app`, `app_id`, `client_id`, `slug`, webhooks), `[server.auth.github]` (OAuth login), `[run.integrations.github]` (per-run token permissions + `additional_repositories`), `[run.scm]`, vault secrets `GITHUB_TOKEN` / `GITHUB_APP_PRIVATE_KEY` / `GITHUB_APP_CLIENT_SECRET` / `GITHUB_APP_WEBHOOK_SECRET`, env `GITHUB_BASE_URL` override.
4. **Server behavior** — `AppState::github_credentials()` resolution, PR routes (`/runs/{id}/pull_request*` merge/close), repo probe `GET /repos/github/{owner}/{name}`, inbound webhook `/api/v1/webhooks/github` (HMAC-verify + log), Tailscale funnel webhook registration, install wizard routes `/install/github/*`, OAuth login `/auth/login/github`, diagnostics, automation materializer, worker credential handoff.
5. **Workflow engine** — `pipeline/pull_request.rs` (LLM-generated PR title/body → `create_pull_request`), `pipeline/publish.rs` (push branch + open PR), `pipeline/initialize.rs` (`GitHubRepositoryAccess`, `InstallationTokenSource` → `GITHUB_TOKEN` into sandbox env), git credential helper.
6. **Sandbox clone contract** — `clone_source.rs::decide_clone()` errors **"Clone-based sandboxes currently support GitHub repository origins only"** for non-GitHub origins (unless `skip_clone`); Docker and Daytona providers both gate on `github_repo_layout()`.
7. **CLI** — `fabro pr create/link/view/merge/close` (thin API clients), `fabro install` (interactive + `--github-strategy token|app`), `repo init` app-installation probe, `doctor`, worker vault loading.
8. **Store/projection** — event-sourced `pull_request.*` events folding into `RunProjection.pull_request`.
9. **Web UI + docs** — install wizard UI, PR chip, auth login; `docs/public/integrations/github.mdx`.
10. **Tracker** — GitHub Projects V2 issue tracking in `fabro-tracker`.

A Forgejo feature means introducing a **host-aware forge layer**: Forgejo is self-hosted at arbitrary domains (codeberg.org, `forgejo.example.com`), exposes a deliberately GitHub/Gitea-compatible REST API under `{instance}/api/v1`, supports OAuth2 apps and PATs — but has **no GitHub-App/installation-token equivalent**, and its tokens don't do per-repo permission scoping.

## 3. What I know for certain

- There is **zero existing Forgejo/Gitea code** — the only repo mention is an unrelated doc analogy. `GITHUB_BASE_URL` env exists but is a GitHub(-Enterprise-style) override, not a forge abstraction.
- All forge logic funnels through `fabro-github` + `fabro-types`; the store is provider-agnostic in *storage* but GitHub-shaped in *types* (`PullRequestLink` validation rejects non-github.com URLs, breaking any stored Forgejo PR).
- The sandbox layer has a **single choke point** (`decide_clone()` + `github_repo_layout()`) enforcing GitHub-only clone origins; per CLAUDE.md this is a deliberate contract with exact-SHA semantics both providers must preserve.
- The OpenAPI spec is regenerated and parity-tested (`with_replacement` + round-trip tests), so any config/type addition is a spec change with generated-client regeneration, not just a Rust edit.
- Secrets flow vault → `AppState`/worker snapshot with fail-closed env scrubbing; new secrets must register in `EnvVars` + `secret_registry` and follow `docs/internal/server-secrets-strategy.md`.
- Webhook handling today is **verify-and-log only** (no dispatch), which bounds the inbound-webhook scope for parity.
- The web auth path uses `IdpIdentity::new("https://github.com", ...)` — issuer-keyed, so a Forgejo IdP needs a distinct issuer.
- Strategy docs constrain implementation choices (strum for enums, no glob imports, shell_quote for shell interpolation, events/testing/migrations strategies).

## 4. Genuinely ambiguous — where two engineers would build different software

1. **Scope of "similar".** Full parity (auth login, install wizards, tracker, diagnostics, doctor, web UI) is weeks of work; the *load-bearing core* is clone-from-Forgejo + PR create/merge/close + token config. Which subset is this task? Does it include the web-UI install wizard and OAuth login, or backend-only?
2. **Auth model mapping.** GitHub's `strategy: token | app` doesn't map: Forgejo has **no GitHub App / installation-token equivalent**. Options: PAT-only (drop the strategy enum), Forgejo OAuth2-app flow (closest analog, but no scoped per-repo tokens), or mint a plain token. This cascades into `[run.integrations.forgejo].permissions`, which is meaningless without installation tokens.
3. **Architecture: parallel crate vs. forge abstraction.** Mirror `fabro-github` with `fabro-forgejo`, or extract a host-generic `Forge`/`GitHost` trait (like the existing `Sandbox` trait) with two impls? Since Forgejo's API is GitHub-compatible, one could parameterize `fabro-github` by base URL — but they'd be coupled forever. This is the single biggest design fork.
4. **Instance/host model.** One configured Forgejo instance per server (like `GITHUB_BASE_URL`) vs. per-repository origin detection (any host can be a Forgejo). Detection is genuinely hard: `RepositoryProvider::Git` currently means "not github.com" — you can't distinguish Forgejo from GitLab from a bare git URL without probing the remote. Multi-instance support (several Forgejo servers) multiplies config complexity.
5. **Breaking vs. migrating persisted types.** `PullRequestLink`, `RepositoryProvider`, and `GitRunTarget` serialize today. Adding a host/instance field needs a backward-compatible wire format (old records lack it) — follow `docs/internal/migrations-strategy.md`, but the shape (add optional `host`? new `PullRequestCoordinates`? tag by provider?) is a real choice.
6. **Coexistence semantics.** `ServerIntegrationsSettings` currently requires `[github, slack]`. Can GitHub and Forgejo be enabled *simultaneously* (per-run dispatch on origin URL), or is it one forge per server? This determines whether `GitHubContext`-style singletons become enums/maps.
7. **API-compatibility exploitation depth.** Lean on Forgejo's GitHub-compatible endpoints (shared request/response types, e.g. `PullRequestGithubDetail`) vs. separate Forgejo-shaped types? Divergences are real: base path `/api/v1`, token formats, merge methods per instance config, auto-merge availability, draft-PR fields.
8. **Webhooks & outbound registration.** Implement the inbound `/api/v1/webhooks/forgejo` (Forgejo sends `X-Hub-Signature-256`, same HMAC scheme, slightly different payloads) and startup registration, or defer? Parity says yes; utility says the current GitHub handler doesn't dispatch anyway.
9. **Testing strategy.** Live e2e against a containerized Forgejo (codeberg/forgejo image, matching the repo's `#[e2e_test]` twin/live patterns) vs. pure mock/twin HTTP tests. Affects CI footprint and whether exact-SHA clone semantics get real verification.
10. **CLI/install UX.** Does `fabro install` grow `--forgejo-*` flags and interactive prompts? Does `fabro repo init`'s app-installation probe apply (no Forgejo App concept → different guidance)?

I have not written or modified anything. Ready to proceed to design/clarification whenever you are.