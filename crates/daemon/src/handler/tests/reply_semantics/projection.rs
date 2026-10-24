use super::*;

#[tokio::test]
async fn stale_input_is_pending_before_reconciliation_and_gets_a_new_fingerprint() {
    let fx = fixture().await;
    let mail = source(&fx, "Could you respond tomorrow?").await;
    let (before, _, errors) = observe(&fx, &mail, true).await;
    assert_eq!(errors, 0);
    body(&fx, &mail, "I will respond tomorrow.", None).await;
    let packet = fx
        .state
        .store
        .classify_eval_packet(std::slice::from_ref(&mail.id))
        .await
        .unwrap();
    assert!(packet[0].asks.is_none());
    assert!(packet[0].home.is_none());
    let (after, turn, errors) = observe(&fx, &mail, false).await;
    assert_eq!(errors, 0);
    assert!(!turn);
    assert_ne!(before.input_fingerprint, after.input_fingerprint);
}

#[tokio::test]
async fn stored_home_controls_delivery_without_changing_semantic_reply_debt() {
    let fx = fixture().await;
    let mail = source(&fx, "Could you respond tomorrow?").await;
    let (_, turn, errors) = observe(&fx, &mail, true).await;
    assert_eq!(errors, 0);
    assert!(turn);
    request(
        &fx,
        Request::SetSenderKind {
            account_id: fx.account.clone(),
            sender_email: mail.from.email.clone(),
            kind: Some(mxr_protocol::SenderKindData::PaperTrail),
        },
    )
    .await;
    let (_, turn, errors) = observe(&fx, &mail, true).await;
    assert_eq!(errors, 0);
    assert!(!turn);
    let packet = fx
        .state
        .store
        .classify_eval_packet(std::slice::from_ref(&mail.id))
        .await
        .unwrap();
    assert_eq!(packet[0].home.as_ref().unwrap().home, "updates");
}

#[tokio::test]
async fn snooze_controls_delivery_without_changing_semantic_reply_debt() {
    let fx = fixture().await;
    let mail = source(&fx, "Could you respond tomorrow?").await;
    let (_, turn, errors) = observe(&fx, &mail, true).await;
    assert_eq!(errors, 0);
    assert!(turn);
    fx.state
        .store
        .insert_snooze(&mxr_core::Snoozed {
            message_id: mail.id.clone(),
            account_id: fx.account.clone(),
            snoozed_at: now(),
            wake_at: now() + Duration::days(1),
            original_labels: vec![fx.inbox.clone()],
        })
        .await
        .unwrap();
    let (_, turn, errors) = observe(&fx, &mail, true).await;
    assert_eq!(errors, 0);
    assert!(!turn);
}
