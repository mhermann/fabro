I have a complete picture now. Here's my analysis.

## 1. Repository structure and where the change lands

**Fabro** is an AI workflow orchestration platform: Rust workspace (`lib/apps/*`, `lib/components/*`, `lib/foundation/*`) + React SPA (`apps/fabro-web`), OpenAPI-first API, Mintlify docs in `docs/public`, internal strategy docs in `docs/internal`. Tests via `cargo nextest`, `insta` snapshots, `httpmock` for HTTP backends, `#[e2e_test]` macro for live/twin modes.

**Web search today** (the exact surface this goal touches):

- `lib/components/fabro-agent/src/web_search.rs` — the entire backend layer. A `SearchBackend` enum with `Brave { api_key, search_url }` and `Venice { api_key, search_url }` variants; each does one HTTP call and maps the JSON into a shared `SearchHit { title, url, description, date }` list. One tool schema regardless of backend.
- Selection is **credential-presence-driven**: `SearchBackend::from_secrets(&ToolSecrets)` — Brave key wins, else Venice key, else the tool isn't registered. No fallback between providers on failure. Notably, an explicit `[server.integrations.search] provider = "venice"` config knob was added with Venice (commit `53efde39`) and then **deliberately removed** in `88ed2ac9` in favor of pure credential-driven selection — a recent, intentional design decision.
- Secrets plumbing: `ToolSecrets { brave_search_api_key, venice_api_key }` (`config.rs`) filled from the vault in server/worker runs (`fabro-workflow/src/pipeline/initialize.rs:314`) or process env in the standalone agent CLI (`cli.rs:44`). Worker env is scrubbed via `WORKER_ENV_ALLOWLIST`, so anything new must flow through the vault/ToolSecrets path.
- Registration: `register_web_search_tool` (`tools.rs:91`), called by every provider profile (anthropic, claude5, openai, gemini, gpt56, kimi). Prompt guidance auto-tracks registration via `WEB_SEARCH_TOOL_NAME`, so availability and prompt text can't drift.
- Observability: `fabro doctor` → `check_web_search` (`fabro-server/src/diagnostics.rs:760`) probes the active backend; demo mode has a canned status; remediation strings name both keys.
- Names/classification: `EnvVars` consts + `secret_registry.rs` (BRAVE/VENICE = OptionalVault) in `fabro-static`; `.env.example`.
- Docs: `docs/public/integrations/{brave,venice}-search.mdx`, `agents/tools.mdx`, `administration/server-configuration.mdx`, `docs.json` nav, changelog entries.

**Useful precedents for a "local" provider:**

- URL-style, non-secret integration config exists: `DAYTONA_API_URL`, `OPENAI_BASE_URL`, `SLACK_BASE_URL`, `GITHUB_BASE_URL` — resolved via `config_env_lookup` (config layers + env) with a `DEFAULT_*` const fallback, and deliberately *not* classified as vault secrets. This is the natural shape for "point at my local service."
- Docker is already a first-class operator dependency (sandbox provider mounts `docker.sock`; multiple compose files), so "run a local search container" fits the deployment model.
- `fabro-http` provides the shared proxy-aware client (test client disables proxies — matters for localhost).
- The two Venice commits give the exact touchpoint checklist for any new backend: `web_search.rs`, `config.rs`, `cli.rs`, `tools.rs`, `initialize.rs`, `diagnostics.rs`, `demo/mod.rs`, `env_vars.rs`, `secret_registry.rs`, docs set, `.env.example`, tests (`parity_matrix.rs` currently gates web-search tests on a live `BRAVE_SEARCH_API_KEY`).
- No references anywhere to searxng/duckduckgo/tantivy — this is greenfield.

## 2. Goal restated concretely

