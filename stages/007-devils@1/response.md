I verified the plan's riskiest claims against the actual repository before judging.

**What I confirmed is sound (no churn needed on these):**
- No changelog collision — no `2026-09-*.mdx` exists; `ZAI_API_KEY` is already in `.env.example`, `EnvVars`, and the secret registry, so the live-test claim holds.
- No hidden snapshot coupling — grepped all of `lib/` for `glm`: the only Rust references are the catalog tests the plan already lists, `review_glm` branch names in unrelated server/store tests, `glm-4.7` behind the `quarantine` feature in `parity_matrix.rs`, and a doc comment in the Bedrock codec. No CLI snapshot, no docs-sync test, no model-list enumeration that additions would break.
- Catalog integrity tests are count-safe: `every_provider_has_catalog_models`, `every_provider_has_exactly_one_default_model`, and `builtin_small_defaults_are_marked_per_provider` assert per-provider invariants, not totals — the flag flip on zai satisfies them.
- `ReasoningEffort` already has `Low`/`High`/`Max` (Venice's glm-5.3 row uses them), `vision = true` on `openai_compatible` is precedented (kimi-k3 on Fireworks/Venice/OpenRouter), and the zai rows' `api_id` defaulting to the slug matches the existing `glm_5_2_in_catalog` assertion pattern.

**Blocking defect — the cost triples are internally inconsistent, and nothing downstream can catch a transposition:**

The plan states the same three TOML keys (`input_cost_per_mtok` / `output_cost_per_mtok` / `cache_input_cost_per_mtok`) in **two different positional orders** without labeling either:
- zai/OpenRouter rows (summary table and §1/§2): input/output/cache — `1.4 / 4.4 / 0.26`, `0.075 / 0.25 / 0.015`
- Fireworks rows (summary table and §3): input/cache/output — `1.4 / 0.26 / 4.4`, `0.15 / 0.03 / 0.50` (mirroring Fireworks' own doc-table order)

An implementer mapping positionally — the natural reading when adjacent rows in the same summary table disagree — records `output = 0.26`, `cache = 4.40` for Fireworks GLM 5.3. That is user-visible billing-estimate data, and the plan's verification **cannot catch it**: no test anywhere asserts cost values, and the planned docs-vs-catalog grep would faithfully reproduce the same transposed numbers in both places. Wrong numbers in three TOML rows plus docs is exactly "work that would have to be redone." Fix: state every cost as explicit key/value pairs (or one labeled order everywhere).

**Advisory, not blocking:**
1. Verification runs `-p fabro-model -p fabro-llm -p fabro-server` + workspace build. Cross-crate consumers of `default_from_env`/`small_default_for_configured_ids` in other crates would only surface at CI; one full `cargo nextest run --workspace` is cheap insurance given the default-flag flip.
2. Rewriting `builtin_glm_5_2_aliases_are_portable` into the 5.3 version drops the only portability coverage for the `glm52`/`glm5.2` aliases that remain on glm-5.2 in both zai and OpenRouter — keep a one-line assertion for those.

Everything else — scope boundaries, alias surgery, limits choices, test plan, doc targets, deliberate exclusions — is correct against the repo as it stands.

{"preferred_next_label": "revise"}