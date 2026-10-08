# Archive subscriptions: account-scoped one-off counts

**Status:** open · **Found:** 2026-10-08 on #315 integration at `9af71ee4`

When the subscriptions request covers every account, the one-off record IDs
are merged by issuer alone in
`crates/records/src/subscriptions/load.rs:60-65`. The response then counts
those IDs by issuer for each subscription in
`crates/daemon/src/handler/record_subscriptions.rs:153-157`. If two accounts
have subscriptions from the same issuer, each response row can include
one-off records from both accounts.

Account-scoped requests still filter records to the requested account:
`record_subscriptions::list` passes the account scope into `load`
(`crates/daemon/src/handler/record_subscriptions.rs:28-33`), and the records
query applies those account IDs (`crates/records/src/subscriptions/load.rs:27-35`).

After v0.6.59, retain the account key while aggregating one-off IDs and add a
regression with the same issuer in two accounts but different one-off counts.
