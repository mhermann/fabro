Exploration complete. Here is my understanding.

## 1. Repository structure and where the change lands

**Fabro** is an AI workflow orchestration platform: agents run as stages of Graphviz-defined workflows, executing inside clone-based sandboxes (Docker default, Daytona, Local) against git repositories, pushing branches/checkpoints and opening PRs.

- **Language/tooling**: Rust workspace under `lib/` (cargo + nextest, pinned nightly fmt/clippy, insta snapshots), React 19 web UI in `apps/fabro-web` (Bun), OpenAPI-first HTTP contract (`docs/public/api-reference/fabro-api.yaml` → progenitor Rust types + generated TS client), docs as MDX in `docs/public/`.
- **Layout**: `lib/foundation/` (fabro-types, fabro-config, fabro-static, fabro-http, fabro-vault, fabro-oauth…), `lib/components/` (fabro-github, fabro-workflow, fabro-sandbox, fabro-tracker, fabro-slack…), `lib/apps/` (fabro-cli, fabro-server).

The GitHub integration is **not one module — it's a vertical slice through ~10,000+ lines across six crates**:

| Layer | Location | Role |
|---|---|---|
| API client crate | `lib/components/fabro-github/` (~4,400 LOC) | GitHub App JWT + installation tokens, PAT, PR CRUD (`create/find/get/merge/close_pull_request`, `enable_auto_merge`, `branch_head_sha`), app webhook config, URL parsing (`parse_github_owner_repo` hardcodes `https://github.com/`), git credential helper, `resolve_clone_credentials`, cached token source with provenance |
| Server | `lib/apps/fabro-server/` | Webhook route `/api/v1/webhooks/github` (HMAC `X-Hub-Signature-256`), OAuth login (`ServerAuthMethod::Github`, `auth/github_endpoints.rs`), install flow (`install.rs` + `fabro-install`: App manifest, vault keys), `git_checkout.rs`, automations (`automation_materializer.rs`), integration status |
| Workflow engine | `lib/components/fabro-workflow/` | Auto-PR pipeline (`pipeline/pull_request.rs`, 2,046 LOC), publish, `git_bridge.rs` (injects `GITHUB_TOKEN` + git credential-helper env into sandboxes) |
| Sandbox | `lib/components/fabro-sandbox/` | `clone_source.rs`: **explicitly GitHub-only** clone contract ("Clone-based sandboxes currently support GitHub repository origins only"), `push_credentials.rs`, Docker/Daytona transports |
| Types/config | `lib/foundation/fabro-types`, `fabro-config` | `GitHubRepositorySlug`, `RepositoryProvider { Github, Git, Unknown }`, `PullRequestGithubDetail`, settings: server `integrations.github { strategy: token\|app, app_id, slug, webhooks }`, `run.integrations.github { permissions, additional_repositories }`, and `run.scm { provider, owner, repository, github }` |
| CLI + web + docs | `fabro-cli` (`gh.rs`, `shared/github.rs`, install cmd), `apps/fabro-web` (integrations panel, auth, install API), `docs/public/integrations/github.mdx` | Setup UX, credentials from vault/`gh auth token`, UI panels, user docs |

Notably, `run.scm` already exists as a **designed extension point**: `RunScmLayer` carries `provider: Option<String>` with a comment "Provider-specific SCM leaves. First-pass providers", and `ScmGitHubLayer` is "Intentionally minimal in the first pass." The codebase anticipated non-GitHub SCMs. There is **zero existing Forgejo/Gitea code** (one incidental mention in a design doc).

## 2. Goal restated concretely

Add Forgejo (self-hostable Gitea fork) as a second first-class SCM/forge provider alongside GitHub, following the same vertical shape: a client crate for Forgejo's API (token auth, PR create/get/merge/close, clone credentials), server settings + install path, webhook ingestion, sandbox cloning/pushing from Forgejo origins, auto-PR from workflows, `run.scm.provider = "forgejo"` config, `RepositoryProvider::Forgejo`, integration status surface, web UI panel, OpenAPI updates, and docs — parameterized by an instance base URL since Forgejo has no fixed host.

