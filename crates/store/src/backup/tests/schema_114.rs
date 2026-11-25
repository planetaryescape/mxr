//! Backups taken at schema 114, before `message_arrivals.arrived_live`.

use super::*;
use crate::pool::tests::store_at_version;

/// A live alert arrival implies the message arrived live, so a backup from
/// before the column gets the same repair migration 115 gives a database.
#[tokio::test]
async fn a_backup_from_schema_114_marks_live_alert_arrivals_as_arrived_live() {
    let source = tempfile::tempdir().unwrap();
    let stage = tempfile::tempdir().unwrap();
    let store = store_at_version(&source.path().join("mxr.db"), 114).await;
    let account = test_account();
    store.insert_account(&account).await.unwrap();
    for (id, is_live) in [("live-alert", 1), ("backfilled", 0)] {
        sqlx::query(
            "INSERT INTO messages (id, account_id, provider_id, thread_id, from_email, to_addrs, date, direction)
             VALUES (?, ?, ?, ?, 'sender@example.com', '[]', 100, 'inbound')",
        )
        .bind(id)
        .bind(account.id.as_str())
        .bind(id)
        .bind(ThreadId::new().as_str())
        .execute(store.writer())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO message_arrivals (message_id, account_id, from_host, is_live)
             VALUES (?, ?, 'example.com', ?)",
        )
        .bind(id)
        .bind(account.id.as_str())
        .bind(is_live)
        .execute(store.writer())
        .await
        .unwrap();
    }
    store.writer().close().await;

    let manifest = write_manifest(
        stage.path(),
        populate(source.path(), stage.path()).await.unwrap(),
    );
    assert_eq!(manifest.schema_version, 115);
    let staged = Store::new(&stage.path().join("mxr.db")).await.unwrap();
    let arrived: Vec<(String, i64)> =
        sqlx::query_as("SELECT message_id, arrived_live FROM message_arrivals ORDER BY message_id")
            .fetch_all(staged.reader())
            .await
            .unwrap();
    assert_eq!(
        arrived,
        vec![("backfilled".into(), 0), ("live-alert".into(), 1)]
    );
}
