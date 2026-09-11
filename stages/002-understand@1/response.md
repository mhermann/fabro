Exploration complete. Here is my understanding.

## 1. Repository structure and where the change lands

**Language/tooling:** Rust workspace (crates in `lib/apps`, `lib/components`, `lib/foundation`) + TypeScript/React 19 web app (`apps/fabro-web`, Bun). OpenAPI-first API (`docs/public/api-reference/fabro-api.yaml` → progenitor Rust types, openapi-generator TS client). Mintlify docs in `docs/public`. Tests via `cargo nextest`; strategy docs in `docs/internal/` that must be read before touching events, errors, logging, migrations, secrets, tests, React effects.

**The GitHub integration is not one module — it's a vertical slice through the entire stack:**

| Layer | File(s) | Role |
|---|---|---|
| Dedicated crate | `lib/components/fabro-github/` (~4.4k lines: `lib.rs`, `access.rs`, `token_source.rs`) | Credentials (`App` w/ RSA JWT + installation tokens, `Pat`, `Installation`), PR REST calls (create/get/merge/close/auto-merge/find-open), URL parsing (`parse_github_owner_repo`, `normalize_repo_origin_url`, `ssh_url_to_https`), token-embedding clone URLs, secret-free git credential helper, `gh auth token` shell-out, `GitHubRepositoryAccess` (validated repo set + permissions) |
| Types | `lib/foundation/fabro-types/` | `GitHubRepositorySlug` (hardcodes `https://github.com/{self}`), `GitRunTarget`/`RunTarget` ("must be a valid GitHub owner/name slug"), `PullRequestLink` (hardcodes github.com `html_url`), `PullRequestGithubDetail`, `[run.integrations.github]` settings (permissions + `additional_repositories` → minted sandbox `GITHUB_TOKEN`), server settings (`[server.auth.github]`, `[server.integrations.github]` with `token`/`app` strategy, webhook strategy) |
| Server | `lib/apps/fabro-server/` | Webhook route `/api/v1/webhooks/github` + HMAC verify (currently log-only), Tailscale-funnel webhook URL management, PR handlers (`server/handler/pull_requests.rs`), run manifest/authenticated-clone resolution, git checkout, automation materializer, install-time endpoints, diagnostics |
| Sandbox | `lib/components/fabro-sandbox/` | Clone-based providers (Docker/Daytona) are **GitHub-origin-only by contract** (`clone_source.rs`: "Clone-based sandboxes currently support GitHub repository origins only"); push credentials, exact-commit fetch, token refresh |
| Workflow | `lib/components/fabro-workflow/` | `run_options.rs` carries `github_app` credentials; PR ops in `handler/command.rs`; `operations/fork.rs` |
| Tracker | `lib/components/fabro-tracker/` | `Tracker` trait with `GitHubTracker` (Projects V2 **GraphQL**) and `LinearTracker` — an existing multi-provider abstraction |
| CLI | `lib/apps/fabro-cli/` | `install.rs` (GitHub App manifest flow, PAT via `gh`), `repo/init.rs`, `shared/github.rs` |
| Config/env | `lib/foundation/fabro-static/` | `GITHUB_APP_PRIVATE_KEY`, `GITHUB_APP_WEBHOOK_SECRET`, `GITHUB_APP_CLIENT_SECRET`, `GITHUB_TOKEN`, `GITHUB_BASE_URL` (registry + secret scrubbing) |
| Web | `apps/fabro-web/app/` | Install flow, automation form, PR chips, run summaries |
| OpenAPI/docs | `docs/public/api-reference/fabro-api.yaml`, `docs/public/**` | `/install/github/*` endpoints; setup guides |

There is **zero existing Forgejo/Gitea code** (case-insensitive search confirms).

## 2. Goal restated in this codebase's terms

Build a Forgejo (self-hosted, Gitea-descended forge) integration with parity to the GitHub slice: a `fabro-forgejo` component crate with token credentials, REST (not GraphQL) PR/merge/close operations, webhook route, clone-source support in Docker/Daytona sandboxes, run-target/repository identity that can name a Forgejo instance (not just `owner/repo` on github.com), server/CLI install + settings, and web/API/docs surface — so users can point Fabro at a Forgejo instance and get authenticated clones, PR lifecycle, and (optionally) webhook-driven behavior.

## 3. What I know for certain

