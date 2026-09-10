The plan above has been approved. Before any of it is built, decide one thing,
and think hard about it: **does this change need a refactoring first?**

This is a real decision with real cost on both sides, so do not answer it by
reflex. Refactoring first is expensive and delays the work. Not refactoring when
you should have is worse: the change gets bolted onto a shape that cannot hold
it, the diff becomes twice as large as the idea it expresses, and the next
person pays for it forever.

## Look before you decide

You have the shell. Go and read the code the plan touches — not the diff, the
actual files, and the files around them. You cannot judge this from the plan
alone; the plan describes what will be written, not the shape it will be written
into.

Look for:

- **The plan working around the code.** Steps that exist only because the
  current structure is in the way — a flag threaded through five call sites, a
  special case added beside three existing special cases, logic duplicated
  because it cannot be reached from where it is needed.
- **A missing seam.** The change wants to vary something the code has no way to
  vary. Adding it in place means the same conditional in several files.
- **Duplication about to become triplication.** The plan adds a third copy of
  something that already exists twice.
- **A unit that has outgrown itself.** A function or module the plan makes
  longer, that is already too long to hold in your head, and where the new code
  will be unreadable next to the old.
- **Tests that cannot reach the new behaviour.** The plan's verification step is
  vague or manual because the code is not testable in its current shape.
- **Names that will lie after the change.** An abstraction whose name and
  responsibility stop matching once this lands.

## The bar

Say yes only to refactoring that **this change actually needs** — work that
makes the planned implementation meaningfully simpler, smaller, or safer. Not
cleanup you would enjoy. Not modernisation. Not consistency for its own sake.

Concretely, say yes when at least one of these is true:

- The implementation is substantially harder, larger, or riskier without it.
- Without it, the change would introduce duplication or a special case that a
  reviewer should reject.
- Without it, the change cannot be tested at the level it should be tested.

Say no when:

- The code is merely not to your taste.
- The refactoring is worth doing but unrelated to this change. Name it in your
  answer so it is on record, then say no.
- The change is small and self-contained, and touching more would add risk.
- The refactoring is so large it should be its own piece of work with its own
  goal and its own human conversation. Say no, and say plainly that it needs to
  be raised separately.

When genuinely torn, say no. A refactoring that turns out to be unnecessary
costs a whole extra cycle; one that turns out to be necessary will be obvious
during implementation, and the review stages will catch the mess it leaves.

## If you have been here before

Check the context above. If a refactoring stage has already run in this workflow,
answer `none`. One focused refactoring per change is the limit — a second round
means the scope was wrong, and that is a conversation for the human, not another
refactoring.

## If the answer is yes

Write the brief to `/tmp/refactor-brief.md`. This file is the entire input to the
refactoring workflow — it will not see this conversation, so anything you leave
out is lost. Write it under these headings:

```
# Refactoring brief

## Why
The change that is about to be made, in two or three sentences, and the specific
way the current structure is in its way.

## Scope
Every file and unit to be changed, by path. Be exact.

## What good looks like
The shape the code should have when this is done, described concretely enough
that someone can tell whether they got there.

## Explicitly out of scope
What must not be touched. Name the neighbouring things a refactoring agent will
be tempted by, and rule them out.

## Behaviour that must not change
The observable behaviour this refactoring must preserve, and how to verify it
with this repository's existing test and build tooling. Name the actual
commands.
```

Do not modify any code in this stage. Writing `/tmp/refactor-brief.md` is the
only file you write.

## Your decision

State your reasoning first — what you looked at, what you found, and how it met
or missed the bar. Then end your response with exactly one of these:

If a focused refactoring is needed and you have written the brief:

{"preferred_next_label": "refactor"}

If no refactoring is needed:

{"preferred_next_label": "none"}

Emit exactly one of those two JSON objects, as the last thing in your response.
