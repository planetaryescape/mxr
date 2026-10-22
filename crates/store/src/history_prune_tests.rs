use crate::test_fixtures::test_account;
use crate::{count_prunable_events, count_prunable_sync_logs, Store, SyncLogEntry, SyncStatus};
use chrono::Utc;
use mxr_core::AccountId;

const DAY: i64 = 86_400;

/// Inserts `count` sync rows for `account` in one statement. Row `i` (0-based)
/// starts at `base + i * step`, so `step = 0` makes every row a tie.
async fn seed_sync_rows(store: &Store, account: &AccountId, count: i64, base: i64, step: i64) {
    sqlx::query(
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < ?4)
         INSERT INTO sync_log (account_id, started_at, status)
         SELECT ?1, ?2 + (i - 1) * ?3, 'success' FROM n",
    )
    .bind(account.as_str())
    .bind(base)
    .bind(step)
    .bind(count)
    .execute(store.writer())
    .await
    .unwrap();
}

async fn seed_events(store: &Store, count: i64, timestamp: i64, category: &str, summary: &str) {
    sqlx::query(
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < ?1)
         INSERT INTO event_log (timestamp, level, category, summary)
         SELECT ?2, 'info', ?3, ?4 FROM n",
    )
    .bind(count)
    .bind(timestamp)
    .bind(category)
    .bind(summary)
    .execute(store.writer())
    .await
    .unwrap();
}

async fn sync_row_count(store: &Store, account: &AccountId) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM sync_log WHERE account_id = ?")
        .bind(account.as_str())
        .fetch_one(store.reader())
        .await
        .unwrap()
}

fn ids(rows: &[SyncLogEntry]) -> Vec<i64> {
    rows.iter().map(|row| row.id).collect()
}

async fn newest_50(store: &Store, account: &AccountId) -> Vec<i64> {
    ids(&store.list_recent_sync_logs(account, 50).await.unwrap())
}

async fn fresh_store_with_account() -> (Store, AccountId) {
    let store = Store::in_memory().await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();
    (store, account.id)
}

#[tokio::test]
async fn sync_prune_keeps_newest_50_per_account_and_deletes_older_rows_past_cutoff() {
    let (store, busy) = fresh_store_with_account().await;
    let now = Utc::now().timestamp();
    let cutoff = now - 90 * DAY;

    // 60 old rows, then 5 recent ones. The 5 recent rows and the newest 45
    // old rows are the account's newest 50, so 15 old rows are deletable.
    seed_sync_rows(&store, &busy, 60, now - 200 * DAY, 1).await;
    seed_sync_rows(&store, &busy, 5, now - DAY, 1).await;

    // A rarely syncing account: every row is older than the cutoff, but all
    // three are in its newest 50, so none may go.
    let quiet = AccountId::new();
    let mut quiet_account = test_account();
    quiet_account.id = quiet.clone();
    quiet_account.email = "quiet@example.com".into();
    store.insert_account(&quiet_account).await.unwrap();
    seed_sync_rows(&store, &quiet, 3, now - 300 * DAY, 1).await;

    let before = newest_50(&store, &busy).await;
    let mut conn = store.reader().acquire().await.unwrap();
    let preview = count_prunable_sync_logs(&mut conn, cutoff, 50)
        .await
        .unwrap();
    drop(conn);

    let deleted = store.prune_sync_logs_before(cutoff, 50).await.unwrap();

    assert_eq!(preview, 15, "dry-run count must match the real prune");
    assert_eq!(deleted, 15);
    assert_eq!(sync_row_count(&store, &busy).await, 50);
    assert_eq!(newest_50(&store, &busy).await, before);
    assert_eq!(sync_row_count(&store, &quiet).await, 3);
}

#[tokio::test]
async fn sync_prune_tie_breaks_match_the_rows_doctor_reads() {
    // All 55 rows share one started_at, so only the id tie-break separates
    // them. The prune must keep exactly the rows doctor's window reads.
    let (store, account) = fresh_store_with_account().await;
    let now = Utc::now().timestamp();
    seed_sync_rows(&store, &account, 55, now - 200 * DAY, 0).await;

    let before = newest_50(&store, &account).await;
    let deleted = store
        .prune_sync_logs_before(now - 90 * DAY, 50)
        .await
        .unwrap();

    assert_eq!(deleted, 5);
    assert_eq!(newest_50(&store, &account).await, before);
    assert_eq!(sync_row_count(&store, &account).await, 50);
}

#[tokio::test]
async fn sync_prune_ranks_by_started_at_not_insert_order() {
    // The row inserted first is the newest by started_at. Ranking by id would
    // keep the wrong 50.
    let (store, account) = fresh_store_with_account().await;
    let now = Utc::now().timestamp();
    seed_sync_rows(&store, &account, 55, now - 200 * DAY, -1).await;

    let before = newest_50(&store, &account).await;
    let deleted = store
        .prune_sync_logs_before(now - 90 * DAY, 50)
        .await
        .unwrap();

    assert_eq!(deleted, 5);
    assert_eq!(newest_50(&store, &account).await, before);
}

