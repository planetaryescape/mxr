# Flaky semantic test: source-kind filter over-fetch

**Status:** resolved 2026-10-02 · **Found:** 2026-09-30 on main's merge commit for #255

`search_over_fetches_candidates_so_a_source_kind_filter_can_still_answer`
(`crates/semantic/src/lib.rs`) failed in the `Test (semantic-local)` CI job
on #255's merge commit and on PR #274 (run 37029953013, job 110914114221),
both times with `hits.len()` 0 against 1. Neither change touched the
semantic crate.

## Semantic search could not reach some chunks

The flake came from a bug in the product's dependency, not from the test.
hnsw_rs 0.3.4 writes each back-link onto the new point's top layer instead
of the layer the forward edge was built on
(`reverse_update_neighborhood_simple`, `let l_n = n_to_add.point_ref.p_id.0`).
A chunk whose random level is 1 or higher gets no layer-0 in-edges, and
search, which finishes on layer 0, can only reach it if it is the entry
point. A larger `ef` cannot fix this because no edge leads to the chunk.

The test indexes two chunks, a header at `[1, 0]` and a body at `[0.8, 0.6]`.
Whenever the body chunk drew level 1 or higher, the search returned only
the header, the `Body` filter removed it, and the search answered nothing.

In production the same bug silently drops results. With the index's real
parameters (M 16, 16 layers, ef_construction 200, search ef 64), 57 of
5,000 random 384-dimensional vectors (1.1%) were not returned even when
queried with their own vector.

Upstream fixed it in jean-pierreBoth/hnswlib-rs@6256b00b11 (2026-09-12) but
has not published a release; crates.io's latest is still 0.3.4. See upstream
issue #37 for the analysis.

## Fix

`vendor/hnsw_rs` is hnsw_rs 0.3.4 with that commit's two `src/hnsw.rs`
hunks applied, wired in with `[patch.crates-io]` in the workspace
`Cargo.toml`. The vendored manifest drops the example, test and dev-only
entries that were not copied. Delete the directory and the patch entry once
0.3.5 or later is on crates.io.

`every_indexed_chunk_is_reachable_whatever_level_it_draws` builds 200
two-chunk indexes through `build_semantic_index` and checks both chunks
come back. It fails on 0.3.4 and passes on the patched copy.

## Evidence

All local runs were on macOS arm64 with `--features local`, from base
`origin/main` 3da0c119.

| Measurement | hnsw_rs 0.3.4 | patched |
| --- | --- | --- |
| Original test, single-test loop | 26 of 1,000 runs failed (2.6%) | 0 of 1,000 |
| Two-chunk index built directly, 5,000 builds | 298 missed a chunk (6%) | 0 |
| Self-recall, 5,000 × 384-d vectors, k = ef = 64 | 57 unreachable (1.1%) | 0 |
| Regression test, 200 builds per run | fails | passes |
