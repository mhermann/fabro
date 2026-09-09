Implementation complete. Summary of what changed, file by file:

## Code

**`lib/foundation/fabro-static/src/env_vars.rs`** — added the `SEARXNG_URL` constant and registered it in the exhaustive single-token test list.

**`lib/foundation/fabro-static/src/secret_registry.rs`** — added `SEARXNG_URL` to `OPTIONAL_VAULT_SECRETS` (both the const list and its test), making `fabro secret set SEARXNG_URL` a vault-accepted name.

**`lib/components/fabro-agent/src/config.rs`** — `ToolSecrets` gains `searxng_url: Option<String>`; `Debug` reports `searxng_search_configured` without printing the URL; redaction test extended.

**`lib/components/fabro-agent/src/web_search.rs`** — the core: new `SearchBackend::SearXng { base_url }` variant; `from_secrets` precedence is now **SearXNG > Brave > Venice**; `search_searxng` GETs `{base}/search?q&format=json&limit` and maps `results[]` (`title`/`url`/`content`/`publishedDate`) through the existing `SearchHit`/`format_hits`; 403 gets the "enable json in search.formats" hint. Tests: precedence (3), URL trim, httpmock happy path (asserts GET + query params), trailing-slash base, 403 hint, client-side truncation, three-way schema parity. 12 new tests.

**`lib/components/fabro-agent/src/tools.rs`**, **`README.md`** — selection doc comments updated (README was a touchpoint the plan missed).

**`lib/components/fabro-workflow/src/pipeline/initialize.rs`** / **`lib/components/fabro-agent/src/cli.rs`** — the two production `ToolSecrets` construction sites now read `SEARXNG_URL` (vault for server runs, process env for CLI runs).

**`lib/apps/fabro-server/src/diagnostics.rs`** — `check_web_search` probes SearXNG first via new `check_searxng_search` (reuses `match_web_search_probe`); not-configured remediation now names all three backends; 3 new precedence/env-isolation tests, 2 updated assertions.

**`lib/apps/fabro-server/src/demo/mod.rs`** — demo doctor fixture string kept in sync.

## Compose & docs

**`docker/searxng/settings.yml`** (new) — `use_default_settings` + JSON format enabled + limiter off. **`docker-compose.local.yaml`** — `searxng` service behind `profiles: ["search"]`, host `127.0.0.1:8888` → container 8080. **`docs/public/integrations/searxng.mdx`** (new) + nav entry in **`docs.json`**. Updated **`tools.mdx`**, **`brave-search.mdx`**, **`venice-search.mdx`**, **`server-configuration.mdx`**, **`server-secrets-strategy.md`** (with the URL-not-a-secret rationale), **`.env.example`**. **Changelog**: appended a SearXNG section to the *existing* `2026-09-09.mdx` (per the critique — the plan's "create" would have clobbered it).

## Adaptations from the plan
- SearXNG's image listens on container port **8080**, so the service URL is `http://searxng:8080` (plan said 8888); host port 8888 still mapped for standalone testing.
- Critique corrections all absorbed: whitespace+slash URL trim, client-side `.take(limit)` truncation, changelog append.
- `searxng()` takes `&str` (clippy `-D warnings`); nav entry placed after `venice-search` to keep existing entries stable.

## Verification
`fabro-agent` 654 passed / 0 failed · `fabro-workflow` 1444 / 0 · `fabro-static` 5 / 0 · `fabro-server` diagnostics+demo 43 / 0 (full suite: 889 pass, 6 fail — **verified identical 6 fail at HEAD** via `git stash`; graphviz `dot` missing + storage-path env issues, unrelated). `fmt --check` clean, clippy clean on all touched crates. Docker unavailable here, so compose was validated structurally rather than with `docker compose config`; live SearXNG verification is documented in the integration page.