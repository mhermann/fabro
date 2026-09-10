The sweep is complete. Here is the full analysis.

---

# Dimension-by-dimension sweep

## 1. Scope boundaries

**Clearly in:** a way to configure a Forgejo instance + credentials; a Rust client for its API; enough of the run path to be end-to-end useful. **Clearly out:** unrelated integrations; the Projects V2 tracker (verified: `fabro-tracker` uses GitHub GraphQL — Forgejo has no GraphQL API at all, so "tracker parity" is not even possible).

**The genuinely unclear line** — and the two readings produce software differing in size by 3–5×:
- (a) *Run-path core*: clone-from-Forgejo in sandboxes, run-driven PR create/merge/close/link/view, PAT credentials in settings/vault, docs + changelog. (Slack precedent verified: it landed as settings + crate + status endpoint + docs, **no install wizard** — this is an accepted product shape in this repo.)
- (b) *+ product surface*: install wizard (browser routes in `install.rs` + `fabro install --forgejo-*`), system-integrations status, diagnostics, doctor, `repo init`.
- (c) *+ full parity*: Forgejo as OAuth login IdP (`server.auth`), inbound webhooks `/api/v1/webhooks/forgejo`, automations/server-submitted run targets on Forgejo repos (`GitRunTarget` extension).

## 2. Users and callers

CLI users (`install`, `run`, `pr *`, `repo init`, `doctor`), the web SPA, and generated API clients. **Hard constraint, answerable from the repo:** existing GitHub behavior must not change, and wire changes must be additive-optional (`ServerIntegrationsSettings` currently has `required: [github, slack]`, so a `forgejo` key must be optional to stay non-breaking). OpenAPI is regenerated + conformance-tested per convention. No question — constraints, not choices.

## 3. Data and state

Verified persisted GitHub-shaped data: `PullRequestLink` in `runs.summary_json` and `pull_request.*` events (deserializer accepts bare `{owner,repo,number}` but renders/validates `html_url` as github.com — a Forgejo link stored naively would render a **wrong github.com URL**); `GitRunTarget.repo` slugs in the automations table; `RepositoryProvider` in run summaries. **Answerable:** events are append-only JSON, so new fields must be `Option`/defaulted; no SQL migration, no settings rewrite needed (per `migrations-strategy.md`: additive-optional fields need no migration). The concrete host-field shape is engineering, not a human question.

## 4. Existing behavior

