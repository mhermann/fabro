Review complete. I read the full diff (24 files), the complete post-change `web_search.rs`, and compared every new piece against its pre-existing counterparts.

## Verdict

**The change is appropriately simple.** I found no dead code, no unused configuration, no single-caller abstraction, and no scope beyond the goal. Details:

**What I checked for complexity, and why it passes:**

- **New code mirrors existing shapes one-for-one.** `SearchBackend::searxng()` follows the existing `brave()`/`venice()` constructors; `searxng_status_error` mirrors `venice_status_error`; `check_searxng_search` in diagnostics is line-for-line the established `check_brave_search`/`check_venice_search` probe pattern (`http_client_or_check` + `EXTERNAL_SERVICE_PROBE_TIMEOUT` + shared `match_web_search_probe`). Nothing new is invented where a pattern existed.
- **No dead or unreachable code.** Every new item (`SEARXNG_REQUEST_TIMEOUT`, `searxng_status_error`, `format_searxng_results`, `check_searxng_search`, both `EnvVars` constants, both `ToolSecrets` fields, the `WORKER_ENV_ALLOWLIST` entry) has a live production caller. The `SEARXNG_API_KEY` in `.env.example` is functional via the existing env→vault startup migration, same as the Brave/Venice entries beside it.
- **Struct-literal blast radius was contained.** All other `ToolSecrets {` sites (parity_matrix, profiles, tools.rs, api.rs) already used `..ToolSecrets::default()`, so the field addition cost zero churn there — the right call versus a builder or a parallel config type.
- **`searxng_url` riding in `ToolSecrets` despite not being a secret** is the simplest option that keeps one selection choke point (`from_secrets`) across every run path; the doc comment on the field states exactly this, and the alternative (a separate config channel through pipeline init and the CLI) would be more indirection, not less.
- **Client-side slicing** (`format_searxng_results` taking `max_results`) is forced by SearXNG having no count parameter, and is commented as such. The redundant-looking `.min(MAX_RESULTS)` re-clamp mirrors how brave/venice each clamp for their own transport — one expression, not worth collapsing.
- **Docs touches (15 files) are not scope creep.** Changing the precedence rule invalidates precedence statements that already existed in the Brave, Venice, tools, prompts, troubleshooting, security, and deploy pages; the edits are consistency repairs, and the one new page follows the sibling integration pages' structure.

**One duplication observation — accepted, not a finding:** the "SearXNG wins + trim/blank-filter URL" selection logic now exists in two crates (`SearchBackend::from_secrets` in fabro-agent, `check_web_search` in fabro-server). This was already the repo's structure — the doctor re-derived brave-else-venice from vault secrets because `SearchBackend` is `pub(crate)` to the agent crate — and the change extends that pattern rather than adding a new one. Collapsing it would mean exporting `SearchBackend` from fabro-agent for a single diagnostics caller and still diverging on the probe path; that trade is not obviously simpler. Tests pin doctor/agent agreement (`check_web_search_prefers_searxng_when_url_and_both_vault_keys_exist`), so the copies cannot silently drift.

Nothing to delete or collapse.