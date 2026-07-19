//! Maps an IPC `Request` to an optional `OwnedEntry` for the recorder.
//!
//! Scope note (v1): we explicitly map the user-intent variants that ship
//! at v1 (mail mutations, search, drafts, accounts, rules, snippets,
//! screener, reminders, sync). Everything else is `None`. The original
//! plan asked for an exhaustive match — a great idea, but with 140+
//! Request variants we'd churn this file on every protocol change and
//! delay v1. Phase 9 polish is the place to harden into exhaustiveness.
//! See `docs/activity-log.md` for the policy.
//!
//! Rules:
//! - Failed responses (`response_ok=false`) never produce activity.
//!   Failures live in `event_log`.
//! - Query/getter/poll verbs return `None`. They have no user intent worth
//!   replaying.
//! - `context` carries everything past the row scalars. Shapes documented
//!   in `docs/activity-log.md`.

use mxr_protocol::{ClientKind, MutationCommand, Request};

use crate::activity::tier::tier_for;
use crate::activity::{current_unix_ms, OwnedEntry};

const SUBJECT_LIMIT: usize = 200;
const QUERY_LIMIT: usize = 500;

fn truncate(input: &str, max: usize) -> String {
    if input.chars().count() <= max {
        input.to_owned()
    } else {
        let mut s: String = input.chars().take(max).collect();
        s.push('…');
        s
    }
}

