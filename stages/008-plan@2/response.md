# Implementation Plan (rev 2): z.ai GLM 5.3 + GLM 5.3 Flash

**Changes from rev 1, addressing the critique:**
- **Fixed the blocking defect:** all costs are now stated as explicit `key = value` pairs (never positional triples), so no transposition between `input`/`output`/`cache` is possible. The rev-1 summary table mixed input/output/cache order (zai, OpenRouter) with input/cache/output order (Fireworks, mirroring Fireworks' own doc table) — nothing downstream could have caught a mis-mapping.
- **Advisory 1 adopted:** verification now includes one full `cargo nextest run --workspace` pass, since the zai default-flag flip can surface in cross-crate consumers of `default_from_env`/`small_default_for_configured_ids` that the three-crate run misses.
- **Advisory 2 adopted:** the rewritten portability test keeps coverage for the `glm52`/`glm5.2` aliases that remain on glm-5.2.

## Resolved parameters

Two decisions carried from the question round; all other numbers researched from Z.ai docs, OpenRouter's live API, and Fireworks' pricing page (2026-09-04):

| | GLM 5.3 | GLM 5.3 Flash |
|---|---|---|
| Slug / family | `glm-5.3` / `glm-5` | `glm-5.3-flash` / `glm-5` |
| Context / max output | 1,048,576 / 131,072 | 1,048,576 / 131,072 |
| Vision | false | true (native multimodal) |
| Reasoning efforts | `low`, `high`, `max` | `low`, `high`, `max` |
| zai flags | `default = true` | `small_default = true` |

Costs (per MTok, by explicit TOML key):

| Row | `input_cost_per_mtok` | `output_cost_per_mtok` | `cache_input_cost_per_mtok` |
|---|---|---|---|
| zai `glm-5.3` | 1.4 | 4.4 | 0.26 |
| zai `glm-5.3-flash` | 0.075 | 0.25 | 0.015 |
| openrouter `glm-5.3` | 1.4 | 4.4 | 0.14 |
| openrouter `glm-5.3-flash` | 0.075 | 0.25 | 0.015 |
| fireworks `glm-5.3` | 1.4 | 4.4 | 0.26 |
| fireworks `glm-5.3-flash` | 0.15 | 0.5 | 0.03 |

Flash rows on zai/OpenRouter record current advertised (promo) rates with a comment that the Z.ai 50% launch promo ends 2026-09-09 (list: input 0.15 / output 0.50 / cache 0.03); Fireworks already charges list.

## Files and changes

### 1. `lib/foundation/fabro-model/src/catalog/providers/zai.toml`

- **Add `glm-5.3`** above `glm-5.2`: `display_name = "GLM 5.3"`, `default = true`, `aliases = ["glm", "glm5", "glm53", "glm5.3"]`; `limits`: `context_window = 1048576`, `max_output = 131072`; `features`: `tools = true`, `vision = false`, `reasoning = true`, `reasoning_effort = "levels"`, `prompt_cache = true`; `controls`: `reasoning_effort = ["low", "high", "max"]`; `costs` per the table above. Comment: served on the same coding endpoint; `thinking.type` only accepts `enabled`.
- **Add `glm-5.3-flash`**: `display_name = "GLM 5.3 Flash"`, `small_default = true`, no aliases; same limits/controls; features identical to 5.3 except `vision = true`; `costs` per table with the promo-expiry comment.
- **Edit `glm-5.2`**: remove `default = true`; shrink `aliases` to `["glm52", "glm5.2"]`.
- **Edit `glm-4.7`**: comment only — upstream auto-routes GLM-5.2/5.1 → GLM-5.3 and GLM-4.7 → GLM-5.3-Flash on the coding plan.

### 2. `lib/foundation/fabro-model/src/catalog/providers/openrouter.toml`

- **Add `glm-5.3`**: `api_id = "z-ai/glm-5.3"`, `display_name = "GLM 5.3 (via OpenRouter)"`, `family = "glm-5"`, `aliases = ["glm", "glm5", "glm53", "glm5.3"]`; limits 1048576/131072; features tools/reasoning-levels/prompt_cache, `vision = false`; controls `["low", "high", "max"]`; costs per table (1.4 / 4.4 / 0.14).
- **Add `glm-5.3-flash`**: `api_id = "z-ai/glm-5.3-flash"`, no aliases, `vision = true`, otherwise same shape; costs per table (0.075 / 0.25 / 0.015) with promo comment.
- **Edit `glm-5.2`**: shrink aliases to `["glm52", "glm5.2"]`.

### 3. `lib/foundation/fabro-model/src/catalog/providers/fireworks.toml`

- **Add `glm-5.3`**: `api_id = "accounts/fireworks/models/glm-5p3"`, `display_name = "GLM 5.3 (via Fireworks)"`, `family = "glm-5"`; limits 1048576/131072; features mirroring the existing fireworks `glm-5.2` row (tools, reasoning, prompt_cache; **no effort controls** — unverifiable that Fireworks exposes `reasoning_effort`, and the 5.2 row omits it); costs: input 1.4 / output 4.4 / cache 0.26.
- **Add `glm-5.3-flash`**: `api_id = "accounts/fireworks/models/glm-5p3-flash"`, same shape with `vision = true`; costs: input 0.15 / output 0.5 / cache 0.03.
- Row comments noting prices verified 2026-09-04.

### 4. `lib/foundation/fabro-model/src/catalog.rs`

- **`LEGACY_BUILTIN_MODEL_IDENTIFIERS`** (~1832): insert `("z-ai/glm-5.3", "openrouter", "glm-5.3")` and `("z-ai/glm-5.3-flash", "openrouter", "glm-5.3-flash")` beside `("z-ai/glm-5.2", ...)`.
- **Rewrite `builtin_glm_5_2_aliases_are_portable`** (~3447) → `builtin_glm_aliases_are_portable`: for `zai` + enabled `openrouter`, `glm`/`glm5`/`glm53`/`glm5.3` → `glm-5.3`, **and `glm52`/`glm5.2` → `glm-5.2`** (retains the coverage the critique asked for).
- **`glm_5_2_in_catalog`** (~7222): update inline snapshot (aliases shrink to `glm52`/`glm5.2`, `default: false`); trailing `get("glm")`/`get("glm5")` assertions become `get("glm52")`/`get("glm5.2")` → `glm-5.2`.
- **Add `glm_5_3_in_catalog`** and **`glm_5_3_flash_in_catalog`** in the same style (inline insta snapshots; assert `default = true` on 5.3, `small_default = true` on flash, alias resolution, and `model_settings("glm-5.3").api_id == "glm-5.3"`).
- **`builtin_deepseek_shared_slugs_are_portable_across_providers`** (~4333): add `"glm-5.3"`, `"glm-5.3-flash"` to the fireworks+openrouter id list.
- Grep for any other `glm-5.2`-as-zai-default assertions; the catalog integrity tests (`every_provider_has_exactly_one_default_model`, `builtin_small_defaults_are_marked_per_provider`) cover the flag flip with no edits.

### 5. `lib/components/fabro-llm/tests/integration.rs`

- Add `zai_glm_5_3_reasoning_tool_round_trip` (`live("ZAI_API_KEY")`) and `openrouter_glm_5_3_reasoning_tool_round_trip` (`live("OPENROUTER_API_KEY")`) — mirrors of the 5.2 tests (multiply-tool round trip, reasoning-before-tool-call assertion, authoritative-cost assertion on the OpenRouter one). Keep the 5.2 tests; upstream 5.2→5.3 auto-routing keeps them green.

### 6. Docs

- **`docs/public/core-concepts/models.mdx`**: update `glm-5.2` row (aliases → `glm52`, `glm5.2`); add zai `glm-5.3` (aliases `glm`, `glm5`, `glm53`, `glm5.3`; $1.40 / $4.40) and `glm-5.3-flash` ($0.075 / $0.25) rows; leave the venice `glm-5.3` row alone; default table `zai` → `glm-5.3`.
- **`docs/public/integrations/openrouter.mdx`** (~59): row becomes `glm-5.3` (`glm`, `glm5`, `glm53`, `glm5.3`), `glm-5.3-flash`, `glm-5.2`, `glm-4.6` with `z-ai/...` API IDs.
- **`docs/public/integrations/fireworks.mdx`** (~56): add `glm-5.3` → `glm-5p3` and `glm-5.3-flash` → `glm-5p3-flash` rows.
- **`docs/public/changelog/2026-09-04.mdx`** (new; no collision — latest is 2026-08-31): "GLM 5.3 and GLM 5.3 Flash" — providers, new zai default, alias moves, small_default, Z.ai auto-routing note, flash promo note.

## Order

1. `zai.toml` → `openrouter.toml` → `fireworks.toml` (data first)
2. `catalog.rs` legacy-identifier table + test updates; `cargo nextest run -p fabro-model`; review `cargo insta pending-snapshots`, accept selectively
3. `integration.rs` live tests (compile-checked only — no keys in this sandbox)
4. Docs + changelog
5. Stale-reference grep + full verification pass

## Verification

- `cargo nextest run -p fabro-model` — catalog build + portability + snapshot tests.
- `cargo nextest run --workspace` — one full pass (added per critique; catches cross-crate consumers of the default/small-default flags that the targeted run misses).
- `cargo build --workspace`; `cargo +nightly-2026-04-14 fmt --all` (clippy with the same pinned toolchain if installed).
- `grep -rn "glm" lib/ docs/public/ apps/ --include="*.toml" --include="*.rs" --include="*.mdx"` — confirm every doc number matches the catalog keys above exactly (this is the only guard on cost values; there are no cost assertions in tests).
- Live round trips **cannot run here** (no `ZAI_API_KEY`/`OPENROUTER_API_KEY`); they'd run via `set -a && source .env && set +a && cargo nextest run -p fabro-llm --profile e2e --run-ignored only glm_5_3`. Noted for the maintainer to trigger.

## Deliberately not doing

- **No Venice `glm-5.3-flash`** — Venice doesn't offer it; a row would be fabricated pricing.
- **No Bedrock changes** — Bedrock carries only GLM 4.7/4.7-Flash/5 today.
- **No Rust adapter/codec changes** — the OpenAI-compatible codec already handles `reasoning_content` (Z.ai) and `reasoning` (OpenRouter); no Rust model enums exist.
- **No removal/repricing of `glm-5.2`/`glm-4.7`/`glm-4.6` rows**; no alias changes on Venice's existing `glm-5.3` row.
- **Not recording OpenRouter's 262,144 max-completion outlier** — Z.ai's documented 128K wins, matching family precedent.
- **No `estimated_output_tps`, no `probe` flag, no flash aliases, no OpenAPI/client regeneration** (models are runtime data; no schema change).
- **No fireworks `small_default`** — Fireworks keeps `gpt-oss-20b`; only one per provider is allowed.