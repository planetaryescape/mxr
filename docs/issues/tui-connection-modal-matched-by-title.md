# TUI closes the "daemon not responding" modal by matching its title

## Where

`crates/tui/src/app/mod.rs`, `App::set_connection_state`, the `ConnectionState::Connected` branch.

## Problem

On reconnect the TUI closes the open error modal only if `err.title.to_lowercase()` contains "daemon". Any other modal whose title mentions the daemon is closed too, and a retitled watchdog modal would stay open.

The comment that said so stays in place for now: removing it lets clippy's `collapsible_if` fire on the nested `if`, and fixing that is a code change this sweep does not make.

## Smallest fix

Mark the watchdog's modal with a typed origin (an enum field on the error modal state) and match on that. Then collapse the `if` and delete the comment.
