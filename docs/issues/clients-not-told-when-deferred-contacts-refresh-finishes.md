# Open analytics views are not told when a deferred contacts refresh finishes

**Status:** open · **Found:** cross-model review of PR #665 (round 3) · **Severity:** should-fix

## What happens

An address change sends `AccountAddressesChanged`, and the web app and the TUI
refresh their analytics on it. Some of the data behind those views is rebuilt
later, though:

- The contacts table is queued with `ContactsRefreshHandle` and rebuilt after
  a debounce (`crates/store/src/contacts_refresh_handle.rs`).
- The reconciliation after the next sync pass emits no event, and
  `SyncCompleted` refreshes mail queries but not analytics
  (`apps/web/src/hooks/useDaemonEventInvalidation.ts`).

Analytics queries do not poll, and the web query client turns off refresh on
focus. So an analytics or contacts view left open can show counts from before
the rebuild until it is reloaded.

## Fix sketch

Emit an event when a contacts refresh commits, and when a post-pass direction
reconciliation changes anything, and have the web app and the TUI refresh
analytics on it.
