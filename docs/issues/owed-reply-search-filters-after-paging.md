# `is:owed-reply` in hybrid and semantic search is filtered after paging

**Status:** open (residual) · **Found:** verifier review of the bulk `--search` cap notice
(faf22f43d) · **Severity:** should-fix

## What still happens

In `hybrid` and `semantic` modes the owed filter runs after the query. It runs on
the page that `execute_search_ast` already cut, which is built from a fixed
candidate window and fused with dense hits. A short or empty page can still come
back, and `has_more` can still be wrong for those modes. The hidden-account check
also runs after fusion, on the fused page.

Moving the filter into the fused candidate list would change those modes' ranking,
so it is left for a separate change. Hybrid and semantic need a design decision on
how far the window should reach before the filter applies.

## Known limit: owed-side cap

The paged `is:owed-reply` path reads at most `OWED_REPLY_FETCH_CAP` (5,000) owed
rows per account (`search_execute.rs`). The bulk selection resolver
(`crates/daemon/src/handler/diagnostics/search_resolve.rs`, `owed_thread_ids(...,
u32::MAX)`) reads every owed row. A search and a bulk action on the same query can
therefore disagree in an account with more than 5,000 owed threads. The cap is not
changed here.
