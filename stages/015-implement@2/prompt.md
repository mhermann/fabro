Goal: Figure out a good way to add a provider for web searching that's not as expensive as brave or venice. Can we set something up that runs locally and provides the search?
Run ID: 01M23NRJC4AXRMG2FXYWZ8B6VP
Pipeline progress: 9 of 17 stages completed

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

## Stage: implement
- Status: succeeded
- Handler: agent
- Model: glm-5.3-flash
- Files touched: /workspace/fabro/.env.example, /workspace/fabro/docs/internal/server-secrets-strategy.md, /workspace/fabro/docs/public/administration/deploy-railway.mdx, /workspace/fabro/docs/public/administration/security.mdx, /workspace/fabro/docs/public/administration/self-host-docker.mdx, /workspace/fabro/docs/public/administration/server-configuration.mdx, /workspace/fabro/docs/public/administration/troubleshooting.mdx, /workspace/fabro/docs/public/agents/prompts.mdx, /workspace/fabro/docs/public/agents/tools.mdx, /workspace/fabro/docs/public/changelog/2026-09-09.mdx, /workspace/fabro/docs/public/docs.json, /workspace/fabro/docs/public/integrations/brave-search.mdx, /workspace/fabro/docs/public/integrations/searxng-search.mdx, /workspace/fabro/docs/public/integrations/venice-search.mdx, /workspace/fabro/lib/apps/fabro-server/src/demo/mod.rs, /workspace/fabro/lib/apps/fabro-server/src/diagnostics.rs, /workspace/fabro/lib/apps/fabro-server/src/spawn_env.rs, /workspace/fabro/lib/components/fabro-agent/src/cli.rs, /workspace/fabro/lib/components/fabro-agent/src/config.rs, /workspace/fabro/lib/components/fabro-agent/src/tools.rs, /workspace/fabro/lib/components/fabro-agent/src/web_search.rs, /workspace/fabro/lib/components/fabro-workflow/src/pipeline/initialize.rs, /workspace/fabro/lib/foundation/fabro-static/src/env_vars.rs, /workspace/fabro/lib/foundation/fabro-static/src/secret_registry.rs
- Response:
  > Implementation complete. Here's the summary.
  > 
  > ## What was built
  > 
  > A third `web_search` backend backed by an operator-run **SearXNG** instance — zero per-query cost, thin-client (Fabro only calls its JSON API; the operator runs the container). Selection order is now **SearXNG URL → Brave key → Venice key**, with no fallback after selection, matching the existing rule.
  > 
  > ## Changes, file by file
  > 
  > **Rust core**
  > - `lib/foundation/fabro-static/src/env_vars.rs` — added `SEARXNG_URL` and `SEARXNG_API_KEY` consts + test list.
  > - `lib/foundation/fabro-static/src/secret_registry.rs` — `SEARXNG_API_KEY` is an optional vault secret; `SEARXNG_URL` pinned as unclassified (config, not secret).
  > - `lib/components/fabro-agent/src/config.rs` — `ToolSecrets` gains `searxng_url`/`searxng_api_key`; `Debug` prints only presence booleans; redaction test extended.
  > - `lib/components/fabro-agent/src/web_search.rs` — new `SearchBackend::Searxng` variant, `searxng()` constructor (trims trailing `/`, appends `/search`), `search_searxng()` (`GET /search?q=…&format=json&pageno=1`, 30s timeout, optional bearer header, 401/403 hint, client-side `max_results` slicing), `format_searxng_results()`; `from_secrets` filters blank URLs at this single choke point; 11 new tests (precedence, blank-URL, request shape via httpmock incl. `header_missing` for keyless, slicing, status errors, shared tool schema).
  > - `lib/components/fabro-agent/src/tools.rs` — registration doc comments updated (registration logic already delegates to `from_secrets`).
  > - `lib/components/fabro-agent/src/cli.rs` — `cli_tool_secrets` reads both new env vars.
  > - `lib/components/fabro-workflow/src/pipeline/initialize.rs` — URL from process env (with required `#[expect(clippy::disallowed_methods)]`), API key from vault.
  > - `lib/apps/fabro-server/src/spawn_env.rs` — `SEARXNG_URL` added to the worker allowlist with comment; `SEARXNG_API_KEY` deliberately excluded; fail-closed test extended both ways.
  > - `lib/apps/fabro-server/src/diagnostics.rs` — `check_web_search` probes SearXNG first (mirrors Daytona URL resolution), new `check_searxng_search`, updated not-configured remediation; per the devil's finding, the two existing brave/venice precedence tests now pin `env_lookup(|_| None)` so an ambient `SEARXNG_URL` can't flip them; 3 new doctor tests.
  > - `lib/apps/fabro-server/src/demo/mod.rs` — demo remediation string updated.
  > 
  > **Docs**
  > - New `docs/public/integrations/searxng-search.mdx` (compose snippet, `settings.yml` JSON-format requirement, honest "locally operated, not hermetic" caveat, troubleshooting) + nav entry in `docs.json` + `changelog/2026-09-09.mdx` (+ nav).
  > - `agents/tools.mdx`, `agents/prompts.mdx`, `administration/server-configuration.mdx`, `troubleshooting.mdx`, `security.mdx`, `deploy-railway.mdx`, `self-host-docker.mdx`, `brave-search.mdx`, `venice-search.mdx` — the latter four per devil's finding #2 (they previously claimed Brave-key-beats-all, now false).
  > - `docs/internal/server-secrets-strategy.md` + `.env.example`.
  > 
  > ## Where reality differed from the plan
  > 1. **No Rust toolchain in this environment** — installed stable + the pinned `nightly-2026-04-14`, cargo-nextest, and cargo-insta.
  > 2. **Memory-constrained container (8GB)** — large test binaries OOM the linker; ran test crates individually with `-j 1`/`debug=0`. Everything that can link here passes: **fabro-static 5, fabro-agent 660, fabro-workflow 1444, fabro-server 906, plus 4184 across all other small crates — all green**; `cargo build --workspace`, `fmt --check`, `clippy -D warnings` (pinned nightly), and `cargo insta pending-snapshots` (none) all clean. Three graph-render tests initially failed at `-j 2` — that turned out to be a parallelism artifact of this environment, not a real failure: they pass at `-j 1`.
  > 3. **fabro-cli's test binaries cannot be linked in this container** under any linker configuration (even on pristine code paths). The crate compiles cleanly and clippy-clean; grep confirms it has no coupling to `ToolSecrets` or backend selection beyond displaying the `web_search` tool name.
  > 4. A `rustc` unused-import warning in `server/tests.rs` is pre-existing (file identical at HEAD; newer local rustc emits it) — left untouched.
  > 
  > The one manual check the plan flagged as impossible here still stands: a live query against a real SearXNG instance. The httpmock tests pin the exact wire contract, and the docs page walks operators through `fabro doctor` verification.

