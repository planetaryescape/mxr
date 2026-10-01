# Web star: no undo on `u`, and a conversation's unstar can miss

**Status:** open · **Found:** 2026-10-01 on `feat/polish` (undo matrix, `verbs.spec`)

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
