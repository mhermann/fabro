Review complete. I read the full diff (`git diff origin/main...HEAD`), the surrounding catalog code (`resolve_on_provider`, `build_model_indexes`, the legacy-ID table, the index/selection paths), both new live e2e tests side-by-side with their GLM 5.2 predecessors, all four docs files, and the history of the 5.2 landing (`c67cfef1`, `1511c84f`, `65cdf520`).

## Verdict: the diff is appropriately simple

The substantive change is pure data: three TOML catalogs get rows that byte-for-byte mirror the incumbent `glm-5.2` shape (same `family`, limits, features, controls; the only deltas are the values themselves and the intended `default`/`small_default`/`vision` flips). No new abstraction, no new code path, no new config surface.

Specifically checked and cleared against the checklist:

- **No dead code.** The two new `LEGACY_BUILTIN_MODEL_IDENTIFIERS` entries look suspicious (a "legacy" table getting entries for a brand-new model), but they're load-bearing: `provider_selector_index` only matches `api_id` on the *owning* provider, while unqualified `select()` (catalog.rs:996) normalizes through this table. Without the entries, bare `z-ai/glm-5.3` without a provider pin would fail. Same mechanism `z-ai/glm-5.2` already uses.
- **No config nobody sets.** Every field (`small_default`, `controls.reasoning_effort`, `cache_input_cost_per_mtok`, …) exists in the sibling rows and is consumed (`small_default_for_provider` is asserted in the new snapshot test).
- **No indirection.** The portability test rewrite from a 4-alias loop to an 8-entry `(alias, canonical_id)` table is the minimal way to express that aliases now split across two models.
- **No scope creep.** zai + OpenRouter + Fireworks matches the verified-availability decisions; Venice/Bedrock untouched; docs/changelog/`docs.json` nav are the standard one-line-per-artifact updates.

## One minor note (precedent-following, not a defect)

The two new live e2e tests (`integration.rs:558`, `:634`, ~164 lines) are near-verbatim copies of the GLM 5.2 tests and of each other — the only differences are adapter construction, the model string, and two OpenRouter-only `CostSource::Authoritative` asserts. This is now the **fourth and fifth copy** of the same ~80-line reasoning+tool round-trip shape in this file. A collapse exists: this same file already demonstrates the thin-wrapper convention (`assert_deep_tool_round_trip` + `run_model_test` at line 61). A `glm_reasoning_tool_round_trip(adapter, model, …)` helper would shrink this addition to ~60 lines and make the GLM 5.4 bump a 10-line change instead of another 80.

I'm flagging it as optional rather than required because the copy-per-model style is exactly what the GLM 5.2 and Kimi K3 tests established, and the task instruction was "use precedent." If a maintainer wants to pay it down, do it as a standalone refactor of all four GLM tests together — not piecemeal in this change.

Nothing else to delete or collapse.