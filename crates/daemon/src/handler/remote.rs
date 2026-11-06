//! Local pairing administration and proof checks. Authentication never upgrades IPC trust.
mod llm_status;
use super::{HandlerResult, ResponseData};
use crate::state::AppState;
use mxr_protocol::{RemoteRequest, RemoteResponse};
use mxr_transport::{PeerAuth, PeerInfo};

/// Retain the original registration for background work instead of inheriting local IPC trust.
#[derive(Debug, Clone)]
pub(crate) struct AdmissionProof {
    pub client_id: String,
    /// Canonical parent registration, shared by its browser child sessions.
    pub owner_client_id: String,
    pub mail_secret: mxr_core::RemoteSecret,
}

tokio::task_local! {
    static ADMISSION: AdmissionProof;
}

pub(crate) fn captured_proof() -> Option<AdmissionProof> {
    ADMISSION.try_with(Clone::clone).ok()
}

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
        RemoteRequest::TouchSession {
            client_id,
            mail_secret,
            session_id,
            surface,
        } => {
            let client = state
                .store
                .authenticate_remote_client(client_id, mail_secret, now)
                .await
                .map_err(authority_error)?;
            state
                .remote_lifecycle
                .touch(&client.client_id, session_id, surface, now)
                .map_err(super::HandlerError::RemoteAuthorityDenied)?;
            RemoteResponse::Client {
                client,
                dry_run: false,
            }
        }
        RemoteRequest::CloseSession {
            client_id,
            mail_secret,
            session_id,
        } => {
            let client = state
                .store
                .authenticate_remote_client(client_id, mail_secret, now)
                .await
                .map_err(authority_error)?;
            state
                .remote_lifecycle
                .close(&client.client_id, session_id, now);
            RemoteResponse::Client {
                client,
                dry_run: false,
            }
        }
        RemoteRequest::Sessions => RemoteResponse::Sessions {
            status: state.remote_lifecycle.snapshot(now),
        },
        RemoteRequest::Drain {
            dry_run,
            deadline_seconds,
        } => {
            if !dry_run {
                state.begin_remote_drain(*deadline_seconds);
            }
            let registered_clients = state
                .store
                .remote_clients()
                .await?
                .iter()
                .filter(|client| client.revoked_at.is_none() && client.expires_at > now)
                .count() as u64;
            let scheduled_sends: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM drafts WHERE status='draft' AND send_at IS NOT NULL",
            )
            .fetch_one(state.store.reader())
            .await?;
            let scheduled_snoozes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM snoozed")
                .fetch_one(state.store.reader())
                .await?;
            let scheduled_reminders: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM auto_reminders WHERE triggered_at IS NULL AND cancelled_at IS NULL")
                .fetch_one(state.store.reader()).await?;
            let active_jobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mutation_jobs WHERE status IN ('queued','running') OR (kind='recoverable_archive' AND CASE WHEN json_valid(data_json) THEN json_extract(data_json,'$.running') ELSE 0 END=1)")
                .fetch_one(state.store.reader()).await?;
            let active_assistant_turns: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM assistant_conversations WHERE status='running'",
            )
            .fetch_one(state.store.reader())
            .await?;
            let mut status = state.remote_lifecycle.snapshot(now);
            if *dry_run && !status.draining {
                status.deadline_at = Some(now + i64::from((*deadline_seconds).clamp(1, 30)));
            }
            RemoteResponse::Drain {
                status,
                registered_clients,
                scheduled_sends: scheduled_sends as u64,
                scheduled_snoozes: scheduled_snoozes as u64,
                scheduled_reminders: scheduled_reminders as u64,
                active_jobs: active_jobs as u64,
                active_assistant_turns: active_assistant_turns as u64,
                dry_run: *dry_run,
            }
        }
        RemoteRequest::ResumeHosting => {
            if state.shutdown_requested() {
                return Err(super::HandlerError::RemoteAuthorityDenied(
                    "Daemon is stopping; wait for a new boot".into(),
                ));
            }
            state.remote_lifecycle.resume();
            RemoteResponse::Sessions {
                status: state.remote_lifecycle.snapshot(now),
            }
        }
        RemoteRequest::IssueBrowserCode {
            client_id,
            mail_secret,
        } => {
            let (code, expires_at) = state
                .store
                .issue_browser_code(client_id, mail_secret, now)
                .await
                .map_err(authority_error)?;
            RemoteResponse::BrowserCode { code, expires_at }
        }
        RemoteRequest::ClaimBrowserCode {
            code,
            replace_session,
        } => {
            let (session_id, mail_secret, expires_at) = state
                .store
                .claim_browser_code_replacing(code, replace_session.as_ref(), now)
                .await
                .map_err(authority_error)?;
            RemoteResponse::BrowserSession {
                session_id,
                mail_secret,
                expires_at,
            }
        }
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
        RemoteRequest::RevokeClient { client_id, dry_run } => {
            let client = state
                .store
                .revoke_remote_client(client_id, *dry_run, now)
                .await
                .map_err(authority_error)?;
            if !dry_run {
                state.remote_lifecycle.close_client(client_id, now);
            }
            RemoteResponse::Client {
                client,
                dry_run: *dry_run,
            }
        }
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
            state.remote_lifecycle.close_client(client_id, now);
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

