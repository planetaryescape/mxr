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
                SendOutcomeState::Accepted => Ok(identity.clone()),
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
                state,
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
                SendOutcomeState::Accepted => assert_eq!(result.unwrap(), identity),
            }
            server.await.unwrap();
        }
    }
}
