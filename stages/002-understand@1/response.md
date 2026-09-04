I now have a complete picture. No code was written and no files were modified.

---

## 1. Repository structure and where the change lands

**Tooling:** Rust workspace (`lib/apps/*`, `lib/components/*`, `lib/foundation/*`), tested with `cargo nextest`; TypeScript web app (`apps/fabro-web`, Bun); Mintlify docs (`docs/public/**/*.mdx`). Formatting/lint via pinned nightly (`nightly-2026-04-14`). Insta snapshots used in some catalog tests.

**How models work here (the key insight):** models are *data*, not code. The catalog is a set of TOML files embedded into the binary via `rust_embed` at `lib/foundation/fabro-model/src/catalog/providers/*.toml`. Each file declares a provider (`display_name`, `adapter`, `base_url`, `auth.credentials`, `priority`) and its model rows (`api_id`, `family`, `aliases`, `default`/`small_default`/`probe`, `limits`, `features`, `controls.reasoning_effort`, `costs`). Z.ai (`zai.toml`) already exists as an `openai_compatible` adapter pointed at `https://api.z.ai/api/coding/paas/v4`, currently listing `glm-5.2` (provider default) and `glm-4.7`.

**A model entry appears in these places (all touched by precedent commit `c67cfef1` "Add GLM 5.2 model support" + follow-ups):**
- `lib/foundation/fabro-model/src/catalog/providers/zai.toml` — direct provider rows
- `.../providers/openrouter.toml` — gateway rows (`api_id = "z-ai/glm-5.2"`), plus `LEGACY_BUILTIN_MODEL_IDENTIFIERS` in `catalog.rs` (wire-ID → slug normalization, line ~1832)
- `.../providers/fireworks.toml` — serverless rows (`api_id = "accounts/fireworks/models/glm-5p2"`, dots → `p`)
- Tests in `lib/foundation/fabro-model/src/catalog.rs`: `builtin_glm_5_2_aliases_are_portable` (~line 3447), the `glm_5_2_in_catalog` insta snapshot (~7222), `builtin_deepseek_shared_slugs_are_portable_across_providers` (~4333)
- Live e2e tests: `lib/components/fabro-llm/tests/integration.rs` (`zai_glm_5_2_reasoning_tool_round_trip`, `openrouter_glm_5_2_reasoning_tool_round_trip`, gated on `ZAI_API_KEY`/`OPENROUTER_API_KEY`)
- Docs: `docs/public/core-concepts/models.mdx` (catalog table + per-provider default table), `docs/public/integrations/{openrouter,fireworks,venice}.mdx`, plus a dated `docs/public/changelog/*.mdx` entry
- No Rust enum/code change is needed for a new model on an existing adapter/codec — the `adapter_registry` is keyed by `AdapterKind`, and the old per-model test table there has since been removed.

**Constraint learned from the builder:** exactly one `default = true` and one `small_default = true` per provider, or the catalog build fails (`CatalogBuildError::MultipleProviderDefaults`).

## 2. Goal restated concretely

Add **GLM 5.3** and **GLM 5.3 Flash** as first-class catalog offerings for the `zai` provider and every other built-in provider that actually serves them (OpenRouter, Fireworks; Venice already has `glm-5.3`): new TOML rows with researched limits/features/controls/costs, portable `glm*` aliases moved to the new flagship, `glm-5.3` becoming the Z.ai provider default per precedent, matching test/doc/changelog updates, and optionally live round-trip tests against Z.ai and OpenRouter.

## 3. What I know for certain

**From the repo:**
- Z.ai ships `glm-5.2` (default, aliases `glm`, `glm5`, `glm52`, `glm5.2`, 1M/131072, efforts `high`/`max`, costs 1.4/4.4/0.26) and `glm-4.7`. Credentials: `ZAI_API_KEY` env/vault.
- "Portable aliases" is an explicit, tested contract: `glm`, `glm5`, `glm52`, `glm5.2` must resolve to the same canonical slug on every provider that offers the family.
- Exactly one default per provider; changing the default means moving the flag (precedent: glm-4.7→glm-5.2, documented in models.mdx "Default model" table).
- `small_default` marks the small/cheap tier per provider (used for metadata enrichment with small budgets); `DeepSeek V4 Flash` and fireworks `gpt-oss-20b` carry it. Z.ai currently has no `small_default`.
- Venice already has a `glm-5.3` row (1M/131072, efforts `low`/`high`/`max`, costs 1.75/5.5/0.325, aliases `glm`, `glm5`, `glm53`, `glm5.3`, `glm-5-3`) — so the unpinned `glm` alias will exist on multiple providers and resolves by provider priority (zai=60 > venice=35).
- OpenRouter responses carry authoritative in-band `usage.cost`, so its cost rows are best-effort estimates only.
- The OpenAI-compatible codec already handles Z.ai's `reasoning_content` and OpenRouter's `reasoning` spellings (added in the GLM 5.2 commit).

