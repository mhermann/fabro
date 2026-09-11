I've now read the remaining integration surfaces (automation triggers, install flow, OAuth login, config defaults, API wire format, git bridge, PR pipeline). Here is the full sweep.

## The sweep, dimension by dimension

### 1. Scope boundaries
"Similar to github integration" maps to a ten-surface slice: credentials, clone/auth, PR lifecycle (create/get/merge/close/auto-merge), webhooks, sandbox token minting (`run.integrations.github` → `GITHUB_TOKEN`), OAuth web login (`server.auth`), install flow (CLI + web wizard), tracker (Projects V2), diagnostics/doctor, docs. Nothing in the goal says which of these are in. The two extreme readings — "a `fabro-forgejo` crate with REST helpers" vs "everything GitHub has, for Forgejo" — differ by roughly an order of magnitude in work. One surface is *impossible* to mirror: GitHub Projects V2 tracking is GraphQL, and Forgejo has no GraphQL API, so the tracker has no analogous feature. **Unclear and material → question.**

### 2. Users and callers
Callers: CLI users (`fabro install`, `fabro run`), server operators (settings.toml, env, docker-compose), web users (install wizard, PR chips, automations), and external API consumers via the generated Rust/TS clients. Public wire formats that are GitHub-specific: `GitRunTarget.repo` is documented as "GitHub repository slug in `owner/name` form" (spec literally says "Public github.com repository target"); `PullRequestLink` persists `owner/repo/number` and recomputes `html_url` as `https://github.com/...`; `additionalProperties: false` + `deny_unknown_fields` throughout means shape changes are contract changes, not silent additions. Existing GitHub behavior must not regress — there's a conformance test that catches spec/router drift. **The wire-format question is real → question.**

### 3. Data and state
Persisted state touched: run manifests (legacy lane carries full `git.origin_url` — a Forgejo URL could round-trip *there* today if parsing allowed it), `PullRequestLink` on runs (⚠️ reuses the GitHub type → would silently rewrite Forgejo PR URLs to github.com — a data-corruption trap if naively shared), stored automation TOMLs embedding `RunTarget`, settings.toml, vault secrets. New settings keys are additive (serde defaults, no migration). A host-qualified repo identity would need compat migration handling per `docs/internal/migrations-strategy.md`. Rollback is easy for additive config, hard once persisted runs carry forge-qualified data. **Feeds the identity question.**

### 4. Existing behavior
Verified, not assumed: webhooks today **verify HMAC and log only** — no run triggering anywhere; automations fire on cron/API triggers only. `GITHUB_BASE_URL` env override exists but **does not enable GHE/Forgejo**: `parse_github_owner_repo` and every clone path still pin `github.com`, and `embed_token_in_url` bails on non-HTTPS. So Forgejo cannot ride the existing override — a real integration is required. The sandbox clone contract is documented (CLAUDE.md + error strings) as GitHub-only. The goal is *extend*, not change, existing GitHub behavior. **Answered from repo.**

### 5. Edge and failure cases
Where the goal is silent and guessing wrong is expensive: multiple Forgejo instances with colliding `owner/repo` names (unresolvable without host-qualified identity); subpath-hosted instances (`https://example.com/forgejo/owner/repo` — parsing ambiguity); plain-HTTP internal instances (token transport over cleartext; current `embed_token_in_url` is HTTPS-only by design); Forgejo PAT scope grammar (`read:repository`/`write:repository`) vs GitHub permission maps (`contents: read`); webhook header variants (`X-Forgejo-Signature`/`X-Gitea-Signature` vs GitHub-compat `X-Hub-Signature-256`); auto-merge has no GitHub-identical endpoint. Most of these resolve once the identity-model and scope questions are answered; the rest are implementation details with safe defaults. **Feeds questions; no standalone ask.**

### 6. Non-functional constraints
No stated perf/latency expectations that a second REST integration would violate; HTTP client reuse patterns already exist. One genuine non-functional widening: multi-instance with user-supplied URLs turns the server into a fetcher of arbitrary hosts (SSRF surface) — today the GitHub-only rule doubles as a one-host allowlist. **Feeds the identity question's cost column.**

### 7. Security, privacy, permissions
Widens data flow: operator tokens sent to the configured instance, clones from non-github.com hosts. Requires: new secrets in `fabro-static` env_vars + secret_registry (scrubbing), per `docs/internal/server-secrets-strategy.md`; webhook HMAC verification per instance secret; sandbox token scope mapping if minting is in scope. Token never in argv/config/git-config (the credential-helper pattern must be replicated). **Process obligations, not questions.**

