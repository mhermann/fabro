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
- **synth**: succeeded
- **deep_review**: succeeded
  - Model: glm-5.2

## Context
- human.gate.interview.answer: 1. yes
- human.gate.interview.question: Answer the open questions
- human.gate.label: 1. yes
- human.gate.selected: freeform
- human.gate.text: 1. yes
- parallel.branch_count: 4
- parallel.results (13.5 KB; full value: `/tmp/fabro/runtime/blobs/a66d26dded6f5a491c9b1b94ca4ddd35a887495cc4cacd9cfeb829bf197c5827.json`)
  Preview: [{"id":"r_correct","index":0,"status":"succeeded","context_updates":{"last_response":"Review complete. I read the full diff, the surrounding catalog code (`resolve_on_provider`/`get_on_provider`/`normalize_legacy_builtin_selector`, `default_for_provider`, `small_default_for_provider`, ","last_stage"…


Write the pull request description for the change you just made.

Write exactly two files:

1. `/tmp/pr-title.txt` — a single line. Imperative mood, no trailing period, no
   more than 70 characters. Describe what the change does, not that it is a
   change.
2. `/tmp/pr-body.md` — the description. Cover what changed and why, anything a
   reviewer should look at closely, and how it was verified. If review findings
   were fixed along the way, say what they were.

Do not write these files anywhere inside the repository — only under `/tmp`.

Do not use the shell to create the PR. A later stage does that.
