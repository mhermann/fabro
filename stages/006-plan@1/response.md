# Implementation Plan: z.ai GLM 5.3 + GLM 5.3 Flash

## Resolved parameters

Carrying forward the two decisions from the question round, plus the researched data everything else derives from:

| | GLM 5.3 | GLM 5.3 Flash |
|---|---|---|
| Slug / family | `glm-5.3` / `glm-5` | `glm-5.3-flash` / `glm-5` |
| Context / max output | 1,048,576 / 131,072 | 1,048,576 / 131,072 |
| Vision | false (text-only per Z.ai + OpenRouter) | true (native multimodal) |
| Efforts | `low`, `high`, `max` | `low`, `high`, `max` |
| zai costs (per MTok) | 1.4 / 4.4 / 0.26 | 0.075 / 0.25 / 0.015 *(current advertised)* |
| openrouter costs | 1.4 / 4.4 / 0.14 | 0.075 / 0.25 / 0.015 |
| fireworks costs | 1.4 / 0.26 / 4.4 | 0.15 / 0.03 / 0.50 *(Fireworks charges list)* |

- Flash pricing records **each provider's current advertised rate** with a TOML comment noting the Z.ai 50% launch promo ends 2026-09-09 (list: $0.15/$0.50/$0.03).
- `glm-5.3-flash` gets **`small_default = true` on zai** (deepseek-v4-flash / gpt-oss-20b precedent; Fireworks keeps `gpt-oss-20b` as its small default — only one allowed per provider).
- `glm-5.3` becomes the **zai provider default**; `glm-5.2` drops the flag.

## Files and changes

### 1. `lib/foundation/fabro-model/src/catalog/providers/zai.toml`

- **Add `glm-5.3`** above `glm-5.2`: `display_name = "GLM 5.3"`, `default = true`, `aliases = ["glm", "glm5", "glm53", "glm5.3"]`; limits `context_window = 1048576`, `max_output = 131072`; features `tools = true`, `vision = false`, `reasoning = true`, `reasoning_effort = "levels"`, `prompt_cache = true`; controls `reasoning_effort = ["low", "high", "max"]`; costs `1.4 / 4.4 / 0.26`. Comment noting Z.ai serves it on the same coding endpoint and `thinking.type` only accepts `enabled`.
- **Add `glm-5.3-flash`**: `display_name = "GLM 5.3 Flash"`, `small_default = true`, no aliases; same limits; features same as 5.3 except `vision = true`; same controls; costs `0.075 / 0.25 / 0.015` with the promo-expiry comment.
- **Edit `glm-5.2`**: remove `default = true`; shrink `aliases` to `["glm52", "glm5.2"]` (the portable `glm`/`glm5` move to 5.3).
- **Edit `glm-4.7`**: comment only — upstream auto-routes GLM-5.2/5.1 → GLM-5.3 and GLM-4.7 → GLM-5.3-Flash on the coding plan.

### 2. `lib/foundation/fabro-model/src/catalog/providers/openrouter.toml`

- **Add `glm-5.3`**: `api_id = "z-ai/glm-5.3"`, `display_name = "GLM 5.3 (via OpenRouter)"`, `family = "glm-5"`, `aliases = ["glm", "glm5", "glm53", "glm5.3"]`; limits 1048576/131072; features tools/reasoning-levels/prompt_cache, `vision = false`; controls `["low", "high", "max"]`; costs `1.4 / 4.4 / 0.14` (live OpenRouter rates).
- **Add `glm-5.3-flash`**: `api_id = "z-ai/glm-5.3-flash"`, no aliases, `vision = true`, otherwise same shape; costs `0.075 / 0.25 / 0.015`.
- **Edit `glm-5.2`**: shrink aliases to `["glm52", "glm5.2"]`.

### 3. `lib/foundation/fabro-model/src/catalog/providers/fireworks.toml`

- **Add `glm-5.3`**: `api_id = "accounts/fireworks/models/glm-5p3"`, `display_name = "GLM 5.3 (via Fireworks)"`, `family = "glm-5"`; limits 1048576/131072; features mirroring the existing fireworks `glm-5.2` row (tools, reasoning, prompt_cache; no effort controls — unverifiable that Fireworks exposes `reasoning_effort`, and the 5.2 row omits it); costs `1.4 / 0.26 / 4.4`.
- **Add `glm-5.3-flash`**: `api_id = "accounts/fireworks/models/glm-5p3-flash"`, same shape with `vision = true`; costs `0.15 / 0.03 / 0.50`.
- Row comments noting prices verified 2026-09-04.

### 4. `lib/foundation/fabro-model/src/catalog.rs`

