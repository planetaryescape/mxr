# `is_early` in the mode handler is always false

Status: open. Found: delete-comment sweep of `crates/daemon/src/handler`.

`is_early` in `crates/daemon/src/handler/modes.rs` returns `!matches!(mode, ...)` over every mode variant, so it is false for all of them. The doc comment it carried said the other modes were "an early version on an existing view (the rest, for now)". That is no longer true. Call sites in the same file branch on it (`messages_membership`, the mode status) and the early branch never runs.

Smallest fix: remove `is_early`, fold the always-false branches at its callers, and drop the early-status path. This changes code, so the sweep only records it.
