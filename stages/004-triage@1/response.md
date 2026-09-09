I have enough to plan. Here's what I understand the human wants, plus the decisions I'm making on their behalf:

**The goal:** Add a web search provider to Fabro that has no per-query cost (unlike Brave's and Venice's paid APIs), by running the search capability locally/cheaply under our control.

**Decisions I'm making:**

- **Engine: SearXNG.** It's the de facto standard answer for self-hosted search — a metasearch aggregator that queries public engines (Google, Bing, DuckDuckGo, Wikipedia, etc.) with no API keys, and exposes a JSON API (`/search?q=...&format=json`). Alternatives (Whoogle has no first-class JSON API, YaCy has poor result quality) don't hold up. Zero marginal cost per query.
- **Deployment: Docker.** The repo is already Docker-first (default sandbox provider, packaged compose service). I'll treat SearXNG as an optional compose service / documented `docker run` (with `json` format enabled in `settings.yml`), not something Fabro auto-spawns. Operators point the provider config at a base URL; a token is optional (only needed if they enable SearXNG's limiter).
- **Integration:** Implement it as a new provider in the existing web-search provider abstraction alongside Brave and Venice — config is just a base URL, and SearXNG's JSON response maps onto the same normalized result type so agent/server call sites don't change. Provider selection follows whatever pattern Brave/Venice already use (enum + config, strum-derived per repo convention).
- **Honest caveat baked into docs:** "local" means locally-operated proxying — SearXNG still hits upstream public engines, so it's free but not hermetic. That's the accepted tradeoff and exactly why it's cheaper than Brave/Venice.

The remaining details (result count, timeout/retry behavior, which upstream engines to enable in `settings.yml`) are implementation choices I can settle during planning without another round.

{"preferred_next_label": "enough"}