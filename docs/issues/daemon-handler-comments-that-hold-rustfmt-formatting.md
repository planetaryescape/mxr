# Five handler comments stay because deleting them makes rustfmt rewrite code

Status: open. Found: delete-comment sweep of `crates/daemon/src/handler`.

rustfmt skips a construct that holds a comment. Deleting the comment lets rustfmt reformat the code, which changes tokens (braces around a match arm, trailing commas, expanded struct bodies). The sweep is comment-only, so these comments stay:

- `account_scope.rs`, the comment before `Request::ListSignatures` in the scope match.
- `mod.rs`, the doc comment on `Materialized::TooLarge`.
- `send_recovery.rs`, the comment inside the match on the `failure(...)` result.
- `diagnostics/search_execute.rs`, the doc comment on `TextFallback`.

Smallest fix: delete these comments, run `cargo fmt`, and commit the reformatted code as a formatting-only change.
