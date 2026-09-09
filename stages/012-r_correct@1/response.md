All 21 targeted tests pass (the `tests.rs` unused-import warning is the pre-existing one). My review is complete.

## Review verdict

**The Rust code is correct. I found no logic defects in it.** One real docs defect and one minor wording nit.

### What I verified in the code

- **Precedence** (`web_search.rs:48-65`, `diagnostics.rs:760-807`): SearXNG URL → Brave key → Venice key, no fallback, in both the run path and the doctor, so doctor never disagrees with what runs select. Blank/whitespace URLs are filtered at the single choke point (`from_secrets`) and identically in the doctor, so `SEARXNG_URL=" "` falls through to Brave consistently.
- **URL construction** (`web_search.rs:84-89`): trailing-slash trim then `/search` append; matches the doctor's probe URL construction (`diagnostics.rs:764`, `859`).
- **Wire format**: `GET /search?q=…&format=json&pageno=1` with `Accept: application/json`, optional bearer, field mapping `title`/`url`/`content`/`publishedDate` — all match SearXNG's JSON schema; reqwest handles query encoding; slicing `.take(max_results.min(20))` is correctly bounded (`as usize` after the min, so no overflow).
- **Secret boundaries**: `ToolSecrets` Debug prints presence only (`config.rs:113-124`, tested); `SEARXNG_API_KEY` is vault-only and excluded from the worker allowlist while `SEARXNG_URL` is allowlisted — confirmed the allowlist is the only env path to the process that runs `tool_secrets_from_configured_sources` (production spawns only `LocalWorkerRuntime`, `server.rs:2540-2551`).
- **Test hygiene**: all four pre-existing `check_web_search` tests are either explicitly pinned (`env_lookup(|_| None)`) or implicitly pinned (their closures return `Some` only for their own var), so an ambient `SEARXNG_URL` can't flip them.
- Ran `searxng`/`from_secrets` fabro-agent tests (14/14) and all `check_web_search` fabro-server tests (7/7) — green.

### Finding 1 — docs claim `SEARXNG_URL` works in `server.env`; it silently doesn't

- `docs/public/integrations/searxng-search.mdx:42` — "the Fabro server reads it from its own process environment (or `server.env`), and it is forwarded to workflow workers."
- `docs/public/administration/server-configuration.mdx` (new paragraph) — "read from the server process env (or `server.env`) and forwarded to workflow workers."
- `docs/public/administration/deploy-railway.mdx` — "set it in Railway variables (or `server.env`)".

False for `server.env`. Every consumer reads only the process env: `config_env_lookup` → `process_env_var` = `std::env::var` (`interp.rs:16-18`, wired at `serve.rs:811`), the worker allowlist copies `std::env::var_os` (`spawn_env.rs:62-76`), and `tool_secrets_from_configured_sources` reads `std::env::var` directly. `server.env` is parsed only into `ServerSecrets::file_entries` (`server_secrets.rs:29-44`) for bootstrap-secret lookups and the env→vault migration — nothing ever applies it to the process env (the only `set_var` in the crate is a test flag, `install.rs:147`).

**Trigger:** operator follows the doc, sets `SEARXNG_URL=http://searxng:8080` in `<storage>/server.env`, restarts. Result: doctor reports "optional, not configured" and `web_search` is unregistered in every run — no error anywhere. The doc even contradicts itself: `searxng-search.mdx:96` ("must be set in the *server process* environment") states the correct rule. Fix: drop "(or `server.env`)" from the three spots. (The `.env` claim in `self-host-docker.mdx` is fine — the packaged compose passes `.env` via `env_file`.)

### Finding 2 — minor wording (docs only)

`docs/public/agents/tools.mdx`: "The standalone agent CLI reads all three from the invoking shell instead." The CLI reads four variables (`cli.rs:45-50`: `SEARXNG_URL`, `SEARXNG_API_KEY`, `BRAVE_SEARCH_API_KEY`, `VENICE_API_KEY`); the sentence's "three" omits `SEARXNG_API_KEY`.

### Things I considered and rejected as findings

- `max_results=0` on SearXNG returns "No results found." while Venice clamps to 1 — matches the pre-existing Brave pass-through behavior; not a regression.
- Blank `SEARXNG_API_KEY` in the vault sends an empty bearer — identical exposure exists for Brave/Venice keys today; degenerate config, no new failure class.
- `SEARXNG_URL` without a scheme fails at request time with reqwest's builder error — operator error with a reasonable surfaced message, same in doctor and runs.