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
- parallel.results (14.5 KB; full value: `/tmp/fabro/runtime/blobs/334c8fc5fdf0a3f75a6e1a6c519a12edadfb3fb7dd7e2b8da2eb9542b5553079.json`)
  Preview: [{"id":"r_correct","index":0,"status":"succeeded","context_updates":{"last_response":"Review complete. I examined the full diff (catalog TOMLs, catalog tests, integration tests, docs), the catalog build/resolution machinery, and ran the relevant test suites.\n\n## Verdict: correct — no","last_stage"…


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

Review the change for correctness only.

Look for logic that produces a wrong result: off-by-one errors, inverted
conditions, unhandled nil or empty cases, incorrect operator precedence, wrong
variable used, race conditions, resource leaks.

For each finding give the file, the line if you can identify it, what is wrong,
and the concrete input or state that triggers it. A finding you cannot make
fail is a guess — label it as such or drop it.

If the diff is correct, say so plainly. Do not invent findings to seem useful.