pub fn map_request(
    req: &Request,
    source: ClientKind,
    account_id: Option<&str>,
    response_ok: bool,
) -> Option<OwnedEntry> {
    if !response_ok {
        return None;
    }
    let now = current_unix_ms();

    macro_rules! skip_activity {
        ($request_class:literal, $reason:literal) => {{
            tracing::debug!(
                request_class = $request_class,
                request_category = ?req.category(),
                reason = $reason,
                "activity mapper: intentionally skipped request kind"
            );
            return None;
        }};
    }

    let (action, target_kind, target_id, context): (
        &'static str,
        Option<&'static str>,
        Option<String>,
        Option<serde_json::Value>,
    ) = match req {
        // ----- mail mutations via Mutation envelope -----
        Request::Mutation { mutation, .. } => match mutation {
            MutationCommand::Archive { message_ids } => (
                "mail.archive",
                Some("message"),
                message_ids.first().map(|m| m.as_str().clone()),
                Some(serde_json::json!({
                    "count": message_ids.len(),
                    "target_ids": message_ids.iter().map(mxr_core::MessageId::as_str).collect::<Vec<_>>(),
                })),
            ),
            MutationCommand::ReadAndArchive { message_ids } => (
                "mail.archive",
                Some("message"),
                message_ids.first().map(|m| m.as_str().clone()),
                Some(serde_json::json!({
                    "count": message_ids.len(),
                    "read_then_archive": true,
                    "target_ids": message_ids.iter().map(mxr_core::MessageId::as_str).collect::<Vec<_>>(),
                })),
            ),
            MutationCommand::Trash { message_ids } => (
                "mail.trash",
                Some("message"),
                message_ids.first().map(|m| m.as_str().clone()),
                Some(serde_json::json!({
                    "count": message_ids.len(),
                    "target_ids": message_ids.iter().map(mxr_core::MessageId::as_str).collect::<Vec<_>>(),
                })),
            ),
            MutationCommand::Spam { message_ids } => (
                "mail.mark_spam",
                Some("message"),
                message_ids.first().map(|m| m.as_str().clone()),
                Some(serde_json::json!({
                    "count": message_ids.len(),
                    "target_ids": message_ids.iter().map(mxr_core::MessageId::as_str).collect::<Vec<_>>(),
                })),
            ),
            MutationCommand::Star {
                message_ids,
                starred,
            } => (
                if *starred { "mail.star" } else { "mail.unstar" },
                Some("message"),
                message_ids.first().map(|m| m.as_str().clone()),
                Some(serde_json::json!({
                    "count": message_ids.len(),
                    "target_ids": message_ids.iter().map(mxr_core::MessageId::as_str).collect::<Vec<_>>(),
                })),
            ),
            MutationCommand::SetRead { message_ids, read } => (
                if *read { "mail.read" } else { "mail.unread" },
                Some("message"),
                message_ids.first().map(|m| m.as_str().clone()),
                Some(serde_json::json!({
                    "count": message_ids.len(),
                    "target_ids": message_ids.iter().map(mxr_core::MessageId::as_str).collect::<Vec<_>>(),
                })),
            ),
            MutationCommand::ModifyLabels {
                message_ids,
                add,
                remove,
            } => {
                // Pick the dominant verb: if either set is non-empty.
                let action = if !add.is_empty() && remove.is_empty() {
                    "mail.label"
                } else if add.is_empty() && !remove.is_empty() {
                    "mail.unlabel"
                } else {
                    "mail.label" // mixed → keep label as the canonical token
                };
                (
                    action,
                    Some("message"),
                    message_ids.first().map(|m| m.as_str().clone()),
                    Some(serde_json::json!({
                        "count": message_ids.len(),
                        "add": add,
                        "remove": remove,
                        "target_ids": message_ids.iter().map(mxr_core::MessageId::as_str).collect::<Vec<_>>(),
                    })),
                )
            }
            MutationCommand::Move {
                message_ids,
                target_label,
            } => (
                "mail.move",
                Some("message"),
                message_ids.first().map(|m| m.as_str().clone()),
                Some(serde_json::json!({
                    "count": message_ids.len(),
                    "to": target_label,
                    "target_ids": message_ids.iter().map(mxr_core::MessageId::as_str).collect::<Vec<_>>(),
                })),
            ),
            MutationCommand::Route {
                message_ids,
                to_label,
                from_queue_label,
                archive,
                dry_run,
            } => (
                "mail.route",
                Some("message"),
                message_ids.first().map(|m| m.as_str().clone()),
                Some(serde_json::json!({
                    "count": message_ids.len(),
                    "to": to_label,
                    "from_queue": from_queue_label,
                    "archive": archive,
                    "dry_run": dry_run,
                    "target_ids": message_ids.iter().map(mxr_core::MessageId::as_str).collect::<Vec<_>>(),
                })),
            ),
        },

        // ----- low-level SetFlags (TUI/web sometimes calls directly) -----
        Request::SetFlags {
            message_id,
            flags: _,
        } => (
            "mail.read",
            Some("message"),
            Some(message_id.as_str().clone()),
            Some(serde_json::json!({ "via": "set_flags" })),
        ),

        // ----- snooze / unsnooze -----
        Request::Snooze {
            message_id,
            wake_at,
        } => (
            "mail.snooze",
            Some("message"),
            Some(message_id.as_str().clone()),
            Some(serde_json::json!({
                "until": wake_at.timestamp_millis(),
            })),
        ),
        Request::Unsnooze { message_id } => (
            "mail.unsnooze",
            Some("message"),
            Some(message_id.as_str().clone()),
            None,
        ),

        // ----- unsubscribe -----
        Request::Unsubscribe { message_id } => (
            "mail.unsubscribe",
            Some("message"),
            Some(message_id.as_str().clone()),
            None,
        ),
        Request::UnsubscribePurge {
            address,
            dry_run,
            archive_on_no_method,
            ..
        } => (
            "mail.unsubscribe_purge",
            Some("sender"),
            Some(address.clone()),
            Some(serde_json::json!({
                "dry_run": dry_run,
                "archive_on_no_method": archive_on_no_method,
            })),
        ),

        // ----- reply-later flag -----
        Request::SetReplyLater { message_id, flag } => (
            if *flag {
                "thread.flag_reply_later"
            } else {
                "thread.unflag_reply_later"
            },
            Some("message"),
            Some(message_id.as_str().clone()),
            None,
        ),

        // ----- thread reading -----
        Request::GetThread { thread_id } => (
            "thread.open",
            Some("thread"),
            Some(thread_id.as_str().clone()),
            None,
        ),
        Request::SummarizeThread { thread_id, .. } => (
            "thread.summarize",
            Some("thread"),
            Some(thread_id.as_str().clone()),
            None,
        ),

        // ----- drafts -----
        Request::DraftCompose { .. } => ("draft.create", Some("draft"), None, None),
        Request::DraftRefine { .. } => ("draft.update", Some("draft"), None, None),
        Request::SaveDraft { draft } => (
            "draft.save",
            Some("draft"),
            Some(draft.id.as_str().clone()),
            None,
        ),
        Request::UpdateDraft { draft } => (
            "draft.update",
            Some("draft"),
            Some(draft.id.as_str().clone()),
            None,
        ),
        Request::DeleteDraft { draft_id } => (
            "draft.discard",
            Some("draft"),
            Some(draft_id.as_str().clone()),
            None,
        ),
        Request::PrepareReply { message_id, .. } => (
            "mail.reply",
            Some("message"),
            Some(message_id.as_str().clone()),
            None,
        ),
        Request::PrepareForward { message_id, .. } => (
            "mail.forward",
            Some("message"),
            Some(message_id.as_str().clone()),
            None,
        ),

        // ----- send -----
        Request::SendDraft { draft, .. } => (
            "mail.send",
            Some("draft"),
            Some(draft.id.as_str().clone()),
            Some(serde_json::json!({
                "subject": truncate(&draft.subject, SUBJECT_LIMIT),
                "to_count": draft.to.len(),
                "cc_count": draft.cc.len(),
                "bcc_count": draft.bcc.len(),
                "has_attachments": !draft.attachments.is_empty(),
            })),
        ),
        Request::SendStoredDraft { draft_id, .. } => (
            "mail.send",
            Some("draft"),
            Some(draft_id.as_str().clone()),
            None,
        ),
        Request::RespondInvite {
            message_id,
            action,
            dry_run,
        } => (
            "mail.calendar_rsvp",
            Some("message"),
            Some(message_id.as_str().clone()),
            Some(serde_json::json!({
                "action": action.partstat(),
                "dry_run": dry_run,
            })),
        ),
        Request::ScheduleSend { draft_id, send_at } => (
            "mail.send",
            Some("draft"),
            Some(draft_id.as_str().clone()),
            Some(serde_json::json!({
                "scheduled": true,
                "when": send_at.timestamp_millis(),
            })),
        ),
        Request::CancelScheduledSend { draft_id } => (
            "draft.discard",
            Some("draft"),
            Some(draft_id.as_str().clone()),
            Some(serde_json::json!({ "kind": "cancel_scheduled" })),
        ),

        // ----- search -----
        Request::Search { query, mode, .. } => (
            "search.run",
            Some("search"),
            None,
            Some(serde_json::json!({
                "query": truncate(query, QUERY_LIMIT),
                "mode": mode.as_ref().map(|m| format!("{m:?}")),
            })),
        ),
        Request::CreateSavedSearch { name, query, .. } => (
            "search.save",
            Some("search"),
            None,
            Some(serde_json::json!({
                "name": name,
                "query": truncate(query, QUERY_LIMIT),
            })),
        ),
        Request::DeleteSavedSearch { name } => (
            "search.delete",
            Some("search"),
            None,
            Some(serde_json::json!({ "name": name })),
        ),
        Request::UpdateSavedSearch { name, new_name, .. } => (
            "search.rename",
            Some("search"),
            None,
            Some(serde_json::json!({
                "name": name,
                "new_name": new_name,
            })),
        ),
        Request::RunSavedSearch { name, .. } => (
            "saved.open",
            Some("search"),
            None,
            Some(serde_json::json!({ "name": name })),
        ),

        // ----- snippets -----
        Request::SetSnippet { name, .. } => (
            "snippet.create",
            Some("snippet"),
            Some(name.clone()),
            Some(serde_json::json!({ "name": name })),
        ),
        Request::DeleteSnippet { name } => (
            "snippet.delete",
            Some("snippet"),
            Some(name.clone()),
            Some(serde_json::json!({ "name": name })),
        ),

        // ----- signatures (treat like snippets for activity purposes) -----
        Request::SetSignature { .. } => ("snippet.create", Some("signature"), None, None),
        Request::DeleteSignature { .. } => ("snippet.delete", Some("signature"), None, None),

        // ----- screener -----
        Request::SetScreenerDecision {
            sender_email,
            disposition,
            ..
        } => {
            let action = match format!("{disposition:?}").to_lowercase().as_str() {
                "allow" => "screener.allow",
                "block" => "screener.block",
                _ => "screener.snooze",
            };
            (
                action,
                Some("sender"),
                Some(sender_email.clone()),
                Some(serde_json::json!({ "sender_email": sender_email })),
            )
        }
        Request::ClearScreenerDecision { sender_email, .. } => (
            "screener.allow",
            Some("sender"),
            Some(sender_email.clone()),
            Some(serde_json::json!({ "cleared": true })),
        ),

        // ----- reminders -----
        Request::SetAutoReminder {
            sent_message_id,
            remind_at,
        } => (
            "reminder.set",
            Some("message"),
            Some(sent_message_id.as_str().clone()),
            Some(serde_json::json!({ "when": remind_at.timestamp_millis() })),
        ),
        Request::CancelAutoReminder { sent_message_id } => (
            "reminder.clear",
            Some("message"),
            Some(sent_message_id.as_str().clone()),
            None,
        ),

        // ----- labels (CreateLabel / DeleteLabel / RenameLabel) -----
        Request::CreateLabel { name, .. } => (
            "mail.label",
            Some("label"),
            Some(name.clone()),
            Some(serde_json::json!({ "label": name, "kind": "create" })),
        ),
        Request::DeleteLabel { name, .. } => (
            "mail.unlabel",
            Some("label"),
            Some(name.clone()),
            Some(serde_json::json!({ "label": name, "kind": "delete" })),
        ),
        Request::RenameLabel { old, new, .. } => (
            "mail.label",
            Some("label"),
            Some(new.clone()),
            Some(serde_json::json!({ "from": old, "to": new, "kind": "rename" })),
        ),

        // ----- rules -----
        Request::UpsertRule { rule } => (
            "rule.update",
            Some("rule"),
            rule.get("name").and_then(|v| v.as_str()).map(str::to_owned),
            Some(serde_json::json!({
                "name": rule.get("name").and_then(|v| v.as_str()),
            })),
        ),
        Request::UpsertRuleForm {
            existing_rule,
            name,
            ..
        } => {
            let action = if existing_rule.is_some() {
                "rule.update"
            } else {
                "rule.create"
            };
            (
                action,
                Some("rule"),
                Some(name.clone()),
                Some(serde_json::json!({ "name": name })),
            )
        }
        Request::DeleteRule { rule } => (
            "rule.delete",
            Some("rule"),
            Some(rule.clone()),
            Some(serde_json::json!({ "name": rule })),
        ),
        Request::DryRunRules { rule, .. } => (
            "rule.test",
            Some("rule"),
            rule.clone(),
            Some(serde_json::json!({ "name": rule })),
        ),

        // ----- accounts -----
        Request::UpsertAccountConfig { account } => (
            "account.add",
            Some("account"),
            Some(account.key.clone()),
            Some(serde_json::json!({
                "key": account.key,
                "email": account.email,
            })),
        ),
        Request::RemoveAccountConfig { key, .. } => (
            "account.remove",
            Some("account"),
            Some(key.clone()),
            Some(serde_json::json!({ "key": key })),
        ),
        Request::CompleteAuthSession { .. } => ("account.signin", Some("account"), None, None),
        Request::SyncNow {
            account_id: account,
        } => (
            "account.sync",
            Some("account"),
            account.as_ref().map(|a| a.as_str().clone()),
            None,
        ),
        Request::SetDefaultAccount { key } => (
            "account.rename",
            Some("account"),
            Some(key.clone()),
            Some(serde_json::json!({ "key": key, "kind": "set_default" })),
        ),

        // ----- undo (user-intent: roll back the last action) -----
        Request::UndoMutation { mutation_id } => (
            "mail.archive",
            Some("mutation"),
            Some(mutation_id.clone()),
            Some(serde_json::json!({ "kind": "undo", "mutation_id": mutation_id })),
        ),

        // ----- export (treated as activity.exported synthesized marker;
        // these higher-level routes emit content) -----
        Request::ExportThread { thread_id, .. } => (
            "thread.summarize",
            Some("thread"),
            Some(thread_id.as_str().clone()),
            Some(serde_json::json!({ "kind": "export" })),
        ),
        Request::ExportSearch { query, .. } => (
            "search.run",
            Some("search"),
            None,
            Some(serde_json::json!({
                "kind": "export",
                "query": truncate(query, QUERY_LIMIT),
            })),
        ),

        // ----- everything else: intentionally skipped. No wildcard: adding a
        // Request variant breaks daemon unit tests until the activity policy
        // is classified as logged above or intentionally skipped here. -----
        Request::ListEnvelopes { .. }
        | Request::ListEnvelopesByIds { .. }
        | Request::GetEnvelope { .. }
        | Request::GetBody { .. }
        | Request::GetInvite { .. }
        | Request::ListInvites { .. }
        | Request::GetHtmlImageAssets { .. }
        | Request::ListBodies { .. }
        | Request::ListThreads { .. }
        | Request::ListLabels { .. }
        | Request::ListAccounts
        | Request::ListAccountsConfig
        | Request::ListRules
        | Request::GetRule { .. }
        | Request::GetRuleForm { .. }
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
        | Request::GetSyncStatus { .. }
        | Request::Count { .. }
        | Request::SearchAggregation { .. }
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
        | Request::GetRelationshipProfile { .. }
        | Request::ListCommitments { .. }
        | Request::GetUserVoice { .. }
        | Request::ListScreenerQueue { .. }
        | Request::ListScreenerDecisions { .. }
        | Request::ListCadenceWatch { .. }
        | Request::ListCadenceDrift { .. }
        | Request::ListDecisionLog { .. }
        | Request::GetDecision { .. }
        | Request::ListOwedReplies { .. }
        | Request::ListDrafts
        | Request::ListOrphanedDrafts
        | Request::GetDraft { .. } => {
            skip_activity!(
                "read_list_status",
                "read/list/status requests are not activity entries"
            );
        }
        Request::PrepareInviteResponse { .. }
        | Request::CheckDraftSafety { .. }
        | Request::ResolveSendFrom { .. }
        | Request::ExtractDraftCommitments { .. }
        | Request::TriageSearch { .. }
        | Request::ExplainEntity { .. }
        | Request::FindExpert { .. }
        | Request::SuggestCollaborators { .. }
        | Request::GetThreadBriefing { .. }
        | Request::GetRecipientBriefing { .. }
        | Request::SendTimeRecommendation { .. }
        | Request::ArchiveAsk { .. }
        | Request::HumanizerScore { .. }
        | Request::HumanizerRewrite { .. } => {
            skip_activity!(
                "preview_analysis",
                "preview/analysis-only requests are not activity entries"
            );
        }
        Request::BackfillCalendarInvites { .. }
        | Request::RefreshContacts
        | Request::RebuildAnalytics
        | Request::RecomputeLinkCounts
        | Request::ReindexSemantic
        | Request::BackfillSemantic
        | Request::RebuildRelationshipProfile { .. }
        | Request::RebuildUserVoice { .. }
        | Request::RebuildDecisionLog { .. }
        | Request::ResetOrphanedDraft { .. } => {
            skip_activity!(
                "maintenance_rebuild",
                "maintenance/rebuild requests are not activity entries"
            );
        }
        Request::AuthorizeAccountConfig { .. }
        | Request::StartAuthSession { .. }
        | Request::GetAuthSession { .. }
        | Request::CancelAuthSession { .. }
        | Request::TestAccountConfig { .. }
        | Request::DisableAccountConfig { .. }
        | Request::RepairAccountConfig { .. }
        | Request::AddAccountAddress { .. }
        | Request::RemoveAccountAddress { .. }
        | Request::SetPrimaryAccountAddress { .. }
        | Request::UpdateLlmConfig { .. }
        | Request::UpdateNotificationChimes { .. }
        | Request::PreviewNotificationChime { .. }
        | Request::EnableSemantic { .. }
        | Request::InstallSemanticProfile { .. }
        | Request::UseSemanticProfile { .. } => {
            skip_activity!(
                "config_auth_helper",
                "config/auth helper requests are not activity entries"
            );
        }
        Request::MarkInviteAnswered { .. } => {
            skip_activity!(
                "invite_bookkeeping",
                "post-send invite bookkeeping is not direct user activity"
            );
        }
        Request::DownloadAttachment { .. } | Request::OpenAttachment { .. } => {
            skip_activity!(
                "attachment",
                "attachment materialization/opening is not in the activity catalog"
            );
        }
        Request::StartMutationJob { .. } => {
            skip_activity!(
                "mutation_job_enqueue",
                "mutation job enqueue is currently outside request-mapper capture"
            );
        }
        Request::ListJobs | Request::GetJob { .. } => {
            skip_activity!("job_polling", "background job polling is not activity");
        }
        Request::ListDeliveries { .. }
        | Request::GetDelivery { .. }
        | Request::ResolveDelivery { .. }
        | Request::DismissDelivery { .. }
        | Request::ScanDeliveries { .. } => {
            skip_activity!(
                "delivery_tracker",
                "delivery tracker requests are not in the activity catalog"
            );
        }
        Request::SetSignatureDefault { .. } | Request::ClearSignatureDefault { .. } => {
            skip_activity!(
                "signature_default_config",
                "signature default config is not in the activity catalog"
            );
        }
        Request::ResolveCommitment { .. }
        | Request::WatchCadence { .. }
        | Request::UnwatchCadence { .. } => {
            skip_activity!(
                "relationship_workflow",
                "relationship workflow requests are not in the activity catalog"
            );
        }
        Request::SaveDraftToServer { .. } => {
            skip_activity!(
                "server_draft_persistence",
                "server draft persistence is not in the activity catalog"
            );
        }
        Request::ListEvents { .. }
        | Request::GetLogs { .. }
        | Request::ListEventCategories
        | Request::CountEvents { .. }
        | Request::GetDoctorReport
        | Request::GenerateBugReport { .. } => {
            skip_activity!("diagnostics", "diagnostics requests are not activity");
        }
        Request::GetStatus | Request::Ping | Request::Shutdown | Request::Authenticate { .. } => {
            skip_activity!(
                "daemon_lifecycle",
                "daemon lifecycle requests are not activity"
            );
        }
        Request::ListActivity { .. }
        | Request::CountActivity { .. }
        | Request::ActivityStats { .. }
        | Request::ExportActivity { .. }
        | Request::RedactActivity { .. }
        | Request::PruneActivity { .. }
        | Request::PauseActivity { .. }
        | Request::ResumeActivity
        | Request::ListSavedActivityFilters
        | Request::GetSavedActivityFilter { .. }
        | Request::UpsertSavedActivityFilter { .. }
        | Request::DeleteSavedActivityFilter { .. } => {
            skip_activity!(
                "activity_self_management",
                "activity-log self-management stays outside request-mapper capture"
            );
        }
    };

    Some(OwnedEntry {
        ts: now,
        account_id: account_id.map(str::to_owned),
        source,
        action: action.to_owned(),
        target_kind: target_kind.map(str::to_owned),
        target_id,
        tier: tier_for(action),
        context,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Write};
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct CapturedLogs(Arc<Mutex<Vec<u8>>>);

    impl Write for CapturedLogs {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn ok_map(req: &Request) -> Option<OwnedEntry> {
        map_request(req, ClientKind::Tui, None, true)
    }

    fn secret_bearing_account_config() -> mxr_protocol::AccountConfigData {
        mxr_protocol::AccountConfigData {
            key: "secret-account".into(),
            name: "Secret Account".into(),
            email: "secret@example.com".into(),
            enabled: true,
            sync: Some(mxr_protocol::AccountSyncConfigData::Gmail {
                credential_source: mxr_protocol::GmailCredentialSourceData::Custom,
                client_id: "CLIENT_ID_FOR_DEBUG_SHAPE".into(),
                client_secret: Some("CLIENT_SECRET_DO_NOT_LOG".into()),
                token_ref: "TOKEN_REF_FOR_DEBUG_SHAPE".into(),
            }),
            send: Some(mxr_protocol::AccountSendConfigData::Smtp {
                host: "smtp.example.com".into(),
                port: 465,
                username: "smtp-user".into(),
                password_ref: "SMTP_PASSWORD_REF".into(),
                password: Some("INLINE_PASSWORD_DO_NOT_LOG".into()),
                auth_required: true,
                use_tls: true,
            }),
            is_default: false,
        }
    }

    #[test]
    fn skipped_request_logging_omits_secret_bearing_payloads() {
        let req = Request::AuthorizeAccountConfig {
            account: secret_bearing_account_config(),
            reauthorize: true,
        };
        let raw_debug = format!("{req:?}");
        assert!(raw_debug.contains("CLIENT_SECRET_DO_NOT_LOG"));
        assert!(raw_debug.contains("INLINE_PASSWORD_DO_NOT_LOG"));

        let logs = Arc::new(Mutex::new(Vec::new()));
        let writer_logs = logs.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_ansi(false)
            .without_time()
            .with_writer(move || CapturedLogs(writer_logs.clone()))
            .finish();

        tracing::subscriber::with_default(subscriber, || {
            assert!(ok_map(&req).is_none());
        });

        let captured = String::from_utf8(logs.lock().unwrap().clone()).unwrap();
        assert!(captured.contains("activity mapper: intentionally skipped request kind"));
        assert!(captured.contains("request_class=\"config_auth_helper\""));
        assert!(captured.contains("request_category=MxrPlatform"));
        assert!(!captured.contains("CLIENT_SECRET_DO_NOT_LOG"));
        assert!(!captured.contains("INLINE_PASSWORD_DO_NOT_LOG"));
        assert!(!captured.contains("CLIENT_ID_FOR_DEBUG_SHAPE"));
        assert!(!captured.contains("TOKEN_REF_FOR_DEBUG_SHAPE"));
        assert!(!captured.contains("SMTP_PASSWORD_REF"));
    }

    #[test]
    fn activity_policy_matches_runtime_for_representative_requests() {
        let wake_at = chrono::DateTime::<chrono::Utc>::from_timestamp(1_715_592_090, 0).unwrap();
        let logged = [
            Request::Search {
                query: "invoice 2026".into(),
                limit: 50,
                offset: 0,
                account_id: None,
                mode: None,
                sort: None,
                explain: false,
            },
            Request::Mutation {
                mutation: MutationCommand::Archive {
                    message_ids: vec![mxr_core::MessageId::new()],
                },
                client_correlation_id: None,
            },
            Request::Snooze {
                message_id: mxr_core::MessageId::new(),
                wake_at,
            },
        ];
        let skipped = [
            Request::Ping,
            Request::ListEnvelopes {
                label_id: None,
                account_id: None,
                limit: 50,
                offset: 0,
            },
            Request::StartMutationJob {
                mutation: MutationCommand::Archive {
                    message_ids: vec![mxr_core::MessageId::new()],
                },
                client_correlation_id: None,
            },
        ];

        for req in logged {
            assert!(
                ok_map(&req).is_some(),
                "expected logged request to map to activity: {req:?}"
            );
        }
        for req in skipped {
            assert!(
                ok_map(&req).is_none(),
                "expected skipped request to map to no activity: {req:?}"
            );
        }
    }

    #[test]
    fn ping_produces_no_activity() {
        assert!(ok_map(&Request::Ping).is_none());
        assert!(ok_map(&Request::GetStatus).is_none());
    }

    #[test]
    fn failed_response_never_records() {
        let req = Request::Ping;
        assert!(map_request(&req, ClientKind::Tui, None, false).is_none());
    }

    #[test]
    fn search_run_carries_query_and_mode() {
        let req = Request::Search {
            query: "invoice 2026".into(),
            limit: 50,
            offset: 0,
            account_id: None,
            mode: None,
            sort: None,
            explain: false,
        };
        let entry = ok_map(&req).unwrap();
        assert_eq!(entry.action, "search.run");
        assert_eq!(entry.target_kind.as_deref(), Some("search"));
        let ctx = entry.context.unwrap();
        assert_eq!(ctx["query"], "invoice 2026");
    }

    #[test]
    fn list_envelopes_is_a_getter_and_does_not_log() {
        let req = Request::ListEnvelopes {
            label_id: None,
            account_id: None,
            limit: 50,
            offset: 0,
        };
        assert!(ok_map(&req).is_none());
    }

    #[test]
    fn snooze_carries_wake_at_in_context() {
        let wake_at = chrono::DateTime::<chrono::Utc>::from_timestamp(1_715_592_090, 0).unwrap();
        let mid = mxr_core::MessageId::new();
        let req = Request::Snooze {
            message_id: mid.clone(),
            wake_at,
        };
        let entry = ok_map(&req).unwrap();
        assert_eq!(entry.action, "mail.snooze");
        assert_eq!(entry.target_kind.as_deref(), Some("message"));
        assert_eq!(entry.target_id.as_deref(), Some(mid.as_str().as_str()));
        let ctx = entry.context.unwrap();
        assert_eq!(ctx["until"], 1_715_592_090 * 1000_i64);
    }

    #[test]
    fn reply_later_flag_toggles_between_paired_actions() {
        let mid = mxr_core::MessageId::new();
        let req_set = Request::SetReplyLater {
            message_id: mid.clone(),
            flag: true,
        };
        let req_clear = Request::SetReplyLater {
            message_id: mid,
            flag: false,
        };
        assert_eq!(ok_map(&req_set).unwrap().action, "thread.flag_reply_later");
        assert_eq!(
            ok_map(&req_clear).unwrap().action,
            "thread.unflag_reply_later"
        );
    }

    #[test]
    fn invite_response_logs_calendar_rsvp_action() {
        let mid = mxr_core::MessageId::new();
        let req = Request::RespondInvite {
            message_id: mid.clone(),
            action: mxr_protocol::CalendarInviteActionData::Accept,
            dry_run: false,
        };
        let entry = ok_map(&req).unwrap();

        assert_eq!(entry.action, "mail.calendar_rsvp");
        assert_eq!(entry.target_kind.as_deref(), Some("message"));
        assert_eq!(entry.target_id.as_deref(), Some(mid.as_str().as_str()));
        let ctx = entry.context.unwrap();
        assert_eq!(ctx["action"], "ACCEPTED");
        assert_eq!(ctx["dry_run"], false);
    }

    #[test]
    fn mutation_archive_includes_target_id_and_count() {
        let m1 = mxr_core::MessageId::new();
        let ids = vec![
            m1.clone(),
            mxr_core::MessageId::new(),
            mxr_core::MessageId::new(),
        ];
        let req = Request::Mutation {
            mutation: MutationCommand::Archive { message_ids: ids },
            client_correlation_id: None,
        };
        let entry = ok_map(&req).unwrap();
        assert_eq!(entry.action, "mail.archive");
        assert_eq!(entry.target_id.as_deref(), Some(m1.as_str().as_str()));
        let ctx = entry.context.unwrap();
        assert_eq!(ctx["count"], 3);
        assert!(ctx["target_ids"].is_array());
        assert_eq!(ctx["target_ids"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn star_dispatches_to_paired_actions() {
        let make = |starred: bool| Request::Mutation {
            mutation: MutationCommand::Star {
                message_ids: vec![mxr_core::MessageId::new()],
                starred,
            },
            client_correlation_id: None,
        };
        assert_eq!(ok_map(&make(true)).unwrap().action, "mail.star");
        assert_eq!(ok_map(&make(false)).unwrap().action, "mail.unstar");
    }

    #[test]
    fn source_propagates_through_entry() {
        let req = Request::Search {
            query: "x".into(),
            limit: 10,
            offset: 0,
            account_id: None,
            mode: None,
            sort: None,
            explain: false,
        };
        let entry = map_request(&req, ClientKind::Web, None, true).unwrap();
        assert!(matches!(entry.source, ClientKind::Web));
    }

    #[test]
    fn account_id_propagates_when_provided() {
        let req = Request::Search {
            query: "x".into(),
            limit: 10,
            offset: 0,
            account_id: None,
            mode: None,
            sort: None,
            explain: false,
        };
        let entry = map_request(&req, ClientKind::Cli, Some("acct_1"), true).unwrap();
        assert_eq!(entry.account_id.as_deref(), Some("acct_1"));
    }

    #[test]
    fn entry_tier_matches_classifier() {
        let req = Request::Search {
            query: "x".into(),
            limit: 10,
            offset: 0,
            account_id: None,
            mode: None,
            sort: None,
            explain: false,
        };
        let entry = ok_map(&req).unwrap();
        assert_eq!(entry.tier, mxr_store::Tier::Standard);

        let req2 = Request::Mutation {
            mutation: MutationCommand::Archive {
                message_ids: vec![mxr_core::MessageId::new()],
            },
            client_correlation_id: None,
        };
        let entry2 = ok_map(&req2).unwrap();
        assert_eq!(entry2.tier, mxr_store::Tier::Important);
    }

    #[test]
    fn long_search_query_truncates_with_ellipsis() {
        let long_query = "a".repeat(1000);
        let req = Request::Search {
            query: long_query,
            limit: 10,
            offset: 0,
            account_id: None,
            mode: None,
            sort: None,
            explain: false,
        };
        let entry = ok_map(&req).unwrap();
        let q = entry.context.unwrap()["query"].as_str().unwrap().to_owned();
        assert!(q.ends_with('…'));
        assert!(q.chars().count() <= QUERY_LIMIT + 1);
    }
}
