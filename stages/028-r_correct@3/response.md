Review complete. Here is my assessment.

## Verdict: one real defect (stale doc row); everything else is correct, including externally verified pricing

### Finding 1 — Stale default-model row in docs (confirmed, reproducible)

**File:** `docs/public/core-concepts/models.mdx`, line 245 (the "Default models" per-provider table)

```
| `zai` | `glm-5.2` |
```

The change moves the Z.ai default to `glm-5.3` — `zai.toml:14` sets `default = true` on `glm-5.3` and removes the flag from `glm-5.2` (lines 61–64), the new catalog test asserts `default_for_provider(zai).id == "glm-5.3"`, and the changelog added in this same PR states "GLM 5.3 becomes the Z.ai default model". The model-catalog table ~180 lines above in the same file was updated, but this table was missed. Trigger: any reader of the "Default models" docs section is told the wrong Z.ai default once this ships. It should read `| zai | glm-5.3 |`. This is the only remaining `glm-5.2`-as-default reference in the repo (repo-wide grep).

### Everything else verified correct

**External data — independently verified against live sources** (I re-fetched these myself, not just trusting the earlier review):

- **z.ai pricing page** (`docs.z.ai/guides/overview/pricing`): GLM-5.3 $1.4 in / $0.26 cached / $4.4 out; GLM-5.3-Flash list $0.15 / $0.03 / $0.50 — exactly matches `zai.toml`. The flash 50% promo ends 2026-09-09; correctly *not* encoded (list prices used).
- **OpenRouter**: `z-ai/glm-5.3` and `z-ai/glm-5.3-flash` exist; the Z.AI first-party endpoint charges $1.40/$4.40/$0.26 and $0.15/$0.50/$0.03 — matches `openrouter.toml`; the Fireworks endpoint rows match `fireworks.toml` exactly too.
- **z.ai model docs**: GLM-5.3 is text-only (vision=false ✓), 1M context / 128K max output → 1048576/131072 ✓; GLM-5.3-Flash is natively multimodal (video/image/text/file in, vision=true ✓), same limits ✓; model codes `glm-5.3` / `glm-5.3-flash` match the API IDs; Fireworks `glm-5p3(-flash)` follows its dot→p convention.
- **Effort controls**: z.ai documents `low`/`high`/`max` (default `max`), reasoning always on — matches `controls.reasoning_effort = ["low","high","max"]` on all three providers; `ReasoningEffort::Max` serializes as `"max"` on the wire.

**Subtleties checked and fine:**

- `reasoning_by_default` is omitted on the new rows, but `catalog.rs:2267-2269` defaults it to `supports_reasoning_effort()` (true for `"levels"` models), so requests never send a thinking-disabled payload that z.ai would reject.
- Exactly one `default` (glm-5.3) and one `small_default` (glm-5.3-flash) on zai — the multiple-defaults guards aren't tripped; all 178 `fabro-model` tests pass.
- Alias semantics: `glm`/`glm5` move to 5.3 on zai and OpenRouter while `glm52`/`glm5.2` still resolve to 5.2; no within-provider alias collisions; unqualified `glm` resolves to zai (priority 60 > venice 35 > fireworks 30 > openrouter 25), which is the intended new default.
- `LEGACY_BUILTIN_MODEL_IDENTIFIERS` correctly gains no `z-ai/glm-5.3` entry — it's for historical pre-separation keys only (I checked the commit history to confirm).
- The GLM 5.2-era `fabro-llm` codec changes (OpenRouter `reasoning` vs `reasoning_content` spelling) were model-agnostic, so no codec change is needed for 5.3; the `adapter_registry` coverage test enumerates offerings generically.
- New live tests are exact clones of the glm-5.2 precedent (19 × 23 = 437 is right), gated on `ZAI_API_KEY`/`OPENROUTER_API_KEY` per house pattern; `cargo test -p fabro-llm --no-run` compiles clean.
- `docs.json` September group is inserted newest-first, matching convention.

**Judgment call, labeled as such (not a defect):** OpenRouter's blended advertised glm-5.3 price is currently lower than the catalog's ($1.15/$3.50 headline; flash half-price promo), but the TOML encodes z.ai list prices, matching both the first-party Z.AI endpoint on OpenRouter and the repo's existing convention for gateway rows.