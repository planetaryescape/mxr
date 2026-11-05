# A Gmail draft-sync test intermittently finds no `accounts` table

**Status:** open · **Found:** 2026-11-04 on main and four branches · **Severity:**
low; a rerun passes, but it fails the `Test (semantic-local)` job and can block a
merge or a release check

## What happens

`handler::tests::draft_revisions::gmail_draft_sync_preserves_local_contents_on_lookup_or_download_failure`
panics at `crates/daemon/src/handler/tests/draft_revisions.rs:115`, the
`SyncNow` assertion in `sync_linked_draft`. The response it got:

```text
Error { message: "Store error: error returned from database: (code: 1) no such table: accounts", kind: Store, code: "store", retryable: false, details: None }
```

nextest stops at the first failure, so the rest of the job is cancelled.

The same panic, at the same line, in CI runs from 2026-11-04:

| Run | Branch |
|---|---|
| 41868657147 | main |
| 41860333402 | fix/key-only-tui-dogfood |
| 41867310900 | feat/key-gaps-now-todo-reading |
| 41898647329 | fix/starred-refresh-and-label-accounts |
| 41899951352 | feat/bulk-search-all-v2 (#851) |

A rerun of the failed job passed on #851. Main at `7cc40675f` passed.

## What is known about the setup

The test builds its state with `AppState::in_memory_with_sync_provider`, and
the Gmail provider talks to a `wiremock` server. `Store::in_memory`
(`crates/store/src/pool.rs`, at `e19851a4f`) migrates one template database
once per process, then copies it with `VACUUM INTO` into a per-store
`file:<name>?mode=memory&cache=shared` database. Both pools on that database
are set to `max_connections(1)`, `max_lifetime(None)` and `idle_timeout(None)`,
so connection recycling is already ruled out as the way the copy disappears.

The cause is not established. The error means the connection that served
`SyncNow` saw a database without the migrated tables. That could be a
different database name, or the shared database closing and reopening empty.

## What would settle it

Log the store's database name and the `sqlite_master` table count at the
failing point and in `Store::in_memory` after the copy, then loop the test
(`scripts/cargo-test -p mxr --lib -- draft_revisions::gmail_draft_sync --test-threads 5`
in a loop, or nextest `--retries 0` with many runs) until it reproduces.
