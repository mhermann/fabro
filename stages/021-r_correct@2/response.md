Review complete. Here is my assessment.

## Verdict: one real defect (stale doc row); the code and data are otherwise correct

### Finding 1 — Stale default-model row in docs (confirmed)

**File:** `docs/public/core-concepts/models.mdx`, line 245 (the "Default models" per-provider table)

```
| `zai` | `glm-5.2` |
```

The change moves the Z.ai provider default to `glm-5.3` (`zai.toml:14` sets `default = true` on `glm-5.3` and removes the flag from `glm-5.2` at line 61-64), and the same PR updated the model rows ~35 lines above in this file — but missed this table. Trigger: any reader of the "Default models" section is told the wrong zai default (`glm-5.2`) after this ships. It should read `| zai | glm-5.3 |`. This is the only remaining `glm-5.2`-as-default reference in the repo (verified via repo-wide grep).

### Everything else verified correct

**Catalog logic** — exactly one `default` (glm-5.3) and one `small_default` (glm-5.3-flash) on zai, so the `MultipleProviderDefaults`/`MultipleProviderSmallDefaults` guards aren't tripped. Global alias resolution (`glm`, `glm5`, `glm53`, `glm5.3`) resolves to zai's glm-5.3 because providers sort by descending priority (zai 60 > venice 35 > openrouter 25) and models sort by provider index; per-provider resolution works on both zai and openrouter. The portable-alias test and the fireworks table test cover these paths and pass.

**External data — verified against live sources** (task asked for researched pricing):

- **z.ai pricing page** (`docs.z.ai/guides/overview/pricing`): GLM-5.3 $1.4 in / $0.26 cached / $4.4 out; GLM-5.2 identical; GLM-5.3-Flash list $0.15/$0.03/$0.50. Exactly matches `zai.toml`.
- **Fireworks pricing page**: GLM 5.3 $1.40/$0.26/$4.40, GLM 5.3 Flash $0.15/$0.03/$0.50, with model slugs `glm-5p3`/`glm-5p3-flash`. Exactly matches `fireworks.toml`.
- **OpenRouter live API**: `z-ai/glm-5.3` and `z-ai/glm-5.3-flash` both exist; `z-ai/glm-5.3` is text-only (vision=false ✓) and flash is `text+image+video` (vision=true ✓); supported efforts `low/high/max` ✓; the Z.AI first-party endpoint on OpenRouter charges the z.ai list prices used in the TOML.
- **z.ai model docs**: glm-5.3 text-only, 1M context / 128K max output, reasoning always on with `low/high/max`; flash multimodal, same limits, model code `glm-5.3-flash`. All match the TOML limits/features/controls, including `max_output = 131072`.

**Reasoning default subtlety checked**: z.ai rejects `thinking.type: "disabled"` for GLM-5.3. The new rows omit `reasoning_by_default`, but `catalog.rs:2267-2269` defaults it to `supports_reasoning_effort()`, which is true for `"levels"` models — so requests default to reasoning-on. No risk of sending a disabled-thinking payload.

**Labeled observation (judgment, not a defect)**: OpenRouter's blended top-level price currently advertises $0.14/M cache-read for glm-5.3 and the half-price flash promo ($0.075/$0.25, mirroring z.ai's 50%-off campaign ending 2026-09-09), while the TOML encodes z.ai list prices. Since OpenRouter endpoints vary ($0.14–$0.26 cache on the glm-5.2 family alone) and the repo's OpenRouter rows already mix endpoint prices, list-price anchoring is defensible; just be aware OpenRouter flash costs are currently half the catalog value until Sept 9.

**Tests**: all 178 `fabro-model` tests pass (including the 4 new/updated glm tests), and all 1,444 `fabro-workflow` tests pass. During broader runs I hit 5 failures in `fabro-llm`/`fabro-cli`/`fabro-server` (tracing-capture, log-file bootstrap, slatedb/object-store tests) — every one passes in isolation, sits in code the diff never touches, and involves no catalog/glm state. They are pre-existing sandbox flakiness under parallel execution, not caused by this change.