## Stage: fork
- Status: succeeded
- Handler: parallel
- Notes: Parallel node dispatched 4 branches (4 succeeded, 0 failed)

## Stage: synth
- Status: succeeded
- Handler: parallel.fan_in
- Notes: Stage completed: synth

## Stage: deep_review
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > I've completed my review. Here's my verdict.
  > 
  > ## What I verified myself
  > 
  > **Code (all sound):**
  > - Selection precedence SearXNG → Brave → Venice implemented at the `from_secrets` choke point, with blank-URL filtering and trailing-slash trimming — matches the human's "local wins" decision.
  > - Plumbing is correct: URL crosses the worker `env_clear()` via `WORKER_ENV_ALLOWLIST` (with a test asserting `SEARXNG_API_KEY` does *not* cross), token flows vault-only, `#[expect]` present on the `std::env::var` read, all existing `ToolSecrets` literals already use `..Default::default()`.
  > - Doctor mirrors agent selection (searxng-first) and the test-hermeticity fix from the critique was applied (`.env_lookup(|_| None)` pins); new tests cover searxng-wins-over-both-keys, blank-URL fallthrough, and a fast-refusal probe.
  > - I ran the suites myself: 660 tests (fabro-static + fabro-agent) and 2350 tests (fabro-server + fabro-workflow) all pass; `cargo +nightly-2026-04-14 clippy --all-targets -- -D warnings` clean on all four crates; `fmt --check` clean. (One initial run died with SIGKILL — sandbox OOM at high parallelism, not a code failure; `-j 2` passed.)
  > - The wire contract is pinned by httpmock tests (query params, `format=json`, bearer present/absent, slicing, error mapping) — tests would fail if the request shape or precedence regressed.
  > 
  > **Goal fit:** thin client to operator-run SearXNG, no per-query cost, local-wins, docs-only deployment artifacts — exactly what was requested and what the human chose.
  > 
  > ## Defects found — one class, three instances (docs only)
  > 
  > The docs claim `SEARXNG_URL` can be set in `server.env`. That is false. I traced it end-to-end: `AppState::config_env_lookup` is exactly `std::env::var` (`serve.rs:811` → `interp.rs:16`); there is **no `dotenvy` anywhere in `lib/`**; `server.env` is only parsed as a file for bootstrap-secret resolution and the env→vault migration; and `std::env::set_var` is clippy-banned in this repo, so the server *cannot* load files into its process env. The worker allowlist copies the live process env, so a `server.env`-only URL never reaches runs either. An operator following Setup step 2 as written gets a silently unregistered backend — while the page's own Troubleshooting section correctly says the URL must be in the *server process* environment. The docs contradict themselves, and the wrong version is the canonical setup instruction.
  > 
  > 1. **`docs/public/integrations/searxng-search.mdx`**, Setup step 2: "reads it from its own process environment (or `server.env`)". Fix: drop the `server.env` parenthetical; state "server process environment" and give real mechanisms (compose `env_file` (the packaged `docker-compose.yaml` does load `.env` into the container), systemd `Environment=`, or export before `fabro serve`).
  > 2. **`docs/public/administration/server-configuration.mdx`**: "read from the server process env (or `server.env`) and forwarded to workflow workers." Same fix — delete "(or `server.env`)".
  > 3. **`docs/public/administration/deploy-railway.mdx`**: "set it in Railway variables (or `server.env`)". Railway variables are correct (platform injects process env); the `server.env` parenthetical is wrong. Delete it.
  > 
  > (`self-host-docker.mdx`'s "set it in `.env`" is **correct** — I confirmed `docker-compose.yaml` declares `env_file: .env`.)
  > 
  > No code changes are needed; the three doc corrections are mechanical. Since the primary setup instruction as merged would not work, this blocks approval.
  > 
  > {"preferred_next_label": "fix"}

## Current context
| Key | Value |
|-----|-------|
| parallel.branch_count | 4 |
| parallel.results | 17.8 KB; full value: `/tmp/fabro/runtime/blobs/7792d3c9d00b5593c43373d5e1b223d3621a4f0ee63a4075e4528aa80d77ed42.json`; Preview: [{"id":"r_correct","index":0,"status":"succeeded","context_updates":{"last_response":"All 21 targeted tests pass (the `tests.rs` unused-import warning is the pre-existing one). My review is complete.\n\n## Review verdict\n\n**The Rust code is correct. I found no logic defects in it.** One ","last_st… |


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