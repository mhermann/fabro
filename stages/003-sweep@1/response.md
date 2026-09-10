I've completed the deeper read. Key new facts discovered since the last stage, then the full sweep.

**New structural discoveries (these change the risk profile):**

- `GitRunTarget.repo` is a **host-less GitHub slug**; `origin_url` is *derived* as `https://github.com/{owner}/{repo}` during validation (`fabro-types/src/run_intent.rs:97-105`). The forge host is not part of the run-target coordinate anywhere.
- `GitRunTarget`, `RunTarget`, `RunIntent`, and `PullRequestLink` are all **unified wire types** (`fabro-api/build.rs` `with_replacement`) — changing them is a public API contract change, not just internal refactoring.
- `PullRequestLink`'s wire struct uses `deny_unknown_fields` and validates `html_url` against github.com; there's a test proving a legacy `provider` field is *rejected* — the link type was deliberately simplified to GitHub-only.
- The GitHub webhook handler **only verifies and logs** deliveries; automations are API/schedule-triggered only. Webhook depth is currently shallow.
- The GitHub **tracker is GraphQL** (Projects V2); Forgejo has **no GraphQL API** — a "similar" tracker cannot be similar.
- Persisted run state lives in JSON (`summary_json`, blobs, events), not typed SQL columns; all existing data is github-shaped.
- `fabro-config/parse.rs` has a deprecation-message registry mentioning `[server.integrations.github]` — config surface is migration-aware.
- Secrets strategy: optional integration secrets are vault-only in the server process, env fallbacks are banned, secret names must be classified through `fabro-static`.
- Migrations strategy: no migration for new defaults or when old data remains valid under additive parsing.

---

## Dimension sweep

**1. Scope boundaries.** Clearly in: talking to a Forgejo instance over its Gitea-compatible `/api/v1` REST API with credentials from the vault. Clearly out: breaking any GitHub behavior. Genuinely unclear: whether "similar to the github integration" means Forgejo becomes a **run target** (clone-based sandbox execution, checkpoint push, auto-PR/auto-merge, sandbox token injection) — which forces wire-type changes to `GitRunTarget` — or an **integration-adjacent** surface (config, status, PR linking, maybe auth/login/webhooks) with runs staying GitHub-only. These two readings produce materially different software: one touches the run-admission coordinate, sandbox clone contract, and OpenAPI; the other doesn't.

**2. Users and callers.** Operators self-hosting Forgejo (or using Codeberg, the hosted Forgejo); web-UI users if login is in scope; agents inside sandboxes consuming the injected forge token. Existing callers that must not change: all GitHub flows, `GITHUB_BASE_URL` twin/test users, OpenAPI/TS-client consumers (additive OK, breaking not), `[run.scm] provider = "github"` configs, `fabro pr link` users. Note: pointing `GITHUB_BASE_URL` at a Forgejo instance does *not* work today (host identity is hardcoded in URL parsing, credential helper, PR links), so "just retarget it" is not an option — self-answered by reading the code.

**3. Data and state.** `PullRequestLink` is persisted in run JSON state. All existing data is github-shaped; additive serde-default fields keep it readable, so **no file or SQL migration is required** (confirmed against `migrations-strategy.md` criteria). Rollback is clean for additive config/secrets; a host field on `GitRunTarget` is only written for new runs (old-binary-reads-new-data is a forward-compat concern, standard for this repo). Not a question.

**4. Existing behaviour.** Extend, don't replace: non-GitHub origins currently fail sandbox setup with an explicit message; `pr link` rejects non-github URLs; `[run.scm] provider != github` disables origin detection; webhooks are verify-and-log only. The goal adds a peer; nothing asks GitHub to change.

**5. Edge and failure cases.** Self-hosted specifics: http/https, custom ports, subpaths; token scope shortfalls; Forgejo version drift (auto-merge support varies); `/pulls/` vs `/pull/` URL spelling; webhook signature header choice; unreachable instances at install/run time. The goal says nothing about these — all are implementation decisions except **how many instances exist**, which is a topology/design question (Q2), because per-origin credential matching has a token-exfiltration security surface.

**6. Non-functional constraints.** Shared `fabro-http` client, structured tracing per logging strategy, no secrets in logs. No perf/memory constraints apply to a REST forge client. Doesn't apply beyond conventions — one line, moving on.

