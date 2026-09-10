I now have a complete picture. Here's the understanding report.

---

## 1. Repository structure and where the change lands

**Fabro** is an AI workflow orchestration platform. Rust workspace (~40 crates, edition 2021) + a React SPA and generated TS client. Tooling: `cargo nextest`, httpmock for HTTP unit tests, insta snapshots, nightly clippy/rustfmt, Mintlify docs in `docs/public/*.mdx`.

**Current web search architecture** — one tool, two paid backends:

| Layer | File | Role |
|---|---|---|
| Backend impl | `lib/components/fabro-agent/src/web_search.rs` | `SearchBackend` enum `{Brave, Venice}`, `from_secrets()` selection (Brave wins), `search()` dispatch, per-provider formatters feeding a shared `SearchHit {title, url, description, date}` → `format_hits()` |
| Config struct | `lib/components/fabro-agent/src/config.rs` | `ToolSecrets { brave_search_api_key, venice_api_key }` |
| Server-run source | `lib/components/fabro-workflow/src/pipeline/initialize.rs:314` | reads both keys **from the vault** (workers start with cleared env) |
| Standalone CLI source | `lib/components/fabro-agent/src/cli.rs:44` | reads both keys **from process env** |
| Registration | `tools.rs::register_web_search_tool`, `profiles/claude5.rs:47`, `profiles/gpt56.rs` | tool registered only if a backend is selected; same `web_search` schema regardless of backend |
| Env/secret registries | `lib/foundation/fabro-static/src/env_vars.rs`, `secret_registry.rs` | `BRAVE_SEARCH_API_KEY`, `VENICE_API_KEY` are "optional vault secrets" |
| Server diagnostics | `lib/apps/fabro-server/src/diagnostics.rs:760` `check_web_search` | probes the selected backend; remediation text names both secrets |
| Docs | `docs/public/agents/tools.mdx` (web_search section), `docs/public/integrations/{brave-search,venice-search}.mdx`, `docs/public/administration/server-configuration.mdx`, `.env.example`, demo-mode strings in `fabro-server/src/demo/mod.rs:675` |
| Tests | httpmock unit tests in `web_search.rs`; live parity-matrix tests in `fabro-agent/tests/it/parity_matrix.rs` keyed on `BRAVE_SEARCH_API_KEY` |

## 2. Goal restated concretely

Add a **third `SearchBackend` variant** that talks to a search service the operator runs themselves — the canonical candidate being **SearXNG** (self-hosted metasearch aggregator with a JSON API, zero per-request cost) — so `web_search` works without Brave/Venice credentials. It must integrate at every layer the existing backends do: backend enum + formatter, configuration plumbing (vault/env/settings), registration gating, doctor diagnostics, docs, and tests. The agent-visible tool (`web_search`) and its schema stay unchanged; the backend remains an implementation detail selected at session construction.

## 3. What I know for certain

- The insertion point is well-isolated: everything funnels through `SearchBackend::from_secrets(&ToolSecrets)` → one `web_search` tool. A new variant touches one enum, one selection function, one `search()` arm, one formatter (SearXNG's `results[].{title,url,content,publishedDate}` maps almost 1:1 to `SearchHit`).
- Credential flow is strictly two-path: vault for server runs, process env for the standalone CLI. `secret_registry.rs` explicitly classifies URL-style config (`DAYTONA_API_URL`, `OPENAI_BASE_URL`) as **non-secrets** — a local backend's URL is config, not a credential.
- The `web_search` executor runs in the worker/server process (it uses `fabro_http` directly, not the sandbox). So "local" means *reachable from the Fabro server's network position* — inside docker-compose that's the compose network, not host localhost.
- Docker is already central to the deployment model (default sandbox provider, compose files mount `docker.sock`), so a sibling SearXNG container is operationally natural.
- Established invariants to preserve: single tool name/schema; selection happens once, no fallback retry between backends on failure; `web_search` is `Shell` permission category (needs `Full` for auto-approval); server reads secrets from vault only.
- Test patterns exist: httpmock-based request/response assertions per backend, `from_secrets` precedence tests, diagnostics tests that assert remediation strings.

## 4. What is genuinely ambiguous

1. **What "runs locally" means.** (a) Self-hosted SearXNG/Whoogle container the operator runs — truly local service; (b) "no paid key" via free endpoints (DuckDuckGo HTML scraping) — no local service at all, different product. Two engineers would build different things from the same sentence.
2. **Who owns the service lifecycle.** Point at an operator-managed URL (simple config) vs. Fabro launching/managing the SearXNG container itself via the existing Docker integration (much bigger: image pulls, lifecycle, health, cleanup).
3. **Config surface for the URL.** Vault entry (uniform with existing keys, but URLs aren't secrets), new env var (`SEARXNG_URL`), a `settings.toml` section (`[server.web_search]`), or some combination — and `ToolSecrets` may need to become a broader "tool config" struct, which ripples into `ProfileDeps`.
4. **Precedence when multiple backends are configured.** Local-first (free wins, Brave as paid fallback) vs Brave-first (quality wins, local as last resort)? Current code has a strict Brave > Venice order with tests pinning it.
5. **Failure semantics.** Keep "selected backend errors out, no retry through others" (documented behavior) or introduce fallback since a local instance dying is now a plausible failure mode.
6. **SearXNG deployment specifics.** Its JSON output must be enabled in `settings.yml` (`search.formats: [json]`); do we ship a compose profile/settings for it, document it, or auto-manage it? Optional API key for instances with the limiter enabled? Timeout behavior (local instances can be slow aggregating engines)?
7. **Diagnostics/doctor shape.** `check_web_search` maps provider→secret-name; a local backend has a URL to probe and different remediation text ("start your SearXNG instance" vs "run fabro secret set …"). Demo-mode and docs strings all need matching decisions.
8. **Result mapping details.** SearXNG returns richer data (`engines`, `publishedDate`, `score`, possible duplicates across engines) — whether to dedupe, rank, or surface the source engine in the output line.
9. **E2E test scope.** Whether the parity matrix grows a local-provider live variant, and what the CI story is for a service that needs a container running.

No files were modified; this stage was exploration only.