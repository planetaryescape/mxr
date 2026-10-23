# The full-set classifier eval changes its answer with the time of day

Status: fixed 2026-10-23 (fix/classify-eval-clock). Found while landing #676 (keyboard), which doesn't touch the classifier.

## What happens

`handler::tests::classify_eval_full::the_full_set_is_graded_on_the_whole_demo` (the CI job "Classifier Eval (full set)") passed on main at 106cd3e2a in CI at 01:34 UTC. From about 01:44 UTC the same test failed on #676 twice, and it also fails on a clean checkout of 106cd3e2a run locally at about 02:30 BST. The snapshot diff is the same each time: the `pay` ask finds 11 to-dos instead of 9, two more bill-due rows (`bulk-bill-issued ask: expected pay, got no row: 42` instead of 44).

## Evidence

- CI run 39884951803 (job "Classifier Eval (full set)"), failed twice; log artifact `classify-eval-full-log`.
- Local: `cargo test -p mxr --lib -- --ignored --exact handler::tests::classify_eval_full::the_full_set_is_graded_on_the_whole_demo` on detached origin/main 106cd3e2a, the same +2 diff.

## Cause

The demo dated its mail from the wall clock (`FakeProvider::with_demo_dataset`), and the to-do scan read the wall clock too (`todos::scan_messages`). A bulk bill writes its due date as a day ("due by Friday 24 October"), counted from its email's date and time (`bulk/render.rs`), and the to-do pass treats it as lapsed once that day ends (`timing::place`, `dates::end_of_day`). So which bills were already over when found depended on the hour the job ran.

## Fix

The eval builds both demo accounts with `with_demo_dataset_at` and runs the to-do and record scans with `post_sync_classification`'s new `now`, all pinned to today at 12:00 UTC. The committed snapshot passes unchanged at 02:40 UTC, an hour at which the unpinned test failed. Deliveries, Reading, Updates and arrivals placement still read the wall clock; none of them showed a time-dependent diff.

## Why it matters

The job fails on unrelated PRs and will fail on main's next push, so it stops telling anyone whether the classifier changed.
