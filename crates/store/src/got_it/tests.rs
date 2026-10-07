use super::*;
use crate::test_fixtures::{test_account, TestEnvelopeBuilder};
use crate::Store;
use mxr_core::id::{DraftId, MessageId, ThreadId};
use mxr_core::types::MessageDirection;

async fn fixture() -> (Store, AccountId, ThreadId, MessageId) {
    let store = Store::in_memory().await.unwrap();
    let account = test_account();
    store.insert_account(&account).await.unwrap();
    let mut envelope = TestEnvelopeBuilder::new()
        .account_id(account.id.clone())
        .build();
    envelope.provider_id = "got-it-target".into();
    store
        .upsert_envelope_with_direction(&envelope, MessageDirection::Inbound)
        .await
        .unwrap();
    (store, account.id, envelope.thread_id, envelope.id)
}

#[tokio::test]
async fn the_first_claim_wins_and_later_ones_see_it() {
    let (store, account, thread, target) = fixture().await;
    let first = DraftId::new();
    let now = Utc::now();
    assert_eq!(
        store
            .claim_got_it(&account, &thread, &target, &first, now)
            .await
            .unwrap(),
        GotItClaim::Claimed
    );
    let second = DraftId::new();
    let again = store
        .claim_got_it(&account, &thread, &target, &second, now)
        .await
        .unwrap();
    assert_eq!(
        again,
        GotItClaim::Existing(GotItRecord {
            draft_id: first.clone(),
            sent_message_id: None,
            claimed_at: now.timestamp(),
        })
    );
    let sent = MessageId::new();
    store.finish_got_it(&target, &sent).await.unwrap();
    let record = store.got_it_for(&target).await.unwrap().unwrap();
    assert_eq!(record.draft_id, first);
    assert_eq!(record.sent_message_id, Some(sent));
}

#[tokio::test]
async fn a_claim_survives_until_its_message_is_deleted() {
    let (store, account, thread, target) = fixture().await;
    store
        .claim_got_it(&account, &thread, &target, &DraftId::new(), Utc::now())
        .await
        .unwrap();
    sqlx::query("DELETE FROM messages WHERE id = ?1")
        .bind(target.as_str())
        .execute(store.writer())
        .await
        .unwrap();
    assert!(store.got_it_for(&target).await.unwrap().is_none());
}
