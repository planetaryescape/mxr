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
            | crate::Request::Count { .. }
            | crate::Request::GetBody { .. }
            | crate::Request::ListBodies { .. }
            | crate::Request::ListLabels { .. }
            | crate::Request::ListEnvelopes { .. }
            | crate::Request::ListEnvelopesByIds { .. }
            | crate::Request::GetEnvelope { .. }
            | crate::Request::GetThread { .. }
            | crate::Request::GetAttachmentTransferMetadata { .. }
            | crate::Request::ReadAttachmentChunk { .. }
    )
}

/// Registration-wide send authority is deliberately narrower than IPC categories.
pub fn remote_send_allowed(request: &crate::Request) -> bool {
    matches!(
        request,
        crate::Request::ListAccounts
            | crate::Request::GetArchiveWorkspace
            | crate::Request::PrepareReply { .. }
            | crate::Request::ResolveSignature { .. }
            | crate::Request::ResolveSendFrom { .. }
            | crate::Request::ListSnippets
            | crate::Request::ListDrafts
            | crate::Request::GetDraft { .. }
            | crate::Request::SaveDraft { .. }
            | crate::Request::UpdateDraft { .. }
            | crate::Request::DeleteDraft { .. }
            | crate::Request::PreviewDraft { .. }
            | crate::Request::CheckDraftSafety { .. }
            | crate::Request::PreviewDraftAttachment { .. }
            | crate::Request::BeginDraftAttachment { .. }
            | crate::Request::WriteDraftAttachmentChunk { .. }
            | crate::Request::FinalizeDraftAttachment { .. }
            | crate::Request::SendDraft { .. }
            | crate::Request::SendStoredDraft { .. }
            | crate::Request::GetSendOutcome { .. }
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
        assert!(remote_read_allowed(&crate::Request::Count {
            query: "subject:fixture".into(),
            account_id: None,
            mode: None,
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
            serde_json::json!({"action":"StartPairing","name":"fixture","capabilities":["mail_compose"],"mail_verifier":"a","cleanup_verifier":"b"}),
            serde_json::json!({"action":"ClaimPairing","request_id":"fixture","mail_secret":42,"cleanup_secret":"b"}),
            serde_json::json!({"action":"ApprovePairing","request_id":"fixture"}),
        ] {
            assert!(serde_json::from_value::<RemoteRequest>(invalid).is_err());
        }
    }
}

#[cfg(test)]
mod compatibility_tests {
    #[test]
    fn old_readiness_decodes_for_an_actionable_protocol_failure_without_advertising_writes(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let data: crate::ResponseData = serde_json::from_value(serde_json::json!({
            "kind":"RemoteReady", "protocol_version":14, "host_id":"old-host", "client_id":"reader",
            "workspace":{"workspace_id":"workspace","recovery_generation":"generation"}
        }))?;
        let crate::ResponseData::RemoteReady {
            capabilities,
            draft_attachment_upload_supported,
            draft_revision_supported,
            send_outcome_supported,
            protocol_version,
            ..
        } = data
        else {
            return Err("Expected remote readiness".into());
        };
        assert!(capabilities.is_empty());
        assert!(
            !draft_attachment_upload_supported
                && !draft_revision_supported
                && !send_outcome_supported
        );
        assert_ne!(protocol_version, crate::IPC_PROTOCOL_VERSION);
        Ok(())
    }
}
