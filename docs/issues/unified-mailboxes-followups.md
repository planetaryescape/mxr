# Unified system mailboxes: follow-ups

**Status:** open · **Found:** the "All accounts shows every account's Inbox"
change (`fix/unified-system-mailboxes`) · **Severity:** low to medium · Logged,
not fixed, in that change.

The change makes Inbox, Starred, Sent, Drafts, Spam, Trash and All Mail list
every enabled account's mail when no account is chosen (web, TUI, `mxr threads
--label`). These were left out on purpose.

## A thread id shared by two accounts lists once

Legacy Gmail thread ids are not account-scoped, so one id can hold messages of
two accounts. `Store::get_threads_batch` keys on the thread id alone, so the
merged listing returns that thread once, under one account, rather than once
per account. Pinned by
`a_thread_id_shared_across_accounts_is_listed_once` in
`crates/store/src/system_mailbox.rs`.

## A role view drops a row it cannot place

In a merged web thread row, which of the thread's messages are "in the lens" is
decided per message against the labels of the message's own account that have
the mailbox's role (`Membership::Role` in `crates/web/src/mailbox_threads.rs`),
by provider label id, plus the starred flag for Starred. A thread the daemon
lists but none of whose messages match is left out of the page instead of
falling back to the whole thread, because a thread-level action on the whole
thread could reach messages outside the mailbox. If label provider ids ever
drift from the store, such rows disappear rather than act too widely. The
account-scoped views keep the older whole-thread fallback.

## Label role is derived by name

The store does not persist `Label::role` (`row_to_label`), so a System label
gets its mailbox role from its name when the role field is empty. A provider
that names a system label something outside the table in `mxr-core`
(`SYSTEM_MAILBOX_NAMES`) will not join the merged mailbox. Persisting the role
at sync time would remove the name table.
