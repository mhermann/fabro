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

## Context
- human.gate.interview.answer: 1. yes
- human.gate.interview.question: Answer the open questions
- human.gate.label: 1. yes
- human.gate.selected: freeform
- human.gate.text: 1. yes


The human has just answered your questions. Decide whether you can now build
the right thing, or whether their answers opened up something new that is worth
one more short round.

Judge only this: is there still an ambiguity where two reasonable engineers
would build materially different software from what you now know?

Go back for another round only when all of these hold:

- The remaining ambiguity would change what you build, not merely how you
  phrase it.
- It came out of their answers, rather than being something you failed to ask
  the first time and could reasonably decide yourself.
- You can state it as one or two concrete questions.

Otherwise proceed. A second round costs the human real time, and most answers
are good enough to build from even when they leave small gaps. Prefer deciding
yourself and saying what you decided.

If you need another round, briefly say what is still unresolved and end your
response with exactly:

{"preferred_next_label": "more"}

If you have enough to plan, briefly restate what you now understand the human
to want, including any decision you are making on their behalf, and end your
response with exactly:

{"preferred_next_label": "enough"}

Emit exactly one of those two JSON objects, as the last thing in your response.
