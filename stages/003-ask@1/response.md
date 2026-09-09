Answers received. Restating for the planning stage:

**Q1 — Search engine: SearXNG (local service).** The new backend talks to a self-hosted SearXNG instance's JSON API (`/search?format=json`, keyless). No scraping fallback.

**Q2 — Configuration: Vault, like the API keys.** The endpoint is stored via `fabro secret set SEARXNG_URL http://...` and flows through the existing vault → `tool_secrets_from_configured_sources` → `ToolSecrets` → `SearchBackend::from_secrets` pipeline. The secret registry and secrets-strategy doc get the new name; a URL-as-vault-entry note goes in the docs.

**Q3 — Precedence: Local wins.** Selection ladder becomes SearXNG (when `SEARXNG_URL` is set) > Brave (`BRAVE_SEARCH_API_KEY`) > Venice (`VENICE_API_KEY`). Doctor's `check_web_search` probes SearXNG first under the same precedence.

**Q4 — Deployment scope: Also ship a compose service.** Full wiring (backend variant, tool registration, diagnostics probe, integrations doc page with run/compose snippets) plus an optional profile-gated or commented `searxng` service in `docker-compose.local.yaml` next to the server, using the host-network or service-name URL.

Decisions I made myself (from the exploration stage, unchanged): map SearXNG results onto the existing `SearchHit`/`format_hits` shape; implement as a new hardcoded `SearchBackend::SearXng` variant consistent with existing code (not a generic pluggable-endpoint system); no required auth (SearXNG local JSON API is keyless — optional bearer key can be a follow-up if someone exposes it); tests are httpmock unit tests in `web_search.rs` following the existing injectable-`search_url` pattern, with the doctor probe and docs updated alongside.