I now have a complete picture. Here is my understanding report.

---

## 1. Repository structure and where the change lands

**Fabro** is an AI workflow orchestration platform: Rust workspace (`lib/apps/*`, `lib/components/*`, `lib/foundation/*`), a React SPA (`apps/fabro-web`, Bun), Mintlify docs (`docs/public`, plus internal strategy docs in `docs/internal`). Testing via `cargo nextest`, `httpmock` for HTTP fakes, `insta` snapshots, and `#[e2e_test]` macros for live/twin modes. Pinned nightly-2026-04-14 for fmt/clippy.

Web search today is a **single `web_search` agent tool** with a two-backend enum. The complete surface a new backend touches:

| File | Role |
|---|---|
| `lib/components/fabro-agent/src/web_search.rs` | `SearchBackend` enum (`Brave` \| `Venice`), `from_secrets()` selection, HTTP calls, result formatters, tool definition, httpmock tests |
| `lib/components/fabro-agent/src/config.rs` | `ToolSecrets { brave_search_api_key, venice_api_key }` |
| `lib/components/fabro-agent/src/tools.rs` | `register_web_search_tool()` — registers only when a backend exists |
| `lib/components/fabro-agent/src/profiles/*` (`mod.rs`, `claude5_tools.rs`, `gpt56.rs`) | system-prompt gating on search availability |
| `lib/components/fabro-workflow/src/pipeline/initialize.rs:314` | run workers read both keys from the **server vault** |
| `lib/components/fabro-agent/src/cli.rs:44` | standalone CLI reads both keys from **process env** |
| `lib/apps/fabro-server/src/diagnostics.rs` | `fabro doctor` `check_web_search`: vault-only secret resolution, live HTTP probe of Brave then Venice |
| `lib/foundation/fabro-static/src/env_vars.rs` + `secret_registry.rs` | env-var constants; both listed as `OPTIONAL_VAULT_SECRETS` |
| `lib/apps/fabro-server/src/demo/mod.rs:675` | demo-mode diagnostics fixture |
| `docs/public/agents/tools.mdx`, `docs/public/integrations/{brave,venice}-search.mdx`, changelog | operator docs |
| `lib/components/fabro-agent/tests/it/parity_matrix.rs`, `tools.rs` live e2e | tests gated on `BRAVE_SEARCH_API_KEY` |

**Relevant history**: Venice was added in `53efde39` *with* a `[server.integrations.search] provider` config knob; `88ed2ac9` then **removed** that knob in favor of pure credential-presence selection ("which vault key exists"). So today there is deliberately **no config surface** for search — selection is credentials-only, Brave > Venice precedence.

## 2. Goal restated concretely

Add a third `web_search` backend — almost certainly **SearXNG**, the standard self-hostable metasearch engine with a JSON API and no per-query billing — so that operators can point Fabro at a locally-running search instance instead of paying Brave/Venice per query. Concretely: a new `SearchBackend` variant with a configurable endpoint URL (not an API key), selected when configured, wired through vault/env plumbing, doctor diagnostics, secret/settings registries, docs, and tests.

## 3. What I know for certain

- The tool is one `web_search` tool with a fixed schema (query + max_results, default 5/max 20); backends are invisible to the agent. Output format is numbered title/URL/description lines via shared `format_hits()`.
- Backend selection is `SearchBackend::from_secrets()` — Brave key wins over Venice key; no key → tool not registered at all.
- Secrets flow differs by process: run workers read the **vault only** (env is cleared/allowlisted per `server-secrets-strategy.md`); the standalone agent CLI reads process env; `fabro exec` has no vault.
- `search_url` for both backends is a compile-time const, mutable only in tests (httpmock pattern: construct backend, mutate URL field, assert wire shape).
- Backend-specific quirks exist per variant: Venice enforces a 400-char query cap pre-HTTP, a 1-min timeout, maps 402 to balance-header errors.
- `fabro doctor` probes the selected backend with a real HTTP request and reports `brave: configured and reachable` etc.; missing keys is a Warning, not an error.
- The codebase explicitly prefers vault-only optional integrations for the server process and rejects ad-hoc env fallbacks (`ServerSecrets` scope rules in `docs/internal/server-secrets-strategy.md`).
- There is zero existing mention of SearXNG/serper/tavily/DDG anywhere in the repo.
- A `server.integrations.<name>` settings layer exists (github, slack) with `{{ secrets.NAME }}` interpolation — a worked precedent for non-secret config + secret credentials.
- The packaged deployment is docker-compose with a single `fabro` service mounting the Docker socket; the repo has `cargo dev docker-build` and compose variants (tailscale, split-web, local).

## 4. What is genuinely ambiguous

1. **What "runs locally" means.** SearXNG is a *metasearch proxy* — the aggregator runs locally but it still queries Google/Bing/DDG upstream (free, keyless, but network-dependent and subject to upstream blocking). A truly local index (YaCy) is a different, much worse product. The user may expect a fully offline index; two engineers would build different things depending on that reading.

2. **Config surface for a URL that is not a secret.** Options with real precedent tension:
   - Store a pseudo-secret in the vault (e.g. `SEARXNG_URL`) → fits the credentials-only selection model and `from_secrets`, but stretches "secret" and hides a URL operators would want to see in settings.
   - A `[server.integrations.search]`-style settings table → fits the github/slack precedent, but `88ed2ac9` deliberately deleted exactly that knob weeks ago; reintroducing it partially reverts a maintainer decision.
   - Env-var-only for both CLI and server → contradicts the vault-only strategy for server integrations.

3. **Selection precedence.** If SearXNG URL *and* Brave/Venice keys are both present: local-first (cheapest wins) or paid-first (quality wins, current Brave-first philosophy)? Also: should the tool fail over across backends on error? (Currently it does not, and that's documented behavior.)

4. **Scope of "set something up":** client support only (operator runs their own SearXNG), a bundled compose sidecar service in `docker-compose.yaml`, or Fabro itself spawning/managing a container (like the sandbox provider does)? Each is a materially different deliverable with different ops/security review needs.

5. **SearXNG deployment variance.** JSON output (`format=json`) is disabled by default in stock SearXNG `settings.yml` and public instances often block it; auth is optional. Should Fabro require JSON enabled, support an optional API key, and how should doctor diagnose a 403 "JSON disabled" vs "unreachable"?

6. **Doctor semantics for a local service.** Is a down local SearXNG a Warning (like missing keys) or an Error? Does the probe hit `/healthz` vs a real query?

7. **Whether `fabro exec`/standalone CLI support matters** for this backend (env-var read) or it's server-deployment-only, given the goal framing is about cost on a server.

My recommendation for stage 2 would be: SearXNG client backend in `web_search.rs` with a vault/settings-resolved URL and explicit precedence, plus compose-sidecar docs — but items 2, 3, and 4 need a decision before implementation.