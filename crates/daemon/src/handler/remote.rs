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
        assert!(json.contains("hosting_available\":false"));
        assert!(!json.contains("secret"));
        assert!(!json.contains("verifier"));
    }
}
