//! Pairing authority foundation. These operations do not expose remote mail.
use mxr_core::{HostAuthority, PendingPairing, RegisteredClient, RemoteCapability, RemoteSecret};
use serde::{Deserialize, Serialize};

mod workflow_policy;
pub use workflow_policy::{remote_ai_allowed, remote_triage_allowed};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "action", deny_unknown_fields)]
pub enum RemoteRequest {
    /// Authenticated lease refresh; closing a surface never deregisters its installation.
    TouchSession {
        client_id: String,
        mail_secret: RemoteSecret,
        session_id: String,
        surface: String,
    },
    CloseSession {
        client_id: String,
        mail_secret: RemoteSecret,
        session_id: String,
    },
    Sessions,
    Drain {
        dry_run: bool,
        deadline_seconds: u32,
    },
    ResumeHosting,
    IssueBrowserCode {
        client_id: String,
        mail_secret: RemoteSecret,
    },
    ClaimBrowserCode {
        code: RemoteSecret,
        #[serde(default)]
        replace_session: Option<(String, RemoteSecret)>,
    },
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
                | Self::Sessions
                | Self::Drain { .. }
                | Self::ResumeHosting
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
                | Self::Sessions
                | Self::Drain { dry_run: true, .. }
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
    Sessions {
        status: RemoteSessions,
    },
    Drain {
        status: RemoteSessions,
        registered_clients: u64,
        scheduled_sends: u64,
        scheduled_snoozes: u64,
        scheduled_reminders: u64,
        active_jobs: u64,
        active_assistant_turns: u64,
        dry_run: bool,
    },
    BrowserCode {
        code: RemoteSecret,
        expires_at: i64,
    },
    BrowserSession {
        session_id: String,
        mail_secret: RemoteSecret,
        expires_at: i64,
    },
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RemotePresence {
    pub client_id: String,
    pub session_id: String,
    pub surface: String,
    pub last_seen_at: i64,
    pub expires_at: i64,
    pub active: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RemoteSessions {
    pub boot_id: String,
    pub draining: bool,
    pub deadline_at: Option<i64>,
    pub in_flight: u64,
    pub sessions: Vec<RemotePresence>,
}

/// Remote mailbox read policy.
/// Deliberately enumerate operations: categories include private diagnostics/exports.
pub fn remote_read_allowed(request: &crate::Request) -> bool {
    matches!(
        request,
        crate::Request::GetArchiveWorkspace
            | crate::Request::ListAccounts
            | crate::Request::ListThreads { .. }
            | crate::Request::GetHeaders { .. }
            | crate::Request::GetHtmlImageAssets { .. }
            | crate::Request::ReadInlineImage { .. }
            | crate::Request::GetInvite { .. }
            | crate::Request::ListInvites { .. }
            | crate::Request::ListCategories { .. }
            | crate::Request::ListCategoryMessages { .. }
            | crate::Request::GetCategoryMemberships { .. }
            | crate::Request::PreviewCategoryName { .. }
            | crate::Request::ExplainCategoryMembership { .. }
            | crate::Request::ListSavedSearches
            | crate::Request::ListSavedSearchUnreadCounts
            | crate::Request::RunSavedSearch { .. }
            | crate::Request::ListSubscriptions { .. }
            | crate::Request::GetSyncStatus { .. }
            | crate::Request::GetFreshness { .. }
            | crate::Request::SearchAggregation { .. }
            | crate::Request::GetArchiveOperation { .. }
            | crate::Request::GetJob { .. }
            | crate::Request::ListJobs
            | crate::Request::ListSnoozed
            | crate::Request::ListReplyQueue
            | crate::Request::ListScheduledSends { .. }
            | crate::Request::ListDeliveries { .. }
            | crate::Request::GetDelivery { .. }
            | crate::Request::GetSenderProfile { .. }
            | crate::Request::ListSenders { .. }
            | crate::Request::ListCommitments { .. }
            | crate::Request::ListScreenerQueue { .. }
            | crate::Request::ListScreenerDecisions { .. }
            | crate::Request::ResolveTime { .. }
            | crate::Request::GetThreadContext { .. }
            | crate::Request::GetDesk { .. }
            | crate::Request::GetTodoRunway { .. }
            | crate::Request::ListTodos { .. }
            | crate::Request::GetTodo { .. }
            | crate::Request::GetTodoCatchup { .. }
            | crate::Request::GetKeymap
            | crate::Request::GetLlmStatus
            | crate::Request::GetNotificationChimes
            | crate::Request::GetModeGuide { .. }
            | crate::Request::GetNow { .. }
            | crate::Request::GetRail { .. }
            | crate::Request::GetModeMembership { .. }
            | crate::Request::GetThreadPreviews { .. }
            | crate::Request::GetArrivals { .. }
            | crate::Request::ListArrivals { .. }
            | crate::Request::GetArrivalModes { .. }
            | crate::Request::ListCorrections { .. }
            | crate::Request::ListMessages { .. }
            | crate::Request::GetPerson { .. }
            | crate::Request::ListMergeSuggestions { .. }
            | crate::Request::ListRecords { .. }
            | crate::Request::ListRecordSubscriptions { .. }
            | crate::Request::GetRecord { .. }
            | crate::Request::GetReadingEdition { .. }
            | crate::Request::GetReadingItem { .. }
            | crate::Request::ListOneTimeCodes { .. }
            | crate::Request::GetUpdatesDigest { .. }
            | crate::Request::ListPlace { .. }
            | crate::Request::GetMessageKind { .. }
            | crate::Request::GetThreadGists {
                generate: false,
                ..
            }
            | crate::Request::GetRemoteReady
            | crate::Request::Ping
            | crate::Request::Search { .. }
            | crate::Request::Count { .. }
            | crate::Request::GetBody { .. }
            | crate::Request::ListBodies { .. }
            | crate::Request::ListLabels { .. }
            | crate::Request::SystemMailboxCounts { .. }
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
        crate::Request::PrepareForward { .. }
            | crate::Request::RespondInvite { .. }
            | crate::Request::PrepareInviteResponse { .. }
            | crate::Request::MarkInviteAnswered { .. }
            | crate::Request::ScheduleSend { .. }
            | crate::Request::CancelScheduledSend { .. }
            | crate::Request::ListAccounts
            | crate::Request::GetArchiveWorkspace
            | crate::Request::PrepareReply { .. }
            | crate::Request::EnsureReplyDraft { .. }
            | crate::Request::SetReplyDraftAudience { .. }
            | crate::Request::ResolveSignature { .. }
            | crate::Request::ResolveSendFrom { .. }
            | crate::Request::ListSnippets
            | crate::Request::SetSnippet { .. }
            | crate::Request::DeleteSnippet { .. }
            | crate::Request::ListSignatures
            | crate::Request::ListSignatureDefaults
            | crate::Request::SetSignature { .. }
            | crate::Request::DeleteSignature { .. }
            | crate::Request::SetSignatureDefault { .. }
            | crate::Request::ClearSignatureDefault { .. }
            | crate::Request::ListAccountAddresses { .. }
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
            | crate::Request::RecoverSendOutcome { .. }
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
        let reply_preview = crate::Request::EnsureReplyDraft {
            message_id: mxr_core::MessageId::new(),
            reply_all: false,
            dry_run: true,
        };
        assert!(!remote_read_allowed(&reply_preview));
        assert!(remote_send_allowed(&reply_preview));
        assert!(remote_send_allowed(
            &crate::Request::SetReplyDraftAudience {
                draft_id: mxr_core::DraftId::new(),
                reply_all: true,
                expected_revision: 1,
                dry_run: true,
            }
        ));
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

#[cfg(test)]
mod workflow_policy_tests {
    use super::*;
    #[test]
    fn generation_needs_ai_and_mutations_need_triage() {
        let read = crate::Request::GetThreadGists {
            thread_ids: vec![],
            generate: false,
            automated: false,
        };
        let generate = crate::Request::GetThreadGists {
            thread_ids: vec![],
            generate: true,
            automated: true,
        };
        assert!(remote_read_allowed(&read));
        assert!(!remote_read_allowed(&generate));
        assert!(remote_ai_allowed(&generate));
        let mutation = crate::Request::Mutation {
            mutation: crate::MutationCommand::SetRead {
                message_ids: vec![],
                read: true,
            },
            client_correlation_id: None,
        };
        assert!(!remote_read_allowed(&mutation));
        assert!(!remote_ai_allowed(&mutation));
        assert!(!remote_send_allowed(&mutation));
        assert!(remote_triage_allowed(&mutation));
        for request in [
            crate::Request::Shutdown,
            crate::Request::GetLogs {
                limit: 10,
                level: None,
                search: None,
            },
            crate::Request::GetLlmConfig,
        ] {
            assert!(!remote_read_allowed(&request));
            assert!(!remote_send_allowed(&request));
            assert!(!remote_triage_allowed(&request));
            assert!(!remote_ai_allowed(&request));
        }
    }
}
