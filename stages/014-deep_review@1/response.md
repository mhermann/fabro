I've completed my review. Here's my verdict.

## What I verified myself

**Code (all sound):**
- Selection precedence SearXNG → Brave → Venice implemented at the `from_secrets` choke point, with blank-URL filtering and trailing-slash trimming — matches the human's "local wins" decision.
- Plumbing is correct: URL crosses the worker `env_clear()` via `WORKER_ENV_ALLOWLIST` (with a test asserting `SEARXNG_API_KEY` does *not* cross), token flows vault-only, `#[expect]` present on the `std::env::var` read, all existing `ToolSecrets` literals already use `..Default::default()`.
- Doctor mirrors agent selection (searxng-first) and the test-hermeticity fix from the critique was applied (`.env_lookup(|_| None)` pins); new tests cover searxng-wins-over-both-keys, blank-URL fallthrough, and a fast-refusal probe.
- I ran the suites myself: 660 tests (fabro-static + fabro-agent) and 2350 tests (fabro-server + fabro-workflow) all pass; `cargo +nightly-2026-04-14 clippy --all-targets -- -D warnings` clean on all four crates; `fmt --check` clean. (One initial run died with SIGKILL — sandbox OOM at high parallelism, not a code failure; `-j 2` passed.)
- The wire contract is pinned by httpmock tests (query params, `format=json`, bearer present/absent, slicing, error mapping) — tests would fail if the request shape or precedence regressed.

**Goal fit:** thin client to operator-run SearXNG, no per-query cost, local-wins, docs-only deployment artifacts — exactly what was requested and what the human chose.

## Defects found — one class, three instances (docs only)

The docs claim `SEARXNG_URL` can be set in `server.env`. That is false. I traced it end-to-end: `AppState::config_env_lookup` is exactly `std::env::var` (`serve.rs:811` → `interp.rs:16`); there is **no `dotenvy` anywhere in `lib/`**; `server.env` is only parsed as a file for bootstrap-secret resolution and the env→vault migration; and `std::env::set_var` is clippy-banned in this repo, so the server *cannot* load files into its process env. The worker allowlist copies the live process env, so a `server.env`-only URL never reaches runs either. An operator following Setup step 2 as written gets a silently unregistered backend — while the page's own Troubleshooting section correctly says the URL must be in the *server process* environment. The docs contradict themselves, and the wrong version is the canonical setup instruction.

1. **`docs/public/integrations/searxng-search.mdx`**, Setup step 2: "reads it from its own process environment (or `server.env`)". Fix: drop the `server.env` parenthetical; state "server process environment" and give real mechanisms (compose `env_file` (the packaged `docker-compose.yaml` does load `.env` into the container), systemd `Environment=`, or export before `fabro serve`).
2. **`docs/public/administration/server-configuration.mdx`**: "read from the server process env (or `server.env`) and forwarded to workflow workers." Same fix — delete "(or `server.env`)".
3. **`docs/public/administration/deploy-railway.mdx`**: "set it in Railway variables (or `server.env`)". Railway variables are correct (platform injects process env); the `server.env` parenthetical is wrong. Delete it.

(`self-host-docker.mdx`'s "set it in `.env`" is **correct** — I confirmed `docker-compose.yaml` declares `env_file: .env`.)

No code changes are needed; the three doc corrections are mechanical. Since the primary setup instruction as merged would not work, this blocks approval.

{"preferred_next_label": "fix"}