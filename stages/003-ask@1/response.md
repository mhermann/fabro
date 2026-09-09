All four answers are in. Restatement for the record:

**1. Deployment model → Thin client.** Fabro will not manage any container lifecycle. We add a third `SearchBackend` variant that makes HTTP calls to an operator-run SearXNG instance; the integration doc ships a copy-pasteable compose snippet plus the recommended SearXNG config (JSON output format enabled). No Docker orchestration code in `fabro-sandbox` or anywhere else.

**2. Precedence → Local wins.** Selection order becomes: local URL configured → use local; else Brave key → Brave; else Venice key → Venice; else tool unregistered. A configured local URL makes paid keys dormant (available again by unsetting the URL). No fallback between providers on failure, per the existing rule.

**3. Config mechanism → URL env var + config layer.** New `EnvVars` const (e.g. `SEARXNG_URL`) resolved via `config_env_lookup` like `DAYTONA_API_URL`, deliberately *not* classified as a vault secret in `secret_registry.rs`. Optional bearer token for protected instances goes in the vault. The URL flows into `ToolSecrets` (or its renamed successor) at pipeline init in `initialize.rs` and from process env in the standalone CLI's `cli_tool_secrets`, so worker env-scrubbing stays intact.

**4. Deploy artifacts → Docs only.** `docker-compose*.yaml` files stay untouched; the compose snippet lives in the new integration doc page only.

Carried-over decisions that stand: SearXNG's native JSON API as the wire format; optional bearer-token auth, keyless by default; map SearXNG `title`/`url`/`content`/`publishedDate` into the existing `SearchHit` shape without cross-engine dedup; `fabro doctor` gains a local-URL probe mirroring the Brave/Venice checks; httpmock unit tests in `web_search.rs` following the existing overridable-`search_url` pattern; full docs set updated (integration page + `docs.json` nav + `agents/tools.mdx` + `server-configuration.mdx` + changelog + `.env.example`).