I have a complete picture now. Here's my report.

## 1. Repository structure and where the change lands

**Stack**: Rust workspace (`lib/apps/*`, `lib/components/*`, `lib/foundation/*`) + TypeScript web app (`apps/fabro-web`, Bun). Config is layered TOML (`lib/foundation/fabro-config`, `defaults.toml` + `settings.toml` + env) with secrets in a vault (`fabro secret set`). Docs are Mintlify MDX under `docs/public` with `docs.json` nav and per-date changelogs. Deployment is Docker (`Dockerfile`, `docker-compose*.yaml` with a single `fabro` service that mounts `docker.sock`).

**Web search today — the exact touch points**:

| Concern | Location |
|---|---|
| Backends + executor | `lib/components/fabro-agent/src/web_search.rs` — `SearchBackend` enum (`Brave`, `Venice`), `from_secrets()`, `search_brave()`/`search_venice()`, shared `SearchHit` formatting |
| Credential struct | `lib/components/fabro-agent/src/config.rs` — `ToolSecrets { brave_search_api_key, venice_api_key }` |
| Server/worker population | `lib/components/fabro-workflow/src/pipeline/initialize.rs::tool_secrets_from_configured_sources` — reads **vault only** |
| Standalone CLI population | `lib/components/fabro-agent/src/cli.rs::cli_tool_secrets` — reads **process env** |
| Env var registry | `lib/foundation/fabro-static/src/env_vars.rs` (+ a test enumerating every constant) and `secret_registry.rs` (`OPTIONAL_VAULT_SECRETS`) |
| Tool registration | `fabro-agent/src/tools.rs::register_web_search_tool`; Claude5 has a second wrapper in `profiles/claude5_tools.rs` that reuses the executor but pins its own definition; `NativeTool::WebSearch` maps wire names |
| System prompts | `profiles/prompts/*.md.j2` gate on `has_web_search` = "registry contains the tool" — **backend-agnostic** |
| Doctor | `lib/apps/fabro-server/src/diagnostics.rs::check_web_search` (Brave → Venice probe; generic `CheckResult`, no OpenAPI change needed); demo-mode row in `demo/mod.rs` |
| Tests | In-module `httpmock` tests in `web_search.rs` (URL fields mutated directly); `tests/it/parity_matrix.rs::web_search_provider_test!` (live, requires `BRAVE_SEARCH_API_KEY`); handler test in `fabro-workflow/src/handler/llm/api.rs` |
| Docs | `docs/public/agents/tools.mdx`, `docs/public/integrations/{brave-search,venice-search}.mdx`, `docs.json`, `server-configuration.mdx`, `.env.example` |

**Relevant history**: Venice was added in `53efde39` *with* a `[server.integrations.search] provider = "venice"` override; `88ed2ac9` then deliberately removed that config section in favor of pure credential-presence selection (Brave > Venice > none). That's the strongest signal about the maintainers' preferred selection model — and a warning that a URL-based (non-credential) backend will need a deliberate answer to "how is it selected".

## 2. Goal restated concretely

Add a third `web_search` backend in `fabro-agent`'s `SearchBackend` enum that is **self-hosted/local — no per-query cost** — most plausibly SearXNG (the de-facto self-hosted metasearcher with a JSON API: `GET /search?q=...&format=json` returning `results[].{title,url,content}`). Because it has no API key, selection can't ride the existing credential-presence rule; it needs a base-URL config input flowing vault/env → `ToolSecrets` (or a successor struct) → `from_secrets()` → registration → doctor probe → docs → an optional compose sidecar. The tool name, schema, output format, and prompt gating should stay identical so nothing downstream notices the backend swap.

## 3. What I know for certain

