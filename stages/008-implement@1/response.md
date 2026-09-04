Implementation complete. Summary of what changed, file by file:

## Catalog data

**`lib/foundation/fabro-model/src/catalog/providers/zai.toml`** — Added `glm-5.3` (text-only flagship, `default = true`, aliases `glm`/`glm5`/`glm53`/`glm5.3`, efforts `low|high|max`, 1,048,576 ctx / 131,072 out, $1.40/$4.40/$0.26 per z.ai's pricing page) and `glm-5.3-flash` (vision=true per z.ai's multimodal docs, `small_default = true`, aliases `glm-flash`/`glm53-flash`/`glm5.3-flash`, $0.15/$0.50/$0.03 list prices). Demoted `glm-5.2` — lost `default`, aliases shrunk to `glm52`/`glm5.2` (5.2-over-4.7 precedent).

**`lib/foundation/fabro-model/src/catalog/providers/openrouter.toml`** — Added `z-ai/glm-5.3` and `z-ai/glm-5.3-flash` rows with matching aliases/features/controls (OpenRouter spells the 5.3 top tier `max`, not `xhigh`), priced at the list/modal provider rates. Trimmed `glm`/`glm5` off the `glm-5.2` row (required — per-provider selectors must be unique); its pricing/limits untouched per plan.

**`lib/foundation/fabro-model/src/catalog/providers/fireworks.toml`** — Added `accounts/fireworks/models/glm-5p3` and `glm-5p3-flash` with numbers verified from docs.fireworks.ai's own pricing page (5.3 cache is $0.26 there, not the 5.2 row's $0.14).

## Tests

**`lib/foundation/fabro-model/src/catalog.rs`** — Renamed `builtin_glm_5_2_aliases_are_portable` → glm-5.3 version; updated the two glm-5.2 inline snapshots (default flip + alias shrink, including the OpenRouter one flagged in review); added `glm_5_3_in_catalog`, `glm_5_3_flash_in_catalog` (snapshot + alias/default/small-default assertions), and `builtin_openrouter_includes_glm_5_3_when_enabled`; extended the exhaustive Fireworks tuple table and the shared-slug portability list.

**`lib/components/fabro-llm/tests/integration.rs`** — Added `zai_glm_5_3_reasoning_tool_round_trip` and `openrouter_glm_5_3_reasoning_tool_round_trip` live e2e tests mirroring the 5.2 pair. (No credentials in this environment, so they are compile-verified and registered only — confirmed via `-- --list`.)

## Docs

- **`docs/public/core-concepts/models.mdx`** — added zai `glm-5.3`/`glm-5.3-flash` rows, corrected `glm-5.2` aliases.
- **`docs/public/integrations/openrouter.mdx`**, **`fireworks.mdx`** — updated model tables.
- **`docs/public/changelog/2026-09-04.mdx`** (new) + **`docs/public/docs.json`** — new "September 2026" changelog nav group (the review-flagged gap).

## Adaptations from plan / environment notes

- `cargo nextest` isn't installed here; used `cargo test` per-crate instead. One gotcha: rapid successive edits produced stale binaries once — forced recompile confirmed green.
- Three parallel `edit_file` calls to the same file raced and two were silently lost; redone sequentially. Also repaired the `openrouter_poolside_laguna_complete` signature my insertion anchor consumed.
- Workspace-wide test run OOMs at link time in this container (ld killed); mitigated by running every catalog-consuming crate individually: fabro-model 178/178, fabro-llm 541/542 (1 pre-existing flaky log test, passes isolated), fabro-server 897/903 (3 graphviz-rendering failures reproduce on a clean tree via `git stash`; 3 storage tests flaky under load, pass isolated), fabro-workflow 1444/1444, fabro-agent 645/645, fabro-cli 455/457 (2 log-env tests pass isolated, pre-existing cross-talk).
- `cargo +nightly-2026-04-14 fmt --check --all` clean; clippy `-D warnings` clean on fabro-model and fabro-llm; docs.json validates as JSON.