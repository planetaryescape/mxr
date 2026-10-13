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
pub(super) async fn read(
    state: &std::sync::Arc<AppState>,
    client_id: &str,
    mail_secret: &mxr_core::RemoteSecret,
    request: &mxr_protocol::Request,
) -> HandlerResult {
    use mxr_protocol::{Request, Response};
    if !mxr_protocol::remote_read_allowed(request) {
        return Err(super::HandlerError::RemoteAuthorityDenied(
            "Unsupported remote operation; R1 supports mailbox reading and search".into(),
        ));
    }
    state
        .store
        .authenticate_remote_client(client_id, mail_secret, chrono::Utc::now().timestamp())
        .await
        .map_err(authority_error)?;
    if matches!(request, Request::GetRemoteReady) {
        let authority = state
            .store
            .remote_authority_status()
            .await
            .map_err(authority_error)?;
        let (workspace_id, recovery_generation) = state.store.archive_workspace().await?;
        let host_id = authority.host_id.ok_or_else(|| {
            super::HandlerError::RemoteAuthorityDenied("Host authority unavailable".into())
        })?;
        return Ok(ResponseData::RemoteReady {
            protocol_version: mxr_protocol::IPC_PROTOCOL_VERSION,
            host_id,
            workspace: mxr_protocol::ArchiveWorkspaceData {
                workspace_id,
                recovery_generation,
            },
            client_id: client_id.into(),
        });
    }
    match Box::pin(super::dispatch(
        state,
        mxr_protocol::ClientKind::Cli,
        &PeerInfo {
            auth: PeerAuth::TokenRequired,
        },
        request,
    ))
    .await
    {
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
            Ok(data)
        }
        Response::Error { .. } => Err(super::HandlerError::RemoteAuthorityDenied(
            "Remote mailbox read failed".into(),
        )),
    }
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
