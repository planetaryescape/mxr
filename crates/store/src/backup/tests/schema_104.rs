//! Backups taken at schema 104, from a database migrated only that far.
//!
//! Migration 105 adds `labels.role` and changes no key or table shape, so a
//! 104 bundle is expected to differ from today's schema by that one column.

use super::*;
use crate::pool::tests::store_at_version;
use mxr_core::types::Role;

/// Two accounts with a synced Inbox each, in a database at schema 104: the
/// labels have no role column.
async fn inbox_source(root: &Path) -> (Account, Account) {
    let store = store_at_version(&root.join("mxr.db"), 104).await;
    let (a, b) = (test_account(), test_account());
    store.insert_account(&a).await.unwrap();
    store.insert_account(&b).await.unwrap();
    for (id, account) in [
        ("00000000-0000-0000-0000-00000000000a", &a),
        ("00000000-0000-0000-0000-00000000000b", &b),
    ] {
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
        let label = format!("inbox-{}", account.id);
        sqlx::query(
            "INSERT INTO labels (id, account_id, name, kind, provider_id, unread_count, total_count)
             VALUES (?, ?, 'INBOX', 'system', 'INBOX', 0, 1)",
        )
        .bind(&label)
        .bind(account.id.as_str())
        .execute(store.writer())
        .await
        .unwrap();
        sqlx::query("INSERT INTO message_labels (message_id, label_id) VALUES (?, ?)")
            .bind(id)
            .bind(&label)
            .execute(store.writer())
            .await
            .unwrap();
    }
    store.writer().close().await;
    (a, b)
}

#[tokio::test]
async fn a_backup_from_schema_104_restores_with_label_roles() {
    let source = tempfile::tempdir().unwrap();
    let stage = tempfile::tempdir().unwrap();
    let restored = tempfile::tempdir().unwrap();
    let (a, b) = inbox_source(source.path()).await;

    let manifest = write_manifest(
        stage.path(),
        populate(source.path(), stage.path()).await.unwrap(),
    );
    assert_eq!(manifest.schema_version, 114);
    inspect(stage.path()).await.unwrap();
    std::fs::copy(stage.path().join("mxr.db"), restored.path().join("mxr.db")).unwrap();
    retire_restored(&restored.path().join("mxr.db"))
        .await
        .unwrap();
    let store = Store::new(&restored.path().join("mxr.db")).await.unwrap();

    let roles: Vec<Option<String>> = sqlx::query_scalar("SELECT role FROM labels")
        .fetch_all(store.writer())
        .await
        .unwrap();
    assert_eq!(roles, vec![Some("inbox".to_string()); 2]);
    let inbox = store
        .system_mailbox_counts(&[a.id.clone(), b.id.clone()])
        .await
        .unwrap()
        .into_iter()
        .find(|(role, _, _)| *role == Role::Inbox)
        .map(|(_, _, total)| total);
    assert_eq!(inbox, Some(2));
    store.writer().close().await;
}
