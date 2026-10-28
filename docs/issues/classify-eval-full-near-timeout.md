# The full-set classifier eval runs up against its 30-minute limit

Status: open. Found 2026-10-28 while landing #735 (keyboard), which doesn't touch the classifier.

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
