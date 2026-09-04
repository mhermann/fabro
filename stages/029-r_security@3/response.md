I've reviewed the complete diff. Here's my security assessment.

## Scope of the change

10 files, all catalog data, docs, or tests:

- **Catalog data (TOML)** — `zai.toml`, `openrouter.toml`, `fireworks.toml`: new `glm-5.3` / `glm-5.3-flash` model entries (limits, features, controls, costs, api_ids); portable aliases `glm`/`glm5` moved from `glm-5.2` to `glm-5.3`; `default = true` moved likewise.
- **Rust** — `catalog.rs`: changes are exclusively inside `mod tests` (line 2631, `#[cfg(test)]`); `fabro-llm/tests/integration.rs`: two new live-gated e2e round-trip tests.
- **Docs** — changelog entry, model tables, docs nav.

## Findings

**No security problems introduced by this diff.**

Specifically checked:

- **Secrets/credentials**: None. The only credential handling is `std::env::var(EnvVars::ZAI_API_KEY)` / `EnvVars::OPENROUTER_API_KEY` in the new e2e tests, matching the pre-existing pattern in the same file (e.g., the existing zai test at line 396). A regex sweep of the diff for key/token/`sk-`/password/Bearer literals produced only `max_tokens` false positives.
- **Credential exfiltration via URL**: The hardcoded base URLs in the new tests — `https://api.z.ai/api/coding/paas/v4` and `https://openrouter.ai/api/v1` — byte-for-byte match the production `base_url` values in `zai.toml:5` and `openrouter.toml:5`. API keys are not sent to any new or wrong host.
- **Injection / unvalidated input**: No new input parsing or user-controlled data reaching sinks. The new TOML values (`api_id` slugs like `z-ai/glm-5.3`, `accounts/fireworks/models/glm-5p3`) are static constants following the existing format, deserialized through the existing catalog loader.
- **Auth/authz**: No auth logic touched. The `providers.zai.auth` section (`env:ZAI_API_KEY`, `vault:ZAI_API_KEY`) appears only as unchanged context.
- **Dependencies**: No `Cargo.toml`, lockfile, or package manifest changes — nothing added.
- **Docs (MDX/JSON)**: Plain prose and table rows; no scripts, iframes, or external links introduced.

One non-security observation (product behavior, not a vulnerability): users who relied on the unpinned `glm`/`glm5` aliases will silently move from GLM 5.2 to GLM 5.3. Pricing is identical ($1.40/$4.40), and pinned forms `glm52`/`glm5.2` still resolve, so there's no billing or availability surprise — and the docs explicitly call out the alias takeover.