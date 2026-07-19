#![cfg_attr(
    test,
    expect(
        clippy::panic,
        clippy::unwrap_used,
        reason = "handler tests use panic and unwrap for direct fixture failures"
    )
)]

mod account_config;
mod accounts;
pub(crate) mod activity;
mod admin;
mod archive_ask;
mod auth_sessions;
mod briefing;
mod commitments;
mod commitments_extract;
mod decisions_extract;
pub(crate) mod deliveries;
#[path = "diagnostics/mod.rs"]
pub(crate) mod diagnostics_impl;
mod draft_compose;
mod draft_context;
mod draft_refine;
mod error;
mod expert;
mod helpers;
mod humanizer;
mod mailbox;
mod mutations;
mod notifications;
mod platform;
mod relationship_profile;
pub(crate) mod reply_later;
mod rules;
mod runtime;
mod safety_llm;
mod safety_timing;
mod screener;
mod sender_view;
mod signatures;
mod snippets;
mod status_helpers;
mod suggest_recipients;
pub(crate) mod summarize;
mod triage;
mod user_voice;
mod whois;

use crate::state::AppState;
use mxr_config::{AgentProfileConfig, DestructiveAction, MxrConfig, SafetyPolicy};
use mxr_core::provider::MailSyncProvider;
#[cfg(test)]
use mxr_core::types::UnsubscribeMethod;
use mxr_core::types::{ExportFormat, Snoozed};
use mxr_export::{ExportAttachment, ExportMessage, ExportThread};
use mxr_protocol::*;
use mxr_reader::ReaderConfig;
use mxr_rules::{Conditions, FieldCondition, Rule, RuleAction, StringMatch};
use mxr_search::parse_query;
use mxr_transport::PeerInfo;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use tracing::Instrument;

pub(crate) use helpers::{
    dir_size_sync, file_size_sync, recent_log_lines_sync, should_fallback_to_tantivy,
};
pub(crate) use mutations::send_stored_draft;
pub(crate) use status_helpers::{
    build_doctor_findings, doctor_data_stats, latest_successful_sync_at, DoctorFindingInputs,
};

pub(crate) use error::HandlerError;

type HandlerResult = Result<ResponseData, HandlerError>;

async fn watch_cadence(
    state: &Arc<AppState>,
    account_id: &mxr_core::AccountId,
    email: &str,
    expected_days: Option<f64>,
    note: Option<String>,
    allow_list_sender: bool,
) -> HandlerResult {
    let entry = mxr_store::RelationshipWatchEntry {
        account_id: account_id.clone(),
        email: email.to_string(),
        expected_days,
        note,
        added_at: chrono::Utc::now(),
    };
    state.store.watch_cadence(&entry, allow_list_sender).await?;
    Ok(ResponseData::Ack)
}

async fn unwatch_cadence(
    state: &Arc<AppState>,
    account_id: &mxr_core::AccountId,
    email: &str,
) -> HandlerResult {
    state.store.unwatch_cadence(account_id, email).await?;
    Ok(ResponseData::Ack)
}

async fn list_cadence_watch(
    state: &Arc<AppState>,
    account_id: &mxr_core::AccountId,
) -> HandlerResult {
    let rows = state.store.list_cadence_watch(account_id).await?;
    Ok(ResponseData::CadenceWatchList {
        entries: rows
            .into_iter()
            .map(|r| mxr_protocol::RelationshipWatchEntryData {
                account_id: r.account_id,
                email: r.email,
                expected_days: r.expected_days,
                note: r.note,
                added_at: r.added_at,
            })
            .collect(),
    })
}

async fn list_cadence_drift(
    state: &Arc<AppState>,
    account_id: &mxr_core::AccountId,
) -> HandlerResult {
    let rows = state.store.list_cadence_drift(account_id).await?;
    Ok(ResponseData::CadenceDriftList {
        rows: rows
            .into_iter()
            .map(|r| mxr_protocol::CadenceDriftRowData {
                email: r.email,
                display_name: r.display_name,
                last_contact_at: r.last_contact_at,
                expected_days: r.expected_days,
                drift_days: r.drift_days,
                total_volume: r.total_volume,
            })
            .collect(),
    })
}

async fn send_time_recommendation(
    state: &Arc<AppState>,
    account_id: &mxr_core::AccountId,
    recipients: &[String],
    proposed_at: Option<chrono::DateTime<chrono::Utc>>,
) -> HandlerResult {
    use chrono::{Datelike, Timelike};
    let normalized_recipients: Vec<String> = recipients
        .iter()
        .map(|recipient| recipient.trim())
        .filter(|recipient| !recipient.is_empty())
        .map(str::to_string)
        .collect();
    if normalized_recipients.is_empty() {
        return Err("at least one recipient is required".into());
    }

    let (proposed_weekday, proposed_hour) = proposed_at.map_or((None, None), |at| {
        let wd = at.weekday().num_days_from_monday() as u8;
        let hr = at.hour() as u8;
        (Some(wd), Some(hr))
    });

    let mut rows = Vec::new();
    let mut best_windows = Vec::new();
    let mut confidence = mxr_protocol::SendTimeConfidenceData::High;
    for recipient in normalized_recipients {
        let rec = state
            .store
            .send_time_recommendation(account_id, &recipient)
            .await?;
        let row_confidence = send_time_confidence_data(rec.confidence);
        confidence = min_send_time_confidence(confidence, row_confidence);
        let row_windows = best_windows_for_recipient(&rec);
        best_windows.extend(row_windows.clone());
        let proposed_expected_reply_seconds = match (proposed_weekday, proposed_hour) {
            (Some(wd), Some(hr)) => rec.bucket_p50(wd, hr),
            _ => None,
        };
        rows.push(mxr_protocol::RecipientSendTimeRowData {
            email: rec.recipient,
            sample_count: rec.sample_count,
            proposed_expected_reply_seconds,
            best_expected_reply_seconds: rec.best_p50_seconds,
            best_windows: row_windows,
        });
    }
    best_windows.sort_by_key(|window| window.expected_reply_seconds);
    Ok(ResponseData::SendTimeRecommendationResponse {
        recommendation: mxr_protocol::SendTimeRecommendationData {
            proposed_at,
            proposed_weekday,
            proposed_hour,
            recipient_rows: rows,
            best_windows,
            confidence,
        },
    })
}

fn send_time_confidence_data(
    confidence: mxr_store::SendTimeConfidence,
) -> mxr_protocol::SendTimeConfidenceData {
    match confidence {
        mxr_store::SendTimeConfidence::Low => mxr_protocol::SendTimeConfidenceData::Low,
        mxr_store::SendTimeConfidence::Medium => mxr_protocol::SendTimeConfidenceData::Medium,
        mxr_store::SendTimeConfidence::High => mxr_protocol::SendTimeConfidenceData::High,
    }
}

fn min_send_time_confidence(
    left: mxr_protocol::SendTimeConfidenceData,
    right: mxr_protocol::SendTimeConfidenceData,
) -> mxr_protocol::SendTimeConfidenceData {
    use mxr_protocol::SendTimeConfidenceData::{High, Low, Medium};
    match (left, right) {
        (Low, _) | (_, Low) => Low,
        (Medium, _) | (_, Medium) => Medium,
        (High, High) => High,
    }
}

fn best_windows_for_recipient(
    rec: &mxr_store::SendTimeRecommendation,
) -> Vec<mxr_protocol::SendWindowData> {
    let Some(best) = rec.best_p50_seconds else {
        return Vec::new();
    };
    rec.buckets
        .iter()
        .filter(|bucket| bucket.p50_seconds == best)
        .map(|bucket| mxr_protocol::SendWindowData {
            weekday: bucket.weekday,
            hour_start: bucket.hour,
            hour_end: bucket.hour.saturating_add(1).min(24),
            expected_reply_seconds: bucket.p50_seconds,
        })
        .collect()
}

async fn rebuild_decision_log(
    state: &Arc<AppState>,
    account_id: &mxr_core::AccountId,
    since_days: u32,
) -> HandlerResult {
    let summary = decisions_extract::rebuild(state, account_id, since_days).await?;
    Ok(ResponseData::DecisionLogRebuildSummary {
        extracted: summary.extracted,
        skipped: summary.skipped,
        errors: summary.errors,
    })
}

async fn list_decision_log(
    state: &Arc<AppState>,
    account_id: &mxr_core::AccountId,
    topic: Option<&str>,
    since_days: Option<u32>,
    limit: u32,
) -> HandlerResult {
    let rows = state
        .store
        .list_decisions(account_id, topic, since_days, limit)
        .await?;
    Ok(ResponseData::DecisionLog {
        decisions: rows
            .into_iter()
            .map(|r| mxr_protocol::DecisionLogEntryData {
                id: r.id,
                account_id: r.account_id,
                thread_id: r.thread_id,
                topic: r.topic,
                decision: r.decision,
                rationale: r.rationale,
                evidence_msg_ids: r.evidence_msg_ids,
                decided_at: r.decided_at,
                extracted_at: r.extracted_at,
            })
            .collect(),
    })
}

async fn get_decision(state: &Arc<AppState>, id: &str) -> HandlerResult {
    let row = state.store.get_decision(id).await?;
    Ok(ResponseData::DecisionDetail {
        decision: row.map(|r| mxr_protocol::DecisionLogEntryData {
            id: r.id,
            account_id: r.account_id,
            thread_id: r.thread_id,
            topic: r.topic,
            decision: r.decision,
            rationale: r.rationale,
            evidence_msg_ids: r.evidence_msg_ids,
            decided_at: r.decided_at,
            extracted_at: r.extracted_at,
        }),
    })
}

async fn list_owed_replies(
    state: &Arc<AppState>,
    account_id: &mxr_core::AccountId,
    older_than_days: Option<u32>,
    within_days: Option<u32>,
    limit: u32,
) -> HandlerResult {
    let rows = state
        .store
        .list_owed_replies(account_id, older_than_days, within_days, limit)
        .await?;
    Ok(ResponseData::OwedReplies {
        rows: rows
            .into_iter()
            .map(|r| mxr_protocol::OwedReplyRowData {
                thread_id: r.thread_id,
                latest_inbound_msg_id: r.latest_inbound_msg_id,
                from_email: r.from_email,
                from_name: r.from_name,
                subject: r.subject,
                latest_inbound_at: r.latest_inbound_at,
                waiting_days: r.waiting_days,
                expected_days: r.expected_days,
                overdue_score: r.overdue_score,
            })
            .collect(),
    })
}

/// IPC concurrency lane for a request. Splits the semaphore pool into a
/// fast lane for short user-initiated commands (lists, gets, mutations)
/// and a slow lane for long-running operations (LLM inference, network
/// attachment downloads, full-store rebuilds) so a burst of slow ops
/// can't starve fast commands of permits.
///
/// Sized so the bulk lane holds enough headroom for a handful of
/// parallel LLM calls or attachment downloads while leaving the hot
/// lane untouched. See `server.rs` for the concrete pool sizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcLane {
    /// Fast user-initiated commands: lists, gets, mutations, sync
    /// status, label CRUD. Should complete in milliseconds. Bound is
    /// generous so realistic burst traffic never queues.
    Hot,
    /// Slow operations: anything that calls into the LLM runtime,
    /// fetches a network attachment, rebuilds an entire index, or runs
    /// a multi-second store aggregate. Bounded tighter to keep the
    /// daemon from spawning unbounded LLM/network work in parallel.
    Bulk,
}

/// Classify a request for permit routing. Default to `Hot`; only
/// requests that are known to be slow (network round-trips, LLM,
/// rebuilds) belong on the bulk lane. Adding a new request defaults to
/// the hot lane — if it turns out to be slow, demote it explicitly.
pub fn request_lane(req: &Request) -> IpcLane {
    match req {
        // LLM-bearing operations: variable latency, can hold permits for
        // many seconds while awaiting inference.
        Request::ArchiveAsk { .. }
        | Request::CheckDraftSafety { .. }
        | Request::DraftCompose { .. }
        | Request::DraftRefine { .. }
        | Request::ExtractDraftCommitments { .. }
        | Request::ExplainEntity { .. }
        | Request::FindExpert { .. }
        | Request::GetRecipientBriefing { .. }
        | Request::GetThreadBriefing { .. }
        | Request::HumanizerRewrite { .. }
        | Request::HumanizerScore { .. }
        | Request::SuggestCollaborators { .. }
        | Request::SummarizeThread { .. }
        | Request::TriageSearch { .. } => IpcLane::Bulk,

        // Network-bound: hit Gmail / IMAP / SMTP, can stall on a slow link.
        Request::DownloadAttachment { .. }
        | Request::OpenAttachment { .. }
        | Request::SendDraft { .. }
        | Request::SendStoredDraft { .. }
        | Request::SyncNow { .. }
        | Request::AuthorizeAccountConfig { .. }
        | Request::StartAuthSession { .. }
        | Request::CompleteAuthSession { .. }
        | Request::TestAccountConfig { .. }
        | Request::UnsubscribePurge { .. } => IpcLane::Bulk,

        // Full-store rebuilds and bulk indexing.
        Request::RebuildAnalytics
        | Request::RebuildDecisionLog { .. }
        | Request::RebuildRelationshipProfile { .. }
        | Request::RebuildUserVoice { .. }
        | Request::RecomputeLinkCounts
        | Request::RefreshContacts
        | Request::BackfillCalendarInvites { .. }
        | Request::ReindexSemantic
        | Request::BackfillSemantic
        | Request::InstallSemanticProfile { .. }
        | Request::UseSemanticProfile { .. }
        | Request::EnableSemantic { .. } => IpcLane::Bulk,

        // HTML asset prefetch with remote-content fetching is a network
        // round-trip per image; demote so a large gallery can't tie up
        // every fast slot.
        Request::GetHtmlImageAssets { allow_remote, .. } if *allow_remote => IpcLane::Bulk,

        // Diagnostics that scan large tables.
        Request::GenerateBugReport { .. } => IpcLane::Bulk,

        _ => IpcLane::Hot,
    }
}

/// Handle an IPC request with the connection's [`PeerInfo`] in scope. The real
/// entry point for served connections; [`handle_request`] is the in-process
/// convenience wrapper that supplies a local peer.
pub async fn handle_request_with_peer(
    state: &Arc<AppState>,
    msg: &IpcMessage,
    peer: PeerInfo,
) -> IpcMessage {
    let response_data = match &msg.payload {
        IpcPayload::Request(req) => {
            let request = request_kind(req);
            let account_id_str = request_account_id(req)
                .map_or_else(|| "-".to_string(), mxr_core::AccountId::as_str);
            let account_key = request_account_key(req).unwrap_or("-");
            let span = tracing::info_span!(
                "ipc_request",
                request_id = msg.id,
                request,
                account_id = account_id_str.as_str(),
                account_key
            );
            let response = dispatch(state, msg.source, &peer, req)
                .instrument(span)
                .await;

            // Activity capture seam. Fire-and-forget; never propagates errors.
            // See `docs/activity-log.md`.
            let ok = matches!(&response, Response::Ok { .. });
            let account_id_for_activity = request_account_id(req).map(mxr_core::AccountId::as_str);
            if let Some(entry) = crate::activity::mapper::map_request(
                req,
                msg.source,
                account_id_for_activity.as_deref(),
                ok,
            ) {
                state.activity.record(entry);
            }
            crate::chimes::play_for_request_response(state, req, &response);

            response
        }
        _ => Response::error("Expected a Request"),
    };

    IpcMessage {
        id: msg.id,
        source: ::mxr_protocol::ClientKind::Daemon,
        payload: IpcPayload::Response(response_data),
    }
}

