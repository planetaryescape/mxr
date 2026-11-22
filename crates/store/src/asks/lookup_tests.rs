//! Account boundaries and batch semantics of the asks lookups.
use super::tests::answer;
use crate::{
    test_fixtures::{test_account, TestEnvelopeBuilder},
    Store,
};

#[tokio::test]
async fn facet_summaries_keep_the_assessments_account_boundary() {
    let store = Store::in_memory().await.unwrap();
    let account = test_account();
    let other = test_account();
    for own in [&account, &other] {
        store.insert_account(own).await.unwrap();
    }
    let message = TestEnvelopeBuilder::new()
        .account_id(account.id.clone())
        .build();
    store.upsert_envelope(&message).await.unwrap();
    let revisions = store
        .assessment_thread_revisions(&account.id, std::slice::from_ref(&message.thread_id))
        .await
        .unwrap();
    store
        .save_asks_batch(&account.id, &revisions, &[answer(&account.id, &message)])
        .await
        .unwrap();
    // Even with the same message and generation, a differently owned facet
    // cannot be used. Exercise the projection's account-update trigger too.
    for (owner, expected) in [(&other.id, 0), (&account.id, 1)] {
        sqlx::query("UPDATE mail_assessments SET account_id = ? WHERE message_id = ?")
            .bind(owner.as_str())
            .bind(message.id.as_str())
            .execute(store.writer())
            .await
            .unwrap();
        let (_, summaries) = store
            .assessment_facets_for_messages(&account.id, std::slice::from_ref(&message.id))
            .await
            .unwrap();
        assert_eq!(summaries.len(), expected);
    }
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
    let (homes, summaries) = store
        .assessment_facets_for_messages(&account.id, &requested)
        .await
        .unwrap();
    assert!(homes.is_empty());
    assert_eq!(summaries.len(), 2);
    for message in [&first, &second] {
        assert_eq!(
            summaries[&message.id].input_fingerprint,
            answer(&account.id, message).input_fingerprint
        );
    }
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
    let (_, summaries) = store
        .assessment_facets_for_messages(&account.id, &requested)
        .await
        .unwrap();
    assert_eq!(summaries.len(), 1);
    assert!(summaries.contains_key(&second.id));
}
