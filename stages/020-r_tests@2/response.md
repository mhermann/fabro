## Test Coverage Review — GLM 5.3 / 5.3 Flash

**What's covered (and covered well):**

- **Z.ai catalog entries** — `glm_5_3_in_catalog` and `glm_5_3_flash_in_catalog` (catalog.rs:7391, :7472) are full insta snapshots: id, family, limits, features (incl. `vision: true` for flash, `reasoning_effort: Levels`), costs, controls `low/high/max`, aliases, plus behavioral assertions on alias resolution, `api_id`, `default_for_provider(zai) == glm-5.3`, and `small_default_for_provider(zai) == glm-5.3-flash`.
- **Alias migration is enforced, not just described** — `builtin_glm_5_3_aliases_are_portable` (catalog.rs:3447) asserts `glm`/`glm5`/`glm53`/`glm5.3` resolve to `glm-5.3` on both zai and openrouter; the updated `glm_5_2_in_catalog` snapshot pins `aliases: [glm52, glm5.2]` and `default: false`. If glm-5.2 had kept the portable aliases, these fail. The build-time "multiple default models" invariant backs the default move.
- **Fireworks entries** — the table-driven `builtin_fireworks_models_when_enabled` (catalog.rs:4183) asserts id, `api_id` (`glm-5p3` / `glm-5p3-flash`), limits, vision, reasoning, and costs; a typo in any of those fails.
- **Live round-trips** — `zai_glm_5_3_reasoning_tool_round_trip` and `openrouter_glm_5_3_reasoning_tool_round_trip` (integration.rs:557, :645) mirror the glm-5.2 precedent exactly: reasoning content present before the tool call, required tool invoked, tool-result replay, and `CostSource::Authoritative` on openrouter (which also proves the `z-ai/glm-5.3` api_id end-to-end).
- Verified: the 7 glm catalog tests, both fireworks tests, and the shared-slug test pass; integration tests compile.

**Findings — specific untested cases:**

1. **OpenRouter `glm-5.3-flash` `api_id` is asserted nowhere.** `builtin_openrouter_includes_glm_5_3_when_enabled` checks `api_id` only for non-flash `glm-5.3` (catalog.rs:3718); `builtin_deepseek_shared_slugs_are_portable_across_providers` checks only id/provider resolution, not settings; and no live test uses flash on openrouter (the round trip uses `z-ai/glm-5.3` only). A typo in `openrouter.toml:648` (`api_id = "z-ai/glm-5.3-flash"`) passes the entire suite and only surfaces as a runtime 404. Every sibling openrouter addition (glm-5.2 at :3641, glm-5.3 at :3718) has this assertion — flash is the outlier.

2. **OpenRouter `glm-5.3-flash` aliases (`glm-flash`, `glm53-flash`, `glm5.3-flash`) are untested.** The zai flash aliases are asserted in `glm_5_3_flash_in_catalog`, but `builtin_glm_5_3_aliases_are_portable` doesn't include the flash aliases for openrouter, so dropping or misspelling one of them in `openrouter.toml:650` fails nothing.

3. **Fireworks GLM 5.3 / 5.3 Flash `controls.reasoning_effort` is untested.** The Fireworks table test (catalog.rs:4195) has no controls column — only the kimi-k3 entries get a controls assertion (catalog.rs:4405-4429). The repo has a precedent test for exactly this risk, `builtin_deepseek_reasoning_controls_match_provider_dialects` (catalog.rs:3103), which exists because the same model supports different effort levels per provider; if Fireworks's GLM endpoint rejects `max` or someone copies `["low","medium","high"]` into `fireworks.toml:197/:221`, no test fails. (Same applies to the openrouter flash controls.)

**Minor, precedent-consistent (not counted as findings):** live tests exercise only `ReasoningEffort::High` and only the non-flash model; flash's vision capability is asserted only via catalog snapshots, with no wire-level test of flash through any adapter — both match the existing glm-5.2 / catalog-only-model pattern.

**Verdict:** Coverage is adequate for the default-model change, alias migration, and the flagship glm-5.3 entries, but the GLM 5.3 Flash **OpenRouter** offering is effectively untested beyond id resolution (api_id, aliases, controls), and Fireworks effort controls are unasserted. Closing findings 1–3 would take one small extension to the existing tests, no new harness needed.