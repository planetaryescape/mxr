//! Pairing authority foundation. These operations do not expose remote mail.
use mxr_core::{HostAuthority, PendingPairing, RegisteredClient, RemoteCapability, RemoteSecret};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "action", deny_unknown_fields)]
pub enum RemoteRequest {
    Status,
    Pairings,
    Clients,
    StartPairing {
        name: String,
        capabilities: Vec<RemoteCapability>,
        mail_verifier: RemoteSecret,
        cleanup_verifier: RemoteSecret,
    },
    ApprovePairing {
        request_id: String,
        dry_run: bool,
    },
    ClaimPairing {
        request_id: String,
        mail_secret: RemoteSecret,
        cleanup_secret: RemoteSecret,
    },
    Authenticate {
        client_id: String,
        mail_secret: RemoteSecret,
    },
    RevokeClient {
        client_id: String,
        dry_run: bool,
    },
    AbandonPairing {
        cleanup_secret: RemoteSecret,
    },
    Cleanup {
        client_id: String,
        cleanup_secret: RemoteSecret,
    },
}
impl RemoteRequest {
    pub fn is_local_admin(&self) -> bool {
        matches!(
            self,
            Self::Status
                | Self::Pairings
                | Self::Clients
                | Self::ApprovePairing { .. }
                | Self::RevokeClient { .. }
        )
    }
    pub fn is_read_only(&self) -> bool {
        matches!(
            self,
            Self::Status
                | Self::Pairings
                | Self::Clients
                | Self::ApprovePairing { dry_run: true, .. }
                | Self::RevokeClient { dry_run: true, .. }
                | Self::Authenticate { .. }
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "result")]
pub enum RemoteResponse {
    Status {
        authority: HostAuthority,
    },
    Pairings {
        pairings: Vec<PendingPairing>,
    },
    Clients {
        clients: Vec<RegisteredClient>,
    },
    Pairing {
        pairing: PendingPairing,
        dry_run: bool,
    },
    Client {
        client: RegisteredClient,
        dry_run: bool,
    },
    Cleaned {
        client_id: String,
    },
}

/// Remote mailbox read policy.
/// Deliberately enumerate operations: categories include private diagnostics/exports.
pub fn remote_read_allowed(request: &crate::Request) -> bool {
    matches!(
        request,
        crate::Request::GetRemoteReady
            | crate::Request::Ping
            | crate::Request::Search { .. }
            | crate::Request::GetBody { .. }
            | crate::Request::ListBodies { .. }
            | crate::Request::ListLabels { .. }
            | crate::Request::ListEnvelopes { .. }
            | crate::Request::ListEnvelopesByIds { .. }
            | crate::Request::GetEnvelope { .. }
            | crate::Request::GetThread { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn read_policy_denies_admin_and_credential_wrappers() {
        for request in [
            crate::Request::GetStatus,
            crate::Request::GetLlmConfig,
            crate::Request::ListAccountsConfig,
            crate::Request::GetRecoveryStatus,
            crate::Request::GetDoctorReport,
            crate::Request::Shutdown,
            crate::Request::RemoteAuthority {
                request: RemoteRequest::Clients,
            },
        ] {
            assert!(!remote_read_allowed(&request));
        }
        assert!(remote_read_allowed(&crate::Request::GetEnvelope {
            message_id: mxr_core::MessageId::new()
        }));
    }
    #[test]
    fn credentials_redact_and_forged_context_fails() {
        let secret = RemoteSecret::new("f".repeat(64));
        let request = RemoteRequest::Authenticate {
            client_id: "client".into(),
            mail_secret: secret,
        };
        assert!(!format!("{request:?}").contains(&"f".repeat(64)));
        let json = serde_json::json!({"action":"Authenticate", "client_id":"client", "mail_secret":"secret", "context":{"admin":true}});
        assert!(serde_json::from_value::<RemoteRequest>(json).is_err());
        for invalid in [
            serde_json::json!({"action":"StartPairing","name":"fixture","capabilities":["mail_write"],"mail_verifier":"a","cleanup_verifier":"b"}),
            serde_json::json!({"action":"ClaimPairing","request_id":"fixture","mail_secret":42,"cleanup_secret":"b"}),
            serde_json::json!({"action":"ApprovePairing","request_id":"fixture"}),
        ] {
            assert!(serde_json::from_value::<RemoteRequest>(invalid).is_err());
        }
    }
}
