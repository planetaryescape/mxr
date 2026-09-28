# Experience rubric: what is still at 2

**Status:** open · **Found:** 2026-09-28 on `feat/delight` (the rung 6 re-score)

The re-score in `docs/web-app-experience-rubric.md` ("Scores") meets the
pass bar: every criterion is at 2 or better, and section A and B1 to B3 are
at 3. These are the criteria at 2, with what would take each to 3.

- **B4, earned delight:** a person should hear the sound palette on real
  speakers (volume curve, the low tide figure) before it's called best in
  class.
- **B6, the app teaches its keys:** count sidebar and command-palette clicks
  toward key hints, not only toolbar, row and bulk buttons.
- **B7, typographic craft:** audit relative dates screen by screen (the
  list, the reader, the desk and places should read the same way for the
  same age).
- **B8, honest system states:** a journey that stops the daemon mid-session
  and checks input still works on cached data.
- **B9, pointer and touch:** a manual pass on a real phone for the swipe
  thresholds and vertical scrolling.
- **C1 to C3:** C2 needs draft assist's sources checked in a journey; C1 and
  C3 need a copy review by someone other than the author.

Also noticed:

- `FocusRoute.tsx` keeps its own guards against repeated `s` and Tab. The
  dispatcher now drops repeats for every action but movement, so the `s`
  guard is redundant; Tab is handled outside the dispatcher and still needs
  its guard.
