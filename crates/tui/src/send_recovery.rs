use crate::ipc::{ipc_call, IpcRequest};
use mxr_core::{AccountId, DraftId, MxrError, SendIdentity, SendOutcomeState};
use mxr_protocol::{Request, Response, ResponseData};
use tokio::sync::mpsc;

/// A deliberate Send press may start another attempt only after durable rejection.
pub(crate) async fn reviewed_send_identity(
    bg: &mpsc::UnboundedSender<IpcRequest>,
    account_id: &AccountId,
    draft_id: &DraftId,
    identity: &SendIdentity,
) -> Result<SendIdentity, MxrError> {
    match ipc_call(
        bg,
        Request::GetSendOutcome {
            account_id: account_id.clone(),
            operation_id: identity.operation_id.clone(),
        },
    )
    .await?
    {
        Response::Ok {
            data: ResponseData::SendOutcome { outcome: None },
        } => Ok(identity.clone()),
        Response::Ok {
            data: ResponseData::SendOutcome {
                outcome: Some(prior),
            },
        } => {
            if prior.account_id != *account_id
                || prior.draft_id != *draft_id
                || prior.identity != *identity
            {
                return Err(MxrError::Ipc(
                    "Send identity conflicts with its reviewed command".into(),
                ));
            }
            match prior.state {
                SendOutcomeState::Unknown => Err(MxrError::Ipc(
                    "Acceptance uncertain; check the send outcome and do not resend".into(),
                )),
                SendOutcomeState::Accepted => Err(MxrError::Ipc(
                    "Provider already accepted this send; inspect the retained outcome".into(),
                )),
                SendOutcomeState::NotAccepted => Ok(SendIdentity {
                    operation_id: format!(
                        "{}:{}:retry:{}",
                        draft_id,
                        identity.expected_revision,
                        DraftId::new()
                    ),
                    expected_revision: identity.expected_revision,
                }),
            }
        }
        Response::Error { message, .. } => Err(MxrError::Ipc(message)),
        _ => Err(MxrError::Ipc("Unexpected send outcome response".into())),
    }
}

/// Persist the reviewed file through a conditional edit before submitting it.
pub(crate) async fn prepare_reviewed_send(
    bg: &mpsc::UnboundedSender<IpcRequest>,
    draft: mxr_core::Draft,
    identity: SendIdentity,
) -> Result<(mxr_core::Draft, SendIdentity), MxrError> {
    let reviewed = reviewed_send_identity(bg, &draft.account_id, &draft.id, &identity).await?;
    match ipc_call(
        bg,
        Request::GetDraft {
            draft_id: draft.id.clone(),
        },
    )
    .await?
    {
        Response::Ok {
            data: ResponseData::Draft { draft: stored },
        } => {
            if stored.account_id != draft.account_id {
                return Err(MxrError::Ipc(
                    "Reviewed draft belongs to another account".into(),
                ));
            }
            if stored.revision != draft.revision {
                // A previous conditional save may have committed before its reply or
                // retained identity was lost. Only the exact reviewed command can
                // recover that revision; another writer's content stays stale.
                if stored.revision > draft.revision && same_reviewed_content(&stored, &draft)? {
                    let revision = stored.revision.ok_or_else(|| {
                        MxrError::Ipc("Saved draft has no reviewed revision".into())
                    })?;
                    let mut selected = SendIdentity::for_revision(&stored.id, revision);
                    if reviewed != identity {
                        selected
                            .operation_id
                            .push_str(&format!(":retry:{}", DraftId::new()));
                    }
                    return Ok((stored, selected));
                }
                return Err(MxrError::Ipc(
                    "Reviewed draft revision is stale. Your file is preserved; compare it with the saved canonical draft before continuing. Reopening alone cannot reconcile different text.".into(),
                ));
            }
            match ipc_call(bg, Request::UpdateDraft { draft }).await? {
                Response::Ok {
                    data: ResponseData::Draft { draft },
                } => {
                    let revision = draft.revision.ok_or_else(|| {
                        MxrError::Ipc("Saved draft has no reviewed revision".into())
                    })?;
                    let mut selected = SendIdentity::for_revision(&draft.id, revision);
                    if reviewed != identity {
                        selected
                            .operation_id
                            .push_str(&format!(":retry:{}", DraftId::new()));
                    }
                    Ok((draft, selected))
                }
                Response::Error { message, .. } => Err(MxrError::Ipc(message)),
                _ => Err(MxrError::Ipc(
                    "Unexpected conditional draft save response".into(),
                )),
            }
        }
        Response::Error {
            kind: mxr_protocol::IpcErrorKind::NotFound,
            ..
        } => Ok((draft, reviewed)),
        Response::Error { message, .. } => Err(MxrError::Ipc(message)),
        _ => Err(MxrError::Ipc("Unexpected reviewed draft response".into())),
    }
}

