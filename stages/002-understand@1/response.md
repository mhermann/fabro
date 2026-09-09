I have a complete picture now. Here's my exploration report.

## 1. Repository structure and where the change lands

**Fabro** is an AI workflow orchestration platform: Rust workspace (`lib/apps`, `lib/components`, `lib/foundation`) plus TypeScript apps (`apps/fabro-web`, `apps/marketing`). Workflows are Graphviz graphs; each node is a stage run by an AI agent with a tool registry.

The web search feature is concentrated in a small, well-isolated set of files:

| Concern | Location |
|---|---|
| Backend enum + HTTP + result formatting | `lib/components/fabro-agent/src/web_search.rs` (`SearchBackend::{Brave, Venice}`) |
| Tool registration (`web_search` tool, registered only when a credential exists) | `lib/components/fabro-agent/src/tools.rs` (`register_web_search_tool`) |
| Secrets struct | `lib/components/fabro-agent/src/config.rs` (`ToolSecrets`) |
| Vault → secrets for server runs | `lib/components/fabro-workflow/src/pipeline/initialize.rs` (`tool_secrets_from_configured_sources`) |
| Process-env → secrets for CLI runs | `lib/components/fabro-agent/src/cli.rs` |
| Env var name registry / secret classification | `lib/foundation/fabro-static/src/env_vars.rs`, `secret_registry.rs` |
| Doctor diagnostics probe | `lib/apps/fabro-server/src/diagnostics.rs` (`check_web_search` → `check_brave_search` / `check_venice_search`) |
| Demo-mode doctor string | `lib/apps/fabro-server/src/demo/mod.rs` |
| Docs | `docs/public/integrations/brave-search.mdx`, `venice-search.mdx`, `agents/tools.mdx`, changelog |
| Tests | Unit tests in `web_search.rs` (httpmock), `tests/it/parity_matrix.rs` (`web_search_provider_test!`), e2e live test gated on `BRAVE_SEARCH_API_KEY` |

Precedents that matter: each `SearchBackend` variant already carries an injectable `search_url` (used by tests via httpmock), and the repo already has a **self-hosted proxy pattern** — the LiteLLM provider is documented as a local service configured via `settings.toml` with a `base_url`. The repo also ships multiple `docker-compose*.yaml` files and a `docker/` dir, so running a sibling local service is an established deployment shape.

## 2. Goal restated concretely

Today the `web_search` agent tool has exactly two backends — Brave (paid/rate-limited API) and Venice (balance-based paid API) — selected purely by which API key is in the vault (Brave wins when both exist; no key ⇒ the tool is omitted from the agent's toolset and prompt). The goal is to add a **third backend that costs nothing per query because it runs as a local/self-hosted service** the operator deploys (the ecosystem-standard answer is SearXNG: a self-hosted metasearch engine with a JSON API, no API key, runs in Docker, aggregates Google/Bing/DDG). Concretely: add a `SearchBackend` variant that talks to a locally-running search service's HTTP endpoint, wire its configuration (a URL rather than an API key) through vault/settings/env plumbing, register the tool when it's configured, probe it in `fabro doctor`, document it, and test it.

## 3. What I know for certain

- **Selection is credential-presence-driven**: `SearchBackend::from_secrets` matches on `(brave_key, venice_key)` tuples; the tool exists iff one is present. A local backend that needs *no key* breaks this assumption and needs a different enable signal.
- **The tool executes in the Fabro process**, not in the run sandbox — `web_search.rs` uses `fabro_http` directly (the `ToolContext` sandbox handle is unused). So "runs locally" means *reachable from the Fabro server/worker process* (e.g. a compose sibling service), not from run containers.
- **Result shape is already normalized**: both backends map into `SearchHit { title, url, description, date }` → `format_hits`. SearXNG's JSON (`results[].title/url/content/publishedDate`) maps onto this cleanly.
- **Both existing backends are simple reqwest calls** against a `search_url` field that is already injectable per-instance — adding a variant that takes a configured base URL requires no architectural change.
- **Brave/Venice costs are real**: Venice is per-query paid (the code even parses `x-venice-balance-usd` on 402); Brave's free tier is rate-limited. This motivates the goal.
- **The full touchpoint list for any new backend is enumerable**: `SearchBackend` variant + `from_secrets`/`search()`, `ToolSecrets` (or a new config field), env var constants, `secret_registry.rs` classification, `initialize.rs` + `cli.rs` plumbing, `diagnostics.rs` probe + demo string, docs (`integrations/*.mdx`, `tools.mdx`), changelog, unit tests (httpmock pattern is ready), parity-matrix wiring.
- **A non-secret config path exists as precedent**: `settings.toml` layers (`SettingsLayer` with `llm`, `server`, `run`, … sections) — LiteLLM's `base_url` lives there. The vault also stores arbitrary strings, so a URL *could* be stored as a pseudo-secret (`fabro secret set SEARXNG_URL ...`) — both patterns have precedent adjacent to this code.
- **Docs/internal strategy files govern part of this work**: `server-secrets-strategy.md` explicitly lists `BRAVE_SEARCH_API_KEY`/`VENICE_API_KEY` as vault-provisioned optional secrets; any new secret must be added there and in the registry.

## 4. What is genuinely ambiguous

1. **Which local engine?** SearXNG is the obvious candidate (JSON API, keyless, Docker), but Whoogle, YaCy, or scraping DuckDuckGo's lite endpoint directly (zero deployment, but fragile/ToS-questionable) are defensible alternatives. "Not as expensive" doesn't pick one.
2. **"Runs locally" vs. actually free/offline**: SearXNG is a *metasearch* — it still queries Google/Bing/DDG upstreams from the operator's machine (needs internet, can get rate-limited/blocked upstream). A fully local index (YaCy) is a different, much weaker product. Which guarantee does the user actually want?
3. **How is the local backend configured and selected?** Options with real trade-offs: (a) store a URL in the vault like a secret (reuses all existing plumbing, but a URL isn't a secret and the secrets strategy doc semantic gets bent); (b) a `settings.toml` section following the LiteLLM precedent (cleaner semantics, but `ToolSecrets`/`from_secrets` and both plumbings in `initialize.rs`/`cli.rs` need a parallel config path); (c) a plain env var. Two engineers would split here.
4. **Precedence semantics**: when Brave/Venice keys *and* a local backend are all configured, which wins? Current code is a strict Brave > Venice ladder; is local the new default (cheap), the fallback, or explicitly chosen?
5. **Auth on the local service**: SearXNG can be exposed with no auth, a bearer token, or behind a reverse proxy. Do we support an optional key, or keep it keyless-only for v1?
6. **Deployment scope**: docs + compose *example* only, or a first-class service added to the shipped `docker-compose.yaml` / a new `cargo dev` helper? Should the repo bundle a searxng service definition, and should `fabro doctor` probe it like the paid providers?
7. **"Provider" granularity**: a new hardcoded `SearchBackend` variant (consistent with today's code), or a config-driven generic endpoint (any base URL + optional key) that makes "provider" pluggable forever? The first is minimal; the second is a small design shift.
8. **Testing expectations**: httpmock unit tests are clearly expected; but should there be an e2e live test, and if so how does CI obtain a running local instance (spin up SearXNG in Docker in CI? twin-style recorded fixtures?)?

My recommendation when we proceed: SearXNG as a new `SearchBackend` variant, configured via a vault-stored `SEARXNG_URL` (with optional `SEARXNG_API_KEY`), taking precedence over Brave/Venice when present — but items 1, 3, and 4 above are the ones worth confirming before writing code.