/// Handle an IPC request that originated in-process (daemon-internal
/// re-dispatch, tests). Supplies a [`PeerInfo::local`] peer; served connections
/// call [`handle_request_with_peer`] with the accepted peer instead.
pub async fn handle_request(state: &Arc<AppState>, msg: &IpcMessage) -> IpcMessage {
    handle_request_with_peer(state, msg, PeerInfo::local()).await
}

async fn dispatch(
    state: &Arc<AppState>,
    source: ClientKind,
    peer: &PeerInfo,
    req: &Request,
) -> Response {
    let started_at = std::time::Instant::now();
    let request = request_kind(req);
    let account_id =
        request_account_id(req).map_or_else(|| "-".to_string(), mxr_core::AccountId::as_str);
    let account_key = request_account_key(req).unwrap_or("-");
    // `peer` is the connection's auth evidence; carried here as the plumbing
    // point for phase 5's per-transport policy. No policy consults it this
    // phase — logged for observability so the seam is visible in traces.
    tracing::debug!(request, account_id, account_key, peer = ?peer.auth, "handling request");

    let config = state.config_snapshot();
    if let Err(message) = enforce_safety_policy(config.general.safety_policy, req) {
        tracing::warn!(request, account_id, account_key, error = %message, "request rejected by safety policy");
        return Response::error(message);
    }
    if let Err(message) = enforce_client_profile(state, &config, source, req).await {
        tracing::warn!(request, account_id, account_key, source = source.as_str(), error = %message, "request rejected by client profile");
        return Response::error(message);
    }

    let result = match req {
        Request::ListEnvelopes {
            label_id,
            account_id,
            limit,
            offset,
        } => {
            mailbox::list_envelopes(
                state,
                label_id.as_ref(),
                account_id.as_ref(),
                *limit,
                *offset,
            )
            .await
        }
        Request::ListEnvelopesByIds { message_ids } => {
            mailbox::list_envelopes_by_ids(state, message_ids).await
        }
        Request::GetEnvelope { message_id } => mailbox::get_envelope(state, message_id).await,
        Request::GetBody { message_id } => mailbox::get_body(state, message_id).await,
        Request::GetInvite { message_id } => mailbox::get_invite(state, message_id).await,
        Request::ListInvites { account_id, limit } => {
            mailbox::list_invites(state, account_id.as_ref(), *limit).await
        }
        Request::BackfillCalendarInvites { account_id } => {
            mailbox::backfill_invites(state, account_id.as_ref()).await
        }
        Request::RespondInvite {
            message_id,
            action,
            dry_run,
        } => mailbox::respond_invite(state, message_id, *action, *dry_run).await,
        Request::PrepareInviteResponse { message_id, action } => {
            mailbox::prepare_invite_response(state, message_id, *action).await
        }
        Request::MarkInviteAnswered {
            message_id,
            attendee_email,
            partstat,
        } => mailbox::mark_invite_answered(state, message_id, attendee_email, *partstat).await,
        Request::GetHtmlImageAssets {
            message_id,
            allow_remote,
        } => mailbox::get_html_image_assets(state, message_id, *allow_remote).await,
        Request::DownloadAttachment {
            message_id,
            attachment_id,
            destination,
        } => {
            mailbox::download_attachment(state, message_id, attachment_id, destination.as_deref())
                .await
        }
        Request::OpenAttachment {
            message_id,
            attachment_id,
        } => mailbox::open_attachment(state, message_id, attachment_id).await,
        Request::ListBodies { message_ids } => mailbox::list_bodies(state, message_ids).await,
        Request::GetThread { thread_id } => mailbox::get_thread(state, thread_id).await,
        Request::ListThreads {
            account_id,
            label_id,
            limit,
            offset,
            sort,
        } => {
            mailbox::list_threads(
                state,
                account_id.as_ref(),
                label_id.as_ref(),
                *limit,
                *offset,
                sort.clone(),
            )
            .await
        }
        Request::ListLabels { account_id } => {
            mailbox::list_labels(state, account_id.as_ref()).await
        }
        Request::CreateLabel {
            name,
            color,
            account_id,
        } => mailbox::create_label(state, name, color.as_deref(), account_id.as_ref()).await,
        Request::DeleteLabel { name, account_id } => {
            mailbox::delete_label(state, name, account_id.as_ref()).await
        }
        Request::RenameLabel {
            old,
            new,
            account_id,
        } => mailbox::rename_label(state, old, new, account_id.as_ref()).await,
        // mxr app/platform
        Request::ListAccounts => accounts::list_accounts(state).await,
        Request::ListAccountsConfig => accounts::list_accounts_config(),
        Request::AuthorizeAccountConfig {
            account,
            reauthorize,
        } => accounts::authorize_account(account.clone(), *reauthorize).await,
        Request::StartAuthSession {
            account,
            reauthorize,
            flow,
        } => auth_sessions::start_auth_session(state, account.clone(), *reauthorize, *flow).await,
        Request::GetAuthSession { session_id } => {
            auth_sessions::get_auth_session(state, session_id)
        }
        Request::CancelAuthSession { session_id } => {
            auth_sessions::cancel_auth_session(state, session_id)
        }
        Request::CompleteAuthSession {
            session_id,
            save_account,
        } => auth_sessions::complete_auth_session(state, session_id, *save_account).await,
        Request::UpsertAccountConfig { account } => {
            accounts::upsert_account(state, account.clone()).await
        }
        Request::SetDefaultAccount { key } => accounts::set_default_account_key(state, key).await,
        Request::TestAccountConfig { account } => {
            accounts::test_account(state, account.clone()).await
        }
        Request::DisableAccountConfig { key } => accounts::disable_account(state, key).await,
        Request::RemoveAccountConfig {
            key,
            purge_local_data,
            dry_run,
        } => accounts::remove_account(state, key, *purge_local_data, *dry_run).await,
        Request::RepairAccountConfig { account } => accounts::repair_account(account.clone()).await,
        Request::ListRules => rules::list_rules(state).await,
        Request::GetRule { rule } => rules::get_rule(state, rule).await,
        Request::GetRuleForm { rule } => rules::get_rule_form(state, rule).await,
        Request::UpsertRule { rule } => rules::upsert_rule_value(state, rule.clone()).await,
        Request::DeleteRule { rule } => rules::delete_rule(state, rule).await,
        Request::UpsertRuleForm {
            existing_rule,
            name,
            condition,
            action,
            priority,
            enabled,
        } => {
            rules::upsert_rule_form(
                state,
                existing_rule.as_ref(),
                name,
                condition,
                action,
                *priority,
                *enabled,
            )
            .await
        }
        Request::ListRuleHistory { rule, limit } => {
            rules::list_rule_history(state, rule.as_ref(), *limit).await
        }
        Request::DryRunRules { rule, all, after } => {
            rules::dry_run(state, rule.as_ref(), *all, after.as_ref()).await
        }
        Request::ListSavedSearches => platform::list_saved_searches(state).await,
        Request::ListSavedSearchUnreadCounts => {
            platform::list_saved_search_unread_counts(state).await
        }
        Request::ListSubscriptions { account_id, limit } => {
            platform::list_subscriptions(state, account_id.as_ref(), *limit).await
        }
        Request::ListStorageBreakdown {
            account_id,
            group_by,
            limit,
        } => platform::list_storage_breakdown(state, account_id.as_ref(), *group_by, *limit).await,
        Request::ListLargestMessages {
            account_id,
            since_days,
            limit,
        } => platform::list_largest_messages(state, account_id.as_ref(), *since_days, *limit).await,
        Request::Wrapped {
            account_id,
            since_unix,
            until_unix,
            label,
        } => platform::wrapped(state, account_id.as_ref(), *since_unix, *until_unix, label).await,
        Request::ListStaleThreads {
            account_id,
            perspective,
            older_than_days,
            within_days,
            limit,
        } => {
            platform::list_stale_threads(
                state,
                account_id.as_ref(),
                *perspective,
                *older_than_days,
                *within_days,
                *limit,
            )
            .await
        }
        Request::ListContactAsymmetry {
            account_id,
            min_inbound,
            limit,
        } => {
            platform::list_contact_asymmetry(state, account_id.as_ref(), *min_inbound, *limit).await
        }
        Request::ListContactDecay {
            account_id,
            threshold_days,
            max_lookback_days,
            limit,
        } => {
            platform::list_contact_decay(
                state,
                account_id.as_ref(),
                *threshold_days,
                *max_lookback_days,
                *limit,
            )
            .await
        }
        Request::RefreshContacts => platform::refresh_contacts(state).await,
        Request::RebuildAnalytics => platform::rebuild_analytics(state).await,
        Request::RecomputeLinkCounts => platform::recompute_link_counts(state).await,
        Request::ListResponseTime {
            account_id,
            direction,
            counterparty,
            since_days,
        } => {
            platform::list_response_time(
                state,
                account_id.as_ref(),
                *direction,
                counterparty.as_deref(),
                *since_days,
            )
            .await
        }
        Request::ListAccountAddresses { account_id } => {
            platform::list_account_addresses(state, account_id).await
        }
        Request::AddAccountAddress {
            account_id,
            email,
            primary,
        } => platform::add_account_address(state, account_id, email, *primary).await,
        Request::RemoveAccountAddress { account_id, email } => {
            platform::remove_account_address(state, account_id, email).await
        }
        Request::SetPrimaryAccountAddress { account_id, email } => {
            platform::set_primary_account_address(state, account_id, email).await
        }
        Request::GetLlmStatus => platform::llm_status(state).await,
        Request::GetLlmConfig => platform::llm_config(state).await,
        Request::UpdateLlmConfig { config } => {
            platform::update_llm_config(state, config.as_ref().clone()).await
        }
        Request::GetNotificationChimes => notifications::get_notification_chimes(state).await,
        Request::UpdateNotificationChimes { config } => {
            notifications::update_notification_chimes(state, config.as_ref().clone()).await
        }
        Request::PreviewNotificationChime { event } => {
            notifications::preview_notification_chime(state, *event).await
        }
        Request::GetSemanticStatus => platform::semantic_status(state).await,
        Request::EnableSemantic { enabled } => platform::enable_semantic(state, *enabled).await,
        Request::InstallSemanticProfile { profile } => {
            platform::install_semantic_profile(state, *profile).await
        }
        Request::UseSemanticProfile { profile } => {
            platform::use_semantic_profile(state, *profile).await
        }
        Request::ReindexSemantic => platform::reindex_semantic(state).await,
        Request::BackfillSemantic => platform::backfill_semantic(state).await,
        Request::CreateSavedSearch {
            name,
            query,
            account_id,
            search_mode,
        } => {
            platform::create_saved_search(state, name, query, account_id.clone(), *search_mode)
                .await
        }
        Request::DeleteSavedSearch { name } => platform::delete_saved_search(state, name).await,
        Request::UpdateSavedSearch {
            name,
            new_name,
            query,
            search_mode,
            sort,
            icon,
            position,
        } => {
            platform::update_saved_search(
                state,
                name,
                mxr_store::SavedSearchUpdate {
                    new_name: new_name.as_deref(),
                    query: query.as_deref(),
                    search_mode: search_mode.as_ref(),
                    sort: sort.as_ref(),
                    icon: icon.as_deref(),
                    position: *position,
                },
            )
            .await
        }
        Request::RunSavedSearch {
            name,
            limit,
            account_id,
        } => platform::run_saved_search(state, name, *limit, account_id.as_ref()).await,

        // admin / maintenance / operational
        Request::ListEvents {
            limit,
            level,
            category,
            since,
            until,
            search,
            category_prefix,
            offset,
        } => {
            admin::list_events(
                state,
                mxr_store::EventLogFilter {
                    limit: *limit,
                    offset: *offset,
                    level: level.as_deref(),
                    category: category.as_deref(),
                    category_prefix: category_prefix.as_deref(),
                    since: *since,
                    until: *until,
                    search: search.as_deref(),
                },
            )
            .await
        }
        Request::GetLogs {
            limit,
            level,
            search,
        } => admin::get_logs(state, *limit, level.as_deref(), search.as_deref()).await,
        Request::ListEventCategories => admin::list_event_categories(state).await,
        Request::CountEvents {
            level,
            category,
            category_prefix,
            since,
            until,
            search,
        } => {
            admin::count_events(
                state,
                mxr_store::EventLogFilter {
                    level: level.as_deref(),
                    category: category.as_deref(),
                    category_prefix: category_prefix.as_deref(),
                    since: *since,
                    until: *until,
                    search: search.as_deref(),
                    ..mxr_store::EventLogFilter::default()
                },
            )
            .await
        }
        Request::GetDoctorReport => admin::doctor_report(state).await,
        Request::GenerateBugReport {
            verbose,
            full_logs,
            since,
        } => admin::bug_report(*verbose, *full_logs, since.clone()).await,
        Request::GetStatus => admin::get_status(state).await,
        Request::Ping => Ok(ResponseData::Pong),
        Request::Shutdown => admin::shutdown(state).await,

        // ----- activity log (Phase 3) -----
        Request::ListActivity {
            filter,
            limit,
            cursor,
        } => activity::list_activity(state, filter, *limit, *cursor).await,
        Request::CountActivity { filter } => activity::count_activity(state, filter).await,
        Request::ActivityStats {
            since,
            until,
            group_by,
        } => activity::activity_stats(state, *since, *until, *group_by).await,
        Request::ExportActivity {
            filter,
            format,
            path,
        } => activity::export_activity(state, filter, *format, path.clone()).await,
        Request::RedactActivity {
            ids,
            filter,
            dry_run,
        } => activity::redact_activity(state, ids, filter.as_ref(), *dry_run).await,
        Request::PruneActivity {
            before_ts,
            tier,
            dry_run,
        } => activity::prune_activity(state, *before_ts, *tier, *dry_run).await,
        Request::PauseActivity { until_ts } => activity::pause_activity(state, *until_ts).await,
        Request::ResumeActivity => activity::resume_activity(state).await,
        Request::ListSavedActivityFilters => activity::list_saved_filters(state).await,
        Request::GetSavedActivityFilter { slug } => activity::get_saved_filter(state, slug).await,
        Request::UpsertSavedActivityFilter { slug, name, filter } => {
            activity::upsert_saved_filter(state, slug, name, filter).await
        }
        Request::DeleteSavedActivityFilter { slug } => {
            activity::delete_saved_filter(state, slug).await
        }

        // core mail/runtime
        Request::Search {
            query,
            limit,
            offset,
            account_id,
            mode,
            sort,
            explain,
        } => {
            runtime::search(
                state,
                query,
                *limit,
                *offset,
                account_id.as_ref(),
                mode.unwrap_or(state.config_snapshot().search.default_mode),
                sort.clone().unwrap_or(mxr_core::types::SortOrder::DateDesc),
                *explain,
            )
            .await
        }
        Request::Count {
            query,
            account_id,
            mode,
        } => {
            runtime::count(
                state,
                query,
                account_id.as_ref(),
                mode.unwrap_or(state.config_snapshot().search.default_mode),
            )
            .await
        }
        Request::SearchAggregation {
            query,
            account_id,
            mode,
            group_by,
            limit,
        } => {
            runtime::search_aggregation(
                state,
                query,
                account_id.as_ref(),
                mode.unwrap_or(state.config_snapshot().search.default_mode),
                *group_by,
                *limit,
            )
            .await
        }
        Request::GetHeaders { message_id } => runtime::get_headers(state, message_id).await,
        Request::SyncNow { account_id } => runtime::sync_now(state, account_id.as_ref()).await,
        Request::ExportThread { thread_id, format } => {
            runtime::export_thread(state, thread_id, format).await
        }
        Request::ExportSearch {
            query,
            account_id,
            format,
        } => runtime::export_search(state, query, account_id.as_ref(), format).await,
        Request::Mutation {
            mutation: cmd,
            client_correlation_id,
        } => mutations::mutation(state, cmd, client_correlation_id.as_deref()).await,
        Request::StartMutationJob {
            mutation: cmd,
            client_correlation_id,
        } => {
            mutations::start_mutation_job(state.clone(), cmd.clone(), client_correlation_id.clone())
                .await
        }
        Request::ListJobs => mutations::list_jobs(state).await,
        Request::GetJob { job_id } => mutations::get_job(state, job_id).await,
        Request::UndoMutation { mutation_id } => mutations::undo_mutation(state, mutation_id).await,
        Request::Snooze {
            message_id,
            wake_at,
        } => mutations::snooze(state, message_id, wake_at).await,
        Request::Unsnooze { message_id } => mutations::unsnooze(state, message_id).await,
        Request::ListSnoozed => mutations::list_snoozed(state).await,
        Request::SetReplyLater { message_id, flag } => {
            reply_later::set_reply_later(state, message_id, *flag).await
        }
        Request::ListReplyQueue => reply_later::list_reply_queue(state).await,
        Request::SetAutoReminder {
            sent_message_id,
            remind_at,
        } => reply_later::set_auto_reminder(state, sent_message_id, *remind_at).await,
        Request::CancelAutoReminder { sent_message_id } => {
            reply_later::cancel_auto_reminder(state, sent_message_id).await
        }
        Request::ScheduleSend { draft_id, send_at } => {
            mutations::schedule_send(state, draft_id, *send_at).await
        }
        Request::CancelScheduledSend { draft_id } => {
            mutations::cancel_scheduled_send(state, draft_id).await
        }
        Request::ListSnippets => snippets::list_snippets(state).await,
        Request::SetSnippet { name, body, vars } => {
            snippets::set_snippet(state, name.clone(), body.clone(), vars.clone()).await
        }
        Request::DeleteSnippet { name } => snippets::delete_snippet(state, name).await,
        Request::ListDeliveries { account_id, filter } => {
            deliveries::list_deliveries(state, account_id.as_ref(), filter.as_deref()).await
        }
        Request::GetDelivery { delivery_id } => deliveries::get_delivery(state, delivery_id).await,
        Request::ResolveDelivery { delivery_id } => {
            deliveries::resolve_delivery(state, delivery_id).await
        }
        Request::DismissDelivery { delivery_id } => {
            deliveries::dismiss_delivery(state, delivery_id).await
        }
        Request::ScanDeliveries {
            account_id,
            since_days,
            dry_run,
        } => deliveries::scan_deliveries(state, account_id.as_ref(), *since_days, *dry_run).await,
        Request::ListSignatures => signatures::list_signatures(state).await,
        Request::ListSignatureDefaults => signatures::list_signature_defaults(state).await,
        Request::SetSignature { name, body } => {
            signatures::set_signature(state, name.clone(), body.clone()).await
        }
        Request::DeleteSignature { name } => signatures::delete_signature(state, name).await,
        Request::SetSignatureDefault {
            name,
            kind,
            account_id,
            from_email,
        } => {
            signatures::set_signature_default(
                state,
                name,
                *kind,
                account_id.as_ref(),
                from_email.as_deref(),
            )
            .await
        }
        Request::ClearSignatureDefault {
            kind,
            account_id,
            from_email,
        } => {
            signatures::clear_signature_default(
                state,
                *kind,
                account_id.as_ref(),
                from_email.as_deref(),
            )
            .await
        }
        Request::ResolveSignature {
            name,
            kind,
            account_id,
            from_email,
        } => {
            signatures::resolve_signature(
                state,
                name.as_deref(),
                *kind,
                account_id.as_ref(),
                from_email.as_deref(),
            )
            .await
        }
        Request::GetSenderProfile { account_id, email } => {
            sender_view::get_sender_profile(state, account_id, email).await
        }
        Request::ListSenders {
            account_id,
            limit,
            since_unix,
        } => platform::list_senders(state, account_id.as_ref(), *limit, *since_unix).await,
        Request::GetRelationshipProfile { account_id, email } => {
            relationship_profile::get_relationship_profile(state, account_id, email).await
        }
        Request::RebuildRelationshipProfile { account_id, email } => {
            relationship_profile::rebuild_relationship_profile(state, account_id, email).await
        }
        Request::ListCommitments {
            account_id,
            email,
            status,
        } => commitments::list_commitments(state, account_id, email.as_deref(), *status).await,
        Request::ResolveCommitment { commitment_id } => {
            commitments::resolve_commitment(state, commitment_id).await
        }
        Request::ListOwedReplies {
            account_id,
            older_than_days,
            within_days,
            limit,
        } => list_owed_replies(state, account_id, *older_than_days, *within_days, *limit).await,
        Request::ArchiveAsk {
            question,
            filters,
            limit,
        } => archive_ask::ask(state, question, filters, *limit as usize).await,
        Request::ListDecisionLog {
            account_id,
            topic,
            since_days,
            limit,
        } => list_decision_log(state, account_id, topic.as_deref(), *since_days, *limit).await,
        Request::RebuildDecisionLog {
            account_id,
            since_days,
        } => rebuild_decision_log(state, account_id, *since_days).await,
        Request::GetDecision { id } => get_decision(state, id).await,
        Request::SendTimeRecommendation {
            account_id,
            recipients,
            proposed_at,
        } => send_time_recommendation(state, account_id, recipients, *proposed_at).await,
        Request::GetThreadBriefing { thread_id, refresh } => {
            briefing::get_thread_briefing(state, thread_id, *refresh).await
        }
        Request::GetRecipientBriefing {
            account_id,
            email,
            refresh,
        } => briefing::get_recipient_briefing(state, account_id, email, *refresh).await,
        Request::SuggestCollaborators { draft, limit } => {
            suggest_recipients::suggest(state, draft, *limit as usize).await
        }
        Request::FindExpert {
            account_id,
            query,
            include_self,
            limit,
        } => expert::find(state, account_id, query, *include_self, *limit as usize).await,
        Request::ExplainEntity {
            account_id,
            query,
            limit,
        } => whois::explain(state, account_id, query, *limit as usize).await,
        Request::WatchCadence {
            account_id,
            email,
            expected_days,
            note,
            allow_list_sender,
        } => {
            watch_cadence(
                state,
                account_id,
                email,
                *expected_days,
                note.clone(),
                *allow_list_sender,
            )
            .await
        }
        Request::UnwatchCadence { account_id, email } => {
            unwatch_cadence(state, account_id, email).await
        }
        Request::ListCadenceWatch { account_id } => list_cadence_watch(state, account_id).await,
        Request::ListCadenceDrift { account_id } => list_cadence_drift(state, account_id).await,
        Request::GetUserVoice { account_id } => user_voice::get_user_voice(state, account_id).await,
        Request::RebuildUserVoice { account_id } => {
            user_voice::rebuild_user_voice(state, account_id).await
        }
        Request::HumanizerScore { text } => humanizer::score_text(text).await,
        Request::HumanizerRewrite {
            text,
            max_iterations,
        } => humanizer::rewrite_text(state, text, *max_iterations).await,
        Request::ListScreenerQueue { account_id, limit } => {
            screener::list_queue(state, account_id, *limit).await
        }
        Request::ListScreenerDecisions { account_id } => {
            screener::list_decisions(state, account_id).await
        }
        Request::SetScreenerDecision {
            account_id,
            sender_email,
            disposition,
            route_label,
        } => {
            screener::set_decision(
                state,
                account_id,
                sender_email.clone(),
                *disposition,
                route_label.clone(),
            )
            .await
        }
        Request::ClearScreenerDecision {
            account_id,
            sender_email,
        } => screener::clear_decision(state, account_id, sender_email).await,
        Request::SummarizeThread { thread_id } => {
            summarize::summarize_thread(state, thread_id).await
        }
        Request::TriageSearch {
            query,
            limit,
            offset,
            account_id,
            mode,
            sort,
        } => {
            triage::triage_search(
                state,
                query,
                *limit,
                *offset,
                account_id.as_ref(),
                mode.unwrap_or(state.config_snapshot().search.default_mode),
                sort.clone().unwrap_or(mxr_core::types::SortOrder::DateDesc),
            )
            .await
        }
        Request::DraftCompose {
            account_id,
            to,
            instruction,
            source_message_id,
            thread_id,
            register,
            length_hint,
        } => {
            draft_compose::draft_compose(
                state,
                account_id.as_ref(),
                to.clone(),
                instruction,
                source_message_id.clone(),
                thread_id.clone(),
                *register,
                *length_hint,
            )
            .await
        }
        Request::DraftRefine { draft_id, knobs } => {
            draft_refine::draft_refine(state, draft_id, knobs.clone()).await
        }
        Request::ListDrafts => mutations::list_drafts(state).await,
        Request::ListOrphanedDrafts => mutations::list_orphaned_drafts(state).await,
        Request::ResetOrphanedDraft { draft_id } => {
            mutations::reset_orphaned_draft(state, draft_id).await
        }
        Request::PrepareReply {
            message_id,
            reply_all,
        } => mutations::prepare_reply(state, message_id, *reply_all).await,
        Request::PrepareForward { message_id } => {
            mutations::prepare_forward(state, message_id).await
        }
        Request::SendDraft {
            draft,
            override_safety_token,
        } => mutations::send_draft(state, draft, override_safety_token.as_deref()).await,
        Request::SaveDraft { draft } => mutations::save_draft(state, draft).await,
        Request::SendStoredDraft {
            draft_id,
            override_safety_token,
        } => mutations::send_stored_draft(state, draft_id, override_safety_token.as_deref()).await,
        Request::CheckDraftSafety { draft, context } => {
            mutations::check_draft_safety_request(state, draft, context).await
        }
        Request::ExtractDraftCommitments { draft } => {
            commitments_extract::extract_request(state, draft).await
        }
        Request::DeleteDraft { draft_id } => mutations::delete_draft(state, draft_id).await,
        Request::GetDraft { draft_id } => mutations::get_draft(state, draft_id).await,
        Request::UpdateDraft { draft } => mutations::update_draft(state, draft).await,
        Request::SaveDraftToServer { draft } => mutations::save_draft_to_server(state, draft).await,
        Request::Unsubscribe { message_id } => mutations::unsubscribe(state, message_id).await,
        Request::UnsubscribePurge {
            address,
            account_id,
            dry_run,
            archive_on_no_method,
        } => {
            mutations::unsubscribe_purge(
                state,
                address,
                account_id.as_ref(),
                *dry_run,
                *archive_on_no_method,
            )
            .await
        }
        Request::SetFlags { message_id, flags } => {
            mutations::set_flags(state, message_id, *flags).await
        }
        Request::GetSyncStatus { account_id } => runtime::get_sync_status(state, account_id).await,
    };

    match result {
        Ok(data) => {
            tracing::debug!(
                request,
                account_id,
                account_key,
                elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0,
                "request completed"
            );
            Response::Ok { data }
        }
        Err(message) => {
            tracing::warn!(
                request,
                account_id,
                account_key,
                elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0,
                error = %message,
                "request failed"
            );
            Response::error(message)
        }
    }
}