#[cfg(test)]
pub(super) async fn read_owned(
    state: &std::sync::Arc<AppState>,
    client_id: &str,
    mail_secret: &mxr_core::RemoteSecret,
    request: &mxr_protocol::Request,
    owner: Option<&mxr_protocol::ArchiveWorkspaceData>,
) -> HandlerResult {
    read_owned_as(
        state,
        client_id,
        mail_secret,
        request,
        owner,
        mxr_protocol::ClientKind::Cli,
    )
    .await
}

pub(super) async fn read_owned_as(
    state: &std::sync::Arc<AppState>,
    client_id: &str,
    mail_secret: &mxr_core::RemoteSecret,
    request: &mxr_protocol::Request,
    owner: Option<&mxr_protocol::ArchiveWorkspaceData>,
    source: mxr_protocol::ClientKind,
) -> HandlerResult {
    use mxr_protocol::{Request, Response};
    let client = state
        .store
        .authenticate_remote_client(client_id, mail_secret, chrono::Utc::now().timestamp())
        .await
        .map_err(authority_error)?;
    let _admission = state.remote_lifecycle.admit(request).map_err(|message| {
        let status = state
            .remote_lifecycle
            .snapshot(chrono::Utc::now().timestamp());
        super::HandlerError::RemoteResponse(Box::new(mxr_protocol::Response::Error {
            message,
            kind: mxr_protocol::IpcErrorKind::RateLimited,
            code: "remote_host_stopping".into(),
            retryable: true,
            details: Some(
                serde_json::json!({"boot_id":status.boot_id,"deadline_at":status.deadline_at}),
            ),
        }))
    })?;
    if !mxr_protocol::remote_read_allowed(request)
        && !(client
            .capabilities
            .contains(&mxr_core::RemoteCapability::MailSend)
            && mxr_protocol::remote_send_allowed(request))
        && !(client
            .capabilities
            .contains(&mxr_core::RemoteCapability::MailTriage)
            && mxr_protocol::remote_triage_allowed(request))
        && !(client
            .capabilities
            .contains(&mxr_core::RemoteCapability::MailAi)
            && mxr_protocol::remote_ai_allowed(request))
    {
        return Err(super::HandlerError::RemoteAuthorityDenied("Remote operation requires an explicit host-approved grant or is local-only; request --allow-send, --allow-triage or --allow-ai with mxr connect and host approval".into()));
    }
    if matches!(request, Request::ApplyComposeProposal { .. })
        && !client
            .capabilities
            .contains(&mxr_core::RemoteCapability::MailSend)
    {
        return Err(super::HandlerError::RemoteAuthorityDenied(
            "Applying writing help requires host-approved send access".into(),
        ));
    }
    // Validate the original envelope owner after authentication, never infer it from the selected host.
    super::workspace_owner::validate(state, request, owner).await.map_err(|response| {
        if matches!(&*response, Response::Error { code, .. } if code == "workspace_owner_unavailable") {
            sanitize_error(*response)
        } else { super::HandlerError::RemoteResponse(response) }
    })?;
    // The gateway preserves the declared kind. Remote grants and cooperative
    // profiles both apply, including specialized preview/send paths below.
    super::enforce_client_profile(state, &state.config_snapshot(), source, request)
        .await
        .map_err(|_| {
            super::HandlerError::RemoteResponse(Box::new(Response::error_kinded(
                format!(
                    "Request `{}` rejected by host {} profile",
                    super::request_kind(request),
                    source.as_str()
                ),
                mxr_protocol::IpcErrorKind::Policy,
            )))
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
        let lifecycle = state
            .remote_lifecycle
            .snapshot(chrono::Utc::now().timestamp());
        return Ok(ResponseData::RemoteReady {
            accepting_mail: !lifecycle.draining,
            shutdown_deadline_at: lifecycle.deadline_at,
            boot_id: state.remote_lifecycle.boot_id().into(),
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
        | Request::ExtractDraftCommitments { draft }
        | Request::PreviewDraft { draft, .. } => super::draft_preview::require_portable(draft)?,
        _ => (),
    }
    if let Request::ScheduleSend { draft_id, .. } = request {
        let draft = state
            .store
            .get_draft(draft_id)
            .await?
            .ok_or("Draft not found")?;
        super::draft_preview::require_portable(&draft)?;
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
    let response = ADMISSION
        .scope(
            AdmissionProof {
                client_id: client_id.to_owned(),
                owner_client_id: client.client_id.clone(),
                mail_secret: mail_secret.clone(),
            },
            Box::pin(super::dispatch(
                state,
                source,
                &PeerInfo {
                    auth: PeerAuth::TokenRequired,
                },
                request,
            )),
        )
        .await;
    match response {
        Response::Ok { mut data } => {
            if let ResponseData::Accounts { accounts } = &mut data {
                for account in accounts {
                    account.sync = None;
                    account.send = None;
                }
            }
            if let ResponseData::HtmlImageAssets { assets, .. } = &mut data {
                for asset in assets {
                    asset.path = None;
                    asset.detail = None;
                }
            }
            match &mut data {
                ResponseData::LlmStatus { snapshot } => llm_status::project(
                    snapshot,
                    client
                        .capabilities
                        .contains(&mxr_core::RemoteCapability::MailAi),
                    format!(
                        "{}:{}",
                        state.remote_lifecycle.boot_id(),
                        state.llm.policy_generation()
                    ),
                ),
                ResponseData::SyncStatus { sync } => {
                    if sync.last_error.is_some() {
                        sync.last_error = Some("Sync failed; inspect host diagnostics".into());
                    }
                    sync.current_cursor_summary = None;
                    if let Some(progress) = &mut sync.progress {
                        progress.message = "Syncing mail".into();
                    }
                }
                ResponseData::SyncBatch { results } => {
                    for account in results {
                        if account.error.is_some() {
                            account.error = Some("Sync failed; inspect host diagnostics".into());
                        }
                    }
                }
                ResponseData::Freshness { freshness } => {
                    for account in &mut freshness.accounts {
                        if let Some(error) = &mut account.last_sync_error {
                            error.message = "Sync failed; inspect host diagnostics".into();
                        }
                    }
                }
                ResponseData::AssistantConversation { conversation } => {
                    if conversation.last_error.is_some() {
                        conversation.last_error =
                            Some("The assistant turn failed; inspect host diagnostics".into());
                    }
                    conversation.last_error_detail = None;
                }
                ResponseData::Jobs { jobs } => jobs.iter_mut().for_each(sanitize_job),
                ResponseData::Job { job } => sanitize_job(job),
                ResponseData::MutationResult { result } => sanitize_mutation(result),
                ResponseData::ArchiveOperation { operation } => {
                    for item in &mut operation.items {
                        if item.reason.is_some() {
                            item.reason = Some("Inspect the operation on the host".into());
                        }
                    }
                }
                _ => (),
            }
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

fn sanitize_mutation(result: &mut mxr_protocol::MutationResultData) {
    for account in &mut result.accounts {
        if account.error.is_some() {
            account.error = Some("Mail mutation failed; inspect host diagnostics".into());
        }
    }
}

fn sanitize_job(job: &mut mxr_protocol::JobData) {
    if job.error.is_some() {
        job.error = Some("Mail job failed; inspect host diagnostics".into());
    }
    for failure in &mut job.failures {
        failure.error = "Mail mutation failed; inspect host diagnostics".into();
    }
    if let Some(result) = &mut job.result {
        sanitize_mutation(result);
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
            RemoteRequest::Sessions,
            RemoteRequest::Drain {
                dry_run: false,
                deadline_seconds: 5,
            },
            RemoteRequest::ResumeHosting,
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
