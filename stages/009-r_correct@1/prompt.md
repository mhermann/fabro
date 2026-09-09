Goal: Figure out a good way to add a provider for web searching that's not as expensive as brave or venice. Can we set something up that runs locally and provides the search?
Run ID: 01M22Z63CN2WNR95GFXQD7F0H6
Pipeline progress: 6 of 17 stages completed

## Stage: understand
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > I have a complete picture now. Here's my exploration report.
  > 
  > ## 1. Repository structure and where the change lands
  > 
  > **Fabro** is an AI workflow orchestration platform: Rust workspace (`lib/apps`, `lib/components`, `lib/foundation`) plus TypeScript apps (`apps/fabro-web`, `apps/marketing`). Workflows are Graphviz graphs; each node is a stage run by an AI agent with a tool registry.
  > 
  > The web search feature is concentrated in a small, well-isolated set of files:
  > 
  > | Concern | Location |
  > |---|---|
  > | Backend enum + HTTP + result formatting | `lib/components/fabro-agent/src/web_search.rs` (`SearchBackend::{Brave, Venice}`) |
  > | Tool registration (`web_search` tool, registered only when a credential exists) | `lib/components/fabro-agent/src/tools.rs` (`register_web_search_tool`) |
  > | Secrets struct | `lib/components/fabro-agent/src/config.rs` (`ToolSecrets`) |
  > | Vault → secrets for server runs | `lib/components/fabro-workflow/src/pipeline/initialize.rs` (`tool_secrets_from_configured_sources`) |
  > | Process-env → secrets for CLI runs | `lib/components/fabro-agent/src/cli.rs` |
  > | Env var name registry / secret classification | `lib/foundation/fabro-static/src/env_vars.rs`, `secret_registry.rs` |
  > | Doctor diagnostics probe | `lib/apps/fabro-server/src/diagnostics.rs` (`check_web_search` → `check_brave_search` / `check_venice_search`) |
  > | Demo-mode doctor string | `lib/apps/fabro-server/src/demo/mod.rs` |
  > | Docs | `docs/public/integrations/brave-search.mdx`, `venice-search.mdx`, `agents/tools.mdx`, changelog |
  > | Tests | Unit tests in `web_search.rs` (httpmock), `tests/it/parity_matrix.rs` (`web_search_provider_test!`), e2e live test gated on `BRAVE_SEARCH_API_KEY` |
  > 
  > Precedents that matter: each `SearchBackend` variant already carries an injectable `search_url` (used by tests via httpmock), and the repo already has a **self-hosted proxy pattern** — the LiteLLM provider is documented as a local service configured via `settings.toml` with a `base_url`. The repo also ships multiple `docker-compose*.yaml` files and a `docker/` dir, so running a sibling local service is an established deployment shape.
  > 
  > ## 2. Goal restated concretely
  > 
  > Today the `web_search` agent tool has exactly two backends — Brave (paid/rate-limited API) and Venice (balance-based paid API) — selected purely by which API key is in the vault (Brave wins when both exist; no key ⇒ the tool is omitted from the agent's toolset and prompt). The goal is to add a **third backend that costs nothing per query because it runs as a local/self-hosted service** the operator deploys (the ecosystem-standard answer is SearXNG: a self-hosted metasearch engine with a JSON API, no API key, runs in Docker, aggregates Google/Bing/DDG). Concretely: add a `SearchBackend` variant that talks to a locally-running search service's HTTP endpoint, wire its configuration (a URL rather than an API key) through vault/settings/env plumbing, register the tool when it's configured, probe it in `fabro doctor`, document it, and test it.
  > 
  > ## 3. What I know for certain
  > 
  > - **Selection is credential-presence-driven**: `SearchBackend::from_secrets` matches on `(brave_key, venice_key)` tuples; the tool exists iff one is present. A local backend that needs *no key* breaks this assumption and needs a different enable signal.
  > - **The tool executes in the Fabro process**, not in the run sandbox — `web_search.rs` uses `fabro_http` directly (the `ToolContext` sandbox handle is unused). So "runs locally" means *reachable from the Fabro server/worker process* (e.g. a compose sibling service), not from run containers.
  > - **Result shape is already normalized**: both backends map into `SearchHit { title, url, description, date }` → `format_hits`. SearXNG's JSON (`results[].title/url/content/publishedDate`) maps onto this cleanly.
  > - **Both existing backends are simple reqwest calls** against a `search_url` field that is already injectable per-instance — adding a variant that takes a configured base URL requires no architectural change.
  > - **Brave/Venice costs are real**: Venice is per-query paid (the code even parses `x-venice-balance-usd` on 402); Brave's free tier is rate-limited. This motivates the goal.
  > - **The full touchpoint list for any new backend is enumerable**: `SearchBackend` variant + `from_secrets`/`search()`, `ToolSecrets` (or a new config field), env var constants, `secret_registry.rs` classification, `initialize.rs` + `cli.rs` plumbing, `diagnostics.rs` probe + demo string, docs (`integrations/*.mdx`, `tools.mdx`), changelog, unit tests (httpmock pattern is ready), parity-matrix wiring.
  > - **A non-secret config path exists as precedent**: `settings.toml` layers (`SettingsLayer` with `llm`, `server`, `run`, … sections) — LiteLLM's `base_url` lives there. The vault also stores arbitrary strings, so a URL *could* be stored as a pseudo-secret (`fabro secret set SEARXNG_URL ...`) — both patterns have precedent adjacent to this code.
  > - **Docs/internal strategy files govern part of this work**: `server-secrets-strategy.md` explicitly lists `BRAVE_SEARCH_API_KEY`/`VENICE_API_KEY` as vault-provisioned optional secrets; any new secret must be added there and in the registry.
  > 
  > ## 4. What is genuinely ambiguous
  > 
  > 1. **Which local engine?** SearXNG is the obvious candidate (JSON API, keyless, Docker), but Whoogle, YaCy, or scraping DuckDuckGo's lite endpoint directly (zero deployment, but fragile/ToS-questionable) are defensible alternatives. "Not as expensive" doesn't pick one.
  > 2. **"Runs locally" vs. actually free/offline**: SearXNG is a *metasearch* — it still queries Google/Bing/DDG upstreams from the operator's machine (needs internet, can get rate-limited/blocked upstream). A fully local index (YaCy) is a different, much weaker product. Which guarantee does the user actually want?
  > 3. **How is the local backend configured and selected?** Options with real trade-offs: (a) store a URL in the vault like a secret (reuses all existing plumbing, but a URL isn't a secret and the secrets strategy doc semantic gets bent); (b) a `settings.toml` section following the LiteLLM precedent (cleaner semantics, but `ToolSecrets`/`from_secrets` and both plumbings in `initialize.rs`/`cli.rs` need a parallel config path); (c) a plain env var. Two engineers would split here.
  > 4. **Precedence semantics**: when Brave/Venice keys *and* a local backend are all configured, which wins? Current code is a strict Brave > Venice ladder; is local the new default (cheap), the fallback, or explicitly chosen?
  > 5. **Auth on the local service**: SearXNG can be exposed with no auth, a bearer token, or behind a reverse proxy. Do we support an optional key, or keep it keyless-only for v1?
  > 6. **Deployment scope**: docs + compose *example* only, or a first-class service added to the shipped `docker-compose.yaml` / a new `cargo dev` helper? Should the repo bundle a searxng service definition, and should `fabro doctor` probe it like the paid providers?
  > 7. **"Provider" granularity**: a new hardcoded `SearchBackend` variant (consistent with today's code), or a config-driven generic endpoint (any base URL + optional key) that makes "provider" pluggable forever? The first is minimal; the second is a small design shift.
  > 8. **Testing expectations**: httpmock unit tests are clearly expected; but should there be an e2e live test, and if so how does CI obtain a running local instance (spin up SearXNG in Docker in CI? twin-style recorded fixtures?)?
  > 
  > My recommendation when we proceed: SearXNG as a new `SearchBackend` variant, configured via a vault-stored `SEARXNG_URL` (with optional `SEARXNG_API_KEY`), taking precedence over Brave/Venice when present — but items 1, 3, and 4 above are the ones worth confirming before writing code.

## Stage: ask
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > Answers received. Restating for the planning stage:
  > 
  > **Q1 — Search engine: SearXNG (local service).** The new backend talks to a self-hosted SearXNG instance's JSON API (`/search?format=json`, keyless). No scraping fallback.
  > 
  > **Q2 — Configuration: Vault, like the API keys.** The endpoint is stored via `fabro secret set SEARXNG_URL http://...` and flows through the existing vault → `tool_secrets_from_configured_sources` → `ToolSecrets` → `SearchBackend::from_secrets` pipeline. The secret registry and secrets-strategy doc get the new name; a URL-as-vault-entry note goes in the docs.
  > 
  > **Q3 — Precedence: Local wins.** Selection ladder becomes SearXNG (when `SEARXNG_URL` is set) > Brave (`BRAVE_SEARCH_API_KEY`) > Venice (`VENICE_API_KEY`). Doctor's `check_web_search` probes SearXNG first under the same precedence.
  > 
  > **Q4 — Deployment scope: Also ship a compose service.** Full wiring (backend variant, tool registration, diagnostics probe, integrations doc page with run/compose snippets) plus an optional profile-gated or commented `searxng` service in `docker-compose.local.yaml` next to the server, using the host-network or service-name URL.
  > 
  > Decisions I made myself (from the exploration stage, unchanged): map SearXNG results onto the existing `SearchHit`/`format_hits` shape; implement as a new hardcoded `SearchBackend::SearXng` variant consistent with existing code (not a generic pluggable-endpoint system); no required auth (SearXNG local JSON API is keyless — optional bearer key can be a follow-up if someone exposes it); tests are httpmock unit tests in `web_search.rs` following the existing injectable-`search_url` pattern, with the doctor probe and docs updated alongside.

## Stage: triage
- Status: succeeded
- Handler: prompt
- Model: glm-5.3
- Response:
  > The answers give me what I need. Restating my understanding:
  > 
  > **What you want:** A web search capability for Fabro that doesn't carry the per-call API cost of Brave or Venice — something self-hosted on your own infrastructure that the agent/tooling can query, and a plan to wire it in as a first-class provider option.
  > 
  > **Decisions I'm making on your behalf:**
  > 
  > 1. **Local = self-hosted metasearch, not a fully-offline index.** SearXNG is the right fit: it runs as a single container, aggregates from public engines (Google/Bing/DuckDuckGo etc.) server-side, exposes a JSON API, and needs no API keys or paid quotas. A fully-local crawler/index (YaCy-style) has badly degraded result quality and a large operational footprint — I'll note it as a rejected alternative rather than build toward it.
  > 
  > 2. **It goes behind a search-provider abstraction**, so Brave/Venice remain selectable and SearXNG becomes a third peer (config pointing at `http://localhost:8888` by default, overridable for remote instances). No hardcoding SearXNG into call sites.
  > 
  > 3. **Setup story:** a docker compose service / `fabro doctor`-visible health check so "runs locally" is a one-command start, matching how the Docker sandbox provider already assumes an operator-manured daemon.
  > 
  > Nothing in your answers left a fork where two engineers would build materially different things — remaining details (exact config keys, retry behavior) are mine to settle.
  > 
  > {"preferred_next_label": "enough"}

## Stage: plan
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response (12.7 KB; full value: `/tmp/fabro/runtime/blobs/6cc418a013c6c9024c8a4d7e8209d14f1440dd58d15b7ba39acea54a3cbb1590.json`)
  Preview: I have complete knowledge of every touchpoint. Here is the implementation plan.
  
  ---
  
  # Implementation Plan: SearXNG local web-search backend
  
  **Decisions locked in from the human's answers:** SearXNG engine · configured via the vault as `SEARXNG_URL` (value is a base URL like `http://searxng:8888`)…

## Stage: devils
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Response:
  > I attacked the plan against the actual repository. Verification results:
  > 
  > **What held up under attack:**
  > - **Plumbing completeness** — I exhaustively grepped for `ToolSecrets {` construction across all non-test code: exactly two production sites exist (`cli.rs:45` and `initialize.rs:314`). The plan covers both. `handler/llm/api.rs` only *receives* `ToolSecrets` via `with_tool_secrets` from initialize.rs, so it's correctly not in the plan's modify list.
  > - **Precedence change blast radius** — parity_matrix uses `..ToolSecrets::default()` (compiles), `handler/llm/api.rs` test likewise; the two struct literals lacking `..Default::default()` (`config.rs` test, `tools.rs` test) are ordinary implementation-time fixes.
  > - **Vault/registry acceptance** — `Vault::get` returns raw values; `fabro secret set` accepts names classified in `OPTIONAL_VAULT_SECRETS`, which the plan adds. No server-side rejection risk.
  > - **Verification strength** — httpmock tests pin method/path/query params, the 403 hint, and both selection ladders; diagnostics precedence test distinguishes backends by summary string. These fail if the change is wrong.
  > 
  > **Defects found (all cheap to absorb, none requiring redesign):**
  > 1. **The changelog step is factually wrong about repo state.** `docs/public/changelog/2026-09-09.mdx` *already exists* (today's GLM 5.3 entry, already in docs.json nav at line 312). The plan says "create" — a blind `write_file` would destroy the existing entry. It must be **append a section**. Git-reversible and diff-visible, so not blocking.
  > 2. **URL hygiene is under-specified.** The plan handles trailing `/` but not leading/trailing whitespace on the vault value. Unlike opaque API keys, a URL is copy-paste-prone; `Vault::get` does no trimming. One `.trim()` at backend construction prevents a confusing reqwest parse error. One line.
  > 3. **`limit` query param is not a documented SearXNG parameter** (SearXNG ignores unknown params; result count is engine/settings-driven). Harmless to send, but `max_results` then goes unenforced — `format_searxng_results` should truncate client-side with `.take(n)`. Small correctness nuance the implementer should fold in.
  > 
  > **Scope check:** `docker/searxng/settings.yml` is not creep — JSON output format is disabled in default SearXNG, so the approved compose service *cannot work* without it. `.env.example` and changelog entries are repo convention.
  > 
  > None of these would force redoing designed work or reverse an expensive decision — they're corrections the implementer absorbs in the normal flow. The plan's architecture, touchpoint list, precedence wiring, and test strategy are all verified correct against the code as it is.
  > 
  > {"preferred_next_label": "approve"}

## Stage: implement
- Status: succeeded
- Handler: agent
- Model: glm-5.3
- Files touched: /workspace/fabro/.env.example, /workspace/fabro/docker-compose.local.yaml, /workspace/fabro/docker/searxng/settings.yml, /workspace/fabro/docs/internal/server-secrets-strategy.md, /workspace/fabro/docs/public/administration/server-configuration.mdx, /workspace/fabro/docs/public/agents/tools.mdx, /workspace/fabro/docs/public/changelog/2026-09-09.mdx, /workspace/fabro/docs/public/docs.json, /workspace/fabro/docs/public/integrations/brave-search.mdx, /workspace/fabro/docs/public/integrations/searxng.mdx, /workspace/fabro/docs/public/integrations/venice-search.mdx, /workspace/fabro/lib/apps/fabro-server/src/demo/mod.rs, /workspace/fabro/lib/apps/fabro-server/src/diagnostics.rs, /workspace/fabro/lib/components/fabro-agent/README.md, /workspace/fabro/lib/components/fabro-agent/src/cli.rs, /workspace/fabro/lib/components/fabro-agent/src/config.rs, /workspace/fabro/lib/components/fabro-agent/src/tools.rs, /workspace/fabro/lib/components/fabro-agent/src/web_search.rs, /workspace/fabro/lib/components/fabro-workflow/src/pipeline/initialize.rs, /workspace/fabro/lib/foundation/fabro-static/src/env_vars.rs, /workspace/fabro/lib/foundation/fabro-static/src/secret_registry.rs
- Response:
  > Implementation complete. Summary of what changed, file by file:
  > 
  > ## Code
  > 
  > **`lib/foundation/fabro-static/src/env_vars.rs`** — added the `SEARXNG_URL` constant and registered it in the exhaustive single-token test list.
  > 
  > **`lib/foundation/fabro-static/src/secret_registry.rs`** — added `SEARXNG_URL` to `OPTIONAL_VAULT_SECRETS` (both the const list and its test), making `fabro secret set SEARXNG_URL` a vault-accepted name.
  > 
  > **`lib/components/fabro-agent/src/config.rs`** — `ToolSecrets` gains `searxng_url: Option<String>`; `Debug` reports `searxng_search_configured` without printing the URL; redaction test extended.
  > 
  > **`lib/components/fabro-agent/src/web_search.rs`** — the core: new `SearchBackend::SearXng { base_url }` variant; `from_secrets` precedence is now **SearXNG > Brave > Venice**; `search_searxng` GETs `{base}/search?q&format=json&limit` and maps `results[]` (`title`/`url`/`content`/`publishedDate`) through the existing `SearchHit`/`format_hits`; 403 gets the "enable json in search.formats" hint. Tests: precedence (3), URL trim, httpmock happy path (asserts GET + query params), trailing-slash base, 403 hint, client-side truncation, three-way schema parity. 12 new tests.
  > 
  > **`lib/components/fabro-agent/src/tools.rs`**, **`README.md`** — selection doc comments updated (README was a touchpoint the plan missed).
  > 
  > **`lib/components/fabro-workflow/src/pipeline/initialize.rs`** / **`lib/components/fabro-agent/src/cli.rs`** — the two production `ToolSecrets` construction sites now read `SEARXNG_URL` (vault for server runs, process env for CLI runs).
  > 
  > **`lib/apps/fabro-server/src/diagnostics.rs`** — `check_web_search` probes SearXNG first via new `check_searxng_search` (reuses `match_web_search_probe`); not-configured remediation now names all three backends; 3 new precedence/env-isolation tests, 2 updated assertions.
  > 
  > **`lib/apps/fabro-server/src/demo/mod.rs`** — demo doctor fixture string kept in sync.
  > 
  > ## Compose & docs
  > 
  > **`docker/searxng/settings.yml`** (new) — `use_default_settings` + JSON format enabled + limiter off. **`docker-compose.local.yaml`** — `searxng` service behind `profiles: ["search"]`, host `127.0.0.1:8888` → container 8080. **`docs/public/integrations/searxng.mdx`** (new) + nav entry in **`docs.json`**. Updated **`tools.mdx`**, **`brave-search.mdx`**, **`venice-search.mdx`**, **`server-configuration.mdx`**, **`server-secrets-strategy.md`** (with the URL-not-a-secret rationale), **`.env.example`**. **Changelog**: appended a SearXNG section to the *existing* `2026-09-09.mdx` (per the critique — the plan's "create" would have clobbered it).
  > 
  > ## Adaptations from the plan
  > - SearXNG's image listens on container port **8080**, so the service URL is `http://searxng:8080` (plan said 8888); host port 8888 still mapped for standalone testing.
  > - Critique corrections all absorbed: whitespace+slash URL trim, client-side `.take(limit)` truncation, changelog append.
  > - `searxng()` takes `&str` (clippy `-D warnings`); nav entry placed after `venice-search` to keep existing entries stable.
  > 
  > ## Verification
  > `fabro-agent` 654 passed / 0 failed · `fabro-workflow` 1444 / 0 · `fabro-static` 5 / 0 · `fabro-server` diagnostics+demo 43 / 0 (full suite: 889 pass, 6 fail — **verified identical 6 fail at HEAD** via `git stash`; graphviz `dot` missing + storage-path env issues, unrelated). `fmt --check` clean, clippy clean on all touched crates. Docker unavailable here, so compose was validated structurally rather than with `docker compose config`; live SearXNG verification is documented in the integration page.


**First, get the change in front of you.** Nothing has been handed to you, so
find it yourself:

    git --no-pager diff origin/main...HEAD

If `origin/main` does not resolve, use `main` instead. Read whole files with
`git show` or by opening them whenever the diff alone is not enough to judge —
a diff hides the context a line sits in, and most real defects live in that
context.

You are reviewing only. Do not edit, create, or delete any file, and do not
commit. Three other reviewers are working in this same checkout at the same
time, and a write from you would corrupt what they are reading.

Review the change for correctness only.

Look for logic that produces a wrong result: off-by-one errors, inverted
conditions, unhandled nil or empty cases, incorrect operator precedence, wrong
variable used, race conditions, resource leaks.

For each finding give the file, the line if you can identify it, what is wrong,
and the concrete input or state that triggers it. A finding you cannot make
fail is a guess — label it as such or drop it.

If the diff is correct, say so plainly. Do not invent findings to seem useful.
