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
- **fork**: succeeded
- **synth**: succeeded
- **deep_review**: succeeded
  - Model: glm-5.2
- **implement**: succeeded
  - Model: glm-5.2
- **fork**: succeeded
- **synth**: succeeded
- **deep_review**: succeeded
  - Model: glm-5.2
- **implement**: succeeded
  - Model: glm-5.2
- **fork**: succeeded

## Context
- human.gate.interview.answer: 1c, z-ai-glm-5-3-flash
2a
- human.gate.interview.question: Answer the open questions
- human.gate.label: 1c, z-ai-glm-5-3-flash
2a
- human.gate.selected: freeform
- human.gate.text: 1c, z-ai-glm-5-3-flash
2a
- parallel.branch_count: 4
- parallel.results (15.7 KB; full value: `/tmp/fabro/runtime/blobs/a885a1c83700af081dcc93fd7665ab60cf52c5e60a2c64870d62b2f0c898440e.json`)
  Preview: [{"id":"r_correct","index":0,"status":"succeeded","context_updates":{"last_response":"Review complete. Here is my assessment.\n\n## Verdict: one real defect (stale doc row); everything else is correct, including externally verified pricing\n\n### Finding 1 — Stale default-model row in do","last_stag…


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
