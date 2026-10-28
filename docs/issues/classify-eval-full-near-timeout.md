# The full-set classifier eval runs up against its 30-minute limit

Status: fixed 2026-10-28 (fix/ci-health). Found 2026-10-28 while landing #735 (keyboard), which doesn't touch the classifier.

## What happens

The CI job "Classifier Eval (full set)" has `timeout-minutes: 30` (`.github/workflows/ci.yml`, job `classifier-eval`). On #735 it was cancelled at the limit once, then passed on the next run with about 80 seconds to spare. On main and other PRs the same day it took 11 to 18 minutes.

| Run | Compile | Account 190b5fb4 | Account 66564acb | Job |
|---|---|---|---|---|
| main, 2026-10-28 10:07 UTC | 2m 43s | 205 s | 177 s | 12 min, passed |
| #735, run 40669165538 | 6m 46s | 600 s | 506 s | cancelled at 30 min |
| #735, run 40673711822 | 6m 02s | 518 s | 518 s | 28.7 min, passed (test 1316 s) |

## Evidence

- Job logs: run 40669165538 job 122751381507 (cancelled) and run 40673711822 job 122766779101 (passed). Each account's sync and classify times are printed every 5 pages.
- The slow runs were slower at everything by about the same factor: compile (2.2 to 2.5 times), sync (about 2.5 times, `5 pages ... sync 7.2s` against 2.9s) and classify. #735 changed nothing in sync, classification or the demo (`crates/store/src/mode_views.rs` only changed `set_hint_seen`'s return value). That points at the runner (`blacksmith-4vcpu-ubuntu-2404`), not the code.

## Why it matters

A runner that is 2.5 times slower turns a 12-minute job into a 30-minute one, so the gate fails on infrastructure, and a real slowdown in the classifier would look the same as a slow runner.

## Options

- Raise `timeout-minutes` with a reason in the workflow comment. Cheap, but hides real slowdowns.
- Print the per-account wall time as a job summary and fail on a ratio against the compile time, so a slow runner and a slow classifier can be told apart.
- Run the eval on a larger runner, or split the two demo accounts into parallel jobs.

## Cause

The job's only speed check was its wall-clock limit, and wall time measures the runner as much as the code. On #735's slow runner compile took 2.2 to 2.5 times as long, and the per-page sync and classify times about 3 times as long (personal account at 25 pages: sync 49.5 s against 16.2 s, classify 244 s against 88 s). The ratio between them barely moved: classify took 4.9 to 5.6 times the sync time on all six runs checked that day, slow or not. The one limit could only fail on a slow runner and could not tell a slower classifier apart.

## Fix

The test uses the sync as its yardstick. Sync runs on the same runner, in the same process, and isn't classifier code, so it slows with the runner but not with the classifier.

- `the_full_set_is_graded_on_the_whole_demo` times the sync of both accounts and checks the rest of the run (post-sync classification, analytics repair, reconcile and grading) against it. It fails when the rest takes more than 25 times the sync time (`WORK_PER_SYNC_BUDGET`). Estimated from the 2026-10-28 CI logs, the ratio was 13 to 14 on main's usual runners and about 10 on #735's slow runner. A local run of the full test on a laptop under heavy load (27 minutes, sync 93 s) measured 16.2 and passed the snapshot. So on a usual runner a classifier doing twice the work per email fails, and no runner measured so far comes near the budget. The ratio isn't perfectly runner-independent: on a runner as slow as #735's, a doubling would land at about 20 and pass. The check runs after the snapshot, so a run that is both slower and wrong reports the wrong answers first.
- The test prints one markdown row per stage (`classify-eval-timing:` lines). A new `Summarise the timings` step, which runs even after a timeout, copies them and cargo's compile time into the job summary.
- `timeout-minutes` goes from 30 to 45. It now only stops a hung job; a 2.5 times slower runner took 29 minutes.

Splitting the two demo accounts into parallel jobs was rejected. Each job would compile the test binary again (3 to 7 minutes of a 4-vCPU runner), and the snapshot grades both accounts together. The demo is built with both accounts in the address registry, so a job holding one account would grade a different mailbox from the one `mxr demo` builds, under two new snapshots. A larger runner costs more on every run to fix a problem on a few.

Cost: none on a normal run. The summary step is a `sed` over the log. A slow runner that used to be cancelled at 30 minutes now finishes in 30 to 45, instead of failing and being re-run in full.

