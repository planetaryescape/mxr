//! Query work must follow the requested IDs rather than account history.
use super::tests::answer;
use super::ASKS_FOR_MESSAGES;
use crate::{
    test_fixtures::{test_account, TestEnvelopeBuilder},
    Store,
};
use mxr_core::{asks::ASKS_POLICY_VERSION, AccountId, MessageId};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Instant;

async fn grow(store: &Store, account: &AccountId, count: usize) -> Vec<MessageId> {
    let ids: Vec<_> = (0..count).map(|_| MessageId::new()).collect();
    sqlx::query("INSERT INTO messages(id, account_id, provider_id, thread_id, from_email, to_addrs, date) SELECT value, ?1, value, value, 'sender@example.test', '[]', 0 FROM json_each(?2)")
        .bind(account.as_str()).bind(serde_json::to_string(&ids).unwrap())
        .execute(store.writer()).await.unwrap();
    sqlx::query("INSERT INTO mail_assessments(message_id, account_id, policy_version, input_fingerprint, input_revision, assessment_json) SELECT m.id, m.account_id, ?2, 'probe', i.effective_revision, '{}' FROM messages m JOIN assessment_effective_thread_inputs i ON i.account_id = m.account_id AND i.thread_id = m.thread_id WHERE m.account_id = ?1 ON CONFLICT(message_id, policy_version) DO UPDATE SET input_revision = excluded.input_revision")
        .bind(account.as_str()).bind(ASKS_POLICY_VERSION).execute(store.writer()).await.unwrap();
    ids
}

async fn steps(store: &Store, account: &AccountId, id: &MessageId) -> usize {
    let ids = serde_json::to_string(&[id]).unwrap();
    let mut conn = store.reader().acquire().await.unwrap();
    // Only the static production query is prefixed; values remain bound.
    let plan: Vec<(i64, i64, i64, String)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "EXPLAIN QUERY PLAN {ASKS_FOR_MESSAGES}"
    )))
    .bind(account.as_str())
    .bind(ASKS_POLICY_VERSION)
    .bind(&ids)
    .fetch_all(&mut *conn)
    .await
    .unwrap();
    eprintln!("native asks lookup plan: {plan:?}");
    let _: Vec<String> = sqlx::query_scalar(ASKS_FOR_MESSAGES)
        .bind(account.as_str())
        .bind(ASKS_POLICY_VERSION)
        .bind(&ids)
        .fetch_all(&mut *conn)
        .await
        .unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let increment = count.clone();
    conn.lock_handle()
        .await
        .unwrap()
        .set_progress_handler(1, move || {
            increment.fetch_add(1, Ordering::Relaxed);
            true
        });
    let started = Instant::now();
    let rows: Vec<String> = sqlx::query_scalar(ASKS_FOR_MESSAGES)
        .bind(account.as_str())
        .bind(ASKS_POLICY_VERSION)
        .bind(ids)
        .fetch_all(&mut *conn)
        .await
        .unwrap();
    conn.lock_handle().await.unwrap().remove_progress_handler();
    assert_eq!(rows, ["{}"]);
    let count = count.load(Ordering::Relaxed);
    eprintln!(
        "native asks singleton: {count} VM steps in {:?}",
        started.elapsed()
    );
    count
}

#[tokio::test]
async fn singleton_lookup_work_is_bounded_by_requested_ids_not_account_size() {
    let store = Store::in_memory().await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();
    let started = Instant::now();
    let ids = grow(&store, &account.id, 1_000).await;
    eprintln!("native full-schema seed 1000: {:?}", started.elapsed());
    let small = steps(&store, &account.id, &ids[0]).await;
    let started = Instant::now();
    grow(&store, &account.id, 9_000).await;
    eprintln!("native full-schema grow 10000: {:?}", started.elapsed());
    let large = steps(&store, &account.id, &ids[0]).await;
    assert!(
        large <= small * 2,
        "singleton work grew with account size: {small} -> {large}"
    );
}

#[tokio::test]
async fn requested_ids_keep_batch_set_semantics_and_account_policy_boundaries() {
    let store = Store::in_memory().await.unwrap();
    let account = test_account();
    let other = test_account();
    store.insert_account(&account).await.unwrap();
    store.insert_account(&other).await.unwrap();
    let first = TestEnvelopeBuilder::new()
        .account_id(account.id.clone())
        .build();
    let mut second = TestEnvelopeBuilder::new()
        .account_id(account.id.clone())
        .build();
    second.provider_id = "second".into();
    let foreign = TestEnvelopeBuilder::new()
        .account_id(other.id.clone())
        .build();
    for message in [&first, &second, &foreign] {
        store.upsert_envelope(message).await.unwrap();
    }
    for (own, messages) in [
        (&account.id, vec![&first, &second]),
        (&other.id, vec![&foreign]),
    ] {
        let threads = messages
            .iter()
            .map(|m| m.thread_id.clone())
            .collect::<Vec<_>>();
        let revisions = store
            .assessment_thread_revisions(own, &threads)
            .await
            .unwrap();
        let answers = messages.iter().map(|m| answer(own, m)).collect::<Vec<_>>();
        assert!(store
            .save_asks_batch(own, &revisions, &answers)
            .await
            .unwrap());
    }
    let requested = [
        second.id.clone(),
        first.id.clone(),
        foreign.id.clone(),
        first.id.clone(),
        mxr_core::MessageId::new(),
    ];
    let found = store
        .asks_for_messages(&account.id, &requested)
        .await
        .unwrap();
    assert_eq!(found.len(), 2);
    assert_eq!(found[&first.id], answer(&account.id, &first));
    assert_eq!(found[&second.id], answer(&account.id, &second));
    assert!(store
        .asks_for_messages(&account.id, &[])
        .await
        .unwrap()
        .is_empty());
    sqlx::query(
        "UPDATE mail_assessments SET policy_version = 'retired-policy' WHERE message_id = ?",
    )
    .bind(first.id.as_str())
    .execute(store.writer())
    .await
    .unwrap();
    let found = store
        .asks_for_messages(&account.id, &requested)
        .await
        .unwrap();
    assert_eq!(found.len(), 1);
    assert!(found.contains_key(&second.id));
}
