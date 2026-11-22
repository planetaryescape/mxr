use super::desk::{Fixture, ME};
use super::promise_projection::{accept, chosen, settle, source, QUOTE};
use mxr_core::MessageDirection;

#[tokio::test]
async fn review2_owned_first_recipient_keeps_one_promise_and_task() {
    let fx = Fixture::new().await;
    let mut mail = source(&fx, QUOTE).await;
    mail.to.insert(
        0,
        mxr_core::Address {
            name: None,
            email: ME.into(),
        },
    );
    fx.store_envelope(&mail, MessageDirection::Outbound).await;
    settle(&fx, &mail).await;
    let original = fx
        .state
        .store
        .list_promise_todos(&fx.account)
        .await
        .unwrap()
        .remove(0);
    let id = accept(&fx, &mail).await;
    let rows = fx
        .state
        .store
        .list_contact_commitments(&fx.account, None, None)
        .await
        .unwrap();
    eprintln!("REVIEW2_RECIPIENT {rows:?}");
    assert_eq!(rows.len(), 1);
    assert_eq!(original.commitment_id.as_deref(), Some(id.as_str()));
    settle(&fx, &mail).await;
    let tasks = fx
        .state
        .store
        .list_promise_todos(&fx.account)
        .await
        .unwrap();
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id, original.id);
    assert_eq!(tasks[0].due_at, Some(chosen()));
}
