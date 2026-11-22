use crate::{test_fixtures::*, Store};
use mxr_core::{AccountId, MessageDirection, ThreadId};

async fn revisions(
    store: &Store,
    account: &AccountId,
    thread: &ThreadId,
) -> std::collections::HashMap<ThreadId, i64> {
    store
        .assessment_thread_revisions(account, std::slice::from_ref(thread))
        .await
        .unwrap()
}

async fn acknowledge_all(store: &Store, account: &AccountId) {
    let mut after = None;
    loop {
        let page = store
            .dirty_assessment_threads(account, after.as_deref())
            .await
            .unwrap();
        let Some(last) = page.last() else { break };
        after = Some(last.as_str());
        let expected = store
            .assessment_thread_revisions(account, &page)
            .await
            .unwrap();
        assert!(store
            .save_assessment_batch(account, &expected, &[])
            .await
            .unwrap());
    }
}

#[tokio::test]
async fn unstamped_assessment_migration_preserves_existing_thread_generations() {
    let store = Store::in_memory().await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();
    let mut messages = Vec::new();
    for _ in 0..2 {
        let mail = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        store
            .upsert_envelope_with_direction(&mail, MessageDirection::Inbound)
            .await
            .unwrap();
        messages.push(mail);
    }
    acknowledge_all(&store, &account.id).await;
    sqlx::query("UPDATE messages SET subject = 'changed evidence' WHERE id = ?")
        .bind(messages[0].id.as_str())
        .execute(store.writer())
        .await
        .unwrap();
    let snapshot =
        "SELECT thread_id, revision, dirty FROM assessment_thread_inputs ORDER BY thread_id";
    let before: Vec<(String, i64, i64)> = sqlx::query_as(snapshot)
        .fetch_all(store.reader())
        .await
        .unwrap();
    assert_eq!(before.len(), 2);
    assert!(before.iter().all(|(_, revision, _)| *revision > 0));
    assert!(before.iter().any(|(_, _, dirty)| *dirty == 0));
    assert!(before.iter().any(|(_, _, dirty)| *dirty == 1));
    sqlx::query("DELETE FROM schema_migrations WHERE version = 97")
        .execute(store.writer())
        .await
        .unwrap();
    store.run_migrations().await.unwrap();
    let after: Vec<(String, i64, i64)> = sqlx::query_as(snapshot)
        .fetch_all(store.reader())
        .await
        .unwrap();
    assert_eq!(after, before);
    let stamps: i64 =
        sqlx::query_scalar("SELECT count(*) FROM schema_migrations WHERE version = 97")
            .fetch_one(store.reader())
            .await
            .unwrap();
    assert_eq!(stamps, 1);
}

#[tokio::test]
async fn idle_batch_checks_stale_generations_without_acquiring_the_writer() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(&dir.path().join("idle.db")).await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();
    let mail = TestEnvelopeBuilder::new()
        .account_id(account.id.clone())
        .build();
    store
        .upsert_envelope_with_direction(&mail, MessageDirection::Inbound)
        .await
        .unwrap();
    acknowledge_all(&store, &account.id).await;
    let old = revisions(&store, &account.id, &mail.thread_id).await;
    let tx = store.writer().begin_with("BEGIN IMMEDIATE").await.unwrap();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        store.save_assessment_batch(&account.id, &old, &[]),
    )
    .await
    .expect("idle CAS must use the separate reader pool")
    .unwrap();
    assert!(result);
    tx.rollback().await.unwrap();
    sqlx::query("UPDATE messages SET subject = 'changed' WHERE id = ?")
        .bind(mail.id.as_str())
        .execute(store.writer())
        .await
        .unwrap();
    acknowledge_all(&store, &account.id).await;
    let tx = store.writer().begin_with("BEGIN IMMEDIATE").await.unwrap();
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        store.save_assessment_batch(&account.id, &old, &[]),
    )
    .await
    .expect("stale settled CAS must reject through the reader pool")
    .unwrap();
    assert!(
        !result,
        "empty answers do not waive the store's generation contract"
    );
    tx.rollback().await.unwrap();
}
