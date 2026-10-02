# Batch mutations committed the search index once per message

**Status:** resolved 2026-10-02 · **Found:** 2026-10-02 through a flaky test

`mutations_and_delivery::async_mutation_job_reports_progress_and_undo_ids_for_large_batch`
polls its 405-message archive job for about 20 s. It passed alone in 10 to
17 s and timed out under full `-p mxr` load: 3 out of 3 full runs on another branch.

## Every archived message paid for a search commit

`apply_mutation_under_guard` ended each message with
`reindex_message_in_search`, and each call is one `SearchUpdateBatch`. The
search worker commits every batch and reloads the reader. Instrumented in
the test (debug build), the 405 reindexes took 11.75 s, a mean of 29 ms
each. The provider call, label reconcile and event log took about 1 to 2 ms
a message together. So the job's time grew with commit latency, and a busy
machine pushed it past the test's budget.

Users paid the same cost on every batch archive, read, star or label
change: one search commit per message.

## Fix

`apply_mutation_batch` records each message it attempted and reindexes
them all in one `SearchUpdateBatch` after the loop: one commit per
synchronous mutation, and one per 100-message chunk in a mutation job.
`apply_mutation_under_guard` no longer reindexes. Undo still reindexes per
snapshot as before.

The failed message is reindexed too, because ReadAndArchive can mark it
read before the archive fails, and the reindex reads current store state.

A failed reindex is now logged at `warn` and does not fail the mutation.
The store and the provider already hold the change, and reporting it as
failed would invite a retry of finished work and drop the undo entry.
Before, a reindex error failed that message and skipped the rest of its
account.

### No silent staleness when a read fails

If reading one message back from the store fails, the batch leaves out
only that message and still commits the rest. The skipped id goes into the
`search_reindex_pending` table (migration 57). Every later reindex retries
up to 500 marked ids, and startup maintenance drains the table. That
matters because a stale entry leaves the document count unchanged, so
the count-based startup repair never sees it. If the store can't take the
mark either, the id stays in memory and the next reindex in this daemon
run retries it.

### Last commit reads last

Batching moved the reindex out from under each message's provider guard,
so a batch could read a message, a concurrent single mutation could
change and commit it, and then the batch could commit its older read last.
Search would keep the old flags or labels, even across a restart.

Each reindex now holds `AppState::search_reindex` from its store reads
through its commit. Every mutation writes the store before it reindexes,
so whichever reindex commits last also read last, and search ends on the
store's newest state. Undo and the flag path go through the same function.
The lock covers mutation reindexes only; the sync engine's own index
writes don't take it.

Each of these tests fails against the broken variant named after it:

- `batch_archive_reindexes_every_message_in_search`: the batch reindex
  disabled.
- `a_failed_search_read_skips_only_that_message_and_marks_it_for_repair`:
  aborting the batch on the first failed read leaves all 3 messages stale.
- `a_mutation_reindex_commits_the_store_state_at_commit_time`: reading
  before taking the lock commits the pre-star state.
- `startup_maintenance_reindexes_messages_marked_stale`: startup without
  the drain.

## Evidence

Measured on macOS arm64, debug build, base `origin/main` 3da0c119.

| Measurement | Before | After |
| --- | --- | --- |
| Test alone | 18.0 s (job done at attempt 614 of 800) | 1.8 s |
| Test in full `-p mxr` nextest, 3 runs | 24.2 s, 14.2 s, 15.0 s (all passed) | 1.6 s, 4.2 s, 2.4 s |
| Full `-p mxr` runs failing | 0 of 3 | 0 of 3 |

The local full runs on main did not reproduce the timeout. A 24.2 s test,
including setup, against a poll loop that sleeps 20 s in total left almost
no margin. After the fix the slowest loaded run used about a fifth of the
budget.
