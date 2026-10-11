# The saved pane width still has an unexplained one-pixel mismatch

At candidate `010ae9518c5ee9bf96c194628483e10a0f0ac4ce`, CI run [38128915333](https://github.com/planetaryescape/mxr/actions/runs/38128915333/job/114439017800) saw the people pane at 452 pixels where the first-frame test expected 453. The unchanged test passed on retry. The log does not identify whether persistence, layout rounding or test sampling caused the difference.

The final S02 CI run at candidate `3d338bebfa9e6b714b6f9503ed5546c037d5eef1` passed all 381 browser tests without retries, including this test. The [curated run record](../build-log/daemon-portability/evidence/s02-ci-38130635937-3d338beb.json) confirms zero flaky tests. That clean run does not establish the cause of the earlier mismatch.

The S03 baseline at `3b5648422e1d284cf369e54c1dfae99052ac93da` saw the same
expected-453/received-452 mismatch at [the unchanged first-frame assertion](https://github.com/planetaryescape/mxr/blob/3b5648422e1d284cf369e54c1dfae99052ac93da/apps/web/e2e/resizable-panes.spec.ts#L199)
in [CI run 38138146795, Web E2E job 114466290553](https://github.com/planetaryescape/mxr/actions/runs/38138146795/job/114466290553).
The retry passed; the job reported 380 passed and one flaky test, plus four
passing speed checks. The S03 candidate run at `7413df8716410b67b75c6051c920aa6e2060551a`
also passed on retry: [CI run 38140757563, Web E2E job 114474067681](https://github.com/planetaryescape/mxr/actions/runs/38140757563/job/114474067681)
reported 380 passed and the same one-pixel mismatch at the unchanged assertion.
The run record gives a 14m06s Web E2E job and 14m45s total CI elapsed time.
Neither log establishes the cause.

The later S03 candidate `7e21504eb4a663543be751efb2ba26c8ba179de4` repeated
the mismatch at the same assertion in [CI run 38143112143, Web E2E job
114480882722](https://github.com/planetaryescape/mxr/actions/runs/38143112143/job/114480882722).
The job reported 380 passed and one flaky test that passed on retry; four
speed checks also passed in 18 seconds. The cause remains unknown. This is
historical evidence for candidate 7e, not a verdict on later code.

The reviewed S03 candidate `04a72f7bef8dcbebb3aa4838b15c855247566348`, merged
as `d6ba93ba7082a644e027b331e67da44d8e6dd4e8`, repeated the unchanged
expected-453/received-452 observation and passed it on retry in
[CI run 38146965743](https://github.com/planetaryescape/mxr/actions/runs/38146965743).
The browser job reported 380 passing tests; four speed checks passed in 23.8
seconds. The log still does not identify the cause.

Reproduce the specific test and capture the persisted width, viewport and frame measurements before choosing a fix. Keep the assertion that reload does not visibly shift the pane; do not replace it with an arbitrary larger tolerance. This is outside S02 draft revision behavior and does not block S02 release acceptance.
