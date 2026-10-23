use crate::{test_fixtures::*, Store};
use mxr_core::asks::*;
use mxr_core::{AccountId, MessageDirection};

fn answer(account: &AccountId, message: &mxr_core::Envelope) -> AskAssessment {
    AskAssessment {
        account_id: account.clone(),
        message_id: message.id.clone(),
        policy_version: ASKS_POLICY_VERSION.into(),
        input_fingerprint: "fingerprint".into(),
        pending_body: true,
        facts: AskFacts {
            zones: vec![],
            recipient_role: RecipientRole::Unknown,
            owner_authored: true,
            owner_names: vec![],
            owner_addresses: vec![],
            predecessor: None,
            owner_replied_afterwards: false,
            amounts: vec![],
            dates: vec![],
            references: vec![],
            template_key: "template".into(),
        },
        units: vec![],
    }
}

#[tokio::test]
async fn sent_asks_persist_without_inbound_home_and_stale_answers_read_pending() {
    let store = Store::in_memory().await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();
    let message = TestEnvelopeBuilder::new()
        .account_id(account.id.clone())
        .build();
    store
        .upsert_envelope_with_direction(&message, MessageDirection::Outbound)
        .await
        .unwrap();
    let revisions = store
        .assessment_thread_revisions(&account.id, std::slice::from_ref(&message.thread_id))
        .await
        .unwrap();
    let ask = answer(&account.id, &message);
    assert!(store
        .save_asks_batch(&account.id, &revisions, std::slice::from_ref(&ask))
        .await
        .unwrap());
    let rows = store
        .asks_for_messages(&account.id, std::slice::from_ref(&message.id))
        .await
        .unwrap();
    assert_eq!(rows.get(&message.id), Some(&ask));
    assert!(store
        .assessments(&account.id, mxr_core::assessment::CLASSIFIER_POLICY_VERSION)
        .await
        .unwrap()
        .is_empty());
    assert!(store
        .assessment_versions(std::slice::from_ref(&account.id))
        .await
        .unwrap()
        .is_empty());
    sqlx::query("UPDATE messages SET subject = 'changed' WHERE id = ?")
        .bind(message.id.as_str())
        .execute(store.writer())
        .await
        .unwrap();
    assert!(store
        .asks_for_messages(&account.id, std::slice::from_ref(&message.id))
        .await
        .unwrap()
        .is_empty());
    assert!(!store
        .save_asks_batch(&account.id, &revisions, &[ask])
        .await
        .unwrap());
}

#[tokio::test]
async fn per_thread_tokens_keep_other_threads_readable_and_reruns_do_not_write() {
    let store = Store::in_memory().await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();
    let first = TestEnvelopeBuilder::new()
        .account_id(account.id.clone())
        .build();
    let mut second = TestEnvelopeBuilder::new()
        .account_id(account.id.clone())
        .build();
    second.from.email = "other@example.com".into();
    for message in [&first, &second] {
        store.upsert_envelope(message).await.unwrap();
    }
    let revisions = store
        .assessment_thread_revisions(
            &account.id,
            &[first.thread_id.clone(), second.thread_id.clone()],
        )
        .await
        .unwrap();
    let answers = [answer(&account.id, &first), answer(&account.id, &second)];
    assert!(store
        .save_asks_batch(&account.id, &revisions, &answers)
        .await
        .unwrap());
    let ids = [first.id.clone(), second.id.clone()];
    assert_eq!(
        store
            .asks_for_messages(&account.id, &ids)
            .await
            .unwrap()
            .len(),
        2
    );
    let before: i64 = sqlx::query_scalar("SELECT total_changes()")
        .fetch_one(store.writer())
        .await
        .unwrap();
    assert!(store
        .save_asks_batch(&account.id, &revisions, &answers)
        .await
        .unwrap());
    let after: i64 = sqlx::query_scalar("SELECT total_changes()")
        .fetch_one(store.writer())
        .await
        .unwrap();
    assert_eq!(before, after);
    sqlx::query("UPDATE messages SET subject = 'new evidence' WHERE id = ?")
        .bind(first.id.as_str())
        .execute(store.writer())
        .await
        .unwrap();
    let remaining = store.asks_for_messages(&account.id, &ids).await.unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining.get(&second.id), Some(&answers[1]));
}

#[tokio::test]
async fn settled_asks_read_and_empty_cas_do_not_wait_for_the_writer() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(&dir.path().join("asks.db")).await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();
    let message = TestEnvelopeBuilder::new()
        .account_id(account.id.clone())
        .build();
    store.upsert_envelope(&message).await.unwrap();
    let revisions = store
        .assessment_thread_revisions(&account.id, std::slice::from_ref(&message.thread_id))
        .await
        .unwrap();
    let ask = answer(&account.id, &message);
    assert!(store
        .save_asks_batch(&account.id, &revisions, &[ask])
        .await
        .unwrap());
    let tx = store.writer().begin_with("BEGIN IMMEDIATE").await.unwrap();
    let read = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        store.asks_for_messages(&account.id, std::slice::from_ref(&message.id)),
    )
    .await
    .expect("asks read must use reader pool")
    .unwrap();
    assert_eq!(read.len(), 1);
    assert!(tokio::time::timeout(
        std::time::Duration::from_secs(1),
        store.save_asks_batch(&account.id, &revisions, &[])
    )
    .await
    .expect("empty CAS must use reader pool")
    .unwrap());
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn batches_reject_wrong_account_policy_and_unobserved_message_atomically() {
    let store = Store::in_memory().await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();
    let message = TestEnvelopeBuilder::new()
        .account_id(account.id.clone())
        .build();
    store.upsert_envelope(&message).await.unwrap();
    let revisions = store
        .assessment_thread_revisions(&account.id, std::slice::from_ref(&message.thread_id))
        .await
        .unwrap();
    let mut ask = answer(&account.id, &message);
    ask.account_id = AccountId::new();
    assert!(store
        .save_asks_batch(&account.id, &revisions, &[ask.clone()])
        .await
        .is_err());
    ask.account_id = account.id.clone();
    ask.policy_version = "r2-1".into();
    assert!(store
        .save_asks_batch(&account.id, &revisions, &[ask.clone()])
        .await
        .is_err());
    ask.policy_version = ASKS_POLICY_VERSION.into();
    ask.message_id = mxr_core::MessageId::new();
    assert!(store
        .save_asks_batch(&account.id, &revisions, &[ask])
        .await
        .is_err());
    assert!(store
        .asks_for_messages(&account.id, std::slice::from_ref(&message.id))
        .await
        .unwrap()
        .is_empty());
}
