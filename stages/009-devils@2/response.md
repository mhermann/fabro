Rev 2 addressed everything from round one: costs are now unambiguous explicit key/value pairs (I re-verified all six rows against the fetched Z.ai/OpenRouter/Fireworks sources — every value lands on the correct key), the full-workspace test run is in, and the `glm52`/`glm5.2` portability coverage is retained. The remaining checks I ran this round turned up only non-blocking items:

**1. `docs/public/docs.json` is missing from the file list (non-blocking).** Mintlify navigation lists changelog pages explicitly (`"changelog/2026-08-27"`, …), so the new `2026-09-04.mdx` needs a one-line nav entry or the page is orphaned from the site nav. This is an add-one-line-during-the-docs-step item that review trivially catches — not redo-worthy work, so it doesn't meet the bar. The implementer should add `"changelog/2026-09-04"` to the changelog nav array alongside creating the file.

**2. Legacy wire-ID coverage is optional, not automatic (non-blocking).** `builtin_legacy_vendor_ids_normalize_for_pinned_and_unpinned_selection` hardcodes only two selectors (`anthropic/claude-fable-5`, `openai/gpt-5.6-sol`) — it does not iterate `LEGACY_BUILTIN_MODEL_IDENTIFIERS`, so the plan's new `z-ai/glm-5.3` entries get no free test coverage. The GLM 5.2 precedent likewise added its legacy entry without a dedicated assertion, so the plan matches precedent; a one-line `("z-ai/glm-5.3", "glm-5.3")` case in the rewritten portability test would be a nice-to-have, not a requirement.

**3. `cargo insta` availability (non-blocking).** If the binary isn't installed in the sandbox, inline snapshots update via `INSTA_UPDATE=always cargo nextest run -p fabro-model` — ordinary implementer judgment; wrong snapshots fail loudly either way, so verification cannot pass on a wrong change.

Everything else held up under re-inspection: all planned TOML fields exist in the `deny_unknown_fields` schema; the catalog integrity tests are count-safe for additions and enforce the zai default flip; no CLI snapshot, docs-sync test, or cross-crate `glm` selector exists to break; the changelog filename is free; the fireworks rows correctly mirror the existing `glm-5.2` row shape (no effort controls); scope matches the request with exclusions (Venice Flash, Bedrock) justified by verified availability.

The plan is a sound basis for implementation.

{"preferred_next_label": "approve"}