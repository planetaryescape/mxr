# Grow the classifier's quick set to about 2,000 emails

Status: deferred from R1b (blueprint 32, build order). Not a bug; a gap between the design and what the quick set holds.

Blueprint 32 describes the quick set as about 2,000 emails: every near-miss twin, every attention timeline, and a seeded sample of ordinary cases. Today it is the 147 labelled emails of the curated demo fixtures and the 24 twin pairs (`crates/provider-fake/src/fixtures/scenarios/curated.rs` and `twins.rs`). R1b made `mxr classify eval --set full` grade every labelled email of the 50,000-message demo and put that in CI, so the 147 are no longer the only regression gate. The quick set is still what an agent runs after every change, and it is thin for that.

## What to do

- Add a seeded sample of the generated bulk to `--set quick`: a fixed number of threads chosen by thread number, covering every family and every visible phrasing, never a held-out one (`scenarios/held_out.rs`). `bulk::locate` and `bulk::label` already answer for any index, so the sample is a list of thread numbers, not stored labels.
- Decide the size from the full run's time: the quick set must stay fast enough to run after every change. Measure it before fixing a number.
- Add a near-miss twin for each bulk family the full set shows a cluster of misses on, as the redesign loop says (blueprint 32, "A change is a redesign checked against the corpus").
- Attention timelines belong to `now eval` (R6) and are out of scope here.

## Why it waits

The full set already catches a regression in CI, and R2 is the next rung. Growing the quick set is worth doing once the full set's first clusters of misses are known, because those say which cases the sample should over-represent.
