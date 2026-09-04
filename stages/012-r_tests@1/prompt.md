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
- **implement**: succeeded
  - Model: glm-5.2
  - Files: /workspace/fabro/docs/public/changelog/2026-09-04.mdx, /workspace/fabro/docs/public/core-concepts/models.mdx, /workspace/fabro/docs/public/docs.json, /workspace/fabro/docs/public/integrations/fireworks.mdx, /workspace/fabro/docs/public/integrations/openrouter.mdx, /workspace/fabro/lib/components/fabro-llm/tests/integration.rs, /workspace/fabro/lib/foundation/fabro-model/src/catalog.rs, /workspace/fabro/lib/foundation/fabro-model/src/catalog/providers/fireworks.toml, /workspace/fabro/lib/foundation/fabro-model/src/catalog/providers/openrouter.toml, /workspace/fabro/lib/foundation/fabro-model/src/catalog/providers/zai.toml

## Context
- human.gate.interview.answer: 1c, z-ai-glm-5-3-flash
2a
- human.gate.interview.question: Answer the open questions
- human.gate.label: 1c, z-ai-glm-5-3-flash
2a
- human.gate.selected: freeform
- human.gate.text: 1c, z-ai-glm-5-3-flash
2a


**First, get the change in front of you.** Nothing has been handed to you, so
find it yourself:

    git --no-pager diff origin/main...HEAD

If `origin/main` does not resolve, use `main` instead. Read whole files with
`git show` or by opening them whenever the diff alone is not enough to judge —
a diff hides the context a line sits in, and most real defects live in that
context.

You are reviewing only. Do not edit, create, or delete any file, and do not
commit. Three other reviewers are working in this same checkout at the same
time, and a write from you would corrupt what they are reading.

Review the change for test coverage only.

Ask: does this change have tests, do they test behaviour rather than
implementation detail, and would they actually fail if the code were wrong?

Look for: new behaviour with no test, tests asserting what the code does rather
than what it should do, tests that pass regardless of the implementation,
missing failure and boundary cases, and tests coupled to internals in a way
that will break on any refactor.

Name the specific untested case. "Add more tests" is not a finding.

If coverage is adequate, say so plainly.