#[tokio::test]
async fn sync_prune_crosses_batch_boundary_and_keeps_newest_rows() {
    // 5,500 rows is more than one 5,000-row batch.
    let (store, account) = fresh_store_with_account().await;
    let now = Utc::now().timestamp();
    seed_sync_rows(&store, &account, 5_500, now - 200 * DAY, 1).await;

    let before = newest_50(&store, &account).await;
    let deleted = store
        .prune_sync_logs_before(now - 90 * DAY, 50)
        .await
        .unwrap();

    assert_eq!(deleted, 5_450);
    assert_eq!(sync_row_count(&store, &account).await, 50);
    assert_eq!(newest_50(&store, &account).await, before);
}

#[tokio::test]
async fn sync_prune_leaves_a_running_row_in_window_and_tolerates_pruned_ids() {
    let (store, account) = fresh_store_with_account().await;
    let now = Utc::now().timestamp();
    seed_sync_rows(&store, &account, 60, now - 300 * DAY, 1).await;
    // A sync that started 200 days ago and is still running is the newest
    // row by started_at, so it stays.
    let running = store
        .insert_sync_log(&account, &SyncStatus::Running)
        .await
        .unwrap();
    sqlx::query("UPDATE sync_log SET started_at = ? WHERE id = ?")
        .bind(now - 200 * DAY)
        .bind(running)
        .execute(store.writer())
        .await
        .unwrap();
    let oldest: i64 = sqlx::query_scalar("SELECT MIN(id) FROM sync_log WHERE account_id = ?")
        .bind(account.as_str())
        .fetch_one(store.reader())
        .await
        .unwrap();

    store
        .prune_sync_logs_before(now - 90 * DAY, 50)
        .await
        .unwrap();

    // Completing a kept row works; completing a pruned row is a no-op.
    store
        .complete_sync_log(running, &SyncStatus::Success, 3, None)
        .await
        .unwrap();
    store
        .complete_sync_log(oldest, &SyncStatus::Success, 3, None)
        .await
        .unwrap();
    let kept = store.get_last_sync(&account).await.unwrap().unwrap();
    assert_eq!(kept.id, running);
    assert_eq!(kept.status, SyncStatus::Success);
    assert_eq!(sync_row_count(&store, &account).await, 50);
}

#[tokio::test]
async fn sync_prune_cutoff_is_strict_and_in_unix_seconds() {
    let (store, account) = fresh_store_with_account().await;
    let cutoff = 1_700_000_000;
    seed_sync_rows(&store, &account, 1, cutoff - 1, 0).await;
    seed_sync_rows(&store, &account, 1, cutoff, 0).await;
    // Fifty newer rows keep the two above from being protected by the window.
    seed_sync_rows(&store, &account, 50, cutoff + DAY, 1).await;

    let deleted = store.prune_sync_logs_before(cutoff, 50).await.unwrap();

    assert_eq!(deleted, 1, "only the row strictly before the cutoff goes");
    assert_eq!(sync_row_count(&store, &account).await, 51);
}

#[tokio::test]
async fn event_prune_keeps_only_the_newest_lexical_rebuild_marker() {
    let store = Store::in_memory().await.unwrap();
    let now = Utc::now().timestamp();
    let cutoff = now - 90 * DAY;
    seed_events(
        &store,
        1,
        now - 300 * DAY,
        "search",
        "Lexical index rebuilt",
    )
    .await;
    let newest_marker = now - 200 * DAY;
    seed_events(&store, 1, newest_marker, "search", "Lexical index rebuilt").await;
    seed_events(&store, 3, now - 200 * DAY, "sync", "Sync finished").await;
    seed_events(&store, 2, now - DAY, "sync", "Sync finished").await;

    let mut conn = store.reader().acquire().await.unwrap();
    let preview = count_prunable_events(&mut conn, cutoff).await.unwrap();
    drop(conn);
    let deleted = store.prune_events_before(cutoff).await.unwrap();

    assert_eq!(preview, 4, "dry-run count must match the real prune");
    assert_eq!(deleted, 4);
    let kept = store
        .latest_event_timestamp("search", Some("Lexical index rebuilt"))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(kept.timestamp(), newest_marker);
}

#[tokio::test]
async fn event_prune_without_any_marker_deletes_old_rows_without_error() {
    let store = Store::in_memory().await.unwrap();
    let now = Utc::now().timestamp();
    seed_events(&store, 4, now - 200 * DAY, "sync", "Sync finished").await;

    let deleted = store.prune_events_before(now - 90 * DAY).await.unwrap();

    assert_eq!(deleted, 4);
}

#[tokio::test]
async fn event_prune_crosses_batch_boundary() {
    let store = Store::in_memory().await.unwrap();
    let now = Utc::now().timestamp();
    seed_events(&store, 5_500, now - 200 * DAY, "sync", "Sync finished").await;

    let deleted = store.prune_events_before(now - 90 * DAY).await.unwrap();

    assert_eq!(deleted, 5_500);
}
