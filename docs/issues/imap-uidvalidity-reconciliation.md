# IMAP UIDVALIDITY resets leave stale rows, and a reused UID inherits the old message

Status: logged, not fixed. A reconciliation was built on `fix/delete-derived`
and pulled out after four review rounds kept finding ways for it to delete
live or recoverable mail. The code is on the local branch
`wip/imap-uidvalidity-reconcile` (not pushed) as a reference. On a reset mxr
keeps main's behaviour: refetch the folder, delete nothing. That leaks; it
never loses mail.

## Two problems after a reset

IMAP provider ids are `folder:uid` (`crates/provider-imap/src/folders.rs`,
`format_provider_id`), and the message id is derived from account, folder
and UID. When a folder's `UIDVALIDITY` changes, every old UID is void and the
provider refetches the folder from `1:*`
(`crates/provider-imap/src/lib.rs`, `delta_sync_folder` and the Gmail All
Mail branch).

- **Stale rows.** A stored row whose UID the server no longer has is never
  deleted: the vanished-UID diff only runs within one validity, and QRESYNC
  sends no `VANISHED` across a reset. The row, its derived data and its
  files stay for good.
- **A reused UID inherits the old message.** The refetch upserts a new email
  under the same `folder:uid`, so it overwrites the old row in place and
  keeps the old message's attachments, flags, pins, snoozes and derived
  rows (summaries, decisions, vectors until re-ingest).

## What a design has to handle

Each of these produced a data-loss finding against the branch code.

- **Partial `UID SEARCH ALL`.** A successful search is not proof of a
  complete listing. An empty or short answer, or one that races new
  arrivals, deleted live rows. A listing has to be checked against `EXISTS`
  from the same SELECT, and an empty answer with `EXISTS > 0` is never
  trusted.
- **Refetch failure.** A UID whose FETCH failed (parse error, timeout) is on
  the server but was not upserted. A listing sent from the same pass must
  not treat it as gone; the cursor floor for failed UIDs
  (`floor_uid_next_to_failed`) also means stored rows can sit above the
  cursor's UIDNEXT, so the cursor cannot bound a cleanup either.
- **Gmail over IMAP and Trash/Spam.** Gmail IMAP syncs only All Mail. A
  message moved to Trash or Spam leaves All Mail but Gmail can still
  restore it. Absence from All Mail is not deletion, so no listing-based
  delete may run there unless Trash and Spam are synced too.
- **Rename and recreate.** Cursors are keyed by folder name. A folder
  renamed away and a new folder created under the old name look like a
  validity reset of one folder, and a listing of the new folder deleted
  the renamed folder's old rows. Stored ids carry the folder name, not the
  validity, so rows cannot be tied to a validity today. STATUS failures
  (unknown validity of another folder) make a rename impossible to rule
  out.
- **Reissued replacement without a listing.** Replacing a stored row when a
  reused UID brings a different email (matched by Message-ID, else date,
  sender and subject) still ran when the listing was suppressed for any of
  the reasons above, so it acted on exactly the cases judged unsafe.
- **A revived id and owed cleanup.** A UID reused for a new message revives
  the deleted message's id. Cleanup owed to the old message (files,
  semantic entries) must not reach the new one. This part is fixed on main
  by the deletion work: a synced message cancels cleanup owed to its id,
  and cleanup and sync serialise on the store's message-cleanup lock.

## A safe design, sketched

1. **Key stored ids by validity.** Store the folder's UIDVALIDITY with each
   IMAP row (a column, or `folder:validity:uid` as the provider id with a
   migration that rewrites existing ids from the cursor). A reused UID under
   a new validity is then a new id and a new row; nothing is overwritten in
   place, and the old row stays until it is known to be gone.
2. **Mark stale, then confirm.** After a reset, mark rows of the old
   validity in that folder stale instead of deleting them. Delete a stale
   row only after two clean syncs in a row each produced a complete listing
   (count equal to `EXISTS`, no FETCH failures) that does not contain the
   message, matched by Message-ID across validities so a message that only
   changed UID is re-linked (state carried over) rather than deleted.
3. **Never for Gmail All Mail** unless Trash and Spam are synced as well;
   without them, absence proves nothing.
4. **Folder identity.** Before treating a name as the same folder, compare
   validities across all listed folders. If the old validity appears under
   another name, treat it as a rename (re-key the rows to the new name). If
   any folder's validity is unknown, do nothing this pass.
5. **Dry-run first.** Expose the pending stale set through `mxr doctor` (or a
   CLI report) with counts and samples before any delete path ships, and
   log counts on every pass.
