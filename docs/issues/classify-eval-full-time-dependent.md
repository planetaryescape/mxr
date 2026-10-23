# The full-set classifier eval changes its answer with the time of day

Status: open. Found 2026-10-23 while landing #676 (keyboard), which doesn't touch the classifier.

## What happens

`handler::tests::classify_eval_full::the_full_set_is_graded_on_the_whole_demo` (the CI job "Classifier Eval (full set)") passed on main at 106cd3e2a in CI at 01:34 UTC. From about 01:44 UTC the same test failed on #676 twice, and it also fails on a clean checkout of 106cd3e2a run locally at about 02:30 BST. The snapshot diff is the same each time: the `pay` ask finds 11 to-dos instead of 9, two more bill-due rows (`bulk-bill-issued ask: expected pay, got no row: 42` instead of 44).

## Evidence

- CI run 39884951803 (job "Classifier Eval (full set)"), failed twice; log artifact `classify-eval-full-log`.
- Local: `cargo test -p mxr --lib -- --ignored --exact handler::tests::classify_eval_full::the_full_set_is_graded_on_the_whole_demo` on detached origin/main 106cd3e2a, the same +2 diff.

## Suspected cause (not confirmed)

The bill-due extraction or the demo's dates read the wall clock, so two demo bills cross a due or act-by window as the day moves on. Confirm by pinning the clock the eval uses (or logging the `now` it grades against) and rerunning across the boundary.

## Why it matters

The job fails on unrelated PRs and will fail on main's next push, so it stops telling anyone whether the classifier changed.
