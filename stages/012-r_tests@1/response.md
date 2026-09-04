## Test coverage review

**Verified green:** `cargo test -p fabro-model` → 177 passed, 0 failed.

### What's well covered

- **zai `glm-5.3` / `glm-5.3-flash` entries** — full insta snapshots pin every field (limits, features, `[Low, High, Max]` effort controls, all three costs, aliases, `default`/`small_default` flags), plus `api_id`, global alias resolution (`get("glm")`/`glm5`/`glm53`/`glm5.3` → `glm-5.3`), and an explicit `small_default_for_provider(zai)` assertion.
- **The default flip** — pinned from both sides: `glm_5_2_in_catalog` snapshot now asserts `default: false` and the shrunk alias list; `glm_5_3_in_catalog` asserts `default: true`. Re-marking 5.2 as default fails its snapshot.
- **Fireworks rows** — `builtin_fireworks_models_when_enabled` pins id, `api_id`, family, limits, vision, reasoning, and all three cost values for both new models, with an exhaustive completeness assert ("expected rows must cover every Fireworks model") so future additions can't skip the table.
- **Portability & legacy wire IDs** — `builtin_glm_aliases_are_portable` covers the new aliases and `z-ai/glm-5.3[-flash]` normalization on both providers plus retained `glm52`/`glm5.2` → 5.2; `every_legacy_builtin_identifier_targets_an_existing_offering` iterates the full legacy table, so the two new entries are covered generically too (contrary to the earlier devils note).
- **Live round trips** — exact mirrors of the 5.2 precedent (reasoning-before-tool-call, tool round trip, authoritative cost on OpenRouter).

### Finding 1 (the one real gap): OpenRouter `glm-5.3` / `glm-5.3-flash` catalog data is unpinned

Offline coverage for the new OpenRouter rows is **resolution-only** (portability, shared-slugs, legacy table). Nothing asserts `model_settings_on_provider(openrouter, "glm-5.3").api_id == "z-ai/glm-5.3"`, the costs (1.4/4.4/0.14 and 0.075/0.25/0.015), limits, or the `[low, high, max]` effort levels. The repo's own precedent is `builtin_openrouter_includes_glm_5_2_when_enabled`, which snapshots the full entry and asserts `api_id` — and per-provider drift is real here (glm-5.2 on OpenRouter has `[High, XHigh]`, not zai's set). Specific defects no offline test would catch: transposed `input`/`output` costs in `openrouter.toml`, or a wrong `api_id` (e.g. missing the `z-ai/` prefix → runtime 404 for every OpenRouter user). The live `openrouter_glm_5_3_reasoning_tool_round_trip` can't backstop this: it's credential-gated (skipped in CI), passes the wire id literally in `Request.model`, and asserts `CostSource::Authoritative` without comparing to the catalog estimate. One snapshot test matching the 5.2 pattern (or adding the two slugs with expected api_id/costs to a table test) closes it.

### Finding 2 (minor): Fireworks glm-5.3's deliberate lack of effort controls is untested

The Fireworks table test asserts effort controls only for the kimi rows. Nothing asserts `controls.reasoning_effort` is empty for the new glm rows, so someone adding `controls.reasoning_effort = [...]` to Fireworks glm-5.3 (contradicting the plan's "no effort controls" design) breaks no test. Matches the 5.2 precedent (also unasserted), so a nit.

### Finding 3 (minor): no live coverage for `glm-5.3-flash` anywhere

Flash is the first vision-capable zai model (`vision = true` is new capability surface for that provider), yet neither live e2e exercises it — its actual API behavior (vision input, effort levels, promo pricing) is verified only as static TOML snapshots. Flagship-only live tests match the 5.2 precedent, so this is a judgment call, not a defect.

### Verdict

Coverage is genuinely mutation-resistant everywhere a test exists — the zai and Fireworks sides would fail on almost any wrong field, and the tests assert behavior (resolution outcomes, snapshots of public catalog data) rather than internals. The single substantive gap is the unpinned OpenRouter data rows (Finding 1); Findings 2–3 are precedent-consistent nits.