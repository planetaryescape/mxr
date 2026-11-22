# The web bridge's client error mapping keeps shapes and arms from before the mxr-client refactor

In `crates/web/src/lib.rs`, the function that maps `ClientError` to `BridgeError` carries three leftovers the deleted comments explained:
- `ClientError::UnexpectedFrame` for a wrong-id response is unreachable now that `with_start_id` pins the wire id, but the arm stays.
- `ClientError::Transport` is unreachable because the bridge dials by path, but the arm keeps the match exhaustive.
- The connect failure detail is the bare io-error string, kept so the payload matches the pre-refactor one.

Smallest fix: if no client depends on the old payload strings, remove the unreachable arms with `unreachable!` or let the compiler's exhaustiveness checks guide a narrower error type, and use the `ClientError::Connect` display.
