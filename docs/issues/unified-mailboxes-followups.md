# Unified system mailboxes: follow-ups

**Status:** open · **Found:** the "All accounts shows every account's Inbox"
change (`fix/unified-system-mailboxes`) · **Severity:** low to medium · Logged,
not fixed, in that change.

The change makes Inbox, Starred, Sent, Drafts, Spam, Trash and All Mail list
every enabled account's mail when no account is chosen (web, TUI, `mxr threads
--label`). These were left out on purpose.

## User labels still show only the default account's

The web sidebar and the TUI sidebar list user labels from `ListLabels` with no
account, which resolves to the default account. A label that exists only on
another account is not reachable from "All accounts". Same-name labels across
accounts would need a decision first: merge them into one entry, or show them
tagged by account. Counts for user labels are the default account's own.

- Web: `crates/web/src/chrome.rs` (`build_bridge_chrome`, user-label items in
  `build_sidebar_sections`).
- TUI: `crates/tui/src/app/sidebar_helpers.rs`.

## Categories and subscriptions without an account

`ListCategories { account_id: None }` and `ListSubscriptions { account_id: None }`
in `build_bridge_chrome` still resolve to the default account.

## Admin chrome

`crates/web/src/admin_handlers.rs` builds the bridge chrome with no account, so
it now makes the extra `ListLabels { all_accounts: true }` call and throws the
sums away. Harmless, and not worth a parameter until something else needs it.

## A thread id shared by two accounts lists once

Legacy Gmail thread ids are not account-scoped, so one id can hold messages of
two accounts. `Store::get_threads_batch` keys on the thread id alone, so the
merged listing returns that thread once, under one account, rather than once
per account. Pinned by
`a_thread_id_shared_across_accounts_is_listed_once` in
`crates/store/src/system_mailbox.rs`.

## Thread rows decide membership by provider label id

In a merged thread row, the web decides which of the thread's messages are "in
the lens" by comparing each message's label provider ids with the default
account's label (`envelope_has_label` in `crates/web/src/mailbox_threads.rs`).
Gmail and the fake provider use `INBOX` as the provider id, and the IMAP mapping
names its Inbox label `INBOX`, so these match across accounts. An account whose
inbox message carries neither string would fail the match and the row falls back
to the whole thread. Row counts and ordering come
from the daemon and are right; only the thread-level action scope can be wider
than the lens.

## Label role is derived by name

The store does not persist `Label::role` (`row_to_label`), so a System label
gets its mailbox role from its name when the role field is empty. A provider
that names a system label something outside the table in `mxr-core`
(`SYSTEM_MAILBOX_NAMES`) will not join the merged mailbox. Persisting the role
at sync time would remove the name table.