fn enforce_safety_policy(policy: SafetyPolicy, req: &Request) -> Result<(), String> {
    match policy {
        SafetyPolicy::Full => Ok(()),
        SafetyPolicy::ReadOnly if request_is_read_only(req) => Ok(()),
        SafetyPolicy::DraftOnly | SafetyPolicy::Restricted
            if request_is_read_only(req) || request_is_draft_only(req) =>
        {
            Ok(())
        }
        SafetyPolicy::ReadOnly => Err(format!(
            "Request `{}` rejected by read-only safety policy",
            request_kind(req)
        )),
        SafetyPolicy::DraftOnly => Err(format!(
            "Request `{}` rejected by draft-only safety policy",
            request_kind(req)
        )),
        SafetyPolicy::Restricted => Err(format!(
            "Request `{}` rejected by restricted safety policy",
            request_kind(req)
        )),
    }
}

async fn enforce_client_profile(
    state: &Arc<AppState>,
    config: &MxrConfig,
    source: ClientKind,
    req: &Request,
) -> Result<(), String> {
    let profile_name = match source {
        ClientKind::Agent | ClientKind::Mcp => source.as_str(),
        ClientKind::Human
        | ClientKind::Tui
        | ClientKind::Cli
        | ClientKind::Script
        | ClientKind::Web
        | ClientKind::Daemon => return Ok(()),
    };

    let profile = config
        .agent_surfaces
        .profiles
        .get(profile_name)
        .ok_or_else(|| format!("{profile_name} IPC requests require a configured profile"))?;

    enforce_safety_policy(profile.safety_policy, req).map_err(|_| {
        format!(
            "Request `{}` rejected by {profile_name} profile safety policy",
            request_kind(req)
        )
    })?;

    if request_requires_send_capability(req) && !profile.allow_send {
        return Err(format!(
            "Request `{}` rejected by {profile_name} profile send gate",
            request_kind(req)
        ));
    }

    if request_requires_destructive_capability(req) && !profile.allow_destructive {
        return Err(format!(
            "Request `{}` rejected by {profile_name} profile destructive gate",
            request_kind(req)
        ));
    }

    // Fine-grained restriction within the destructive gate: when the
    // profile lists specific allowed actions, a destructive request whose
    // action isn't listed is rejected even though `allow_destructive` is
    // true. An empty list means "no per-action restriction" (back-compat).
    if !profile.allowed_destructive_actions.is_empty() {
        if let Some(action) = request_destructive_action(req) {
            if !profile.allowed_destructive_actions.contains(&action) {
                return Err(format!(
                    "Request `{}` ({action:?}) rejected by {profile_name} profile destructive-action allowlist",
                    request_kind(req)
                ));
            }
        }
    }

    enforce_account_allowlist(state, profile_name, profile, req).await
}

