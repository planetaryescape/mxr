# Keep the TUI outage timer stable across reconnect attempts

Status: observed follow-up, not a release blocker.

The supervised TUI fixture at candidate `4b2d19d621d0fba90eb9c5f0dbef570dd68ad28f` showed repeated reconnect status frames without the watchdog error modal. In `crates/tui/src/ipc.rs`, failed reconnect attempts emit `ConnectionState::Reconnecting { since: Instant::now(), .. }` again before the two-second sleep. `crates/tui/src/app/mod.rs` opens its modal only after the current `since` exceeds `CONNECTION_STALE_THRESHOLD`, so the repeated timestamp resets prevent escalation while the retry loop continues. The fixture's 130-column terminal frame also clips the remainder of the long reconnect error after the service-manager guidance.

The captured fixture receipt confirms a 130-column terminal and visible guidance; the retained frame hash is in [`manifest.json`](../build-log/daemon-portability/evidence/s08c-candidate-4b2d19d6/manifest.json).

Follow-up: preserve the first outage timestamp until a successful connection, and render actionable reconnect guidance where it remains readable at the captured width. Add a focused test that keeps the connection in retry for longer than the watchdog threshold and verifies the modal appears. Re-run the captured 130-column fixture after the change.
