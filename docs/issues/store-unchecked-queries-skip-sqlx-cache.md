# Two store queries bypass the sqlx compile-time check

Two queries use `sqlx::query(...)` instead of `sqlx::query!`. Their comments said this avoided regenerating the `.sqlx` cache. Deleting the comments left the reason in this issue.

- `crates/store/src/account.rs`, the query in the account update path (a `sqlx::query(` call that used to carry "Unchecked on purpose").
- `crates/store/src/scheduled_sends.rs`, the read-only join (a `sqlx::query(` call that used to carry "Unchecked query").

These queries get no compile-time column or type check.

Smallest fix: convert both to `sqlx::query!` and run the sqlx prepare workflow once (see the memory note on the sqlx prepare workflow). The cost is a `.sqlx` cache change.
