The answers settled the main forks: local/self-hosted is the point (not just "cheaper API"), Docker is an acceptable runtime for it (consistent with the existing Docker-first sandbox model), and this should be a real, configurable provider rather than a hardcoded hack.

What I now understand the human wants:

- A web search backend that runs locally and costs nothing per query, replacing the need to pay Brave/Venice rates for agent-driven search.
- Concretely: a SearXNG instance (metasearch aggregator over Google/Bing/DuckDuckGo/etc.) run as a local Docker service, with a new search provider in the Fabro codebase that talks to its JSON API — wired into configuration (defaults.toml/env) alongside the existing Brave/Venice options, plus tool support in the agent so workflows can actually use it.

Decisions I'm making on their behalf rather than asking again:

- **SearXNG over direct engine scraping** (e.g., raw DuckDuckGo): more stable, no brittle rate-limit dance, and it returns structured JSON we can normalize.
- **Shared long-running service** next to the server (same pattern as the existing compose services), not an ephemeral per-run container — search doesn't need per-run isolation and startup latency would hurt.
- **Opt-in rollout**: existing providers stay the default until the user points config at the local instance; `fabro doctor` gets a check for a reachable SearXNG endpoint, and we document the recommended locked-down settings (JSON format enabled, public instance protections off).

The remaining unknowns (default port, result count, how to map SearXNG fields onto our existing search-result shape) are implementation details I can resolve sensibly without another round.

{"preferred_next_label": "enough"}