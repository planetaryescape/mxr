# Two cross-engine specs fail on WebKit on the Mac runner

**Status:** open · **Found:** 2026-10-22, CI run 39868854342 (main at `aa26dd25b`), job 120142913937

The `Web E2E (WebKit / macOS)` job finished for the first time after #662 cut it to `e2e:webkit:engine` (191 tests, 12.6 minutes). Two tests failed on both attempts; neither failed on Chromium in the same run.

## Archive's Coming up misses the Lisbon trip

`e2e/archive.spec.ts:73` expects `coming-up` to contain `/Lisbon, .* · in 3 days/`. The section rendered without it ("Coming up No Headspace charge since 13 Sep; expected around 13 Oct …"). The test ran at 23:19Z, 00:19 on 2026-10-23 in the runner's Europe/London zone, so a day boundary between the demo data's dates and the browser's clock is one candidate. Rerun it in the day (`npm run e2e:webkit:engine -- archive.spec.ts`) to tell a date edge from a WebKit difference.

## Updates' undo doesn't bring Strava back

`e2e/updates.spec.ts:157`: after Let go of all and `u`, `update-line` filtered to "Strava" is not found within 5 s. Repro the same way with `updates.spec.ts`.

## Main's Chromium suite was red in the same run

Both `Web E2E` shards failed on `e2e/wide-layout.spec.ts:65` (Now's header, card and rows share one column) at 390, 1440, 1920 and 2560 px, and `e2e/resizable-panes.spec.ts:356` (a reply drafted on a phone survives going back). Those come from main, not the runner.
