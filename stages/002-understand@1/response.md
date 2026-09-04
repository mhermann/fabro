Exploration complete. Here is the understanding report.

---

## 1. Repository structure and where the change lands

**Fabro** is an AI workflow orchestration platform: a Rust workspace (`lib/apps`, `lib/components`, `lib/foundation`), a React 19 web app (`apps/fabro-web`), Mintlify docs (`docs/public`), with `cargo nextest` for tests and insta snapshots.

The provider/model system lives in **`lib/foundation/fabro-model`**:

- `src/catalog/providers/*.toml` — one TOML per provider, embedded into the binary via `rust_embed`. Each model row declares `api_id`, `display_name`, `family`, `default`/`small_default`, `aliases`, `estimated_output_tps`, `limits` (context/max output), `features` (tools/vision/reasoning/reasoning_effort kind/prompt_cache), `controls.reasoning_effort` (allowed levels), and `costs` (input/output/cache per Mtok).
- `src/catalog.rs` — parses/validates/merges the catalog (enforces one `default` per provider, alias uniqueness per provider, reasoning-controls consistency), indexes selectors, and holds extensive tests including insta snapshots of model rows.
- `src/reasoning.rs` — `ReasoningEffort` enum: `low | medium | high | xhigh | max`.
- `src/types.rs` — `Model`, `ModelFeatures` with `ReasoningEffortFeature::{Levels, AlwaysAdaptive, None}`.

The `zai` provider (`zai.toml`) is an `openai_compatible` adapter pointed at `https://api.z.ai/api/coding/paas/v4`, credentials `ZAI_API_KEY`, priority 60. It currently serves `glm-5.2` (`default = true`, aliases `glm, glm5, glm52, glm5.2`, 1,048,576 ctx / 131,072 out, efforts `high|max`, costs 1.4/4.4/0.26) and `glm-4.7`.

Everything downstream (server `GET /models`, CLI, web UI, agent profiles) reads the catalog dynamically — no hardcoded model lists anywhere else. Docs (`docs/public/core-concepts/models.mdx` table, `integrations/*.mdx`, dated changelog files) are maintained by hand.

## 2. Goal restated in this codebase's terms

Add **`glm-5.3`** and **`glm-5.3-flash`** as model offerings in the built-in catalog:

1. **`zai.toml`**: add `glm-5.3` (text-only flagship) and `glm-5.3-flash` (multimodal, vision-capable). Following the GLM 5.2 precedent (commit `c67cfef1`): the new flagship gets `default = true`, `glm-5.2` loses `default`, and the portable aliases `glm`/`glm5` move from `glm-5.2` to `glm-5.3` (which keeps `glm52`, `glm5.2`), mirroring how `glm-4.7` was demoted and kept only `glm4`.
2. **`openrouter.toml`**: add both models with `api_id = "z-ai/glm-5.3"` / `"z-ai/glm-5.3-flash"`, OpenRouter pricing, and portable aliases matching the zai rows (the `builtin_glm_5_2_aliases_are_portable` test asserts zai/openrouter alias parity).
3. **"Others"**: `fireworks.toml` already serves `glm-5.2` (`accounts/fireworks/models/glm-5p2`), so `glm-5p3` / `glm-5p3-flash` rows are the natural extension; `venice.toml` already has `glm-5.3` but not Flash; `bedrock.toml` has an older distinct `glm-5`.
4. **Catalog tests** in `catalog.rs`: update the `glm_5_2_in_catalog` snapshot (its `default` flips to `false`), extend/replace `builtin_glm_5_2_aliases_are_portable`, add `glm_5_3_*` tests following the existing pattern, possibly extend the Fireworks tuple-table test.
5. **Docs**: update the `models.mdx` table (which currently lists `glm-5.3` under *venice* only), `openrouter.mdx` (and `fireworks.mdx` if touched), plus a changelog bullet in a dated `docs/public/changelog/2026-09-*.mdx`.
6. Optionally, live e2e round-trip tests in `lib/components/fabro-llm/tests/integration.rs` mirroring `zai_glm_5_2_reasoning_tool_round_trip` / `openrouter_glm_5_2_...`.

No codec/adapter work appears needed: zai already rides the `openai_compatible` codec, which sends `reasoning_effort` top-level and reads `reasoning_content`/`reasoning` — the `fabro-llm` adapter registry test (`every_builtin_catalog_offering_resolves`) iterates the catalog automatically.

## 3. What I know for certain

