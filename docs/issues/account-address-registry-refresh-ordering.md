# Sync's address registry can publish an older snapshot

**Status:** open · **Found:** cross-model review of the owned-address
refactor (round 1) · **Severity:** should-fix (sync direction only)

## What happens

`AppState::refresh_account_addresses` (`crates/daemon/src/state.rs`) reads the
store, builds each account's set, then publishes it with
`InMemoryAccountAddressLookup::replace` (`crates/core/src/types.rs`). Nothing
serialises these refreshes or orders them. Two refreshes that overlap can
publish out of order, so an older snapshot can overwrite a newer one:

- a removed alias can come back into sync's set, or
- a newly added alias can disappear from it.

A refresh that fails only logs a warning, so sync keeps the previous set until
the next refresh succeeds.

## Why it is not in the same PR

Since the owned-address refactor, sends, reply defaults, From validation,
safety checks and invite RSVP read the store directly
(`AppState::owned_addresses`). Only sync's direction classification reads the
registry. So this race changes which messages are counted as outbound during
sync, not who a message is sent to or which From is accepted.

## Fix sketch

Serialise `load_account_address_registry` behind one async mutex, or give each
publish a monotonically increasing version and drop publishes older than the
current one. Either removes out-of-order publishes. Surfacing refresh errors to
the handler that made the change would also let a failed refresh be retried.
