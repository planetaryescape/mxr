# A merged web row stays unread after marking its listed messages read

**Status:** open · **Severity:** low · **Found:** 2026-10-01 (star undo matrix, `verbs.spec`)

`row.unread` on a web mailbox row is computed over the whole thread, while
read and unread act on the row's listed messages. A thread with an unread
message outside the current lens keeps its row unread after `I`.

Star had the same shape and was fixed on 2026-10-01: rows carry
`starred_message_ids` (`crates/web/src/mailbox_threads.rs`) and the unstar
names exactly what made the row starred. Unread could follow the same
pattern with the row's unread message ids.
