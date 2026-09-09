I now have the full picture: all production code, tests, and docs in the diff. Here is my complexity-only review.

## Verdict: The diff is appropriately simple. No changes needed.

**What the change is:** a third `web_search` backend (`SearchBackend::Searxng`) wired through the exact same shape as Brave and Venice, plus a doctor probe, docs, and tests. Nothing more.

### Things I specifically hunted for and cleared

**Abstractions with a single caller** — every new item mirrors an existing per-provider counterpart one-to-one: `searxng()` constructor ↔ `brave()`/`venice()`, `search_searxng()` ↔ `search_brave()`/`search_venice()`, `searxng_status_error()` ↔ `venice_status_error()`, `format_searxng_results()` ↔ `format_venice_results()`, `check_searxng_search()` ↔ `check_brave_search()`/`check_venice_search()`. No new trait, layer, or indirection was introduced.

**Duplicated logic** — the one real candidate is that the "SearXNG URL wins, trim it, strip the trailing slash, treat blank as absent" rule now lives in two crates: `web_search.rs` (`from_secrets`/`searxng()`) and `diagnostics.rs` (`check_web_search`/`check_searxng_search`). I checked whether this is new: it isn't. The Brave-beats-Venice precedence and each provider's probe URL were already duplicated between these exact two functions before this change; the diff extends the existing pattern rather than creating one. Collapsing it would require exporting `SearchBackend::from_secrets` from `fabro-agent` and building a `ToolSecrets` from diagnostics state (async vault read + `config_env_lookup` indirection) — a cross-crate abstraction with exactly one external caller, which is its own complexity smell. The duplicated 4 lines are the simpler option; leave it.

**Near-duplicate formatters** — `format_searxng_results` differs from `format_venice_results` only in the date key and a `.take(count)`. A parameterized single formatter is possible, but three flat ~12-line functions are easier to read than one with four key parameters, and the two-near-duplicates pattern already existed. Not a real-time cost.

**Configuration nobody sets** — `SEARXNG_API_KEY` is optional by design (keyless instances are the default path, and there's a `header_missing` test proving the keyless request works). It's enrolled in the vault registry exactly like `BRAVE_SEARCH_API_KEY`/`VENICE_API_KEY`, so it behaves like its peers, including the env→vault startup migration. Nothing is plumbed but unreachable.

**Dead/unreachable code** — none. `tools.rs` and `demo/mod.rs` are comment/string-only edits; `spawn_env.rs` adds one allowlist entry with a comment explaining why the key is deliberately excluded, plus a fail-closed assertion both ways.

**Scope beyond the goal** — none. The goal was a cheap local search provider; the change is exactly backend + doctor probe + docs. The edits to `brave-search.mdx`, `venice-search.mdx`, `server-configuration.mdx`, `deploy-railway.mdx`, and `self-host-docker.mdx` are not scope creep — those pages previously asserted "Brave key beats everything," which this change makes false; leaving them would have produced self-contradicting docs. The dual-path config (URL via process env, token via vault) is inherent to the decided design and matches how `DAYTONA_API_URL` already works, with the tradeoff documented in `spawn_env.rs`, `initialize.rs`, and the docs.

**Test weight** — 11 new agent tests + 3 doctor tests is proportionate to the existing venice/brave test density in the same files; they pin the wire contract (request shape, keyless header absence, client-side slicing, status mapping) rather than re-testing trivia.

Two trivia-level notes, not findings: `pageno=1` is technically redundant (SearXNG defaults to page 1) but is explicit, fixed, and matches the documented request; and the new `SEARXNG_*` consts aren't in `env_vars.rs`'s token-shape test list, but that list already covers only ~72 of 130 consts, so it isn't maintained exhaustively anyway.

If a future cleanup ever wants to remove duplication, the target would be the doctor's per-provider selection logic reusing `from_secrets` — but that's a pre-existing structure, not something this change made worse.