Verified current behavior: a Forgejo origin fails at `clone_source.rs` ("Clone-based sandboxes currently support GitHub repository origins only") and at PR parsing ("Not a GitHub HTTPS URL"). Goal is *extend*, not change. `GITHUB_BASE_URL` env exists but is a GitHub-style override, not a forge abstraction (Forgejo's API differs in base path `/api/v1`, auth header, and has no `/graphql`) — keeping it GitHub-only is correct. `git log --all` confirms **zero prior Forgejo/Gitea work**. Answerable.

## 5. Edge and failure cases

Verified API divergences the goal is silent on — all engineering decisions (bucket: mine): auto-merge uses GitHub **GraphQL** + `node_id` (`enablePullRequestAutoMerge`) — Forgejo has no GraphQL and no `node_id`; draft is a `WIP:`/`[WIP]` **title prefix** in Gitea-lineage APIs, not a boolean; merge methods are `merge|rebase|rebase-merge|squash|fast-forward-only`, admin-configurable per instance (error mapping differs from GitHub's 405/409); `find_open_pull_request` relies on GitHub's `head=owner:branch` filter; Forgejo tokens (`sha256_…`/40-hex, non-expiring, category-scoped not per-repo) mean `[run.integrations.*].permissions` cannot map 1:1; self-hosted instances raise TLS/private-CA questions. None of these change *what* the human wants — only how it's built.

## 6. Non-functional constraints

Doesn't apply in any expensive way: no perf SLOs on forge calls; repo's real constraints (no-proxy test clients, single-flight token caching, fail-closed worker env allowlist, SPA asset budgets) are conventions I follow, not choices. One line: no question lives here.

## 7. Security, privacy, permissions

New secret (`FORGEJO_TOKEN`) must flow the full documented pipeline (`EnvVars` → `secret_registry` → vault → worker scrubbing) — answerable from `server-secrets-strategy.md`. Token-in-URL embedding reuses the `DisplaySafeUrl` redaction pattern. Webhook HMAC (Forgejo sends `X-Hub-Signature-256`, same scheme) only matters if scope includes webhooks. OAuth login widens auth surface — only if scope says. No personal-data flow beyond GitHub-equivalent.

## 8. Compatibility and versioning

Additive-optional everywhere; no feature flags in this repo (verified Slack precedent: always present, status reflects configuration) — decided, not asked.

## 9. Testing and verification

Answerable: mirror `fabro-github`'s `HttpClient` trait + `MockHttpClient` + integration tests; live tests behind `live("VAR")`-style env gates per `testing-strategy.md`. Sandbox clone semantics get unit tests at `clone_source` plus existing docker test patterns. A containerized-Forgejo e2e is possible but heavy — my call: optional, env-gated.

## 10. Operational surface

Answerable: changelog mdx + `docs/public/integrations/forgejo.mdx` + `IntegrationProvider::Forgejo` status are required by convention; diagnostics/doctor only if the scope answer includes the ops surface.

## 11. Dependencies and integration

No new crates (repo pattern is thin hand-rolled clients on `fabro-http`); new env vars `FORGEJO_URL`/`FORGEJO_TOKEN`; user-provided instance. Mine/answerable.

## 12. Definition of done

The rejection scenarios: **overbuild** (touched web login/wizards unasked), **underbuild** (API client exists but runs still can't clone — unusable end-to-end), **wrong instance model** (single hardcoded instance vs. their multi-instance reality), **wrong forge** (they also use Gitea). The first two are Q1; the third is Q2; the fourth is Q3.

---

# Bucketing

**Answerable from the repository** (answered): where code lands and what it mirrors; Slack shows wizard-less integration is acceptable; additive-only wire changes; no migrations needed; secrets pipeline requirements; test/tooling conventions; docs/changelog required; tracker impossible (no Forgejo GraphQL); API divergences are real (GraphQL auto-merge, `node_id`, draft-as-title, merge methods); no prior Forgejo work; `run.scm.provider` is currently github-only; `GITHUB_BASE_URL` stays GitHub-only.

**Mine to decide** (see `## Decided without asking`): crate/architecture shape, host-field on `PullRequestLink`, PAT-only v1 credentials, auto-merge = fail-loud error on Forgejo, draft = title-prefix mapping, merge-strategy mapping + error texts, pagination fallback, TLS policy, `forgejo` naming, permissions-table non-mapping, live-test gating, GitHub+Forgejo coexistence (per-origin dispatch, additive by construction).

**Genuinely theirs:** three questions survive — capability scope, instance model, Gitea compat.

# Stress-test results

- **Regret check:** the imagined rejection "you built the wrong slice" traces to Q1; "we run three Forgejo servers" traces to Q2; "we also have Gitea" traces to Q3. Added nothing beyond these — architecture/data-shape regrets are bounded by my forward-compatible decisions.
- **Second-round check:** the follow-ups I could predict were web-login/webhooks (merged into Q1's options) and GitHub+Forgejo coexistence (resolved by decision: yes, per-origin, additive). Neither needs a separate question now.
- **Merge check:** Q1 absorbed the auth-model question (each scope option names its credential story), the web-login question, the webhooks question, and the automations/server-targets question. Q2 absorbed the coexistence question.
- **Self-answer check:** tried to self-answer Q3 ("the ask says Forgejo") — rejected, because Forgejo/Gitea conflation is near-universal among self-hosters and the answer changes docs, branding, and the testing matrix. Q1/Q2 resist self-answer because both Slack (wizard-less) and GitHub (full surface) are valid in-repo precedents.
- **Consequence check:** Q1 changes which crates are touched at all (server auth subsystem vs not) — a 3–5× size difference; Q2 changes the config schema shape (single `url` field vs instance map), origin-matching code, and install UX; Q3 changes branding/docs/testing and whether non-Forgejo Gitea hosts are first-class. All three state concrete build differences.

---

## Vetted questions

1. **Which slice of the GitHub integration must Forgejo match in this change?**
   - **(a) Run-path core** — configure instance+PAT in settings/vault; sandbox clone from Forgejo; run-driven PR create/view/link/merge/close; docs + changelog. *Cost:* focused, ~fabro-github-mirror scale; no new UX.
   - **(b) (a) + install/ops surface** — `fabro install` support (flags +/interactive), browser install-wizard routes, system-integrations status wiring, diagnostics, doctor. *Cost:*
 one to two additional crates' worth of UX surface and tests.
   - **(c) Full parity** — (b) plus Forgejo OAuth login for the web UI, inbound webhooks `/api/v1/webhooks/forgejo`, and Forgejo repos as automations/server-submitted run targets. *Cost:* touches the auth subsystem, webhook stack, and `GitRunTarget`/admission model.
   - *What I'd build differently:* (a) never touches `server.auth`, `install.rs`, or `run_intent.rs`; (c) extends all three.

2. **How many Forgejo instances can one Fabro server serve?**
   - **(a) Single configured instance** — `[server.integrations.forgejo] url` + one token; run/PR origins must match that host. *Cost:* simplest schema and matching logic.
   - **(b) Multiple named instances** — map of instance → url+token; origins matched by host. *Cost:* richer config schema, per-instance credential resolution, more install/status UI.
   - (Origin-agnostic "any host with a token" is possible but strictly dominated by (b).)
   - *What I'd build differently:* (a) is one url field + host-equality check; (b) is an instance registry threaded through every credential lookup and status surface. (Either way I'll persist host-aware PR links, so later widening is non-breaking.)

3. **Forgejo only, or Forgejo + Gitea (API-compatible fork) branded as such?**
   - **(a) Forgejo only** — docs, settings, error messages say "Forgejo"; tested against Forgejo. *Cost:* none; Gitea users get no promises.
   - **(b) Forgejo/Gitea** — same code, dual branding, `gitea.com`/CE instances first-class, testing matrix doubles. *Cost:* docs/branding breadth, CI matrix, support expectation.
   - *What I'd build differently:* internals are identical (generic host-based client); (b) changes user-facing naming, validation messages, docs pages, and the live-test matrix.

## Decided without asking

- **Architecture:** new `fabro-forgejo` component crate mirroring `fabro-github` (thin client on `fabro-http`, hand-rolled, `HttpClient` trait for testability); no upfront whole-hog `Forge` trait refactor of `fabro-github` — repo culture prefers minimal seams over speculative abstraction; GitHub paths stay byte-identical.
- **Persisted PR links gain an optional `host`** (absent ⇒ github.com rendering unchanged; tolerant deserialization) — forward-compatible with any scope answer, no migration needed per `migrations-strategy.md`.
- **Credentials: PAT-only v1** (`FORGEJO_TOKEN` vault secret + instance URL) — Forgejo has no GitHub-App/installation-token analog; OAuth2 flow can be added later additively if the scope answer demands install parity.
- **`[run.integrations.forgejo] permissions` is not supported v1** (Forgejo tokens are category-scoped, not per-repo) — the configured token is used as-is; documented.
- **Auto-merge on Forgejo: explicit fail-loud validation error** rather than silent skip — GitHub's GraphQL `enablePullRequestAutoMerge` has no Forgejo equivalent; silent divergence would mislead users.
- **Draft PRs map to `WIP:`/`[WIP]` title prefixes**; **merge strategies** map the three shared methods with clear errors for instance-disallowed ones; **PR search** falls back to client-side head filtering if Forgejo lacks GitHub's `head=` param.
- **TLS via system trust store only** (no insecure/self-signed escape hatch in v1) — security-conservative default for a self-hosted forge.
- **GitHub and Forgejo coexist** on one server, dispatched per-origin — additive by construction.
- **Naming is `forgejo`** throughout (settings keys, env vars `FORGEJO_URL`/`FORGEJO_TOKEN`, routes, docs) — follows the ask; internals stay host-generic.
- **Secrets follow the full pipeline** (`EnvVars` registry, `OptionalVault` classification, worker env scrubbing) per `server-secrets-strategy.md`.
- **Testing:** mock-`HttpClient` unit/integration tests as the default; live tests env-gated via the `live("VAR")` pattern; no mandatory containerized-Forgejo CI.
- **Docs/changelog/status:** `docs/public/integrations/forgejo.mdx`, a changelog entry, and `IntegrationProvider::Forgejo` status reporting are in scope for any answer.
- **Out of scope:** the Projects V2 tracker (no Forgejo GraphQL — impossible), and any change to existing GitHub behavior or wire formats.