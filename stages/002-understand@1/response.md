I now have a complete picture. Here's my understanding-stage report.

## 1. Repository structure and where the change lands

**Stack/tooling:** Rust workspace (cargo, nextest, pinned nightly rustfmt/clippy) + TypeScript/Bun for the web SPA. Mintlify docs in `docs/public`, internal strategy docs in `docs/internal`. A root `AGENTS.md` (mirror of CLAUDE.md) encodes conventions: strum for enum strings, no glob imports, `shell_quote()` for shell interpolation, OpenAPI-first for server APIs, feature-gated `test-support`, testing strategy doc to consult before adding tests.

**Web search today lives in `fabro-agent`** (`lib/components/fabro-agent/`), specifically:

| Concern | File |
|---|---|
| Backends + tool factory | `src/web_search.rs` — `SearchBackend` enum (`Brave`/`Venice`), `from_secrets()` precedence, `make_web_search_tool()` |
| Secret struct | `src/config.rs` — `ToolSecrets { brave_search_api_key, venice_api_key }` (redacted `Debug`), carried in `NativeToolOptions.secrets` |
| Registration | `src/tools.rs` `register_web_search_tool()` — tool registered **only** when a backend credential exists; `WEB_SEARCH_TOOL_NAME = "web_search"` |
| Profile wiring | `src/profiles/{mod,claude5,claude5_tools,gpt56}.rs` — `with_tool_secrets()`, per-profile tool naming, system-prompt "has web_search" gating |
| Secret sourcing (server path) | `fabro-workflow/src/pipeline/initialize.rs` `tool_secrets_from_configured_sources()` — reads **vault only** (workers start with cleared env) |
| Secret sourcing (CLI path) | `fabro-agent/src/cli.rs` `cli_tool_secrets()` — reads **process env** |
| Env/secret registries | `fabro-static/src/env_vars.rs`, `secret_registry.rs` (`OPTIONAL_VAULT_SECRETS`) |
| Health checks | `fabro-server/src/diagnostics.rs` `check_web_search()` (drives `fabro doctor`), plus a static entry in `demo/mod.rs` |
| Docs | `docs/public/agents/tools.mdx` (web_search section), `docs/public/integrations/{brave,venice}-search.mdx`, permissions doc (web_search = `shell` category, `full` permission) |
| Tests | Unit tests with `httpmock` inside `web_search.rs`, live e2e via `parity_matrix.rs` (`web_search_provider_test!` requiring `BRAVE_SEARCH_API_KEY`), profile prompt snapshots in `profiles/mod.rs` |

**Useful precedents:** URL-style (non-secret) config exists for Daytona (`DAYTONA_API_URL` via `config_env_lookup` in `fabro-server/src/server.rs`); the repo ships multiple `docker-compose*.yaml` files and Docker is already central to the deployment model.

## 2. Goal restated concretely

Add a third `web_search` backend in `fabro-agent` that talks to a search service running on the operator's own machine/network (the obvious candidate is a self-hosted SearXNG instance with its JSON API — free, no per-query billing, no API key), so agents get web search without Brave/Venice per-request cost ($0.01/req, 20 rpm on Venice; metered on Brave). This touches the backend enum, secret/config plumbing (a URL, not a credential), both secret-sourcing paths (vault/env), doctor diagnostics, docs, and tests. The agent-facing contract — one `web_search` tool with `query`/`max_results`, numbered title/URL/description output — should stay identical; only the HTTP backend changes (same pattern the Venice backend already established).

## 3. What I know for certain

- Backend selection is credential-presence-driven: Brave > Venice > tool-not-registered. There is **explicitly no fallback** between backends on failure (documented in both integration pages).
- The tool executes **in the Fabro process** (`search_http_client()` is an in-process reqwest client via `fabro-http`), not inside the sandbox container. So "local" means *reachable from the Fabro server/CLI process*. In compose deployments that means service-name networking, not `localhost` of the host.
- Server runs read search keys from the **vault only** (`fabro secret set`); the standalone agent CLI reads them from process env. Any new provider must be wired into both paths.
- There is **no existing mention** of SearXNG/Serper/Tavily/DDG anywhere in the repo — this is a greenfield backend, not a rework.
- Adding a provider has a known, enumerable blast radius (enum, `ToolSecrets`, env registries, two sourcing paths, diagnostics, demo static check, docs, unit + parity + snapshot tests) — the Venice backend (commit `53efde39`, changelog `2026-08-21.mdx`) is a worked example.
- A local backend needs a **URL setting, not a secret**; `DAYTONA_API_URL` is the established pattern for that (env/settings lookup, deliberately *not* in the vault secret list).
- Permission classification (`shell` category, `full` level for auto-approval) is name-based and unaffected by backend choice.
- Output normalization is centralized (`format_hits`/`SearchHit`), so result-shape differences get absorbed at the formatting boundary.

## 4. What's genuinely ambiguous

1. **What "runs locally" means.** (a) Fabro merely *integrates* an operator-run SearXNG (extra compose service, Fabro points at a URL) — small change; (b) Fabro *manages* the container lifecycle itself, as it does for sandbox containers — much bigger; (c) in-process engineless search (scraping DDG/HTML endpoints directly from Rust) — no local service but fragile and ToS-questionable; (d) a true local index (YaCy/crawler) — a different product. Reasonable engineers would pick very different scopes here.
2. **Precedence semantics.** Where does a local backend rank — always cheapest-first (local beats Brave even when both are configured), or lowest priority, or explicit opt-in only? Today precedence is implicit (credential presence); a URL is trivially "present", so a default-on local backend silently changes behavior for existing users who run SearXNG. Explicit selection vs inference is a real design fork.
3. **Configuration surface.** `SEARXNG_URL` env var following the Daytona precedent? A `settings.toml`/`defaults.toml` key? Vault entry (wrong tool for a non-secret, but consistent with how search is configured today)? Optional bearer token support? Default URL/port at all, or require explicit configuration?
4. **Deployment story.** Does the repo ship a SearXNG service in `docker-compose.yaml` (and which of the four compose variants), as an opt-in compose profile, or just document it? SearXNG needs `formats: [json]` enabled in its own `settings.yml` — who owns that config?
5. **Tool contract details.** Identical schema and output shape (my assumption), or expose backend-specific richness? Does `max_results` map directly? Does the no-fallback-on-failure policy stay, or does a free local backend change the economics of retrying through a paid one?
6. **Testing depth.** httpmock unit tests are obvious; but does the local backend get a live e2e in the parity matrix (requires a running instance — CI cost/complexity), twin-mode scripting, or unit-only coverage?
7. **Doctor/diagnostics semantics.** What does the probe look like when there's no key to validate — just a reachability probe against the URL? What's the remediation text ("start SearXNG and set SEARXNG_URL")? And does the demo-mode static check need a third state?
8. **Failure/quality expectations.** SearXNG aggregates upstream engines that can themselves be rate-limited or degraded; is "cheaper but sometimes flaky" acceptable, and should that surface differently in errors?