- **`LEGACY_BUILTIN_MODEL_IDENTIFIERS`** (~line 1832): insert `("z-ai/glm-5.3", "openrouter", "glm-5.3")` and `("z-ai/glm-5.3-flash", "openrouter", "glm-5.3-flash")` next to the existing `z-ai/glm-5.2` entry.
- **Rewrite `builtin_glm_5_2_aliases_are_portable`** (~3447) → `builtin_glm_5_3_aliases_are_portable`: for `zai` + enabled `openrouter`, aliases `glm`, `glm5`, `glm53`, `glm5.3` must resolve to `glm-5.3`.
- **`glm_5_2_in_catalog`** (~7222): inline snapshot changes (aliases shrink to `glm52`/`glm5.2`, `default: false`); trailing `get("glm")`/`get("glm5")` assertions change to `get("glm52")`/`get("glm5.2")` → `glm-5.2`.
- **Add `glm_5_3_in_catalog`** and **`glm_5_3_flash_in_catalog`** tests in the same style (inline insta snapshots; assert `default = true` on 5.3, `small_default = true` on flash, alias resolution, and `model_settings("glm-5.3").api_id == "glm-5.3"`).
- **`builtin_deepseek_shared_slugs_are_portable_across_providers`** (~4333): add `"glm-5.3"`, `"glm-5.3-flash"` to the fireworks+openrouter id list.
- Grep for any other `glm-5.2`-as-zai-default assertions and update; existing `zai` small-default coverage comes free via `builtin_small_defaults_are_marked_per_provider` (~4515).

### 5. `lib/components/fabro-llm/tests/integration.rs`

- Add `zai_glm_5_3_reasoning_tool_round_trip` (`live("ZAI_API_KEY")`) and `openrouter_glm_5_3_reasoning_tool_round_trip` (`live("OPENROUTER_API_KEY")`) — direct mirrors of the 5.2 tests (multiply-tool round trip, reasoning-before-tool-call assertion, authoritative-cost assertion on the OpenRouter one). Keep the 5.2 tests; upstream 5.2→5.3 auto-routing keeps them green.

### 6. Docs

- **`docs/public/core-concepts/models.mdx`**: update the `glm-5.2` row (aliases → `glm52`, `glm5.2`); add `glm-5.3` and `glm-5.3-flash` zai rows (aliases `glm`, `glm5`, `glm53`, `glm5.3` on 5.3; prices $1.40/$4.40 and $0.075/$0.25); leave the existing venice `glm-5.3` row alone; change the per-provider default table `zai` → `glm-5.3`.
- **`docs/public/integrations/openrouter.mdx`** (~line 59): row becomes `glm-5.3` (`glm`, `glm5`, `glm53`, `glm5.3`), `glm-5.3-flash`, `glm-5.2`, `glm-4.6` with `z-ai/...` API IDs.
- **`docs/public/integrations/fireworks.mdx`** (~line 56): add `glm-5.3`, `glm-5.3-flash` rows with `glm-5p3` / `glm-5p3-flash` API IDs.
- **`docs/public/changelog/2026-09-04.mdx`** (new): "GLM 5.3 and GLM 5.3 Flash" — providers added, new zai default, alias moves, small_default, Z.ai upstream auto-routing note, flash promo note.

## Order

1. `zai.toml` → `openrouter.toml` → `fireworks.toml` (data first)
2. `catalog.rs` legacy-identifier table + test updates; run `cargo nextest run -p fabro-model`, review pending snapshots with `cargo insta pending-snapshots`, accept selectively
3. `integration.rs` live tests (compile-checked only — no keys here)
4. Docs + changelog
5. Grep `glm-5.2`/`glm-5` across repo for stale references; full verification pass

## Verification

- `cargo nextest run -p fabro-model` — catalog build + portability + snapshot tests (the one-default-per-provider build error would catch a missed flag flip).
- `cargo build --workspace` and `cargo nextest run -p fabro-llm -p fabro-server` — compile + no drift in dependents.
- `cargo +nightly-2026-04-14 fmt --all` (and clippy if the pinned toolchain is installed).
- `grep -rn "glm" lib/ docs/public/ --include="*.toml" --include="*.rs" --include="*.mdx"` — confirm docs tables match catalog numbers exactly.
- Live round trips **cannot run here** (no `ZAI_API_KEY`/`OPENROUTER_API_KEY`); they'd run via `set -a && source .env && set +a && cargo nextest run -p fabro-llm --profile e2e --run-ignored only glm_5_3`. I'll note this in the PR description for the maintainer to trigger.

## Deliberately not doing

- **No Venice `glm-5.3-flash`** — Venice doesn't offer it; a row would be fabricated pricing.
- **No Bedrock changes** — Bedrock only carries GLM 4.7/4.7-Flash/5 today.
- **No Rust adapter/codec changes** — the OpenAI-compatible codec already handles both `reasoning_content` (Z.ai) and `reasoning` (OpenRouter) spellings; no Rust model enums exist anymore.
- **No removal or repricing of `glm-5.2`/`glm-4.7`/`glm-4.6` rows**, and no alias changes on Venice's existing `glm-5.3` row.
- **Not recording OpenRouter's 262,144 max-completion outlier** — Z.ai's documented 128K wins, matching family precedent.
- **No `estimated_output_tps`, no `probe` flag, no flash aliases, no OpenAPI/client regeneration** (models are dynamic; no schema change).