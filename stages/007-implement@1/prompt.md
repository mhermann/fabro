Goal: Figure out a good way to add a provider for web searching that's not as expensive as brave or venice. Can we set something up that runs locally and provides the search?
Run ID: 01M23NRJC4AXRMG2FXYWZ8B6VP
Pipeline progress: 5 of 17 stages completed

## Stage: understand
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response (8.0 KB; full value: `/tmp/fabro/runtime/blobs/a3abe41c2dee025990e67190ad6fd651ac218f47da8bca0db228d314ebfa65bc.json`)
  Preview: I have a complete picture now. Here's my analysis.
  
  ## 1. Repository structure and where the change lands
  
  **Fabro** is an AI workflow orchestration platform: Rust workspace (`lib/apps/*`, `lib/components/*`, `lib/foundation/*`) + React SPA (`apps/fabro-web`), OpenAPI-first API, Mintlify docs in `do…

## Stage: ask
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > All four answers are in. Restatement for the record:
  > 
  > **1. Deployment model → Thin client.** Fabro will not manage any container lifecycle. We add a third `SearchBackend` variant that makes HTTP calls to an operator-run SearXNG instance; the integration doc ships a copy-pasteable compose snippet plus the recommended SearXNG config (JSON output format enabled). No Docker orchestration code in `fabro-sandbox` or anywhere else.
  > 
  > **2. Precedence → Local wins.** Selection order becomes: local URL configured → use local; else Brave key → Brave; else Venice key → Venice; else tool unregistered. A configured local URL makes paid keys dormant (available again by unsetting the URL). No fallback between providers on failure, per the existing rule.
  > 
  > **3. Config mechanism → URL env var + config layer.** New `EnvVars` const (e.g. `SEARXNG_URL`) resolved via `config_env_lookup` like `DAYTONA_API_URL`, deliberately *not* classified as a vault secret in `secret_registry.rs`. Optional bearer token for protected instances goes in the vault. The URL flows into `ToolSecrets` (or its renamed successor) at pipeline init in `initialize.rs` and from process env in the standalone CLI's `cli_tool_secrets`, so worker env-scrubbing stays intact.
  > 
  > **4. Deploy artifacts → Docs only.** `docker-compose*.yaml` files stay untouched; the compose snippet lives in the new integration doc page only.
  > 
  > Carried-over decisions that stand: SearXNG's native JSON API as the wire format; optional bearer-token auth, keyless by default; map SearXNG `title`/`url`/`content`/`publishedDate` into the existing `SearchHit` shape without cross-engine dedup; `fabro doctor` gains a local-URL probe mirroring the Brave/Venice checks; httpmock unit tests in `web_search.rs` following the existing overridable-`search_url` pattern; full docs set updated (integration page + `docs.json` nav + `agents/tools.mdx` + `server-configuration.mdx` + changelog + `.env.example`).

## Stage: triage
- Status: succeeded
- Handler: prompt
- Model: glm-5.3
- Response:
  > I have enough to plan. Here's what I understand the human wants, plus the decisions I'm making on their behalf:
  > 
  > **The goal:** Add a web search provider to Fabro that has no per-query cost (unlike Brave's and Venice's paid APIs), by running the search capability locally/cheaply under our control.
  > 
  > **Decisions I'm making:**
  > 
  > - **Engine: SearXNG.** It's the de facto standard answer for self-hosted search — a metasearch aggregator that queries public engines (Google, Bing, DuckDuckGo, Wikipedia, etc.) with no API keys, and exposes a JSON API (`/search?q=...&format=json`). Alternatives (Whoogle has no first-class JSON API, YaCy has poor result quality) don't hold up. Zero marginal cost per query.
  > - **Deployment: Docker.** The repo is already Docker-first (default sandbox provider, packaged compose service). I'll treat SearXNG as an optional compose service / documented `docker run` (with `json` format enabled in `settings.yml`), not something Fabro auto-spawns. Operators point the provider config at a base URL; a token is optional (only needed if they enable SearXNG's limiter).
  > - **Integration:** Implement it as a new provider in the existing web-search provider abstraction alongside Brave and Venice — config is just a base URL, and SearXNG's JSON response maps onto the same normalized result type so agent/server call sites don't change. Provider selection follows whatever pattern Brave/Venice already use (enum + config, strum-derived per repo convention).
  > - **Honest caveat baked into docs:** "local" means locally-operated proxying — SearXNG still hits upstream public engines, so it's free but not hermetic. That's the accepted tradeoff and exactly why it's cheaper than Brave/Venice.
  > 
  > The remaining details (result count, timeout/retry behavior, which upstream engines to enable in `settings.yml`) are implementation choices I can settle during planning without another round.
  > 
  > {"preferred_next_label": "enough"}

## Stage: plan
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response (12.8 KB; full value: `/tmp/fabro/runtime/blobs/144a8a70e013bfc7c34a69e51f708f43ca087ac73d036eb0d0be59e93e97c577.json`)
  Preview: # Implementation Plan: SearXNG local backend for `web_search`
  
  Per the human's answers: **thin client** to an operator-run SearXNG instance, **local wins** over paid keys, config via **`SEARXNG_URL` env var** (non-secret) with an **optional `SEARXNG_API_KEY` vault secret** (forwarded as a bearer tok…

## Stage: devils
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > I attacked the plan against the repository as it actually is. Verdict and findings:
  > 
  > ## Verified sound (the load-bearing claims)
  > 
  > - **Worker plumbing**: confirmed `lib/apps/fabro-cli/src/commands/run/runner.rs:268` (`load_worker_vault`) loads the vault from the shared storage dir, so `vault.get(SEARXNG_API_KEY)` works in the worker; and `spawn_env.rs` copies the *server's* process env through the allowlist, so `SEARXNG_URL` crosses `env_clear()`. The dual-path design (URL=env, token=vault) is real, not aspirational.
  > - **Single secret construction site**: `initialize.rs:359` computes `ToolSecrets` once; line 265's `tool_secrets_for_api` is a clone. No hidden second path.
  > - **Struct-literal blast radius**: enumerated all `ToolSecrets {` sites (`cli.rs:45`, `initialize.rs:316`, `config.rs:353`, `web_search.rs:316`, `tools.rs:1753`, `api.rs:3932`, `parity_matrix.rs:227`). Compiler catches each; step 11 covers them.
  > - **Clippy**: `std::env::var` is in `clippy.toml` `disallowed-methods`, so the `#[expect]` in `initialize.rs` is *definitely* required, not conditional — trivial.
  > - **No UI coupling**: `apps/fabro-web` only maps the tool name to a label; doctor results render generically. Correctly excluded.
  > - **Snapshots/demo**: profile prompt snapshots inject keys directly (unaffected); the demo remediation string is not test-pinned.
  > - **`fabro secret set SEARXNG_API_KEY`** will be accepted (no restrictive name allowlist in `secret/set.rs`).
  > 
  > ## Defects found — corrections the implementer must absorb (not redo-inducing)
  > 
  > 1. **The plan makes a false claim about existing diagnostics tests.** `TestAppStateBuilder` defaults `env_lookup` to the **real process env** (`test_support.rs:655`). The existing precedence tests (`check_web_search_prefers_brave_when_both_vault_keys_exist`, the venice/env-backed ones) do not override it. Once `check_web_search` consults `SEARXNG_URL` first, any ambient `SEARXNG_URL` in a developer shell flips those tests to the searxng branch. The plan says they "keep passing with only remediation-text edits" — wrong. Fix while implementing: pin `env_lookup` (returning `None`) in those tests, and have the new searxng tests set/absent the URL explicitly. Important trap to avoid: the failure mode must be fixed by pinning env in tests, **not** by reordering `check_web_search` — reordering would make doctor report a different backend than runs actually use, silently inverting the human's "local wins" decision.
  > 2. **Four doc files missing from the list.** `venice-search.mdx:20,49` states "Fabro prefers direct Brave Search whenever `BRAVE_SEARCH_API_KEY` is also present" — false after this change; `brave-search.mdx` frames Brave as the automatic default; `deploy-railway.mdx:44` and `self-host-docker.mdx:111` enumerate vault secrets ("Brave Search, Venice Search") where `SEARXNG_API_KEY` belongs. Add them to phase 5; without them the docs self-contradict.
  > 
  > Two non-blocking placement notes: put the empty/whitespace URL filtering in `from_secrets` (single choke point) rather than only `initialize.rs`, so the standalone-CLI path can't register a broken backend on `SEARXNG_URL=" "`; and be aware that adding `SEARXNG_API_KEY` to `OPTIONAL_VAULT_SECRETS` also enrolls it in the still-wired env→vault startup migration (`serve.rs:683`) — identical to BRAVE/VENICE behavior, benign, no action.
  > 
  > Both corrections are minutes of work with unambiguous fixes; the architecture, file map, precedence wiring, and verification strategy all check out against the actual code. The plan is a sound basis for implementation.
  > 
  > {"preferred_next_label": "approve"}


Implement the approved plan.

The goal is:

Figure out a good way to add a provider for web searching that's not as expensive as brave or venice. Can we set something up that runs locally and provides the search?

Follow the plan. Where reality differs from it, adapt and say what you changed
and why.

Rules:

- Match the surrounding code — its naming, structure, error handling, and
  comment density. Read a neighbouring file before you write a new one.
- Run this repository's existing tests and build after your change. If they
  fail, fix them.
- Do not commit. Fabro commits your work automatically at the end of the stage.
- Do not open a pull request. A later stage does that.

If you are here after a review found defects, its findings are in the
context above — read them and fix exactly what was raised, point by point.
Do not take the opportunity to refactor unrelated code. If you believe a finding
is wrong, say so and explain why rather than silently ignoring it; repeating the
same work unchanged will simply be rejected again.

Finish by summarising what you changed, file by file.