/// Map a destructive-class request to the specific action it performs,
/// for per-action profile scoping. Benign mailbox mutations (star, mark
/// read, label tagging) and non-destructive requests return `None` — the
/// per-action allowlist doesn't constrain them (the coarse
/// `allow_destructive` gate still applies via `classify_request`).
fn request_destructive_action(req: &Request) -> Option<DestructiveAction> {
    match req {
        Request::Mutation { mutation, .. } | Request::StartMutationJob { mutation, .. } => {
            mutation_destructive_action(mutation)
        }
        Request::DeleteLabel { .. } => Some(DestructiveAction::DeleteLabel),
        Request::RemoveAccountConfig { .. } => Some(DestructiveAction::RemoveAccount),
        Request::Unsubscribe { .. } | Request::UnsubscribePurge { .. } => {
            Some(DestructiveAction::Unsubscribe)
        }
        Request::RedactActivity { .. } => Some(DestructiveAction::RedactActivity),
        Request::PruneActivity { .. } => Some(DestructiveAction::PruneActivity),
        _ => None,
    }
}

fn mutation_destructive_action(cmd: &MutationCommand) -> Option<DestructiveAction> {
    match cmd {
        MutationCommand::Archive { .. } | MutationCommand::ReadAndArchive { .. } => {
            Some(DestructiveAction::Archive)
        }
        MutationCommand::Route { archive, .. } => Some(if *archive {
            DestructiveAction::Archive
        } else {
            DestructiveAction::Move
        }),
        MutationCommand::Trash { .. } => Some(DestructiveAction::Trash),
        MutationCommand::Spam { .. } => Some(DestructiveAction::Spam),
        MutationCommand::Move { .. } => Some(DestructiveAction::Move),
        // Reversible / benign mailbox mutations: not per-action scoped.
        MutationCommand::Star { .. }
        | MutationCommand::SetRead { .. }
        | MutationCommand::ModifyLabels { .. } => None,
    }
}

async fn enforce_account_allowlist(
    state: &Arc<AppState>,
    profile_name: &str,
    profile: &AgentProfileConfig,
    req: &Request,
) -> Result<(), String> {
    match request_account_scope(state, req).await? {
        RequestAccountScope::None => Ok(()),
        RequestAccountScope::AnyAccount => Err(format!(
            "Request `{}` rejected by {profile_name} profile account allowlist; specify an allowed account",
            request_kind(req)
        )),
        RequestAccountScope::AccountKeys(keys) => {
            for key in keys {
                if !account_token_allowed(profile, &key) {
                    return Err(format!(
                        "Request `{}` rejected by {profile_name} profile account allowlist",
                        request_kind(req)
                    ));
                }
            }
            Ok(())
        }
        RequestAccountScope::Accounts(account_ids) => {
            for account_id in account_ids {
                if !account_id_allowed(state, profile, &account_id).await? {
                    return Err(format!(
                        "Request `{}` rejected by {profile_name} profile account allowlist",
                        request_kind(req)
                    ));
                }
            }
            Ok(())
        }
    }
}

#[derive(Debug)]
enum RequestAccountScope {
    None,
    AnyAccount,
    AccountKeys(Vec<String>),
    Accounts(Vec<mxr_core::AccountId>),
}

async fn request_account_scope(
    state: &Arc<AppState>,
    req: &Request,
) -> Result<RequestAccountScope, String> {
    if let Some(key) = request_account_key(req) {
        return Ok(RequestAccountScope::AccountKeys(vec![key.to_string()]));
    }
    if let Some(account_id) = request_account_id(req) {
        return Ok(RequestAccountScope::Accounts(vec![account_id.clone()]));
    }

    match req {
        Request::ListAccounts
        | Request::ListAccountsConfig
        | Request::ListDrafts
        | Request::ListOrphanedDrafts
        | Request::ListSnoozed
        | Request::ListReplyQueue
        | Request::ListEnvelopes {
            account_id: None, ..
        }
        | Request::ListThreads {
            account_id: None, ..
        }
        | Request::ListLabels { account_id: None }
        | Request::Search {
            account_id: None, ..
        }
        | Request::TriageSearch {
            account_id: None, ..
        }
        | Request::Count {
            account_id: None, ..
        }
        | Request::SearchAggregation {
            account_id: None, ..
        }
        | Request::ListSubscriptions {
            account_id: None, ..
        }
        | Request::ListDeliveries {
            account_id: None, ..
        }
        | Request::ScanDeliveries {
            account_id: None, ..
        }
        | Request::ListSenders {
            account_id: None, ..
        }
        | Request::ListStorageBreakdown {
            account_id: None, ..
        }
        | Request::ListLargestMessages {
            account_id: None, ..
        }
        | Request::SyncNow { account_id: None }
        | Request::UnsubscribePurge {
            account_id: None, ..
        }
        | Request::ArchiveAsk {
            filters: ArchiveAskFiltersData {
                account_id: None, ..
            },
            ..
        } => Ok(RequestAccountScope::AnyAccount),
        Request::GetEnvelope { message_id }
        | Request::GetBody { message_id }
        | Request::GetInvite { message_id }
        | Request::GetHeaders { message_id }
        | Request::GetHtmlImageAssets { message_id, .. }
        | Request::DownloadAttachment { message_id, .. }
        | Request::OpenAttachment { message_id, .. }
        | Request::Unsubscribe { message_id }
        | Request::Snooze { message_id, .. }
        | Request::Unsnooze { message_id }
        | Request::SetReplyLater { message_id, .. }
        | Request::PrepareReply { message_id, .. }
        | Request::PrepareForward { message_id }
        | Request::RespondInvite { message_id, .. }
        | Request::PrepareInviteResponse { message_id, .. }
        | Request::MarkInviteAnswered { message_id, .. } => {
            envelope_account_scope(state, std::slice::from_ref(message_id)).await
        }
        Request::ListEnvelopesByIds { message_ids } | Request::ListBodies { message_ids } => {
            envelope_account_scope(state, message_ids).await
        }
        Request::Mutation { mutation, .. } | Request::StartMutationJob { mutation, .. } => {
            mutation_account_scope(state, mutation).await
        }
        Request::GetThread { thread_id }
        | Request::SummarizeThread { thread_id }
        | Request::ExportThread { thread_id, .. } => {
            let thread = state
                .store
                .get_thread(thread_id)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("Thread not found: {thread_id}"))?;
            Ok(RequestAccountScope::Accounts(vec![thread.account_id]))
        }
        Request::DraftCompose {
            thread_id: Some(thread_id),
            ..
        } => {
            let thread = state
                .store
                .get_thread(thread_id)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("Thread not found: {thread_id}"))?;
            Ok(RequestAccountScope::Accounts(vec![thread.account_id]))
        }
        Request::DraftCompose {
            source_message_id: Some(message_id),
            ..
        } => envelope_account_scope(state, std::slice::from_ref(message_id)).await,
        Request::SendStoredDraft { draft_id, .. }
        | Request::DeleteDraft { draft_id }
        | Request::GetDraft { draft_id }
        | Request::ScheduleSend { draft_id, .. }
        | Request::CancelScheduledSend { draft_id } => draft_account_scope(state, draft_id).await,
        Request::DraftRefine { draft_id, .. } => draft_account_scope(state, draft_id).await,
        _ => Ok(RequestAccountScope::None),
    }
}

