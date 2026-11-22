use super::send_outcomes::{call, fixture, send};
use super::*;
use mxr_core::{DraftContent, SendIdentity};
use mxr_store::CommitmentStatus;

#[tokio::test]
async fn accepted_send_projection_failure_replays_without_resending() {
    let (state, fake, mut draft) = fixture().await;
    draft.content = DraftContent::markdown("I'll send the report tomorrow.");
    state.store.update_draft(&draft).await.unwrap();
    let draft = state
        .store
        .get_draft(&draft.id)
        .await
        .unwrap()
        .expect("updated revision");
    let identity = SendIdentity::for_draft(&draft);
    sqlx::query("CREATE TRIGGER reject_sent_asks BEFORE INSERT ON mail_assessments BEGIN SELECT RAISE(FAIL, 'synthetic projection failure'); END")
        .execute(state.store.writer()).await.unwrap();
    let first = call(&state, send(&draft, identity.clone())).await;
    let Response::Ok {
        data: ResponseData::SendReceipt {
            local_message_id, ..
        },
    } = &first
    else {
        panic!("accepted send remains accepted: {first:?}");
    };
    assert_eq!(
        super::send_outcomes::outcome(&state, &draft, &identity)
            .await
            .state,
        mxr_core::SendOutcomeState::Accepted
    );
    let replay = call(&state, send(&draft, identity)).await;
    assert_eq!(
        serde_json::to_value(&first).unwrap(),
        serde_json::to_value(replay).unwrap()
    );
    assert_eq!(fake.sent_drafts().len(), 1);
    assert!(state
        .store
        .asks_for_messages(&draft.account_id, std::slice::from_ref(local_message_id))
        .await
        .unwrap()
        .is_empty());
    let dirty: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM assessment_thread_inputs WHERE account_id=? AND dirty=1",
    )
    .bind(draft.account_id.as_str())
    .fetch_one(state.store.reader())
    .await
    .unwrap();
    assert_eq!(dirty, 1, "failure retains durable reconciliation work");
    sqlx::query("DROP TRIGGER reject_sent_asks")
        .execute(state.store.writer())
        .await
        .unwrap();
    super::super::assessments::reconcile_account(&state, &draft.account_id)
        .await
        .unwrap();
    let open = state
        .store
        .list_contact_commitments(
            &draft.account_id,
            Some("recipient@example.com"),
            Some(CommitmentStatus::Open),
        )
        .await
        .unwrap();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].evidence_msg_id, *local_message_id);
    assert_eq!(open[0].direction, mxr_store::CommitmentDirection::Yours);
}