fn same_reviewed_content(
    left: &mxr_core::Draft,
    right: &mxr_core::Draft,
) -> Result<bool, MxrError> {
    let normalize = |draft: &mxr_core::Draft| {
        let mut draft = draft.clone();
        draft.revision = None;
        draft.created_at = chrono::DateTime::UNIX_EPOCH;
        draft.updated_at = chrono::DateTime::UNIX_EPOCH;
        serde_json::to_value(draft).map_err(|error| MxrError::Ipc(error.to_string()))
    };
    Ok(normalize(left)? == normalize(right)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::DraftSendOutcome;

    #[tokio::test]
    async fn send_retry_preserves_review_and_requires_proven_non_acceptance() {
        let account = AccountId::new();
        let draft = DraftId::new();
        let identity = SendIdentity::for_revision(&draft, 1);
        for state in [
            SendOutcomeState::NotAccepted,
            SendOutcomeState::Unknown,
            SendOutcomeState::Accepted,
        ] {
            let (tx, mut rx) = mpsc::unbounded_channel::<IpcRequest>();
            let reply = DraftSendOutcome {
                identity: identity.clone(),
                draft_id: draft.clone(),
                account_id: account.clone(),
                state: state.clone(),
                local_message_id: None,
                provider_message_id: None,
                rfc2822_message_id: None,
            };
            let server = tokio::spawn(async move {
                let request = rx.recv().await.unwrap();
                assert!(matches!(request.request, Request::GetSendOutcome { .. }));
                request
                    .reply
                    .send(Ok(Response::Ok {
                        data: ResponseData::SendOutcome {
                            outcome: Some(reply),
                        },
                    }))
                    .unwrap();
                assert!(rx.try_recv().is_err());
            });
            let result = reviewed_send_identity(&tx, &account, &draft, &identity).await;
            match state {
                SendOutcomeState::NotAccepted => {
                    let retry = result.unwrap();
                    assert_ne!(retry.operation_id, identity.operation_id);
                    assert_eq!(retry.expected_revision, identity.expected_revision);
                }
                SendOutcomeState::Unknown => assert!(result.is_err()),
                SendOutcomeState::Accepted => assert!(result.is_err()),
            }
            server.await.unwrap();
        }
    }
    #[tokio::test]
    async fn send_corrected_content_updates_only_the_reviewed_revision() {
        let draft = reviewed_fixture();
        let identity = SendIdentity::for_draft(&draft);
        for (stored_revision, matching_content) in [(1, false), (2, false), (2, true)] {
            let (tx, mut rx) = mpsc::unbounded_channel::<IpcRequest>();
            let mut stored = draft.clone();
            if !matching_content {
                stored.subject = "Rejected subject".into();
            }
            stored.revision = Some(stored_revision);
            let proof = DraftSendOutcome {
                identity: identity.clone(),
                draft_id: draft.id.clone(),
                account_id: draft.account_id.clone(),
                state: SendOutcomeState::NotAccepted,
                local_message_id: None,
                provider_message_id: None,
                rfc2822_message_id: None,
            };
            let server = tokio::spawn(async move {
                let query = rx.recv().await.unwrap();
                assert!(matches!(query.request, Request::GetSendOutcome { .. }));
                query
                    .reply
                    .send(Ok(Response::Ok {
                        data: ResponseData::SendOutcome {
                            outcome: Some(proof),
                        },
                    }))
                    .unwrap();
                let get = rx.recv().await.unwrap();
                assert!(matches!(get.request, Request::GetDraft { .. }));
                get.reply
                    .send(Ok(Response::Ok {
                        data: ResponseData::Draft { draft: stored },
                    }))
                    .unwrap();
                if stored_revision == 1 {
                    let update = rx.recv().await.unwrap();
                    let Request::UpdateDraft { mut draft } = update.request else {
                        panic!("expected conditional update")
                    };
                    assert_eq!(draft.subject, "Corrected subject");
                    assert_eq!(draft.revision, Some(1));
                    draft.revision = Some(2);
                    update
                        .reply
                        .send(Ok(Response::Ok {
                            data: ResponseData::Draft { draft },
                        }))
                        .unwrap();
                }
                assert!(rx.try_recv().is_err());
            });
            let result = prepare_reviewed_send(&tx, draft.clone(), identity.clone()).await;
            if stored_revision == 1 || matching_content {
                let (saved, operation) = result.unwrap();
                assert_eq!(saved.id, draft.id);
                assert_eq!(saved.subject, draft.subject);
                assert_eq!(saved.revision, Some(2));
                assert_eq!(operation.expected_revision, 2);
                assert_ne!(operation.operation_id, identity.operation_id);
            } else {
                assert!(result.is_err());
            }
            server.await.unwrap();
        }
    }
    fn reviewed_fixture() -> mxr_core::Draft {
        let now = chrono::Utc::now();
        mxr_core::Draft {
            revision: Some(1),
            id: DraftId::new(),
            account_id: AccountId::new(),
            from: None,
            reply_headers: None,
            intent: mxr_core::DraftIntent::New,
            to: vec![],
            cc: vec![],
            bcc: vec![],
            subject: "Corrected subject".into(),
            content: mxr_core::DraftContent::markdown("reviewed correction"),
            attachments: vec![],
            inline_assets: vec![],
            inline_calendar_reply: None,
            created_at: now,
            updated_at: now,
        }
    }

    #[tokio::test]
    async fn send_recovers_its_committed_edit_after_update_reply_loss() {
        let draft = reviewed_fixture();
        let identity = SendIdentity::for_draft(&draft);
        let (tx, mut rx) = mpsc::unbounded_channel::<IpcRequest>();
        let stored = draft.clone();
        let server = tokio::spawn(async move {
            let mut stored = stored;
            for attempt in 0..2 {
                let outcome = rx.recv().await.unwrap();
                assert!(matches!(outcome.request, Request::GetSendOutcome { .. }));
                outcome
                    .reply
                    .send(Ok(Response::Ok {
                        data: ResponseData::SendOutcome { outcome: None },
                    }))
                    .unwrap();
                let get = rx.recv().await.unwrap();
                assert!(matches!(get.request, Request::GetDraft { .. }));
                get.reply
                    .send(Ok(Response::Ok {
                        data: ResponseData::Draft {
                            draft: stored.clone(),
                        },
                    }))
                    .unwrap();
                if attempt == 0 {
                    let update = rx.recv().await.unwrap();
                    let Request::UpdateDraft { mut draft } = update.request else {
                        panic!("expected reviewed edit")
                    };
                    draft.revision = Some(2);
                    stored = draft;
                    // Commit happened; only the reply was lost.
                    update
                        .reply
                        .send(Err(MxrError::Ipc("connection closed after update".into())))
                        .unwrap();
                }
            }
            assert!(
                rx.try_recv().is_err(),
                "recovery must not edit or submit again"
            );
        });
        assert!(prepare_reviewed_send(&tx, draft.clone(), identity.clone())
            .await
            .is_err());
        let (recovered, selected) = prepare_reviewed_send(&tx, draft.clone(), identity)
            .await
            .unwrap();
        assert_eq!(recovered.id, draft.id);
        assert_eq!(recovered.subject, draft.subject);
        assert_eq!(recovered.revision, Some(2));
        assert_eq!(selected.expected_revision, 2);
        server.await.unwrap();
    }
}