- The whole pipeline assumes **GitHub is identified implicitly by `github.com`**: `GitHubRepositorySlug` has no host, `https_url()` hardcodes the domain, `parse_github_owner_repo` rejects non-github.com URLs, and `GitRunTarget::validate` requires a GitHub slug. A Forgejo repo cannot round-trip these types today.
- Auth model on GitHub = App JWT → installation tokens (scoped permissions map, multi-repo), PATs, or installation tokens; refresh logic in `token_source.rs`; `GITHUB_TOKEN` minted into sandboxes per `[run.integrations.github]`.
- Sandbox clone path is GitHub-only by documented contract (CLAUDE.md + `clone_source.rs` error); auth is via `embed_token_in_url` (`x-access-token:<token>@github.com`) and the env-based credential helper.
- The webhook endpoint verifies HMAC-SHA256 (`X-Hub-Signature-256`) and only logs the delivery — no run triggering exists yet, so "webhook integration" currently means *receive + verify*, not act.
- Server settings already have an integrations framework (`github`, `slack`) and the tracker layer already demonstrates provider abstraction (GitHub/Linear).
- `GITHUB_BASE_URL` env override already exists for API-base redirection (GHE-style), but URL parsing/clone paths still pin `github.com`.
- The repo has strict conventions that this work must follow: strum for enums, OpenAPI-first API changes + `with_replacement` type-unification rules, test-support feature gating, secret registry updates, shell quoting via `shell_quote()`, and reading the relevant `docs/internal/*-strategy.md` before touching those areas.
- Forgejo, as a Gitea fork: REST API at `<instance>/api/v1` is largely GitHub-shaped but not identical (token auth header form, no GitHub-App model in the same shape — though Forgejo v10 added "Forgejo Apps"), no GraphQL (so no Projects V2 tracker equivalent), webhook headers/signatures differ (`X-Forgejo-Event`/`X-Gitea-Event`, `X-Forgejo-Signature`), and it is inherently **multi-instance/self-hosted** — every repository identity needs an instance base URL.

## 4. What is genuinely ambiguous

Reasonable engineers would diverge on:

1. **Identity model (the biggest one).** Should Forgejo support be *one configured instance per Fabro server* (simple: settings-level `forgejo.url` + token, reuse `owner/repo` slugs scoped to that instance) or *multi-instance* (requires host-qualified repo identity, i.e., changing/parallel to `GitHubRepositorySlug` and `GitRunTarget`)? This determines whether `fabro-types` gets a forge-aware `GitRunTarget` variant or Forgejo stays a side-channel.
2. **Which surface is "the integration"?** Minimum viable: PAT + clone + PR create/merge/close (unblocks core workflow). Full parity would add: webhook route, run-integrations token minting into sandboxes (`run.integrations.forgejo`?), OAuth login (`server.auth`), tracker (no GraphQL exists — skip or REST-based?), install flow, CLI `repo init`, diagnostics. The instruction "similar to github integration" doesn't say how much.
3. **Auth mechanism.** PAT-only (Forgejo's universal answer) vs. OAuth2 app vs. Forgejo Apps (v10+, scoped, closest to GitHub Apps but a much newer, version-dependent API). GitHub's installation-token machinery largely doesn't map; a naive port would be wrong.
4. **Architecture: provider abstraction vs. sibling crate.** Extract a shared `Forge` trait (git host interface) that GitHub and Forgejo both implement — a large refactor of `fabro-github` consumers across 7 crates — vs. a parallel `fabro-forgejo` crate with its own call sites and explicit forge dispatch at boundaries. Both are defensible; the abstraction choice shapes diff size and future Gitea/GitLab support.
5. **Sandbox contract update.** CLAUDE.md documents clone-based providers as GitHub-only with specific branch/SHA semantics. Extending `CloneDecision`/`CloneSource` to Forgejo changes a documented contract; alternatively Forgejo runs could be forced onto `skip_clone`/Local initially.
6. **Webhook semantics.** Given GitHub webhooks don't trigger anything today, should Forgejo's be receive-and-verify parity only, or the first real event-driven automation? Also: which signature header(s) and event-name mapping to accept (Forgejo can emit GitHub-compatible webhooks when configured — do we rely on that or support native headers?).
7. **Wire/API compatibility.** Should existing API types (`PullRequestLink`, `PullRequestGithubDetail`, `GitHubRepositorySlug` in the OpenAPI spec) grow a forge/host discriminator (breaking-ish, needs `with_replacement` care per the type-ownership rules), or should Forgejo get distinct DTOs?
8. **Token/secret plumbing.** New env vars (`FORGEJO_TOKEN`, `FORGEJO_URL`?) vs. reusing `GITHUB_TOKEN`-style naming; secret-registry additions; per-instance credential storage for multi-instance.

I have not written or modified anything — this stage is exploration only.