### 8. Compatibility and versioning
Additive settings/env are safe. Wire changes to `GitRunTarget` are the one compatibility decision (old clients ↔ new server is fine for optional fields; persisted automations and the generated TS client churn). The integrations-settings pattern (`enabled` flag, like github/slack) gives the feature-flag mechanism — Forgejo should default off. **Feeds identity question.**

### 9. Testing and verification
Existing tooling covers this well: `MockHttpClient` pattern in `fabro-github` ports directly; live tests are env-gated (`.env.example`, `--profile e2e`); API changes are caught by conformance tests; CLI snapshots via insta. New need: a live Forgejo instance for E2E (Docker-spinable; Forgejo publishes an image). No "twin" harness exists for forges and building one is out of proportion — mock-client tests + optional live tests suffice. **Answered; no new test *kind* implied.**

### 10. Operational surface
Repo conventions require: tracing calls per logging strategy, diagnostics parity (`fabro doctor`/install verification for the new instance), Mintlify docs page, changelog entry. These are obligations I can just discharge. **Not a question.**

### 11. Dependencies and integration
No new crates needed (reqwest, hmac, sha2, serde all present). New external dependency: a Forgejo instance (config + test infra). Forgejo implements GitHub-style OAuth paths (`/login/oauth/authorize`, `/login/oauth/access_token`), which matters only if login parity is in scope. **Feeds scope/auth questions.**

### 12. Definition of done
The rejection scenario: "I pointed Fabro at my company's Forgejo and ran a workflow with a PR — nothing worked / you only built a library / you broke my GitHub setup / you can't have two instances and I need two." Each of those traces to Q1 (scope menu), Q2 (identity), and the additive-guardrail question. A secondary scenario — "why does the Forgejo webhook do nothing?" — traces to the webhook-behavior question, since GitHub's own webhook is receive-only.

## Bucketing

**Answerable from the repository** (answers found, stated above): `GITHUB_BASE_URL` does not already solve this; webhooks are inert by design; automations are cron/API-triggered; `GitRunTarget`/`PullRequestLink`/OAuth are GitHub-pinned in wire and docs; no provider trait exists for the git-host surface (~100 direct `fabro_github::` call sites across 7 crates); merge strategies map 1:1; settings/env/secret registries are additive; testing patterns port cleanly.

**Yours to decide** (decided, see below): crate placement and naming; REST-v1/Gitea-lineage API targeting; auth header form; webhook header acceptance set; HTTPS-only instances; tracker exclusion (no Forgejo equivalent exists — nothing to be "similar" to); test strategy; docs/changelog; env-var naming.

**Genuinely theirs**: scope menu, instance/identity model, credential mechanism beyond PAT, additive-vs-refactor architecture, webhook behavior beyond parity. Five candidates.

## Stress tests

