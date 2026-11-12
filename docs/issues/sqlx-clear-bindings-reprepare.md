# sqlx re-prepares many cached SQLite statements on every run

Status: open, found 2026-11-12. Not fixed. The fix belongs in sqlx-sqlite, so it needs a decision first.

## What happens

sqlx-sqlite calls `sqlite3_clear_bindings` before it binds a cached statement (`vendor/sqlx-sqlite/src/connection/execute.rs`, the "sanity check" before `bind`) and again when it resets one (`vendor/sqlx-sqlite/src/statement/virtual.rs`, `VirtualStatement::reset`). sqlx does this so a parameter with no argument reads as NULL.

SQLite expires a statement in `sqlite3_clear_bindings` when the planner marked any of its parameters as "re-prepare on bind" (`if( p->expmask ) p->expired = 1;`). The planner marks parameters in several cases. One is an equality filter such as `col = ?`: `whereLoopOutputAdjust` calls `sqlite3ExprIsInteger` with a parse context, and that marks the parameter. Every marked statement is parsed and planned again on each run, so its cached plan is of no use.

## Evidence

- A sample of `classify_eval_full` (macOS `sample`, 200 s): on the SQLite worker thread, `sqlite3Reprepare` had 5,076 samples and `sqlite3VdbeExec` had 3,755. `sqlite3_prepare_v3` had 60, so the statement cache worked. Across all threads, re-preparing was about 21% of the active samples.
- `sqlite3_stmt_status(SQLITE_STMTSTATUS_REPREPARE)` on the test's connection: 6,535 re-prepares in 39,088 runs. The worst statements re-prepared on every run, for example `ASKS_FOR_MESSAGES` in `crates/store/src/asks.rs` (3,157 of 3,157) and the asks expiry `UPDATE todos` in `crates/store/src/asks_todos.rs` (2,145 of 2,145).
- A ctypes test against a migrated schema (`mxr-store`'s `create_db` example, SQLite 3.54.1): the same statements re-prepared on every run only when `sqlite3_clear_bindings` ran between runs.
- With a temporary switch that skipped `clear_bindings`, the `classify_eval_full` generated-demo test ran with 16% fewer instructions retired (133.6e9 to 112.6e9, three alternating runs each, `/usr/bin/time -l`).

The daemon uses the same sqlx code with its file database, so it pays this cost too.

## Options

1. Patch `vendor/sqlx-sqlite` to clear bindings only when the arguments do not cover every parameter, and otherwise let the new bind replace the old value. This extends a vendored copy that `sqlite-advisory-vendored-sqlx-sqlite.md` plans to delete, so it also needs an upstream sqlx change.
2. Turn on `SQLITE_DBCONFIG_ENABLE_QPSG` for each connection. SQLite then never marks parameters, but it also stops plans that depend on bound values, such as a partial index matched against `?`.
3. Rewrite only the hot statements. This is fragile, because any new `col = ?` filter brings the cost back.
