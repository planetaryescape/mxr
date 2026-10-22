# A failed registry refresh leaves sync with the previous address set

**Status:** fixed · **Found:** cross-model review of the owned-address
refactor (rounds 1 and 2) · **Severity:** should-fix

## What happens

`AppState::refresh_account_addresses` (`crates/daemon/src/state.rs`) rebuilds
sync's address registry after every address change. If the rebuild fails, it
logs a warning and the handler still reports the change as done. Sync keeps
classifying with the previous set until a later refresh succeeds:

- a removed alias still counts as you in sync, or
- a newly added alias does not count yet.

Fixed: a failed rebuild marks the registry stale (`InMemoryAccountAddressLookup::mark_stale`), which reads as not loaded, so sync writes Unknown direction. `AppState::ensure_account_addresses_current` retries before each sync pass, analytics reclassification skips while stale, and `mxr doctor` reports the stale cache. Test: `failed_registry_rebuild_stops_sync_reading_the_set_from_before_it`.

Out-of-order publishes are fixed: rebuilds are serialised behind one async
mutex (`REGISTRY_REBUILD`), so a rebuild that read the store before a change
cannot publish after the rebuild that change triggered.

## Effect

Sends, reply defaults, From validation, safety checks and invite RSVP read the
store directly (`AppState::owned_addresses`), so they are unaffected. Sync uses
the registry to classify direction, and direction drives routing: a message
from a newly owned address misread as inbound can meet a screener denial, which
moves it out of the inbox (`crates/sync/src/engine.rs`, around the screener
routing). Analytics repair also reclassifies through the registry.

## Fix sketch

Return the refresh error to the handler that made the change, so the CLI shows
it and the user can retry, or retry the rebuild in the background until it
succeeds.
