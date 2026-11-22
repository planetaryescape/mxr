# TUI treats some errors as unreachable and falls back silently

## Where

- `crates/tui/src/app/compose_helpers.rs` and `crates/tui/src/compose_flow.rs`: a failed parse of `fm.from` is treated as "no override" because `validate_draft` ran earlier.
- `crates/tui/src/client.rs`, `map_request_error`: the `ClientError::Transport` arm "is unreachable here" and exists to keep the match exhaustive.
- `crates/tui/src/app/analytics_actions.rs`: the mime drill-down falls back to a generic `has:attachment` jump because the lexical engine does not index mime types.

## Problem

Each site hides a failure behind a comment that says it cannot happen, or works around a missing capability. If the assumption breaks, the code swallows the error.

## Smallest fix

Carry the parsed `from` through the type so no second parse exists. Return an explicit error for the unreachable `Transport` arm. Index mime types in the lexical engine, or remove the mime drill-down.
