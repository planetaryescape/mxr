# `mxr::snooze` only re-exports `mxr_config::snooze`

**Status:** open · **Found:** delete-comment sweep, `b-daemon-rest` · **Severity:** nice-to-have

## What happens

`crates/daemon/src/snooze.rs` is a `pub use` of `mxr_config::snooze`, kept "for backward
compatibility" according to its deleted header comment.

## Smallest fix

Point the remaining `crate::snooze::` callers at `mxr_config::snooze` and delete the module.
