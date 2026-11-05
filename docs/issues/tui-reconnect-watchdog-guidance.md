# TUI reconnect guidance clips at 130 columns

Status: observed follow-up, not a release blocker.

The "Daemon not responding" modal now opens during a long reconnect, because the outage timer keeps its first timestamp (`set_connection_state` in `crates/tui/src/app/mod.rs`). The fixture at candidate `4b2d19d621d0fba90eb9c5f0dbef570dd68ad28f` also showed that its 130-column terminal frame clips the rest of the reconnect error after the service-manager guidance. The retained frame hash is in [`manifest.json`](../build-log/daemon-portability/evidence/s08c-candidate-4b2d19d6/manifest.json).

Still open: make the reconnect guidance readable at 130 columns, then re-run the captured 130-column fixture against the fix. This needs the supervised fixture, which is not reproducible in a unit test here, so the timer fix was verified by unit tests only.
