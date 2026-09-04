## Implementation Plan: z.ai GLM-5.3 + GLM-5.3-Flash support

**Working decisions** (no explicit answers came back to my two questions, so I'm locking in the defaults I recommended — flag now if wrong):
- **Coverage = (b)**: zai + openrouter + fireworks (every provider already serving `glm-5.2`; Fireworks demonstrably serves both new models). Venice keeps its existing `glm-5.3` row, no Flash row. Bedrock untouched.
- **Pricing = (a)**: stable list prices everywhere — GLM-5.3 at 1.40/4.40/0.26, Flash at 0.15/0.50/0.03 (input/output/cache per Mtok). List = modal price across OpenRouter providers; the 50%-off Flash promo ends Sep 9 and would go stale. Existing `glm-5.2` rows are left untouched.

---

### 1. Files to modify

**A. `lib/foundation/fabro-model/src/catalog/providers/zai.toml`**
- Add `[providers.zai.models."glm-5.3"]` above `glm-5.2`: `display_name = "GLM 5.3"`, `family = "glm-5"`, `default = true`, `aliases = ["glm", "glm5", "glm53", "glm5.3"]`; `limits` 1048576/131072; `features` tools=true, vision=false, reasoning=true, `reasoning_effort = "levels"`, prompt_cache=true; `controls.reasoning_effort = ["low", "high", "max"]`; `costs` 1.4/4.4/0.26. (`reasoning_by_default` omitted — defaults to true for effort-capable models, matching the existing glm-5.2 row.)
- Add `[providers.zai.models."glm-5.3-flash"]`: `display_name = "GLM 5.3 Flash"`, `family = "glm-5"`, `small_default = true`, `aliases = ["glm-flash", "glm53-flash", "glm5.3-flash"]`; limits 1048576/131072; features tools=true, **vision=true**, reasoning=true, `reasoning_effort = "levels"`, prompt_cache=true; controls `["low", "high", "max"]`; costs **0.15/0.50/0.03**.
- Demote `glm-5.2`: delete `default = true`, shrink aliases to `["glm52", "glm5.2"]` (moving `glm`/`glm5` to 5.3, exactly the 5.2-over-4.7 precedent). `glm-4.7` unchanged.

**B. `lib/foundation/fabro-model/src/catalog/providers/openrouter.toml`**
- Add `glm-5.3`: `api_id = "z-ai/glm-5.3"`, `display_name = "GLM 5.3 (via OpenRouter)"`, `family = "glm-5"`, aliases `["glm", "glm5", "glm53", "glm5.3"]`, limits 1048576/131072, features mirroring zai (vision=false, levels, prompt_cache), controls `["low", "high", "max"]` (OpenRouter spells the 5.3 tier `max`, not `xhigh` — unlike its 5.2 row), costs 1.4/4.4/0.26.
- Add `glm-5.3-flash`: `api_id = "z-ai/glm-5.3-flash"`, display "GLM 5.3 Flash (via OpenRouter)", aliases `["glm-flash", "glm53-flash", "glm5.3-flash"]`, vision=true, same controls, costs 0.15/0.50/0.03.
- Edit `glm-5.2` aliases to `["glm52", "glm5.2"]` — required, or the build fails on duplicate per-provider selectors. Pricing/limits untouched.

**C. `lib/foundation/fabro-model/src/catalog/providers/fireworks.toml`**
- Add `glm-5.3` → `api_id = "accounts/fireworks/models/glm-5p3"` and `glm-5.3-flash` → `"accounts/fireworks/models/glm-5p3-flash"` (dots→`p` rule), display names "(via Fireworks)", family "glm-5", limits 1048576/131072, features/controls mirroring zai (incl. vision=true on Flash), no aliases (matches the alias-free glm-5.2 row).
- **Pre-write research step**: fetch `docs.fireworks.ai/serverless/pricing` for these two rows and use their numbers (the file's header convention: prices from that page, cache likely 10% of input as on their glm-5.2 row — expect ~1.4/4.4/0.14 and ~0.15/0.50/0.015; if the page lags, fall back to the OpenRouter Fireworks provider rows: 1.40/4.40/0.26 and 0.15/0.50/0.03). Whatever numbers land in the TOML must match the tuple-table test in D.

**D. `lib/foundation/fabro-model/src/catalog.rs`** (tests only)
- Update `glm_5_2_in_catalog` inline snapshot: `default: true` → `false`, aliases `["glm52","glm5.2"]`, drop the `get("glm")`/`get("glm5")` assertions or repoint them at `glm-5.3`.
- Rewrite `builtin_glm_5_2_aliases_are_portable` → `builtin_glm_5_3_aliases_are_portable`: `["glm","glm5","glm53","glm5.3"]` resolve to `glm-5.3` on both zai and openrouter.
- Add `glm_5_3_in_catalog` (snapshot + controls `["low","high","max"]` + `get("glm")`/`get("glm5")` → glm-5.3) and `glm_5_3_flash_in_catalog` (snapshot incl. `small_default: true`, `vision: true`, `get("glm-flash")` resolution).
- Add `builtin_openrouter_includes_glm_5_3_when_enabled` snapshot (mirroring the 5.2 one, `api_id z-ai/glm-5.3`).
- Extend the Fireworks tuple-table test (~line 4180) with both new rows and the resolve-on-provider id list (~line 4351).
- Grep for any other test asserting the zai default is `glm-5.2` and update.

**E. `lib/components/fabro-llm/tests/integration.rs`**
- Add `zai_glm_5_3_reasoning_tool_round_trip` (`#[e2e_test(live("ZAI_API_KEY"))]`) and `openrouter_glm_5_3_reasoning_tool_round_trip` (`live("OPENROUTER_API_KEY")`), direct copies of the 5.2 tests with model ids `glm-5.3` / `z-ai/glm-5.3`.

**F. Docs**
- `docs/public/core-concepts/models.mdx`: update the `glm-5.2` zai row aliases to `glm52, glm5.2`; add a `glm-5.3` zai row (aliases `glm, glm5, glm53, glm5.3`, 1M, $1.40/$4.40) and a `glm-5.3-flash` zai row (aliases `glm-flash, glm53-flash, glm5.3-flash`, 1M, $0.15/$0.50). Leave the venice `glm-5.3` row as-is (it's a distinct offering, like kimi-k3 vs kimi-k3-fast).
- `docs/public/integrations/openrouter.mdx` (~line 59): GLM row becomes `glm-5.3`, `glm-5.3-flash`, `glm-5.2`, `glm-4.6` with `z-ai/...` api ids.
- `docs/public/integrations/fireworks.mdx` (~line 56): add `glm-5.3` → `accounts/fireworks/models/glm-5p3` and `glm-5.3-flash` → `glm-5p3-flash`.
- `docs/public/changelog/2026-09-04.mdx` (new): mirror the frontmatter of the newest existing changelog file; Improvements bullet "Added GLM 5.3 and GLM 5.3 Flash through Z.AI, OpenRouter, and Fireworks".

### 2. Order

1. Fireworks/OpenRouter pricing verification fetch (only remaining research).
2. A (zai.toml) → run `-p fabro-model` to surface the expected snapshot failures.
3. B, C (openrouter + fireworks TOMLs).
4. D (catalog tests; `cargo insta pending-snapshots` → verify each pending diff is the intended default/alias flip → accept).
5. E (live tests — compile-verified now, run later if creds exist).
6. F (docs).
7. Full verification pass.

### 3. Verification

- `cargo nextest run -p fabro-model` — catalog build rules (single default, alias uniqueness) + all updated/new tests.
- `cargo nextest run -p fabro-llm` — `every_builtin_catalog_offering_resolves` iterates the catalog, proving the new rows route.
- `cargo nextest run -p fabro-server` — `list_models` and conformance.
- `cargo nextest run --workspace` — catches downstream consumers (e.g., fabro-workflow fallback tests referencing `glm-5.2`).
- `cargo insta pending-snapshots` before `accept` — never blind-accept; expected pendings are exactly the glm-5.2 default/alias snapshot plus new glm-5.3 snapshots.
- `cargo +nightly-2026-04-14 fmt --check --all` and `cargo +nightly-2026-04-14 clippy --workspace --all-targets -- -D warnings`.
- Optional live check (only if `ZAI_API_KEY`/`OPENROUTER_API_KEY` in `.env`): `set -a && source .env && set +a && cargo nextest run -p fabro-llm --profile e2e --run-ignored only glm_5_3`. If no creds, state that these were compile-checked only.
- Docs: no automated checker for mdx known — manual read-through of the three edited pages + new changelog against the TOML values.

### 4. Deliberately not doing

- **No Venice `glm-5.3-flash` row** — Venice's own api id for it is unverified; Venice's catalog was curated separately.
- **No Bedrock rows** — Bedrock carries only the older `zai.glm-5`; no evidence AWS lists 5.3.
- **No codec/adapter/agent-profile code changes** — zai rides `openai_compatible`, which already sends top-level `reasoning_effort` and parses `reasoning_content`; z.ai's docs make `thinking.type` optional-default-`enabled`, so no request change is needed. Confirmed against the GLM 5.2 commit, which needed wire changes only for OpenRouter's `reasoning` spelling — already shipped.
- **No refresh of glm-5.2's OpenRouter pricing** (0.784/2.464/0.1456) — list-price convention chosen; avoid unrelated churn.
- **No removal of `glm-4.7`/`glm-4.6`**, no `LEGACY_BUILTIN_MODEL_IDENTIFIERS` additions (post-migration precedent: GPT-5.6 added none), no OpenAPI/TS-client regeneration (spec unchanged), no `estimated_output_tps` (glm-5.2 has none; z.ai publishes no authoritative figure).