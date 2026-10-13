//! Local pairing administration and proof checks. Authentication never upgrades IPC trust.
use super::{HandlerResult, ResponseData};
use crate::state::AppState;
use mxr_protocol::{RemoteRequest, RemoteResponse};
use mxr_transport::{PeerAuth, PeerInfo};

fn authority_error(error: sqlx::Error) -> super::HandlerError {
    match error {
        sqlx::Error::Protocol(message) if message.starts_with("Remote authority denied") => {
            super::HandlerError::RemoteAuthorityDenied(message)
        }
        other => other.into(),
    }
}

pub(super) async fn handle(
    state: &AppState,
    peer: &PeerInfo,
    request: &RemoteRequest,
) -> HandlerResult {
    if request.is_local_admin()
        && !matches!(
            peer.auth,
            PeerAuth::UnixPeer { .. } | PeerAuth::LocalProcess
        )
    {
        return Err(super::HandlerError::RemoteAuthorityDenied(
            "Remote hosting administration requires the explicit local Unix socket".into(),
        ));
    }
    let now = chrono::Utc::now().timestamp();
    let response = match request {
        RemoteRequest::Status => RemoteResponse::Status {
            authority: state
                .store
                .remote_authority_status()
                .await
                .map_err(authority_error)?,
        },
        RemoteRequest::Pairings => RemoteResponse::Pairings {
            pairings: state
                .store
                .remote_pairings(now)
                .await
                .map_err(authority_error)?,
        },
        RemoteRequest::Clients => RemoteResponse::Clients {
            clients: state
                .store
                .remote_clients()
                .await
                .map_err(authority_error)?,
        },
        RemoteRequest::StartPairing {
            name,
            capabilities,
            mail_verifier,
            cleanup_verifier,
        } => RemoteResponse::Pairing {
            pairing: state
                .store
                .start_remote_pairing(name, capabilities, mail_verifier, cleanup_verifier, now)
                .await
                .map_err(authority_error)?,
            dry_run: false,
        },
        RemoteRequest::ApprovePairing {
            request_id,
            dry_run,
        } => RemoteResponse::Pairing {
            pairing: state
                .store
                .approve_remote_pairing(request_id, *dry_run, now)
                .await
                .map_err(authority_error)?,
            dry_run: *dry_run,
        },
        RemoteRequest::ClaimPairing {
            request_id,
            mail_secret,
            cleanup_secret,
        } => RemoteResponse::Client {
            client: state
                .store
                .claim_remote_pairing(request_id, mail_secret, cleanup_secret, now)
                .await
                .map_err(authority_error)?,
            dry_run: false,
        },
        RemoteRequest::Authenticate {
            client_id,
            mail_secret,
        } => RemoteResponse::Client {
            client: state
                .store
                .authenticate_remote_client(client_id, mail_secret, now)
                .await
                .map_err(authority_error)?,
            dry_run: false,
        },
        RemoteRequest::RevokeClient { client_id, dry_run } => RemoteResponse::Client {
            client: state
                .store
                .revoke_remote_client(client_id, *dry_run, now)
                .await
                .map_err(authority_error)?,
            dry_run: *dry_run,
        },
        RemoteRequest::AbandonPairing { cleanup_secret } => {
            state
                .store
                .abandon_remote_pairing(cleanup_secret, now)
                .await
                .map_err(authority_error)?;
            RemoteResponse::Cleaned {
                client_id: String::new(),
            }
        }
        RemoteRequest::Cleanup {
            client_id,
            cleanup_secret,
        } => {
            state
                .store
                .cleanup_remote_client(client_id, cleanup_secret, now)
                .await
                .map_err(authority_error)?;
            RemoteResponse::Cleaned {
                client_id: client_id.clone(),
            }
        }
    };
    Ok(ResponseData::RemoteAuthority { response })
}

