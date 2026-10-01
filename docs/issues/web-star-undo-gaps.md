# Web star: no undo on `u`, and a conversation's unstar can miss

**Status:** resolved 2026-10-01 on `fix/unstar` · **Found:** 2026-10-01 on `feat/polish` (undo matrix, `verbs.spec`)

## 1. `u` after a star says "Nothing to undo"

The daemon keeps no undo record for a star (`undoable_kind` in
`crates/daemon/src/handler/mutations.rs` returns `None`: a star is its own
inverse). The verb table said star and unstar undo through the daemon, and
the old `verbs.spec` passed only because an earlier verb's "Undone" toast
was still on screen. The table now says `toggle`: pressing `s` again is the
way back, and `verbs.spec` checks that. `u` still answers "Nothing to undo"
after a star.

Fix: record the prior starred flag per message in the daemon's undo log, so
`u` restores it exactly (a client-side inverse would unstar messages that
were already starred before a conversation-wide star).

## 2. Unstarring a conversation can leave it starred

A conversation row is starred when any of its messages is, including
messages outside the list being shown (a starred reply in Sent). The row's
target holds only the list's message ids, so `s` on such a row sends
`starred: false` for the inbox message alone and the row stays starred.
Seen in the demo data on "Launch checklist for Project Aurora" (one inbox
message, one sent message, both starred). `verbs.spec` uses a
single-message row for unstar until this is fixed.

Fix: unstar the whole conversation (every message id in the thread) when the
target is one conversation, or compute the row's starred flag from the
listed messages only.

## Resolution

- Star is undoable. `undoable_kind` maps `Star` to the new
  `UndoableMutationKind::Star`, whose snapshot keeps each message's prior
  flags. Undo sets each message back to its own prior star at the provider
  (`SetStarred`: Gmail `STARRED`, IMAP `\Flagged`) and locally, and leaves
  labels alone. A message whose star failed part way is uncertain, so undo
  sends its prior star to the provider whatever the local copy shows. Read
  and star restores share `restore_flag` in `handler/mutations.rs`.
- The fix for the unstar sits where the mismatch starts: the bridge's
  `thread_row` already reads every envelope of the thread to set `starred`,
  so it now also returns `starred_message_ids`. The web target's
  `unstarIds` is the row's messages plus those, so the unstar names exactly
  what made the row starred. No protocol change; the daemon just gets the
  ids. Starring keeps its semantics: it stars the listed messages.
- TUI rows show their newest listed message's star. A starred row of
  several messages used to star again on `s`; it now unstars them.
- `verbs.spec` unstars the demo's "Launch checklist for Project Aurora"
  (starred inbox message and starred reply in Sent), checks both lose the
  star, then `u` restores both. Daemon tests: `handler/tests/stars.rs`.
- Left open: `row.unread` has the same shape (whole thread) while read and
  unread act on the listed messages, so an unread message outside the lens
  keeps the row unread after `I`.