**7. Security, privacy, permissions.** Widens data flow: tokens travel to a self-hosted host (possibly plain http), get embedded in sandbox remote URLs (existing GitHub pattern), and would be offered by host if multi-instance (credential-helper key is host-scoped: `credential.https://<host>.helper`). Secrets must be vault-only, registered in `fabro-static`, no env fallbacks (strategy doc is explicit). Browser OAuth against an instance makes the issuer the instance URL (additive; `IdpIdentity.issuer` is free-form). Not a question — constraints are documented — but topology (Q2) determines whether host-allowlisting is needed.

**8. Compatibility and versioning.** Wire changes must be additive-with-defaults; OpenAPI → progenitor → TS client regeneration is the mandated flow. House style has no feature flags — integrations are settings-gated (`enabled = false`). No deprecation. Self-answered.

**9. Testing and verification.** The repo has a full GitHub twin (`test/twin/github`) and httpmock-based unit tests; the natural approach is a `test/twin/forgejo` harness. Live tests need a real instance + credentials (`.env`); default to twin-only unless the human has an instance. Mostly mine to decide; noted in Decided.

**10. Operational surface.** Convention requires: `IntegrationProvider::Forgejo` status entry + web panel, `docs/public/integrations/forgejo.mdx` + `docs.json` nav, changelog entry, structured logs. Required regardless of scope tier — not a question.

**11. Dependencies and integration.** External: a Forgejo instance (operator-provided). No new Rust crates needed (reqwest/jwt/hmac already in tree). New env/secret names and settings blocks. Doesn't apply as a question — it's all implied by the topology answer.

**12. Definition of done.** Rejection scenarios: "I can't run a workflow against my Forgejo repo" (scope), "I meant our single company instance / I meant Codeberg / I meant any instance" (topology), "I wanted users to log in with Forgejo" (auth model), "you rewrote all of fabro-github and regressed GitHub" or "you copy-pasted 4k lines instead of unifying" (architecture). These map exactly to the vetted questions below.

---

## Bucketing

**Answerable from the repository** (answered above): where the change lands; no existing forge code; wire types are unified public contract; `GitRunTarget` is host-less; `PullRequestLink` persistence and compat approach; no migrations needed; secrets/vault constraints; webhook depth is log-only; GitHub tracker is GraphQL and Forgejo has none; clone-based sandboxes are GitHub-only by explicit contract; `GITHUB_BASE_URL` retargeting is insufficient; docs/status/changelog conventions.

**Yours to decide** (decided, see below): env/secret naming, webhook route + payload format, URL spelling tolerance, auth-header scheme, twin-test approach, sandbox token env var name, Gitea-compat branding, docs/status surface.

**Genuinely theirs**: scope tier, instance topology, auth model, crate architecture.

---

## Stress-test results

