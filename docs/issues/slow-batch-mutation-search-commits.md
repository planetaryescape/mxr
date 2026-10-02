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

`batch_archive_reindexes_every_message_in_search` archives three indexed
inbox messages in one request and checks `in:inbox` no longer finds any.
It fails with the batch reindex disabled.

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
