# TUI reply cache keeps recipients from before an alias was registered

**Status:** fixed · **Found:** 2026-10-21 on `fix/reply-own-sent` (cross-model review of PR #580)

The TUI prewarms `PrepareReply` for the message under the cursor and keeps
the result in `reply_context_cache` for the session
(`crates/tui/src/runner.rs` prewarm skips cached ids; `compose_actions.rs`
dispatches cached contexts; `compose_flow.rs` uses them without asking the
daemon again). Nothing invalidates that cache when the account's owned
addresses change.

Journey: view a message you sent from an alias that isn't registered yet, so
the TUI caches its reply context. Register the alias with
`mxr accounts addresses add`. Press `r` on the same message in the same
session: To is still your alias, because the cached context predates the
registration. `a` also leaves the real recipient in Cc. The editor shows
the To line before anything sends, and a new TUI session gets the right
recipients.

Fix: drop `reply_context_cache` when the daemon reports an account-address
change (or on any accounts event), including in-flight prewarms, or
version cached contexts by the account's address-set revision.

## Fixed

The daemon now sends `DaemonEvent::AccountAddressesChanged` after an alias is
added, removed or made primary (`crates/daemon/src/handler/diagnostics/mod.rs`).
The TUI clears `reply_context_cache` and bumps `reply_context_generation`
(`ComposeState::invalidate_reply_contexts`, `crates/tui/src/app/state/compose.rs`).
A prewarm carries the generation it started under, and a result from an older
generation is not cached (`cache_warmed_reply_context`). A deferred reply
then takes the cold path. Tests: `adding_an_alias_tells_clients_that_cached_reply_contexts_may_be_stale`,
`an_account_address_change_drops_every_cached_reply_context`,
`a_prewarm_started_before_an_address_change_is_not_cached`.
