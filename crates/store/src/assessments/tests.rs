use crate::{test_fixtures::*, Store};
use mxr_core::{AccountId, MessageBody, MessageDirection, MessageMetadata, ThreadId};

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
    // The SQL can finish before a crash prevents the final version stamp.
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
async fn batch_acknowledgement_materializes_json_once_instead_of_correlated_scans() {
    let store = Store::in_memory().await.unwrap();
    let plan: Vec<(i64, i64, i64, String)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "EXPLAIN QUERY PLAN {}",
        super::ACKNOWLEDGE_BATCH
    )))
    .bind("account")
    .bind(r#"{"thread":1}"#)
    .fetch_all(store.reader())
    .await
    .unwrap();
    let details: Vec<_> = plan.iter().map(|row| row.3.as_str()).collect();
    assert!(
        details
            .iter()
            .any(|step| step.contains("MATERIALIZE expected")),
        "{details:?}"
    );
    assert!(
        !details.iter().any(|step| step.contains("CORRELATED")),
        "{details:?}"
    );
}

#[tokio::test]
async fn sender_ingest_writes_scale_linearly_and_lazy_pages_keep_new_mutations() {
    let store = Store::in_memory().await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();
    let mut first = None;
    let mut last_thread = ThreadId::new();
    let start: i64 = sqlx::query_scalar("SELECT total_changes()")
        .fetch_one(store.writer())
        .await
        .unwrap();
    for i in 0..1000 {
        let mut mail = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        mail.provider_id = format!("sender-ingest-{i}");
        store
            .upsert_envelope_with_direction(&mail, MessageDirection::Inbound)
            .await
            .unwrap();
        last_thread = mail.thread_id.clone();
        first.get_or_insert(mail);
        if i == 199 || i == 999 {
            let changes: i64 = sqlx::query_scalar("SELECT total_changes()")
                .fetch_one(store.writer())
                .await
                .unwrap();
            assert!(
                changes - start < (i + 1) * 30,
                "history fanout: {} changes for {} messages",
                changes - start,
                i + 1
            );
        }
    }
    acknowledge_all(&store, &account.id).await;
    let first = first.unwrap();
    let old = revisions(&store, &account.id, &last_thread).await;
    let before_body: i64 = sqlx::query_scalar("SELECT total_changes()")
        .fetch_one(store.writer())
        .await
        .unwrap();
    let body = MessageBody {
        message_id: first.id.clone(),
        text_plain: Some("new sender evidence".into()),
        text_html: None,
        attachments: vec![],
        fetched_at: chrono::Utc::now(),
        metadata: MessageMetadata::default(),
    };
    store.insert_body(&body).await.unwrap();
    let after_body: i64 = sqlx::query_scalar("SELECT total_changes()")
        .fetch_one(store.writer())
        .await
        .unwrap();
    assert!(
        after_body - before_body < 30,
        "a body must not rewrite sender history"
    );
    // Dependency changes reject CAS immediately, before any page is drained.
    assert!(!store
        .save_assessment_batch(&account.id, &old, &[])
        .await
        .unwrap());
    let page = store
        .dirty_assessment_threads(&account.id, None)
        .await
        .unwrap();
    assert_eq!(page.len(), 100);
    let expected = store
        .assessment_thread_revisions(&account.id, &page)
        .await
        .unwrap();
    let before_ack: i64 = sqlx::query_scalar("SELECT total_changes()")
        .fetch_one(store.writer())
        .await
        .unwrap();
    assert!(store
        .save_assessment_batch(&account.id, &expected, &[])
        .await
        .unwrap());
    let after_ack: i64 = sqlx::query_scalar("SELECT total_changes()")
        .fetch_one(store.writer())
        .await
        .unwrap();
    assert_eq!(
        after_ack - before_ack,
        100,
        "a dependency page only acknowledges its own 100 threads"
    );
    // Another change after this page settled must requeue that same page.
    sqlx::query("UPDATE bodies SET text_plain = 'changed again' WHERE message_id = ?")
        .bind(first.id.as_str())
        .execute(store.writer())
        .await
        .unwrap();
    assert!(!store
        .save_assessment_batch(&account.id, &expected, &[])
        .await
        .unwrap());
    assert_eq!(
        store
            .dirty_assessment_threads(&account.id, None)
            .await
            .unwrap(),
        page
    );
    acknowledge_all(&store, &account.id).await;
    assert!(store
        .dirty_assessment_threads(&account.id, None)
        .await
        .unwrap()
        .is_empty());
    // Deleting body evidence also invalidates the sender dependants.
    let old = store
        .assessment_thread_revisions(&account.id, &page)
        .await
        .unwrap();
    sqlx::query("DELETE FROM bodies WHERE message_id = ?")
        .bind(first.id.as_str())
        .execute(store.writer())
        .await
        .unwrap();
    assert!(!store
        .save_assessment_batch(&account.id, &old, &[])
        .await
        .unwrap());
    acknowledge_all(&store, &account.id).await;
    let old = store
        .assessment_thread_revisions(&account.id, &page)
        .await
        .unwrap();
    let mut sent = TestEnvelopeBuilder::new()
        .account_id(account.id.clone())
        .build();
    sent.provider_id = "outbound-to-common-sender".into();
    sent.from.email = account.email.clone();
    sent.to[0].email = "ALICE@example.com".into();
    let before_sent: i64 = sqlx::query_scalar("SELECT total_changes()")
        .fetch_one(store.writer())
        .await
        .unwrap();
    store
        .upsert_envelope_with_direction(&sent, MessageDirection::Outbound)
        .await
        .unwrap();
    let after_sent: i64 = sqlx::query_scalar("SELECT total_changes()")
        .fetch_one(store.writer())
        .await
        .unwrap();
    assert!(
        after_sent - before_sent < 30,
        "outbound counterparties must not rewrite sender history"
    );
    assert!(!store
        .save_assessment_batch(&account.id, &old, &[])
        .await
        .unwrap());
    // Reconciliation bookkeeping alone never invalidates a view cache.
    let seq = store.change_feed().seq();
    acknowledge_all(&store, &account.id).await;
    assert_eq!(store.change_feed().seq(), seq);
    sqlx::query("DELETE FROM accounts WHERE id = ?")
        .bind(account.id.as_str())
        .execute(store.writer())
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM assessment_sender_inputs")
        .fetch_one(store.reader())
        .await
        .unwrap();
    assert_eq!(count, 0);
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
