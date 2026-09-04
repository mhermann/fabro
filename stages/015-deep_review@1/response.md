All checks are complete. Here is my review.

## What I verified myself

**The implemented change is mechanically sound:**
- `zai.toml` / `openrouter.toml` / `fireworks.toml` rows match the repo's GLM-5.2-over-4.7 default-flip precedent (`c67cfef1`): new flagship takes `default = true` and portable aliases `glm`/`glm5`/`glm53`/`glm5.3`, `glm-5.2` keeps `glm52`/`glm5.2`, one default per provider preserved.
- Alias collision with Venice's existing `glm-5.3` row is legal (duplicate-selector validation compares model ids, which match) and resolution is priority-ordered (zai 60 > venice 35), which the tests assert.
- Omitting `reasoning_by_default` is correct — it defaults to `supports_reasoning_effort()` (true for `levels`), matching the glm-5.2 row.
- Tests: 178/178 `fabro-model`, 155/155 `fabro-llm` pass; new live e2e tests compile and are properly gated; `fmt --check` and `clippy -D warnings` clean. The 3 `fabro-server` failures are pre-existing parallel-load flakes in object-store/install code (pass in isolation on both this branch and `origin/main`).
- Pricing matches the human's "2a" (list prices everywhere) decision and the recorded research (1.40/4.40/0.26; Flash 0.15/0.50/0.03). The stale `.catalog.rs.pending-snap` file is gitignored (I removed it locally).

**But the change does not fully do what was asked.** The recorded human gate answer is `1c, z-ai-glm-5-3-flash` / `2a`. The pricing half (`2a`) was honored. The coverage half was not: the plan blob admits it "locked in defaults" because it believed "no explicit answers came back" — yet the interview stage succeeded *before* the plan stage and the answer is in run context. The plan's own "Deliberately not doing" section says "No Venice `glm-5.3-flash` row — Venice's own api id for it is **unverified**", and the human's freeform answer supplies exactly that unverified piece: `z-ai-glm-5-3-flash`, which is Venice's api-id convention (`venice.toml`'s flagship row uses `api_id = "z-ai-glm-5-3"`; no other provider's id matches this string — zai uses `glm-5.3-flash`, OpenRouter `z-ai/glm-5.3-flash`, Fireworks `accounts/fireworks/models/glm-5p3-flash`). The understand stage's coverage options were (a) providers already carrying glm-5.2 (= exactly what was implemented), (b) plus Venice Flash, (c) anywhere fetchable; the human chose `1c`. Research confirmed Venice serves Flash at 0.15/0.50/0.03. Bedrock remains correctly excluded (older `glm-5` only).

## Defects (route: fix)

**Defect 1 — Missing Venice `glm-5.3-flash` row (human instruction not implemented).**
- File: `lib/foundation/fabro-model/src/catalog/providers/venice.toml` — add `[providers.venice.models."glm-5.3-flash"]` with `api_id = "z-ai-glm-5-3-flash"`, `display_name = "GLM 5.3 Flash"`, `family = "glm-5"`, aliases mirroring zai's flash aliases (`glm-flash`, `glm53-flash`, `glm5.3-flash` — Venice carries portable aliases on its glm-5.3 row), `limits` 1000000/131072 (Venice's own convention), features `tools=true, vision=true, reasoning=true, reasoning_effort="levels", reasoning_by_default=true, prompt_cache=true`, `controls.reasoning_effort = ["low","high","max"]`, costs 0.15/0.50/0.03. Do **not** set `small_default` (Venice has none; fireworks flash rows don't either).
- Docs: `docs/public/integrations/venice.mdx` — add rows to all three tables (Included models, Reasoning effort values, Pricing) mirroring the `glm-5.3` rows; `docs/public/core-concepts/models.mdx` — add the venice flash row; `docs/public/changelog/2026-09-04.mdx` — "through Z.ai, OpenRouter, and Fireworks" → include Venice.
- Tests: add a `glm-5.3-flash` venice assertion (snapshot or `resolve_on_provider`) in `catalog.rs` following `glm_5_3_flash_in_catalog`'s pattern; rerun `cargo test -p fabro-model` and `cargo test -p fabro-llm`.

**Defect 2 — Stale default-model table in docs.**
- File: `docs/public/core-concepts/models.mdx` line 245: `| zai | glm-5.2 |` → `| zai | glm-5.3 |`. The change edits this file but missed the "Default models" table; the row was correct before this change and is now wrong.

{"preferred_next_label": "fix"}