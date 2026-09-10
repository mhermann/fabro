I now have the full change in front of me: all 15 files, the complete `web_search.rs`, the full diagnostics probe section in context, the sibling integration docs, and the repo precedents the implementer leaned on. Here is my complexity-only review.

## Overall verdict

The change is appropriately simple. It extends the existing seam (`SearchBackend` enum → `from_secrets` → `search()` → per-backend formatter) with a third variant that mirrors Brave/Venice line for line, and plumbs one config value through exactly the two population sites that already existed. I found one minor collapse candidate and nothing else.

## Finding 1 (minor): `match_web_search_probe` is now a forwarding shim

`lib/apps/fabro-server/src/diagnostics.rs:869-879`

The refactor split the probe matcher so searxng could carry its own remediation text. The right part of that refactor is `match_web_search_probe_with_remediation` (one 4-arm matcher, three callers — no duplication). The leftover is the old `match_web_search_probe`, which is now 10 lines with zero logic of its own: it formats one string and forwards. Its only reason to exist is that its two callers (brave, venice) didn't have to change.

The simpler version deletes the wrapper and inlines at both existing call sites:

```rust
match_web_search_probe_with_remediation(
    probe,
    "brave",
    "Check BRAVE_SEARCH_API_KEY and network connectivity".to_string(),
)
```

Net effect: one fewer function, one fewer hop for a reader tracing `check_brave_search` → remediation text, at the cost of editing two lines that were otherwise untouched. Genuinely minor — the name is accurate and the hop is one line — but it is exactly the "indirection whose only purpose is diff minimization" shape.

## Things I checked and consider sound (so nobody re-litigates)

- **No abstraction-with-single-caller introduced.** `search_searxng`, `searxng_status_error`, `format_searxng_results`, `check_searxng_search` all mirror existing per-backend siblings; unifying them was already considered and correctly rejected (the JSON shapes and error semantics genuinely differ).
- **No config nobody sets.** One field on `ToolSecrets`, one env-var constant, one line in each of the two population sites, one `.env.example` line. Nothing else.
- **No dead code.** Every new item (`SearchBackend::Searxng`, `searxng()`, the status-error fn, the formatter, all three diagnostics tests) is reachable and exercised. The `.min(MAX_RESULTS)` inside `format_searxng_results` is a redundant second clamp given `max_results_arg`, but `search_brave` has the identical pattern, so it's consistent, not new complexity.
- **The URL-validation block with `#[expect(clippy::disallowed_types)]`** follows an established repo precedent (bedrock, docker.rs, git_bridge, etc.) and replaces an opaque reqwest builder error with an actionable one. It's ~10 lines including the test; I don't think it costs a future reader anything.
- **Docs are not scope creep.** `integrations/searxng.mdx` copies the exact section structure of the brave-search and venice-search pages (Setup / How it works / Permissions / Example workflow / Troubleshooting / Further reading); the touches to `tools.mdx`, `prompts.mdx`, `server-configuration.mdx`, and `troubleshooting.mdx` are one-line mention updates to existing sentences; the changelog page and both `docs.json` nav entries follow the repo's per-date changelog convention.
- **Diagnostics repeats rather than abstracts** the third `diagnostic_secret` block in `check_web_search` — consistent with the file's existing explicit style; collapsing it into a loop would be the kind of new abstraction this repo rejected.

The only action I'd take is Finding 1, and it's optional.