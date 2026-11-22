# Four handler comments stay because deleting them makes rustfmt rewrite code

Status: open. Found: delete-comment sweep of `crates/daemon/src/handler`.

rustfmt skips a construct that holds a comment. Deleting the comment lets rustfmt reformat the code, which changes tokens (braces around a match arm, trailing commas, expanded struct bodies). The sweep is comment-only, so these comments stay:

- `account_scope.rs`, `Request` or-pattern in the scope match, the comment before `Request::ListSignatures`.
- `mod.rs`, the enum variant `TooLarge { bytes: u64 }` area.
- `send_recovery.rs`, the match arm on the `failure(...)` result.
- `diagnostics/search_execute.rs`, one comment at about line 173.

Smallest fix: run `cargo fmt` once with these comments deleted, commit the reformatted code as a separate formatting-only change, then delete the comments.
