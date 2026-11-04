# Whole-job undo for --all was dropped after four undo defects

**Status:** open, not scheduled · **Found:** 2026-11-04, verifier round 4 on
`feat/bulk-search-all` at `213d92667` · **Decision:** D191

`mxr archive|read-archive|label|unlabel --search … --all` shipped without
undo. Whole-job undo was built for it first: an `UndoJob` request and
`mxr undo <job_id>`, open for an hour after the job ended, reversing every
chunk's undo entry as a background job of its own. Entries were claimed in
the store through a `claimed_by` column (migration 107), so a chunk undo and a
whole-job undo could not both restore one entry. Four review rounds kept
finding undo defects, and BK had decided beforehand (2026-10-29) that `--all`
would ship without whole-job undo if round 4 still found material ones. It
did, so the request, the column, the open-ended undo windows and the
per-message snapshots of `--all` jobs were removed. An `--all` job now writes
no undo entries, and its preview and confirmation prompt say it cannot be
undone.

Bringing it back means fixing each of the four, and holding the invariant
under each one, with a test that reproduces the finding.

## A restarted undo restored a message twice

The undo kept its progress (which snapshots were already restored) in memory
while it waited out a rate limit; the stored entry still held the restored
messages. Restart the daemon during that wait, archive message A again, then
retry the original undo within its hour: startup released the claim and the
retry restored A again, undoing the later archive.

**Invariant:** a snapshot is removed from the stored entry, durably, as soon as
it is restored, and before the undo waits, releases a claim or replies. No
restart can replay a restored snapshot.

## A restart during a read-archive pause lost the unread snapshot

Read-archive marks a message read and then archives it. When the provider
rate-limited the archive, the job kept the message's pre-run snapshot only in
memory (to carry it to the retry) and persisted the job before sleeping. A
restart during the pause left the message read with no undo entry, and a
re-run snapshotted the already-read state.

**Invariant:** every message a job may have changed has its pre-run snapshot
in a stored undo entry before the job waits, yields or ends. Nothing a job
needs for undo lives only in memory across a pause.

## An uncertain label addition was reported undone while it stayed applied upstream

When the provider accepted `label Work` but its response was lost, the
message's snapshot was marked uncertain. The undo had only the mutation kind
and the prior labels. Locally the message never gained Work, so restoring the
prior labels changed nothing, the provider was never asked to remove Work, and
the undo reported success and consumed the snapshot.

**Invariant:** a label edit's undo entry records the delta it applied (the
frozen provider ids added and removed), and undoing an uncertain snapshot
reverses that delta at the provider. An undo reports a message restored only
once the provider state is back to the snapshot.

## Pruning job history deleted undo inside its hour

Job rows are pruned to the newest 100. Finish job J, start 100 more jobs
within the hour, and J's row is gone: `mxr undo J` failed although its
snapshots were still inside their window.

**Invariant:** a job is kept at least until its undo window closes; history
pruning never removes a job whose undo is still open.