- Selection is strictly **credential presence**: Brave key wins over Venice key; no key ⇒ tool not registered and prompts omit it. Once selected, a failed call errors — **no fallback retry** to the other backend (documented behavior with tests).
- Venice costs **$0.01/request, 20 req/min** (documented in `venice-search.mdx`) and rejects queries >400 chars pre-HTTP; Brave's free tier is rate-limited — this is the cost motivation behind the ask.
- The `web_search` executor runs HTTP **from the Fabro server/worker process** (via shared `fabro_http::http_client()` with proxy policy; `test_http_client()` for tests) — not inside the run sandbox. So a local SearXNG at `http://searxng:8888` / `localhost` is reachable the same way Brave is.
- Server reads search keys **from the vault only** (workers start from a cleared env); only the standalone agent CLI reads process env. Any new input must respect that split or extend it explicitly.
- `SearchBackend` variants carry `search_url` as a mutable field used by in-module httpmock tests — adding a variant gets a cheap, established test pattern for free.
- Prompt templates, the permission classification (`shell` category / `full` level), and the diagnostics API schema are all backend-agnostic — a same-schema backend needs **no** prompt, permission, or OpenAPI changes.
- A `[server.integrations.search]` config section existed and was removed; today `ServerIntegrationsLayer` only has `github` and `slack`. Base-URL env-var precedents exist for other integrations (`OPENAI_BASE_URL`, `GITHUB_BASE_URL`, `SLACK_BASE_URL`, `DAYTONA_API_URL` — all process-env, none vaulted).
- No existing SearXNG/local-search code, docs, or brainstorm anywhere in the repo; no "search twin" for e2e tests.

## 4. What is genuinely ambiguous

1. **What "local" means.** (a) Locally-*operated* metasearch (SearXNG — free, but it still queries Google/Bing/DDG upstream, so it's not offline and inherits their rate-limiting/blocking of your instance); (b) a truly local index (YaCy/Tantivy — offline but drastically worse "web" results); (c) merely "no paid API" (DDG/HTML scraping inside the Fabro process — no new service, but fragile and ToS-risky). These produce very different software.
2. **Engine choice.** SearXNG vs Whoogle vs 4get vs a bespoke scraper. SearXNG is the only one with a stable JSON API and broad adoption, but that's a judgment call, not a fact in the instruction. Also: SearXNG needs `formats: [json]` enabled in its `settings.yml` — a real deployment gotcha that shapes the docs/setup story.
3. **Selection precedence.** Where does a local backend rank when Brave/Venice keys are *also* present? Local-first (cheapest default) contradicts the current "Brave always wins" documentation; remote-first makes local only a fallback — which collides with the documented, tested **no-fallback** rule. And does a bare URL (no credential to validate) register the tool at all, or should reachability gate registration?
4. **Config channel for a URL.** Vault entry (`fabro secret set SEARXNG_URL` — the vault is a generic string store, but a URL isn't a secret and `DAYTONA_API_URL`-style precedents are process-env, which the server ignores for search today) vs. a revived `[server.integrations.search]` section (explicitly deleted once) vs. a new env var plumbed into `initialize.rs` vs. `defaults.toml`. Each has a defensible precedent in this repo.
5. **Lifecycle ownership.** Does Fabro *ship and manage* the SearXNG container (compose service, healthcheck, `fabro install`/doctor integration — a meaningful new orchestration surface), or is it bring-your-own (user runs an instance, points Fabro at a URL, docs-only)? The compose files currently run a single service.
6. **Generic vs. specific backend.** A SearXNG-specific variant vs. a generic "OpenSearch-compatible/base-URL" backend that could later cover other self-hosted engines — affects naming (`SEARXNG_URL` vs `FABRO_SEARCH_URL`), doctor strings, and future extensibility.
7. **Surface fidelity.** Keep the identical tool schema (zero prompt/parity churn — clearly the low-risk path) vs. exposing backend-specific knobs (SearXNG `categories`, `language`, `time_range`, optional bearer token for rate-limited instances). Two definition sites (`tools.rs` and `claude5_tools.rs`) must stay in sync either way.
8. **Failure semantics.** Keep "selected backend fails ⇒ error" (consistent, documented) vs. making local a latency-tolerant fallback — the latter changes documented behavior and needs its own tests.
9. **Test depth.** In-module httpmock tests are the obvious minimum; unclear whether maintainers would also want a docker-based integration test against a real SearXNG, or live parity-matrix coverage (there is no search twin today).
10. **Result normalization details.** SearXNG `results[]` mixes web results with `infoboxes`/`answers` and lacks a date field on many entries — how much of that to surface in the `SearchHit` shape (and whether `max_results` maps to a count param or client-side truncation) is unspecified.