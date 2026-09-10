Goal: Figure out a good way to add a provider for web searching that's not as expensive as brave or venice. Can we set something up that runs locally and provides the search?
Run ID: 01M25MCE55ZB328VA1R5K1P085
Pipeline progress: 7 of 22 stages completed

## Stage: understand
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > I now have a complete picture. Here's the understanding report.
  > 
  > ---
  > 
  > ## 1. Repository structure and where the change lands
  > 
  > **Fabro** is an AI workflow orchestration platform. Rust workspace (~40 crates, edition 2021) + a React SPA and generated TS client. Tooling: `cargo nextest`, httpmock for HTTP unit tests, insta snapshots, nightly clippy/rustfmt, Mintlify docs in `docs/public/*.mdx`.
  > 
  > **Current web search architecture** — one tool, two paid backends:
  > 
  > | Layer | File | Role |
  > |---|---|---|
  > | Backend impl | `lib/components/fabro-agent/src/web_search.rs` | `SearchBackend` enum `{Brave, Venice}`, `from_secrets()` selection (Brave wins), `search()` dispatch, per-provider formatters feeding a shared `SearchHit {title, url, description, date}` → `format_hits()` |
  > | Config struct | `lib/components/fabro-agent/src/config.rs` | `ToolSecrets { brave_search_api_key, venice_api_key }` |
  > | Server-run source | `lib/components/fabro-workflow/src/pipeline/initialize.rs:314` | reads both keys **from the vault** (workers start with cleared env) |
  > | Standalone CLI source | `lib/components/fabro-agent/src/cli.rs:44` | reads both keys **from process env** |
  > | Registration | `tools.rs::register_web_search_tool`, `profiles/claude5.rs:47`, `profiles/gpt56.rs` | tool registered only if a backend is selected; same `web_search` schema regardless of backend |
  > | Env/secret registries | `lib/foundation/fabro-static/src/env_vars.rs`, `secret_registry.rs` | `BRAVE_SEARCH_API_KEY`, `VENICE_API_KEY` are "optional vault secrets" |
  > | Server diagnostics | `lib/apps/fabro-server/src/diagnostics.rs:760` `check_web_search` | probes the selected backend; remediation text names both secrets |
  > | Docs | `docs/public/agents/tools.mdx` (web_search section), `docs/public/integrations/{brave-search,venice-search}.mdx`, `docs/public/administration/server-configuration.mdx`, `.env.example`, demo-mode strings in `fabro-server/src/demo/mod.rs:675` |
  > | Tests | httpmock unit tests in `web_search.rs`; live parity-matrix tests in `fabro-agent/tests/it/parity_matrix.rs` keyed on `BRAVE_SEARCH_API_KEY` |
  > 
  > ## 2. Goal restated concretely
  > 
  > Add a **third `SearchBackend` variant** that talks to a search service the operator runs themselves — the canonical candidate being **SearXNG** (self-hosted metasearch aggregator with a JSON API, zero per-request cost) — so `web_search` works without Brave/Venice credentials. It must integrate at every layer the existing backends do: backend enum + formatter, configuration plumbing (vault/env/settings), registration gating, doctor diagnostics, docs, and tests. The agent-visible tool (`web_search`) and its schema stay unchanged; the backend remains an implementation detail selected at session construction.
  > 
  > ## 3. What I know for certain
  > 
  > - The insertion point is well-isolated: everything funnels through `SearchBackend::from_secrets(&ToolSecrets)` → one `web_search` tool. A new variant touches one enum, one selection function, one `search()` arm, one formatter (SearXNG's `results[].{title,url,content,publishedDate}` maps almost 1:1 to `SearchHit`).
  > - Credential flow is strictly two-path: vault for server runs, process env for the standalone CLI. `secret_registry.rs` explicitly classifies URL-style config (`DAYTONA_API_URL`, `OPENAI_BASE_URL`) as **non-secrets** — a local backend's URL is config, not a credential.
  > - The `web_search` executor runs in the worker/server process (it uses `fabro_http` directly, not the sandbox). So "local" means *reachable from the Fabro server's network position* — inside docker-compose that's the compose network, not host localhost.
  > - Docker is already central to the deployment model (default sandbox provider, compose files mount `docker.sock`), so a sibling SearXNG container is operationally natural.
  > - Established invariants to preserve: single tool name/schema; selection happens once, no fallback retry between backends on failure; `web_search` is `Shell` permission category (needs `Full` for auto-approval); server reads secrets from vault only.
  > - Test patterns exist: httpmock-based request/response assertions per backend, `from_secrets` precedence tests, diagnostics tests that assert remediation strings.
  > 
  > ## 4. What is genuinely ambiguous
  > 
  > 1. **What "runs locally" means.** (a) Self-hosted SearXNG/Whoogle container the operator runs — truly local service; (b) "no paid key" via free endpoints (DuckDuckGo HTML scraping) — no local service at all, different product. Two engineers would build different things from the same sentence.
  > 2. **Who owns the service lifecycle.** Point at an operator-managed URL (simple config) vs. Fabro launching/managing the SearXNG container itself via the existing Docker integration (much bigger: image pulls, lifecycle, health, cleanup).
  > 3. **Config surface for the URL.** Vault entry (uniform with existing keys, but URLs aren't secrets), new env var (`SEARXNG_URL`), a `settings.toml` section (`[server.web_search]`), or some combination — and `ToolSecrets` may need to become a broader "tool config" struct, which ripples into `ProfileDeps`.
  > 4. **Precedence when multiple backends are configured.** Local-first (free wins, Brave as paid fallback) vs Brave-first (quality wins, local as last resort)? Current code has a strict Brave > Venice order with tests pinning it.
  > 5. **Failure semantics.** Keep "selected backend errors out, no retry through others" (documented behavior) or introduce fallback since a local instance dying is now a plausible failure mode.
  > 6. **SearXNG deployment specifics.** Its JSON output must be enabled in `settings.yml` (`search.formats: [json]`); do we ship a compose profile/settings for it, document it, or auto-manage it? Optional API key for instances with the limiter enabled? Timeout behavior (local instances can be slow aggregating engines)?
  > 7. **Diagnostics/doctor shape.** `check_web_search` maps provider→secret-name; a local backend has a URL to probe and different remediation text ("start your SearXNG instance" vs "run fabro secret set …"). Demo-mode and docs strings all need matching decisions.
  > 8. **Result mapping details.** SearXNG returns richer data (`engines`, `publishedDate`, `score`, possible duplicates across engines) — whether to dedupe, rank, or surface the source engine in the output line.
  > 9. **E2E test scope.** Whether the parity matrix grows a local-provider live variant, and what the CI story is for a service that needs a container running.
  > 
  > No files were modified; this stage was exploration only.

## Stage: sweep
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response (12.0 KB; full value: `/tmp/fabro/runtime/blobs/4a06c307bb9fb8d4f80a2a3431c9641afae59bc8856d5af93ae3f929d4958382.json`)
  Preview: I now have everything needed. Here is the full sweep.
  
  ---
  
  ## Dimension-by-dimension sweep
  
  ### 1. Scope boundaries
  - **Clearly in:** a third `web_search` backend with $0 marginal cost per query, wired through the existing single insertion point (`SearchBackend` enum in `lib/components/fabro-agent/…

## Stage: ask
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > Both questions answered. Restating for the planning stage:
  > 
  > ## Answers
  > 
  > 1. **What should the new no-cost web_search source be?**
  >    **Answer: Self-hosted SearXNG.**
  >    *What I take it to mean:* Build a third `SearchBackend` variant in `lib/components/fabro-agent/src/web_search.rs` that talks to a SearXNG instance's JSON API (`GET /search?q=...&format=json`), mapping `results[].{title,url,content,publishedDate}` to the existing `SearchHit {title, url, description, date}` shape. No scraping backend, no keyless-endpoint fallback. Docs must call out that the instance needs JSON output enabled in its `settings.yml` and may be rate-limited by upstream engines.
  > 
  > 2. **Who sets up and keeps the service running?**
  >    **Answer: Operator-managed (LiteLLM-style).**
  >    *What I take it to mean:* No container-lifecycle work in Fabro — no docker.sock management, no auto-start, no health supervision. The change is a backend + configuration + docs: Fabro points at an existing instance via config, and the integration doc explains how to run SearXNG (an optional compose snippet in docs is fine, but nothing in code manages it).
  > 
  > ## Decisions already settled (carrying forward, not re-asked)
  > 
  > - **Precedence: SearXNG-first when configured** — `searxng > brave > venice`; users who don't configure it see zero change. Cost is the motivation, so a paid key must not win over a configured local instance.
  > - **No fallback between backends on failure** — existing documented/tested invariant; a dead SearXNG errors the call, same as a dead Brave key.
  > - **No provider enum / settings section** — commit `88ed2ac9` deliberately removed `[server.integrations.search]`; selection stays presence-based.
  > - **Config surface:** new vault key (e.g. `SEARXNG_URL`, set via `fabro secret set`) for server runs, process env for the standalone CLI — mirroring the existing two keys exactly; no OpenAPI/TS-client changes.
  > - **Unchanged:** tool name/schema, permission category (Shell), events, result output format.
  > - **Also included:** doctor/diagnostics support for the new backend, demo-mode string update, docs (`tools.mdx`, new `integrations/searxng.mdx` + `docs.json` nav, `server-configuration.mdx`, changelog entry), httpmock unit tests + precedence tests mirroring the Brave/Venice suites.

## Stage: triage
- Status: succeeded
- Handler: prompt
- Model: glm-5.3
- Response:
  > You've given me enough to build this. Restating what I understand:
  > 
  > **Goal:** Add a web search provider to Fabro that has zero marginal cost per query and runs on the user's own infrastructure, as an alternative to the paid Brave/Venice providers, wired into the existing search abstraction so agent tools and workflow stages can use it unchanged.
  > 
  > **Decisions I'm making on your behalf** (none of these change the shape of the software enough to warrant another round):
  > 
  > 1. **Engine: SearXNG.** It's the canonical self-hosted metasearch engine — no API keys, no per-query billing, aggregates Google/Bing/DDG/etc., ships a first-class JSON API (`/search?format=json`), and has a standard Docker image. Building our own crawler/indexer or using Whoogle (no stable JSON API) would be materially worse for the effort.
  > 2. **Integration shape:** a new provider variant alongside Brave/Venice in the existing search-provider enum, configured by base URL (defaulting to `http://localhost:8080`). Fabro talks to the instance's JSON API and normalizes results into the same result struct the other providers emit, so downstream consumers don't change.
  > 3. **Deployment story:** document the one-line `docker run searxng/searxng` (with `SEARXNG_BASE_URL` and JSON format enabled) and add an optional compose service alongside the existing packaged compose file. Fabro does **not** manage the container's lifecycle in code — it's an operator-run local service, consistent with how the Docker sandbox provider treats the daemon as operator infrastructure.
  > 4. **Failure behavior:** if the local instance is unreachable, the search tool fails with a clear actionable error rather than silently falling back to a paid provider — "free and local" was the explicit priority, and silent fallback would bill people without their consent.
  > 
  > If SearXNG's result quality turns out to be a concern, that's tunable per-instance (which engines it aggregates) and doesn't affect the provider code.
  > 
  > {"preferred_next_label": "enough"}

## Stage: plan
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response (11.1 KB; full value: `/tmp/fabro/runtime/blobs/f46b6510f41589b4b87849307dfa69a8ab2899a009733bbb9fa147abe943990c.json`)
  Preview: I have everything verified against the current code. Here is the implementation plan.
  
  ---
  
  # Implementation Plan: SearXNG backend for `web_search`
  
  ## Decisions locked in (from the human + repo conventions)
  
  - **Backend:** self-hosted SearXNG, JSON API (`GET /search?q=…&format=json`). Operator-mana…

## Stage: devils
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > I attacked the plan against the repository as it actually is. Findings:
  > 
  > ## Defects found
  > 
  > 1. **`docs.json` changelog nav entry is missing from the plan.** Changelog pages are individually listed in `docs.json` under month groups (`"changelog/2026-09-04"` sits in a "September 2026" group at ~line 312). The plan creates `docs/public/changelog/2026-09-10.mdx` and edits `docs.json` only for the integrations nav — the new changelog page would be orphaned. One-line fix the implementer must fold in: add `"changelog/2026-09-10"` at the top of the September group while already editing that file. Not rework-class; doesn't justify a planning round on its own.
  > 
  > ## Claims verified as sound (so the implementer doesn't re-litigate)
  > 
  > - **No third `ToolSecrets` population site exists.** I checked the suspect: `server/handler/sessions.rs` builds its profile via `AgentProfileBuilder::new(...).build()` with *no* `with_tool_secrets` — the "Ask Fabro" session deliberately denies `web_search` via `AskFabroToolAccessPolicy` (comment at line 730). So the plan's two sites (workflow `initialize.rs` vault read, `cli.rs` env read) are complete, and that path is correctly untouched.
  > - **No exhaustive `match` on `SearchBackend` outside `web_search.rs`** — only construction sites (`claude5_tools.rs:541/546` schema tests, `from_secrets` callers). Adding a variant breaks nothing else.
  > - **The struct-literal fallout list is complete.** Grepped every `ToolSecrets` literal: `initialize.rs:316`, `cli.rs:46`, `config.rs:354`, `profiles/mod.rs:748`, `tools.rs:1754`, `web_search.rs:317`, `parity_matrix.rs:228`, `api.rs:3935` — all named in the plan. `ToolSecrets::default()` uses are unaffected.
  > - **`fabro secret set SEARXNG_URL` works**: the secrets handler rejects only bootstrap names; the vault accepts arbitrary names. Not registering it in `secret_registry.rs` is correct per that file's own precedent (URL-style config deliberately unclassified).
  > - **httpmock test pattern works for the new variant**: mutating a `base_url` field to point at the mock server is the exact pattern the Venice/Brave tests use with `search_url`; `search_http_client()` already uses `test_http_client()` under `cfg(test)`.
  > - **`fabro_http::Url` is re-exported** (lib.rs re-export list), so the pre-HTTP URL validation step compiles as planned. Per-request `.timeout()` on a GET is the same reqwest facility Venice already uses on POST.
  > - **Verification is discriminating, not vacuous**: the httpmock tests assert exact query params (`q`, `format=json`), parsing, truncation, and the 403-hint text; precedence and redaction tests pin the selection order and Debug output. These fail if the backend is wrong.
  > 
  > ## Not blocking
  > 
  > - The "Ask Fabro" sessions never having `web_search` is pre-existing deliberate policy, not a gap this change must close.
  > - Excluding the admin "secrets list" doc mentions is a stated, defensible choice.
  > - No OpenAPI/conformance drift is possible — nothing in the API surface names search config anymore (verified the refactor removed it all).
  > 
  > The plan is sound as a basis for work; the single defect is a one-line fold-in while already editing `docs.json`, which I have named explicitly above.
  > 
  > {"preferred_next_label": "approve"}

## Stage: refactor_check
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > I decided this by reading the code the change will land in — `web_search.rs` in full, `ToolSecrets` in `config.rs`, both population sites (`initialize.rs`, `cli.rs`), `check_web_search` in diagnostics, and the test patterns — and asking each bar question against the actual shapes.
  > 
  > ## What I found
  > 
  > **The seams the change needs already exist.** The varying dimension is "which HTTP backend serves `web_search`," and `SearchBackend` is precisely that seam: an enum with per-variant data, one `from_secrets` selection point, one `search()` dispatch, one formatter per provider feeding a shared `SearchHit`/`format_hits`. A third variant is the designed-for extension, not a workaround. Config flows as one struct populated at exactly two sites, each gaining one line — not a flag threaded through five call sites.
  > 
  > **Duplication: a third similar function, not triplication of logic.** `search_brave`/`search_venice`/`search_searxng` share a ~10-line send/parse skeleton but genuinely differ in request construction (GET+query vs POST+body), error semantics (Venice's 402 balance headers, SearXNG's 403 json-format hint), and timeouts. A shared helper would collapse ~5 lines per backend and blur those differences. Likewise the formatters: three ~15-line mappers over *structurally different* JSON shapes (nested `web.results` vs flat `results[].content` vs `results[].publishedDate`+truncation) into one struct — that's variety, not copy-paste. Unifying them would be abstraction for its own sake.
  > 
  > **No unit outgrows itself.** `web_search.rs` is ~295 lines of production code; the change adds ~60–80. It stays one coherent module.
  > 
  > **Tests reach everything.** The httpmock pattern (mutate the backend's URL field, assert exact request params and response rendering) and the diagnostics `TestAppStateBuilder.vault_entries` pattern cover the new behaviour with no manual verification gap.
  > 
  > **Names:** `SearchBackend`, the URL consts, and `check_web_search` all stay accurate. One honest drift: `ToolSecrets` will hold `searxng_url`, which is a URL, not a secret. I judge this below the bar: Fabro's own vocabulary stores this value in the vault via `fabro secret set` (the product calls vault entries secrets), a rename would churn ~8 literals and two crates for zero behavioral gain, and the Debug impl already frames fields as `*_configured` rather than values.
  > 
  > ## On record but rejected as refactoring candidates
  > 
  > - Renaming `ToolSecrets` → `ToolConfig`: naming purity, not obstruction; expensive churn relative to the change.
  > - Extracting a shared send/parse helper for the three backend functions: collapses little, obscures per-backend error semantics.
  > - Splitting `web_search.rs` into per-backend files: premature until a fourth backend exists.
  > 
  > None of these make the implementation meaningfully harder, larger, or riskier; the change is small and self-contained against existing seams. No refactoring stage has run previously in this workflow — this is a decision on the merits, and the merits say no.
  > 
  > {"preferred_next_label": "none"}


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
- Never merge a pull request, and never run `gh pr merge`. This workflow does
  not merge. A human decides whether the change lands.
- If a refactoring stage ran before you, the code no longer looks the way the
  plan describes. Re-read every file the plan names before you follow it, and
  adapt the plan to the shape the code is in now.

If you are here after a review found defects, or after a reviewer left comments
on the open pull request, the findings are in the context above — read them and
fix exactly what was raised, point by point.
Do not take the opportunity to refactor unrelated code. If you believe a finding
is wrong, say so and explain why rather than silently ignoring it; repeating the
same work unchanged will simply be rejected again.

Finish by summarising what you changed, file by file.