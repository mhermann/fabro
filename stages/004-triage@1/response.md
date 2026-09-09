The answers give me what I need. Restating my understanding:

**What you want:** A web search capability for Fabro that doesn't carry the per-call API cost of Brave or Venice — something self-hosted on your own infrastructure that the agent/tooling can query, and a plan to wire it in as a first-class provider option.

**Decisions I'm making on your behalf:**

1. **Local = self-hosted metasearch, not a fully-offline index.** SearXNG is the right fit: it runs as a single container, aggregates from public engines (Google/Bing/DuckDuckGo etc.) server-side, exposes a JSON API, and needs no API keys or paid quotas. A fully-local crawler/index (YaCy-style) has badly degraded result quality and a large operational footprint — I'll note it as a rejected alternative rather than build toward it.

2. **It goes behind a search-provider abstraction**, so Brave/Venice remain selectable and SearXNG becomes a third peer (config pointing at `http://localhost:8888` by default, overridable for remote instances). No hardcoding SearXNG into call sites.

3. **Setup story:** a docker compose service / `fabro doctor`-visible health check so "runs locally" is a one-command start, matching how the Docker sandbox provider already assumes an operator-manured daemon.

Nothing in your answers left a fork where two engineers would build materially different things — remaining details (exact config keys, retry behavior) are mine to settle.

{"preferred_next_label": "enough"}