/// The proof is checked at each admission on the writer, never cached on a socket.
#[cfg(test)]
pub(super) async fn read(
    state: &std::sync::Arc<AppState>,
    client_id: &str,
    mail_secret: &mxr_core::RemoteSecret,
    request: &mxr_protocol::Request,
) -> HandlerResult {
    read_owned(state, client_id, mail_secret, request, None).await
}

pub(super) async fn read_owned(
    state: &std::sync::Arc<AppState>,
    client_id: &str,
    mail_secret: &mxr_core::RemoteSecret,
    request: &mxr_protocol::Request,
    owner: Option<&mxr_protocol::ArchiveWorkspaceData>,
) -> HandlerResult {
    use mxr_protocol::{Request, Response};
    let client = state
        .store
        .authenticate_remote_client(client_id, mail_secret, chrono::Utc::now().timestamp())
        .await
        .map_err(authority_error)?;
    if !mxr_protocol::remote_read_allowed(request)
        && !(client
            .capabilities
            .contains(&mxr_core::RemoteCapability::MailSend)
            && mxr_protocol::remote_send_allowed(request))
    {
        return Err(super::HandlerError::RemoteAuthorityDenied("Remote operation requires explicit host-approved send access or is unsupported; use mxr connect --allow-send and host approval".into()));
    }
    // Validate the original envelope owner after authentication, never infer it from the selected host.
    super::workspace_owner::validate(state, request, owner).await.map_err(|response| {
        if matches!(&*response, Response::Error { code, .. } if code == "workspace_owner_unavailable") {
            sanitize_error(*response)
        } else { super::HandlerError::RemoteResponse(response) }
    })?;
    if matches!(
        request,
        Request::GetRemoteReady | Request::GetArchiveWorkspace
    ) {
        let authority = state
            .store
            .remote_authority_status()
            .await
            .map_err(authority_error)?;
        let (workspace_id, recovery_generation) = state.store.archive_workspace().await?;
        let workspace = mxr_protocol::ArchiveWorkspaceData {
            workspace_id,
            recovery_generation,
        };
        if matches!(request, Request::GetArchiveWorkspace) {
            return Ok(ResponseData::ArchiveWorkspace { workspace });
        }
        return Ok(ResponseData::RemoteReady {
            attachment_transfer_supported: true,
            capabilities: client.capabilities,
            draft_attachment_upload_supported: true,
            draft_revision_supported: true,
            send_outcome_supported: true,
            protocol_version: mxr_protocol::IPC_PROTOCOL_VERSION,
            host_id: authority.host_id.ok_or_else(|| {
                super::HandlerError::RemoteAuthorityDenied("Host authority unavailable".into())
            })?,
            workspace,
            client_id: client_id.into(),
        });
    }
    match request {
        Request::SaveDraft { draft }
        | Request::UpdateDraft { draft }
        | Request::SendDraft { draft, .. }
        | Request::CheckDraftSafety { draft, .. }
        | Request::PreviewDraft { draft, .. } => super::draft_preview::require_portable(draft)?,
        _ => (),
    }
    if let Err(message) =
        super::enforce_safety_policy(state.config_snapshot().general.safety_policy, request)
    {
        return Err(super::HandlerError::RemoteResponse(Box::new(
            Response::error(message),
        )));
    }
    if let Request::CheckDraftSafety { draft, context } = request {
        let ResponseData::DraftPreview { report, .. } =
            sanitize_result(super::draft_preview::preview(state, draft, context, &[]).await)?
        else {
            return Err("Unexpected draft preview".into());
        };
        return Ok(ResponseData::DraftSafetyReportResponse { report });
    }
    if let Request::SendDraft {
        draft,
        send_identity,
        override_safety_token,
    } = request
    {
        return sanitize_result(
            super::mutations::send_draft_remote(
                state,
                draft,
                send_identity.as_ref(),
                override_safety_token.as_deref(),
            )
            .await,
        );
    }
    if let Request::SendStoredDraft {
        draft_id,
        send_identity,
        override_safety_token,
        managed_attachments_reviewed,
    } = request
    {
        if !managed_attachments_reviewed {
            return Err(super::HandlerError::InvalidRequest(
                "Review portable attachments before sending".into(),
            ));
        }
        let result = super::mutations::send_stored_draft_remote(
            state,
            draft_id,
            send_identity.as_ref(),
            override_safety_token.as_deref(),
        )
        .await;
        return sanitize_result(result);
    }
    let response = Box::pin(super::dispatch(
        state,
        mxr_protocol::ClientKind::Cli,
        &PeerInfo {
            auth: PeerAuth::TokenRequired,
        },
        request,
    ))
    .await;
    match response {
        Response::Ok { mut data } => {
            if let ResponseData::Bodies { failures, .. } = &mut data {
                for failure in failures {
                    failure.error = "Body unavailable on the remote host".into();
                }
            }
            let bodies: &mut [mxr_core::MessageBody] = match &mut data {
                ResponseData::Body { body } => std::slice::from_mut(body),
                ResponseData::Bodies { bodies, .. } => bodies,
                _ => &mut [],
            };
            for body in bodies {
                for attachment in &mut body.attachments {
                    attachment.local_path = None;
                }
            }
            match &data {
                ResponseData::Draft { draft } => super::draft_preview::require_portable(draft)?,
                ResponseData::Drafts { drafts } => {
                    for draft in drafts {
                        super::draft_preview::require_portable(draft)?;
                    }
                }
                _ => (),
            }
            Ok(data)
        }
        error => Err(sanitize_error(error)),
    }
}

