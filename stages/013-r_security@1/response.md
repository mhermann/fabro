Review complete. I've examined the full diff across all 10 files.

## Security review: no security problems introduced

**What the change is:** Three data-only provider TOML additions (`zai.toml`, `openrouter.toml`, `fireworks.toml`) registering GLM 5.3 / GLM 5.3 Flash with aliases, limits, features, costs; the `default`/`small_default` flags and portable `glm`/`glm5` aliases moved off `glm-5.2`; matching docs (MDX/JSON); test-only updates in `catalog.rs` (all hunks ≥ line 3444, inside the `#[cfg(test)] mod tests` at line 2630); and two live e2e tests in `integration.rs`.

**Checked against each category:**

- **Secrets/credentials:** None committed. `credentials = ["env:ZAI_API_KEY", "vault:ZAI_API_KEY"]` is pre-existing indirection (references, not values). Tests read keys from env at runtime, gated behind `#[e2e_test(live("..."))]`, which are ignored by default and only run with the e2e profile. No key is logged or printed.
- **Injection:** The new TOML strings (`api_id`, `aliases`, `display_name`) are maintainer-authored build-time constants compiled in via `RustEmbed` (`catalog.rs:23`), not user input. They flow only into outbound LLM request bodies as model identifiers, matching the exact format of sibling entries (`z-ai/glm-5.2`, `accounts/fireworks/models/glm-5p2`). No shell, path, SQL, or HTML sinks touched.
- **Unvalidated input → dangerous sink:** No new input paths, endpoints, or untrusted deserialization. TOML parsing uses serde with `deny_unknown_fields` over embedded/test-literal content.
- **Authorization:** Untouched.
- **Unsafe deserialization / path traversal:** None.
- **Dependencies:** No manifest changes (no `Cargo.toml`, `package.json`, or lockfiles in the diff).
- **Test endpoints:** Hardcoded HTTPS to `https://api.z.ai/api/coding/paas/v4` and `https://openrouter.ai/api/v1` — byte-identical to existing tests in the same file (lines 374, 396, etc.). No endpoint substitution smuggled in.

One non-security observation (product, not exploit): the portable `glm`/`glm5`/`glm53`/`glm5.3` aliases now resolve to zai GLM 5.3 while venice still lists the same aliases in the docs table. Resolution is per-provider (`resolve_on_provider`), so there's no cross-provider override — just a potential UX ambiguity, not a security issue.