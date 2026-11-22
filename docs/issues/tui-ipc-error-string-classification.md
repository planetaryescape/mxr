# TUI classifies IPC failures by substring match on error text

## Where

- `crates/tui/src/client.rs`, `map_request_error`: turns each `ClientError` into an `MxrError::Ipc(String)`.
- `crates/tui/src/ipc.rs`, `should_reconnect_ipc`: decides whether to reconnect by lowercasing that string and matching "broken pipe", "connection closed", "connection refused" and "connection reset".

## Problem

The reconnect decision depends on wording in two files. A message change in `map_request_error`, or in the `mxr-client` error text it passes through (`ClientError::Io`), silently stops reconnects. Nothing ties the two together.

A comment in `map_request_error` used to explain this coupling. It was deleted in the comment sweep because a comment cannot enforce it.

## Smallest fix

Give `MxrError::Ipc` callers a typed signal: have `map_request_error` return a small enum or a `reconnect: bool` next to the message, and have `should_reconnect_ipc` read it. Keep one test that each `ClientError` variant maps to the expected reconnect answer.
