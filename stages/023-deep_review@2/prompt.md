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
- parallel.results (14.5 KB; full value: `/tmp/fabro/runtime/blobs/d725f9a4dddb3fa94f16dbf4c03c1057f5cfafb229db378f33a61a4cbee4a751.json`)
  Preview: [{"id":"r_correct","index":0,"status":"succeeded","context_updates":{"last_response":"Review complete. Here is my assessment.\n\n## Verdict: one real defect (stale doc row); the code and data are otherwise correct\n\n### Finding 1 — Stale default-model row in docs (confirmed)\n\n**File:** ","last_st…


You are the last check before this change is opened as a pull request and
merged automatically. Treat that responsibility seriously.

Above you have a consolidated verdict from four reviewers who each looked at
this change from a different angle — correctness, security, tests, and
simplicity.

You have not been handed the change itself. Get it yourself:

    git --no-pager diff origin/main...HEAD

If `origin/main` does not resolve, use `main` instead.

Do not simply ratify their verdict. Your job is to review the change yourself,
using their findings as leads rather than as conclusions. Use the shell to read
any file you need in full — a diff hides the context a line sits in, and some
defects are only visible in the surrounding code.

Weigh in particular:

- Anything the four reviewers disagreed about, or that only one of them saw.
- Whether the change actually does what the goal asked for. The reviewers were
  each looking at a narrow dimension; nobody has yet checked the whole thing
  against the original intent.
- Whether the tests would fail if the code were wrong.
- Anything the reviewers could not have seen because it lives in code the diff
  does not touch.

Then choose one of three routes.

Route "fix" — there are defects, and you can describe each one precisely enough
that another agent can fix it without guessing. Say which file, what is wrong,
and what correct looks like. End with exactly:

{"preferred_next_label": "fix"}

Route "escalate" — a human should decide. Use this when the change may not match
what was actually wanted, when it touches security, data integrity, migrations,
deletion, or anything else where being wrong is expensive, or when you are
simply not confident. State plainly what you want the human to decide. End with
exactly:

{"preferred_next_label": "escalate"}

Route "approve" — the change does what the goal asked, you found nothing
blocking, and you are confident. End with exactly:

{"preferred_next_label": "approve"}

When torn between approve and escalate, escalate. A pause costs minutes; a
wrong merge costs much more.

Emit exactly one of those three JSON objects, as the last thing in your
response.
