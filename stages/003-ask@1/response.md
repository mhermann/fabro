This is the first ask — no plan has bounced yet, so these are the pre-code questions. Most forks I can resolve from the repo precedent and the "use precedent" note; only two are genuine product calls.

**Decided myself (from precedent + verified data, unless you object):**
- **Scope of "others":** zai + OpenRouter + Fireworks (all three verifiably serve GLM 5.3 and 5.3 Flash today). Bedrock untouched (only has GLM 4.7/5), Venice untouched (already has `glm-5.3`, offers no Flash — adding one would be fabrication).
- **Default:** `glm-5.3` becomes the zai provider default; `glm-5.2` drops the flag (only one default allowed per provider; this is exactly what the glm-4.7→glm-5.2 change did).
- **Aliases:** portable set `glm`, `glm5`, `glm53`, `glm5.3` moves to `glm-5.3` on zai/OpenRouter; `glm52`/`glm5.2` stay pinned to `glm-5.2`. Flash gets no aliases initially.
- **Limits & features:** 1,048,576 context / 131,072 max output for both (Z.ai's own docs; ignoring OpenRouter's 262,144 outlier), efforts `low`/`high`/`max` (Z.ai, Venice, and OpenRouter all agree), `vision = true` on Flash only (natively multimodal), reasoning-by-default implicit via `levels`.
- **Tests & docs:** mirror the GLM 5.2 precedent — catalog portability/snapshot tests, live `zai`/`openrouter` round-trip e2e tests, models.mdx tables, integration doc tables, dated changelog entry.

**Questions:**

1. **Should `glm-5.3-flash` be the `small_default` on zai?** Today zai has no small default, so metadata enrichment (small token budget, short timeout) falls back to the provider default — currently glm-5.2, soon glm-5.3. Options: (a) yes, mark Flash as zai's `small_default` (matches the deepseek-v4-flash / gpt-oss-20b precedent, cheaper enrichment); (b) no, leave zai without a small default (zero behavior change for existing Z.ai users).

2. **Which price do I record for `glm-5.3-flash`?** Z.ai's promo (50% off) ends 2026-09-09 — five days from now. Options: (a) current advertised rates per provider (Z.ai + OpenRouter: $0.075 / $0.25 / $0.015; Fireworks list: $0.15 / $0.50 / $0.03) with a comment noting the expiring promo — matches the repo's "record what the provider charges" convention; (b) list prices everywhere ($0.15 / $0.50 / $0.03) — durable past the promo but overestimates until it ends. (GLM 5.3 itself is unambiguous: $1.4 / $4.4 everywhere.)