async fn envelope_account_scope(
    state: &Arc<AppState>,
    message_ids: &[mxr_core::MessageId],
) -> Result<RequestAccountScope, String> {
    let mut accounts = Vec::new();
    for message_id in message_ids {
        let envelope = state
            .store
            .get_envelope(message_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Message not found: {message_id}"))?;
        push_unique_account(&mut accounts, envelope.account_id);
    }
    Ok(RequestAccountScope::Accounts(accounts))
}

async fn mutation_account_scope(
    state: &Arc<AppState>,
    mutation: &MutationCommand,
) -> Result<RequestAccountScope, String> {
    match mutation {
        MutationCommand::Archive { message_ids }
        | MutationCommand::ReadAndArchive { message_ids }
        | MutationCommand::Trash { message_ids }
        | MutationCommand::Spam { message_ids }
        | MutationCommand::Star { message_ids, .. }
        | MutationCommand::SetRead { message_ids, .. }
        | MutationCommand::ModifyLabels { message_ids, .. }
        | MutationCommand::Move { message_ids, .. }
        | MutationCommand::Route { message_ids, .. } => {
            envelope_account_scope(state, message_ids).await
        }
    }
}

async fn draft_account_scope(
    state: &Arc<AppState>,
    draft_id: &mxr_core::DraftId,
) -> Result<RequestAccountScope, String> {
    let draft = state
        .store
        .get_draft(draft_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Draft not found: {draft_id}"))?;
    Ok(RequestAccountScope::Accounts(vec![draft.account_id]))
}

fn push_unique_account(accounts: &mut Vec<mxr_core::AccountId>, account_id: mxr_core::AccountId) {
    if !accounts.iter().any(|existing| existing == &account_id) {
        accounts.push(account_id);
    }
}

async fn account_id_allowed(
    state: &Arc<AppState>,
    profile: &AgentProfileConfig,
    account_id: &mxr_core::AccountId,
) -> Result<bool, String> {
    let account_id_token = account_id.as_str();
    if account_token_allowed(profile, &account_id_token) {
        return Ok(true);
    }

    let Some(account) = state
        .store
        .get_account(account_id)
        .await
        .map_err(|e| e.to_string())?
    else {
        return Ok(false);
    };

    if account_token_allowed(profile, &account.email) {
        return Ok(true);
    }
    if let Some(sync) = &account.sync_backend {
        if account_token_allowed(profile, &sync.config_key) {
            return Ok(true);
        }
    }
    if let Some(send) = &account.send_backend {
        if account_token_allowed(profile, &send.config_key) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn account_token_allowed(profile: &AgentProfileConfig, token: &str) -> bool {
    profile
        .allowed_accounts
        .iter()
        .any(|allowed| allowed == token || allowed.eq_ignore_ascii_case(token))
}

/// Safety class for an IPC request. Every `Request` variant maps to
/// exactly one class via an exhaustive match — adding a new request
/// variant is a compile error until it is classified here, which is
/// the whole point: a new mutating request can never silently slip
/// through a `ReadOnly`/`DraftOnly` policy because someone forgot to
/// add it to an allowlist.
///
/// Classes, from least to most authority:
/// - `Read`: no user-visible state change and no significant side
///   effect. Pure DB reads, exports, previews, and local AI analysis.
///   Remote-fetching requests (tracking-pixel risk) and requests that
///   persist anything are deliberately NOT `Read`.
/// - `DraftOnly`: creates or deletes a *local* draft.
/// - `Send`: transmits mail to a provider.
/// - `Mutate`: reversible local/provider state change that isn't a
///   send and isn't destructive (config, rules, snippets, signatures,
///   snooze, reminders, semantic, screener, activity controls).
/// - `Destructive`: irreversible or provider-destructive mailbox /
///   account / data mutation (batch mailbox mutations, label/account
///   removal, unsubscribe, activity redaction/pruning).
/// - `Admin`: daemon lifecycle (shutdown).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RequestClass {
    Read,
    DraftOnly,
    Send,
    Mutate,
    Destructive,
    Admin,
}

fn classify_request(req: &Request) -> RequestClass {
    use RequestClass::{Admin, Destructive, DraftOnly, Mutate, Read, Send};
    match req {
        // --- Read: pure reads, exports, previews, local AI analysis ---
        Request::ListEnvelopes { .. }
        | Request::ListEnvelopesByIds { .. }
        | Request::GetEnvelope { .. }
        | Request::GetBody { .. }
        | Request::GetInvite { .. }
        | Request::ListInvites { .. }
        | Request::PrepareInviteResponse { .. }
        | Request::ListBodies { .. }
        | Request::GetThread { .. }
        | Request::ListThreads { .. }
        | Request::ListLabels { .. }
        | Request::ListRules
        | Request::ListAccounts
        | Request::ListAccountsConfig
        | Request::GetAuthSession { .. }
        | Request::GetRule { .. }
        | Request::GetRuleForm { .. }
        | Request::DryRunRules { .. }
        | Request::ListEvents { .. }
        | Request::GetLogs { .. }
        | Request::GetDoctorReport
        | Request::GenerateBugReport { .. }
        | Request::ListRuleHistory { .. }
        | Request::Search { .. }
        | Request::GetSyncStatus { .. }
        | Request::Count { .. }
        | Request::SearchAggregation { .. }
        | Request::GetHeaders { .. }
        | Request::ListSavedSearches
        | Request::ListSavedSearchUnreadCounts
        | Request::ListSubscriptions { .. }
        | Request::ListStorageBreakdown { .. }
        | Request::ListLargestMessages { .. }
        | Request::Wrapped { .. }
        | Request::ListStaleThreads { .. }
        | Request::ListContactAsymmetry { .. }
        | Request::ListContactDecay { .. }
        | Request::ListResponseTime { .. }
        | Request::ListAccountAddresses { .. }
        | Request::GetLlmStatus
        | Request::GetLlmConfig
        | Request::GetNotificationChimes
        | Request::GetSemanticStatus
        | Request::RunSavedSearch { .. }
        | Request::ListJobs
        | Request::GetJob { .. }
        | Request::ListSnoozed
        | Request::ListReplyQueue
        | Request::ListSnippets
        | Request::ListDeliveries { .. }
        | Request::GetDelivery { .. }
        | Request::ListSignatures
        | Request::ListSignatureDefaults
        | Request::ResolveSignature { .. }
        | Request::GetSenderProfile { .. }
        | Request::ListSenders { .. }
        | Request::GetRelationshipProfile { .. }
        | Request::ListCommitments { .. }
        | Request::GetUserVoice { .. }
        | Request::HumanizerScore { .. }
        | Request::HumanizerRewrite { .. }
        | Request::ListScreenerQueue { .. }
        | Request::ListScreenerDecisions { .. }
        | Request::SummarizeThread { .. }
        | Request::TriageSearch { .. }
        | Request::DraftCompose { .. }
        | Request::DraftRefine { .. }
        | Request::PrepareReply { .. }
        | Request::PrepareForward { .. }
        | Request::ListOwedReplies { .. }
        | Request::ListDecisionLog { .. }
        | Request::GetDecision { .. }
        | Request::SendTimeRecommendation { .. }
        | Request::GetThreadBriefing { .. }
        | Request::GetRecipientBriefing { .. }
        | Request::SuggestCollaborators { .. }
        | Request::FindExpert { .. }
        | Request::ExplainEntity { .. }
        | Request::ListCadenceWatch { .. }
        | Request::ListCadenceDrift { .. }
        | Request::ListDrafts
        | Request::ListOrphanedDrafts
        | Request::GetDraft { .. }
        | Request::ExportThread { .. }
        | Request::ExportSearch { .. }
        | Request::GetStatus
        | Request::Ping
        | Request::ListEventCategories
        | Request::CountEvents { .. }
        | Request::ListActivity { .. }
        | Request::CountActivity { .. }
        | Request::ActivityStats { .. }
        | Request::ExportActivity { .. }
        | Request::ListSavedActivityFilters
        | Request::GetSavedActivityFilter { .. } => Read,

        // --- DraftOnly: local draft create/update/delete ---
        Request::SaveDraft { .. } | Request::UpdateDraft { .. } | Request::DeleteDraft { .. } => {
            DraftOnly
        }

        // --- Send: transmits mail to a provider ---
        Request::SendDraft { .. }
        | Request::SendStoredDraft { .. }
        | Request::ScheduleSend { .. }
        | Request::RespondInvite { dry_run: false, .. } => Send,

        // --- Destructive: irreversible / provider-destructive ---
        Request::Mutation { .. }
        | Request::StartMutationJob { .. }
        | Request::DeleteLabel { .. }
        | Request::RemoveAccountConfig { .. }
        | Request::Unsubscribe { .. }
        | Request::UnsubscribePurge { .. }
        | Request::RedactActivity { .. }
        | Request::PruneActivity { .. } => Destructive,

        // --- Admin: daemon lifecycle ---
        Request::Shutdown => Admin,

        // --- Mutate: everything else (reversible state change) ---
        // A dry-run RSVP is the same verb as a real RSVP, so it stays
        // out of `Read`; `GetHtmlImageAssets`/attachment fetches do
        // remote egress and are deliberately not `Read` either.
        Request::RespondInvite { dry_run: true, .. }
        | Request::BackfillCalendarInvites { .. }
        | Request::MarkInviteAnswered { .. }
        | Request::GetHtmlImageAssets { .. }
        | Request::DownloadAttachment { .. }
        | Request::OpenAttachment { .. }
        | Request::CreateLabel { .. }
        | Request::RenameLabel { .. }
        | Request::AuthorizeAccountConfig { .. }
        | Request::StartAuthSession { .. }
        | Request::CancelAuthSession { .. }
        | Request::CompleteAuthSession { .. }
        | Request::UpsertAccountConfig { .. }
        | Request::SetDefaultAccount { .. }
        | Request::TestAccountConfig { .. }
        | Request::DisableAccountConfig { .. }
        | Request::RepairAccountConfig { .. }
        | Request::UpsertRule { .. }
        | Request::UpsertRuleForm { .. }
        | Request::DeleteRule { .. }
        | Request::SyncNow { .. }
        | Request::SetFlags { .. }
        | Request::RefreshContacts
        | Request::RebuildAnalytics
        | Request::RecomputeLinkCounts
        | Request::AddAccountAddress { .. }
        | Request::RemoveAccountAddress { .. }
        | Request::SetPrimaryAccountAddress { .. }
        | Request::UpdateLlmConfig { .. }
        | Request::UpdateNotificationChimes { .. }
        | Request::PreviewNotificationChime { .. }
        | Request::EnableSemantic { .. }
        | Request::InstallSemanticProfile { .. }
        | Request::UseSemanticProfile { .. }
        | Request::ReindexSemantic
        | Request::BackfillSemantic
        | Request::CreateSavedSearch { .. }
        | Request::DeleteSavedSearch { .. }
        | Request::UpdateSavedSearch { .. }
        | Request::UndoMutation { .. }
        | Request::Snooze { .. }
        | Request::Unsnooze { .. }
        | Request::SetReplyLater { .. }
        | Request::SetAutoReminder { .. }
        | Request::CancelAutoReminder { .. }
        | Request::CancelScheduledSend { .. }
        | Request::SetSnippet { .. }
        | Request::DeleteSnippet { .. }
        | Request::ResolveDelivery { .. }
        | Request::DismissDelivery { .. }
        | Request::ScanDeliveries { .. }
        | Request::SetSignature { .. }
        | Request::DeleteSignature { .. }
        | Request::SetSignatureDefault { .. }
        | Request::ClearSignatureDefault { .. }
        | Request::RebuildRelationshipProfile { .. }
        | Request::ResolveCommitment { .. }
        | Request::RebuildUserVoice { .. }
        | Request::SetScreenerDecision { .. }
        | Request::ClearScreenerDecision { .. }
        | Request::CheckDraftSafety { .. }
        | Request::ExtractDraftCommitments { .. }
        | Request::ArchiveAsk { .. }
        | Request::RebuildDecisionLog { .. }
        | Request::WatchCadence { .. }
        | Request::UnwatchCadence { .. }
        | Request::SaveDraftToServer { .. }
        | Request::ResetOrphanedDraft { .. }
        | Request::PauseActivity { .. }
        | Request::ResumeActivity
        | Request::UpsertSavedActivityFilter { .. }
        | Request::DeleteSavedActivityFilter { .. } => Mutate,
    }
}

fn request_requires_send_capability(req: &Request) -> bool {
    matches!(classify_request(req), RequestClass::Send)
}

/// The destructive capability gates everything that is not a read, a
/// local-draft edit, or a send. This intentionally spans `Mutate`,
/// `Destructive`, and `Admin` so the gate's behaviour is unchanged
/// from the previous hand-rolled definition; PR 2.1 will split the
/// gate per granular scope using the finer `RequestClass`.
fn request_requires_destructive_capability(req: &Request) -> bool {
    matches!(
        classify_request(req),
        RequestClass::Mutate | RequestClass::Destructive | RequestClass::Admin
    )
}

fn request_is_draft_only(req: &Request) -> bool {
    matches!(classify_request(req), RequestClass::DraftOnly)
}

fn request_is_read_only(req: &Request) -> bool {
    matches!(classify_request(req), RequestClass::Read)
}

fn request_kind(req: &Request) -> &'static str {
    match req {
        Request::ListEnvelopes { .. } => "list_envelopes",
        Request::ListEnvelopesByIds { .. } => "list_envelopes_by_ids",
        Request::GetEnvelope { .. } => "get_envelope",
        Request::GetBody { .. } => "get_body",
        Request::GetInvite { .. } => "get_invite",
        Request::ListInvites { .. } => "list_invites",
        Request::BackfillCalendarInvites { .. } => "backfill_calendar_invites",
        Request::RespondInvite { .. } => "respond_invite",
        Request::PrepareInviteResponse { .. } => "prepare_invite_response",
        Request::MarkInviteAnswered { .. } => "mark_invite_answered",
        Request::GetHtmlImageAssets { .. } => "get_html_image_assets",
        Request::DownloadAttachment { .. } => "download_attachment",
        Request::OpenAttachment { .. } => "open_attachment",
        Request::ListBodies { .. } => "list_bodies",
        Request::GetThread { .. } => "get_thread",
        Request::ListThreads { .. } => "list_threads",
        Request::ListLabels { .. } => "list_labels",
        Request::CreateLabel { .. } => "create_label",
        Request::DeleteLabel { .. } => "delete_label",
        Request::RenameLabel { .. } => "rename_label",
        Request::ListRules => "list_rules",
        Request::ListAccounts => "list_accounts",
        Request::ListAccountsConfig => "list_accounts_config",
        Request::AuthorizeAccountConfig { .. } => "authorize_account_config",
        Request::StartAuthSession { .. } => "start_auth_session",
        Request::GetAuthSession { .. } => "get_auth_session",
        Request::CancelAuthSession { .. } => "cancel_auth_session",
        Request::CompleteAuthSession { .. } => "complete_auth_session",
        Request::UpsertAccountConfig { .. } => "upsert_account_config",
        Request::SetDefaultAccount { .. } => "set_default_account",
        Request::TestAccountConfig { .. } => "test_account_config",
        Request::DisableAccountConfig { .. } => "disable_account_config",
        Request::RemoveAccountConfig { .. } => "remove_account_config",
        Request::RepairAccountConfig { .. } => "repair_account_config",
        Request::GetRule { .. } => "get_rule",
        Request::GetRuleForm { .. } => "get_rule_form",
        Request::UpsertRule { .. } => "upsert_rule",
        Request::UpsertRuleForm { .. } => "upsert_rule_form",
        Request::DeleteRule { .. } => "delete_rule",
        Request::DryRunRules { .. } => "dry_run_rules",
        Request::ListEvents { .. } => "list_events",
        Request::GetLogs { .. } => "get_logs",
        Request::GetDoctorReport => "get_doctor_report",
        Request::GenerateBugReport { .. } => "generate_bug_report",
        Request::ListRuleHistory { .. } => "list_rule_history",
        Request::Search { .. } => "search",
        Request::SyncNow { .. } => "sync_now",
        Request::GetSyncStatus { .. } => "get_sync_status",
        Request::SetFlags { .. } => "set_flags",
        Request::Count { .. } => "count",
        Request::SearchAggregation { .. } => "search_aggregation",
        Request::GetHeaders { .. } => "get_headers",
        Request::ListSavedSearches => "list_saved_searches",
        Request::ListSavedSearchUnreadCounts => "list_saved_search_unread_counts",
        Request::ListSubscriptions { .. } => "list_subscriptions",
        Request::ListStorageBreakdown { .. } => "list_storage_breakdown",
        Request::ListLargestMessages { .. } => "list_largest_messages",
        Request::Wrapped { .. } => "wrapped",
        Request::ListStaleThreads { .. } => "list_stale_threads",
        Request::ListContactAsymmetry { .. } => "list_contact_asymmetry",
        Request::ListContactDecay { .. } => "list_contact_decay",
        Request::RefreshContacts => "refresh_contacts",
        Request::RebuildAnalytics => "rebuild_analytics",
        Request::RecomputeLinkCounts => "recompute_link_counts",
        Request::ListResponseTime { .. } => "list_response_time",
        Request::ListAccountAddresses { .. } => "list_account_addresses",
        Request::AddAccountAddress { .. } => "add_account_address",
        Request::RemoveAccountAddress { .. } => "remove_account_address",
        Request::SetPrimaryAccountAddress { .. } => "set_primary_account_address",
        Request::GetLlmStatus => "get_llm_status",
        Request::GetLlmConfig => "get_llm_config",
        Request::UpdateLlmConfig { .. } => "update_llm_config",
        Request::GetNotificationChimes => "get_notification_chimes",
        Request::UpdateNotificationChimes { .. } => "update_notification_chimes",
        Request::PreviewNotificationChime { .. } => "preview_notification_chime",
        Request::GetSemanticStatus => "get_semantic_status",
        Request::EnableSemantic { .. } => "enable_semantic",
        Request::InstallSemanticProfile { .. } => "install_semantic_profile",
        Request::UseSemanticProfile { .. } => "use_semantic_profile",
        Request::ReindexSemantic => "reindex_semantic",
        Request::BackfillSemantic => "backfill_semantic",
        Request::CreateSavedSearch { .. } => "create_saved_search",
        Request::DeleteSavedSearch { .. } => "delete_saved_search",
        Request::UpdateSavedSearch { .. } => "update_saved_search",
        Request::RunSavedSearch { .. } => "run_saved_search",
        Request::Mutation { mutation: cmd, .. } => mutation_kind(cmd),
        Request::StartMutationJob { mutation: cmd, .. } => mutation_kind(cmd),
        Request::ListJobs => "list_jobs",
        Request::GetJob { .. } => "get_job",
        Request::UndoMutation { .. } => "undo_mutation",
        Request::Unsubscribe { .. } => "unsubscribe",
        Request::UnsubscribePurge { .. } => "unsubscribe_purge",
        Request::Snooze { .. } => "snooze",
        Request::Unsnooze { .. } => "unsnooze",
        Request::ListSnoozed => "list_snoozed",
        Request::SetReplyLater { .. } => "set_reply_later",
        Request::ListReplyQueue => "list_reply_queue",
        Request::SetAutoReminder { .. } => "set_auto_reminder",
        Request::CancelAutoReminder { .. } => "cancel_auto_reminder",
        Request::ScheduleSend { .. } => "schedule_send",
        Request::CancelScheduledSend { .. } => "cancel_scheduled_send",
        Request::ListSnippets => "list_snippets",
        Request::SetSnippet { .. } => "set_snippet",
        Request::DeleteSnippet { .. } => "delete_snippet",
        Request::ListDeliveries { .. } => "list_deliveries",
        Request::GetDelivery { .. } => "get_delivery",
        Request::ResolveDelivery { .. } => "resolve_delivery",
        Request::DismissDelivery { .. } => "dismiss_delivery",
        Request::ScanDeliveries { .. } => "scan_deliveries",
        Request::ListSignatures => "list_signatures",
        Request::ListSignatureDefaults => "list_signature_defaults",
        Request::SetSignature { .. } => "set_signature",
        Request::DeleteSignature { .. } => "delete_signature",
        Request::SetSignatureDefault { .. } => "set_signature_default",
        Request::ClearSignatureDefault { .. } => "clear_signature_default",
        Request::ResolveSignature { .. } => "resolve_signature",
        Request::GetSenderProfile { .. } => "get_sender_profile",
        Request::ListSenders { .. } => "list_senders",
        Request::GetRelationshipProfile { .. } => "get_relationship_profile",
        Request::RebuildRelationshipProfile { .. } => "rebuild_relationship_profile",
        Request::ListCommitments { .. } => "list_commitments",
        Request::ResolveCommitment { .. } => "resolve_commitment",
        Request::GetUserVoice { .. } => "get_user_voice",
        Request::RebuildUserVoice { .. } => "rebuild_user_voice",
        Request::HumanizerScore { .. } => "humanizer_score",
        Request::HumanizerRewrite { .. } => "humanizer_rewrite",
        Request::ListScreenerQueue { .. } => "list_screener_queue",
        Request::ListScreenerDecisions { .. } => "list_screener_decisions",
        Request::SetScreenerDecision { .. } => "set_screener_decision",
        Request::ClearScreenerDecision { .. } => "clear_screener_decision",
        Request::SummarizeThread { .. } => "summarize_thread",
        Request::TriageSearch { .. } => "triage_search",
        Request::DraftCompose { .. } => "draft_compose",
        Request::DraftRefine { .. } => "draft_refine",
        Request::PrepareReply { .. } => "prepare_reply",
        Request::PrepareForward { .. } => "prepare_forward",
        Request::SendDraft { .. } => "send_draft",
        Request::SaveDraft { .. } => "save_draft",
        Request::SendStoredDraft { .. } => "send_stored_draft",
        Request::CheckDraftSafety { .. } => "check_draft_safety",
        Request::ExtractDraftCommitments { .. } => "extract_draft_commitments",
        Request::ListOwedReplies { .. } => "list_owed_replies",
        Request::ArchiveAsk { .. } => "archive_ask",
        Request::ListDecisionLog { .. } => "list_decision_log",
        Request::GetDecision { .. } => "get_decision",
        Request::RebuildDecisionLog { .. } => "rebuild_decision_log",
        Request::SendTimeRecommendation { .. } => "send_time_recommendation",
        Request::GetThreadBriefing { .. } => "get_thread_briefing",
        Request::GetRecipientBriefing { .. } => "get_recipient_briefing",
        Request::SuggestCollaborators { .. } => "suggest_collaborators",
        Request::FindExpert { .. } => "find_expert",
        Request::ExplainEntity { .. } => "explain_entity",
        Request::WatchCadence { .. } => "watch_cadence",
        Request::UnwatchCadence { .. } => "unwatch_cadence",
        Request::ListCadenceWatch { .. } => "list_cadence_watch",
        Request::ListCadenceDrift { .. } => "list_cadence_drift",
        Request::DeleteDraft { .. } => "delete_draft",
        Request::GetDraft { .. } => "get_draft",
        Request::UpdateDraft { .. } => "update_draft",
        Request::SaveDraftToServer { .. } => "save_draft_to_server",
        Request::ListDrafts => "list_drafts",
        Request::ListOrphanedDrafts => "list_orphaned_drafts",
        Request::ResetOrphanedDraft { .. } => "reset_orphaned_draft",
        Request::ExportThread { .. } => "export_thread",
        Request::ExportSearch { .. } => "export_search",
        Request::GetStatus => "get_status",
        Request::Ping => "ping",
        Request::Shutdown => "shutdown",
        Request::ListEventCategories => "list_event_categories",
        Request::CountEvents { .. } => "count_events",
        Request::ListActivity { .. } => "list_activity",
        Request::CountActivity { .. } => "count_activity",
        Request::ActivityStats { .. } => "activity_stats",
        Request::ExportActivity { .. } => "export_activity",
        Request::RedactActivity { .. } => "redact_activity",
        Request::PruneActivity { .. } => "prune_activity",
        Request::PauseActivity { .. } => "pause_activity",
        Request::ResumeActivity => "resume_activity",
        Request::ListSavedActivityFilters => "list_saved_activity_filters",
        Request::GetSavedActivityFilter { .. } => "get_saved_activity_filter",
        Request::UpsertSavedActivityFilter { .. } => "upsert_saved_activity_filter",
        Request::DeleteSavedActivityFilter { .. } => "delete_saved_activity_filter",
    }
}

fn mutation_kind(cmd: &MutationCommand) -> &'static str {
    match cmd {
        MutationCommand::Archive { .. } => "mutation.archive",
        MutationCommand::ReadAndArchive { .. } => "mutation.read_and_archive",
        MutationCommand::Trash { .. } => "mutation.trash",
        MutationCommand::Spam { .. } => "mutation.spam",
        MutationCommand::Star { .. } => "mutation.star",
        MutationCommand::SetRead { .. } => "mutation.set_read",
        MutationCommand::ModifyLabels { .. } => "mutation.modify_labels",
        MutationCommand::Move { .. } => "mutation.move",
        MutationCommand::Route { .. } => "mutation.route",
    }
}

fn request_account_id(req: &Request) -> Option<&mxr_core::AccountId> {
    match req {
        Request::ListEnvelopes { account_id, .. }
        | Request::ListThreads { account_id, .. }
        | Request::ListLabels { account_id }
        | Request::DeleteLabel { account_id, .. }
        | Request::CreateLabel { account_id, .. }
        | Request::RenameLabel { account_id, .. }
        | Request::Search { account_id, .. }
        | Request::Count { account_id, .. }
        | Request::SearchAggregation { account_id, .. }
        | Request::ListSubscriptions { account_id, .. }
        | Request::ListInvites { account_id, .. }
        | Request::BackfillCalendarInvites { account_id }
        | Request::ListDeliveries { account_id, .. }
        | Request::ScanDeliveries { account_id, .. }
        | Request::ListSenders { account_id, .. }
        | Request::ListStorageBreakdown { account_id, .. }
        | Request::ListLargestMessages { account_id, .. }
        | Request::Wrapped { account_id, .. }
        | Request::ListStaleThreads { account_id, .. }
        | Request::ListContactAsymmetry { account_id, .. }
        | Request::ListContactDecay { account_id, .. }
        | Request::ListResponseTime { account_id, .. }
        | Request::SyncNow { account_id } => account_id.as_ref(),
        Request::ListAccountAddresses { account_id }
        | Request::AddAccountAddress { account_id, .. }
        | Request::RemoveAccountAddress { account_id, .. }
        | Request::SetPrimaryAccountAddress { account_id, .. }
        | Request::GetRelationshipProfile { account_id, .. }
        | Request::RebuildRelationshipProfile { account_id, .. }
        | Request::ListCommitments { account_id, .. }
        | Request::ListOwedReplies { account_id, .. }
        | Request::ListDecisionLog { account_id, .. }
        | Request::RebuildDecisionLog { account_id, .. }
        | Request::SendTimeRecommendation { account_id, .. }
        | Request::FindExpert { account_id, .. }
        | Request::ExplainEntity { account_id, .. }
        | Request::WatchCadence { account_id, .. }
        | Request::UnwatchCadence { account_id, .. }
        | Request::ListCadenceWatch { account_id }
        | Request::ListCadenceDrift { account_id }
        | Request::GetRecipientBriefing { account_id, .. }
        | Request::GetUserVoice { account_id }
        | Request::RebuildUserVoice { account_id } => Some(account_id),
        Request::DraftCompose { account_id, .. } => account_id.as_ref(),
        Request::SetSignatureDefault { account_id, .. }
        | Request::ClearSignatureDefault { account_id, .. }
        | Request::ResolveSignature { account_id, .. } => account_id.as_ref(),
        Request::GetSyncStatus { account_id } => Some(account_id),
        Request::SendDraft { draft, .. }
        | Request::SaveDraft { draft }
        | Request::UpdateDraft { draft }
        | Request::SaveDraftToServer { draft }
        | Request::CheckDraftSafety { draft, .. }
        | Request::ExtractDraftCommitments { draft }
        | Request::SuggestCollaborators { draft, .. } => Some(&draft.account_id),
        Request::SendStoredDraft { .. } | Request::DeleteDraft { .. } => None,
        Request::ArchiveAsk { filters, .. } => filters.account_id.as_ref(),
        _ => None,
    }
}

fn request_account_key(req: &Request) -> Option<&str> {
    match req {
        Request::AuthorizeAccountConfig { account, .. }
        | Request::StartAuthSession { account, .. }
        | Request::UpsertAccountConfig { account }
        | Request::TestAccountConfig { account } => Some(account.key.as_str()),
        Request::SetDefaultAccount { key } => Some(key.as_str()),
        Request::DisableAccountConfig { key } => Some(key.as_str()),
        Request::RemoveAccountConfig { key, .. } => Some(key.as_str()),
        Request::RepairAccountConfig { account } => Some(account.key.as_str()),
        _ => None,
    }
}
fn build_reply_references(envelope: &mxr_core::types::Envelope) -> Vec<String> {
    let mut references = envelope.references.clone();
    if let Some(message_id) = &envelope.message_id_header {
        if !references.iter().any(|reference| reference == message_id) {
            references.push(message_id.clone());
        }
    }
    references
}

/// Build an ExportThread from a thread_id by fetching envelopes and bodies from the store.
async fn build_export_thread(
    state: &AppState,
    thread_id: &mxr_core::ThreadId,
) -> Result<ExportThread, String> {
    let thread = state
        .store
        .get_thread(thread_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Thread not found: {thread_id}"))?;

    let envelopes = state
        .store
        .get_thread_envelopes(thread_id)
        .await
        .map_err(|e| e.to_string())?;

    let mut messages = Vec::with_capacity(envelopes.len());
    for env in &envelopes {
        let body = state
            .store
            .get_body(&env.id)
            .await
            .map_err(|e| e.to_string())?;

        messages.push(ExportMessage {
            id: env.id.to_string(),
            from_name: env.from.name.clone(),
            from_email: env.from.email.clone(),
            to: env.to.iter().map(|a| a.email.clone()).collect(),
            date: env.date,
            subject: env.subject.clone(),
            body_text: body.as_ref().and_then(|b| b.text_plain.clone()),
            body_html: body.as_ref().and_then(|b| b.text_html.clone()),
            headers_raw: body.as_ref().and_then(|b| b.metadata.raw_headers.clone()),
            attachments: body
                .as_ref()
                .map(|b| {
                    b.attachments
                        .iter()
                        .map(|a| ExportAttachment {
                            filename: a.filename.clone(),
                            size_bytes: a.size_bytes,
                            local_path: a.local_path.as_ref().map(|p| p.display().to_string()),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        });
    }

    Ok(ExportThread {
        thread_id: thread_id.to_string(),
        subject: thread.subject,
        messages,
    })
}

async fn find_label_by_name(
    state: &AppState,
    account_id: &mxr_core::AccountId,
    name: &str,
) -> Result<mxr_core::Label, String> {
    let labels = state
        .store
        .list_labels_by_account(account_id)
        .await
        .map_err(|e| e.to_string())?;
    labels
        .into_iter()
        .find(|label| label.name == name)
        .ok_or_else(|| format!("Label not found: {name}"))
}

/// Fast path used for reply/forward quoted text. The user is composing a
/// reply; they want the original content quoted, *not* the aggressive
/// reader-view cleaning (which strips signatures, boilerplate, tracking,
/// collapses prior quotes, and runs several regex passes that get
/// slow-to-pathological on big HTML emails).
///
/// Plain text passes through as-is. HTML-only messages get one
/// `html2text` pass. Empty bodies return an empty string.
pub(crate) fn render_reply_quoted_text(body: &mxr_core::types::MessageBody) -> String {
    if let Some(text) = body.text_plain.as_deref() {
        if !text.is_empty() {
            return text.to_string();
        }
    }
    if let Some(html) = body.text_html.as_deref() {
        if !html.is_empty() {
            // plain_no_decorate keeps the pre-0.17 behavior: no
            // markdown-style emphasis markers in rendered context.
            return html2text::config::plain_no_decorate()
                .string_from_read(html.as_bytes(), 80)
                .unwrap_or_default();
        }
    }
    String::new()
}

/// Memoized render of the reply quoted text for `message_id`. Bodies
/// are immutable post-sync, so a hit is correct forever. Misses do
/// the (now cheap) render and insert into the cache.
pub(crate) fn get_or_render_reply_context(
    state: &crate::state::AppState,
    message_id: &mxr_core::MessageId,
    body: &mxr_core::types::MessageBody,
) -> std::sync::Arc<String> {
    {
        let cache = state.reply_context_cache.lock();
        if let Some(cached) = cache.get(message_id) {
            return cached.clone();
        }
    }
    let rendered = std::sync::Arc::new(render_reply_quoted_text(body));
    state
        .reply_context_cache
        .lock()
        .insert(message_id.clone(), rendered.clone());
    rendered
}

async fn populate_envelope_label_provider_ids(
    state: &AppState,
    envelope: &mut mxr_core::types::Envelope,
    labels: &[mxr_core::types::Label],
) -> Result<(), String> {
    let label_ids = state
        .store
        .get_message_label_ids(&envelope.id)
        .await
        .map_err(|e| e.to_string())?;
    envelope.label_provider_ids = labels
        .iter()
        .filter(|label| label_ids.iter().any(|id| id == &label.id))
        .map(|label| label.provider_id.clone())
        .collect();
    Ok(())
}

async fn persist_local_label_changes(
    state: &AppState,
    message_id: &mxr_core::MessageId,
    add: &[String],
    remove: &[String],
) -> Result<(), sqlx::Error> {
    let envelope = state
        .store
        .get_envelope(message_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    let labels = state
        .store
        .list_labels_by_account(&envelope.account_id)
        .await?;
    let mut label_ids = state.store.get_message_label_ids(message_id).await?;

    for label_ref in remove {
        if let Some(label) = labels
            .iter()
            .find(|candidate| candidate.provider_id == *label_ref || candidate.name == *label_ref)
        {
            label_ids.retain(|id| id != &label.id);
        }
    }

    for label_ref in add {
        if let Some(label) = labels
            .iter()
            .find(|candidate| candidate.provider_id == *label_ref || candidate.name == *label_ref)
        {
            if !label_ids.iter().any(|id| id == &label.id) {
                label_ids.push(label.id.clone());
            }
        }
    }

    state
        .store
        .set_message_labels(message_id, &label_ids, mxr_core::EventSource::User)
        .await?;
    state
        .store
        .recalculate_label_counts(&envelope.account_id)
        .await?;
    Ok(())
}

pub(crate) async fn reconcile_label_mutation(
    state: &AppState,
    provider: &dyn MailSyncProvider,
    message_id: &mxr_core::MessageId,
    add: &[String],
    remove: &[String],
) -> Result<(), String> {
    if provider.capabilities().mutate.labels {
        persist_local_label_changes(state, message_id, add, remove)
            .await
            .map_err(|e| e.to_string())
    } else {
        state
            .sync_engine
            .sync_account(provider)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn same_remote_message(candidate: &mxr_core::Envelope, original: &mxr_core::Envelope) -> bool {
    candidate.account_id == original.account_id
        && candidate.message_id_header == original.message_id_header
        && candidate.subject == original.subject
        && candidate.from.email == original.from.email
        && candidate.date == original.date
        && candidate.size_bytes == original.size_bytes
}

async fn find_reconciled_message_id(
    state: &AppState,
    original: &mxr_core::Envelope,
    previous_message_id: &mxr_core::MessageId,
) -> Result<mxr_core::MessageId, String> {
    let mut candidates = if let Some(header) = original.message_id_header.as_deref() {
        state
            .store
            .list_envelopes_by_message_id_header(&original.account_id, header)
            .await
            .map_err(|e| e.to_string())?
    } else {
        state
            .store
            .list_envelopes_by_remote_fingerprint(
                &original.account_id,
                &original.subject,
                &original.from.email,
                original.date,
                original.size_bytes,
            )
            .await
            .map_err(|e| e.to_string())?
    };

    candidates.retain(|candidate| {
        candidate.id != *previous_message_id && same_remote_message(candidate, original)
    });

    match candidates.len() {
        1 => Ok(candidates.remove(0).id),
        0 => Err(format!(
            "Reconciled message not found after folder mutation for {previous_message_id}"
        )),
        _ => Err(format!(
            "Ambiguous reconciled message after folder mutation for {previous_message_id}"
        )),
    }
}

pub(crate) async fn apply_snooze(
    state: &AppState,
    message_id: &mxr_core::MessageId,
    wake_at: &chrono::DateTime<chrono::Utc>,
) -> Result<(), String> {
    let envelope = state
        .store
        .get_envelope(message_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Message not found: {message_id}"))?;
    let provider_id = state
        .store
        .get_provider_id(message_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Missing provider id for message: {message_id}"))?;
    let original_labels = state
        .store
        .get_message_label_ids(message_id)
        .await
        .map_err(|e| e.to_string())?;
    let provider = state.get_provider(Some(&envelope.account_id))?;
    let _provider_guard = state.acquire_provider_operation(&envelope.account_id).await;
    let snooze_mutation_id = uuid::Uuid::now_v7().to_string();
    provider
        .apply_mutation(
            &snooze_mutation_id,
            &mxr_core::Mutation::ModifyLabels {
                provider_message_id: provider_id.clone(),
                add: vec![],
                remove: vec!["INBOX".to_string()],
            },
        )
        .await
        .map_err(|e| e.to_string())?;
    let now = chrono::Utc::now().timestamp();
    if let Err(error) = state
        .store
        .record_mutation_applied(&snooze_mutation_id, &provider_id, &envelope.account_id, now)
        .await
    {
        tracing::warn!(%error, "snooze failed to record dedup row");
    }
    reconcile_label_mutation(
        state,
        provider.as_ref(),
        message_id,
        &[],
        &["INBOX".to_string()],
    )
    .await?;
    let snoozed_message_id = if provider.capabilities().mutate.labels {
        message_id.clone()
    } else {
        find_reconciled_message_id(state, &envelope, message_id).await?
    };
    state
        .store
        .insert_snooze(&Snoozed {
            message_id: snoozed_message_id,
            account_id: envelope.account_id,
            snoozed_at: chrono::Utc::now(),
            wake_at: *wake_at,
            original_labels,
        })
        .await
        .map_err(|e| e.to_string())
}

pub(crate) async fn restore_snoozed_message(
    state: &AppState,
    snoozed: &Snoozed,
) -> Result<(), String> {
    let provider_id = state
        .store
        .get_provider_id(&snoozed.message_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Missing provider id for message: {}", snoozed.message_id))?;
    let labels = state
        .store
        .list_labels_by_account(&snoozed.account_id)
        .await
        .map_err(|e| e.to_string())?;
    let restore_provider_ids: Vec<String> = labels
        .iter()
        .filter(|label| snoozed.original_labels.iter().any(|id| id == &label.id))
        .map(|label| label.provider_id.clone())
        .collect();

    let provider = state.get_provider(Some(&snoozed.account_id))?;
    let _provider_guard = state.acquire_provider_operation(&snoozed.account_id).await;
    let wake_mutation_id = uuid::Uuid::now_v7().to_string();
    provider
        .apply_mutation(
            &wake_mutation_id,
            &mxr_core::Mutation::ModifyLabels {
                provider_message_id: provider_id.clone(),
                add: restore_provider_ids.clone(),
                remove: vec![],
            },
        )
        .await
        .map_err(|e| e.to_string())?;
    let now = chrono::Utc::now().timestamp();
    if let Err(error) = state
        .store
        .record_mutation_applied(&wake_mutation_id, &provider_id, &snoozed.account_id, now)
        .await
    {
        tracing::warn!(%error, "wake-from-snooze failed to record dedup row");
    }
    reconcile_label_mutation(
        state,
        provider.as_ref(),
        &snoozed.message_id,
        &restore_provider_ids,
        &[],
    )
    .await?;
    state
        .store
        .remove_snooze(&snoozed.message_id)
        .await
        .map_err(|e| e.to_string())
}

fn parse_rule_value(value: serde_json::Value) -> Result<Rule, String> {
    serde_json::from_value(value).map_err(|e| e.to_string())
}

async fn build_rule_from_form(
    state: &AppState,
    existing_rule: Option<&String>,
    name: &str,
    condition: &str,
    action: &str,
    priority: i32,
    enabled: bool,
) -> Result<Rule, String> {
    let existing = if let Some(rule) = existing_rule {
        state
            .store
            .get_rule_by_id_or_name(rule)
            .await
            .map_err(|e| e.to_string())?
            .map(|row| {
                serde_json::from_value::<Rule>(mxr_store::row_to_rule_json(&row))
                    .map_err(|e| e.to_string())
            })
            .transpose()?
    } else {
        None
    };

    let now = chrono::Utc::now();
    Ok(Rule {
        id: existing
            .as_ref()
            .map(|rule| rule.id.clone())
            .unwrap_or_default(),
        name: name.to_string(),
        enabled,
        priority,
        conditions: parse_rule_condition_string(condition)?,
        actions: parse_rule_actions_string(action)?,
        created_at: existing.as_ref().map_or(now, |rule| rule.created_at),
        updated_at: now,
    })
}

fn parse_rule_condition_string(input: &str) -> Result<Conditions, String> {
    let ast = parse_query(input).map_err(|e| e.to_string())?;
    query_ast_to_conditions(ast)
}

fn query_ast_to_conditions(node: mxr_search::ast::QueryNode) -> Result<Conditions, String> {
    use mxr_search::ast::{DateBound, DateValue, FilterKind, QueryField, QueryNode, SizeOp};

    Ok(match node {
        QueryNode::And(left, right) => Conditions::And {
            conditions: vec![
                query_ast_to_conditions(*left)?,
                query_ast_to_conditions(*right)?,
            ],
        },
        QueryNode::Or(left, right) => Conditions::Or {
            conditions: vec![
                query_ast_to_conditions(*left)?,
                query_ast_to_conditions(*right)?,
            ],
        },
        QueryNode::Not(node) => Conditions::Not {
            condition: Box::new(query_ast_to_conditions(*node)?),
        },
        QueryNode::Field { field, value } => Conditions::Field(match field {
            QueryField::From => FieldCondition::From {
                pattern: StringMatch::Contains(value),
            },
            QueryField::To => FieldCondition::To {
                pattern: StringMatch::Contains(value),
            },
            QueryField::Subject => FieldCondition::Subject {
                pattern: StringMatch::Contains(value),
            },
            QueryField::Body => FieldCondition::BodyContains {
                pattern: StringMatch::Contains(value),
            },
            QueryField::Cc
            | QueryField::Bcc
            | QueryField::Filename
            | QueryField::List
            | QueryField::DeliveredTo
            | QueryField::Rfc822MsgId => {
                return Err("field is not supported in rules form".to_string())
            }
            _ => return Err("unknown field is not supported in rules form".to_string()),
        }),
        QueryNode::Label(label) => Conditions::Field(FieldCondition::HasLabel { label }),
        QueryNode::Filter(FilterKind::Unread) => Conditions::Field(FieldCondition::IsUnread),
        QueryNode::Filter(FilterKind::Starred) => Conditions::Field(FieldCondition::IsStarred),
        QueryNode::Filter(FilterKind::HasAttachment) => {
            Conditions::Field(FieldCondition::HasAttachment)
        }
        QueryNode::Filter(FilterKind::Read) => Conditions::Not {
            condition: Box::new(Conditions::Field(FieldCondition::IsUnread)),
        },
        QueryNode::Filter(FilterKind::Draft) => Conditions::Field(FieldCondition::HasLabel {
            label: "DRAFT".to_string(),
        }),
        QueryNode::Filter(FilterKind::Sent) => Conditions::Field(FieldCondition::HasLabel {
            label: "SENT".to_string(),
        }),
        QueryNode::Filter(FilterKind::Trash) => Conditions::Field(FieldCondition::HasLabel {
            label: "TRASH".to_string(),
        }),
        QueryNode::Filter(FilterKind::Spam) => Conditions::Field(FieldCondition::HasLabel {
            label: "SPAM".to_string(),
        }),
        QueryNode::Filter(FilterKind::Inbox) => Conditions::Field(FieldCondition::HasLabel {
            label: "INBOX".to_string(),
        }),
        QueryNode::Filter(FilterKind::Archived) => Conditions::Field(FieldCondition::HasLabel {
            label: "ARCHIVE".to_string(),
        }),
        QueryNode::Filter(FilterKind::HasLink) => Conditions::Field(FieldCondition::LinkDensity {
            match_kind: mxr_rules::LinkDensityMatch::Any,
        }),
        QueryNode::Filter(FilterKind::HasLinkHeavy) => {
            Conditions::Field(FieldCondition::LinkDensity {
                match_kind: mxr_rules::LinkDensityMatch::Heavy,
            })
        }
        QueryNode::Filter(FilterKind::NoLinks) => Conditions::Field(FieldCondition::LinkDensity {
            match_kind: mxr_rules::LinkDensityMatch::None,
        }),
        QueryNode::Filter(
            FilterKind::Answered
            | FilterKind::Anywhere
            | FilterKind::HasUserLabels
            | FilterKind::NoUserLabels
            | FilterKind::HasCalendar
            | FilterKind::HasDrive
            | FilterKind::HasDocument
            | FilterKind::HasSpreadsheet
            | FilterKind::HasPresentation
            | FilterKind::HasYoutube
            | FilterKind::HasInlineImage,
        ) => return Err("search filter is not supported in rules form".to_string()),
        // Mxr-specific custom filters route through Custom; rules don't
        // support them (computed dynamically across thread state).
        QueryNode::Filter(FilterKind::Custom(_)) => {
            return Err("search filter is not supported in rules form".to_string())
        }
        QueryNode::Text(value) | QueryNode::Phrase(value) => {
            Conditions::Field(FieldCondition::BodyContains {
                pattern: StringMatch::Contains(value),
            })
        }
        QueryNode::DateRange { bound, date } => {
            let date = match date {
                DateValue::Specific(date) => {
                    chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
                        date.and_hms_opt(0, 0, 0)
                            .ok_or_else(|| "invalid date".to_string())?,
                        chrono::Utc,
                    )
                }
                _ => return Err("relative dates are not supported in rules form".to_string()),
            };
            match bound {
                DateBound::After => Conditions::Field(FieldCondition::DateAfter { date }),
                DateBound::Before => Conditions::Field(FieldCondition::DateBefore { date }),
                DateBound::Exact => Conditions::And {
                    conditions: vec![
                        Conditions::Field(FieldCondition::DateAfter { date }),
                        Conditions::Field(FieldCondition::DateBefore {
                            date: date + chrono::Duration::days(1),
                        }),
                    ],
                },
                _ => return Err("unknown date bound is not supported in rules form".to_string()),
            }
        }
        QueryNode::Size { op, bytes } => match op {
            SizeOp::GreaterThan => Conditions::Field(FieldCondition::SizeGreaterThan { bytes }),
            SizeOp::GreaterThanOrEqual => Conditions::Field(FieldCondition::SizeGreaterThan {
                bytes: bytes.saturating_sub(1),
            }),
            SizeOp::LessThan => Conditions::Field(FieldCondition::SizeLessThan { bytes }),
            SizeOp::LessThanOrEqual => Conditions::Field(FieldCondition::SizeLessThan {
                bytes: bytes.saturating_add(1),
            }),
            SizeOp::Equal => Conditions::And {
                conditions: vec![
                    Conditions::Field(FieldCondition::SizeGreaterThan {
                        bytes: bytes.saturating_sub(1),
                    }),
                    Conditions::Field(FieldCondition::SizeLessThan {
                        bytes: bytes.saturating_add(1),
                    }),
                ],
            },
            _ => return Err("unknown size op is not supported in rules form".to_string()),
        },
        QueryNode::Near { .. } => return Err("AROUND is not supported in rules form".to_string()),
        QueryNode::Exact(_) => {
            return Err("+word exact-match is not supported in rules form".to_string())
        }
        _ => return Err("unknown query node is not supported in rules form".to_string()),
    })
}

fn parse_rule_actions_string(value: &str) -> Result<Vec<RuleAction>, String> {
    let actions = value
        .split([',', ';'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(parse_rule_action_string)
        .collect::<Result<Vec<_>, _>>()?;
    if actions.is_empty() {
        return Err("rule action list is empty".to_string());
    }
    Ok(actions)
}

fn parse_rule_action_string(value: &str) -> Result<RuleAction, String> {
    let trimmed = value.trim();
    let lower = trimmed.to_ascii_lowercase();
    if lower == "archive" {
        return Ok(RuleAction::Archive);
    }
    if lower == "trash" {
        return Ok(RuleAction::Trash);
    }
    if lower == "star" {
        return Ok(RuleAction::Star);
    }
    if matches!(lower.as_str(), "mark-read" | "read") {
        return Ok(RuleAction::MarkRead);
    }
    if matches!(lower.as_str(), "mark-unread" | "unread") {
        return Ok(RuleAction::MarkUnread);
    }
    if let Some(label) = strip_action_prefix(trimmed, "add-label:")
        .or_else(|| strip_action_prefix(trimmed, "label:"))
    {
        let label = label.trim();
        if label.is_empty() {
            return Err("label action requires a label".to_string());
        }
        return Ok(RuleAction::AddLabel {
            label: label.to_string(),
        });
    }
    if let Some(label) = strip_action_prefix(trimmed, "remove-label:")
        .or_else(|| strip_action_prefix(trimmed, "unlabel:"))
    {
        let label = label.trim();
        if label.is_empty() {
            return Err("remove-label action requires a label".to_string());
        }
        return Ok(RuleAction::RemoveLabel {
            label: label.to_string(),
        });
    }
    if let Some(command) = strip_action_prefix(trimmed, "shell:") {
        let command = command.trim();
        if command.is_empty() {
            return Err("shell action requires a command".to_string());
        }
        return Ok(RuleAction::ShellHook {
            command: command.to_string(),
        });
    }
    Err(format!("Unsupported action: {value}"))
}

fn strip_action_prefix<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    value
        .get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
        .then(|| &value[prefix.len()..])
}

fn rule_to_form_data(rule: &Rule) -> Result<mxr_protocol::RuleFormData, String> {
    let action = rule_actions_to_string(&rule.actions)?;
    Ok(mxr_protocol::RuleFormData {
        id: Some(rule.id.to_string()),
        name: rule.name.clone(),
        condition: conditions_to_query(&rule.conditions)?,
        action,
        priority: rule.priority,
        enabled: rule.enabled,
    })
}

fn rule_actions_to_string(actions: &[RuleAction]) -> Result<String, String> {
    if actions.is_empty() {
        return Err("rule has no actions".to_string());
    }
    actions
        .iter()
        .map(rule_action_to_string)
        .collect::<Result<Vec<_>, _>>()
        .map(|parts| parts.join(","))
}

fn rule_action_to_string(action: &RuleAction) -> Result<String, String> {
    match action {
        RuleAction::Archive => Ok("archive".to_string()),
        RuleAction::Trash => Ok("trash".to_string()),
        RuleAction::Star => Ok("star".to_string()),
        RuleAction::MarkRead => Ok("mark-read".to_string()),
        RuleAction::MarkUnread => Ok("mark-unread".to_string()),
        RuleAction::AddLabel { label } => Ok(format!("add-label:{label}")),
        RuleAction::RemoveLabel { label } => Ok(format!("remove-label:{label}")),
        RuleAction::ShellHook { command } => Ok(format!("shell:{command}")),
        RuleAction::Snooze { .. } => {
            Err("snooze rules are not editable in the TUI yet".to_string())
        }
    }
}

fn conditions_to_query(conditions: &Conditions) -> Result<String, String> {
    match conditions {
        Conditions::And { conditions } => {
            let parts = conditions
                .iter()
                .map(conditions_to_query)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(parts
                .into_iter()
                .map(|part| format!("({part})"))
                .collect::<Vec<_>>()
                .join(" AND "))
        }
        Conditions::Or { conditions } => {
            let parts = conditions
                .iter()
                .map(conditions_to_query)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(parts
                .into_iter()
                .map(|part| format!("({part})"))
                .collect::<Vec<_>>()
                .join(" OR "))
        }
        Conditions::Not { condition } => Ok(format!("NOT ({})", conditions_to_query(condition)?)),
        Conditions::Field(field) => field_condition_to_query(field),
    }
}

fn field_condition_to_query(field: &FieldCondition) -> Result<String, String> {
    match field {
        FieldCondition::From { pattern } => string_match_to_query("from", pattern),
        FieldCondition::To { pattern } => string_match_to_query("to", pattern),
        FieldCondition::Subject { pattern } => string_match_to_query("subject", pattern),
        FieldCondition::HasLabel { label } => Ok(format!("label:{label}")),
        FieldCondition::HasAttachment => Ok("has:attachment".to_string()),
        FieldCondition::DateAfter { date } => Ok(format!("after:{}", date.format("%Y-%m-%d"))),
        FieldCondition::DateBefore { date } => Ok(format!("before:{}", date.format("%Y-%m-%d"))),
        FieldCondition::IsUnread => Ok("is:unread".to_string()),
        FieldCondition::IsStarred => Ok("is:starred".to_string()),
        FieldCondition::BodyContains { pattern } => string_match_to_query("", pattern),
        FieldCondition::LinkDensity { match_kind } => Ok(match match_kind {
            mxr_rules::LinkDensityMatch::Any => "has:link".to_string(),
            mxr_rules::LinkDensityMatch::Heavy => "has:link-heavy".to_string(),
            mxr_rules::LinkDensityMatch::None => "has:link-none".to_string(),
        }),
        FieldCondition::SizeGreaterThan { .. }
        | FieldCondition::SizeLessThan { .. }
        | FieldCondition::HasUnsubscribe => {
            Err("condition not editable in the TUI yet".to_string())
        }
    }
}

fn string_match_to_query(field: &str, pattern: &StringMatch) -> Result<String, String> {
    let value = match pattern {
        StringMatch::Contains(value) | StringMatch::Exact(value) => value.clone(),
        StringMatch::Regex(_) | StringMatch::Glob(_) => {
            return Err("regex/glob rules are not editable in the TUI yet".to_string())
        }
    };
    if field.is_empty() {
        Ok(value)
    } else {
        Ok(format!("{field}:{value}"))
    }
}

async fn handle_export_thread(
    state: &AppState,
    thread_id: &mxr_core::ThreadId,
    format: &ExportFormat,
) -> Response {
    match build_export_thread(state, thread_id).await {
        Ok(export_thread) => {
            let reader_config = ReaderConfig::default();
            let content = mxr_export::export(&export_thread, format, &reader_config);
            Response::Ok {
                data: ResponseData::ExportResult { content },
            }
        }
        Err(e) => Response::error(e),
    }
}

async fn handle_export_search(
    state: &AppState,
    query: &str,
    account_id: Option<&mxr_core::AccountId>,
    format: &ExportFormat,
) -> Response {
    let search_result = match account_id {
        Some(account_id) => {
            let account_id = account_id.as_str();
            state
                .search
                .search_in_account(
                    query,
                    Some(account_id.as_str()),
                    100,
                    0,
                    mxr_core::types::SortOrder::DateDesc,
                )
                .await
        }
        None => {
            state
                .search
                .search(query, 100, 0, mxr_core::types::SortOrder::DateDesc)
                .await
        }
    };
    let search_results = match search_result {
        Ok(results) => results,
        Err(e) => {
            return Response::error(e.to_string());
        }
    };

    // Collect unique thread IDs from search results
    let thread_ids: Vec<mxr_core::ThreadId> = {
        let mut seen = std::collections::HashSet::new();
        search_results
            .results
            .iter()
            .filter_map(|r| {
                let tid = mxr_core::ThreadId::from_uuid(uuid::Uuid::parse_str(&r.thread_id).ok()?);
                if seen.insert(tid.clone()) {
                    Some(tid)
                } else {
                    None
                }
            })
            .collect()
    };

    let reader_config = ReaderConfig::default();
    let mut all_content = String::new();

    for tid in &thread_ids {
        match build_export_thread(state, tid).await {
            Ok(export_thread) => {
                all_content.push_str(&mxr_export::export(&export_thread, format, &reader_config));
                all_content.push('\n');
            }
            Err(e) => {
                tracing::warn!(thread_id = %tid, error = %e, "Skipping thread in bulk export");
            }
        }
    }

    Response::Ok {
        data: ResponseData::ExportResult {
            content: all_content,
        },
    }
}

async fn materialize_attachment_file(
    state: &AppState,
    message_id: &mxr_core::MessageId,
    attachment_id: &mxr_core::AttachmentId,
) -> Result<mxr_protocol::AttachmentFile, mxr_core::MxrError> {
    let envelope = state
        .store
        .get_envelope(message_id)
        .await
        .map_err(|err| mxr_core::MxrError::Store(err.to_string()))?
        .ok_or_else(|| mxr_core::MxrError::NotFound(format!("message {message_id}")))?;

    let mut body = state.sync_engine.get_body(message_id).await?;
    let attachment = body
        .attachments
        .iter()
        .find(|attachment| &attachment.id == attachment_id)
        .cloned()
        .ok_or_else(|| mxr_core::MxrError::NotFound(format!("attachment {attachment_id}")))?;

    if let Some(path) = attachment.local_path.as_ref().filter(|path| path.exists()) {
        return Ok(mxr_protocol::AttachmentFile {
            attachment_id: attachment.id,
            filename: attachment.filename,
            path: path.display().to_string(),
        });
    }

    let provider = state
        .get_provider(Some(&envelope.account_id))
        .map_err(mxr_core::MxrError::Provider)?;
    let bytes = provider
        .fetch_attachment(&envelope.provider_id, &attachment.provider_id)
        .await?;

    let target_dir = state.attachment_dir().join(message_id.as_str());
    tokio::fs::create_dir_all(&target_dir)
        .await
        .map_err(mxr_core::MxrError::Io)?;

    let filename = sanitized_attachment_filename(&attachment.filename, &attachment.id);
    let path = target_dir.join(filename);
    tokio::fs::write(&path, bytes)
        .await
        .map_err(mxr_core::MxrError::Io)?;
    set_private_file_permissions(&path).await?;

    for existing in &mut body.attachments {
        if existing.id == *attachment_id {
            existing.local_path = Some(path.clone());
        }
    }
    state
        .store
        .insert_body(&body)
        .await
        .map_err(|err| mxr_core::MxrError::Store(err.to_string()))?;

    Ok(mxr_protocol::AttachmentFile {
        attachment_id: attachment.id,
        filename: attachment.filename,
        path: path.display().to_string(),
    })
}

/// Like `materialize_attachment_file` but writes the bytes to a user-chosen
/// `destination` path instead of the daemon's internal cache. The caller
/// owns the full path including the filename; the daemon creates parent
/// directories as needed. Used by the TUI save-attachment modal.
pub(super) async fn materialize_attachment_to_path(
    state: &AppState,
    message_id: &mxr_core::MessageId,
    attachment_id: &mxr_core::AttachmentId,
    destination: &std::path::Path,
) -> Result<mxr_protocol::AttachmentFile, mxr_core::MxrError> {
    let envelope = state
        .store
        .get_envelope(message_id)
        .await
        .map_err(|err| mxr_core::MxrError::Store(err.to_string()))?
        .ok_or_else(|| mxr_core::MxrError::NotFound(format!("message {message_id}")))?;

    let body = state.sync_engine.get_body(message_id).await?;
    let attachment = body
        .attachments
        .iter()
        .find(|attachment| &attachment.id == attachment_id)
        .cloned()
        .ok_or_else(|| mxr_core::MxrError::NotFound(format!("attachment {attachment_id}")))?;

    // Reuse the cached bytes if we've already pulled them down, so a
    // user-initiated save after Open doesn't refetch from the provider.
    let bytes = match attachment.local_path.as_ref().filter(|path| path.exists()) {
        Some(cached) => tokio::fs::read(cached)
            .await
            .map_err(mxr_core::MxrError::Io)?,
        None => {
            let provider = state
                .get_provider(Some(&envelope.account_id))
                .map_err(mxr_core::MxrError::Provider)?;
            provider
                .fetch_attachment(&envelope.provider_id, &attachment.provider_id)
                .await?
        }
    };

    let destination = safe_attachment_destination(state, destination).await?;
    if let Some(parent) = destination.parent() {
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(mxr_core::MxrError::Io)?;
        }
    }
    tokio::fs::write(&destination, bytes)
        .await
        .map_err(mxr_core::MxrError::Io)?;
    set_private_file_permissions(&destination).await?;

    Ok(mxr_protocol::AttachmentFile {
        attachment_id: attachment.id,
        filename: attachment.filename,
        path: destination.display().to_string(),
    })
}

async fn safe_attachment_destination(
    state: &AppState,
    destination: &Path,
) -> Result<PathBuf, mxr_core::MxrError> {
    if destination.file_name().is_none() {
        return Err(invalid_attachment_destination(
            "attachment destination must include a filename",
        ));
    }
    if tokio::fs::symlink_metadata(destination)
        .await
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err(invalid_attachment_destination(
            "attachment destination must not be a symlink",
        ));
    }

    let destination = absolutize_without_parent(destination)?;
    let allowed_roots = allowed_attachment_destination_roots(state)?;
    if !allowed_roots
        .iter()
        .any(|root| destination.starts_with(root))
    {
        return Err(invalid_attachment_destination(
            "attachment destination must be under the configured download directory, current directory, or system temp directory",
        ));
    }

    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(mxr_core::MxrError::Io)?;
        let canonical_parent = tokio::fs::canonicalize(parent)
            .await
            .map_err(mxr_core::MxrError::Io)?;
        let canonical_roots = canonical_allowed_roots(&allowed_roots).await?;
        if !canonical_roots
            .iter()
            .any(|root| canonical_parent.starts_with(root))
        {
            return Err(invalid_attachment_destination(
                "attachment destination resolves outside allowed directories",
            ));
        }
    }

    Ok(destination)
}

fn allowed_attachment_destination_roots(
    state: &AppState,
) -> Result<Vec<PathBuf>, mxr_core::MxrError> {
    let config = state.config_snapshot();
    Ok(vec![
        absolutize_without_parent(&config.general.download_dir)?,
        absolutize_without_parent(&std::env::temp_dir())?,
        absolutize_without_parent(&std::env::current_dir().map_err(mxr_core::MxrError::Io)?)?,
    ])
}

async fn canonical_allowed_roots(roots: &[PathBuf]) -> Result<Vec<PathBuf>, mxr_core::MxrError> {
    let mut canonical = Vec::with_capacity(roots.len());
    for root in roots {
        match tokio::fs::canonicalize(root).await {
            Ok(path) => canonical.push(path),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                canonical.push(root.clone());
            }
            Err(error) => return Err(mxr_core::MxrError::Io(error)),
        }
    }
    Ok(canonical)
}

fn absolutize_without_parent(path: &Path) -> Result<PathBuf, mxr_core::MxrError> {
    let mut output = if path.is_absolute() {
        PathBuf::new()
    } else {
        std::env::current_dir().map_err(mxr_core::MxrError::Io)?
    };
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => output.push(prefix.as_os_str()),
            Component::RootDir => output.push(component.as_os_str()),
            Component::CurDir => {}
            Component::Normal(part) => output.push(part),
            Component::ParentDir => {
                return Err(invalid_attachment_destination(
                    "attachment destination must not contain parent-directory components",
                ));
            }
        }
    }
    Ok(output)
}

fn invalid_attachment_destination(message: &'static str) -> mxr_core::MxrError {
    mxr_core::MxrError::Io(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        message,
    ))
}

#[cfg(unix)]
async fn set_private_file_permissions(path: &Path) -> Result<(), mxr_core::MxrError> {
    use std::os::unix::fs::PermissionsExt;

    tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .await
        .map_err(mxr_core::MxrError::Io)
}

#[cfg(not(unix))]
async fn set_private_file_permissions(_path: &Path) -> Result<(), mxr_core::MxrError> {
    Ok(())
}

fn sanitized_attachment_filename(filename: &str, attachment_id: &mxr_core::AttachmentId) -> String {
    const MAX_ATTACHMENT_FILENAME_BYTES: usize = 220;
    const MAX_EXTENSION_BYTES: usize = 16;

    let candidate = std::path::Path::new(filename)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(filename);
    let sanitized: String = candidate
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '\0' => '_',
            _ if ch.is_control() => '_',
            _ => ch,
        })
        .collect();

    let sanitized = if sanitized.trim().is_empty()
        || is_special_attachment_path_component(&sanitized)
        || is_reserved_windows_attachment_name(&sanitized)
    {
        "attachment".to_string()
    } else {
        sanitized
    };

    // APFS limits individual path components to 255 bytes. Some
    // real-world attachment filenames exceed that once MIME-decoded;
    // keep the extension when possible and always add a stable attachment-id
    // suffix to avoid cache collisions between same-name attachments.
    let path = std::path::Path::new(&sanitized);
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .filter(|extension| extension.len() <= MAX_EXTENSION_BYTES);
    let suffix = match extension {
        Some(extension) => format!("-{}.{}", attachment_id.as_str(), extension),
        None => format!("-{}", attachment_id.as_str()),
    };
    let max_stem_bytes = MAX_ATTACHMENT_FILENAME_BYTES.saturating_sub(suffix.len());
    let mut stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(&sanitized)
        .to_string();
    while stem.len() > max_stem_bytes {
        stem.pop();
    }
    format!("{stem}{suffix}")
}

fn is_special_attachment_path_component(filename: &str) -> bool {
    matches!(filename, "." | "..")
}

fn is_reserved_windows_attachment_name(filename: &str) -> bool {
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(filename)
        .trim_end_matches([' ', '.'])
        .to_ascii_uppercase();
    matches!(
        stem.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

fn open_local_file(path: &str) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(path).spawn()?;
        Ok(())
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open").arg(path).spawn()?;
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", path])
            .spawn()?;
        Ok(())
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        anyhow::bail!("opening attachments is not supported on this platform")
    }
}

#[cfg(test)]
mod tests;
