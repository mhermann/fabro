Before anyone asks the human anything, work out — carefully, and at length — what
actually needs asking. This stage exists because the most expensive failure in
this workflow is building the wrong thing correctly, and that failure is almost
always traceable to a question nobody thought to ask.

Take your time here. Read more of the repository if you need to. The cost of
this stage is minutes; the cost of missing a question is the whole run.

## Sweep every dimension

Go through each of these deliberately, one at a time, and write down what you
find. Do not skip a dimension because it "obviously doesn't apply" — say why it
doesn't apply, in one line, and move on. The skipped dimension is where the
missed question lives.

1. **Scope boundaries.** What is clearly in? What is clearly out? Where is the
   line genuinely unclear — and would the two readings produce materially
   different software?
2. **Users and callers.** Who or what invokes this? Are there existing callers
   whose behaviour must not change? Is anything public API, a wire format, a CLI
   surface, or a config file someone else depends on?
3. **Data and state.** Does this touch persisted data? Is a migration implied?
   What happens to data written by the previous version? Is a rollback possible?
4. **Existing behaviour.** What currently happens in this area, and is the goal
   asking to change it, extend it, or leave it intact? Look — do not assume.
5. **Edge and failure cases.** Empty, missing, malformed, concurrent, partial,
   very large. Which of these does the goal not say anything about, and where
   would guessing wrong be expensive?
6. **Non-functional constraints.** Performance, memory, latency, concurrency
   limits, cost. Does this repository have stated expectations you would violate?
7. **Security, privacy, permissions.** Does this widen who can do what, or where
   data flows? Anything touching auth, secrets, or personal data.
8. **Compatibility and versioning.** Backwards compatibility, feature flags,
   staged rollout, deprecation of the thing being replaced.
9. **Testing and verification.** How will anyone know this works? Does the
   repository's existing tooling actually cover this area, or is a new kind of
   test implied?
10. **Operational surface.** Logging, metrics, error reporting, documentation,
    changelog — does this repository's convention require them for a change
    like this?
11. **Dependencies and integration.** New dependencies, external services,
    environment variables, credentials, or infrastructure the change assumes.
12. **Definition of done.** What would make the human say "that is not what I
    asked for" even though every test passes?

## Then decide what is actually a question

Most of what the sweep turns up is not a question for the human. Sort every
finding into exactly one of three buckets, and say which bucket it went in:

- **Answerable from the repository.** Go and answer it. Read the code, the
  tests, the docs, the git history. Write down the answer. This is the largest
  bucket and it should be.
- **Yours to decide.** The goal does not say, but any reasonable choice is fine
  and cheap to change later. Decide it now, state the decision and the reason.
  Do not spend the human's attention on it.
- **Genuinely theirs.** Two reasonable engineers would build materially
  different software depending on the answer, and being wrong is expensive to
  undo. Only these become questions.

## Now stress-test the question set

Before you hand it over, run these checks and say what each one changed:

- **The regret check.** Imagine the change is built, reviewed, and rejected by
  the human as wrong. What was the question you failed to ask? Add it.
- **The second-round check.** What would make you come back and ask a second
  time? If you can name it, ask it now — a second round costs the human real
  time.
- **The merge check.** Can any two questions be answered by one better question?
  Merge them.
- **The self-answer check.** For each remaining question, try to answer it
  yourself from the repository. If you can, it is not a question. Remove it.
- **The consequence check.** For each remaining question, state what you would
  build differently under each answer. If you cannot state a difference, it is
  not worth asking. Remove it.

## Output

End with a section titled `## Vetted questions` containing at most five
questions, ordered by how much they change what gets built. For each one give:

- The question, phrased as a concrete either/or with real options where possible.
- The two or three options, and what each one costs.
- One line on what you would build differently depending on the answer.

Then a section titled `## Decided without asking` listing everything you settled
yourself, with the decision and a one-line reason.

If the sweep genuinely produced no question worth the human's time, say so
plainly and leave `## Vetted questions` empty. That is a legitimate outcome, but
only after you have done the sweep — not instead of it.

Do not call any question tool in this stage, and do not write code or modify
files. The next stage puts these in front of the human.
