Review complete. I read the full diff (all 10 files) plus the surrounding files (`catalog.rs` test module boundaries, `zai.toml` provider header) to confirm context.

## What the change is

- Static catalog data: new `glm-5.3` / `glm-5.3-flash` entries in `zai.toml`, `openrouter.toml`, `fireworks.toml`; `glm`/`glm5` aliases and the zai default moved from 5.2 to 5.3.
- Test-only Rust changes: catalog unit tests in `catalog.rs` (verified every hunk lands inside `#[cfg(test)] mod tests`, lines 2630–7787) and two live E2E round-trip tests in `fabro-llm/tests/integration.rs`.
- Documentation: changelog, models table, docs.json, two integration pages.

## Security findings

**None.** Specifically checked:

- **Secrets/credentials**: nothing hardcoded. The new tests read `ZAI_API_KEY` / `OPENROUTER_API_KEY` via `std::env::var(EnvVars::...)` at runtime and are gated behind `#[e2e_test(live(...))]`, matching the existing pattern in this file. A grep of the diff for secret-like literals found only `max_tokens` values.
- **Injection / dangerous sinks**: the only new strings reaching a sink are static `api_id` values (`z-ai/glm-5.3`, `accounts/fireworks/models/glm-5p3`) sent as the `model` field in JSON request bodies. No shell interpolation, no path construction, no new untrusted input paths.
- **Endpoints**: no `base_url`, `auth`, or `credentials` changes in any provider TOML. The hardcoded URLs in the tests (`https://api.z.ai/api/coding/paas/v4`, `https://openrouter.ai/api/v1`) exactly match the existing catalog base URLs and are HTTPS.
- **Deserialization**: the only new deserialization is test assertions reading `response.raw.pointer("/choices/0/message")` from the provider's own reply — key inspection only, no unsafe handling.
- **Dependencies**: no `Cargo.toml`, `package.json`, or lockfile changes.
- **Authorization**: no auth logic touched.

One non-security observation (correctness, not a vuln): pricing is static catalog data feeding cost estimates; the values ($1.40/$4.40 and $0.15/$0.50, cache $0.26/$0.03) only affect reported cost math, and the 5.3 rates are identical to 5.2's, so the default-model switch introduces no silent cost escalation.