**From the repo:**
- The default-change precedent is exactly `c67cfef1` (GLM 5.2 over 4.7) and `18d46282` (GPT-5.6 Sol became OpenAI default): new flagship gets `default = true`, old default loses it, generic aliases migrate, docs + snapshots updated. Exactly one `default` per provider is enforced at build time (`MultipleProviderDefaults`).
- Aliases are per-provider; unqualified selectors pick the highest-priority ready provider. zai (60) > venice (35) > fireworks (30) > openrouter (25), so `glm` resolves to zai when both keys exist.
- Venice already ships `glm-5.3`: 1,000,000 ctx / 131,072 out, `levels` + `reasoning_by_default = true`, efforts `low|high|max`, prompt_cache, costs 1.75/5.50/0.325 — a same-model precedent inside this very catalog.
- `ReasoningEffort` already includes `low` and `max`; `AlwaysAdaptive` is only behaviorally consumed by the Anthropic adapter, so for OpenAI-compatible routes `levels` vs `always_adaptive` is descriptive.
- `small_default = true` is the convention for cheap/fast tier models on their home provider (haiku, gpt-5.4-mini, deepseek-v4-flash, gemini-flash-lite).
- No GLM/zai hardcoding exists in fabro-server, fabro-web, evals, or the OpenAPI spec — catalog data flows through automatically.

**From external research (z.ai docs + OpenRouter, fetched today):**
- **GLM-5.3** (`glm-5.3`, released 2026-08-18): text-only, 1M context, **128K max output**, reasoning always-on with efforts **`low|high|max`** (default `max`; disabling fails), function calling + streaming + context caching + structured output supported. z.ai list price **$1.40 / $4.40 / $0.26 cache** per Mtok — identical to GLM-5.2. Served via the same coding endpoint (`/api/coding/paas/v4`).
- **GLM-5.3-Flash** (`glm-5.3-flash`, released 2026-08-26): native multimodal (**image/video/file input → text**), 1M context, **128K max output**, text parameters identical to GLM-5.3 (same effort levels, thinking cannot be disabled), function calling/caching/structured output supported. z.ai list price **$0.15 / $0.50 / $0.03 cache**, currently 50% off (**$0.075 / $0.25 / $0.015**) until Sep 9, 2026.
- **OpenRouter**: `z-ai/glm-5.3` headline **$1.15 / $3.50**, cache $0.10 (Z.ai first-party row is 1.40/4.40/0.26); `z-ai/glm-5.3-flash` headline **$0.075 / $0.25 / $0.015** (the 50%-off price; list 0.15/0.50/0.03). Both also served by Fireworks (5.3 at 1.40/4.40/0.26; Flash at 0.15/0.50/0.03) and Venice (Flash at 0.15/0.50/0.03).

## 4. What is genuinely ambiguous

1. **Which "others" beyond OpenRouter.** Fireworks and Venice both demonstrably serve these models, but is the intent "every provider that already carries GLM 5.2" (zai + openrouter + fireworks), "plus Venice Flash," or "anywhere it's fetchable"? Bedrock carries only an older `glm-5` and probably shouldn't grow a row unless AWS lists it. Two engineers would reasonably ship different provider coverage.
2. **Which price to record.** z.ai rows: list (1.40/4.40/0.26 and 0.15/0.50/0.03) vs current promo (Flash at half price until Sep 9 — a price that goes stale in days). OpenRouter rows: the existing `glm-5.2` entry (0.784/2.464/0.1456) proves the convention is "OpenRouter's displayed headline at update time" — which is currently a *discounted/cheapest-provider* figure (1.15/3.50/0.10 for 5.3), not Z.ai list. Whether to use headline vs Z.ai-row pricing for OpenRouter is a judgment call with a stale-by-September-9 wrinkle.
3. **`reasoning_effort` feature kind.** z.ai says reasoning is always-on and cannot be disabled — textbook `always_adaptive`. But Venice's existing `glm-5.3` row uses `levels` + `reasoning_by_default = true`. Behaviorally near-identical on this adapter today, but the rows will disagree across providers for the same model if zai/openrouter pick one and Venice keeps the other.
4. **Flash's role on zai.** Should `glm-5.3-flash` be `small_default = true` (haiku/deepseek-flash/mini precedent), and does it inherit the `glm-flash`-style aliases? What exactly the alias set should be for each new model (`glm53`, `glm5.3`, `glm-flash`, `glm53-flash`, …) is convention, not fact — though the 5.2 → 5.3 alias migration pattern is clear for the flagship.
5. **Limits numbers.** Repo's glm-5.2 uses 1,048,576/131,072 while z.ai documents 5.3/Flash as "1M context, 128K max output" (OpenRouter FAQ says 1,310,720/262,144, which looks like input+output combined). Whether `context_window = 1048576, max_output = 131072` or `1000000/131072` (Venice's choice) — and 128K = 131072 vs 128000 — needs a deliberate convention pick.
6. **Scope of test/docs work.** Whether to add live `e2e_test` round-trips for 5.3 (they cost real API calls and the 5.2 ones exist), update the Venice `models.mdx` row's provider attribution, whether the Flash tier needs an `estimated_output_tps` figure, and whether a fresh dated changelog file is created vs. appending to an existing one.
7. **`glm-4.7`'s fate.** It stays regardless, but with 5.3 at identical list pricing and better benchmarks, some would also prune `glm-4.6`-aged rows; nothing in the instruction demands removal, so leaving them is the conservative read.