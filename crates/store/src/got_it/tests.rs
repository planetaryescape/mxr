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
