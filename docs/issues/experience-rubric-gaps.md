# Experience rubric: criteria still below the pass bar

**Status:** open · **Found:** 2026-09-28 on `feat/delight` (the rung 6 re-score)

The re-score in `docs/web-app-experience-rubric.md` (section "Scores") puts
every criterion at 2 or better, but the bar asks for 3 on all of section A
and on B1 to B3. These are the ones short of it, each with the change that
would close it.

## A8. Promises made to you have no journey

Sending a dated promise is covered end to end (`focus.spec`). Promises other
people made to you show in the reader's context block, but only
`contextFormat.test.ts` checks it. Fix: a `reader-context.spec` case that
stubs a `theirs` commitment and asserts the line in the context block.

## A9. No inventory of persistent controls

The reply toolbar is gone and the sidebar holds places, but nothing lists
each persistent control in the reader and the list with its reason, so the
check in the rubric can't be run. Fix: a short table in
`docs/web-app-experience-rubric.md` (or the component headers) naming every
always-visible control and why it earns its place, then remove any that
can't justify itself.

## A10. Age and cadence text isn't asserted in a journey

Desk rows show "22h · usually 47m", tested only in `deskCopy.test.ts`. Fix:
assert a Waiting on row's age and "usually" text in `desk.spec`.

## B1. No skeleton timing policy

Keydown to paint is about 16 ms at p95 (budget 50), but the rule "no spinner
or skeleton for local data under 300 ms; once shown, at least 400 ms" isn't
implemented anywhere. Fix: one `useDelayedSkeleton(pending)` hook used by
`ListSkeleton` and `ReaderSkeleton`, with a vitest for both thresholds.

## B3. No verb table

Undo works for every verb the daemon can undo, but there is no single table
in the code mapping each verb to its trigger, rule, feedback and undo
(Saffer's four parts). Fix: turn `mailUndo.ts`'s announcements and
`actionPastTense.ts` into one exported table and test that every
`MailAction` has a row.

## Also worth doing (criteria already at 2)

- B6: count sidebar and command-palette clicks toward key hints.
- B7: audit relative dates screen by screen (the list, the reader, the desk
  and places should read the same way for the same age).
- B4 and B9: a person should hear the sound palette on real speakers and
  swipe on a real phone before either is called best in class.
