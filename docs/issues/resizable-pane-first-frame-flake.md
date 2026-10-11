# The saved pane width still has an unexplained one-pixel mismatch

At candidate `010ae9518c5ee9bf96c194628483e10a0f0ac4ce`, CI run [38128915333](https://github.com/planetaryescape/mxr/actions/runs/38128915333/job/114439017800) saw the people pane at 452 pixels where the first-frame test expected 453. The unchanged test passed on retry. The log does not identify whether persistence, layout rounding or test sampling caused the difference.

The final S02 CI run at candidate `3d338bebfa9e6b714b6f9503ed5546c037d5eef1` passed all 381 browser tests without retries, including this test. The [curated run record](../build-log/daemon-portability/evidence/s02-ci-38130635937-3d338beb.json) confirms zero flaky tests. That clean run does not establish the cause of the earlier mismatch.

Reproduce the specific test and capture the persisted width, viewport and frame measurements before choosing a fix. Keep the assertion that reload does not visibly shift the pane; do not replace it with an arbitrary larger tolerance. This is outside S02 draft revision behavior and does not block S02 release acceptance.