**Researched (authoritative sources, fetched today):**
- **Z.ai direct (docs.z.ai):** `glm-5.3` — text-only, 1M context, 128K max output, reasoning always on with `reasoning_effort` `low`/`high`/`max` (default `max`), `thinking.type` only accepts `enabled`; tools/streaming/caching/structured output supported; served on the same `api.z.ai/api/coding/paas/v4` chat-completions endpoint the catalog already uses. Pricing per MTok: **$1.4 input / $4.4 output / $0.26 cached**.
- **Z.ai direct `glm-5.3-flash`:** model code `glm-5.3-flash`, native multimodal (image/video/file+text input), 1M context, 128K max output, same reasoning contract as 5.3. Pricing per MTok: **$0.15 / $0.50 / $0.03 list**, currently 50% off (**$0.075 / $0.25 / $0.015**) until 2026-09-09; caching supported.
- **Z.ai coding plan routing:** requests for GLM-5.2/5.1 auto-route to GLM-5.3; GLM-4.7 auto-routes to GLM-5.3-Flash.
- **OpenRouter live API:** `z-ai/glm-5.3` — text-only, 1,310,720 advertised / 1,048,576 top-provider context, 262,144 max completion tokens, efforts `max`/`high`/`low`, $1.40/$4.40/$0.14 per MTok. `z-ai/glm-5.3-flash` — text+image+video, same context, 131,072 max completion, efforts `max`/`high`/`low`, **$0.075/$0.25/$0.015** (matches Z.ai promo).
- **Fireworks live pricing page:** `glm-5p3` → $1.40/$0.26/$4.40; `glm-5p3-flash` → $0.15/$0.03/$0.50 (standard tier). Both exist today.
- **Amazon Bedrock:** Z.ai models on Bedrock are only GLM 4.7, GLM 4.7 Flash, GLM 5 — **no GLM 5.3**, so `bedrock.toml` is out of scope.

## 4. Genuinely ambiguous (two reasonable engineers would differ)

1. **Scope of "others as well."** Fireworks provably serves both new models and OpenRouter is explicitly named; Venice already has `glm-5.3` (no Flash) and Bedrock has nothing. Does "others" mean *only* providers that serve GLM 5.3 today (zai+openrouter+fireworks), or should Venice get a `glm-5.3-flash` row too (Venice's catalog doesn't list one — adding it would be fabrication)?
2. **Which model becomes the Z.ai default.** GLM 5.3 is the same price as 5.2 and is the flagship (strong precedent: glm-4.7→glm-5.2 moved the default to the flagship). But DeepSeek made its *Flash* the default+small_default because of cost. Flagship (GLM 5.3) is the likelier read, but it's a product call.
3. **`small_default` for `glm-5.3-flash` on Z.ai.** It fits the "small/cheap tier" role and other providers mark their cheap tiers, but Z.ai has never had a `small_default`; adding one changes metadata-enrichment behavior for existing Z.ai users (budget/timeout-sensitive paths). Also: should fireworks' `glm-5p3-flash` get it (fireworks already has `gpt-oss-20b` as small default — so it *can't*, without displacing it)?
4. **Which price to record for `glm-5.3-flash`.** List ($0.15/$0.50/$0.03) vs. current promotional ($0.075/$0.25/$0.015, ends 2026-09-09 — days from now). The repo has precedent both ways (glm-5.2 rows use list prices; `laguna-xs-2.1` records a promotional rate with an explanatory comment; OpenRouter's live API currently returns the promo price).
5. **Limits disagreement between sources.** Z.ai docs say 1M context/128K output for both; OpenRouter advertises 1,310,720 context (1,048,576 top-provider) and 262,144 max completion for `glm-5.3`. Existing rows use 1,048,576/131,072. Whether to record OpenRouter's larger numbers or normalize to Z.ai's is a judgment call.
6. **Alias surgery on the older rows.** Presumably `glm`/`glm5` move to `glm-5.3` while `glm52`/`glm5.2` stay pinned to the old model — but does `glm-5.2` keep its full alias set, or get trimmed? And should the Z.ai auto-routing note (5.2→5.3, 4.7→5.3-Flash on the coding plan) be recorded in the TOML comments/docs, since users selecting `glm-5.2` will silently get 5.3 upstream?
7. **Effort levels on OpenRouter.** Z.ai and Venice expose `low`/`high`/`max` for 5.3; the existing OpenRouter `glm-5.2` row uses `high`/`xhigh` (OpenRouter's own naming). For `glm-5.3` OpenRouter reports `max`/`high`/`low` — whether to translate or mirror verbatim is a small consistency choice.
8. **`reasoning_by_default` spelling.** GLM 5.3/Flash cannot disable thinking. Catalog-wise, `reasoning_effort = "levels"` already implies reasoning-by-default, and Venice's glm-5.3 row also sets it explicitly. Whether the new rows set it explicitly (and whether the *existing* `glm-5.2` zai row is left inconsistent) is unresolved.
9. **Live e2e tests.** The GLM 5.2 precedent added two live round-trip tests (Z.ai + OpenRouter). Include the same for 5.3 (they only run under `--profile e2e` with real keys), and/or for Flash? A Flash test would also be the only coverage of vision content blocks on this route.
10. **Changelog file/date.** A new dated changelog entry (today 2026-09-04) is customary, but whether this ships standalone or folded into the next entry, and its title, is editorial.