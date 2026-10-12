use super::*;
use mxr_core::{Draft, DraftSendOutcome, SendIdentity, SendOutcomeState};

async fn fixture() -> (Arc<AppState>, Arc<mxr_provider_fake::FakeProvider>, Draft) {
    fixture_with_sender(None).await
}
async fn fixture_with_sender(
    sender: Option<Arc<dyn mxr_core::MailSendProvider>>,
) -> (Arc<AppState>, Arc<mxr_provider_fake::FakeProvider>, Draft) {
    let account_id = mxr_core::AccountId::new();
    let account = crate::test_fixtures::test_account_with_id(account_id.clone());
    let fake = Arc::new(mxr_provider_fake::FakeProvider::new(account_id.clone()));
    let state = Arc::new(
        AppState::in_memory_with_sync_provider(
            account,
            fake.clone(),
            Some(sender.unwrap_or_else(|| fake.clone())),
        )
        .await
        .unwrap(),
    );
    let now = chrono::Utc::now();
    let draft = Draft {
        revision: Some(1),
        id: mxr_core::DraftId::new(),
        account_id,
        from: None,
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::Address {
            name: None,
            email: "recipient@example.com".into(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Reviewed send".into(),
        content: mxr_core::DraftContent::markdown("Reviewed text"),
        managed_attachments: None,
        attachments: vec![],
        inline_assets: vec![],
        inline_calendar_reply: None,
        created_at: now,
        updated_at: now,
    };
    state.store.insert_draft(&draft).await.unwrap();
    (state, fake, draft)
}
async fn call(state: &Arc<AppState>, request: Request) -> Response {
    match handle_request(
        state,
        &IpcMessage {
            id: 1,
            source: ClientKind::Cli,
            payload: IpcPayload::Request(request),
        },
    )
    .await
    .payload
    {
        IpcPayload::Response(response) => response,
        other => panic!("unexpected {other:?}"),
    }
}
fn send(draft: &Draft, identity: SendIdentity) -> Request {
    Request::SendStoredDraft {
        managed_attachments_reviewed: true,
        send_identity: Some(identity),
        draft_id: draft.id.clone(),
        override_safety_token: None,
    }
}
async fn outcome(
    state: &Arc<AppState>,
    draft: &Draft,
    identity: &SendIdentity,
) -> DraftSendOutcome {
    match call(
        state,
        Request::GetSendOutcome {
            account_id: draft.account_id.clone(),
            operation_id: identity.operation_id.clone(),
        },
    )
    .await
    {
        Response::Ok {
            data: ResponseData::SendOutcome {
                outcome: Some(outcome),
            },
        } => outcome,
        other => panic!("expected durable outcome, got {other:?}"),
    }
}
fn error_contains(response: Response, expected: &str) {
    match response {
        Response::Error { message, .. } => assert!(message.contains(expected), "{message}"),
        other => panic!("expected error, got {other:?}"),
    }
}

#[tokio::test]
async fn send_outcomes_concurrent_replay_survives_cleanup_and_conflicting_command_refuses() {
    let (state, fake, draft) = fixture().await;
    let identity = SendIdentity::for_draft(&draft);
    let (first, second) = tokio::join!(
        call(&state, send(&draft, identity.clone())),
        call(&state, send(&draft, identity.clone()))
    );
    assert!(matches!(
        first,
        Response::Ok {
            data: ResponseData::SendReceipt { .. }
        }
    ));
    assert_eq!(
        serde_json::to_value(first).unwrap(),
        serde_json::to_value(second).unwrap()
    );
    assert_eq!(fake.sent_drafts().len(), 1);
    assert!(state.store.get_draft(&draft.id).await.unwrap().is_none());
    assert_eq!(
        outcome(&state, &draft, &identity).await.state,
        SendOutcomeState::Accepted
    );
    let mut changed = draft.clone();
    changed.subject = "different payload".into();
    error_contains(
        call(
            &state,
            Request::SendDraft {
                send_identity: Some(identity.clone()),
                draft: changed,
                override_safety_token: None,
            },
        )
        .await,
        "conflicts",
    );
    error_contains(
        call(
            &state,
            Request::GetSendOutcome {
                account_id: mxr_core::AccountId::new(),
                operation_id: identity.operation_id,
            },
        )
        .await,
        "account",
    );
    assert_eq!(fake.sent_drafts().len(), 1);
}

#[tokio::test]
async fn send_outcomes_accepted_then_error_blocks_new_identity_reset_and_schedule() {
    let (state, fake, draft) = fixture().await;
    fake.fail_sends_after_accepting(true);
    let identity = SendIdentity::for_draft(&draft);
    error_contains(
        call(&state, send(&draft, identity.clone())).await,
        "uncertain",
    );
    assert_eq!(
        outcome(&state, &draft, &identity).await.state,
        SendOutcomeState::Unknown
    );
    error_contains(
        call(
            &state,
            Request::ResetOrphanedDraft {
                draft_id: draft.id.clone(),
            },
        )
        .await,
        "uncertain",
    );
    let new_identity = SendIdentity {
        operation_id: "new-operation".into(),
        expected_revision: 1,
    };
    error_contains(call(&state, send(&draft, new_identity)).await, "blocked");
    state
        .store
        .schedule_send(&draft.id, chrono::Utc::now() - chrono::Duration::seconds(1))
        .await
        .unwrap();
    crate::loops::process_due_scheduled_sends(&state, chrono::Utc::now())
        .await
        .unwrap();
    assert_eq!(fake.sent_drafts().len(), 1);
    assert_eq!(
        state.store.get_draft_status(&draft.id).await.unwrap(),
        Some(mxr_core::DraftStatus::Sending)
    );
}

#[tokio::test]
async fn send_outcomes_stale_revision_and_missing_client_identity_have_no_provider_effect() {
    let (state, fake, mut draft) = fixture().await;
    let identity = SendIdentity::for_draft(&draft);
    draft.subject = "new revision".into();
    state.store.update_draft(&draft).await.unwrap();
    error_contains(call(&state, send(&draft, identity)).await, "stale");
    error_contains(
        call(
            &state,
            Request::SendStoredDraft {
                managed_attachments_reviewed: true,
                send_identity: None,
                draft_id: draft.id.clone(),
                override_safety_token: None,
            },
        )
        .await,
        "identity",
    );
    for revision in [None, Some(0)] {
        assert!(matches!(
            call(
                &state,
                Request::ScheduleSend {
                    managed_attachments_reviewed: true,
                    expected_revision: revision,
                    draft_id: draft.id.clone(),
                    send_at: chrono::Utc::now(),
                }
            )
            .await,
            Response::Error { .. }
        ));
    }
    assert!(state
        .store
        .list_pending_scheduled_sends(None)
        .await
        .unwrap()
        .is_empty());
    assert!(fake.sent_drafts().is_empty());
}

#[tokio::test]
async fn send_outcomes_receipt_failure_keeps_uncertainty_and_claim_failure_rolls_back() {
    let (state, fake, draft) = fixture().await;
    let identity = SendIdentity::for_draft(&draft);
    sqlx::query("CREATE TRIGGER fail_claim BEFORE INSERT ON draft_send_outcomes BEGIN SELECT RAISE(ABORT, 'claim failure'); END").execute(state.store.writer()).await.unwrap();
    assert!(matches!(
        call(&state, send(&draft, identity.clone())).await,
        Response::Error { .. }
    ));
    assert!(fake.sent_drafts().is_empty());
    assert_eq!(
        state.store.get_draft_status(&draft.id).await.unwrap(),
        Some(mxr_core::DraftStatus::Draft)
    );
    sqlx::query("DROP TRIGGER fail_claim")
        .execute(state.store.writer())
        .await
        .unwrap();
    sqlx::query("CREATE TRIGGER fail_accept BEFORE UPDATE ON draft_send_outcomes WHEN NEW.state = 'accepted' BEGIN SELECT RAISE(ABORT, 'receipt failure'); END").execute(state.store.writer()).await.unwrap();
    error_contains(
        call(&state, send(&draft, identity.clone())).await,
        "uncertain",
    );
    assert_eq!(
        outcome(&state, &draft, &identity).await.state,
        SendOutcomeState::Unknown
    );
    error_contains(call(&state, send(&draft, identity)).await, "uncertain");
    assert_eq!(fake.sent_drafts().len(), 1);
}

#[tokio::test]
async fn send_outcomes_local_ingest_failure_does_not_change_provider_acceptance() {
    let (state, fake, draft) = fixture().await;
    let identity = SendIdentity::for_draft(&draft);
    sqlx::query("CREATE TRIGGER fail_ingest BEFORE INSERT ON messages BEGIN SELECT RAISE(ABORT, 'ingest failure'); END").execute(state.store.writer()).await.unwrap();
    assert!(matches!(
        call(&state, send(&draft, identity.clone())).await,
        Response::Ok {
            data: ResponseData::SendOutcome { .. }
        }
    ));
    let accepted = outcome(&state, &draft, &identity).await;
    assert_eq!(accepted.state, SendOutcomeState::Accepted);
    assert!(accepted.local_message_id.is_none());
    assert!(matches!(
        call(&state, send(&draft, identity)).await,
        Response::Ok {
            data: ResponseData::SendOutcome { .. }
        }
    ));
    assert_eq!(fake.sent_drafts().len(), 1);
}

struct RejectedSender;
#[async_trait]
impl mxr_core::MailSendProvider for RejectedSender {
    fn name(&self) -> &str {
        "rejected"
    }
    async fn send(
        &self,
        _draft: &Draft,
        _from: &mxr_core::Address,
        _message_id: &str,
    ) -> Result<mxr_core::SendReceipt, mxr_core::MxrError> {
        unreachable!()
    }
    async fn send_classified(
        &self,
        _draft: &Draft,
        _from: &mxr_core::Address,
        _message_id: &str,
    ) -> Result<mxr_core::SendReceipt, mxr_core::SendFailure> {
        Err(mxr_core::SendFailure::NotAccepted(
            mxr_core::MxrError::Provider("controlled pre-submission rejection".into()),
        ))
    }
}
#[tokio::test]
async fn send_outcomes_safe_retry_requires_explicit_non_acceptance_and_a_new_reviewed_operation() {
    let (state, _, draft) = fixture_with_sender(Some(Arc::new(RejectedSender))).await;
    let identity = SendIdentity::for_draft(&draft);
    error_contains(
        call(&state, send(&draft, identity.clone())).await,
        "did not accept",
    );
    assert_eq!(
        outcome(&state, &draft, &identity).await.state,
        SendOutcomeState::NotAccepted
    );
    assert_eq!(
        state.store.get_draft_status(&draft.id).await.unwrap(),
        Some(mxr_core::DraftStatus::Draft)
    );
    error_contains(
        call(&state, send(&draft, identity)).await,
        "new send operation",
    );
    let new_identity = SendIdentity {
        operation_id: "reviewed-retry".into(),
        expected_revision: 1,
    };
    error_contains(
        call(&state, send(&draft, new_identity.clone())).await,
        "did not accept",
    );
    assert_eq!(
        outcome(&state, &draft, &new_identity).await.state,
        SendOutcomeState::NotAccepted
    );
}
#[tokio::test]
async fn send_outcomes_scheduled_revision_cannot_silently_follow_a_later_edit() {
    let (state, fake, mut draft) = fixture().await;
    state
        .store
        .schedule_send(&draft.id, chrono::Utc::now() - chrono::Duration::seconds(1))
        .await
        .unwrap();
    draft.subject = "unreviewed edit".into();
    state.store.update_draft(&draft).await.unwrap();
    crate::loops::process_due_scheduled_sends(&state, chrono::Utc::now())
        .await
        .unwrap();
    assert!(fake.sent_drafts().is_empty());
}
