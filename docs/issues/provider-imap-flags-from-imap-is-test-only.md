# `flags_from_imap` in provider-imap is a keyword-dropping wrapper used only by tests

## Where

`crates/provider-imap/src/parse.rs`, `flags_from_imap`. Its doc comment said it was "kept for callers that don't yet thread keywords through".

## Smallest fix

At the base commit only `crates/provider-imap/tests/live_smoke.rs` and the unit tests in `parse.rs` call it. Point those at `flags_and_keywords_from_imap` and delete the wrapper.
