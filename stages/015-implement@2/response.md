All review findings are fixed. The diff is four one-line doc corrections — no code changed, so the previously green test suites (fabro-static, fabro-agent, fabro-workflow, fabro-server, fmt, clippy) remain valid; I re-verified that no Rust code or test references the changed doc strings.

## What I fixed

The deep_review findings were all one defect class: docs claimed `SEARXNG_URL` can be set in `server.env`. It can't — `AppState::config_env_lookup` is plain `std::env::var`, no dotenvy exists in `lib/`, and `server.env` is only parsed for bootstrap-secret resolution. Following the docs as written would produce a silently unregistered backend.

**`docs/public/integrations/searxng-search.mdx`** (Setup step 2)
Replaced "process environment (or `server.env`)" with "process environment — never from `server.env`" and added the three real mechanisms: `export` before `fabro serve`, systemd `Environment=`, or a `.env` line under the packaged Docker Compose setup. Before referencing the compose path, I confirmed `docker-compose.yaml` declares `env_file: .env` (optional), so `.env` values genuinely become the container's process env. Now consistent with the page's own Troubleshooting note ("must be set in the *server process* environment").

**`docs/public/administration/server-configuration.mdx`** (line 372)
"(or `server.env`)" → "(never from `server.env`)". Also aligns with the same file's lines 418/427, which already correctly said process env only.

**`docs/public/administration/deploy-railway.mdx`** (line 44)
Dropped "(or `server.env`)"; Railway variables remain the instruction (the platform injects process env). Added an explicit "Railway injects variables into the container's process environment; `server.env` inside `/storage` is not consulted for it" — path verified against `fabro-config/src/storage.rs` (`env_path() = root.join("server.env")`) and the doc's `/storage` volume mount.

**`docs/public/administration/security.mdx`** (line 41) — *beyond the three listed findings, flagged explicitly*
The review listed three instances, but this line had the same ambiguity: it grouped `SEARXNG_URL` under "Process env and `server.env` are reserved for … plus non-secret endpoint config such as `SEARXNG_URL`". Changed to "process-env endpoint config" so the Secrets page can't be read as endorsing `server.env` either. I judged this in-scope (same defect class, same correction) rather than unrelated refactoring; reverting it is trivial if you disagree.

## Verification

- Grep across all docs: every remaining `SEARXNG_URL` + `server.env` co-occurrence now explicitly *negates* `server.env`; no line implies it works.
- Frontmatter and inline-code backtick balance intact in all four edited files.
- The only docs validator (`cargo dev docs check`) covers generated reference files (`cli.mdx`, `user-configuration.mdx`), which this change doesn't touch — skipped building it for that reason.
- `self-host-docker.mdx`'s "set it in `.env`" was already correct (compose `env_file`) and was left alone, per the review.