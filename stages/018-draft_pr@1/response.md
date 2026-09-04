Both files are written. Summary:

**`/tmp/pr-title.txt`** (64 chars, imperative, no trailing period):
> Add GLM 5.3 and GLM 5.3 Flash across Z.ai, OpenRouter, Fireworks

**`/tmp/pr-body.md`** covers:
- **What/why**: catalog rows for GLM 5.3 (new Z.ai default, portable `glm`/`glm5` aliases) and GLM 5.3 Flash (small-default, first vision-capable Z.ai model) across zai/openrouter/fireworks TOMLs, legacy wire-ID entries, snapshot/table test updates, two live e2e tests, and all five docs artifacts — with the verified pricing table (promo vs. list rates) and per-provider cache-read difference.
- **Reviewer focus**: the silent `glm`/`glm5` alias move (intentional per default-bump precedent, cost-neutral), the deliberate `max_output = 131072` choice over OpenRouter's outlier, the 2026-09-09 promo expiry follow-up, the accepted OpenRouter offline-snapshot coverage gap, and the deferred e2e-helper refactor.
- **Verification**: `cargo test -p fabro-model` → 177 passed / 0 failed (re-run just now), e2e tests compile + live-gated, `docs.json` parses, all six pricing rows independently re-verified against Z.ai/OpenRouter/Fireworks sources during deep review.
- **Review findings**: four-way parallel review returned no blocking findings; no fixes were needed post-review, with non-blocking notes surfaced for reviewers instead.