Both questions answered. Restating for the planning stage:

## Answers

1. **What should the new no-cost web_search source be?**
   **Answer: Self-hosted SearXNG.**
   *What I take it to mean:* Build a third `SearchBackend` variant in `lib/components/fabro-agent/src/web_search.rs` that talks to a SearXNG instance's JSON API (`GET /search?q=...&format=json`), mapping `results[].{title,url,content,publishedDate}` to the existing `SearchHit {title, url, description, date}` shape. No scraping backend, no keyless-endpoint fallback. Docs must call out that the instance needs JSON output enabled in its `settings.yml` and may be rate-limited by upstream engines.

2. **Who sets up and keeps the service running?**
   **Answer: Operator-managed (LiteLLM-style).**
   *What I take it to mean:* No container-lifecycle work in Fabro — no docker.sock management, no auto-start, no health supervision. The change is a backend + configuration + docs: Fabro points at an existing instance via config, and the integration doc explains how to run SearXNG (an optional compose snippet in docs is fine, but nothing in code manages it).

## Decisions already settled (carrying forward, not re-asked)

- **Precedence: SearXNG-first when configured** — `searxng > brave > venice`; users who don't configure it see zero change. Cost is the motivation, so a paid key must not win over a configured local instance.
- **No fallback between backends on failure** — existing documented/tested invariant; a dead SearXNG errors the call, same as a dead Brave key.
- **No provider enum / settings section** — commit `88ed2ac9` deliberately removed `[server.integrations.search]`; selection stays presence-based.
- **Config surface:** new vault key (e.g. `SEARXNG_URL`, set via `fabro secret set`) for server runs, process env for the standalone CLI — mirroring the existing two keys exactly; no OpenAPI/TS-client changes.
- **Unchanged:** tool name/schema, permission category (Shell), events, result output format.
- **Also included:** doctor/diagnostics support for the new backend, demo-mode string update, docs (`tools.mdx`, new `integrations/searxng.mdx` + `docs.json` nav, `server-configuration.mdx`, changelog entry), httpmock unit tests + precedence tests mirroring the Brave/Venice suites.