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
        context_token: crate::asks_context_token().unwrap(),
        calendar: None,
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
    second.provider_id = "fake-2".into();
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

#[tokio::test]
async fn timezone_context_mismatch_is_pending_on_both_read_surfaces() {
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
    ask.context_token = "old-zone-rules".into();
    store
        .save_asks_batch(&account.id, &revisions, &[ask])
        .await
        .unwrap();
    assert!(store
        .asks_for_messages(&account.id, std::slice::from_ref(&message.id))
        .await
        .unwrap()
        .is_empty());
    let (home, asks) = store
        .assessment_facets_for_messages(&account.id, std::slice::from_ref(&message.id))
        .await
        .unwrap();
    assert!(home.is_empty() && asks.is_empty());
}

#[tokio::test]
async fn rejected_asks_roll_back_home_and_dirty_acknowledgement_together() {
    use mxr_core::assessment::*;
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
    let home = MailAssessment {
        message_id: message.id.clone(),
        account_id: account.id.clone(),
        policy_version: CLASSIFIER_POLICY_VERSION.into(),
        input_fingerprint: "home".into(),
        home: "messages".into(),
        who: "person".into(),
        facts: AssessmentFacts {
            sender_shape: "person".into(),
            has_list_id: false,
            has_unsubscribe: false,
            auto_generated: false,
            bulk: false,
            templated_sender: false,
            written_to: false,
            body_cached: false,
            recipients: 1,
            addressed: true,
        },
        deciding: AssessmentClaim {
            rule: "person".into(),
            reason: "person".into(),
            source: "rule".into(),
            evidence: vec![],
        },
        overridden: vec![],
        not_sure: None,
        fallback: false,
        pending_body: true,
    };
    assert!(store
        .save_assessment_facets(&account.id, &revisions, &[home], &[ask])
        .await
        .is_err());
    assert!(store
        .assessments(&account.id, CLASSIFIER_POLICY_VERSION)
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        store
            .dirty_assessment_threads(&account.id, None)
            .await
            .unwrap(),
        [message.thread_id]
    );
}

#[tokio::test]
async fn projection_outbox_survives_restart_and_rejects_same_fingerprint_new_generation_ack() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("mail.db");
    let account = test_account();
    let message = TestEnvelopeBuilder::new()
        .account_id(account.id.clone())
        .build();
    let mut ask = answer(&account.id, &message);
    ask.pending_body = false;
    let ticket = {
        let store = Store::new(&path).await.unwrap();
        store.insert_account(&account).await.unwrap();
        store.upsert_envelope(&message).await.unwrap();
        let revisions = store
            .assessment_thread_revisions(&account.id, std::slice::from_ref(&message.thread_id))
            .await
            .unwrap();
        store
            .save_assessment_facets(&account.id, &revisions, &[], std::slice::from_ref(&ask))
            .await
            .unwrap();
        assert!(store
            .dirty_assessment_threads(&account.id, None)
            .await
            .unwrap()
            .is_empty());
        store
            .pending_asks_projections(&account.id)
            .await
            .unwrap()
            .remove(0)
        // Simulated crash: no projection acknowledgement before the store closes.
    };
    let store = Store::new(&path).await.unwrap();
    assert_eq!(
        store
            .pending_asks_projections(&account.id)
            .await
            .unwrap()
            .len(),
        1
    );
    sqlx::query("UPDATE messages SET subject = 'new input' WHERE id = ?")
        .bind(message.id.as_str())
        .execute(store.writer())
        .await
        .unwrap();
    assert!(!store.acknowledge_asks_projection(&ticket).await.unwrap());
    let revisions = store
        .assessment_thread_revisions(&account.id, std::slice::from_ref(&message.thread_id))
        .await
        .unwrap();
    store
        .save_assessment_facets(&account.id, &revisions, &[], std::slice::from_ref(&ask))
        .await
        .unwrap();
    assert!(!store.acknowledge_asks_projection(&ticket).await.unwrap());
    let current = store
        .pending_asks_projections(&account.id)
        .await
        .unwrap()
        .remove(0);
    assert!(current.revision > ticket.revision);
    assert!(store.acknowledge_asks_projection(&current).await.unwrap());
    assert!(!store.acknowledge_asks_projection(&current).await.unwrap());
    store
        .save_assessment_facets(&account.id, &revisions, &[], &[ask])
        .await
        .unwrap();
    assert!(store
        .pending_asks_projections(&account.id)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn evaluation_packet_reads_generation_matched_facets_and_record_in_one_read() {
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
    let ask = answer(&account.id, &message);
    store
        .save_assessment_facets(&account.id, &revisions, &[], std::slice::from_ref(&ask))
        .await
        .unwrap();
    let before: i64 = sqlx::query_scalar("SELECT total_changes()")
        .fetch_one(store.writer())
        .await
        .unwrap();
    let packet = store
        .classify_eval_packet(std::slice::from_ref(&message.id))
        .await
        .unwrap();
    assert_eq!(packet.len(), 1);
    assert_eq!(packet[0].envelope.id, message.id);
    assert_eq!(packet[0].asks.as_ref(), Some(&ask));
    assert!(packet[0].home.is_none());
    assert!(packet[0].projection_pending);
    assert!(packet[0].projected_todos.is_empty());
    let after: i64 = sqlx::query_scalar("SELECT total_changes()")
        .fetch_one(store.writer())
        .await
        .unwrap();
    assert_eq!(
        before, after,
        "evaluation cannot repair or acknowledge anything"
    );
    sqlx::query("UPDATE messages SET subject = 'replacement' WHERE id = ?")
        .bind(message.id.as_str())
        .execute(store.writer())
        .await
        .unwrap();
    let packet = store
        .classify_eval_packet(std::slice::from_ref(&message.id))
        .await
        .unwrap();
    assert!(packet[0].asks.is_none());
    assert_eq!(packet[0].envelope.subject, "replacement");
}
