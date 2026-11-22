# Two store spots work around missing structure with placeholders

- `crates/store/src/contacts.rs`, the `let _ = decode_id::<AccountId>;` line near the end of the module, exists only to keep a `decode_id` re-export in use "even when query helpers don't pull it". The fix is to drop the re-export or its dead use.
- `crates/store/src/wrapped.rs`, the asymmetry aggregate (`placeholder_last_seen`), fills `ContactAsymmetryRow.last_seen_at` with the window end because the row type requires a value and another query was judged too costly. The fix is to make `last_seen_at` optional on that row type, or compute it.

Each is a value that looks real and is not.