fn sanitize_result(result: HandlerResult) -> HandlerResult {
    result.map_err(|error| sanitize_error(error.into_response()))
}
fn sanitize_error(response: mxr_protocol::Response) -> super::HandlerError {
    use mxr_protocol::{IpcErrorKind, Response};
    if let Response::Error { code, kind, .. } = &response {
        if matches!(
            code.as_str(),
            "remote_path_backed_draft"
                | "draft_revision_conflict"
                | "send_outcome_unknown"
                | "send_not_accepted"
        ) {
            return super::HandlerError::RemoteResponse(Box::new(response));
        }
        if *kind == IpcErrorKind::NotFound {
            return super::HandlerError::RemoteResponse(Box::new(Response::error_kinded(
                "Requested remote mail or draft was not found",
                IpcErrorKind::NotFound,
            )));
        }
    }
    super::HandlerError::RemoteResponse(Box::new(Response::error_kinded("Remote operation failed; retain the original workspace and operation identity, query its outcome before retrying a send, and inspect host diagnostics", IpcErrorKind::InvalidRequest)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn token_peer_cannot_inspect_or_mutate_local_authority() {
        let state = AppState::in_memory().await.unwrap();
        let peer = PeerInfo {
            auth: PeerAuth::TokenRequired,
        };
        for request in [
            RemoteRequest::Status,
            RemoteRequest::Pairings,
            RemoteRequest::Clients,
            RemoteRequest::ApprovePairing {
                request_id: "missing".into(),
                dry_run: true,
            },
            RemoteRequest::RevokeClient {
                client_id: "missing".into(),
                dry_run: false,
            },
        ] {
            assert!(handle(&state, &peer, &request).await.is_err());
        }
        assert!(state
            .store
            .remote_authority_status()
            .await
            .unwrap()
            .host_id
            .is_none());
    }
    #[tokio::test]
    async fn inspection_has_no_credential_or_activity_fields() {
        let state = AppState::in_memory().await.unwrap();
        let response = handle(&state, &PeerInfo::local(), &RemoteRequest::Status)
            .await
            .unwrap();
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("hosting_available\":true"));
        assert!(!json.contains("secret"));
        assert!(!json.contains("verifier"));
    }
}