Add a third `web_search` backend in `fabro-agent`'s `web_search.rs` that calls a **self-hosted, zero-per-query-cost search service running on the operator's own infrastructure** (the canonical candidate is a SearXNG instance, which aggregates public engines behind a free local JSON API), selected via a **URL-style configuration** rather than a paid API key — following the `DAYTONA_API_URL` precedent — while preserving the existing contract: same single tool schema, same `SearchHit` formatting, same registration gating (tool omitted when unconfigured), same no-fallback-on-failure rule, plus the standard surround: `fabro doctor` probe, vault/config plumbing, env var const, docs page + nav + changelog, and httpmock unit tests.

## 3. What I know for certain

- The tool contract is stable and backend-agnostic: `query` + `max_results` in, numbered title/URL/description/date out. A new backend is one enum variant + one HTTP function + one formatter.
- Backend selection today is *only* credential presence (Brave > Venice > unregistered), and the team recently removed explicit provider config to keep it that way.
- Server runtime is vault-only for integration secrets; worker env is scrubbed — new config must flow through vault or `config_env_lookup`-style resolution at pipeline init, and through `ToolSecrets` (or its successor) into profiles.
- `fabro doctor`, demo mode, `secret_registry`, `EnvVars`, `.env.example`, and the docs set must all be updated in lockstep; the Venice commits are the exact blueprint.
- All backends are tested with `httpmock` by overriding the `search_url` field — a local-URL backend is trivially testable that way, and would even let the parity-matrix web-search tests run without a paid Brave key.
- Docker is already an operator-managed dependency; compose files exist for local, prod, split-web, tailscale deployments.
- Nothing local-search-related exists in the repo today.

## 4. Genuinely ambiguous — where two engineers would build different things

1. **What "runs locally" means.** Three very different builds: (a) Fabro only *points* at an operator-run SearXNG URL (thin client, user runs the container); (b) Fabro *manages* the container lifecycle (spawn/health-check a SearXNG sidecar via Docker, like sandbox containers); (c) in-process search (bundled index like Tantivy, or scraping DuckDuckGo HTML) — zero deploy but fragile and ToS-risky. The instruction "set something up that runs locally" most suggests (a) or (b), and they differ enormously in scope.
2. **Which engine.** SearXNG is the obvious free metasearch choice, but it still queries upstream public engines (which can rate-limit/block a server IP). A true local index is a far larger project with much worse freshness. Result quality directly affects agent usefulness.
3. **Selection precedence.** Does a configured local URL *win* over paid keys (cost-first), *lose* (quality-first), or require explicit opt-in? Current philosophy is credential-presence, and a URL is not a credential — this may force partially reintroducing the explicit provider knob that was deliberately removed in `88ed2ac9`, or inventing a "credential-shaped" URL entry.
4. **Config mechanism and bucket.** Env-var + config-layer lookup (`SEARXNG_URL`-style, like `DAYTONA_API_URL`)? Vault entry? New `[server.integrations.search]` section? Per-workflow override? And does the standalone CLI agent (env-only secrets today) support it?
5. **Auth on the local instance.** Local SearXNG may be open on the LAN or protected by a bearer token. Keyless-only, or optional key? Optional key complicates the "presence = enabled" selection logic.
6. **Fallback semantics.** Current rule: no fallback between providers. For cost optimization, some would want local→paid fallback on failure; that contradicts existing behavior and needs an explicit call.
7. **Deployment story in-repo.** Do we ship a SearXNG service in `docker-compose.local.yaml`/docs, have `fabro install` offer setup, or leave deployment entirely to the operator docs page?
8. **Result mapping.** SearXNG returns multi-engine results with `score`, `engines`, `publishedDate` — dedupe/rank across engines, or take raw ordering? Does date map to `SearchHit.date`?
9. **Test posture.** Should parity-matrix web-search tests move from live-Brave-gated to local-mock-based (a nice side effect), or keep live coverage for every backend?

The dominant fork is ambiguity #1 — everything else is secondary to deciding whether Fabro is a thin client to an operator-run service or takes on managing the search infrastructure itself.