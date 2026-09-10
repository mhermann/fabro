Investigation complete. Here's the full sweep, the bucket sort, the stress tests, and the final question set.

# Dimension-by-dimension sweep

**1. Scope boundaries.** "Similar to the GitHub integration" maps to seven separable surfaces in this repo: (i) clone/credential resolution, (ii) run-branch/meta-branch push, (iii) auto-PR pipeline (create/find/merge/close/auto-merge), (iv) server-side run targeting (`GitRunTarget` → `https://github.com/{owner}/{repo}` is derived in `run_intent.rs`; Forgejo repos are currently *inexpressible* as run targets), (v) Docker/Daytona sandbox cloning (explicitly rejects non-GitHub origins in `clone_source.rs`), (vi) OAuth browser login (`AuthMethod::Github`, `/login/github`), (vii) webhooks (verified: today's `github_webhook` handler **only verifies HMAC and logs** — it triggers nothing, so "webhook parity" is near-vacuous), plus an eighth: Projects V2 tracker (GraphQL — Forgejo has no equivalent). The line between tiers is genuinely unclear and produces materially different software.

**2. Users and callers.** CLI users (`fabro run` in a clone, `fabro install`, `fabro doctor`), server API consumers (runs, automations, PR supervision in `server/handler/pull_requests.rs`), the web SPA (repo picker uses `GET /api/v1/repos/github/{owner}/{name}`, PR chips render `PullRequestLink`). Public wire surfaces that would change: OpenAPI spec + generated Rust/TS clients (run target shape, `PullRequestLink` validation, `IntegrationProvider` enum, `AuthMethod` enum), `settings.toml` schema. Existing GitHub behavior must not change; everything can be additive.

**3. Data and state.** `PullRequestLink` hard-rejects non-`github.com` hosts on **deserialize of stored/wire data** (`pull_request.rs:142`) — storing a Forgejo PR requires widening this (additive; old data stays valid). Run summaries/PR rows live in SQLite (recent commits moved PR recovery there); no schema break needed, links are strings. No migration implied (migrations-strategy doc: only for old-shape rewrites; new additive settings get normal config resolution). Rollback = removing an additive settings table; benign.

**4. Existing behavior.** Verified what happens *today* for a Forgejo user: local `fabro run` inside a Forgejo clone — clone works (local sandbox uses cwd via `detect_repo_info`), run-branch push silently uses no injected creds (git_bridge credential helper is host-scoped to `https://github.com`), meta-branch push silently skips (`run_metadata.rs:302` returns `None` on non-github origin), auto-PR **errors** if enabled (`parse_github_owner_repo` fails). Docker/Daytona sandboxes fail explicitly. Server-side runs: impossible (slug → github.com URL). The goal extends this; it does not replace GitHub.

**5. Edge and failure cases.** Self-hosted instances behind internal CA / self-signed TLS (common; `fabro-http` uses reqwest system roots, only a `danger_accept_invalid_certs` test knob exists); instances on subpaths; token with insufficient scopes (Forgejo PATs are scope-limited, minted manually — no per-run minting exists); expired/rotated tokens; rate limits per instance; empty body/missing signature webhooks. The goal says nothing about these; none are expensive to guess (standard error surfacing + docs), except TLS-for-internal-CA which I log as a follow-up.

**6. Non-functional constraints.** No stated perf/latency budgets in the repo for integrations. No installation-token mint latency issue (Forgejo has none — PATs are static). Nothing to violate.

**7. Security, privacy, permissions.** New secret `FORGEJO_TOKEN` must join `EnvVars`, `secret_registry.rs`, vault (`SecretType::Token`), redaction. Verified Forgejo fact that matters: **instance-wide OAuth2 apps confer administrative rights (scopes not implemented)** — user-registered OAuth2 apps are the safe path if login is in scope; this shapes tier (c) design. Static PAT with repo-write is a bigger blast radius than GitHub installation tokens — a documented limitation, not solvable upstream. Webhook signature header differs (`X-Forgejo-Signature`, bare hex) but the endpoint is a no-op anyway. No widening of who can do what on GitHub paths.

**8. Compatibility and versioning.** Additive settings (`#[serde(default)]` on a new `ServerIntegrationsSettings` field), additive OpenAPI fields (`provider` discriminator defaulting to `github`), TS client regen in-repo (lockstep, no external consumers known). `IntegrationProvider` and `AuthMethod` enum extensions are additive. No deprecation of anything GitHub. Staged rollout isn't this repo's pattern — integrations are settings-gated (`enabled = false` default, matching GitHub's `Default`).

**9. Testing and verification.** The repo's pattern is directly reusable: `fabro-github` uses a mock `HttpClient` trait (`tests_mock.rs`) for unit tests and `tests/live_access.rs` behind the e2e profile; server changes are covered by `server/tests.rs` + OpenAPI conformance; Forgejo is trivially self-hostable, so an httpmock-based suite plus an optional live e2e (env-gated, e.g. against codeberg.org or a CI-spawned container) fits without a new test kind.

**10. Operational surface.** Required by convention: `docs/public/integrations/forgejo.mdx` + `docs.json` nav entry, dated changelog file, integration status via `IntegrationProvider` (system handler builds the list), `fabro doctor` check mirroring GitHub's five-field check, logging per logging-strategy. Install wizard: GitHub's guided flow exists because App manifest registration is complex; Forgejo setup is URL+token, so manual settings + doctor suffices for v1 (decided below).

**11. Dependencies and integration.** No new Rust crates (HTTP, JSON, URL handling all present; no JWT needed — Forgejo has no app auth). New env vars/vault secrets `FORGEJO_URL`-shaped config; external dependency is the operator's Forgejo instance itself. Codeberg is just `url = "https://codeberg.org"`.

**12. Definition of done.** Rejection scenarios: "I wanted my team's self-hosted Forgejo runs from the web UI" while we shipped CLI-only clone+PR; "we have three instances" while we shipped one; "users sign in with Codeberg accounts" while we shipped no login. These are exactly the vetted questions below. Rejected-as-not-rejection: gitea branding, wizard flow, webhook receipt.

# Bucket sort

**Answerable from the repository** (answers found, not asked):
- Webhook handling is verify-and-log only → Forgejo webhook "support" has no functional payload to match; skip without guilt.
- The codebase's integration convention is parallel side-by-side surfaces (`fabro-github` + `fabro-slack`, shared `ServerIntegrationsSettings`, `IntegrationProvider` enum, per-integration docs page) — not a shared-trait-first refactor. `Tracker`/`Sandbox` traits exist only where multiple impls share a runtime interface.
- The settings redesign brainstorm (R64) already specified provider-neutral `[run.scm]` with provider-specific nested tables — coexistence was the designed intent.
- `PullRequestLink` and `GitRunTarget` constraints, local-run behavior for non-GitHub origins, sandbox origin rejection, defaults (`GithubIntegrationSettings.enabled` defaults false), changelog/docs conventions — all verified above.
- Forgejo facts (verified against forgejo.org docs): OAuth2 authorization-code provider exists; no GitHub-App/installation-token equivalent; PATs are manually scoped; webhooks send `X-GitHub-Event` + `X-Forgejo-Signature`; REST API is Gitea-shaped (`/api/v1/...`, `Authorization: token`).

**Yours to decide** (decided — see final section): crate layout, settings shape, env/vault naming, PAT-only auth base, webhooks/tracker exclusion, wizard exclusion, gitea positioning, testing approach, TLS scope, draft-PR/auto-merge semantics, sandbox token env var name.

**Genuinely theirs:** two questions survive — scope tier, and instance/addressing model.

# Stress tests

- **Regret check.** The rejection I'd most fear: shipped CLI-only clone+PR when the human runs Fabro as a server for a team on self-hosted Forgejo → that's Q1. Second: schema built for one instance when they run several → that's Q2. Both added; nothing else survives this check (architecture regret is asymmetric — a parallel crate can be refactored later; a premature refactor destabilizes GitHub for nothing).
- **Second-round check.** The follow-up I'd otherwise need: "wait, does that include browser login / web repo pickers?" → Q1's tiers now enumerate those explicitly. "Can GitHub and Forgejo coexist on one server?" → Q2's options make coexistence explicit. No predictable second round remains.
- **Merge check.** Auth-model, OAuth-login, sandbox-clone, and automations questions all merged into Q1's tiers (they travel together). Instance-count and coexistence merged into Q2. Eight candidate questions collapsed to two.
- **Self-answer check.** "Parallel crate vs. abstraction refactor" — answered from repo precedent (parallel; decided). "Draft/auto-merge semantics" — implementation-level, self-answered. "Which surfaces does the wizard touch" — self-answered (none). Removed from the set.
- **Consequence check.** Q1: tier (a) touches a new crate + PR pipeline + `PullRequestLink`; tier (b) adds the run-addressing wire change, server checkout, automations, sandbox clone; tier (c) adds the auth surface. Distinct builds. Q2: option (a) is an additive `provider` tag; (b) is a global switch with no tag; (c) is a named-instances schema. Distinct schemas, distinct retrofit cost. Both pass.

## Vetted questions

**Q1 — Which tier of GitHub parity should Forgejo v1 ship?**
- **(a) Local/CLI only** — `fabro run` inside a Forgejo clone gets credential-injected push, meta-branch push, and auto-PR via the Forgejo API. *Cost:* new `fabro-forgejo` crate + workflow PR/push pipeline branch + widening `PullRequestLink`; no server/API/OpenAPI changes; server-side and web remain GitHub-only.
- **(b) (a) + server-side** — run targets can name Forgejo repos (API, automations, server checkout, Docker/Daytona clone support, web repo picker, integration status, doctor). *Cost:* everything in (a) plus the run-target addressing change across OpenAPI → Rust/TS codegen, `run_intent`, `git_checkout`, `clone_source`, and the repo-lookup endpoint — the largest surface.
- **(c) (b) + identity** — OAuth2 browser sign-in via Forgejo (`AuthMethod::Forgejo`, web login flow, user-registered OAuth2 app; note: instance-wide Forgejo OAuth2 apps are admin-scoped, so user-registered apps are required). *Cost:* everything in (b) plus the auth surface, secrets, and web login UI.
- *What changes:* under (a) the server crate is untouched; under (b) the run-target wire format and sandbox clone contract change; under (c) the auth model changes too. **This is the difference between a focused crate and a cross-cutting platform change.**

**Q2 — How are Forgejo repositories identified across the system?**
- **(a) One configured instance + provider tag** — `[server.integrations.forgejo] url = "…"`, and run targets gain an optional `provider: "github" | "forgejo"` discriminator (default `github`, wire-additive). GitHub and Forgejo coexist on one server. *Cost:* small additive wire change; one token in the vault.
- **(b) Exclusive SCM switch** — a global `[run.scm] provider` choice; a server is either GitHub or Forgejo, never both. *Cost:* simplest addressing (no per-target tag) but blocks mixed use and is awkward to walk back.
- **(c) Multiple named instances** — `[[…forgejo.instances]]`, repo refs carry the instance name. *Cost:* heaviest settings + wire schema, per-instance credentials, name-resolution logic; only worth it if operators genuinely run several forges.
- *What changes:* (a)/(b)/(c) produce different settings schemas, different API shapes, and different retrofit migrations if chosen wrong; the repo's own settings brainstorm (R64) designed for (a)-style coexistence.

## Decided without asking

- **Parallel `fabro-forgejo` crate in `lib/components/`, not a `fabro-scm` trait refactor** — matches how `fabro-slack` sits beside `fabro-github`; a premature abstraction would destabilize the GitHub path against the "minimal, focused changes" rule; seams can be extracted later where duplication is real (URL parsing helpers).
- **Auth base is a scoped PAT** (`Authorization: token …`) stored as `FORGEJO_TOKEN` in the vault — Forgejo has no App/installation-token equivalent, so GitHub's App strategy has no counterpart; OAuth2 exists but is only relevant if Q1 = (c).
- **No webhook surface in v1** — the existing GitHub webhook handler only verifies HMAC and logs; there is no behavior to mirror, and Forgejo's signature header (`X-Forgejo-Signature`, bare hex) differs anyway.
- **No Projects tracker, no auto-merge assumption** — Forgejo has no Projects V2 GraphQL; auto-merge/draft semantics get verified against the real API during implementation (Gitea-lineage PRs use WIP-prefix rather than a draft flag).
- **No `fabro install` wizard changes** — GitHub's guided flow exists because App manifest registration is complex; Forgejo is URL + token, so manual settings + a `fabro doctor` check + docs page is enough, and avoids churning the wizard for non-Forgejo users.
- **`forgejo` naming throughout; Gitea likely works but is not claimed** — the API is Gitea-shaped so compat is nearly free later; naming nothing "gitea" now avoids dual-branding docs/tests.
- **Codeberg is just `url = "https://codeberg.org"`** — no special-casing; one config shape covers public and self-hosted.
- **`enabled = false` default** for the new integration, mirroring `GithubIntegrationSettings`.
- **Sandbox token injected as `FORGEJO_TOKEN`** (honest naming over `GITHUB_TOKEN` compatibility), documented in the run-integrations settings.
- **Testing mirrors `fabro-github`: mock `HttpClient` unit tests + optional live e2e behind the e2e profile** (self-hosted container or codeberg token via env); OpenAPI conformance covers wire changes; no new test kind.
- **TLS uses the system trust store in v1** — internal-CA bundles flagged as a documented follow-up (reqwest already exposes the knobs if needed); self-signed certs are not silently accepted.
- **No data migrations** ��� all changes are additive settings/fields; `PullRequestLink` widening keeps old github.com links valid.