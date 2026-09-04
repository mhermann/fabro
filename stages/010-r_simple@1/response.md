Review complete. All the changed tests pass, and I've compared the change against repo precedent.

## What the change is

Zero production Rust changes — pure catalog data (`zai.toml`, `openrouter.toml`, `fireworks.toml`), docs (changelog, models table, two integration pages, `docs.json`), catalog unit tests, and two live e2e tests. That's the right shape for this goal: the catalog system exists so model additions need no code.

## Finding 1 (the only real one): the two new live tests are verbatim copies of the tests directly above them

`lib/components/fabro-llm/tests/integration.rs` — I diffed the bodies mechanically:

- `zai_glm_5_3_reasoning_tool_round_trip` (new, lines 557–631) is **byte-identical** to `zai_glm_5_2_reasoning_tool_round_trip` (lines 393–467) except `5.2` → `5.3`.
- `openrouter_glm_5_3_reasoning_tool_round_trip` (new, lines 633–719) is byte-identical to `openrouter_glm_5_2_reasoning_tool_round_trip` (lines 469–554) except the version strings.

That's 164 added lines duplicating 164 lines in the same file, bringing it to **four near-identical ~75-line copies** of the same multiply-tool round trip. The cost is concrete: the next GLM bump copy-pastes two more; any change to the flow (e.g. the reasoning-content assertion) must be applied four times; and a reader comparing a zai failure to an openrouter failure has to diff two 75-line blocks to find the real difference.

The same file already establishes the collapse pattern twice — `assert_deep_tool_round_trip(catalog, provider, model_id, credential)` and `run_multi_turn_cache_test(...)` are shared helpers with thin `#[e2e_test]` wrappers (poolside, deepseek, fireworks-kimi, openrouter-kimi, modal-kimi all use the first one).

**Simpler version** — either:

1. Extract the round trip into a helper, e.g. `async fn glm_reasoning_tool_round_trip(adapter: &OpenAiCompatibleAdapter, model: &str, label: &str)`, with the four `#[e2e_test]` wrappers reduced to adapter construction + one call; keep the `CostSource::Authoritative` asserts only in the OpenRouter wrappers. ~150 of the 164 lines disappear.
2. Or, since the 5.2 tests exist to validate adapter wire behavior for the *current* default model, repoint the existing 5.2 tests at 5.3 in place — 5.2 keeps its catalog unit coverage (`glm_5_2_in_catalog`), and no new file lines are added at all. This also matches the one-version-per-family precedent elsewhere in the file (`kimi_k3_reasoning_tool_round_trip` is the only Kimi round trip; `openai_gpt_5_5_complete` replaced rather than accumulated).

Option 1 is better if keeping live 5.2 coverage is deliberate; option 2 if it isn't.

## Things I checked and consider fine (not findings)

- **Catalog unit tests** (`glm_5_3_in_catalog`, `glm_5_3_flash_in_catalog`, `builtin_openrouter_includes_glm_5_3_when_enabled`, renamed alias-portability test): each mirrors the shape of its 5.2 counterpart exactly, including the snapshot + `model_settings` double-assert. Precedent, not new complexity.
- **TOML repetition across providers**: inherent to the declarative catalog format — every model spells out its limits/features/costs per provider.
- **`small_default = true` on glm-5.3-flash**: real mechanism (one small default per provider; `small_default_for_provider` / `small_default_for_configured_ids` consume it), and zai was previously the one provider without one. Not unused config.
- **Docs**: changelog + new month group in `docs.json` + table rows all follow existing structure; nothing extraneous.
- **Scope**: no refactors, no stray production changes; Bedrock's legacy `glm-5` and Venice's pre-existing `glm-5.3` correctly left alone.

Everything except the integration-test duplication is appropriately simple.