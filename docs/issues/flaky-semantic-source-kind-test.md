# Flaky semantic test: source-kind filter over-fetch

**Status:** open · **Found:** 2026-09-30 on main's merge commit for #255

`search_over_fetches_candidates_so_a_source_kind_filter_can_still_answer`
(`crates/semantic/src/lib.rs`) failed once in the `Test (semantic-local)`
CI job at 2745:9 (`hits.len()` or the chunk kind), while the identical tree
passed on the PR. #255 does not touch the semantic crate.

The test expects a filtered search with `limit = 1` to return the body
chunk even though the header chunk is the nearest neighbour. Approximate
nearest-neighbour search can return a different candidate set between runs,
so the over-fetch may not always include the body chunk.

To fix: make the over-fetch factor deterministic for tiny indexes (search
exactly when the index holds fewer vectors than the over-fetch), or assert
against an exact search in this test. Rerun the test 200 times to confirm.