- **Regret check:** added nothing beyond the four — the strongest rejection ("built the wrong slice correctly") is covered by Q1; the "wrong instance model" regret by Q2; "no login" by Q3; "wrong codebase direction" by Q4.
- **Second-round check:** candidates were "do you have a test instance?", "can old data break?", "what env var names?". All self-answered or defaulted cheaply; none worth a slot.
- **Merge check:** merged sandbox-clone support, tracker, wire-type changes, and PR support *into* Q1 as named capability slices (they're all consequences of the tier). Merged Codeberg into Q2 as an option. Merged "Gitea too?" into Decided (build Gitea-compatible, brand Forgejo).
- **Self-answer check:** removed wire-compat, migration, secrets-handling, webhook-depth, and testing-strategy questions — all answered from repo docs/code.
- **Consequence check:** each surviving question below states the divergent build; none is answer-neutral.

---

## Vetted questions

1. **Which capability tier is "Forgejo support"?**
   - **A — Full run-target parity:** Forgejo repos work as run targets: clone-based sandbox execution (Docker/Daytona), checkpoint push, auto-PR/auto-merge, sandbox token injection. *Cost:* forces a host/provider field on the `GitRunTarget`/`PullRequestLink` wire types (OpenAPI + TS client + persisted state), and lifts the sandbox "GitHub origins only" contract.
   - **B — Integration surface only:** config, vault credentials, integration status, `fabro pr link` for Forgejo URLs, docs. Runs stay GitHub-only. *Cost:* users cannot point a run at a Forgejo repo; much smaller diff, no wire changes.
   - **C — A plus auth:** tier A **plus** Forgejo as a browser login method and inbound webhooks.
   - *What differs:* A/C restructure the run-admission coordinate and sandbox clone layer; B is contained additive config/URL work. (Note: the GitHub Projects-equivalent tracker has no Forgejo analogue — GraphQL-only on GitHub — and is excluded from every tier unless you say otherwise.)

2. **One configured Forgejo instance, or arbitrary/multiple instances (including Codeberg)?**
   - **A — Single server-wide instance:** `[server.integrations.forgejo] url` + one token; matches the Slack/Daytona single-credential pattern. *Cost:* only one forge reachable; simplest security story.
   - **B — Per-origin/multi-instance:** any Forgejo remote works, credentials matched by host. *Cost:* needs a host-keyed credential store, host allowlisting to prevent token exfiltration to attacker-controlled git hosts, and host-aware URL parsing everywhere.
   - **C — Codeberg-first:** treat `codeberg.org` as a fixed second forge host like github.com. *Cost:* hardcodes another host; self-hosted instances still unsupported.
   - *What differs:* config schema, credential storage/lookup design, URL-parse and credential-helper plumbing, and the security review surface.

3. **Which auth model(s) for the Forgejo side?**
   - **A — PAT only:** static scoped token from the vault. *Cost:* no browser sign-in; mirrors the GitHub `token` strategy precedent (web login disabled).
   - **B — PAT + OAuth2 app:** adds Forgejo as a `ServerAuthMethod` with PKCE login, `AuthMethod::Forgejo`, instance-URL identity issuer, allowed-usernames gating. *Cost:* the whole `auth/` + install + web-login surface, per the strategy matrix.
   - *What differs:* B builds the OAuth/login/install wizard machinery; A skips it entirely. (Forgejo has no GitHub-App/installation-token equivalent, so that credential kind simply has no counterpart either way.)

4. **Architecture: contained parallel crate, or generalize into a shared forge abstraction first?**
   - **A — Parallel `fabro-forgejo` crate** mirroring `fabro-github`'s needed surface (token-only credentials, REST ops, URL math). *Cost:* some intentional duplication; zero risk to GitHub paths.
   - **B — Extract a forge-provider trait** (auth, PR ops, URL/credential-helper math) and migrate GitHub onto it in the same change. *Cost:* touches every GitHub call site (server, workflow, sandbox, tracker); larger review and regression risk, but no permanent split — and the repo's own type-unification guidance leans this way.
   - *What differs:* size and risk of the diff, and whether a third forge (GitLab/Gitea) later is cheap or another rewrite.

## Decided without asking

- **No data/DB migrations:** all persisted PR/run data is github-shaped; additive serde-default fields keep old data valid; new shapes are only written for new Forgejo runs. (Matches `migrations-strategy.md`: no migration for additive parsing.)
- **Wire-compat method:** additive optional fields only on `PullRequestLink`/`GitRunTarget`; regenerate OpenAPI → progenitor → TS client; keep `with_replacement` unification.
- **Secrets:** new `FORGEJO_*` names registered in `fabro-static` `EnvVars` + secret/redaction registries; vault-only in the server process; no env fallbacks (per `server-secrets-strategy.md`).
- **Webhooks (if in scope):** separate route `/api/v1/webhooks/forgejo`, HMAC-SHA256 verification using Forgejo's GitHub-compatible payload format so the existing verification code is shared; log-only depth matching current GitHub behavior.
- **URL parsing:** accept `/pulls/N` for Forgejo PR URLs (tolerate `/pull/N`); reuse GitHub slug grammar for owner/repo initially.
- **Auth header:** use the token scheme Forgejo documents (`Authorization: token …`); verify against the twin.
- **Sandbox token injection (if in scope):** inject as `FORGEJO_TOKEN` (and document `GITEA_TOKEN` as a follow-up decision for agent compatibility).
- **Gitea compatibility:** build against the Gitea-compatible API but brand/configure as Forgejo only in v1 docs.
- **Testing:** build `test/twin/forgejo` mirroring `test/twin/github`; twin-only by default; live `#[e2e_test(live(...))]` tests added later if an instance is available.
- **Operational surface:** `IntegrationProvider::Forgejo` status entry, web integrations panel section, `docs/public/integrations/forgejo.mdx` + `docs.json` nav, changelog entry — required for whichever tier is chosen.
- **GitHub behavior is frozen:** all existing tests stay green; `GITHUB_BASE_URL` twin flows unchanged.