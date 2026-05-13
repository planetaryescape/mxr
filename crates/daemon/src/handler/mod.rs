#![cfg_attr(
    test,
    allow(
        clippy::bool_assert_comparison,
        clippy::len_zero,
        clippy::panic,
        clippy::unwrap_used
    )
)]

mod accounts;
mod admin;
mod archive_ask;
mod auth_sessions;
mod commitments;
mod commitments_extract;
#[path = "diagnostics/mod.rs"]
pub(crate) mod diagnostics_impl;
mod draft_assist;
mod draft_new;
mod draft_refine;
mod helpers;
mod humanizer;
mod mailbox;
mod mutations;
mod platform;
mod relationship_profile;
mod reply_later;
mod rules;
mod runtime;
mod safety_llm;
mod screener;
mod sender_view;
mod signatures;
mod snippets;
mod status_helpers;
pub(crate) mod summarize;
mod user_voice;

use crate::state::AppState;
use mxr_config::SafetyPolicy;
use mxr_core::provider::MailSyncProvider;
#[cfg(test)]
use mxr_core::types::UnsubscribeMethod;
use mxr_core::types::{ExportFormat, Snoozed};
use mxr_export::{ExportAttachment, ExportMessage, ExportThread};
use mxr_protocol::*;
use mxr_reader::ReaderConfig;
use mxr_rules::{Conditions, FieldCondition, Rule, RuleAction, StringMatch};
use mxr_search::parse_query;
use std::sync::Arc;
use tracing::Instrument;

pub(crate) use helpers::{
    dir_size_sync, file_size_sync, recent_log_lines_sync, should_fallback_to_tantivy,
};
pub(crate) use mutations::send_stored_draft;
pub(crate) use status_helpers::{
    build_doctor_findings, doctor_data_stats, latest_successful_sync_at,
};

type HandlerResult = Result<ResponseData, String>;

