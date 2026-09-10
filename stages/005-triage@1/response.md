You've given me enough to build this. Restating what I understand:

**Goal:** Add a web search provider to Fabro that has zero marginal cost per query and runs on the user's own infrastructure, as an alternative to the paid Brave/Venice providers, wired into the existing search abstraction so agent tools and workflow stages can use it unchanged.

**Decisions I'm making on your behalf** (none of these change the shape of the software enough to warrant another round):

1. **Engine: SearXNG.** It's the canonical self-hosted metasearch engine — no API keys, no per-query billing, aggregates Google/Bing/DDG/etc., ships a first-class JSON API (`/search?format=json`), and has a standard Docker image. Building our own crawler/indexer or using Whoogle (no stable JSON API) would be materially worse for the effort.
2. **Integration shape:** a new provider variant alongside Brave/Venice in the existing search-provider enum, configured by base URL (defaulting to `http://localhost:8080`). Fabro talks to the instance's JSON API and normalizes results into the same result struct the other providers emit, so downstream consumers don't change.
3. **Deployment story:** document the one-line `docker run searxng/searxng` (with `SEARXNG_BASE_URL` and JSON format enabled) and add an optional compose service alongside the existing packaged compose file. Fabro does **not** manage the container's lifecycle in code — it's an operator-run local service, consistent with how the Docker sandbox provider treats the daemon as operator infrastructure.
4. **Failure behavior:** if the local instance is unreachable, the search tool fails with a clear actionable error rather than silently falling back to a paid provider — "free and local" was the explicit priority, and silent fallback would bill people without their consent.

If SearXNG's result quality turns out to be a concern, that's tunable per-instance (which engines it aggregates) and doesn't affect the provider code.

{"preferred_next_label": "enough"}