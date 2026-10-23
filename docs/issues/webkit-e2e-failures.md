# Two cross-engine specs fail on WebKit on the Mac runner

**Status:** open (Updates undo) · **Found:** 2026-10-22, CI run 39868854342 (main at `aa26dd25b`), job 120142913937

The `Web E2E (WebKit / macOS)` job finished for the first time after #662 cut it to `e2e:webkit:engine` (191 tests, 12.6 minutes). Two tests failed on both attempts; neither failed on Chromium in the same run.

## Archive's trip label is a clock edge, not a WebKit bug (fixed)

The trip showed as "Lisbon, October 2026 · in 2 days" (the CI page snapshot), not "in 3 days". The demo puts the flight exactly 72 hours after the daemon starts, and `coming_up::relative` counts calendar days in the daemon's zone. CI built the demo at 23:17Z on 22 Oct (00:17 BST, 23 Oct); London's clocks went back on the 25th, so the flight was 23:17 GMT on Sunday 25 Oct, two calendar days after Friday 23 Oct. The label was right and the test's "72 hours is 3 days" was not. `e2e/archive.spec.ts` now accepts 2 or 3 days and says when 2 is correct.

## Updates' undo doesn't bring Strava back

`e2e/updates.spec.ts:157`: after Let go of all and `u`, the digest comes back with Needs a look and Changed, but Strava sits in the folded Routine, not Changed. It passes locally on WebKit and Chromium, including with `TZ=UTC` just after midnight and with `TZ=America/Los_Angeles`; it failed only in CI at 23:19Z on 22 Oct (00:19 BST). Reproducing it needs that wall clock: either libfaketime for the daemon, or a run between 23:00Z and 00:00Z while British Summer Time is in force.

## Main's Chromium suite was red in the same run (fixed)

- `e2e/wide-layout.spec.ts:65` still expected the done check against the row's right edge. #598 (D148, blueprint 27 "Trail") keeps the time after the actions on the lit row, so the test now checks that the trail ends at the edge with the check before it.
- `e2e/resizable-panes.spec.ts:356`: on a phone, a reply left open when going back to the people docked as a 640px window over the list, so the row underneath couldn't be tapped. #658's filter line pushed the first row under it. On phones the docked draft is now its title bar alone (`ComposeHost`), and reopening the person brings it back inline.
