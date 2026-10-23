# Two cross-engine specs fail on WebKit on the Mac runner

**Status:** resolved · **Found:** 2026-10-22, CI run 39868854342 (main at `aa26dd25b`), job 120142913937

The `Web E2E (WebKit / macOS)` job finished for the first time after #662 cut it to `e2e:webkit:engine` (191 tests, 12.6 minutes). Two tests failed on both attempts; neither failed on Chromium in the same run.

## Archive's trip label is a clock edge, not a WebKit bug (fixed)

The trip showed as "Lisbon, October 2026 · in 2 days" (the CI page snapshot), not "in 3 days". The demo puts the flight exactly 72 hours after the daemon starts, and `coming_up::relative` counts calendar days in the daemon's zone. CI built the demo at 23:17Z on 22 Oct (00:17 BST, 23 Oct); London's clocks went back on the 25th, so the flight was 23:17 GMT on Sunday 25 Oct, two calendar days after Friday 23 Oct. The label was right and the test's "72 hours is 3 days" was not. `e2e/archive.spec.ts` now accepts 2 or 3 days and says when 2 is correct.

## Updates' undo raced the restore (fixed)

`e2e/updates.spec.ts:157` waited only for the digest to hold any message after `u`. Undo puts the messages back over about two seconds (16, then all 62, in instrumented runs), and the page re-reads the digest when the undo answers. Replaying CI's clock with libfaketime (`faketime -f "@2026-10-23 00:17:00"`) failed 3 of 5 runs on WebKit and 1 of 2 on Chromium; at 01:17 BST it passed. The test now waits until the digest is back to its count before the let-go, then checks the page: 3 of 3 at CI's clock. What ties the race to the hour after midnight is not established.

## Main's Chromium suite was red in the same run (fixed)

- `e2e/wide-layout.spec.ts:65` still expected the done check against the row's right edge. #598 (D148, blueprint 27 "Trail") keeps the time after the actions on the lit row, so the test now checks that the trail ends at the edge with the check before it.
- `e2e/resizable-panes.spec.ts:356`: on a phone, a reply left open when going back to the people docked as a 640px window over the list, so the row underneath couldn't be tapped. #658's filter line pushed the first row under it. On phones the docked draft is now its title bar alone (`ComposeHost`), and reopening the person brings it back inline.
