All green. Implementation complete — here's the summary, file by file.

## Changes

**Catalog data (`lib/foundation/fabro-model/src/catalog/providers/`)**
- **`zai.toml`** — Added `glm-5.3` (provider default, portable aliases `glm`/`glm5`/`glm53`/`glm5.3`, text-only, efforts `low`/`high`/`max`, costs 1.4/4.4/0.26) and `glm-5.3-flash` (small default, native multimodal `vision = true`, same limits/efforts, promo costs 0.075/0.25/0.015 with expiry comment). `glm-5.2` lost `default = true` and its aliases shrank to `glm52`/`glm5.2`. Header comment documents the coding-endpoint contract and upstream auto-routing (5.2→5.3, 4.7→5.3-Flash).
- **`openrouter.toml`** — Added `z-ai/glm-5.3` (costs 1.4/4.4/0.14 from the live API) and `z-ai/glm-5.3-flash` (0.075/0.25/0.015); trimmed `glm-5.2` aliases the same way.
- **`fireworks.toml`** — Added `accounts/fireworks/models/glm-5p3` (1.4/4.4/0.26) and `glm-5p3-flash` (0.15/0.5/0.03, `vision = true`), mirroring the existing glm-5.2 row shape (no effort controls), with a verified-date comment.

**Catalog code + tests (`catalog.rs`)**
- Two `LEGACY_BUILTIN_MODEL_IDENTIFIERS` entries for the OpenRouter wire IDs.
- Rewrote `builtin_glm_5_2_aliases_are_portable` → `builtin_glm_aliases_are_portable`, covering both the new alias set on 5.3, the wire-ID normalization, and the retained `glm52`/`glm5.2` → 5.2 mapping on both providers.
- Updated the `glm_5_2_in_catalog` snapshot (aliases, `default: false`) and the OpenRouter glm-5.2 snapshot; added `glm_5_3_in_catalog` and `glm_5_3_flash_in_catalog` snapshot tests (default/small-default flags, settings, alias resolution).
- Added both new slugs to the fireworks+openrouter shared-slugs portability test and the fireworks expected-rows coverage table.

**Live e2e tests (`lib/components/fabro-llm/tests/integration.rs`)** — Added `zai_glm_5_3_reasoning_tool_round_trip` and `openrouter_glm_5_3_reasoning_tool_round_trip`, exact mirrors of the 5.2 precedent (reasoning-before-tool-call + authoritative-cost assertions). Existing 5.2 tests kept (upstream routing keeps them green).

**Docs** — `models.mdx` (two new zai rows, 5.2 alias shrink, default table `zai` → `glm-5.3`), `openrouter.mdx` and `fireworks.mdx` model tables, new `changelog/2026-09-04.mdx`, and a "September 2026" group in `docs.json` nav (the review's non-blocking note).

## Deviations from plan (and why)

1. **Two test updates the plan's file list didn't name:** `builtin_fireworks_models_when_enabled` (expected-rows table asserts it covers every Fireworks model) and `builtin_openrouter_includes_glm_5_2_when_enabled` (inline alias snapshot). The plan's grep step anticipated these; both were straightforward additions.
2. **Tooling substitutions:** `cargo nextest` and `cargo insta` aren't installed here, so I used `cargo test` and hand-edited inline snapshots (verified by re-run). The first full-workspace run was OOM-killed by the sandbox linker (`ld: signal 9`); re-ran with `-j 2` successfully.

## Verification

- `cargo test -p fabro-model`: **177 passed, 0 failed** (was 175; +2 new tests).
- `cargo test --workspace -j 2`: **72 test targets ok**; the single failure (`server_client::tests::http_target_transport_times_out_when_peer_accepts_without_http_response`) was proven to fail identically on the unmodified tree via `git stash` — it needs a `/root/.fabro` auth store this sandbox lacks.
- `cargo +nightly-2026-04-14 fmt --check --all`: clean (installed the pinned rustfmt component; it reformatted one assertion of mine).
- `docs.json` validated as JSON; cross-checked every doc number against the catalog values.
- Live Z.ai/OpenRouter round trips compile but can't run here (no keys); they'll run via `cargo nextest run -p fabro-llm --profile e2e --run-ignored only glm_5_3` with credentials.