- **Regret check**: the strongest rejection modes are "wrong size" (→ Q1), "wrong identity model, persisted data now wrong" (→ Q2), "you destabilized GitHub" (→ architecture Q), "we needed login/apps not just PATs" (→ auth Q). One I initially missed and folded in: "the webhook does nothing?!" — GitHub's own webhook is inert, so "similar" is ambiguous here in a way that costs a second round.
- **Second-round check**: things that would send me back: "which Forgejo version?" (folded into auth Q — Forgejo Apps are v10+); "does full parity include the Projects tracker?" (folded into Q1 as an explicit N/A); "what about codeberg.org?" (just an instance — covered by Q2); "Gitea too?" (folded into decided: wire-compatible, not advertised). No remaining second-round triggers I can name.
- **Merge check**: instance-count, wire-format evolution, and run-admission lane were three candidate questions with one consequence class — merged into a single identity-model question. Scope-menu + tracker-N/A merged into Q1.
- **Self-answer check**: removed "PAT vs Bearer header" (canonical form, repo has precedent), "tracker in scope" (impossible as parity), "manifest lane URL acceptance" (accept configured instance's URLs — detail), "feature flag" (settings pattern exists).
- **Consequence check**: every remaining question below states what changes between answers; none is cosmetic.

## Vetted questions

1. **Which surfaces must the Forgejo integration include?**
   - **A. Core git+PR**: credentials, clone into Docker/Daytona sandboxes, PR create/get/merge/close (API + web), settings. — Smallest; unblocks the main workflow story.
   - **B. A + sandbox token minting (`run.integrations.forgejo` → `FORGEJO_TOKEN`), webhook route (receive+verify), install flow (CLI + web), diagnostics.** — "Similar to GitHub" in most senses; roughly 2× A.
   - **C. B + OAuth2 web login (`server.auth`) + automation-target parity.** — Full parity; adds a browser auth flow and its security surface.
   - *What I'd build differently*: A is a crate + clone-path work; B adds config/install/webhook/token plumbing; C adds an entire login provider. Tracker is excluded at every tier (Forgejo has no GraphQL API — nothing to mirror).

2. **One configured Forgejo instance per server, or host-qualified multi-instance identity?**
   - **A. Single instance**: `[server.integrations.forgejo] url` + token; `owner/repo` slugs reused, scoped to that instance; no wire-format change. — Small, covers the one-company-forge case; can't express two forges.
   - **B. Multi-instance**: repo identity becomes host-qualified; `GitRunTarget`/`RunTarget` wire change, persisted automations/PR-link migration, generated-client churn, new SSRF surface to police. — A platform change; expensive to undo once data is persisted.
   - *What I'd build differently*: A keeps a Forgejo slug type + settings-level dispatch; B introduces a forge-qualified `RepositoryId` rippling through run intent, manifests, PR links, and the store.

3. **Beyond a personal access token, which credential mechanisms?**
   - **A. PAT-only** (`Authorization: token …`). — One credential variant; universally supported.
   - **B. PAT + OAuth2 app** (Forgejo implements GitHub-style `/login/oauth/*`). — Required if Q1=C; adds client-id/secret config and browser flow.
   - **C. B + Forgejo Apps scoped tokens** (v10+; the closest analog to GitHub Apps). — Newest API; version-gates the feature.
   - *What I'd build differently*: A is one enum variant; B adds a full login flow; C adds app-registration and scoped-token machinery pinned to Forgejo ≥ v10.

4. **Strictly additive parallel implementation, or refactor GitHub behind a shared forge abstraction?**
   - **A. Additive**: new `fabro-forgejo` crate + explicit dispatch at boundaries; GitHub code untouched. — Zero GitHub regression risk; some duplicated shape.
   - **B. Shared `Forge` trait**: refactor `fabro-github` onto it, both providers implement. — Cleaner, eases future Gitea/GitLab, matches the repo's type-unification guidance; touches ~100 call sites across 7 crates with regression risk.
   - *What I'd build differently*: A lands faster with a wide-but-shallow diff; B is a deeper refactor review with a trait at the center. Behavior is identical either way.

5. **If webhooks are in scope: inert parity or event-driven?**
   - **A. Parity**: verify HMAC + log, exactly like the GitHub route today. — Consistent; honest about current capability.
   - **B. Forgejo webhooks trigger automations/runs on push/PR events.** — A genuinely new capability GitHub's own integration lacks; new security and idempotency surface.
   - *What I'd build differently*: A is a route + verifier; B is an event-processing pipeline with replay protection.

## Decided without asking

- **New sibling crate `lib/components/fabro-forgejo`**, mirroring `fabro-github`'s structure (context/credentials/PR ops/URL parsing, `MockHttpClient` tests) — matches repo layout convention; nothing is forced into `fabro-github`.
- **Target the Gitea-lineage REST v1 API of current stable Forgejo**; wire-compatible with Gitea but advertised as Forgejo only — the goal names Forgejo.
- **`Authorization: token <PAT>`** canonical header form — the documented Forgejo convention; Bearer acceptance is version-dependent.
- **HTTPS-only instance URLs**, rejecting plain-HTTP with a clear error — consistent with `embed_token_in_url`'s existing HTTPS requirement and avoids cleartext token transport.
- **Accept all three webhook signature/header spellings** (`X-Forgejo-*`, `X-Gitea-*`, GitHub-compat `X-Hub-Signature-256`/`X-GitHub-Event`), all HMAC-SHA256 — cheap, and Forgejo instances are configured in any of these modes.
- **`MergeStrategy` maps directly** (merge/squash/rebase all exist in Forgejo); auto-merge endpoint shape verified against the real API during implementation rather than assumed GitHub-identical.
- **Tracker excluded** — Forgejo has no GraphQL/Projects V2; there is nothing to be "similar" to.
- **Naming**: `FORGEJO_URL`, `FORGEJO_TOKEN` env vars; `[server.integrations.forgejo]` with `enabled` defaulting off (github/slack settings pattern); sandbox var `FORGEJO_TOKEN` mirroring `GITHUB_TOKEN`. All secrets registered in `fabro-static` env_vars + secret_registry per the server-secrets strategy.
- **Testing**: port the `MockHttpClient` pattern for unit tests; live tests env-gated via `FABRO_TEST_FORGEJO_*` under the existing e2e profile; API-spec changes go through the OpenAPI-first workflow with conformance tests.
- **Docs + changelog entry** included per repo convention; relevant `docs/internal/*-strategy.md` files re-read at implementation time.
- **GitHub behavior, wire formats, and persisted data remain untouched in all branches** — the safe default whichever way Q4 is answered.