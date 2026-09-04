Goal: Add z.ai GLM5.3 and GLM5.3 Flash support. Understand the provider model for the models

Notes:
- Use precedent for changing default model
- Implement in openrouter and others as well
- Research the pricing yourself

## Completed stages
- **understand**: succeeded
  - Model: glm-5.2
- **ask**: succeeded
  - Model: glm-5.2
- **interview**: succeeded
- **triage**: succeeded
  - Model: glm-5.2
- **plan**: succeeded
  - Model: glm-5.2
- **devils**: succeeded
  - Model: glm-5.2
- **plan**: succeeded
  - Model: glm-5.2
- **devils**: succeeded
  - Model: glm-5.2
- **implement**: succeeded
  - Model: glm-5.2
  - Files: /workspace/fabro/docs/public/changelog/2026-09-04.mdx, /workspace/fabro/docs/public/core-concepts/models.mdx, /workspace/fabro/docs/public/docs.json, /workspace/fabro/docs/public/integrations/fireworks.mdx, /workspace/fabro/docs/public/integrations/openrouter.mdx, /workspace/fabro/lib/components/fabro-llm/tests/integration.rs, /workspace/fabro/lib/foundation/fabro-model/src/catalog.rs, /workspace/fabro/lib/foundation/fabro-model/src/catalog/providers/fireworks.toml, /workspace/fabro/lib/foundation/fabro-model/src/catalog/providers/openrouter.toml, /workspace/fabro/lib/foundation/fabro-model/src/catalog/providers/zai.toml
- **fork**: succeeded

## Context
- human.gate.interview.answer: 1. yes
- human.gate.interview.question: Answer the open questions
- human.gate.label: 1. yes
- human.gate.selected: freeform
- human.gate.text: 1. yes
- parallel.branch_count: 4
- parallel.results (13.5 KB; full value: `/tmp/fabro/runtime/blobs/a66d26dded6f5a491c9b1b94ca4ddd35a887495cc4cacd9cfeb829bf197c5827.json`)
  Preview: [{"id":"r_correct","index":0,"status":"succeeded","context_updates":{"last_response":"Review complete. I read the full diff, the surrounding catalog code (`resolve_on_provider`/`get_on_provider`/`normalize_legacy_builtin_selector`, `default_for_provider`, `small_default_for_provider`, ","last_stage"…


Four reviewers looked at this change from different angles — correctness,
security, tests, and simplicity. Their findings are in the branch results above.

Produce one consolidated report for the deep reviewer who comes next:

1. Merge findings that are the same problem seen from two angles.
2. Discard findings that are plainly wrong, speculative, or mere preference.
3. Order what remains by how much it actually matters.
4. Note explicitly where the reviewers disagreed, or where only one of them saw
   something. Those are the places a second pair of eyes is most valuable, so
   flag them rather than quietly resolving them yourself.

For each surviving finding give the file, what is wrong, and why it matters.

Do not decide whether the change should ship — that is the next stage's job.
Your output is evidence for that decision, not the decision itself. If the
reviewers found nothing worth passing on, say so plainly rather than padding.