async fn send_time_recommendation(
    state: &Arc<AppState>,
    account_id: &mxr_core::AccountId,
    recipient: &str,
) -> HandlerResult {
    let rec = state
        .store
        .send_time_recommendation(account_id, recipient)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::SendTimeRecommendationResponse {
        recommendation: mxr_protocol::SendTimeRecommendationData {
            recipient: rec.recipient,
            buckets: rec
                .buckets
                .into_iter()
                .map(|b| mxr_protocol::SendTimeBucketData {
                    weekday: b.weekday,
                    hour: b.hour,
                    p50_seconds: b.p50_seconds,
                    sample_count: b.sample_count,
                })
                .collect(),
            best_weekday: rec.best_weekday,
            best_hour: rec.best_hour,
            best_p50_seconds: rec.best_p50_seconds,
            confidence: match rec.confidence {
                mxr_store::SendTimeConfidence::Low => mxr_protocol::SendTimeConfidenceData::Low,
                mxr_store::SendTimeConfidence::Medium => {
                    mxr_protocol::SendTimeConfidenceData::Medium
                }
                mxr_store::SendTimeConfidence::High => mxr_protocol::SendTimeConfidenceData::High,
            },
            sample_count: rec.sample_count,
        },
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
        .await
        .map_err(|e| e.to_string())?;
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
        .await
        .map_err(|e| e.to_string())?;
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

pub async fn handle_request(state: &Arc<AppState>, msg: &IpcMessage) -> IpcMessage {
    let response_data = match &msg.payload {
        IpcPayload::Request(req) => {
            let request = request_kind(req);
            let account_id = request_account_id(req)
                .map(|id| id.as_str())
                .unwrap_or_else(|| "-".to_string());
            let account_key = request_account_key(req).unwrap_or("-");
            let span = tracing::info_span!(
                "ipc_request",
                request_id = msg.id,
                request,
                account_id,
                account_key
            );
            dispatch(state, req).instrument(span).await
        }
        _ => Response::error("Expected a Request"),
    };

    IpcMessage {
        id: msg.id,
        payload: IpcPayload::Response(response_data),
    }
}

async fn dispatch(state: &Arc<AppState>, req: &Request) -> Response {
    let started_at = std::time::Instant::now();
    let request = request_kind(req);
    let account_id = request_account_id(req)
        .map(|id| id.as_str())
        .unwrap_or_else(|| "-".to_string());
    let account_key = request_account_key(req).unwrap_or("-");
    tracing::debug!(request, account_id, account_key, "handling request");

    if let Err(message) = enforce_safety_policy(state.config_snapshot().general.safety_policy, req)
    {
        tracing::warn!(request, account_id, account_key, error = %message, "request rejected by safety policy");
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
        Request::GetHtmlImageAssets {
            message_id,
            allow_remote,
        } => mailbox::get_html_image_assets(state, message_id, *allow_remote).await,
        Request::DownloadAttachment {
            message_id,
            attachment_id,
        } => mailbox::download_attachment(state, message_id, attachment_id).await,
        Request::OpenAttachment {
            message_id,
            attachment_id,
        } => mailbox::open_attachment(state, message_id, attachment_id).await,
        Request::ListBodies { message_ids } => mailbox::list_bodies(state, message_ids).await,
        Request::GetThread { thread_id } => mailbox::get_thread(state, thread_id).await,
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
            platform::update_llm_config(state, config.clone()).await
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
            search_mode,
        } => platform::create_saved_search(state, name, query, *search_mode).await,
        Request::DeleteSavedSearch { name } => platform::delete_saved_search(state, name).await,
        Request::RunSavedSearch { name, limit } => {
            platform::run_saved_search(state, name, *limit).await
        }

        // admin / maintenance / operational
        Request::ListEvents {
            limit,
            level,
            category,
        } => admin::list_events(state, *limit, level.as_deref(), category.as_deref()).await,
        Request::GetLogs { limit, level } => admin::get_logs(state, *limit, level.as_deref()).await,
        Request::GetDoctorReport => admin::doctor_report(state).await,
        Request::GenerateBugReport {
            verbose,
            full_logs,
            since,
        } => admin::bug_report(*verbose, *full_logs, since.clone()).await,
        Request::GetStatus => admin::get_status(state).await,
        Request::Ping => Ok(ResponseData::Pong),
        Request::Shutdown => admin::shutdown(state).await,

        // core mail/runtime
        Request::Search {
            query,
            limit,
            offset,
            mode,
            sort,
            explain,
        } => {
            runtime::search(
                state,
                query,
                *limit,
                *offset,
                mode.unwrap_or(state.config_snapshot().search.default_mode),
                sort.clone().unwrap_or(mxr_core::types::SortOrder::DateDesc),
                *explain,
            )
            .await
        }
        Request::Count { query, mode } => {
            runtime::count(
                state,
                query,
                mode.unwrap_or(state.config_snapshot().search.default_mode),
            )
            .await
        }
        Request::GetHeaders { message_id } => runtime::get_headers(state, message_id).await,
        Request::SyncNow { account_id } => runtime::sync_now(state, account_id.as_ref()).await,
        Request::ExportThread { thread_id, format } => {
            runtime::export_thread(state, thread_id, format).await
        }
        Request::ExportSearch { query, format } => {
            runtime::export_search(state, query, format).await
        }
        Request::Mutation {
            mutation: cmd,
            client_correlation_id,
        } => mutations::mutation(state, cmd, client_correlation_id.as_deref()).await,
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
        Request::ListSenders { limit } => platform::list_senders(state, *limit).await,
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
        Request::SendTimeRecommendation {
            account_id,
            recipient,
        } => send_time_recommendation(state, account_id, recipient).await,
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
        Request::DraftAssist {
            thread_id,
            instruction,
        } => draft_assist::draft_assist(state, thread_id, instruction).await,
        Request::DraftNew {
            account_id,
            to,
            purpose,
            register,
            length_hint,
        } => {
            draft_new::draft_new(
                state,
                account_id,
                to.clone(),
                purpose,
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
        } => {
            mutations::send_stored_draft(state, draft_id, override_safety_token.as_deref()).await
        }
        Request::CheckDraftSafety { draft, context } => {
            mutations::check_draft_safety_request(state, draft, context).await
        }
        Request::ExtractDraftCommitments { draft } => {
            commitments_extract::extract_request(state, draft).await
        }
        Request::DeleteDraft { draft_id } => mutations::delete_draft(state, draft_id).await,
        Request::SaveDraftToServer { draft } => mutations::save_draft_to_server(state, draft).await,
        Request::Unsubscribe { message_id } => mutations::unsubscribe(state, message_id).await,
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

fn request_is_draft_only(req: &Request) -> bool {
    matches!(req, Request::SaveDraft { .. } | Request::DeleteDraft { .. })
}

fn request_is_read_only(req: &Request) -> bool {
    matches!(
        req,
        Request::ListEnvelopes { .. }
            | Request::ListEnvelopesByIds { .. }
            | Request::GetEnvelope { .. }
            | Request::GetBody { .. }
            | Request::ListBodies { .. }
            | Request::GetThread { .. }
            | Request::ListLabels { .. }
            | Request::ListAccounts
            | Request::ListAccountsConfig
            | Request::GetAuthSession { .. }
            | Request::ListRules
            | Request::GetRule { .. }
            | Request::GetRuleForm { .. }
            | Request::DryRunRules { .. }
            | Request::ListSavedSearches
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
            | Request::GetSemanticStatus
            | Request::RunSavedSearch { .. }
            | Request::ListEvents { .. }
            | Request::GetLogs { .. }
            | Request::GetDoctorReport
            | Request::GenerateBugReport { .. }
            | Request::Search { .. }
            | Request::GetSyncStatus { .. }
            | Request::Count { .. }
            | Request::GetHeaders { .. }
            | Request::ListRuleHistory { .. }
            | Request::ListSnoozed
            | Request::ListReplyQueue
            | Request::ListSnippets
            | Request::ListSignatures
            | Request::ListSignatureDefaults
            | Request::ResolveSignature { .. }
            | Request::GetSenderProfile { .. }
            | Request::ListSenders { .. }
            | Request::ListScreenerQueue { .. }
            | Request::ListScreenerDecisions { .. }
            | Request::SummarizeThread { .. }
            | Request::DraftNew { .. }
            | Request::DraftRefine { .. }
            | Request::ListDrafts
            | Request::ListOrphanedDrafts
            | Request::PrepareReply { .. }
            | Request::PrepareForward { .. }
            | Request::ExportThread { .. }
            | Request::ExportSearch { .. }
            | Request::GetStatus
            | Request::Ping
    )
}

fn request_kind(req: &Request) -> &'static str {
    match req {
        Request::ListEnvelopes { .. } => "list_envelopes",
        Request::ListEnvelopesByIds { .. } => "list_envelopes_by_ids",
        Request::GetEnvelope { .. } => "get_envelope",
        Request::GetBody { .. } => "get_body",
        Request::GetHtmlImageAssets { .. } => "get_html_image_assets",
        Request::DownloadAttachment { .. } => "download_attachment",
        Request::OpenAttachment { .. } => "open_attachment",
        Request::ListBodies { .. } => "list_bodies",
        Request::GetThread { .. } => "get_thread",
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
        Request::GetHeaders { .. } => "get_headers",
        Request::ListSavedSearches => "list_saved_searches",
        Request::ListSubscriptions { .. } => "list_subscriptions",
        Request::ListStorageBreakdown { .. } => "list_storage_breakdown",
        Request::ListLargestMessages { .. } => "list_largest_messages",
        Request::Wrapped { .. } => "wrapped",
        Request::ListStaleThreads { .. } => "list_stale_threads",
        Request::ListContactAsymmetry { .. } => "list_contact_asymmetry",
        Request::ListContactDecay { .. } => "list_contact_decay",
        Request::RefreshContacts => "refresh_contacts",
        Request::RebuildAnalytics => "rebuild_analytics",
        Request::ListResponseTime { .. } => "list_response_time",
        Request::ListAccountAddresses { .. } => "list_account_addresses",
        Request::AddAccountAddress { .. } => "add_account_address",
        Request::RemoveAccountAddress { .. } => "remove_account_address",
        Request::SetPrimaryAccountAddress { .. } => "set_primary_account_address",
        Request::GetLlmStatus => "get_llm_status",
        Request::GetLlmConfig => "get_llm_config",
        Request::UpdateLlmConfig { .. } => "update_llm_config",
        Request::GetSemanticStatus => "get_semantic_status",
        Request::EnableSemantic { .. } => "enable_semantic",
        Request::InstallSemanticProfile { .. } => "install_semantic_profile",
        Request::UseSemanticProfile { .. } => "use_semantic_profile",
        Request::ReindexSemantic => "reindex_semantic",
        Request::BackfillSemantic => "backfill_semantic",
        Request::CreateSavedSearch { .. } => "create_saved_search",
        Request::DeleteSavedSearch { .. } => "delete_saved_search",
        Request::RunSavedSearch { .. } => "run_saved_search",
        Request::Mutation { mutation: cmd, .. } => mutation_kind(cmd),
        Request::UndoMutation { .. } => "undo_mutation",
        Request::Unsubscribe { .. } => "unsubscribe",
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
        Request::DraftAssist { .. } => "draft_assist",
        Request::DraftNew { .. } => "draft_new",
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
        Request::SendTimeRecommendation { .. } => "send_time_recommendation",
        Request::DeleteDraft { .. } => "delete_draft",
        Request::SaveDraftToServer { .. } => "save_draft_to_server",
        Request::ListDrafts => "list_drafts",
        Request::ListOrphanedDrafts => "list_orphaned_drafts",
        Request::ResetOrphanedDraft { .. } => "reset_orphaned_draft",
        Request::ExportThread { .. } => "export_thread",
        Request::ExportSearch { .. } => "export_search",
        Request::GetStatus => "get_status",
        Request::Ping => "ping",
        Request::Shutdown => "shutdown",
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
    }
}

fn request_account_id(req: &Request) -> Option<&mxr_core::AccountId> {
    match req {
        Request::ListEnvelopes { account_id, .. }
        | Request::ListLabels { account_id }
        | Request::DeleteLabel { account_id, .. }
        | Request::CreateLabel { account_id, .. }
        | Request::RenameLabel { account_id, .. }
        | Request::ListSubscriptions { account_id, .. }
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
        | Request::SendTimeRecommendation { account_id, .. }
        | Request::GetUserVoice { account_id }
        | Request::RebuildUserVoice { account_id }
        | Request::DraftNew { account_id, .. } => Some(account_id),
        Request::SetSignatureDefault { account_id, .. }
        | Request::ClearSignatureDefault { account_id, .. }
        | Request::ResolveSignature { account_id, .. } => account_id.as_ref(),
        Request::GetSyncStatus { account_id } => Some(account_id),
        Request::SendDraft { draft, .. }
        | Request::SaveDraft { draft }
        | Request::SaveDraftToServer { draft }
        | Request::CheckDraftSafety { draft, .. }
        | Request::ExtractDraftCommitments { draft } => Some(&draft.account_id),
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
        .ok_or_else(|| format!("Thread not found: {}", thread_id))?;

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

fn render_message_context(body: &mxr_core::types::MessageBody) -> String {
    mxr_reader::clean(
        body.text_plain.as_deref(),
        body.text_html.as_deref(),
        &ReaderConfig::default(),
    )
    .content
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
    if provider.capabilities().labels {
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
            "Reconciled message not found after folder mutation for {}",
            previous_message_id
        )),
        _ => Err(format!(
            "Ambiguous reconciled message after folder mutation for {}",
            previous_message_id
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
    provider
        .modify_labels(&provider_id, &[], &["INBOX".to_string()])
        .await
        .map_err(|e| e.to_string())?;
    reconcile_label_mutation(
        state,
        provider.as_ref(),
        message_id,
        &[],
        &["INBOX".to_string()],
    )
    .await?;
    let snoozed_message_id = if provider.capabilities().labels {
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
    provider
        .modify_labels(&provider_id, &restore_provider_ids, &[])
        .await
        .map_err(|e| e.to_string())?;
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
        actions: vec![parse_rule_action_string(action)?],
        created_at: existing.as_ref().map(|rule| rule.created_at).unwrap_or(now),
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
        QueryNode::Filter(
            FilterKind::Answered
            | FilterKind::ReplyLater
            | FilterKind::Anywhere
            | FilterKind::HasUserLabels
            | FilterKind::NoUserLabels
            | FilterKind::HasDrive
            | FilterKind::HasDocument
            | FilterKind::HasSpreadsheet
            | FilterKind::HasPresentation
            | FilterKind::HasYoutube
            | FilterKind::HasInlineImage,
        ) => return Err("search filter is not supported in rules form".to_string()),
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
        },
        QueryNode::Near { .. } => return Err("AROUND is not supported in rules form".to_string()),
    })
}

fn parse_rule_action_string(value: &str) -> Result<RuleAction, String> {
    let lower = value.to_ascii_lowercase();
    if lower == "archive" {
        return Ok(RuleAction::Archive);
    }
    if lower == "trash" {
        return Ok(RuleAction::Trash);
    }
    if lower == "star" {
        return Ok(RuleAction::Star);
    }
    if lower == "mark-read" {
        return Ok(RuleAction::MarkRead);
    }
    if lower == "mark-unread" {
        return Ok(RuleAction::MarkUnread);
    }
    if let Some(label) = value.strip_prefix("add-label:") {
        return Ok(RuleAction::AddLabel {
            label: label.to_string(),
        });
    }
    if let Some(label) = value.strip_prefix("remove-label:") {
        return Ok(RuleAction::RemoveLabel {
            label: label.to_string(),
        });
    }
    if let Some(command) = value.strip_prefix("shell:") {
        return Ok(RuleAction::ShellHook {
            command: command.to_string(),
        });
    }
    Err(format!("Unsupported action: {value}"))
}

fn rule_to_form_data(rule: &Rule) -> Result<mxr_protocol::RuleFormData, String> {
    let action = rule
        .actions
        .first()
        .ok_or_else(|| "rule has no actions".to_string())
        .and_then(rule_action_to_string)?;
    Ok(mxr_protocol::RuleFormData {
        id: Some(rule.id.to_string()),
        name: rule.name.clone(),
        condition: conditions_to_query(&rule.conditions)?,
        action,
        priority: rule.priority,
        enabled: rule.enabled,
    })
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

async fn list_runtime_accounts(state: &AppState) -> Result<Vec<AccountSummaryData>, String> {
    use std::collections::BTreeMap;

    let config = state.config_snapshot();
    let default_config_key = config.general.default_account.clone();
    let runtime_ids = state.runtime_account_ids();
    let default_account_id = state.default_account_id_opt();
    let runtime_accounts = state
        .store
        .list_accounts()
        .await
        .map_err(|e| e.to_string())?;

    let mut accounts: BTreeMap<String, AccountSummaryData> = BTreeMap::new();

    for account in runtime_accounts
        .into_iter()
        .filter(|account| runtime_ids.iter().any(|id| id == &account.id))
    {
        let key = account
            .sync_backend
            .as_ref()
            .map(|backend| backend.config_key.clone())
            .or_else(|| {
                account
                    .send_backend
                    .as_ref()
                    .map(|backend| backend.config_key.clone())
            });
        let sync_kind = account
            .sync_backend
            .as_ref()
            .map(|backend| provider_kind_label(&backend.provider_kind).to_string());
        let send_kind = account
            .send_backend
            .as_ref()
            .map(|backend| provider_kind_label(&backend.provider_kind).to_string());
        let provider_kind = sync_kind
            .clone()
            .or_else(|| send_kind.clone())
            .unwrap_or_else(|| "unknown".to_string());
        let mut capabilities = state
            .get_provider(Some(&account.id))
            .map(|provider| AccountCapabilitiesData::from(provider.capabilities()))
            .unwrap_or_default();
        capabilities.supports_send = send_kind.is_some();
        let map_key = key.clone().unwrap_or_else(|| account.id.to_string());

        accounts.insert(
            map_key,
            AccountSummaryData {
                account_id: account.id.clone(),
                key,
                name: account.name,
                email: account.email,
                provider_kind,
                sync_kind,
                send_kind,
                enabled: account.enabled,
                is_default: default_account_id.as_ref() == Some(&account.id),
                source: AccountSourceData::Runtime,
                editable: AccountEditModeData::RuntimeOnly,
                sync: None,
                send: None,
                capabilities,
            },
        );
    }

    for (key, account) in config.accounts {
        let account_id = config_account_id(&key, &account);
        let summary = accounts
            .entry(key.clone())
            .or_insert_with(|| AccountSummaryData {
                account_id: account_id.clone(),
                key: Some(key.clone()),
                name: account.name.clone(),
                email: account.email.clone(),
                provider_kind: account_primary_provider_kind(&account),
                sync_kind: account.sync.as_ref().map(config_sync_kind_label),
                send_kind: account.send.as_ref().map(config_send_kind_label),
                enabled: account.enabled,
                is_default: false,
                source: AccountSourceData::Config,
                editable: AccountEditModeData::Full,
                sync: None,
                send: None,
                capabilities: config_account_capabilities(&account),
            });

        summary.account_id = account_id;
        summary.key = Some(key.clone());
        summary.name = account.name.clone();
        summary.email = account.email.clone();
        summary.provider_kind = account_primary_provider_kind(&account);
        summary.sync_kind = account.sync.as_ref().map(config_sync_kind_label);
        summary.send_kind = account.send.as_ref().map(config_send_kind_label);
        summary.enabled = account.enabled;
        summary.sync = account.sync.clone().map(sync_config_to_data);
        summary.send = account.send.clone().map(send_config_to_data);
        summary.is_default = default_config_key.as_deref() == Some(key.as_str());
        summary.source = match summary.source {
            AccountSourceData::Runtime => AccountSourceData::Both,
            _ => AccountSourceData::Config,
        };
        summary.editable = AccountEditModeData::Full;
        summary.capabilities = state
            .get_provider(Some(&summary.account_id))
            .map(|provider| AccountCapabilitiesData::from(provider.capabilities()))
            .unwrap_or_else(|_| config_account_capabilities(&account));
        summary.capabilities.supports_send = summary.send_kind.is_some();
    }

    let mut accounts = accounts.into_values().collect::<Vec<_>>();
    accounts.sort_by(|left, right| {
        right
            .is_default
            .cmp(&left.is_default)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then_with(|| left.email.to_lowercase().cmp(&right.email.to_lowercase()))
    });
    Ok(accounts)
}

fn list_account_configs() -> Result<Vec<AccountConfigData>, String> {
    let config = mxr_config::load_config().map_err(|e| e.to_string())?;
    let default_account = config.general.default_account.clone();
    let mut accounts = config
        .accounts
        .into_iter()
        .map(|(key, account)| AccountConfigData {
            is_default: default_account.as_deref() == Some(key.as_str()),
            key,
            name: account.name,
            email: account.email,
            enabled: account.enabled,
            sync: account.sync.map(sync_config_to_data),
            send: account.send.map(send_config_to_data),
        })
        .collect::<Vec<_>>();
    accounts.sort_by(|left, right| left.key.cmp(&right.key));
    Ok(accounts)
}

async fn upsert_account_config(
    state: &Arc<AppState>,
    account: AccountConfigData,
) -> AccountOperationResult {
    let save_result = (|| -> Result<String, String> {
        let mut config = mxr_config::load_config().map_err(|e| e.to_string())?;
        persist_account_passwords(&account).map_err(|e| e.to_string())?;

        config.accounts.insert(
            account.key.clone(),
            mxr_config::AccountConfig {
                name: account.name.clone(),
                email: account.email.clone(),
                enabled: account.enabled,
                sync: account.sync.clone().map(sync_data_to_config).transpose()?,
                send: account.send.clone().map(send_data_to_config).transpose()?,
            },
        );
        if account.is_default || config.general.default_account.is_none() {
            config.general.default_account = Some(account.key.clone());
        }
        mxr_config::save_config(&config).map_err(|e| e.to_string())?;
        Ok(format!("Saved account '{}' to config.", account.key))
    })();

    match save_result {
        Ok(save_detail) => match state.reload_accounts_from_disk().await {
            Ok(()) => account_operation_result(
                true,
                format!("Saved account '{}' and reloaded runtime.", account.key),
                Some(account_step(
                    true,
                    format!("{save_detail} Runtime reloaded."),
                )),
                None,
                None,
                None,
            ),
            Err(error) => account_operation_result(
                false,
                format!(
                    "Saved account '{}' but failed to reload runtime.",
                    account.key
                ),
                Some(account_step(
                    false,
                    format!("{save_detail} Reload failed: {error}"),
                )),
                None,
                None,
                None,
            ),
        },
        Err(error) => account_operation_result(
            false,
            format!("Failed to save account '{}'.", account.key),
            Some(account_step(false, error)),
            None,
            None,
            None,
        ),
    }
}

async fn set_default_account(state: &Arc<AppState>, key: &str) -> Result<String, String> {
    let mut config = mxr_config::load_config().map_err(|e| e.to_string())?;
    if !config.accounts.contains_key(key) {
        return Err(format!("Account '{}' cannot be set as default", key));
    }
    config.general.default_account = Some(key.to_string());
    mxr_config::save_config(&config).map_err(|e| e.to_string())?;
    state.reload_accounts_from_disk().await?;
    Ok(format!("Default account set to '{}'.", key))
}

async fn remove_account_config(
    state: &Arc<AppState>,
    key: &str,
    purge_local_data: bool,
    dry_run: bool,
) -> AccountOperationResult {
    let config = match mxr_config::load_config() {
        Ok(config) => config,
        Err(error) => {
            return account_operation_result(
                false,
                format!("Failed to remove account '{key}'."),
                Some(account_step(false, error.to_string())),
                None,
                None,
                None,
            )
        }
    };
    let Some(account) = config.accounts.get(key).cloned() else {
        return account_operation_result(
            false,
            format!("Account '{key}' not found."),
            Some(account_step(false, format!("Account '{key}' not found."))),
            None,
            None,
            None,
        );
    };

    let account_id = config_account_id(key, &account);
    let message_ids = match state.store.list_message_ids_by_account(&account_id).await {
        Ok(message_ids) => message_ids,
        Err(error) => {
            return account_operation_result(
                false,
                format!("Failed to inspect cached mail for account '{key}'."),
                Some(account_step(false, error.to_string())),
                None,
                None,
                None,
            )
        }
    };
    let cached_message_count = message_ids.len();
    let cache_action = if purge_local_data { "purge" } else { "detach" };

    if dry_run {
        return account_operation_result(
            true,
            format!(
                "Would remove account '{key}' from config and {cache_action} {cached_message_count} cached message(s)."
            ),
            Some(account_step(true, "Dry run only; no changes made.".into())),
            None,
            None,
            None,
        );
    }

    let save_result = (|| -> Result<(), String> {
        let mut config = config;
        config.accounts.remove(key);
        refresh_default_account(&mut config);
        mxr_config::save_config(&config).map_err(|e| e.to_string())?;
        Ok(())
    })();
    if let Err(error) = save_result {
        return account_operation_result(
            false,
            format!("Failed to remove account '{key}'."),
            Some(account_step(false, error)),
            None,
            None,
            None,
        );
    }

    let local_result = if purge_local_data {
        match state
            .search
            .apply_batch(mxr_search::SearchUpdateBatch {
                entries: Vec::new(),
                removed_message_ids: message_ids,
            })
            .await
            .map_err(|e| e.to_string())
        {
            Ok(()) => state
                .store
                .delete_account(&account_id)
                .await
                .map(|_| ())
                .map_err(|e| e.to_string()),
            Err(error) => Err(error),
        }
    } else {
        state
            .store
            .set_account_enabled(&account_id, false)
            .await
            .map_err(|e| e.to_string())
    };
    if let Err(error) = local_result {
        return account_operation_result(
            false,
            format!("Removed account '{key}' from config but failed to update cached mail."),
            Some(account_step(false, error)),
            None,
            None,
            None,
        );
    }

    match state.reload_accounts_from_disk().await {
        Ok(()) => account_operation_result(
            true,
            if purge_local_data {
                format!(
                    "Removed account '{key}' and purged {cached_message_count} cached message(s)."
                )
            } else {
                format!("Removed account '{key}' from config; cached mail detached.")
            },
            Some(account_step(
                true,
                "Config saved and daemon runtime reloaded.".into(),
            )),
            None,
            None,
            None,
        ),
        Err(error) => account_operation_result(
            false,
            format!("Removed account '{key}' but failed to reload runtime."),
            Some(account_step(false, error)),
            None,
            None,
            None,
        ),
    }
}

async fn disable_account_config(state: &Arc<AppState>, key: &str) -> AccountOperationResult {
    let mut config = match mxr_config::load_config() {
        Ok(config) => config,
        Err(error) => {
            return account_operation_result(
                false,
                format!("Failed to disable account '{key}'."),
                Some(account_step(false, error.to_string())),
                None,
                None,
                None,
            )
        }
    };
    let Some(account) = config.accounts.get_mut(key) else {
        return account_operation_result(
            false,
            format!("Account '{key}' not found."),
            Some(account_step(false, format!("Account '{key}' not found."))),
            None,
            None,
            None,
        );
    };

    account.enabled = false;
    let account_id = config_account_id(key, account);
    refresh_default_account(&mut config);
    if let Err(error) = mxr_config::save_config(&config) {
        return account_operation_result(
            false,
            format!("Failed to disable account '{key}'."),
            Some(account_step(false, error.to_string())),
            None,
            None,
            None,
        );
    }
    if let Err(error) = state.store.set_account_enabled(&account_id, false).await {
        return account_operation_result(
            false,
            format!("Disabled account '{key}' in config but failed to update cached mail."),
            Some(account_step(false, error.to_string())),
            None,
            None,
            None,
        );
    }

    match state.reload_accounts_from_disk().await {
        Ok(()) => account_operation_result(
            true,
            format!("Disabled account '{key}'."),
            Some(account_step(
                true,
                "Config saved and daemon runtime reloaded.".into(),
            )),
            None,
            None,
            None,
        ),
        Err(error) => account_operation_result(
            false,
            format!("Disabled account '{key}' but failed to reload runtime."),
            Some(account_step(false, error)),
            None,
            None,
            None,
        ),
    }
}

fn repair_account_config(account: AccountConfigData) -> AccountOperationResult {
    match repair_account_passwords(&account) {
        Ok(count) => account_operation_result(
            true,
            format!("Repaired keychain credentials for '{}'.", account.key),
            Some(account_step(
                true,
                format!("Stored {count} password-backed credential(s)."),
            )),
            None,
            None,
            None,
        ),
        Err(error) => {
            let detail = error.to_string();
            let summary = if detail.contains("no password-backed") {
                format!(
                    "Account '{}' has no password-backed credentials to repair.",
                    account.key
                )
            } else {
                format!("Failed to repair credentials for '{}'.", account.key)
            };
            account_operation_result(
                false,
                summary,
                Some(account_step(false, detail)),
                None,
                None,
                None,
            )
        }
    }
}

fn refresh_default_account(config: &mut mxr_config::MxrConfig) {
    let current_default_is_enabled = config
        .general
        .default_account
        .as_ref()
        .and_then(|key| config.accounts.get(key))
        .map(|account| account.enabled)
        .unwrap_or(false);
    if current_default_is_enabled {
        return;
    }

    config.general.default_account = config
        .accounts
        .iter()
        .filter(|(_, account)| account.enabled)
        .map(|(key, _)| key.clone())
        .min();
}

async fn authorize_account_config(
    account: AccountConfigData,
    reauthorize: bool,
) -> AccountOperationResult {
    // Outlook device-code flow — check sync first, fall back to send for send-only accounts
    let outlook_tenant = match &account.sync {
        Some(AccountSyncConfigData::OutlookPersonal { .. }) => {
            Some(mxr_provider_outlook::OutlookTenant::Personal)
        }
        Some(AccountSyncConfigData::OutlookWork { .. }) => {
            Some(mxr_provider_outlook::OutlookTenant::Work)
        }
        _ => match &account.send {
            Some(AccountSendConfigData::OutlookPersonal { .. }) => {
                Some(mxr_provider_outlook::OutlookTenant::Personal)
            }
            Some(AccountSendConfigData::OutlookWork { .. }) => {
                Some(mxr_provider_outlook::OutlookTenant::Work)
            }
            _ => None,
        },
    };
    if let Some(tenant) = outlook_tenant {
        let (client_id, token_ref) = match &account.sync {
            Some(
                AccountSyncConfigData::OutlookPersonal {
                    client_id,
                    token_ref,
                }
                | AccountSyncConfigData::OutlookWork {
                    client_id,
                    token_ref,
                },
            ) => (client_id.clone(), token_ref.clone()),
            _ => match &account.send {
                Some(
                    AccountSendConfigData::OutlookPersonal {
                        client_id,
                        token_ref,
                    }
                    | AccountSendConfigData::OutlookWork {
                        client_id,
                        token_ref,
                    },
                ) => (client_id.clone(), token_ref.clone()),
                _ => unreachable!(),
            },
        };
        let cid = client_id
            .or_else(|| mxr_provider_outlook::OutlookAuth::bundled_client_id().map(String::from))
            .unwrap_or_default();
        if cid.is_empty() {
            return account_operation_result(
                false,
                "Outlook authorization requires a client ID.".into(),
                None,
                Some(account_step(
                    false,
                    "No bundled client ID and none provided. Add client_id to account config."
                        .into(),
                )),
                None,
                None,
            );
        }
        let auth = mxr_provider_outlook::OutlookAuth::new(cid, token_ref, tenant);
        if !reauthorize {
            if auth.get_valid_access_token().await.is_ok() {
                return account_operation_result(
                    true,
                    "Outlook authorization ready.".into(),
                    None,
                    Some(account_step(true, "Existing OAuth token valid.".into())),
                    None,
                    None,
                );
            }
        }
        let device_resp = match auth.start_device_flow().await {
            Ok(r) => r,
            Err(e) => {
                return account_operation_result(
                    false,
                    "Outlook authorization failed.".into(),
                    None,
                    Some(account_step(false, e.to_string())),
                    None,
                    None,
                );
            }
        };
        let device_code_url = device_resp
            .verification_uri_complete
            .clone()
            .unwrap_or_else(|| device_resp.verification_uri.clone());
        let device_code_user_code = device_resp.user_code.clone();
        let _ = open::that(&device_code_url);
        tracing::info!(
            user_code = %device_resp.user_code,
            url = %device_code_url,
            "Outlook device code flow started — user must enter code in browser"
        );
        return match auth
            .poll_for_token(&device_resp.device_code, device_resp.interval)
            .await
        {
            Ok(tokens) => {
                if let Err(e) = auth.save_tokens(&tokens) {
                    account_operation_result(
                        false,
                        "Outlook authorization failed.".into(),
                        None,
                        Some(account_step(false, format!("Token save failed: {e}"))),
                        None,
                        None,
                    )
                } else {
                    AccountOperationResult {
                        ok: true,
                        summary: "Outlook authorization complete.".into(),
                        save: None,
                        auth: Some(account_step(true, "Token stored successfully.".into())),
                        sync: None,
                        send: None,
                        device_code_url: Some(device_code_url),
                        device_code_user_code: Some(device_code_user_code),
                    }
                }
            }
            Err(e) => account_operation_result(
                false,
                "Outlook authorization failed.".into(),
                None,
                Some(account_step(false, e.to_string())),
                None,
                None,
            ),
        };
    }

    let Some(AccountSyncConfigData::Gmail {
        credential_source,
        client_id,
        client_secret,
        token_ref,
    }) = account.sync
    else {
        return account_operation_result(
            false,
            "Authorization is only available for Gmail and Outlook accounts.".into(),
            None,
            Some(account_step(
                false,
                "Selected account does not use Gmail or Outlook sync.".into(),
            )),
            None,
            None,
        );
    };

    let (client_id, client_secret) =
        match resolve_gmail_credentials(credential_source, client_id, client_secret) {
            Ok(creds) => creds,
            Err(error) => {
                return account_operation_result(
                    false,
                    "Gmail authorization unavailable.".into(),
                    None,
                    Some(account_step(false, error)),
                    None,
                    None,
                )
            }
        };

    let mut auth = mxr_provider_gmail::auth::GmailAuth::new(client_id, client_secret, token_ref);
    let auth_result = if reauthorize {
        auth.interactive_auth().await
    } else {
        match auth.load_existing().await {
            Ok(()) => Ok(()),
            Err(_) => auth.interactive_auth().await,
        }
    };

    match auth_result {
        Ok(()) => account_operation_result(
            true,
            if reauthorize {
                "Gmail authorization refreshed.".into()
            } else {
                "Gmail authorization ready.".into()
            },
            None,
            Some(account_step(
                true,
                if reauthorize {
                    "Browser authorization completed and token stored.".into()
                } else {
                    "OAuth token is available for this Gmail account.".into()
                },
            )),
            None,
            None,
        ),
        Err(error) => account_operation_result(
            false,
            "Gmail authorization failed.".into(),
            None,
            Some(account_step(false, error.to_string())),
            None,
            None,
        ),
    }
}

async fn test_account_config(account: AccountConfigData) -> AccountOperationResult {
    if let Err(error) = persist_account_passwords(&account) {
        return account_operation_result(
            false,
            "Failed to persist account secrets before testing.".into(),
            None,
            Some(account_step(false, error.to_string())),
            None,
            None,
        );
    }

    let mut auth = None;
    let mut sync = None;
    let mut send = None;
    let mut ok = true;

    if let Some(sync_config) = account.sync.clone() {
        match sync_config {
            AccountSyncConfigData::Gmail {
                credential_source,
                client_id,
                client_secret,
                token_ref,
            } => {
                let creds = resolve_gmail_credentials(credential_source, client_id, client_secret);
                match creds {
                    Ok((client_id, client_secret)) => {
                        let mut gmail_auth = mxr_provider_gmail::auth::GmailAuth::new(
                            client_id,
                            client_secret,
                            token_ref,
                        );
                        let auth_result = match gmail_auth.load_existing().await {
                            Ok(()) => Ok("Existing OAuth token loaded.".to_string()),
                            Err(_) => gmail_auth.interactive_auth().await.map(|_| {
                                "Browser authorization completed and token stored.".to_string()
                            }),
                        };
                        match auth_result {
                            Ok(detail) => {
                                auth = Some(account_step(true, detail));
                                let client =
                                    mxr_provider_gmail::client::GmailClient::new(gmail_auth);
                                match client.list_labels().await {
                                    Ok(response) => {
                                        let count =
                                            response.labels.map(|labels| labels.len()).unwrap_or(0);
                                        sync = Some(account_step(
                                            true,
                                            format!("Gmail sync ok: {count} labels"),
                                        ));
                                    }
                                    Err(error) => {
                                        ok = false;
                                        sync = Some(account_step(false, error.to_string()));
                                    }
                                }
                            }
                            Err(error) => {
                                ok = false;
                                auth = Some(account_step(false, error.to_string()));
                                sync = Some(account_step(
                                    false,
                                    "Skipped Gmail sync because authorization failed.".into(),
                                ));
                            }
                        }
                    }
                    Err(error) => {
                        ok = false;
                        auth = Some(account_step(false, error));
                        sync = Some(account_step(
                            false,
                            "Skipped Gmail sync because OAuth credentials are unavailable.".into(),
                        ));
                    }
                }
            }
            AccountSyncConfigData::Imap {
                host,
                port,
                username,
                password_ref,
                auth_required,
                use_tls,
                ..
            } => {
                match crate::provider_credentials::imap_config_with_credentials(
                    host,
                    port,
                    username,
                    password_ref,
                    auth_required,
                    use_tls,
                ) {
                    Ok(config) => {
                        let provider = mxr_provider_imap::ImapProvider::new(
                            mxr_core::AccountId::from_provider_id("imap", &account.email),
                            config,
                        );
                        match provider.sync_labels().await {
                            Ok(folders) => {
                                sync = Some(account_step(
                                    true,
                                    format!("IMAP sync ok: {} folders", folders.len()),
                                ));
                            }
                            Err(error) => {
                                ok = false;
                                sync = Some(account_step(false, error.to_string()));
                            }
                        }
                    }
                    Err(error) => {
                        ok = false;
                        sync = Some(account_step(false, error.to_string()));
                    }
                }
            }
            AccountSyncConfigData::OutlookPersonal {
                client_id,
                token_ref,
            }
            | AccountSyncConfigData::OutlookWork {
                client_id,
                token_ref,
            } => {
                let tenant = match &account.sync {
                    Some(AccountSyncConfigData::OutlookWork { .. }) => {
                        mxr_provider_outlook::OutlookTenant::Work
                    }
                    _ => mxr_provider_outlook::OutlookTenant::Personal,
                };
                let cid =
                    client_id.or_else(|| mxr_provider_outlook::BUNDLED_CLIENT_ID.map(String::from));
                match cid {
                    None => {
                        ok = false;
                        sync = Some(account_step(
                            false,
                            "No client_id and no bundled OUTLOOK_CLIENT_ID".into(),
                        ));
                    }
                    Some(cid) => {
                        let auth_inst = std::sync::Arc::new(
                            mxr_provider_outlook::OutlookAuth::new(cid, token_ref, tenant),
                        );
                        let email = account.email.clone();
                        let token_fn: std::sync::Arc<
                            dyn Fn() -> futures::future::BoxFuture<'static, anyhow::Result<String>>
                                + Send
                                + Sync,
                        > = std::sync::Arc::new(move || {
                            let a = auth_inst.clone();
                            Box::pin(async move {
                                a.get_valid_access_token()
                                    .await
                                    .map_err(|e| anyhow::anyhow!(e))
                            })
                        });
                        let factory = mxr_provider_imap::XOAuth2ImapSessionFactory::new(
                            "outlook.office365.com".to_string(),
                            993,
                            email.clone(),
                            token_fn,
                        );
                        let provider = mxr_provider_imap::ImapProvider::with_session_factory(
                            mxr_core::AccountId::from_provider_id("outlook", &email),
                            mxr_provider_imap::config::ImapConfig::new(
                                "outlook.office365.com".to_string(),
                                993,
                                email,
                                String::new(),
                                true,
                                true,
                            ),
                            Box::new(factory),
                        );
                        match provider.sync_labels().await {
                            Ok(folders) => {
                                sync = Some(account_step(
                                    true,
                                    format!("Outlook IMAP ok: {} folders", folders.len()),
                                ));
                            }
                            Err(error) => {
                                ok = false;
                                sync = Some(account_step(false, error.to_string()));
                            }
                        }
                    }
                }
            }
            AccountSyncConfigData::Fake => {
                sync = Some(account_step(true, "Fake sync provider (test-only)".into()));
            }
        }
    }

    match account.send {
        Some(AccountSendConfigData::Gmail) => {
            send = Some(account_step(true, "Gmail send configured.".into()));
        }
        Some(
            send_cfg @ (AccountSendConfigData::OutlookPersonal { .. }
            | AccountSendConfigData::OutlookWork { .. }),
        ) => {
            let (token_ref, send_client_id, tenant) = match send_cfg {
                AccountSendConfigData::OutlookPersonal {
                    token_ref,
                    client_id,
                } => (
                    token_ref,
                    client_id,
                    mxr_provider_outlook::OutlookTenant::Personal,
                ),
                AccountSendConfigData::OutlookWork {
                    token_ref,
                    client_id,
                } => (
                    token_ref,
                    client_id,
                    mxr_provider_outlook::OutlookTenant::Work,
                ),
                _ => unreachable!(),
            };
            let cid = send_client_id
                .or_else(|| match &account.sync {
                    Some(
                        AccountSyncConfigData::OutlookPersonal {
                            client_id: Some(id),
                            ..
                        }
                        | AccountSyncConfigData::OutlookWork {
                            client_id: Some(id),
                            ..
                        },
                    ) => Some(id.clone()),
                    _ => None,
                })
                .or_else(|| mxr_provider_outlook::BUNDLED_CLIENT_ID.map(String::from));
            match cid {
                None => {
                    ok = false;
                    send = Some(account_step(
                        false,
                        "No client_id and no bundled OUTLOOK_CLIENT_ID for Outlook send".into(),
                    ));
                }
                Some(cid) => {
                    let auth_inst = std::sync::Arc::new(mxr_provider_outlook::OutlookAuth::new(
                        cid, token_ref, tenant,
                    ));
                    let email = account.email.clone();
                    let token_fn: std::sync::Arc<
                        dyn Fn() -> futures::future::BoxFuture<'static, anyhow::Result<String>>
                            + Send
                            + Sync,
                    > = std::sync::Arc::new(move || {
                        let a = auth_inst.clone();
                        Box::pin(async move {
                            a.get_valid_access_token()
                                .await
                                .map_err(|e| anyhow::anyhow!(e))
                        })
                    });
                    let smtp_host = match tenant {
                        mxr_provider_outlook::OutlookTenant::Personal => "smtp-mail.outlook.com",
                        mxr_provider_outlook::OutlookTenant::Work => "smtp.office365.com",
                    };
                    let provider = mxr_provider_outlook::OutlookSmtpSendProvider::new(
                        smtp_host.to_string(),
                        587,
                        email,
                        token_fn,
                    );
                    match provider.test_connection().await {
                        Ok(()) => {
                            send = Some(account_step(true, "Outlook SMTP ok".into()));
                        }
                        Err(error) => {
                            ok = false;
                            send = Some(account_step(false, error));
                        }
                    }
                }
            }
        }
        Some(AccountSendConfigData::Fake) => {
            send = Some(account_step(true, "Fake send provider (test-only)".into()));
        }
        Some(AccountSendConfigData::Smtp {
            host,
            port,
            username,
            password_ref,
            auth_required,
            use_tls,
            ..
        }) => {
            let config = match crate::provider_credentials::smtp_config_with_credentials(
                host,
                port,
                username,
                password_ref,
                auth_required,
                use_tls,
            ) {
                Ok(config) => config,
                Err(error) => {
                    ok = false;
                    send = Some(account_step(false, error.to_string()));
                    return account_operation_result(
                        ok,
                        format!("Account '{}' test failed.", account.key),
                        None,
                        auth,
                        sync,
                        send,
                    );
                }
            };
            let provider = mxr_provider_smtp::SmtpSendProvider::new(config);
            match provider.test_connection().await {
                Ok(()) => {
                    send = Some(account_step(true, "SMTP send ok".into()));
                }
                Err(error) => {
                    ok = false;
                    send = Some(account_step(false, error.to_string()));
                }
            }
        }
        None if account.sync.is_none() => {
            ok = false;
            send = Some(account_step(
                false,
                "No sync or send configuration provided.".into(),
            ));
        }
        None => {}
    }

    account_operation_result(
        ok,
        if ok {
            format!("Account '{}' test passed.", account.key)
        } else {
            format!("Account '{}' test failed.", account.key)
        },
        None,
        auth,
        sync,
        send,
    )
}

fn account_step(ok: bool, detail: String) -> AccountOperationStep {
    AccountOperationStep { ok, detail }
}

fn account_operation_result(
    ok: bool,
    summary: String,
    save: Option<AccountOperationStep>,
    auth: Option<AccountOperationStep>,
    sync: Option<AccountOperationStep>,
    send: Option<AccountOperationStep>,
) -> AccountOperationResult {
    AccountOperationResult {
        ok,
        summary,
        save,
        auth,
        sync,
        send,
        device_code_url: None,
        device_code_user_code: None,
    }
}

fn resolve_gmail_credentials(
    credential_source: GmailCredentialSourceData,
    client_id: String,
    client_secret: Option<String>,
) -> Result<(String, String), String> {
    match credential_source {
        GmailCredentialSourceData::Bundled => {
            match (
                mxr_provider_gmail::auth::BUNDLED_CLIENT_ID,
                mxr_provider_gmail::auth::BUNDLED_CLIENT_SECRET,
            ) {
                (Some(id), Some(secret)) => Ok((id.to_string(), secret.to_string())),
                _ => {
                    if client_id.trim().is_empty()
                        || client_secret.as_deref().unwrap_or("").trim().is_empty()
                    {
                        Err("This mxr build does not include one-click Gmail OAuth credentials. Install an official release build, run `mxr demo`, or switch Gmail Credential source to Custom and enter your own Google OAuth client ID/client secret.".into())
                    } else {
                        Ok((client_id, client_secret.unwrap_or_default()))
                    }
                }
            }
        }
        GmailCredentialSourceData::Custom => {
            if client_id.trim().is_empty()
                || client_secret.as_deref().unwrap_or("").trim().is_empty()
            {
                Err("Custom Gmail OAuth requires both client ID and client secret.".into())
            } else {
                Ok((client_id, client_secret.unwrap_or_default()))
            }
        }
    }
}

fn sync_config_to_data(sync: mxr_config::SyncProviderConfig) -> AccountSyncConfigData {
    match sync {
        mxr_config::SyncProviderConfig::Gmail {
            credential_source,
            client_id,
            client_secret,
            token_ref,
        } => AccountSyncConfigData::Gmail {
            credential_source: match credential_source {
                mxr_config::GmailCredentialSource::Bundled => GmailCredentialSourceData::Bundled,
                mxr_config::GmailCredentialSource::Custom => GmailCredentialSourceData::Custom,
            },
            client_id,
            client_secret,
            token_ref,
        },
        mxr_config::SyncProviderConfig::Imap {
            host,
            port,
            username,
            password_ref,
            auth_required,
            use_tls,
        } => AccountSyncConfigData::Imap {
            host,
            port,
            username,
            password_ref,
            password: None,
            auth_required,
            use_tls,
        },
        mxr_config::SyncProviderConfig::OutlookPersonal {
            client_id,
            token_ref,
        } => AccountSyncConfigData::OutlookPersonal {
            client_id,
            token_ref,
        },
        mxr_config::SyncProviderConfig::OutlookWork {
            client_id,
            token_ref,
        } => AccountSyncConfigData::OutlookWork {
            client_id,
            token_ref,
        },
        mxr_config::SyncProviderConfig::Fake => AccountSyncConfigData::Fake,
    }
}

fn config_account_id(key: &str, account: &mxr_config::AccountConfig) -> mxr_core::AccountId {
    let kind = account
        .sync
        .as_ref()
        .map(config_sync_kind_label)
        .or_else(|| account.send.as_ref().map(config_send_kind_label))
        .unwrap_or_else(|| key.to_string());
    mxr_core::AccountId::from_provider_id(&kind, &account.email)
}

fn config_sync_kind_label(sync: &mxr_config::SyncProviderConfig) -> String {
    match sync {
        mxr_config::SyncProviderConfig::Gmail { .. } => "gmail".into(),
        mxr_config::SyncProviderConfig::Imap { .. } => "imap".into(),
        mxr_config::SyncProviderConfig::OutlookPersonal { .. } => "outlook".into(),
        mxr_config::SyncProviderConfig::OutlookWork { .. } => "outlook-work".into(),
        mxr_config::SyncProviderConfig::Fake => "fake".into(),
    }
}

fn config_send_kind_label(send: &mxr_config::SendProviderConfig) -> String {
    match send {
        mxr_config::SendProviderConfig::Gmail => "gmail".into(),
        mxr_config::SendProviderConfig::Smtp { .. } => "smtp".into(),
        mxr_config::SendProviderConfig::OutlookPersonal { .. } => "outlook".into(),
        mxr_config::SendProviderConfig::OutlookWork { .. } => "outlook-work".into(),
        mxr_config::SendProviderConfig::Fake => "fake".into(),
    }
}

fn account_primary_provider_kind(account: &mxr_config::AccountConfig) -> String {
    account
        .sync
        .as_ref()
        .map(config_sync_kind_label)
        .or_else(|| account.send.as_ref().map(config_send_kind_label))
        .unwrap_or_else(|| "unknown".into())
}

fn config_account_capabilities(account: &mxr_config::AccountConfig) -> AccountCapabilitiesData {
    let mut capabilities = account
        .sync
        .as_ref()
        .map(config_sync_capabilities)
        .unwrap_or_default();
    capabilities.supports_send = account.send.is_some();
    capabilities
}

fn config_sync_capabilities(sync: &mxr_config::SyncProviderConfig) -> AccountCapabilitiesData {
    match sync {
        mxr_config::SyncProviderConfig::Gmail { .. } => AccountCapabilitiesData {
            labels: true,
            server_search: true,
            delta_sync: true,
            batch_operations: true,
            native_thread_ids: true,
            ..AccountCapabilitiesData::default()
        },
        mxr_config::SyncProviderConfig::Imap { .. } => AccountCapabilitiesData {
            server_search: true,
            delta_sync: true,
            ..AccountCapabilitiesData::default()
        },
        mxr_config::SyncProviderConfig::Fake => AccountCapabilitiesData {
            labels: true,
            native_thread_ids: true,
            ..AccountCapabilitiesData::default()
        },
        mxr_config::SyncProviderConfig::OutlookPersonal { .. }
        | mxr_config::SyncProviderConfig::OutlookWork { .. } => AccountCapabilitiesData::default(),
    }
}

fn provider_kind_label(kind: &mxr_core::ProviderKind) -> &'static str {
    match kind {
        mxr_core::ProviderKind::Gmail => "gmail",
        mxr_core::ProviderKind::Imap => "imap",
        mxr_core::ProviderKind::Smtp => "smtp",
        mxr_core::ProviderKind::OutlookPersonal => "outlook-personal",
        mxr_core::ProviderKind::OutlookWork => "outlook-work",
        mxr_core::ProviderKind::Fake => "fake",
    }
}

fn send_config_to_data(send: mxr_config::SendProviderConfig) -> AccountSendConfigData {
    match send {
        mxr_config::SendProviderConfig::Gmail => AccountSendConfigData::Gmail,
        mxr_config::SendProviderConfig::OutlookPersonal {
            client_id,
            token_ref,
        } => AccountSendConfigData::OutlookPersonal {
            client_id,
            token_ref,
        },
        mxr_config::SendProviderConfig::OutlookWork {
            client_id,
            token_ref,
        } => AccountSendConfigData::OutlookWork {
            client_id,
            token_ref,
        },
        mxr_config::SendProviderConfig::Smtp {
            host,
            port,
            username,
            password_ref,
            auth_required,
            use_tls,
        } => AccountSendConfigData::Smtp {
            host,
            port,
            username,
            password_ref,
            password: None,
            auth_required,
            use_tls,
        },
        mxr_config::SendProviderConfig::Fake => AccountSendConfigData::Fake,
    }
}

fn sync_data_to_config(
    data: AccountSyncConfigData,
) -> Result<mxr_config::SyncProviderConfig, String> {
    match data {
        AccountSyncConfigData::Gmail {
            credential_source,
            client_id,
            client_secret,
            token_ref,
        } => Ok(mxr_config::SyncProviderConfig::Gmail {
            credential_source: match credential_source {
                GmailCredentialSourceData::Bundled => mxr_config::GmailCredentialSource::Bundled,
                GmailCredentialSourceData::Custom => mxr_config::GmailCredentialSource::Custom,
            },
            client_id,
            client_secret,
            token_ref,
        }),
        AccountSyncConfigData::Imap {
            host,
            port,
            username,
            password_ref,
            auth_required,
            use_tls,
            ..
        } => Ok(mxr_config::SyncProviderConfig::Imap {
            host,
            port,
            username,
            password_ref,
            auth_required,
            use_tls,
        }),
        AccountSyncConfigData::OutlookPersonal {
            client_id,
            token_ref,
        } => Ok(mxr_config::SyncProviderConfig::OutlookPersonal {
            client_id,
            token_ref,
        }),
        AccountSyncConfigData::OutlookWork {
            client_id,
            token_ref,
        } => Ok(mxr_config::SyncProviderConfig::OutlookWork {
            client_id,
            token_ref,
        }),
        AccountSyncConfigData::Fake => Ok(mxr_config::SyncProviderConfig::Fake),
    }
}

fn send_data_to_config(
    data: AccountSendConfigData,
) -> Result<mxr_config::SendProviderConfig, String> {
    match data {
        AccountSendConfigData::Gmail => Ok(mxr_config::SendProviderConfig::Gmail),
        AccountSendConfigData::OutlookPersonal {
            client_id,
            token_ref,
        } => Ok(mxr_config::SendProviderConfig::OutlookPersonal {
            client_id,
            token_ref,
        }),
        AccountSendConfigData::OutlookWork {
            client_id,
            token_ref,
        } => Ok(mxr_config::SendProviderConfig::OutlookWork {
            client_id,
            token_ref,
        }),
        AccountSendConfigData::Fake => Ok(mxr_config::SendProviderConfig::Fake),
        AccountSendConfigData::Smtp {
            host,
            port,
            username,
            password_ref,
            auth_required,
            use_tls,
            ..
        } => Ok(mxr_config::SendProviderConfig::Smtp {
            host,
            port,
            username,
            password_ref,
            auth_required,
            use_tls,
        }),
    }
}

fn persist_account_passwords(account: &AccountConfigData) -> anyhow::Result<()> {
    tracing::debug!(
        account_key = %account.key,
        sync_kind = %match account.sync {
            Some(AccountSyncConfigData::Gmail { .. }) => "gmail",
            Some(AccountSyncConfigData::Imap { .. }) => "imap",
            Some(AccountSyncConfigData::OutlookPersonal { .. }) => "outlook",
            Some(AccountSyncConfigData::OutlookWork { .. }) => "outlook-work",
            Some(AccountSyncConfigData::Fake) => "fake",
            None => "none",
        },
        send_kind = %match account.send {
            Some(AccountSendConfigData::Gmail) => "gmail",
            Some(AccountSendConfigData::Smtp { .. }) => "smtp",
            Some(AccountSendConfigData::OutlookPersonal { .. }) => "outlook",
            Some(AccountSendConfigData::OutlookWork { .. }) => "outlook-work",
            Some(AccountSendConfigData::Fake) => "fake",
            None => "none",
        },
        has_inline_imap_password = matches!(
            account.sync,
            Some(AccountSyncConfigData::Imap {
                password: Some(ref password),
                ..
            }) if !password.is_empty()
        ),
        has_inline_smtp_password = matches!(
            account.send,
            Some(AccountSendConfigData::Smtp {
                password: Some(ref password),
                ..
            }) if !password.is_empty()
        ),
        "persisting inline account credentials if supplied"
    );

    if let Some(AccountSyncConfigData::Imap {
        auth_required,
        username,
        password_ref,
        password: Some(password),
        ..
    }) = &account.sync
    {
        persist_account_password("IMAP", *auth_required, username, password_ref, password)?;
    }

    if let Some(AccountSendConfigData::Smtp {
        auth_required,
        username,
        password_ref,
        password: Some(password),
        ..
    }) = &account.send
    {
        persist_account_password("SMTP", *auth_required, username, password_ref, password)?;
    }

    Ok(())
}

fn repair_account_passwords(account: &AccountConfigData) -> anyhow::Result<usize> {
    let mut repaired = 0usize;
    let mut repairable = 0usize;

    if let Some(AccountSyncConfigData::Imap {
        auth_required,
        username,
        password_ref,
        password,
        ..
    }) = &account.sync
    {
        if *auth_required {
            repairable += 1;
            let password = password
                .as_deref()
                .filter(|password| !password.is_empty())
                .ok_or_else(|| {
                    anyhow::anyhow!("IMAP password is required to repair this account.")
                })?;
            persist_account_password("IMAP", true, username, password_ref, password)?;
            repaired += 1;
        }
    }

    if let Some(AccountSendConfigData::Smtp {
        auth_required,
        username,
        password_ref,
        password,
        ..
    }) = &account.send
    {
        if *auth_required {
            repairable += 1;
            let password = password
                .as_deref()
                .filter(|password| !password.is_empty())
                .ok_or_else(|| {
                    anyhow::anyhow!("SMTP password is required to repair this account.")
                })?;
            persist_account_password("SMTP", true, username, password_ref, password)?;
            repaired += 1;
        }
    }

    if repairable == 0 {
        anyhow::bail!(
            "Account '{}' has no password-backed IMAP/SMTP credentials to repair",
            account.key
        );
    }

    Ok(repaired)
}

fn persist_account_password(
    service: &str,
    auth_required: bool,
    username: &str,
    password_ref: &str,
    password: &str,
) -> anyhow::Result<()> {
    if !auth_required || password.is_empty() {
        tracing::debug!(
            credential_service = service,
            password_ref,
            auth_required,
            password_supplied = !password.is_empty(),
            "skipping credential persist"
        );
        return Ok(());
    }
    if username.trim().is_empty() {
        anyhow::bail!("{service} user is required to store the password.");
    }
    if password_ref.trim().is_empty() {
        anyhow::bail!("{service} pass ref is required to store the password.");
    }
    tracing::info!(
        credential_service = service,
        password_ref,
        "persisting credential to keychain"
    );
    mxr_keychain::set_password(password_ref, username, password)?;
    tracing::info!(
        credential_service = service,
        password_ref,
        "credential persisted to keychain"
    );
    Ok(())
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

async fn handle_export_search(state: &AppState, query: &str, format: &ExportFormat) -> Response {
    let search_results = match state
        .search
        .search(query, 100, 0, mxr_core::types::SortOrder::DateDesc)
        .await
    {
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

    let sanitized = if sanitized.trim().is_empty() {
        format!("attachment-{}", attachment_id.as_str())
    } else {
        sanitized
    };

    // APFS limits individual path components to 255 bytes. Some
    // real-world attachment filenames exceed that once MIME-decoded;
    // keep the extension when possible and add a stable attachment-id
    // suffix to avoid collisions after truncation.
    if sanitized.len() <= MAX_ATTACHMENT_FILENAME_BYTES {
        return sanitized;
    }

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
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::TimeZone;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex as StdMutex;

    #[test]
    fn sanitized_attachment_filename_truncates_long_names_preserving_extension() {
        let attachment_id = mxr_core::AttachmentId::from_provider_id("test", "long-pdf");
        let filename = format!("{}.pdf", "a".repeat(400));

        let sanitized = sanitized_attachment_filename(&filename, &attachment_id);

        assert!(
            sanitized.len() <= 220,
            "filename should fit conservative path component limit: {} bytes",
            sanitized.len()
        );
        assert!(sanitized.ends_with(&format!("-{}.pdf", attachment_id.as_str())));
    }

    #[test]
    fn sanitized_attachment_filename_truncates_utf8_on_char_boundary() {
        let attachment_id = mxr_core::AttachmentId::from_provider_id("test", "utf8-pdf");
        let filename = format!("{}.pdf", "é".repeat(200));

        let sanitized = sanitized_attachment_filename(&filename, &attachment_id);

        assert!(
            sanitized.len() <= 220,
            "filename should fit conservative path component limit: {} bytes",
            sanitized.len()
        );
        assert!(sanitized.ends_with(&format!("-{}.pdf", attachment_id.as_str())));
    }

    #[test]
    fn sanitized_attachment_filename_uses_stable_fallback_for_blank_names() {
        let attachment_id = mxr_core::AttachmentId::from_provider_id("test", "blank");

        let sanitized = sanitized_attachment_filename("   ", &attachment_id);

        assert_eq!(sanitized, format!("attachment-{}", attachment_id.as_str()));
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum FolderCopyReanchorMode {
        Normal,
        MissingAfterArchive,
    }

    struct FolderCopyProvider {
        account_id: mxr_core::AccountId,
        reanchor_mode: FolderCopyReanchorMode,
        folders: StdMutex<Vec<String>>,
        last_synced_provider_ids: StdMutex<Vec<String>>,
    }

    impl FolderCopyProvider {
        fn with_reanchor_mode(
            account_id: mxr_core::AccountId,
            reanchor_mode: FolderCopyReanchorMode,
        ) -> Self {
            Self {
                account_id,
                reanchor_mode,
                folders: StdMutex::new(vec!["INBOX".to_string()]),
                last_synced_provider_ids: StdMutex::new(Vec::new()),
            }
        }

        fn current_provider_ids(&self) -> Vec<String> {
            self.folders
                .lock()
                .unwrap()
                .iter()
                .map(|folder| format!("{folder}:1"))
                .collect()
        }

        fn synced_messages(&self) -> Vec<mxr_core::SyncedMessage> {
            self.folders
                .lock()
                .unwrap()
                .iter()
                .map(|folder| {
                    let provider_id = format!("{folder}:1");
                    let message_id =
                        mxr_core::MessageId::from_provider_id("folder-copy", &provider_id);
                    let envelope = mxr_core::Envelope {
                        id: message_id.clone(),
                        account_id: self.account_id.clone(),
                        provider_id,
                        thread_id: mxr_core::ThreadId::from_provider_id("folder-copy", "thread-1"),
                        message_id_header: Some("<folder-copy@example.com>".to_string()),
                        in_reply_to: None,
                        references: vec![],
                        from: mxr_core::Address {
                            name: Some("Folder Provider".to_string()),
                            email: "folder-provider@example.com".to_string(),
                        },
                        to: vec![mxr_core::Address {
                            name: Some("Receiver".to_string()),
                            email: "receiver@example.com".to_string(),
                        }],
                        cc: vec![],
                        bcc: vec![],
                        subject: "Folder-backed message".to_string(),
                        date: chrono::Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
                        flags: mxr_core::MessageFlags::READ,
                        snippet: format!("copy in {folder}"),
                        has_attachments: false,
                        size_bytes: 128,
                        unsubscribe: mxr_core::UnsubscribeMethod::None,
                        label_provider_ids: vec![folder.clone()],
                    };
                    let body = mxr_core::MessageBody {
                        message_id,
                        text_plain: Some(format!("body in {folder}")),
                        text_html: None,
                        attachments: vec![],
                        fetched_at: chrono::Utc::now(),
                        metadata: mxr_core::MessageMetadata::default(),
                    };
                    mxr_core::SyncedMessage { envelope, body }
                })
                .collect()
        }

        fn sync_labels_for_account(&self) -> Vec<mxr_core::Label> {
            let folders = self.folders.lock().unwrap().clone();
            ["INBOX", "Archive"]
                .into_iter()
                .map(|name| {
                    let kind = if name == "INBOX" {
                        mxr_core::LabelKind::System
                    } else {
                        mxr_core::LabelKind::Folder
                    };
                    let count = folders
                        .iter()
                        .filter(|folder| folder.eq_ignore_ascii_case(name))
                        .count() as u32;
                    mxr_core::Label {
                        id: mxr_core::LabelId::from_provider_id("folder-copy", name),
                        account_id: self.account_id.clone(),
                        name: name.to_string(),
                        kind,
                        color: None,
                        provider_id: name.to_string(),
                        unread_count: 0,
                        total_count: count,
                    }
                })
                .collect()
        }
    }

    struct FailingSendProvider {
        message: &'static str,
    }

    struct UnsupportedServerDraftProvider;

    struct FailingSyncProvider {
        account_id: mxr_core::AccountId,
        message: &'static str,
        calls: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl mxr_core::MailSendProvider for FailingSendProvider {
        fn name(&self) -> &str {
            "failing-send"
        }

        async fn send(
            &self,
            _draft: &mxr_core::Draft,
            _from: &mxr_core::Address,
            _rfc2822_message_id: &str,
        ) -> Result<mxr_core::SendReceipt, mxr_core::MxrError> {
            Err(mxr_core::MxrError::Provider(self.message.to_string()))
        }
    }

    #[async_trait]
    impl mxr_core::MailSendProvider for UnsupportedServerDraftProvider {
        fn name(&self) -> &str {
            "unsupported-server-draft"
        }

        async fn send(
            &self,
            _draft: &mxr_core::Draft,
            _from: &mxr_core::Address,
            _rfc2822_message_id: &str,
        ) -> Result<mxr_core::SendReceipt, mxr_core::MxrError> {
            unreachable!("save_draft_to_server fallback test must not send")
        }
    }

    #[async_trait]
    impl mxr_core::MailSyncProvider for FailingSyncProvider {
        fn name(&self) -> &str {
            "failing-sync"
        }

        fn account_id(&self) -> &mxr_core::AccountId {
            &self.account_id
        }

        fn capabilities(&self) -> mxr_core::SyncCapabilities {
            mxr_core::SyncCapabilities {
                labels: true,
                server_search: false,
                delta_sync: false,
                push: false,
                batch_operations: false,
                native_thread_ids: true,
            }
        }

        async fn authenticate(&mut self) -> Result<(), mxr_core::MxrError> {
            Ok(())
        }

        async fn refresh_auth(&mut self) -> Result<(), mxr_core::MxrError> {
            Ok(())
        }

        async fn sync_labels(&self) -> Result<Vec<mxr_core::Label>, mxr_core::MxrError> {
            Ok(Vec::new())
        }

        async fn sync_messages(
            &self,
            _cursor: &mxr_core::SyncCursor,
        ) -> Result<mxr_core::SyncBatch, mxr_core::MxrError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Err(mxr_core::MxrError::Provider(self.message.to_string()))
        }

        async fn fetch_attachment(
            &self,
            _provider_message_id: &str,
            _provider_attachment_id: &str,
        ) -> Result<Vec<u8>, mxr_core::MxrError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Err(mxr_core::MxrError::Provider(self.message.to_string()))
        }

        async fn modify_labels(
            &self,
            _provider_message_id: &str,
            _add: &[String],
            _remove: &[String],
        ) -> Result<(), mxr_core::MxrError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Err(mxr_core::MxrError::Provider(self.message.to_string()))
        }

        async fn trash(&self, _provider_message_id: &str) -> Result<(), mxr_core::MxrError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Err(mxr_core::MxrError::Provider(self.message.to_string()))
        }

        async fn set_read(
            &self,
            _provider_message_id: &str,
            _read: bool,
        ) -> Result<(), mxr_core::MxrError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Err(mxr_core::MxrError::Provider(self.message.to_string()))
        }

        async fn set_starred(
            &self,
            _provider_message_id: &str,
            _starred: bool,
        ) -> Result<(), mxr_core::MxrError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Err(mxr_core::MxrError::Provider(self.message.to_string()))
        }
    }

    #[async_trait]
    impl mxr_core::MailSyncProvider for FolderCopyProvider {
        fn name(&self) -> &str {
            "folder-copy"
        }

        fn account_id(&self) -> &mxr_core::AccountId {
            &self.account_id
        }

        fn capabilities(&self) -> mxr_core::SyncCapabilities {
            mxr_core::SyncCapabilities {
                labels: false,
                server_search: false,
                delta_sync: false,
                push: false,
                batch_operations: false,
                native_thread_ids: true,
            }
        }

        async fn authenticate(&mut self) -> Result<(), mxr_core::MxrError> {
            Ok(())
        }

        async fn refresh_auth(&mut self) -> Result<(), mxr_core::MxrError> {
            Ok(())
        }

        async fn sync_labels(&self) -> Result<Vec<mxr_core::Label>, mxr_core::MxrError> {
            Ok(self.sync_labels_for_account())
        }

        async fn sync_messages(
            &self,
            _cursor: &mxr_core::SyncCursor,
        ) -> Result<mxr_core::SyncBatch, mxr_core::MxrError> {
            let current_provider_ids = self.current_provider_ids();
            let mut last_synced = self.last_synced_provider_ids.lock().unwrap();
            let deleted_provider_ids = last_synced
                .iter()
                .filter(|provider_id| !current_provider_ids.contains(provider_id))
                .cloned()
                .collect();
            *last_synced = current_provider_ids;

            Ok(mxr_core::SyncBatch {
                upserted: self.synced_messages(),
                deleted_provider_ids,
                label_changes: vec![],
                next_cursor: mxr_core::SyncCursor::Initial,
            })
        }

        async fn fetch_attachment(
            &self,
            _provider_message_id: &str,
            _provider_attachment_id: &str,
        ) -> Result<Vec<u8>, mxr_core::MxrError> {
            Ok(vec![])
        }

        async fn modify_labels(
            &self,
            provider_message_id: &str,
            add: &[String],
            remove: &[String],
        ) -> Result<(), mxr_core::MxrError> {
            let source_folder = provider_message_id
                .rsplit_once(':')
                .map(|(folder, _)| folder.to_string())
                .unwrap_or_else(|| "INBOX".to_string());
            let mut folders = self.folders.lock().unwrap();

            let added_folders: Vec<String> = add
                .iter()
                .filter(|label| {
                    !matches!(
                        label.to_ascii_uppercase().as_str(),
                        "READ" | "SEEN" | "STARRED" | "FLAGGED" | "DRAFT" | "DRAFTS" | "ANSWERED"
                    )
                })
                .cloned()
                .collect();
            let removed_folders: Vec<String> = remove
                .iter()
                .filter(|label| {
                    !matches!(
                        label.to_ascii_uppercase().as_str(),
                        "READ" | "SEEN" | "STARRED" | "FLAGGED" | "DRAFT" | "DRAFTS" | "ANSWERED"
                    )
                })
                .cloned()
                .collect();

            if removed_folders
                .iter()
                .any(|folder| folder.eq_ignore_ascii_case("INBOX"))
                && added_folders.is_empty()
            {
                if self.reanchor_mode == FolderCopyReanchorMode::MissingAfterArchive {
                    folders.clear();
                    return Ok(());
                }

                folders.retain(|folder| !folder.eq_ignore_ascii_case("INBOX"));
                if !folders
                    .iter()
                    .any(|folder| folder.eq_ignore_ascii_case("Archive"))
                {
                    folders.push("Archive".to_string());
                }
                return Ok(());
            }

            if added_folders
                .iter()
                .any(|folder| folder.eq_ignore_ascii_case("INBOX"))
                && folders
                    .iter()
                    .all(|folder| !folder.eq_ignore_ascii_case("INBOX"))
                && folders
                    .iter()
                    .any(|folder| folder.eq_ignore_ascii_case("Archive"))
                && removed_folders.is_empty()
            {
                folders.clear();
                folders.push("INBOX".to_string());
                return Ok(());
            }

            folders.retain(|folder| {
                !removed_folders
                    .iter()
                    .any(|removed| removed.eq_ignore_ascii_case(folder))
            });

            for folder in added_folders {
                if !folders
                    .iter()
                    .any(|existing| existing.eq_ignore_ascii_case(&folder))
                {
                    folders.push(folder);
                }
            }

            if folders.is_empty() {
                folders.push(source_folder);
            }

            Ok(())
        }

        async fn trash(&self, _provider_message_id: &str) -> Result<(), mxr_core::MxrError> {
            Ok(())
        }

        async fn set_read(
            &self,
            _provider_message_id: &str,
            _read: bool,
        ) -> Result<(), mxr_core::MxrError> {
            Ok(())
        }

        async fn set_starred(
            &self,
            _provider_message_id: &str,
            _starred: bool,
        ) -> Result<(), mxr_core::MxrError> {
            Ok(())
        }
    }

    async fn folder_copy_state() -> Arc<AppState> {
        folder_copy_state_with_mode(FolderCopyReanchorMode::Normal).await
    }

    async fn folder_copy_state_with_mode(reanchor_mode: FolderCopyReanchorMode) -> Arc<AppState> {
        let account_id = mxr_core::AccountId::from_provider_id("imap", "folder-copy@example.com");
        let account = mxr_core::Account {
            id: account_id.clone(),
            name: "Folder Copy".to_string(),
            email: "folder-copy@example.com".to_string(),
            sync_backend: Some(mxr_core::BackendRef {
                provider_kind: mxr_core::ProviderKind::Imap,
                config_key: "folder-copy".to_string(),
            }),
            send_backend: None,
            enabled: true,
        };
        let provider = Arc::new(FolderCopyProvider::with_reanchor_mode(
            account_id,
            reanchor_mode,
        ));
        let provider: Arc<dyn mxr_core::MailSyncProvider> = provider;
        Arc::new(
            AppState::in_memory_with_sync_provider(account, provider, None)
                .await
                .unwrap(),
        )
    }

    #[tokio::test]
    async fn dispatch_ping_returns_pong() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::Ping),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Pong,
            }) => {}
            other => panic!("Expected Pong, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_list_envelopes_after_sync() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        // Initial sync
        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();

        let msg = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::ListEnvelopes {
                label_id: None,
                account_id: None,
                limit: 100,
                offset: 0,
            }),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Envelopes { envelopes },
            }) => {
                assert_eq!(envelopes.len(), 55);
            }
            other => panic!("Expected Envelopes, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_list_envelopes_by_label() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();

        // Get labels first
        let labels_msg = IpcMessage {
            id: 10,
            payload: IpcPayload::Request(Request::ListLabels { account_id: None }),
        };
        let resp = handle_request(&state, &labels_msg).await;
        let labels = match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Labels { labels },
            }) => labels,
            other => panic!("Expected Labels, got {:?}", other),
        };

        // Find Inbox label
        let inbox = labels
            .iter()
            .find(|l| l.name == "Inbox")
            .expect("Inbox label missing");

        // Fetch envelopes by Inbox label
        let msg = IpcMessage {
            id: 11,
            payload: IpcPayload::Request(Request::ListEnvelopes {
                label_id: Some(inbox.id.clone()),
                account_id: None,
                limit: 100,
                offset: 0,
            }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Envelopes { envelopes },
            }) => {
                assert!(
                    !envelopes.is_empty(),
                    "Inbox label should have envelopes, got 0. Inbox label_id={}",
                    inbox.id
                );
            }
            IpcPayload::Response(Response::Error { message, .. }) => {
                panic!("Got error response: {message}");
            }
            other => panic!("Expected Envelopes, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_list_labels_without_accounts_returns_empty() {
        let state = Arc::new(AppState::in_memory_without_accounts().await.unwrap());

        let msg = IpcMessage {
            id: 12,
            payload: IpcPayload::Request(Request::ListLabels { account_id: None }),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Labels { labels },
            }) => assert!(labels.is_empty()),
            other => panic!("Expected Labels, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_list_envelopes_without_accounts_returns_empty() {
        let state = Arc::new(AppState::in_memory_without_accounts().await.unwrap());

        let msg = IpcMessage {
            id: 13,
            payload: IpcPayload::Request(Request::ListEnvelopes {
                label_id: None,
                account_id: None,
                limit: 100,
                offset: 0,
            }),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Envelopes { envelopes },
            }) => assert!(envelopes.is_empty()),
            other => panic!("Expected Envelopes, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_create_label_persists_and_returns_label() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let account_id = state.default_account_id();

        let create_msg = IpcMessage {
            id: 14,
            payload: IpcPayload::Request(Request::CreateLabel {
                name: "Urgent".to_string(),
                color: Some("#ff6600".to_string()),
                account_id: Some(account_id.clone()),
            }),
        };
        let resp = handle_request(&state, &create_msg).await;
        let created = match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Label { label },
            }) => label,
            other => panic!("Expected Label, got {:?}", other),
        };
        assert_eq!(created.name, "Urgent");
        assert_eq!(created.color.as_deref(), Some("#ff6600"));
        assert_eq!(created.account_id, account_id);

        let list_msg = IpcMessage {
            id: 15,
            payload: IpcPayload::Request(Request::ListLabels {
                account_id: Some(account_id),
            }),
        };
        let resp = handle_request(&state, &list_msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Labels { labels },
            }) => {
                assert!(labels.iter().any(|label| label.name == "Urgent"));
            }
            other => panic!("Expected Labels, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_upsert_and_list_rules() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let now = chrono::Utc::now();
        let rule = serde_json::json!({
            "id": "rule-1",
            "name": "Archive newsletters",
            "enabled": true,
            "priority": 10,
            "conditions": {"type":"field","field":"has_label","label":"newsletters"},
            "actions": [{"type":"archive"}],
            "created_at": now,
            "updated_at": now
        });

        let upsert_msg = IpcMessage {
            id: 20,
            payload: IpcPayload::Request(Request::UpsertRule { rule: rule.clone() }),
        };
        let resp = handle_request(&state, &upsert_msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::RuleData { rule: returned },
            }) => {
                assert_eq!(returned["name"], "Archive newsletters");
            }
            other => panic!("Expected RuleData, got {:?}", other),
        }

        let list_msg = IpcMessage {
            id: 21,
            payload: IpcPayload::Request(Request::ListRules),
        };
        let resp = handle_request(&state, &list_msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Rules { rules },
            }) => {
                assert_eq!(rules.len(), 1);
                assert_eq!(rules[0]["id"], "rule-1");
            }
            other => panic!("Expected Rules, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_dry_run_rules_returns_matching_messages() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();
        let now = chrono::Utc::now();
        let rule = serde_json::json!({
            "id": "rule-1",
            "name": "Mark unread",
            "enabled": true,
            "priority": 10,
            "conditions": {"type":"field","field":"is_unread"},
            "actions": [{"type":"mark_read"}],
            "created_at": now,
            "updated_at": now
        });
        let _ = handle_request(
            &state,
            &IpcMessage {
                id: 22,
                payload: IpcPayload::Request(Request::UpsertRule { rule }),
            },
        )
        .await;

        let dry_run_msg = IpcMessage {
            id: 23,
            payload: IpcPayload::Request(Request::DryRunRules {
                rule: Some("rule-1".to_string()),
                all: false,
                after: None,
            }),
        };
        let resp = handle_request(&state, &dry_run_msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::RuleDryRun { results },
            }) => {
                assert_eq!(results.len(), 1);
                let matches = results[0]["matches"]
                    .as_array()
                    .expect("matches should be an array");
                assert!(matches.len() >= 1);
            }
            other => panic!("Expected RuleDryRun, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_upsert_rule_form_and_get_rule_form() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        let upsert_msg = IpcMessage {
            id: 231,
            payload: IpcPayload::Request(Request::UpsertRuleForm {
                existing_rule: None,
                name: "Archive unread".into(),
                condition: "is:unread".into(),
                action: "archive".into(),
                priority: 25,
                enabled: true,
            }),
        };
        let resp = handle_request(&state, &upsert_msg).await;
        let rule_id = match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::RuleData { rule },
            }) => {
                assert_eq!(rule["name"], "Archive unread");
                rule["id"].as_str().unwrap().to_string()
            }
            other => panic!("Expected RuleData, got {:?}", other),
        };

        let get_form_msg = IpcMessage {
            id: 232,
            payload: IpcPayload::Request(Request::GetRuleForm { rule: rule_id }),
        };
        let resp = handle_request(&state, &get_form_msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::RuleFormData { form },
            }) => {
                assert_eq!(form.name, "Archive unread");
                assert_eq!(form.condition, "is:unread");
                assert_eq!(form.action, "archive");
                assert_eq!(form.priority, 25);
                assert!(form.enabled);
            }
            other => panic!("Expected RuleFormData, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_rename_label_updates_visible_label() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let account_id = state.default_account_id();

        let create_msg = IpcMessage {
            id: 14,
            payload: IpcPayload::Request(Request::CreateLabel {
                name: "Projects".to_string(),
                color: None,
                account_id: Some(account_id.clone()),
            }),
        };
        let _ = handle_request(&state, &create_msg).await;

        let rename_msg = IpcMessage {
            id: 15,
            payload: IpcPayload::Request(Request::RenameLabel {
                old: "Projects".to_string(),
                new: "Client Work".to_string(),
                account_id: Some(account_id.clone()),
            }),
        };
        let resp = handle_request(&state, &rename_msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Label { label },
            }) => {
                assert_eq!(label.name, "Client Work");
                assert_eq!(label.provider_id, "Client Work");
            }
            other => panic!("Expected Label, got {:?}", other),
        }

        let list_msg = IpcMessage {
            id: 16,
            payload: IpcPayload::Request(Request::ListLabels {
                account_id: Some(account_id),
            }),
        };
        let resp = handle_request(&state, &list_msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Labels { labels },
            }) => {
                assert!(labels.iter().any(|label| label.name == "Client Work"));
                assert!(!labels.iter().any(|label| label.name == "Projects"));
            }
            other => panic!("Expected Labels, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_delete_label_removes_it_from_store() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let account_id = state.default_account_id();

        let create_msg = IpcMessage {
            id: 17,
            payload: IpcPayload::Request(Request::CreateLabel {
                name: "Temporary".to_string(),
                color: None,
                account_id: Some(account_id.clone()),
            }),
        };
        let _ = handle_request(&state, &create_msg).await;

        let delete_msg = IpcMessage {
            id: 18,
            payload: IpcPayload::Request(Request::DeleteLabel {
                name: "Temporary".to_string(),
                account_id: Some(account_id.clone()),
            }),
        };
        let resp = handle_request(&state, &delete_msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack, got {:?}", other),
        }

        let list_msg = IpcMessage {
            id: 19,
            payload: IpcPayload::Request(Request::ListLabels {
                account_id: Some(account_id),
            }),
        };
        let resp = handle_request(&state, &list_msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Labels { labels },
            }) => {
                assert!(!labels.iter().any(|label| label.name == "Temporary"));
            }
            other => panic!("Expected Labels, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_count_after_sync() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();

        let msg = IpcMessage {
            id: 3,
            payload: IpcPayload::Request(Request::Count {
                query: "deployment".to_string(),
                mode: None,
            }),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Count { count },
            }) => {
                assert!(count > 0, "Expected non-zero count for 'deployment'");
            }
            other => panic!("Expected Count, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_list_saved_searches_empty() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        let msg = IpcMessage {
            id: 4,
            payload: IpcPayload::Request(Request::ListSavedSearches),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SavedSearches { searches },
            }) => {
                assert!(searches.is_empty());
            }
            other => panic!("Expected empty SavedSearches, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_create_and_list_saved_searches() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        // Create
        let create_msg = IpcMessage {
            id: 5,
            payload: IpcPayload::Request(Request::CreateSavedSearch {
                name: "Important".to_string(),
                query: "is:starred".to_string(),
                search_mode: mxr_core::SearchMode::Lexical,
            }),
        };
        let resp = handle_request(&state, &create_msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SavedSearchData { search },
            }) => {
                assert_eq!(search.name, "Important");
                assert_eq!(search.query, "is:starred");
                assert_eq!(search.search_mode, mxr_core::SearchMode::Lexical);
            }
            other => panic!("Expected SavedSearchData, got {:?}", other),
        }

        // List
        let list_msg = IpcMessage {
            id: 6,
            payload: IpcPayload::Request(Request::ListSavedSearches),
        };
        let resp = handle_request(&state, &list_msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SavedSearches { searches },
            }) => {
                assert_eq!(searches.len(), 1);
                assert_eq!(searches[0].name, "Important");
                assert_eq!(searches[0].search_mode, mxr_core::SearchMode::Lexical);
            }
            other => panic!("Expected SavedSearches, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_create_saved_search_persists_requested_mode() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let create_msg = IpcMessage {
            id: 51,
            payload: IpcPayload::Request(Request::CreateSavedSearch {
                name: "Hybrid".to_string(),
                query: "deployment".to_string(),
                search_mode: mxr_core::SearchMode::Hybrid,
            }),
        };

        let resp = handle_request(&state, &create_msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SavedSearchData { search },
            }) => {
                assert_eq!(search.search_mode, mxr_core::SearchMode::Hybrid);
            }
            other => panic!("Expected SavedSearchData, got {:?}", other),
        }

        let saved = state
            .store
            .get_saved_search_by_name("Hybrid")
            .await
            .unwrap()
            .expect("saved search");
        assert_eq!(saved.search_mode, mxr_core::SearchMode::Hybrid);
    }

    #[tokio::test]
    async fn dispatch_run_saved_search_returns_results() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();

        let create = IpcMessage {
            id: 200,
            payload: IpcPayload::Request(Request::CreateSavedSearch {
                name: "Deploy".into(),
                query: "deployment".into(),
                search_mode: mxr_core::SearchMode::Lexical,
            }),
        };
        handle_request(&state, &create).await;

        let msg = IpcMessage {
            id: 201,
            payload: IpcPayload::Request(Request::RunSavedSearch {
                name: "Deploy".into(),
                limit: 10,
            }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data:
                    ResponseData::SearchResults {
                        results,
                        has_more,
                        explain,
                        ..
                    },
            }) => {
                assert_eq!(has_more, false);
                assert_eq!(explain.is_none(), true);
                assert!(results.len() >= 1);
                assert!(results.len() <= 10);
                assert!(
                    results
                        .iter()
                        .all(|item| item.mode == mxr_core::SearchMode::Lexical),
                    "saved search should return lexical results"
                );
            }
            other => panic!("Expected SearchResults, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_status() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        let msg = IpcMessage {
            id: 7,
            payload: IpcPayload::Request(Request::GetStatus),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data:
                    ResponseData::Status {
                        uptime_secs: _,
                        accounts,
                        total_messages: _,
                        daemon_pid,
                        sync_statuses,
                        protocol_version,
                        daemon_version,
                        daemon_build_id,
                        repair_required,
                        ..
                    },
            }) => {
                assert_eq!(accounts.len(), 1);
                let daemon_pid = daemon_pid.expect("daemon pid should be present");
                assert!(daemon_pid > 0);
                assert_eq!(sync_statuses.len(), 1);
                assert!(protocol_version >= mxr_protocol::IPC_PROTOCOL_VERSION);
                let daemon_version = daemon_version.expect("daemon version should be present");
                assert_ne!(daemon_version, "");
                let daemon_build_id = daemon_build_id.expect("daemon build id should be present");
                assert_ne!(daemon_build_id, "");
                assert_eq!(repair_required, false);
            }
            other => panic!("Expected Status, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_status_reports_degraded_relationship_llm_features_when_llm_disabled() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        let msg = IpcMessage {
            id: 7,
            payload: IpcPayload::Request(Request::GetStatus),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data:
                    ResponseData::Status {
                        feature_health: Some(feature_health),
                        ..
                    },
            }) => {
                assert!(matches!(
                    feature_health.relationship_profile,
                    FeatureHealth::Degraded { .. }
                ));
                assert!(matches!(
                    feature_health.commitments,
                    FeatureHealth::Degraded { .. }
                ));
                assert!(matches!(
                    feature_health.humanizer,
                    FeatureHealth::Degraded { .. }
                ));
            }
            other => panic!("Expected Status with feature health, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn dispatch_status_does_not_block_when_search_is_busy() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        let msg = IpcMessage {
            id: 8,
            payload: IpcPayload::Request(Request::GetStatus),
        };

        let resp = tokio::time::timeout(
            std::time::Duration::from_millis(250),
            handle_request(&state, &msg),
        )
        .await
        .expect("status should not block on a busy search index");

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Status { .. },
            }) => {}
            other => panic!("Expected Status, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_shutdown_acknowledges_without_exiting() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        let msg = IpcMessage {
            id: 9,
            payload: IpcPayload::Request(Request::Shutdown),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack, got {:?}", other),
        }
        assert!(state.shutdown_requested());
    }

    #[tokio::test]
    async fn dispatch_doctor_report() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        let msg = IpcMessage {
            id: 81,
            payload: IpcPayload::Request(Request::GetDoctorReport),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::DoctorReport { report },
            }) => {
                assert!(report.database_path.contains("mxr.db"));
                assert!(report.index_path.contains("search_index"));
                let daemon_version = report.daemon_version.expect("doctor report daemon version");
                assert_ne!(daemon_version, "");
                let daemon_build_id = report.daemon_build_id.expect("doctor report build id");
                assert_ne!(daemon_build_id, "");
            }
            other => panic!("Expected DoctorReport, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_sync_status() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let account_id = state.default_account_id();

        let msg = IpcMessage {
            id: 82,
            payload: IpcPayload::Request(Request::GetSyncStatus { account_id }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SyncStatus { sync },
            }) => {
                assert_ne!(sync.account_name, "");
                let summary = sync
                    .current_cursor_summary
                    .expect("sync status should include cursor summary");
                assert_ne!(summary, "");
            }
            other => panic!("Expected SyncStatus, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_search_returns_results() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        // Sync first so search index is populated
        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();

        let msg = IpcMessage {
            id: 10,
            payload: IpcPayload::Request(Request::Search {
                query: "deployment".to_string(),
                limit: 10,
                offset: 0,
                mode: None,
                sort: None,
                explain: false,
            }),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SearchResults { results, .. },
            }) => {
                assert!(
                    results.len() >= 1,
                    "Search for 'deployment' should return results"
                );
                assert!(results.len() <= 10);
                assert_eq!(results[0].mode, mxr_core::SearchMode::Lexical);
            }
            other => panic!("Expected SearchResults, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_search_explain_returns_execution_details() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();

        let msg = IpcMessage {
            id: 11,
            payload: IpcPayload::Request(Request::Search {
                query: "deployment".to_string(),
                limit: 5,
                offset: 0,
                mode: Some(mxr_core::SearchMode::Lexical),
                sort: Some(mxr_core::types::SortOrder::DateDesc),
                explain: true,
            }),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data:
                    ResponseData::SearchResults {
                        results,
                        explain: Some(explain),
                        ..
                    },
            }) => {
                assert!(results.len() >= 1);
                assert!(results.len() <= 5);
                assert_eq!(explain.requested_mode, mxr_core::SearchMode::Lexical);
                assert_eq!(explain.executed_mode, mxr_core::SearchMode::Lexical);
                assert_eq!(explain.dense_candidates, 0);
                assert_eq!(explain.final_results as usize, results.len());
                assert_eq!(explain.results.len(), results.len());
            }
            other => panic!(
                "Expected SearchResults with explain payload, got {:?}",
                other
            ),
        }
    }

    #[tokio::test]
    async fn dispatch_structured_search_in_semantic_mode_falls_back_to_lexical() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();

        let msg = IpcMessage {
            id: 13,
            payload: IpcPayload::Request(Request::Search {
                query: "is:unread".to_string(),
                limit: 10,
                offset: 0,
                mode: Some(mxr_core::SearchMode::Semantic),
                sort: Some(mxr_core::types::SortOrder::DateDesc),
                explain: false,
            }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SearchResults { results, .. },
            }) => {
                assert!(results.len() >= 1);
                assert!(results.len() <= 10);
            }
            other => panic!("Expected SearchResults, got {:?}", other),
        }
    }

    // Requires the local semantic embedder to populate explain.semantic_query;
    // gate to the semantic-local lane so the fast lane stays green.
    #[cfg(feature = "semantic-local")]
    #[tokio::test]
    async fn dispatch_structured_search_in_semantic_mode_explains_fallback() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();

        let msg = IpcMessage {
            id: 14,
            payload: IpcPayload::Request(Request::Search {
                query: "is:unread".to_string(),
                limit: 10,
                offset: 0,
                mode: Some(mxr_core::SearchMode::Semantic),
                sort: Some(mxr_core::types::SortOrder::DateDesc),
                explain: true,
            }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data:
                    ResponseData::SearchResults {
                        explain: Some(explain),
                        ..
                    },
            }) => {
                assert_eq!(explain.requested_mode, mxr_core::SearchMode::Semantic);
                assert_eq!(explain.executed_mode, mxr_core::SearchMode::Lexical);
                assert!(explain
                    .notes
                    .iter()
                    .any(|note| note.contains("no semantic text terms")));
            }
            other => panic!(
                "Expected SearchResults with explain payload, got {:?}",
                other
            ),
        }
    }

    #[cfg(feature = "semantic-local")]
    #[tokio::test]
    async fn dispatch_fielded_semantic_query_explains_disabled_fallback() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();

        let msg = IpcMessage {
            id: 15,
            payload: IpcPayload::Request(Request::Search {
                query: "body:deployment".to_string(),
                limit: 10,
                offset: 0,
                mode: Some(mxr_core::SearchMode::Hybrid),
                sort: Some(mxr_core::types::SortOrder::DateDesc),
                explain: true,
            }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data:
                    ResponseData::SearchResults {
                        results,
                        explain: Some(explain),
                        ..
                    },
            }) => {
                assert!(!results.is_empty());
                assert_eq!(explain.requested_mode, mxr_core::SearchMode::Hybrid);
                assert_eq!(explain.executed_mode, mxr_core::SearchMode::Lexical);
                assert_eq!(explain.semantic_query.as_deref(), Some("deployment"));
                assert!(explain
                    .notes
                    .iter()
                    .any(|note| note.contains("semantic search disabled in config")));
            }
            other => panic!(
                "Expected SearchResults with explain payload, got {:?}",
                other
            ),
        }
    }

    #[tokio::test]
    async fn dispatch_search_rejects_invalid_structured_query() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let msg = IpcMessage {
            id: 12,
            payload: IpcPayload::Request(Request::Search {
                query: "older:30q".to_string(),
                limit: 10,
                offset: 0,
                mode: None,
                sort: None,
                explain: false,
            }),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Error { message, .. }) => {
                assert!(message.contains("Invalid search query"));
                assert!(message.contains("invalid date"));
            }
            other => panic!("Expected Error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_get_body_after_sync() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();

        // Get first envelope
        let envelopes_msg = IpcMessage {
            id: 11,
            payload: IpcPayload::Request(Request::ListEnvelopes {
                label_id: None,
                account_id: None,
                limit: 1,
                offset: 0,
            }),
        };
        let resp = handle_request(&state, &envelopes_msg).await;
        let message_id = match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Envelopes { envelopes },
            }) => {
                assert_eq!(envelopes.len(), 1);
                envelopes[0].id.clone()
            }
            other => panic!("Expected Envelopes, got {:?}", other),
        };

        // Get body for that envelope
        let body_msg = IpcMessage {
            id: 12,
            payload: IpcPayload::Request(Request::GetBody {
                message_id: message_id.clone(),
            }),
        };
        let resp = handle_request(&state, &body_msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Body { body },
            }) => {
                assert!(
                    body.text_plain.is_some(),
                    "Body should have text_plain content"
                );
            }
            other => panic!("Expected Body, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_list_bodies_omits_missing_rows() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let missing_id = mxr_core::MessageId::new();

        let msg = IpcMessage {
            id: 13,
            payload: IpcPayload::Request(Request::ListBodies {
                message_ids: vec![missing_id],
            }),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Bodies { bodies, .. },
            }) => {
                assert!(
                    bodies.is_empty(),
                    "missing body rows should be omitted so clients can retry"
                );
            }
            other => panic!("Expected Bodies, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_get_body_rehydrates_missing_store_row_from_provider() {
        let (state, _) = AppState::in_memory_with_fake().await.unwrap();
        let state = Arc::new(state);
        let id = sync_and_get_first_id(&state).await;

        sqlx::query("DELETE FROM bodies WHERE message_id = ?")
            .bind(id.to_string())
            .execute(state.store.writer())
            .await
            .unwrap();

        let msg = IpcMessage {
            id: 14,
            payload: IpcPayload::Request(Request::GetBody {
                message_id: id.clone(),
            }),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Body { body },
            }) => {
                assert!(
                    body.text_plain.is_some() || body.text_html.is_some(),
                    "provider hydration should restore a readable body"
                );
            }
            other => panic!("Expected Body, got {:?}", other),
        }

        let stored = state.store.get_body(&id).await.unwrap().unwrap();
        assert!(
            stored.text_plain.is_some() || stored.text_html.is_some(),
            "hydrated body should be persisted back into the store"
        );
    }

    #[tokio::test]
    async fn dispatch_list_bodies_rehydrates_missing_store_row_from_provider() {
        let (state, _) = AppState::in_memory_with_fake().await.unwrap();
        let state = Arc::new(state);
        let id = sync_and_get_first_id(&state).await;

        sqlx::query("DELETE FROM bodies WHERE message_id = ?")
            .bind(id.to_string())
            .execute(state.store.writer())
            .await
            .unwrap();

        let msg = IpcMessage {
            id: 15,
            payload: IpcPayload::Request(Request::ListBodies {
                message_ids: vec![id.clone()],
            }),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Bodies { bodies, .. },
            }) => {
                assert_eq!(bodies.len(), 1);
                assert!(
                    bodies[0].text_plain.is_some() || bodies[0].text_html.is_some(),
                    "list bodies should rehydrate readable content on cache miss"
                );
            }
            other => panic!("Expected Bodies, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_get_body_rehydrates_legacy_best_effort_body_from_provider() {
        let (state, _) = AppState::in_memory_with_fake().await.unwrap();
        let state = Arc::new(state);
        let id = sync_and_get_first_id(&state).await;

        let stale = mxr_core::types::MessageBody {
            message_id: id.clone(),
            text_plain: Some("No readable body content was available for this message.".into()),
            text_html: None,
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
            metadata: mxr_core::types::MessageMetadata::default(),
        };
        state.store.insert_body(&stale).await.unwrap();

        let msg = IpcMessage {
            id: 19,
            payload: IpcPayload::Request(Request::GetBody {
                message_id: id.clone(),
            }),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Body { body },
            }) => {
                assert_ne!(body.text_plain, stale.text_plain);
                assert!(
                    body.text_plain.is_some() || body.text_html.is_some(),
                    "legacy synthesized body should be replaced with provider content"
                );
            }
            other => panic!("Expected Body, got {:?}", other),
        }

        let stored = state.store.get_body(&id).await.unwrap().unwrap();
        assert_ne!(stored.text_plain, stale.text_plain);
        assert!(
            stored.text_plain.is_some() || stored.text_html.is_some(),
            "rehydrated body should be persisted back into the store"
        );
    }

    #[tokio::test]
    async fn dispatch_get_body_rehydrates_best_effort_summary_when_snippet_implies_real_body() {
        let (state, _) = AppState::in_memory_with_fake().await.unwrap();
        let state = Arc::new(state);
        let id = sync_and_get_first_id(&state).await;

        let stale = mxr_core::types::MessageBody {
            message_id: id.clone(),
            text_plain: Some("No readable body content was available for this message.".into()),
            text_html: None,
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
            metadata: mxr_core::types::MessageMetadata {
                text_plain_source: Some(mxr_core::types::BodyPartSource::BestEffortSummary),
                raw_headers: Some(
                    "Content-Type: multipart/alternative; boundary=\"debug-boundary\"".into(),
                ),
                ..Default::default()
            },
        };
        state.store.insert_body(&stale).await.unwrap();

        let msg = IpcMessage {
            id: 20,
            payload: IpcPayload::Request(Request::GetBody {
                message_id: id.clone(),
            }),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Body { body },
            }) => {
                assert_ne!(body.text_plain, stale.text_plain);
                assert!(
                    body.text_plain.is_some() || body.text_html.is_some(),
                    "stored best-effort summaries should be repaired when provider content exists"
                );
            }
            other => panic!("Expected Body, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_list_bodies_preserves_attachments() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;
        let attachment_id = mxr_core::AttachmentId::new();

        state
            .store
            .insert_body(&mxr_core::types::MessageBody {
                message_id: id.clone(),
                text_plain: Some("hello".into()),
                text_html: Some("<p>hello</p>".into()),
                attachments: vec![mxr_core::types::AttachmentMeta {
                    id: attachment_id.clone(),
                    message_id: id.clone(),
                    filename: "report.pdf".into(),
                    mime_type: "application/pdf".into(),
                    disposition: mxr_core::types::AttachmentDisposition::Attachment,
                    content_id: None,
                    content_location: None,
                    size_bytes: 1024,
                    local_path: None,
                    provider_id: "att-1".into(),
                }],
                fetched_at: chrono::Utc::now(),
                metadata: Default::default(),
            })
            .await
            .unwrap();

        let msg = IpcMessage {
            id: 16,
            payload: IpcPayload::Request(Request::ListBodies {
                message_ids: vec![id.clone()],
            }),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Bodies { bodies, .. },
            }) => {
                assert_eq!(bodies.len(), 1);
                assert_eq!(bodies[0].text_plain.as_deref(), Some("hello"));
                assert_eq!(bodies[0].text_html.as_deref(), Some("<p>hello</p>"));
                assert_eq!(bodies[0].attachments.len(), 1);
                assert_eq!(bodies[0].attachments[0].id, attachment_id);
                assert_eq!(bodies[0].attachments[0].filename, "report.pdf");
            }
            other => panic!("Expected Bodies, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_get_body_synthesizes_readable_summary_for_calendar_only_messages() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        let stored = mxr_core::types::MessageBody {
            message_id: id.clone(),
            text_plain: None,
            text_html: None,
            attachments: vec![mxr_core::types::AttachmentMeta {
                id: mxr_core::AttachmentId::new(),
                message_id: id.clone(),
                filename: "invite.ics".into(),
                mime_type: "text/calendar".into(),
                disposition: mxr_core::types::AttachmentDisposition::Attachment,
                content_id: None,
                content_location: None,
                size_bytes: 2048,
                local_path: None,
                provider_id: "att-calendar".into(),
            }],
            fetched_at: chrono::Utc::now(),
            metadata: mxr_core::types::MessageMetadata {
                calendar: Some(mxr_core::types::CalendarMetadata {
                    method: Some("REQUEST".into()),
                    summary: Some("Demo call".into()),
                }),
                ..Default::default()
            },
        };
        state.store.insert_body(&stored).await.unwrap();

        let msg = IpcMessage {
            id: 17,
            payload: IpcPayload::Request(Request::GetBody {
                message_id: id.clone(),
            }),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Body { body },
            }) => {
                let text = body
                    .text_plain
                    .expect("calendar-only body should be synthesized");
                assert!(text.contains("Calendar invite"));
                assert!(text.contains("Summary: Demo call"));
                assert!(text.contains("invite.ics"));
            }
            other => panic!("Expected Body, got {:?}", other),
        }

        let repaired = state.store.get_body(&id).await.unwrap().unwrap();
        assert!(repaired
            .text_plain
            .as_deref()
            .is_some_and(|text| text.contains("Calendar invite")));
    }

    #[tokio::test]
    async fn dispatch_get_body_preserves_exact_sources_and_inline_metadata() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;
        let attachment_id = mxr_core::AttachmentId::new();

        let stored = mxr_core::types::MessageBody {
            message_id: id.clone(),
            text_plain: Some("Hello team, \n> exact quote\n".into()),
            text_html: Some("<p>Hello <img src=\"cid:logo@example.com\"></p>".into()),
            attachments: vec![mxr_core::types::AttachmentMeta {
                id: attachment_id.clone(),
                message_id: id.clone(),
                filename: "logo.png".into(),
                mime_type: "image/png".into(),
                disposition: mxr_core::types::AttachmentDisposition::Inline,
                content_id: Some("logo@example.com".into()),
                content_location: Some("https://example.com/logo.png".into()),
                size_bytes: 2048,
                local_path: None,
                provider_id: "att-inline".into(),
            }],
            fetched_at: chrono::Utc::now(),
            metadata: mxr_core::types::MessageMetadata {
                text_plain_format: Some(mxr_core::types::TextPlainFormat::Flowed { delsp: true }),
                text_plain_source: Some(mxr_core::types::BodyPartSource::Exact),
                text_html_source: Some(mxr_core::types::BodyPartSource::Exact),
                ..Default::default()
            },
        };

        state.store.insert_body(&stored).await.unwrap();

        let msg = IpcMessage {
            id: 18,
            payload: IpcPayload::Request(Request::GetBody {
                message_id: id.clone(),
            }),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Body { body },
            }) => {
                assert_eq!(body.text_plain, stored.text_plain);
                assert_eq!(body.text_html, stored.text_html);
                assert_eq!(
                    body.metadata.text_plain_format,
                    stored.metadata.text_plain_format
                );
                assert_eq!(
                    body.metadata.text_plain_source,
                    stored.metadata.text_plain_source
                );
                assert_eq!(
                    body.metadata.text_html_source,
                    stored.metadata.text_html_source
                );
                assert_eq!(body.attachments.len(), 1);
                assert_eq!(body.attachments[0].id, attachment_id);
                assert_eq!(
                    body.attachments[0].content_id.as_deref(),
                    Some("logo@example.com")
                );
                assert_eq!(
                    body.attachments[0].content_location.as_deref(),
                    Some("https://example.com/logo.png")
                );
            }
            other => panic!("Expected Body, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_get_html_image_assets_resolves_inline_and_blocks_remote() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;
        let attachment_id = mxr_core::AttachmentId::new();
        let temp_dir = tempfile::tempdir().unwrap();
        let inline_path = temp_dir.path().join("logo.png");
        std::fs::write(&inline_path, tiny_png_bytes()).unwrap();

        let stored = mxr_core::types::MessageBody {
            message_id: id.clone(),
            text_plain: None,
            text_html: Some(concat!(
                "<img alt=\"Logo\" src=\"cid:logo@example.com\">",
                "<img alt=\"Badge\" src=\"data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO9xw1QAAAAASUVORK5CYII=\">",
                "<img alt=\"Hero\" src=\"https://example.com/hero.png\">"
            ).into()),
            attachments: vec![mxr_core::types::AttachmentMeta {
                id: attachment_id.clone(),
                message_id: id.clone(),
                filename: "logo.png".into(),
                mime_type: "image/png".into(),
                disposition: mxr_core::types::AttachmentDisposition::Inline,
                content_id: Some("logo@example.com".into()),
                content_location: None,
                size_bytes: 67,
                local_path: Some(inline_path.clone()),
                provider_id: "att-inline".into(),
            }],
            fetched_at: chrono::Utc::now(),
            metadata: mxr_core::types::MessageMetadata {
                text_html_source: Some(mxr_core::types::BodyPartSource::Exact),
                ..Default::default()
            },
        };
        state.store.insert_body(&stored).await.unwrap();

        let msg = IpcMessage {
            id: 16,
            payload: IpcPayload::Request(Request::GetHtmlImageAssets {
                message_id: id.clone(),
                allow_remote: false,
            }),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::HtmlImageAssets { assets, .. },
            }) => {
                assert_eq!(assets.len(), 3);

                let inline = assets
                    .iter()
                    .find(|asset| asset.source.starts_with("cid:"))
                    .expect("cid asset");
                assert_eq!(inline.status, mxr_core::types::HtmlImageAssetStatus::Ready);
                assert_eq!(inline.path.as_deref(), Some(inline_path.as_path()));

                let embedded = assets
                    .iter()
                    .find(|asset| asset.source.starts_with("data:"))
                    .expect("data asset");
                assert_eq!(
                    embedded.status,
                    mxr_core::types::HtmlImageAssetStatus::Ready,
                    "embedded asset: {:?}",
                    embedded
                );
                assert!(embedded.path.as_ref().is_some_and(|path| path.exists()));

                let remote = assets
                    .iter()
                    .find(|asset| asset.source.starts_with("https://"))
                    .expect("remote asset");
                assert_eq!(
                    remote.status,
                    mxr_core::types::HtmlImageAssetStatus::Blocked
                );
                assert!(remote.path.is_none());
            }
            other => panic!("Expected HtmlImageAssets, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_get_html_image_assets_fetches_remote_when_enabled() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("GET"))
            .respond_with(
                wiremock::ResponseTemplate::new(200)
                    .insert_header("content-type", "image/png")
                    .set_body_bytes(tiny_png_bytes()),
            )
            .mount(&server)
            .await;

        let stored = mxr_core::types::MessageBody {
            message_id: id.clone(),
            text_plain: None,
            text_html: Some(format!(
                r#"<img alt="Hero" src="{}/hero.png">"#,
                server.uri()
            )),
            attachments: vec![],
            fetched_at: chrono::Utc::now(),
            metadata: mxr_core::types::MessageMetadata {
                text_html_source: Some(mxr_core::types::BodyPartSource::Exact),
                ..Default::default()
            },
        };
        state.store.insert_body(&stored).await.unwrap();

        let msg = IpcMessage {
            id: 17,
            payload: IpcPayload::Request(Request::GetHtmlImageAssets {
                message_id: id.clone(),
                allow_remote: true,
            }),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::HtmlImageAssets { assets, .. },
            }) => {
                assert_eq!(assets.len(), 1);
                assert_eq!(
                    assets[0].status,
                    mxr_core::types::HtmlImageAssetStatus::Ready
                );
                let path = assets[0].path.as_ref().expect("cached path");
                assert!(path.exists());
                assert_eq!(std::fs::read(path).unwrap(), tiny_png_bytes());
            }
            other => panic!("Expected HtmlImageAssets, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_download_attachment_persists_local_path() {
        let state = AppState::in_memory().await.unwrap();
        state.set_attachment_dir_for_tests(
            std::env::temp_dir().join(format!("mxr-attachments-test-{}", uuid::Uuid::new_v4())),
        );
        let state = Arc::new(state);

        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();

        let list_msg = IpcMessage {
            id: 14,
            payload: IpcPayload::Request(Request::ListEnvelopes {
                label_id: None,
                account_id: None,
                limit: 200,
                offset: 0,
            }),
        };
        let resp = handle_request(&state, &list_msg).await;
        let envelope = match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Envelopes { envelopes },
            }) => envelopes
                .into_iter()
                .find(|envelope| envelope.has_attachments)
                .expect("fixture should include an attachment"),
            other => panic!("Expected Envelopes, got {:?}", other),
        };

        let body_msg = IpcMessage {
            id: 15,
            payload: IpcPayload::Request(Request::GetBody {
                message_id: envelope.id.clone(),
            }),
        };
        let resp = handle_request(&state, &body_msg).await;
        let attachment_id = match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Body { body },
            }) => body.attachments[0].id.clone(),
            other => panic!("Expected Body, got {:?}", other),
        };

        let download_msg = IpcMessage {
            id: 16,
            payload: IpcPayload::Request(Request::DownloadAttachment {
                message_id: envelope.id.clone(),
                attachment_id: attachment_id.clone(),
            }),
        };
        let resp = handle_request(&state, &download_msg).await;
        let path = match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::AttachmentFile { file },
            }) => std::path::PathBuf::from(file.path),
            other => panic!("Expected AttachmentFile, got {:?}", other),
        };

        assert!(path.exists(), "downloaded attachment should exist on disk");

        let body = state
            .store
            .get_body(&envelope.id)
            .await
            .unwrap()
            .expect("body should remain cached");
        let attachment = body
            .attachments
            .iter()
            .find(|attachment| attachment.id == attachment_id)
            .expect("attachment should still exist");
        assert_eq!(attachment.local_path.as_ref(), Some(&path));

        let _ = std::fs::remove_dir_all(state.attachment_dir());
    }

    #[tokio::test]
    async fn dispatch_set_reply_later_persists_flag_visible_in_queue() {
        // Behavior: marking a message reply-later via IPC persists the flag,
        // and subsequent `ListReplyQueue` requests return the envelope.
        // Clearing the flag removes it from the queue.
        let (state, _) = AppState::in_memory_with_fake().await.unwrap();
        let state = Arc::new(state);
        let id = sync_and_get_first_id(&state).await;

        // Initially the queue is empty.
        let resp = handle_request(
            &state,
            &IpcMessage {
                id: 200,
                payload: IpcPayload::Request(Request::ListReplyQueue),
            },
        )
        .await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::ReplyQueue { messages },
            }) => assert!(messages.is_empty(), "fresh queue is empty"),
            other => panic!("expected ReplyQueue, got {other:?}"),
        }

        // Set the flag.
        let resp = handle_request(
            &state,
            &IpcMessage {
                id: 201,
                payload: IpcPayload::Request(Request::SetReplyLater {
                    message_id: id.clone(),
                    flag: true,
                }),
            },
        )
        .await;
        assert!(matches!(
            resp.payload,
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack
            })
        ));

        // Queue now contains the flagged envelope.
        let resp = handle_request(
            &state,
            &IpcMessage {
                id: 202,
                payload: IpcPayload::Request(Request::ListReplyQueue),
            },
        )
        .await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::ReplyQueue { messages },
            }) => {
                assert_eq!(messages.len(), 1, "one flagged message");
                assert_eq!(messages[0].id, id);
            }
            other => panic!("expected ReplyQueue, got {other:?}"),
        }
        let ast = mxr_search::parse_query("is:reply-later").unwrap();
        let schema = mxr_search::MxrSchema::build();
        let query = mxr_search::QueryBuilder::new(&schema).build(&ast);
        let search_page = state
            .search
            .search_ast(query, 10, 0, mxr_core::types::SortOrder::DateDesc)
            .await
            .unwrap();
        assert_eq!(search_page.results.len(), 1, "search sees reply-later");
        assert_eq!(search_page.results[0].message_id, id.as_str());

        // Clear the flag.
        let resp = handle_request(
            &state,
            &IpcMessage {
                id: 203,
                payload: IpcPayload::Request(Request::SetReplyLater {
                    message_id: id.clone(),
                    flag: false,
                }),
            },
        )
        .await;
        assert!(matches!(
            resp.payload,
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack
            })
        ));

        // Queue is empty again.
        let resp = handle_request(
            &state,
            &IpcMessage {
                id: 204,
                payload: IpcPayload::Request(Request::ListReplyQueue),
            },
        )
        .await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::ReplyQueue { messages },
            }) => assert!(messages.is_empty(), "queue empty after clear"),
            other => panic!("expected ReplyQueue, got {other:?}"),
        }
        let ast = mxr_search::parse_query("is:reply-later").unwrap();
        let schema = mxr_search::MxrSchema::build();
        let query = mxr_search::QueryBuilder::new(&schema).build(&ast);
        let search_page = state
            .search
            .search_ast(query, 10, 0, mxr_core::types::SortOrder::DateDesc)
            .await
            .unwrap();
        assert!(search_page.results.is_empty(), "search updates after clear");
    }

    #[tokio::test]
    async fn dispatch_set_auto_reminder_persists_and_loop_fires_when_due() {
        // End-to-end: setting a reminder via IPC persists it; the
        // background-loop function fires it once `now >= remind_at` and
        // emits a `ReminderTriggered` event.
        let (state, _) = AppState::in_memory_with_fake().await.unwrap();
        let state = Arc::new(state);
        let id = sync_and_get_first_id(&state).await;
        let mut events = state.event_tx.subscribe();

        // Set the reminder for "1 hour ago" so it's already due.
        let remind_at = chrono::Utc::now() - chrono::Duration::hours(1);
        let resp = handle_request(
            &state,
            &IpcMessage {
                id: 300,
                payload: IpcPayload::Request(Request::SetAutoReminder {
                    sent_message_id: id.clone(),
                    remind_at,
                }),
            },
        )
        .await;
        assert!(matches!(
            resp.payload,
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack
            })
        ));

        // Run one tick of the loop with `now` past the reminder.
        let fired = crate::loops::process_due_reminders(&state, chrono::Utc::now())
            .await
            .unwrap();
        assert_eq!(fired, 1, "one due reminder fires");

        // Expect a ReminderTriggered event for the right message.
        let received = events.try_recv().expect("event published");
        match received.payload {
            IpcPayload::Event(DaemonEvent::ReminderTriggered { sent_message_id }) => {
                assert_eq!(sent_message_id, id);
            }
            other => panic!("expected ReminderTriggered event, got {other:?}"),
        }

        // Second tick: nothing fires (already-triggered reminders are
        // excluded).
        let fired_again = crate::loops::process_due_reminders(&state, chrono::Utc::now())
            .await
            .unwrap();
        assert_eq!(fired_again, 0, "fired reminders are not re-fired");
    }

    #[tokio::test]
    async fn dispatch_cancel_auto_reminder_prevents_firing() {
        // Setting then cancelling a reminder leaves no due rows for
        // the loop to fire.
        let (state, _) = AppState::in_memory_with_fake().await.unwrap();
        let state = Arc::new(state);
        let id = sync_and_get_first_id(&state).await;

        let remind_at = chrono::Utc::now() - chrono::Duration::hours(1);
        let _ = handle_request(
            &state,
            &IpcMessage {
                id: 310,
                payload: IpcPayload::Request(Request::SetAutoReminder {
                    sent_message_id: id.clone(),
                    remind_at,
                }),
            },
        )
        .await;
        let resp = handle_request(
            &state,
            &IpcMessage {
                id: 311,
                payload: IpcPayload::Request(Request::CancelAutoReminder {
                    sent_message_id: id.clone(),
                }),
            },
        )
        .await;
        assert!(matches!(
            resp.payload,
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack
            })
        ));

        let fired = crate::loops::process_due_reminders(&state, chrono::Utc::now())
            .await
            .unwrap();
        assert_eq!(fired, 0, "cancelled reminders never fire");
    }

    #[tokio::test]
    async fn dispatch_schedule_send_persists_and_loop_flushes_when_due() {
        // End-to-end: schedule an existing draft for a past send_at,
        // run one tick of the loop, expect the send pipeline to fire
        // and the draft's status to advance past 'draft'.
        let (state, _) = AppState::in_memory_with_fake().await.unwrap();
        let state = Arc::new(state);
        let _ = sync_and_get_first_id(&state).await;

        // Insert a draft for the synthetic account.
        let account = state
            .store
            .list_accounts()
            .await
            .unwrap()
            .first()
            .unwrap()
            .clone();
        let draft = mxr_core::types::Draft {
            id: mxr_core::id::DraftId::new(),
            account_id: account.id.clone(),
            reply_headers: None,
            intent: mxr_core::DraftIntent::New,
            to: vec![mxr_core::types::Address {
                name: None,
                email: "you@example.com".into(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "scheduled".into(),
            body_markdown: "Body".into(),
            attachments: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        state.store.insert_draft(&draft).await.unwrap();

        // Schedule for "1 hour ago" — already due.
        let resp = handle_request(
            &state,
            &IpcMessage {
                id: 400,
                payload: IpcPayload::Request(Request::ScheduleSend {
                    draft_id: draft.id.clone(),
                    send_at: chrono::Utc::now() - chrono::Duration::hours(1),
                }),
            },
        )
        .await;
        assert!(matches!(
            resp.payload,
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack
            })
        ));
        assert_eq!(
            state
                .store
                .get_scheduled_send(&draft.id)
                .await
                .unwrap()
                .is_some(),
            true,
            "send_at persisted"
        );

        // Run a tick of the flusher.
        let fired = crate::loops::process_due_scheduled_sends(&state, chrono::Utc::now())
            .await
            .unwrap();
        assert_eq!(fired, 1);

        // Draft no longer needs sending: either advanced past `draft`
        // status (FakeProvider may delete on success) or is gone entirely.
        let status = state.store.get_draft_status(&draft.id).await.unwrap();
        assert!(
            !matches!(status, Some(mxr_core::types::DraftStatus::Draft)),
            "draft no longer in 'draft' status: {status:?}"
        );

        // The schedule entry is cleared (the row may be gone too) so a
        // second tick won't try to re-flush it.
        assert!(state
            .store
            .get_scheduled_send(&draft.id)
            .await
            .unwrap()
            .is_none());
        let fired_again = crate::loops::process_due_scheduled_sends(&state, chrono::Utc::now())
            .await
            .unwrap();
        assert_eq!(fired_again, 0);
    }

    #[tokio::test]
    async fn dispatch_cancel_scheduled_send_prevents_flush() {
        let (state, _) = AppState::in_memory_with_fake().await.unwrap();
        let state = Arc::new(state);
        let _ = sync_and_get_first_id(&state).await;

        let account = state
            .store
            .list_accounts()
            .await
            .unwrap()
            .first()
            .unwrap()
            .clone();
        let draft = mxr_core::types::Draft {
            id: mxr_core::id::DraftId::new(),
            account_id: account.id.clone(),
            reply_headers: None,
            intent: mxr_core::DraftIntent::New,
            to: vec![mxr_core::types::Address {
                name: None,
                email: "you@example.com".into(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "scheduled-then-cancelled".into(),
            body_markdown: "Body".into(),
            attachments: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        state.store.insert_draft(&draft).await.unwrap();

        let _ = handle_request(
            &state,
            &IpcMessage {
                id: 410,
                payload: IpcPayload::Request(Request::ScheduleSend {
                    draft_id: draft.id.clone(),
                    send_at: chrono::Utc::now() - chrono::Duration::hours(1),
                }),
            },
        )
        .await;
        let resp = handle_request(
            &state,
            &IpcMessage {
                id: 411,
                payload: IpcPayload::Request(Request::CancelScheduledSend {
                    draft_id: draft.id.clone(),
                }),
            },
        )
        .await;
        assert!(matches!(
            resp.payload,
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack
            })
        ));

        let fired = crate::loops::process_due_scheduled_sends(&state, chrono::Utc::now())
            .await
            .unwrap();
        assert_eq!(fired, 0);

        // Draft remains in 'draft' status — never sent.
        assert_eq!(
            state.store.get_draft_status(&draft.id).await.unwrap(),
            Some(mxr_core::types::DraftStatus::Draft)
        );
    }

    /// Helper: sync, list envelopes, return first envelope's id.
    async fn sync_and_get_first_id(state: &Arc<AppState>) -> mxr_core::MessageId {
        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();

        let msg = IpcMessage {
            id: 100,
            payload: IpcPayload::Request(Request::ListEnvelopes {
                label_id: None,
                account_id: None,
                limit: 1,
                offset: 0,
            }),
        };
        let resp = handle_request(state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Envelopes { envelopes },
            }) => {
                assert_eq!(envelopes.len(), 1);
                envelopes[0].id.clone()
            }
            other => panic!("Expected Envelopes, got {:?}", other),
        }
    }

    fn assert_mutation_succeeded(payload: IpcPayload) -> MutationResultData {
        match payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::MutationResult { result },
            }) => {
                assert!(
                    result.succeeded > 0,
                    "expected mutation success: {result:?}"
                );
                result
            }
            other => panic!("Expected MutationResult success, got {:?}", other),
        }
    }

    async fn add_failing_sync_account(
        state: &AppState,
        calls: Arc<AtomicUsize>,
    ) -> (mxr_core::AccountId, mxr_core::MessageId) {
        let account_id = mxr_core::AccountId::from_provider_id("imap", "hello@bhekani.com");
        let account = mxr_core::Account {
            id: account_id.clone(),
            name: "consulting".to_string(),
            email: "hello@bhekani.com".to_string(),
            sync_backend: Some(mxr_core::BackendRef {
                provider_kind: mxr_core::ProviderKind::Imap,
                config_key: "consulting".to_string(),
            }),
            send_backend: None,
            enabled: true,
        };
        state.store.insert_account(&account).await.unwrap();
        state.add_sync_provider_for_test(Arc::new(FailingSyncProvider {
            account_id: account_id.clone(),
            message: "Keyring error: Failed to read password from keychain",
            calls,
        }));

        let envelope = crate::test_fixtures::TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .provider_id("bad-provider-id")
            .subject("bad account message")
            .build();
        let message_id = envelope.id.clone();
        state.store.upsert_envelope(&envelope).await.unwrap();
        (account_id, message_id)
    }

    fn tiny_png_bytes() -> Vec<u8> {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO9xw1QAAAAASUVORK5CYII=")
            .expect("valid 1x1 png")
    }

    #[tokio::test]
    async fn dispatch_mutation_star() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::mutation(MutationCommand::Star {
                message_ids: vec![id.clone()],
                starred: true,
            })),
        };
        let resp = handle_request(&state, &msg).await;
        assert_mutation_succeeded(resp.payload);

        // Verify flag is set
        let get_msg = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::GetEnvelope { message_id: id }),
        };
        let resp = handle_request(&state, &get_msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Envelope { envelope },
            }) => {
                assert!(
                    envelope
                        .flags
                        .contains(mxr_core::types::MessageFlags::STARRED),
                    "Expected STARRED flag to be set, got {:?}",
                    envelope.flags
                );
            }
            other => panic!("Expected Envelope, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn modify_labels_on_folder_provider_does_not_leave_one_message_in_two_folders() {
        let state = folder_copy_state().await;
        let id = sync_and_get_first_id(&state).await;

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::mutation(MutationCommand::ModifyLabels {
                message_ids: vec![id],
                add: vec!["Archive".to_string()],
                remove: vec![],
            })),
        };
        let resp = handle_request(&state, &msg).await;
        assert_mutation_succeeded(resp.payload);

        let envelopes = state
            .store
            .list_envelopes_by_account(&state.default_account_id(), 20, 0)
            .await
            .unwrap();
        assert_eq!(
            envelopes.len(),
            2,
            "expected exactly one inbox copy and one archive copy after folder add: {envelopes:?}"
        );
        assert!(
            !envelopes.iter().any(|envelope| {
                envelope
                    .label_provider_ids
                    .iter()
                    .any(|provider_id| provider_id == "INBOX")
                    && envelope
                        .label_provider_ids
                        .iter()
                        .any(|provider_id| provider_id == "Archive")
            }),
            "folder-based providers should not be flattened into one message with two folders: {envelopes:?}"
        );
        assert!(
            envelopes
                .iter()
                .any(|envelope| envelope.label_provider_ids == vec!["INBOX".to_string()]),
            "expected inbox copy after folder add"
        );
        assert!(
            envelopes
                .iter()
                .any(|envelope| envelope.label_provider_ids == vec!["Archive".to_string()]),
            "expected archive copy after folder add"
        );
    }

    #[tokio::test]
    async fn snooze_on_folder_provider_reanchors_to_reconciled_message_copy() {
        let state = folder_copy_state().await;
        let original_id = sync_and_get_first_id(&state).await;

        let snooze = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::Snooze {
                message_id: original_id.clone(),
                wake_at: chrono::Utc::now() + chrono::Duration::hours(4),
            }),
        };
        match handle_request(&state, &snooze).await.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack for Snooze, got {:?}", other),
        }

        let snoozed = state.store.list_snoozed().await.unwrap();
        assert_eq!(snoozed.len(), 1, "expected one snoozed message");
        assert_ne!(
            snoozed[0].message_id, original_id,
            "folder-backed snooze should track the reconciled message copy"
        );

        let archived = state
            .store
            .list_envelopes_by_account(&state.default_account_id(), 20, 0)
            .await
            .unwrap();
        assert_eq!(
            archived.len(),
            1,
            "expected exactly one archived copy after snooze: {archived:?}"
        );
        assert!(
            archived
                .iter()
                .all(|envelope| envelope.label_provider_ids == vec!["Archive".to_string()]),
            "expected only archived copy after snooze: {archived:?}"
        );

        let unsnooze = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::Unsnooze {
                message_id: snoozed[0].message_id.clone(),
            }),
        };
        match handle_request(&state, &unsnooze).await.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack for Unsnooze, got {:?}", other),
        }

        let inbox = state
            .store
            .list_envelopes_by_account(&state.default_account_id(), 20, 0)
            .await
            .unwrap();
        assert_eq!(
            inbox.len(),
            1,
            "expected exactly one inbox copy after unsnooze: {inbox:?}"
        );
        assert!(
            inbox
                .iter()
                .all(|envelope| envelope.label_provider_ids == vec!["INBOX".to_string()]),
            "expected only inbox copy after unsnooze: {inbox:?}"
        );
        assert!(
            state.store.list_snoozed().await.unwrap().is_empty(),
            "expected snooze row to be cleared after unsnooze"
        );
    }

    #[tokio::test]
    async fn snooze_on_folder_provider_errors_when_reconciled_copy_is_missing() {
        let state = folder_copy_state_with_mode(FolderCopyReanchorMode::MissingAfterArchive).await;
        let original_id = sync_and_get_first_id(&state).await;

        let snooze = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::Snooze {
                message_id: original_id,
                wake_at: chrono::Utc::now() + chrono::Duration::hours(4),
            }),
        };
        match handle_request(&state, &snooze).await.payload {
            IpcPayload::Response(Response::Error { message, .. }) => {
                assert!(
                    message.contains("Reconciled message not found"),
                    "expected missing reanchor error, got: {message}"
                );
            }
            other => panic!(
                "Expected Error for missing reconciled snooze copy, got {:?}",
                other
            ),
        }

        assert!(
            state.store.list_snoozed().await.unwrap().is_empty(),
            "expected no snooze row after failed reanchor"
        );
        assert!(
            state
                .store
                .list_envelopes_by_account(&state.default_account_id(), 20, 0)
                .await
                .unwrap()
                .is_empty(),
            "expected provider sync to reflect the missing reconciled copy"
        );
    }

    #[tokio::test]
    async fn dispatch_mutation_set_read() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::mutation(MutationCommand::SetRead {
                message_ids: vec![id.clone()],
                read: true,
            })),
        };
        let resp = handle_request(&state, &msg).await;
        assert_mutation_succeeded(resp.payload);

        let get_msg = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::GetEnvelope { message_id: id }),
        };
        let resp = handle_request(&state, &get_msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Envelope { envelope },
            }) => {
                assert!(
                    envelope.flags.contains(mxr_core::types::MessageFlags::READ),
                    "Expected READ flag to be set, got {:?}",
                    envelope.flags
                );
            }
            other => panic!("Expected Envelope, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_mutation_archive() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::mutation(MutationCommand::Archive {
                message_ids: vec![id.clone()],
            })),
        };
        let resp = handle_request(&state, &msg).await;
        assert_mutation_succeeded(resp.payload);

        let events = state
            .store
            .list_events(10, None, Some("mutation"))
            .await
            .unwrap();
        let id_str = id.as_str();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].message_id.as_deref(), Some(id_str.as_str()));
        assert!(events[0].summary.contains("Archived"));
    }

    /// Phase 1.4 / Behaviors 1+2+3+8: archive a message, observe the new
    /// `mutation_id` in the response, undo it within the window, and
    /// verify the message is back under the INBOX label both locally and
    /// on the (fake) provider. Proves the snapshot capture, write,
    /// reverse-op dispatch, and local restoration all line up.
    #[tokio::test]
    async fn undo_archive_restores_inbox_label() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        // Pre-condition: the message has the INBOX label.
        let pre = state.store.get_envelope(&id).await.unwrap().unwrap();
        assert!(
            pre.label_provider_ids.iter().any(|l| l == "INBOX"),
            "fixture must start in INBOX; got {:?}",
            pre.label_provider_ids
        );

        // Archive — captures snapshot, writes undo entry, returns mutation_id.
        let archive = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::mutation(MutationCommand::Archive {
                message_ids: vec![id.clone()],
            })),
        };
        let result = assert_mutation_succeeded(handle_request(&state, &archive).await.payload);
        let mutation_id = result
            .mutation_id
            .clone()
            .expect("Archive must return a mutation_id");

        let post_archive = state.store.get_envelope(&id).await.unwrap().unwrap();
        assert!(
            !post_archive.label_provider_ids.iter().any(|l| l == "INBOX"),
            "INBOX must be removed by Archive; got {:?}",
            post_archive.label_provider_ids
        );

        // Undo — restores INBOX both locally and via the fake provider.
        let undo = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::UndoMutation {
                mutation_id: mutation_id.clone(),
            }),
        };
        let resp = handle_request(&state, &undo).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("expected Ack from UndoMutation; got {other:?}"),
        }

        let restored = state.store.get_envelope(&id).await.unwrap().unwrap();
        assert!(
            restored.label_provider_ids.iter().any(|l| l == "INBOX"),
            "INBOX must be restored after Undo; got {:?}",
            restored.label_provider_ids
        );

        // The undo entry is consumed — replaying the same id is now a no-op
        // (regression test for "user mashes u and double-undoes").
        let replay = IpcMessage {
            id: 3,
            payload: IpcPayload::Request(Request::UndoMutation { mutation_id }),
        };
        match handle_request(&state, &replay).await.payload {
            IpcPayload::Response(Response::Error { message, .. }) => {
                assert!(
                    message.to_lowercase().contains("not found"),
                    "second undo must return not-found; got {message}"
                );
            }
            other => panic!("expected Error on replay; got {other:?}"),
        }
    }

    /// Phase 1.4 / Behavior 4: Undo for an unknown id returns Error
    /// with "not found" so the TUI can render the right message instead
    /// of silently succeeding or panicking.
    #[tokio::test]
    async fn undo_unknown_mutation_id_returns_not_found() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::UndoMutation {
                mutation_id: "01HVTOTALLYBOGUSID0000000".into(),
            }),
        };
        match handle_request(&state, &msg).await.payload {
            IpcPayload::Response(Response::Error { message, .. }) => {
                assert!(
                    message.to_lowercase().contains("not found"),
                    "expected not-found error; got {message}"
                );
            }
            other => panic!("expected Error; got {other:?}"),
        }
    }

    /// Phase 1.4 / Behavior 6: a bulk Archive of multiple messages
    /// produces a single mutation_id and a single Undo restores all of
    /// them. Catches regressions where snapshots are dropped or only the
    /// first envelope is restored.
    #[tokio::test]
    async fn undo_bulk_archive_restores_all_messages() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        // Sync first to populate the fixture.
        let _ = sync_and_get_first_id(&state).await;

        // Pull three INBOX-tagged messages by listing envelopes.
        let list_msg = IpcMessage {
            id: 100,
            payload: IpcPayload::Request(Request::ListEnvelopes {
                label_id: None,
                account_id: None,
                limit: 3,
                offset: 0,
            }),
        };
        let envelopes = match handle_request(&state, &list_msg).await.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Envelopes { envelopes },
            }) => envelopes,
            other => panic!("expected Envelopes; got {other:?}"),
        };
        let ids: Vec<mxr_core::MessageId> =
            envelopes.iter().take(3).map(|e| e.id.clone()).collect();
        assert!(ids.len() >= 2, "fixture must contain >=2 messages");

        let archive = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::mutation(MutationCommand::Archive {
                message_ids: ids.clone(),
            })),
        };
        let result = assert_mutation_succeeded(handle_request(&state, &archive).await.payload);
        let mutation_id = result.mutation_id.clone().expect("mutation_id required");
        assert_eq!(result.succeeded, ids.len() as u32);

        let undo = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::UndoMutation { mutation_id }),
        };
        match handle_request(&state, &undo).await.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("expected Ack; got {other:?}"),
        }

        // Every archived message should now have INBOX again.
        for id in &ids {
            let env = state.store.get_envelope(id).await.unwrap().unwrap();
            assert!(
                env.label_provider_ids.iter().any(|l| l == "INBOX"),
                "{id} must have INBOX restored; got {:?}",
                env.label_provider_ids
            );
        }
    }

    /// Phase 1.4: Star is not undoable — the response carries no
    /// mutation_id so clients know not to render the undo affordance.
    #[tokio::test]
    async fn star_mutation_omits_mutation_id() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;
        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::mutation(MutationCommand::Star {
                message_ids: vec![id],
                starred: true,
            })),
        };
        let result = assert_mutation_succeeded(handle_request(&state, &msg).await.payload);
        assert!(
            result.mutation_id.is_none(),
            "Star must not return a mutation_id; got {:?}",
            result.mutation_id
        );
    }

    #[tokio::test]
    async fn mutation_archives_healthy_account_when_other_account_provider_fails() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let healthy_id = sync_and_get_first_id(&state).await;
        let failing_calls = Arc::new(AtomicUsize::new(0));
        add_failing_sync_account(&state, failing_calls.clone()).await;

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::mutation(MutationCommand::Archive {
                message_ids: vec![healthy_id],
            })),
        };
        let result = assert_mutation_succeeded(handle_request(&state, &msg).await.payload);

        assert_eq!(result.requested, 1);
        assert_eq!(result.succeeded, 1);
        assert_eq!(result.skipped, 0);
        assert_eq!(failing_calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn mixed_account_mutation_returns_partial_success() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let healthy_id = sync_and_get_first_id(&state).await;
        let failing_calls = Arc::new(AtomicUsize::new(0));
        let (bad_account_id, bad_id) =
            add_failing_sync_account(&state, failing_calls.clone()).await;

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::mutation(MutationCommand::Archive {
                message_ids: vec![healthy_id, bad_id],
            })),
        };
        let result = assert_mutation_succeeded(handle_request(&state, &msg).await.payload);

        assert_eq!(result.requested, 2);
        assert_eq!(result.succeeded, 1);
        assert_eq!(result.skipped, 1);
        assert_eq!(failing_calls.load(Ordering::SeqCst), 1);
        let bad_account = result
            .accounts
            .iter()
            .find(|account| account.account_id == bad_account_id)
            .expect("bad account result");
        assert_eq!(bad_account.succeeded, 0);
        assert_eq!(bad_account.skipped, 1);
        assert!(bad_account
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("keychain"));
    }

    #[tokio::test]
    async fn dispatch_mutation_read_and_archive() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::mutation(MutationCommand::ReadAndArchive {
                message_ids: vec![id.clone()],
            })),
        };
        let resp = handle_request(&state, &msg).await;
        assert_mutation_succeeded(resp.payload);

        let envelope = state
            .store
            .get_envelope(&id)
            .await
            .unwrap()
            .expect("message should still exist");
        assert!(envelope.flags.contains(mxr_core::types::MessageFlags::READ));

        let label_ids = state.store.get_message_label_ids(&id).await.unwrap();
        assert!(!label_ids
            .iter()
            .any(|label_id| label_id.as_str() == "INBOX"));

        let events = state
            .store
            .list_events(10, None, Some("mutation"))
            .await
            .unwrap();
        assert!(events[0].summary.contains("read and archived"));
    }

    #[tokio::test]
    async fn dispatch_mutation_trash() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::mutation(MutationCommand::Trash {
                message_ids: vec![id],
            })),
        };
        let resp = handle_request(&state, &msg).await;
        assert_mutation_succeeded(resp.payload);
    }

    #[tokio::test]
    async fn dispatch_prepare_reply() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;
        let expected_subject = state
            .store
            .get_envelope(&id)
            .await
            .unwrap()
            .unwrap()
            .subject;

        // Fetch body first so it's cached
        let body_msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::GetBody {
                message_id: id.clone(),
            }),
        };
        handle_request(&state, &body_msg).await;

        let msg = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::PrepareReply {
                message_id: id,
                reply_all: false,
            }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::ReplyContext { context },
            }) => {
                assert!(context.reply_to.contains('@'));
                assert_eq!(context.subject, expected_subject);
            }
            other => panic!("Expected ReplyContext, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_prepare_reply_all() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;
        let expected_subject = state
            .store
            .get_envelope(&id)
            .await
            .unwrap()
            .unwrap()
            .subject;

        // Fetch body first
        let body_msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::GetBody {
                message_id: id.clone(),
            }),
        };
        handle_request(&state, &body_msg).await;

        let msg = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::PrepareReply {
                message_id: id,
                reply_all: true,
            }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::ReplyContext { context },
            }) => {
                assert!(context.reply_to.contains('@'));
                assert_eq!(context.subject, expected_subject);
                // cc may or may not be empty depending on the message, but the field should exist
            }
            other => panic!("Expected ReplyContext, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_prepare_reply_renders_html_context() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        state
            .store
            .insert_body(&mxr_core::types::MessageBody {
                message_id: id.clone(),
                text_plain: None,
                text_html: Some("<p>Hello <b>world</b></p>".into()),
                attachments: vec![],
                fetched_at: chrono::Utc::now(),
                metadata: Default::default(),
            })
            .await
            .unwrap();

        let msg = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::PrepareReply {
                message_id: id,
                reply_all: false,
            }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::ReplyContext { context },
            }) => {
                assert!(context.thread_context.contains("Hello world"));
                assert!(!context.thread_context.contains("<p>"));
            }
            other => panic!("Expected ReplyContext, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_prepare_forward() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;
        let expected_subject = state
            .store
            .get_envelope(&id)
            .await
            .unwrap()
            .unwrap()
            .subject;

        // Fetch body first
        let body_msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::GetBody {
                message_id: id.clone(),
            }),
        };
        handle_request(&state, &body_msg).await;

        let msg = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::PrepareForward { message_id: id }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::ForwardContext { context },
            }) => {
                assert_eq!(context.subject, expected_subject);
                assert!(
                    !context.forwarded_content.is_empty(),
                    "forwarded_content should be non-empty"
                );
            }
            other => panic!("Expected ForwardContext, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn modify_labels_persists_to_store_immediately() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        let create = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::CreateLabel {
                name: "Follow Up".into(),
                color: None,
                account_id: None,
            }),
        };
        let label = match handle_request(&state, &create).await.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Label { label },
            }) => label,
            other => panic!("Expected Label response, got {:?}", other),
        };

        let modify = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::mutation(MutationCommand::ModifyLabels {
                message_ids: vec![id.clone()],
                add: vec![label.name.clone()],
                remove: vec![],
            })),
        };
        assert_mutation_succeeded(handle_request(&state, &modify).await.payload);

        let label_ids = state.store.get_message_label_ids(&id).await.unwrap();
        assert!(label_ids.iter().any(|label_id| label_id == &label.id));
    }

    #[tokio::test]
    async fn get_thread_includes_message_label_provider_ids() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;
        let envelope = state.store.get_envelope(&id).await.unwrap().unwrap();

        let create = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::CreateLabel {
                name: "Recruiters".into(),
                color: None,
                account_id: None,
            }),
        };
        let label = match handle_request(&state, &create).await.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Label { label },
            }) => label,
            other => panic!("Expected Label response, got {:?}", other),
        };

        state
            .store
            .add_message_label(&id, &label.id, mxr_core::EventSource::User)
            .await
            .unwrap();

        let msg = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::GetThread {
                thread_id: envelope.thread_id,
            }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Thread { messages, .. },
            }) => {
                let message = messages
                    .into_iter()
                    .find(|message| message.id == id)
                    .unwrap();
                assert!(message
                    .label_provider_ids
                    .iter()
                    .any(|provider_id| provider_id == &label.provider_id));
            }
            other => panic!("Expected Thread response, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn list_envelopes_includes_message_label_provider_ids() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        let create = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::CreateLabel {
                name: "Recruiters".into(),
                color: None,
                account_id: None,
            }),
        };
        let label = match handle_request(&state, &create).await.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Label { label },
            }) => label,
            other => panic!("Expected Label response, got {:?}", other),
        };

        state
            .store
            .add_message_label(&id, &label.id, mxr_core::EventSource::User)
            .await
            .unwrap();

        let msg = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::ListEnvelopes {
                label_id: None,
                account_id: None,
                limit: 200,
                offset: 0,
            }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Envelopes { envelopes },
            }) => {
                let envelope = envelopes
                    .into_iter()
                    .find(|envelope| envelope.id == id)
                    .unwrap();
                assert!(envelope
                    .label_provider_ids
                    .iter()
                    .any(|provider_id| provider_id == &label.provider_id));
            }
            other => panic!("Expected Envelopes response, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn list_accounts_surfaces_runtime_accounts_without_config_entries() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::ListAccounts),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Accounts { accounts },
            }) => {
                assert_eq!(accounts.len(), 1);
                assert_eq!(accounts[0].email, "user@example.com");
                assert_eq!(accounts[0].source, AccountSourceData::Runtime);
                assert_eq!(accounts[0].editable, AccountEditModeData::RuntimeOnly);
                assert!(accounts[0].is_default);
            }
            other => panic!("Expected Accounts response, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn get_llm_status_reports_noop_provider_by_default() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::GetLlmStatus),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::LlmStatus { snapshot },
            }) => {
                assert!(!snapshot.enabled);
                assert_eq!(snapshot.provider, "noop");
                assert_eq!(snapshot.model, "noop");
                assert_eq!(snapshot.configured_model, "qwen2.5:3b-instruct");
                assert_eq!(snapshot.base_url, None);
                assert_eq!(snapshot.context_window, 0);
            }
            other => panic!("Expected LlmStatus response, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn config_reload_rebuilds_llm_provider_for_status() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let mut config = state.config_snapshot();
        config.llm.enabled = true;
        config.llm.model = "local-test-model".to_string();
        config.llm.base_url = "http://127.0.0.1:11434/v1".to_string();
        config.llm.context_window = 4096;
        config.llm.request_timeout_secs = 30;
        state.set_config_for_test(config).await;

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::GetLlmStatus),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::LlmStatus { snapshot },
            }) => {
                assert!(snapshot.enabled);
                assert_eq!(snapshot.provider, "openai_compatible");
                assert_eq!(snapshot.model, "local-test-model");
                assert_eq!(snapshot.configured_model, "local-test-model");
                assert_eq!(
                    snapshot.base_url.as_deref(),
                    Some("http://127.0.0.1:11434/v1")
                );
                assert_eq!(snapshot.context_window, 4096);
                assert_eq!(snapshot.request_timeout_secs, 30);
            }
            other => panic!("Expected LlmStatus response, got {:?}", other),
        }
    }

    #[test]
    fn update_llm_config_persists_and_rebuilds_provider_status() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let config_dir = temp_dir.path().join("config");
        let data_dir = temp_dir.path().join("data");
        let socket_path = temp_dir.path().join("mxr.sock");
        std::fs::create_dir_all(&config_dir).expect("config dir");

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        temp_env::with_vars(
            [
                ("MXR_CONFIG_DIR", Some(config_dir)),
                ("MXR_DATA_DIR", Some(data_dir)),
                ("MXR_SOCKET_PATH", Some(socket_path)),
            ],
            || {
                runtime.block_on(async {
                    mxr_config::save_config(&mxr_config::MxrConfig::default())
                        .expect("save default config");
                    let state = Arc::new(AppState::in_memory_without_accounts().await.unwrap());
                    let msg = IpcMessage {
                        id: 1,
                        payload: IpcPayload::Request(Request::UpdateLlmConfig {
                            config: mxr_protocol::LlmConfigData {
                                enabled: true,
                                base_url: "http://127.0.0.1:11434/v1".into(),
                                model: "local-test-model".into(),
                                api_key_env: "MXR_TEST_LLM_KEY".into(),
                                context_window: 4096,
                                request_timeout_secs: 30,
                                allow_cloud_relationship_data: true,
                                overrides: None,
                            },
                        }),
                    };

                    let resp = handle_request(&state, &msg).await;
                    match resp.payload {
                        IpcPayload::Response(Response::Ok {
                            data: ResponseData::LlmConfig { config },
                        }) => {
                            assert!(config.enabled);
                            assert_eq!(config.model, "local-test-model");
                            assert!(config.allow_cloud_relationship_data);
                        }
                        other => panic!("Expected LlmConfig response, got {other:?}"),
                    }

                    let saved = mxr_config::load_config().expect("load saved config");
                    assert!(saved.llm.enabled);
                    assert_eq!(saved.llm.model, "local-test-model");
                    assert_eq!(saved.llm.api_key_env, "MXR_TEST_LLM_KEY");
                    assert!(saved.llm.allow_cloud_relationship_data);

                    let status_msg = IpcMessage {
                        id: 2,
                        payload: IpcPayload::Request(Request::GetLlmStatus),
                    };
                    let status_resp = handle_request(&state, &status_msg).await;
                    match status_resp.payload {
                        IpcPayload::Response(Response::Ok {
                            data: ResponseData::LlmStatus { snapshot },
                        }) => {
                            assert!(snapshot.enabled);
                            assert_eq!(snapshot.provider, "openai_compatible");
                            assert_eq!(snapshot.model, "local-test-model");
                            assert_eq!(snapshot.context_window, 4096);
                            assert_eq!(snapshot.request_timeout_secs, 30);
                        }
                        other => panic!("Expected LlmStatus response, got {other:?}"),
                    }
                });
            },
        );
    }

    #[tokio::test]
    async fn update_llm_config_rejects_blank_model() {
        let state = Arc::new(AppState::in_memory_without_accounts().await.unwrap());
        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::UpdateLlmConfig {
                config: mxr_protocol::LlmConfigData {
                    enabled: true,
                    base_url: "http://127.0.0.1:11434/v1".into(),
                    model: "  ".into(),
                    api_key_env: String::new(),
                    context_window: 4096,
                    request_timeout_secs: 30,
                    allow_cloud_relationship_data: false,
                    overrides: None,
                },
            }),
        };

        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Error { message, .. }) => {
                assert!(message.contains("llm.model must not be empty"));
            }
            other => panic!("Expected error response, got {other:?}"),
        }
        assert_eq!(
            state.config_snapshot().llm.model,
            mxr_config::LlmConfig::default().model
        );
    }

    #[tokio::test]
    async fn dispatch_send_draft() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        let draft = mxr_core::types::Draft {
            id: mxr_core::DraftId::new(),
            account_id: state.default_account_id(),
            reply_headers: None,
            intent: mxr_core::DraftIntent::New,
            to: vec![mxr_core::types::Address {
                name: None,
                email: "test@example.com".to_string(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "Test subject".to_string(),
            body_markdown: "Test body".to_string(),
            attachments: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::SendDraft { draft, override_safety_token: None }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SendReceipt { .. },
            }) => {}
            other => panic!("Expected SendReceipt, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn draft_only_safety_policy_blocks_send_but_allows_local_draft() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let mut config = state.config_snapshot();
        config.general.safety_policy = mxr_config::SafetyPolicy::DraftOnly;
        state.set_config_for_test(config).await;

        let draft = mxr_core::types::Draft {
            id: mxr_core::DraftId::new(),
            account_id: state.default_account_id(),
            reply_headers: None,
            intent: mxr_core::DraftIntent::New,
            to: vec![mxr_core::types::Address {
                name: None,
                email: "test@example.com".to_string(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "Draft-only policy".to_string(),
            body_markdown: "Test body".to_string(),
            attachments: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let send = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::SendDraft {
                draft: draft.clone(),
                override_safety_token: None,
            }),
        };
        match handle_request(&state, &send).await.payload {
            IpcPayload::Response(Response::Error { message, .. }) => {
                assert!(message.contains("draft-only safety policy"));
            }
            other => panic!("Expected safety policy error, got {:?}", other),
        }

        let save = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::SaveDraft { draft }),
        };
        match handle_request(&state, &save).await.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected SaveDraft Ack, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn read_only_safety_policy_blocks_mutations_but_allows_search() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let mut config = state.config_snapshot();
        config.general.safety_policy = mxr_config::SafetyPolicy::ReadOnly;
        state.set_config_for_test(config).await;

        let mutation = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::mutation(MutationCommand::Star {
                message_ids: vec![mxr_core::MessageId::new()],
                starred: true,
            })),
        };
        match handle_request(&state, &mutation).await.payload {
            IpcPayload::Response(Response::Error { message, .. }) => {
                assert!(message.contains("read-only safety policy"));
            }
            other => panic!("Expected safety policy error, got {:?}", other),
        }

        let search = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::Search {
                query: "hello".into(),
                limit: 10,
                offset: 0,
                mode: None,
                sort: None,
                explain: false,
            }),
        };
        match handle_request(&state, &search).await.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SearchResults { .. },
            }) => {}
            other => panic!("Expected SearchResults, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_send_draft_preserves_keychain_repair_error() {
        let account_id = mxr_core::AccountId::new();
        let account = crate::test_fixtures::test_account_with_id(account_id.clone());
        let sync_provider = Arc::new(mxr_provider_fake::FakeProvider::new(account_id.clone()));
        let send_provider: Arc<dyn mxr_core::MailSendProvider> = Arc::new(FailingSendProvider {
            message: "Keyring error: Password for mxr/consulting-smtp/hello@bhekani.com requires interactive macOS keychain approval. Re-save that account password once with `mxr accounts repair`.",
        });
        let state = Arc::new(
            AppState::in_memory_with_sync_provider(account, sync_provider, Some(send_provider))
                .await
                .unwrap(),
        );

        let draft = mxr_core::types::Draft {
            id: mxr_core::DraftId::new(),
            account_id,
            reply_headers: None,
            intent: mxr_core::DraftIntent::New,
            to: vec![mxr_core::types::Address {
                name: None,
                email: "test@example.com".to_string(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "Test subject".to_string(),
            body_markdown: "Test body".to_string(),
            attachments: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::SendDraft { draft, override_safety_token: None }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Error { message, .. }) => {
                assert!(message.contains("consulting-smtp"));
                assert!(message.contains("mxr accounts repair"));
            }
            other => panic!("Expected send error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_snooze_and_list() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        // Snooze
        let wake_at = chrono::Utc::now() + chrono::Duration::hours(24);
        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::Snooze {
                message_id: id.clone(),
                wake_at,
            }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack for Snooze, got {:?}", other),
        }

        // List snoozed - should have 1
        let msg = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::ListSnoozed),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SnoozedMessages { snoozed },
            }) => {
                assert_eq!(snoozed.len(), 1, "Expected 1 snoozed message");
            }
            other => panic!("Expected SnoozedMessages, got {:?}", other),
        }

        // Unsnooze
        let msg = IpcMessage {
            id: 3,
            payload: IpcPayload::Request(Request::Unsnooze { message_id: id }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack for Unsnooze, got {:?}", other),
        }

        // List snoozed - should have 0
        let msg = IpcMessage {
            id: 4,
            payload: IpcPayload::Request(Request::ListSnoozed),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SnoozedMessages { snoozed },
            }) => {
                assert_eq!(
                    snoozed.len(),
                    0,
                    "Expected 0 snoozed messages after unsnooze"
                );
            }
            other => panic!("Expected SnoozedMessages, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn snooze_removes_inbox_and_unsnooze_restores_it() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;
        let envelope = state.store.get_envelope(&id).await.unwrap().unwrap();
        let inbox = state
            .store
            .list_labels_by_account(&envelope.account_id)
            .await
            .unwrap()
            .into_iter()
            .find(|label| label.provider_id == "INBOX")
            .unwrap();

        let before = state.store.get_message_label_ids(&id).await.unwrap();
        assert!(before.iter().any(|label_id| label_id == &inbox.id));

        let snooze = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::Snooze {
                message_id: id.clone(),
                wake_at: chrono::Utc::now() + chrono::Duration::hours(4),
            }),
        };
        match handle_request(&state, &snooze).await.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack, got {:?}", other),
        }

        let snoozed_labels = state.store.get_message_label_ids(&id).await.unwrap();
        assert!(!snoozed_labels.iter().any(|label_id| label_id == &inbox.id));

        let unsnooze = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::Unsnooze {
                message_id: id.clone(),
            }),
        };
        match handle_request(&state, &unsnooze).await.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack, got {:?}", other),
        }

        let restored_labels = state.store.get_message_label_ids(&id).await.unwrap();
        assert!(restored_labels.iter().any(|label_id| label_id == &inbox.id));
    }

    #[tokio::test]
    async fn dispatch_set_flags() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        use mxr_core::types::MessageFlags;
        let flags = MessageFlags::READ | MessageFlags::STARRED;
        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::SetFlags {
                message_id: id.clone(),
                flags,
            }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack, got {:?}", other),
        }

        // Verify flags
        let get_msg = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::GetEnvelope { message_id: id }),
        };
        let resp = handle_request(&state, &get_msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Envelope { envelope },
            }) => {
                assert_eq!(
                    envelope.flags, flags,
                    "Expected flags {:?}, got {:?}",
                    flags, envelope.flags
                );
            }
            other => panic!("Expected Envelope, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_unsubscribe_no_method() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        // The first envelope from FakeProvider fixtures uses UnsubscribeMethod::None
        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::Unsubscribe { message_id: id }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Error { message, .. }) => {
                assert!(
                    message.contains("unsubscribe"),
                    "Expected error about unsubscribe, got: {}",
                    message
                );
            }
            other => panic!("Expected Error for no unsubscribe method, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_unsubscribe_mailto_sends_via_provider() {
        let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
        let state = Arc::new(state);
        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();

        let mailto_id = state
            .store
            .list_envelopes_by_account(&state.default_account_id(), 200, 0)
            .await
            .unwrap()
            .into_iter()
            .find(|envelope| matches!(envelope.unsubscribe, UnsubscribeMethod::Mailto { .. }))
            .map(|envelope| envelope.id)
            .expect("mailto fixture");

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::Unsubscribe {
                message_id: mailto_id,
            }),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack for mailto unsubscribe, got {:?}", other),
        }

        let sent = fake.sent_drafts();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].to[0].email, "unsub@changelog.com");
        assert_eq!(sent[0].subject, "unsubscribe");
    }

    #[tokio::test]
    async fn dispatch_mutation_nonexistent_message() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        let fake_id = mxr_core::MessageId::new();
        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::mutation(MutationCommand::Star {
                message_ids: vec![fake_id],
                starred: true,
            })),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Error { message, .. }) => {
                assert!(
                    message.contains("not found") || message.contains("Not found"),
                    "Expected 'not found' error, got: {}",
                    message
                );
            }
            other => panic!("Expected Error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_list_drafts_empty() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::ListDrafts),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Drafts { drafts },
            }) => {
                assert!(drafts.is_empty(), "Expected empty drafts list");
            }
            other => panic!("Expected Drafts, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_list_drafts_includes_all_accounts() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let default_account_id = state.default_account_id();
        let other_account_id = mxr_core::AccountId::new();
        let other_account = crate::test_fixtures::test_account_with_id(other_account_id.clone());
        state.store.insert_account(&other_account).await.unwrap();

        let old_draft = mxr_core::types::Draft {
            id: mxr_core::DraftId::new(),
            account_id: default_account_id,
            reply_headers: None,
            intent: mxr_core::DraftIntent::New,
            to: vec![],
            cc: vec![],
            bcc: vec![],
            subject: "Default account draft".to_string(),
            body_markdown: "older".to_string(),
            attachments: vec![],
            created_at: chrono::Utc::now() - chrono::Duration::minutes(5),
            updated_at: chrono::Utc::now() - chrono::Duration::minutes(5),
        };
        let new_draft = mxr_core::types::Draft {
            id: mxr_core::DraftId::new(),
            account_id: other_account_id,
            reply_headers: None,
            intent: mxr_core::DraftIntent::New,
            to: vec![],
            cc: vec![],
            bcc: vec![],
            subject: "Other account draft".to_string(),
            body_markdown: "newer".to_string(),
            attachments: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        state.store.insert_draft(&old_draft).await.unwrap();
        state.store.insert_draft(&new_draft).await.unwrap();

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::ListDrafts),
        };
        let resp = handle_request(&state, &msg).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Drafts { drafts },
            }) => {
                assert_eq!(drafts.len(), 2);
                assert_eq!(drafts[0].id, new_draft.id);
                assert_eq!(drafts[1].id, old_draft.id);
            }
            other => panic!("Expected Drafts, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_save_and_send_stored_draft() {
        let account_id = mxr_core::AccountId::new();
        let account = crate::test_fixtures::test_account_with_id(account_id.clone());
        let fake = Arc::new(mxr_provider_fake::FakeProvider::new(account_id.clone()));
        let sync_provider: Arc<dyn mxr_core::MailSyncProvider> = fake.clone();
        let send_provider: Arc<dyn mxr_core::MailSendProvider> = fake.clone();
        let state = Arc::new(
            AppState::in_memory_with_sync_provider(account, sync_provider, Some(send_provider))
                .await
                .unwrap(),
        );

        let draft = mxr_core::types::Draft {
            id: mxr_core::DraftId::new(),
            account_id,
            reply_headers: None,
            intent: mxr_core::DraftIntent::New,
            to: vec![mxr_core::types::Address {
                name: None,
                email: "test@example.com".to_string(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "Stored draft".to_string(),
            body_markdown: "Test body".to_string(),
            attachments: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let save_msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::SaveDraft {
                draft: draft.clone(),
            }),
        };
        let save_resp = handle_request(&state, &save_msg).await;
        assert!(matches!(
            save_resp.payload,
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack
            })
        ));

        let send_msg = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::SendStoredDraft { draft_id: draft.id.clone(), override_safety_token: None }),
        };
        let send_resp = handle_request(&state, &send_msg).await;
        assert!(
            matches!(
                send_resp.payload,
                IpcPayload::Response(Response::Ok {
                    data: ResponseData::SendReceipt { .. }
                })
            ),
            "send_stored_draft should return SendReceipt, got {:?}",
            send_resp.payload
        );

        assert_eq!(fake.sent_drafts().len(), 1);
        assert!(state.store.get_draft(&draft.id).await.unwrap().is_none());
    }

    /// Slice 1.3: when CheckDraftSafety returns Blocked, the daemon
    /// mints a single-use override token and stamps it onto each
    /// blocker issue. The next SendStoredDraft with that token must
    /// succeed (and FakeProvider must actually be invoked exactly once),
    /// while a second send attempt with the same token must fail with
    /// the token already-used error.
    #[tokio::test]
    async fn override_token_unblocks_send_exactly_once() {
        let account_id = mxr_core::AccountId::new();
        let account = crate::test_fixtures::test_account_with_id(account_id.clone());
        let fake = Arc::new(mxr_provider_fake::FakeProvider::new(account_id.clone()));
        let sync_provider: Arc<dyn mxr_core::MailSyncProvider> = fake.clone();
        let send_provider: Arc<dyn mxr_core::MailSendProvider> = fake.clone();
        let state = Arc::new(
            AppState::in_memory_with_sync_provider(account, sync_provider, Some(send_provider))
                .await
                .unwrap(),
        );

        // PEM private key in the body → Blocker.
        let draft = mxr_core::types::Draft {
            id: mxr_core::DraftId::new(),
            account_id: account_id.clone(),
            reply_headers: None,
            intent: mxr_core::DraftIntent::New,
            to: vec![mxr_core::types::Address {
                name: None,
                email: "alice@example.com".to_string(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "key transfer".to_string(),
            body_markdown: "Here is the key:\n-----BEGIN RSA PRIVATE KEY-----\n...\n"
                .to_string(),
            attachments: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        // Save the draft so SendStoredDraft can locate it.
        let save = handle_request(
            &state,
            &IpcMessage {
                id: 1,
                payload: IpcPayload::Request(Request::SaveDraft {
                    draft: draft.clone(),
                }),
            },
        )
        .await;
        assert!(matches!(
            save.payload,
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack
            })
        ));

        // 1. Check returns Blocked + a token on the blocker issue.
        let check = handle_request(
            &state,
            &IpcMessage {
                id: 2,
                payload: IpcPayload::Request(Request::CheckDraftSafety {
                    draft: draft.clone(),
                    context: Default::default(),
                }),
            },
        )
        .await;
        let token = match check.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::DraftSafetyReportResponse { report },
            }) => {
                assert!(matches!(
                    report.verdict,
                    mxr_core::DraftSafetyVerdict::Blocked
                ));
                let blocker = report
                    .issues
                    .iter()
                    .find(|i| i.severity == mxr_core::DraftSafetySeverity::Blocker)
                    .expect("at least one blocker");
                blocker
                    .override_token
                    .clone()
                    .expect("blocker should carry override token")
            }
            other => panic!("expected DraftSafetyReportResponse, got {other:?}"),
        };

        // 2. SendStoredDraft WITHOUT the token: refused, FakeProvider untouched.
        assert_eq!(fake.sent_drafts().len(), 0);
        let blocked = handle_request(
            &state,
            &IpcMessage {
                id: 3,
                payload: IpcPayload::Request(Request::SendStoredDraft {
                    draft_id: draft.id.clone(),
                    override_safety_token: None,
                }),
            },
        )
        .await;
        match blocked.payload {
            IpcPayload::Response(Response::Error { message, .. }) => {
                assert!(message.contains("blocked"), "{message}");
            }
            other => panic!("expected Error, got {other:?}"),
        }
        assert_eq!(
            fake.sent_drafts().len(),
            0,
            "provider must NOT be called when blocked"
        );
        // Draft must still be in `Draft` status (no CAS to Sending).
        assert!(state.store.get_draft(&draft.id).await.unwrap().is_some());

        // 3. SendStoredDraft WITH token: succeeds.
        let ok = handle_request(
            &state,
            &IpcMessage {
                id: 4,
                payload: IpcPayload::Request(Request::SendStoredDraft {
                    draft_id: draft.id.clone(),
                    override_safety_token: Some(token.clone()),
                }),
            },
        )
        .await;
        assert!(
            matches!(
                ok.payload,
                IpcPayload::Response(Response::Ok {
                    data: ResponseData::SendReceipt { .. }
                })
            ),
            "expected SendReceipt with override, got {:?}",
            ok.payload
        );
        assert_eq!(fake.sent_drafts().len(), 1);

        // 4. Reusing the same token after the draft is gone — token is
        // single-use; consume must fail. We test by minting a fresh
        // override against a new draft, sending once, then trying the
        // SAME token a second time to assert single-use.
        let draft2 = mxr_core::types::Draft {
            id: mxr_core::DraftId::new(),
            account_id: account_id.clone(),
            reply_headers: None,
            intent: mxr_core::DraftIntent::New,
            to: vec![mxr_core::types::Address {
                name: None,
                email: "bob@example.com".to_string(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "again".into(),
            body_markdown: "-----BEGIN RSA PRIVATE KEY-----\nzz\n".into(),
            attachments: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let _ = handle_request(
            &state,
            &IpcMessage {
                id: 5,
                payload: IpcPayload::Request(Request::SaveDraft {
                    draft: draft2.clone(),
                }),
            },
        )
        .await;
        let check2 = handle_request(
            &state,
            &IpcMessage {
                id: 6,
                payload: IpcPayload::Request(Request::CheckDraftSafety {
                    draft: draft2.clone(),
                    context: Default::default(),
                }),
            },
        )
        .await;
        let token2 = match check2.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::DraftSafetyReportResponse { report },
            }) => report
                .issues
                .iter()
                .find(|i| i.severity == mxr_core::DraftSafetySeverity::Blocker)
                .and_then(|i| i.override_token.clone())
                .expect("blocker token"),
            other => panic!("unexpected: {other:?}"),
        };
        // First use succeeds.
        let first = handle_request(
            &state,
            &IpcMessage {
                id: 7,
                payload: IpcPayload::Request(Request::SendStoredDraft {
                    draft_id: draft2.id.clone(),
                    override_safety_token: Some(token2.clone()),
                }),
            },
        )
        .await;
        assert!(matches!(
            first.payload,
            IpcPayload::Response(Response::Ok { .. })
        ));
        // Second use with the SAME token must fail (token consumed). We
        // can't re-send the same draft (already gone after send), so we
        // make a third draft and try to use the spent token.
        let draft3 = mxr_core::types::Draft {
            id: mxr_core::DraftId::new(),
            ..draft2
        };
        let _ = handle_request(
            &state,
            &IpcMessage {
                id: 8,
                payload: IpcPayload::Request(Request::SaveDraft {
                    draft: draft3.clone(),
                }),
            },
        )
        .await;
        let reuse = handle_request(
            &state,
            &IpcMessage {
                id: 9,
                payload: IpcPayload::Request(Request::SendStoredDraft {
                    draft_id: draft3.id.clone(),
                    override_safety_token: Some(token2),
                }),
            },
        )
        .await;
        match reuse.payload {
            IpcPayload::Response(Response::Error { message, .. }) => {
                assert!(
                    message.contains("override token unknown or already used")
                        || message.contains("does not cover blocker"),
                    "got {message}"
                );
            }
            other => panic!("expected error on token reuse, got {other:?}"),
        }
    }

    /// The live send pipeline must touch `last_heartbeat_at` once it has
    /// CAS'd a draft into `Sending`. Otherwise, a long-running send (large
    /// attachment, slow OAuth refresh) could be misidentified as orphaned
    /// by the 1h startup recovery cutoff. We verify this by exercising the
    /// failure path: with no send provider configured, `send_stored_draft`
    /// CAS's into `Sending`, touches the heartbeat, then reverts to
    /// `Draft` when provider lookup fails — leaving a fresh heartbeat we
    /// can read back.
    #[tokio::test]
    async fn send_stored_draft_touches_heartbeat_after_cas() {
        let account_id = mxr_core::AccountId::new();
        let account = crate::test_fixtures::test_account_with_id(account_id.clone());
        let fake = Arc::new(mxr_provider_fake::FakeProvider::new(account_id.clone()));
        let sync_provider: Arc<dyn mxr_core::MailSyncProvider> = fake.clone();
        // No send provider — `send_provider_for_account` will fail.
        let state = Arc::new(
            AppState::in_memory_with_sync_provider(account, sync_provider, None)
                .await
                .unwrap(),
        );

        let draft = mxr_core::types::Draft {
            id: mxr_core::DraftId::new(),
            account_id,
            reply_headers: None,
            intent: mxr_core::DraftIntent::New,
            to: vec![mxr_core::types::Address {
                name: None,
                email: "test@example.com".to_string(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "Heartbeat probe".to_string(),
            body_markdown: "Body".to_string(),
            attachments: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        state.store.insert_draft(&draft).await.unwrap();
        // Pre-condition: a brand-new draft has no heartbeat.
        assert_eq!(
            state.store.get_draft_heartbeat(&draft.id).await.unwrap(),
            None,
            "fresh draft must have NULL last_heartbeat_at"
        );

        let before = chrono::Utc::now();
        let send_msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::SendStoredDraft { draft_id: draft.id.clone(), override_safety_token: None }),
        };
        let send_resp = handle_request(&state, &send_msg).await;
        assert!(
            matches!(
                send_resp.payload,
                IpcPayload::Response(Response::Error { .. })
            ),
            "send_stored_draft without a send provider must error, got {:?}",
            send_resp.payload
        );

        // Post-condition: heartbeat was set during the CAS-to-Sending phase
        // and survives the revert-to-Draft on provider-lookup failure.
        let heartbeat = state
            .store
            .get_draft_heartbeat(&draft.id)
            .await
            .unwrap()
            .expect("send_stored_draft must touch the heartbeat after CAS");
        let after = chrono::Utc::now();
        assert!(
            heartbeat >= before - chrono::Duration::seconds(1),
            "heartbeat {heartbeat} must not predate test start {before}"
        );
        assert!(
            heartbeat <= after + chrono::Duration::seconds(1),
            "heartbeat {heartbeat} must not postdate test end {after}"
        );
    }

    #[tokio::test]
    async fn send_stored_draft_blocks_empty_recipient_before_sending_state() {
        let account_id = mxr_core::AccountId::new();
        let account = crate::test_fixtures::test_account_with_id(account_id.clone());
        let fake = Arc::new(mxr_provider_fake::FakeProvider::new(account_id.clone()));
        let sync_provider: Arc<dyn mxr_core::MailSyncProvider> = fake.clone();
        let send_provider: Arc<dyn mxr_core::MailSendProvider> = fake.clone();
        let state = Arc::new(
            AppState::in_memory_with_sync_provider(account, sync_provider, Some(send_provider))
                .await
                .unwrap(),
        );

        let draft = mxr_core::types::Draft {
            id: mxr_core::DraftId::new(),
            account_id,
            reply_headers: None,
            intent: mxr_core::DraftIntent::New,
            to: vec![],
            cc: vec![],
            bcc: vec![],
            subject: "No recipients".to_string(),
            body_markdown: "Body".to_string(),
            attachments: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        state.store.insert_draft(&draft).await.unwrap();

        let send_msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::SendStoredDraft { draft_id: draft.id.clone(), override_safety_token: None }),
        };
        match handle_request(&state, &send_msg).await.payload {
            IpcPayload::Response(Response::Error { message, .. }) => {
                assert!(message.contains("draft safety"));
                assert!(message.contains("recipient"));
            }
            other => panic!("Expected draft safety error, got {other:?}"),
        }

        assert_eq!(
            state.store.get_draft_status(&draft.id).await.unwrap(),
            Some(mxr_core::DraftStatus::Draft)
        );
        assert_eq!(
            state.store.get_draft_heartbeat(&draft.id).await.unwrap(),
            None
        );
        assert_eq!(fake.sent_drafts().len(), 0);
    }

    #[tokio::test]
    async fn send_draft_blocks_invalid_recipient_before_provider_send() {
        let account_id = mxr_core::AccountId::new();
        let account = crate::test_fixtures::test_account_with_id(account_id.clone());
        let fake = Arc::new(mxr_provider_fake::FakeProvider::new(account_id.clone()));
        let sync_provider: Arc<dyn mxr_core::MailSyncProvider> = fake.clone();
        let send_provider: Arc<dyn mxr_core::MailSendProvider> = fake.clone();
        let state = Arc::new(
            AppState::in_memory_with_sync_provider(account, sync_provider, Some(send_provider))
                .await
                .unwrap(),
        );

        let draft = mxr_core::types::Draft {
            id: mxr_core::DraftId::new(),
            account_id,
            reply_headers: None,
            intent: mxr_core::DraftIntent::New,
            to: vec![mxr_core::types::Address {
                name: None,
                email: "not an address".to_string(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "Invalid recipient".to_string(),
            body_markdown: "Body".to_string(),
            attachments: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let send_msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::SendDraft { draft, override_safety_token: None }),
        };
        match handle_request(&state, &send_msg).await.payload {
            IpcPayload::Response(Response::Error { message, .. }) => {
                assert!(message.contains("draft safety"));
                assert!(message.contains("invalid recipient"));
            }
            other => panic!("Expected draft safety error, got {other:?}"),
        }

        assert_eq!(fake.sent_drafts().len(), 0);
    }

    #[tokio::test]
    async fn send_stored_reply_all_blocks_missing_original_recipient_before_sending_state() {
        let account_id = mxr_core::AccountId::new();
        let account = crate::test_fixtures::test_account_with_id(account_id.clone());
        let account_email = account.email.clone();
        let fake = Arc::new(mxr_provider_fake::FakeProvider::new(account_id.clone()));
        let sync_provider: Arc<dyn mxr_core::MailSyncProvider> = fake.clone();
        let send_provider: Arc<dyn mxr_core::MailSendProvider> = fake.clone();
        let state = Arc::new(
            AppState::in_memory_with_sync_provider(account, sync_provider, Some(send_provider))
                .await
                .unwrap(),
        );

        let mut parent = crate::test_fixtures::TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .provider_id("reply-all-parent")
            .message_id_header(Some("<reply-all-parent@example.com>".to_string()))
            .build();
        parent.from = mxr_core::types::Address {
            name: None,
            email: "alice@example.com".to_string(),
        };
        parent.to = vec![
            mxr_core::types::Address {
                name: None,
                email: account_email,
            },
            mxr_core::types::Address {
                name: None,
                email: "bob@example.com".to_string(),
            },
        ];
        parent.cc = vec![mxr_core::types::Address {
            name: None,
            email: "carol@example.com".to_string(),
        }];
        state.store.upsert_envelope(&parent).await.unwrap();

        let draft = mxr_core::types::Draft {
            id: mxr_core::DraftId::new(),
            account_id,
            reply_headers: Some(mxr_core::ReplyHeaders {
                in_reply_to: "<reply-all-parent@example.com>".to_string(),
                references: vec!["<reply-all-parent@example.com>".to_string()],
                thread_id: None,
            }),
            intent: mxr_core::DraftIntent::ReplyAll,
            to: vec![mxr_core::types::Address {
                name: None,
                email: "alice@example.com".to_string(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "Re: parent".to_string(),
            body_markdown: "reply".to_string(),
            attachments: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        state.store.insert_draft(&draft).await.unwrap();

        let send_msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::SendStoredDraft { draft_id: draft.id.clone(), override_safety_token: None }),
        };
        match handle_request(&state, &send_msg).await.payload {
            IpcPayload::Response(Response::Error { message, .. }) => {
                assert!(message.contains("reply-all is missing recipient"));
                assert!(message.contains("bob@example.com"));
            }
            other => panic!("Expected draft safety error, got {other:?}"),
        }

        assert_eq!(
            state.store.get_draft_status(&draft.id).await.unwrap(),
            Some(mxr_core::DraftStatus::Draft)
        );
        assert_eq!(
            state.store.get_draft_heartbeat(&draft.id).await.unwrap(),
            None
        );
        assert_eq!(fake.sent_drafts().len(), 0);
    }

    #[tokio::test]
    async fn dispatch_send_draft_preserves_parent_thread_for_synthetic_sent() {
        let account_id = mxr_core::AccountId::new();
        let account = crate::test_fixtures::test_account_with_id(account_id.clone());
        let fake = Arc::new(mxr_provider_fake::FakeProvider::new(account_id.clone()));
        let sync_provider: Arc<dyn mxr_core::MailSyncProvider> = fake.clone();
        let send_provider: Arc<dyn mxr_core::MailSendProvider> = fake.clone();
        let state = Arc::new(
            AppState::in_memory_with_sync_provider(account, sync_provider, Some(send_provider))
                .await
                .unwrap(),
        );
        let parent_thread_id = mxr_core::ThreadId::new();
        let parent = crate::test_fixtures::TestEnvelopeBuilder::new()
            .account_id(account_id.clone())
            .thread_id(parent_thread_id.clone())
            .provider_id("parent")
            .message_id_header(Some("<parent@example.com>".to_string()))
            .build();
        state.store.upsert_envelope(&parent).await.unwrap();
        state
            .store
            .set_reply_later(&parent.id, chrono::Utc::now())
            .await
            .unwrap();

        let draft = mxr_core::types::Draft {
            id: mxr_core::DraftId::new(),
            account_id,
            reply_headers: Some(mxr_core::ReplyHeaders {
                in_reply_to: "<parent@example.com>".to_string(),
                references: vec!["<parent@example.com>".to_string()],
                thread_id: None,
            }),
            intent: mxr_core::DraftIntent::Reply,
            to: vec![mxr_core::types::Address {
                name: None,
                email: "test@example.com".to_string(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "Re: parent".to_string(),
            body_markdown: "reply".to_string(),
            attachments: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::SendDraft { draft, override_safety_token: None }),
        };
        let resp = handle_request(&state, &msg).await;
        let local_message_id = match resp.payload {
            IpcPayload::Response(Response::Ok {
                data:
                    ResponseData::SendReceipt {
                        local_message_id, ..
                    },
            }) => local_message_id,
            other => panic!("Expected SendReceipt, got {:?}", other),
        };
        let sent = state
            .store
            .get_envelope(&local_message_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(sent.thread_id, parent_thread_id);
        assert!(
            !state.store.is_reply_later(&parent.id).await.unwrap(),
            "sending a reply clears the parent reply-later flag"
        );
    }

    #[tokio::test]
    async fn dispatch_save_draft_to_server_falls_back_to_local_draft() {
        let account_id = mxr_core::AccountId::new();
        let account = crate::test_fixtures::test_account_with_id(account_id.clone());
        let fake = Arc::new(mxr_provider_fake::FakeProvider::new(account_id.clone()));
        let sync_provider: Arc<dyn mxr_core::MailSyncProvider> = fake;
        let send_provider: Arc<dyn mxr_core::MailSendProvider> =
            Arc::new(UnsupportedServerDraftProvider);
        let state = Arc::new(
            AppState::in_memory_with_sync_provider(account, sync_provider, Some(send_provider))
                .await
                .unwrap(),
        );

        let draft = mxr_core::types::Draft {
            id: mxr_core::DraftId::new(),
            account_id,
            reply_headers: None,
            intent: mxr_core::DraftIntent::New,
            to: vec![mxr_core::types::Address {
                name: None,
                email: "test@example.com".to_string(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "Local fallback".to_string(),
            body_markdown: "body".to_string(),
            attachments: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::SaveDraftToServer {
                draft: draft.clone(),
            }),
        };
        let resp = handle_request(&state, &msg).await;
        assert!(matches!(
            resp.payload,
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack
            })
        ));
        assert!(state.store.get_draft(&draft.id).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn dispatch_saved_search_delete() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        // Create a saved search
        let create_msg = IpcMessage {
            id: 20,
            payload: IpcPayload::Request(Request::CreateSavedSearch {
                name: "ToDelete".to_string(),
                query: "is:unread".to_string(),
                search_mode: mxr_core::SearchMode::Lexical,
            }),
        };
        let resp = handle_request(&state, &create_msg).await;
        match &resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SavedSearchData { search },
            }) => {
                assert_eq!(search.name, "ToDelete");
            }
            other => panic!("Expected SavedSearchData, got {:?}", other),
        }

        // Verify it's in the list
        let list_msg = IpcMessage {
            id: 21,
            payload: IpcPayload::Request(Request::ListSavedSearches),
        };
        let resp = handle_request(&state, &list_msg).await;
        match &resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SavedSearches { searches },
            }) => {
                assert_eq!(searches.len(), 1);
                assert_eq!(searches[0].name, "ToDelete");
            }
            other => panic!("Expected SavedSearches with 1 item, got {:?}", other),
        }

        // Delete it
        let delete_msg = IpcMessage {
            id: 22,
            payload: IpcPayload::Request(Request::DeleteSavedSearch {
                name: "ToDelete".to_string(),
            }),
        };
        let resp = handle_request(&state, &delete_msg).await;
        match &resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack, got {:?}", other),
        }

        // Verify it's gone
        let list_msg2 = IpcMessage {
            id: 23,
            payload: IpcPayload::Request(Request::ListSavedSearches),
        };
        let resp = handle_request(&state, &list_msg2).await;
        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::SavedSearches { searches },
            }) => {
                assert!(
                    searches.is_empty(),
                    "Saved searches should be empty after delete"
                );
            }
            other => panic!("Expected empty SavedSearches, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_export_thread_markdown() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        // Sync to get messages
        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();

        // Get an envelope to find its thread_id
        let list_msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::ListEnvelopes {
                label_id: None,
                account_id: None,
                limit: 1,
                offset: 0,
            }),
        };
        let resp = handle_request(&state, &list_msg).await;
        let thread_id = match &resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Envelopes { envelopes },
            }) => envelopes[0].thread_id.clone(),
            other => panic!("Expected Envelopes, got {:?}", other),
        };

        // Export the thread as markdown
        let export_msg = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::ExportThread {
                thread_id,
                format: mxr_core::types::ExportFormat::Markdown,
            }),
        };
        let resp = handle_request(&state, &export_msg).await;
        match &resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::ExportResult { content },
            }) => {
                assert!(
                    content.starts_with("# Thread:"),
                    "Should be markdown: {}",
                    content
                );
                assert!(content.contains("Exported from mxr"));
            }
            other => panic!("Expected ExportResult, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_sync_now_acknowledges() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        let msg = IpcMessage {
            id: 300,
            payload: IpcPayload::Request(Request::SyncNow { account_id: None }),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_export_thread_json_is_valid() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();

        let list_msg = IpcMessage {
            id: 1,
            payload: IpcPayload::Request(Request::ListEnvelopes {
                label_id: None,
                account_id: None,
                limit: 1,
                offset: 0,
            }),
        };
        let resp = handle_request(&state, &list_msg).await;
        let thread_id = match &resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Envelopes { envelopes },
            }) => envelopes[0].thread_id.clone(),
            other => panic!("Expected Envelopes, got {:?}", other),
        };

        let export_msg = IpcMessage {
            id: 2,
            payload: IpcPayload::Request(Request::ExportThread {
                thread_id,
                format: mxr_core::types::ExportFormat::Json,
            }),
        };
        let resp = handle_request(&state, &export_msg).await;
        match &resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::ExportResult { content },
            }) => {
                let parsed: serde_json::Value =
                    serde_json::from_str(content).expect("Export JSON should be valid");
                assert!(parsed["message_count"].as_u64().unwrap() >= 1);
                assert!(parsed["subject"].is_string());
            }
            other => panic!("Expected ExportResult, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_get_headers_includes_standards_metadata() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let id = sync_and_get_first_id(&state).await;

        let mut body = state.store.get_body(&id).await.unwrap().unwrap();
        body.metadata.list_id = Some("fixtures.example.com".into());
        body.metadata.auth_results = vec!["mx.example.net; dkim=pass".into()];
        body.metadata.content_language = vec!["en".into(), "fr".into()];
        state.store.insert_body(&body).await.unwrap();

        let msg = IpcMessage {
            id: 3,
            payload: IpcPayload::Request(Request::GetHeaders {
                message_id: id.clone(),
            }),
        };
        let resp = handle_request(&state, &msg).await;

        let headers = match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Headers { headers },
            }) => headers,
            other => panic!("Expected Headers, got {:?}", other),
        };

        assert!(headers.iter().any(|(name, _)| name == "From"));
        assert!(headers.iter().any(|(name, _)| name == "Subject"));
        assert!(headers
            .iter()
            .any(|(name, value)| name == "List-Id" && value == "fixtures.example.com"));
        assert!(headers.iter().any(|(name, value)| {
            name == "Authentication-Results" && value == "mx.example.net; dkim=pass"
        }));
        assert!(headers
            .iter()
            .any(|(name, value)| { name == "Content-Language" && value == "en, fr" }));
    }

    #[tokio::test]
    async fn dispatch_export_search_json_is_valid() {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        state
            .sync_engine
            .sync_account(state.default_provider().as_ref())
            .await
            .unwrap();

        let msg = IpcMessage {
            id: 4,
            payload: IpcPayload::Request(Request::ExportSearch {
                query: "deployment".into(),
                format: mxr_core::types::ExportFormat::Json,
            }),
        };
        let resp = handle_request(&state, &msg).await;

        match &resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::ExportResult { content },
            }) => {
                let parsed: serde_json::Value =
                    serde_json::from_str(content).expect("Export JSON should be valid");
                let messages = parsed["messages"]
                    .as_array()
                    .expect("export search should include messages");
                assert!(messages.len() >= 1, "export search should return results");
                assert!(messages[0].as_object().is_some());
            }
            other => panic!("Expected ExportResult, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn dispatch_save_draft_to_server() {
        let state = Arc::new(AppState::in_memory().await.unwrap());

        let draft = mxr_core::types::Draft {
            id: mxr_core::DraftId::new(),
            account_id: state.default_account_id(),
            reply_headers: None,
            intent: mxr_core::DraftIntent::New,
            to: vec![mxr_core::types::Address {
                name: Some("Recipient".into()),
                email: "recipient@example.com".into(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "Saved draft".into(),
            body_markdown: "Body".into(),
            attachments: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let msg = IpcMessage {
            id: 5,
            payload: IpcPayload::Request(Request::SaveDraftToServer { draft }),
        };
        let resp = handle_request(&state, &msg).await;

        match resp.payload {
            IpcPayload::Response(Response::Ok {
                data: ResponseData::Ack,
            }) => {}
            other => panic!("Expected Ack, got {:?}", other),
        }
    }
}
