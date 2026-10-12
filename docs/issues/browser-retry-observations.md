# Three browser tests passed only after retry

Status: observed, causes unverified. Updated 2026-10-12.

Capture a trace while repeating the three tests below before changing their assertions or product code. The remote-pairing PR exposed these retry-passing results in full CI; it did not change these tests or their product paths. GitHub marked the final workflow successful after its configured retries.

| Test | Observed failure | Evidence |
| --- | --- | --- |
| `apps/web/e2e/archive.spec.ts:308`, dark export preview | Axe reported serious `color-contrast` for `.hover:bg-primary/85` at line 314. | [Job 114982455433](https://github.com/planetaryescape/mxr/actions/runs/38300609819/job/114982455433), source `7cf3302e60b8ae78f5dafb68625e1e1ea471ef03`. |
| `apps/web/e2e/controls.spec.ts:114`, Reading header inventory | Expected `Let go of all`; the retrieved name list was empty at line 127. | The first job above and [job 114991700894](https://github.com/planetaryescape/mxr/actions/runs/38303428621/job/114991700894), source `a8b79cf11112d6b7349943fe137ea5e2d0e545d1`. |
| `apps/web/e2e/natural-time.spec.ts:39`, previewed snooze time | The response's `snoozed` list was empty when line 93 checked for the previewed timestamp. | Job 114991700894. |

The first run reported 383 passed, two flaky and one failed. The failed teaching test was a separate persistence synchronization problem, corrected in [PR 443](https://github.com/planetaryescape/mxr/pull/443): its reload now waits for the daemon's hint-seen state. The second run reported 384 passed and two flaky; all four production performance checks passed.

The retained error contexts do not establish whether the remaining observations come from product behavior, animation, asynchronous persistence or test setup. Record the actual DOM and request sequence in a focused reproduction. Preserve the behavior each assertion protects, then run the focused repeats and applicable CI once the correction is ready. Do not increase retries or add a fixed delay as a substitute for finding the cause.