## 3. What I know for certain

- The GitHub slice is deep: credentials → token minting/caching → git bridge env → sandbox clone/push → PR pipeline → webhooks → OAuth → install → UI/docs, with a test harness (mock `HttpClient` trait, httpmock-based PR pipeline tests, twin/e2e patterns).
- `fabro-github` already parameterizes the **API base URL** (`GITHUB_BASE_URL` env, `GitHubContext`), but URL parsing, origin detection (`is_github_origin`), the credential helper host (`credential.https://github.com.helper`), and clone-source validation are hardcoded to github.com.
- Clone-based sandboxes (Docker/Daytona) reject non-GitHub origins today; `GitHubRepoLayout` builds owner/repo paths and symlinks from the origin.
- `run.scm` with `provider` + per-provider leaves, and `RepositoryRef.provider` enum `[github, git, unknown]`, are deliberate seams for this exact change.
- Server settings support two GitHub strategies: `token` (PAT via vault/`gh auth token`) and `app` (GitHub App + installation tokens + OAuth + webhooks).
- Env-var registry (`fabro-static`), vault secret types, redaction (`DisplaySafeUrl`), shell quoting, strum conventions, OpenAPI conformance tests, and strategy docs (events, migrations, secrets, error-handling) all constrain how new integration code must be written.
- Forgejo exposes a GitHub-*ish* REST API under `/api/v1` but has **no GitHub Apps**: auth is PAT/scoped tokens and OAuth2 apps; webhooks use `X-Gitea-Event`/`X-Gub-Signature`-family headers (HMAC-SHA256 with a per-hook secret); web PR URLs use `/pulls/{n}` not `/pull/{n}`.

## 4. Genuinely ambiguous (two reasonable engineers would build differently)

1. **Auth model parity**: GitHub has `token` and `app` strategies. Forgejo has no App/installation-token equivalent. PAT-only? PAT + OAuth2-app browser login? Is browser sign-in (currently GitHub-OAuth-gated) in scope at all?
2. **One instance or many**: Forgejo is self-hosted. Does Fabro support a single configured instance (env/settings like `FORGEJO_URL` + `FORGEJO_TOKEN`, mirroring `GITHUB_*`), or arbitrary per-run origins detected by URL host? This ripples through credential lookup, `additional_repositories` slugs (no host component today), and the credential-helper host scoping.
3. **Crate architecture**: new `fabro-forgejo` crate mirroring `fabro-github` vs. generalizing `fabro-github` into a provider-abstracted forge crate (the existing `GITHUB_BASE_URL` override shows partial precedent). The API shapes are close but not identical (e.g., merge endpoint payload differs), so "reuse with base URL" vs "separate DTOs" is a real fork.
4. **Surface scope for v1**: minimum viable = clone/push/auto-PR? Or also webhooks→automations, repo-access check endpoint (`/api/v1/repos/forgejo/...`), sandbox token injection (`FORGEJO_TOKEN` vs reusing `GITHUB_TOKEN` since Forgejo tooling reads it), integration status, tracker (Forgejo has no Projects V2 GraphQL)?
5. **Webhook ingestion**: separate route `/api/v1/webhooks/forgejo` with Forgejo's header/signature scheme, and which events feed the automation scheduler?
6. **Config naming**: `run.scm.forgejo` leaf (empty like GitHub's?), `integrations.forgejo` server settings shape, `RepositoryProvider::Forgejo` vs reclassifying as generic `Git` with a forge type, whether `gitea` is an alias.
7. **PR link handling**: existing validation only accepts `https://github.com/owner/repo/pull/N`; Forgejo uses instance-host URLs and `/pulls/N` — new validation, and does `PullRequestGithubDetail` get a Forgejo sibling or a provider-neutral rename (OpenAPI/type-ownership rules apply)?
8. **Install UX**: `fabro install` flow for Forgejo — PAT prompt only, or OAuth2 app creation helper? Affects web install API endpoints and the install-flow UI.

No files were modified; this stage was exploration only.