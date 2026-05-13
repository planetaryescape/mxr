use super::{
    apply_snooze, build_reply_references, reconcile_label_mutation, render_message_context,
    restore_snoozed_message, HandlerResult,
};
use crate::state::AppState;
use lettre::message::Mailbox;
use mxr_core::types::{
    Address, Draft, DraftSafetyIssue, DraftSafetyIssueCode, DraftSafetyReport, DraftSafetySeverity,
    DraftStatus, Envelope, MessageBody, MessageDirection, MessageFlags, MessageMetadata,
    SendReceipt, UnsubscribeMethod,
};
use mxr_protocol::{
    AccountMutationResultData, DaemonEvent, DraftSafetyContextData, DraftSafetyModeData,
    ForwardContext, IpcMessage, IpcPayload, MutationCommand, MutationResultData, ReplyContext,
    ResponseData,
};
use mxr_store::{EventLogRefs, UndoEntry, UndoEntrySnapshot, UndoableMutationKind};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// How long after the mutation the user can undo it. Matches the plan
/// (~60s) and pairs with `tick_connection_state`-style UI affordances on
/// the TUI side.
const UNDO_WINDOW_SECS: i64 = 60;

pub(crate) fn check_draft_safety(draft: &Draft) -> DraftSafetyReport {
    let mut issues = Vec::new();
    let recipient_count = draft.to.len() + draft.cc.len() + draft.bcc.len();

    if recipient_count == 0 {
        issues.push(DraftSafetyIssue::new(
            DraftSafetyIssueCode::NoRecipients,
            DraftSafetySeverity::Blocker,
            "draft has no recipients",
        ));
    }

    for address in draft.to.iter().chain(&draft.cc).chain(&draft.bcc) {
        if address.email.trim().parse::<Mailbox>().is_err() {
            issues.push(DraftSafetyIssue::new(
                DraftSafetyIssueCode::InvalidRecipient,
                DraftSafetySeverity::Blocker,
                format!("invalid recipient address: {}", address.email),
            ));
        }
    }

    DraftSafetyReport::from_issues(issues)
}

async fn check_draft_safety_with_context(
    state: &AppState,
    draft: &Draft,
) -> Result<DraftSafetyReport, String> {
    let mut report = check_draft_safety(draft);
    if draft.intent != mxr_core::DraftIntent::ReplyAll {
        return Ok(report);
    }

    let Some(reply_headers) = draft.reply_headers.as_ref() else {
        return Ok(report);
    };
    let mut parents = state
        .store
        .list_envelopes_by_message_id_header(&draft.account_id, &reply_headers.in_reply_to)
        .await
        .map_err(|e| e.to_string())?;
    let Some(parent) = parents.pop() else {
        return Ok(report);
    };

    let mut self_addresses = HashSet::new();
    if let Some(account) = state
        .store
        .get_account(&draft.account_id)
        .await
        .map_err(|e| e.to_string())?
    {
        self_addresses.insert(account.email.to_ascii_lowercase());
    }
    for address in state
        .store
        .list_account_addresses(&draft.account_id)
        .await
        .map_err(|e| e.to_string())?
    {
        self_addresses.insert(address.email.to_ascii_lowercase());
    }

    let actual_recipients = draft
        .to
        .iter()
        .chain(&draft.cc)
        .chain(&draft.bcc)
        .map(|address| address.email.to_ascii_lowercase())
        .collect::<HashSet<_>>();
    let reply_target = parent.from.email.to_ascii_lowercase();

    let mut additions = Vec::new();
    for expected in parent.to.iter().chain(&parent.cc) {
        let email = expected.email.to_ascii_lowercase();
        if email.is_empty() || self_addresses.contains(&email) || email == reply_target {
            continue;
        }
        if !actual_recipients.contains(&email) {
            additions.push(DraftSafetyIssue::new(
                DraftSafetyIssueCode::MissingReplyAllRecipient,
                DraftSafetySeverity::Blocker,
                format!("reply-all is missing recipient: {}", expected.email),
            ));
        }
    }
    report.extend(additions);
    Ok(report)
}

async fn enforce_draft_safety_with_override(
    state: &AppState,
    draft: &Draft,
    override_token: Option<&str>,
) -> Result<(), String> {
    let report = run_safety_pipeline(
        state,
        draft,
        &DraftSafetyContextData {
            mode: DraftSafetyModeData::Send,
            reply_all: matches!(draft.intent, mxr_core::types::DraftIntent::ReplyAll),
            original_message_id: None,
            thread_id: None,
            allow_llm: false,
        },
    )
    .await?;
    // Always persist the audit row, including the Safe path. Helps
    // `mxr doctor` and post-mortem debugging.
    let _ = state
        .store
        .record_safety_run(&draft.account_id, Some(&draft.id), &report)
        .await
        .map_err(|e| {
            tracing::warn!("failed to record safety audit: {e}");
            ""
        });

    if report.allowed {
        return Ok(());
    }

    // Blocker present. If an override token was provided AND it covers
    // the actually-present blocker kinds, consume it and let the send
    // proceed. Otherwise refuse.
    let blocker_kinds: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.severity == DraftSafetySeverity::Blocker)
        .map(|i| i.code)
        .collect();

    if let Some(token) = override_token {
        match state.store.consume_safety_override(token).await {
            Ok(Some(allowed_kinds)) => {
                let unauthorized: Vec<_> = blocker_kinds
                    .iter()
                    .filter(|k| !allowed_kinds.contains(k))
                    .collect();
                if unauthorized.is_empty() {
                    tracing::info!(
                        draft_id = %draft.id,
                        kinds = ?allowed_kinds,
                        "safety override token consumed"
                    );
                    return Ok(());
                } else {
                    return Err(format!(
                        "override token does not cover blocker(s): {:?}",
                        unauthorized
                    ));
                }
            }
            Ok(None) => return Err("override token unknown or already used".to_string()),
            Err(e) => return Err(format!("failed to consume override token: {e}")),
        }
    }

    let messages = report
        .issues
        .iter()
        .filter(|issue| issue.severity == DraftSafetySeverity::Blocker)
        .map(|issue| issue.message.as_str())
        .collect::<Vec<_>>()
        .join("; ");
    Err(format!("draft safety blocked send: {messages}"))
}

/// Build a `mxr_safety::SafetyContext` from store data (self addresses,
/// reply-all flag from context). Slice 1.2 keeps the contact/style
/// loaders empty; Slice 1.3 wires them up.
async fn build_safety_context(
    state: &AppState,
    draft: &Draft,
    context: &DraftSafetyContextData,
) -> Result<mxr_safety::SafetyContext, String> {
    let mut self_addresses = Vec::new();
    if let Some(account) = state
        .store
        .get_account(&draft.account_id)
        .await
        .map_err(|e| e.to_string())?
    {
        self_addresses.push(account.email.to_ascii_lowercase());
    }
    for address in state
        .store
        .list_account_addresses(&draft.account_id)
        .await
        .map_err(|e| e.to_string())?
    {
        self_addresses.push(address.email.to_ascii_lowercase());
    }
    Ok(mxr_safety::SafetyContext {
        mode_reply_all: context.reply_all
            || matches!(draft.intent, mxr_core::types::DraftIntent::ReplyAll),
        self_addresses,
        known_contacts: Vec::new(),
        contact_styles: Vec::new(),
        thread_display_names: Vec::new(),
    })
}

/// Run the full safety pipeline: existing daemon checks (recipients,
/// reply-all parent diff) + new `mxr-safety` deterministic checks.
async fn run_safety_pipeline(
    state: &AppState,
    draft: &Draft,
    context: &DraftSafetyContextData,
) -> Result<DraftSafetyReport, String> {
    let mut report = check_draft_safety_with_context(state, draft).await?;
    let safety_ctx = build_safety_context(state, draft, context).await?;
    let safety_cfg = mxr_safety::SafetyConfig::default();
    let extra = mxr_safety::check_draft_deterministic(draft, &safety_ctx, &safety_cfg);
    report.extend(extra.issues);
    Ok(report)
}

/// IPC handler: `Request::CheckDraftSafety`. When the verdict is
/// Blocked, mints a single-use override token and stamps it onto each
/// Blocker issue, so callers (CLI, TUI) can surface a copy-pasteable
/// `--override-safety <token>` value next to the user-facing reason.
pub(crate) async fn check_draft_safety_request(
    state: &Arc<AppState>,
    draft: &Draft,
    context: &DraftSafetyContextData,
) -> HandlerResult {
    let mut report = run_safety_pipeline(state, draft, context).await?;

    // Always audit a check run.
    let _ = state
        .store
        .record_safety_run(&draft.account_id, Some(&draft.id), &report)
        .await
        .map_err(|e| {
            tracing::warn!("failed to record safety audit: {e}");
            ""
        });

    if matches!(report.verdict, mxr_core::DraftSafetyVerdict::Blocked) {
        let blocker_kinds: Vec<_> = report
            .issues
            .iter()
            .filter(|i| i.severity == DraftSafetySeverity::Blocker)
            .map(|i| i.code)
            .collect();
        if !blocker_kinds.is_empty() {
            match state
                .store
                .mint_safety_override(Some(&draft.id), &blocker_kinds)
                .await
            {
                Ok(token) => {
                    for issue in report.issues.iter_mut() {
                        if issue.severity == DraftSafetySeverity::Blocker {
                            issue.override_token = Some(token.clone());
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("failed to mint override token: {e}");
                }
            }
        }
    }

    Ok(ResponseData::DraftSafetyReportResponse { report })
}

async fn log_mutation(
    state: &AppState,
    envelope: &Envelope,
    summary: String,
    details: Option<String>,
) -> Result<(), String> {
    let message_id = envelope.id.as_str();
    state
        .store
        .insert_event_refs(
            "info",
            "mutation",
            &summary,
            EventLogRefs {
                account_id: Some(&envelope.account_id),
                message_id: Some(message_id.as_str()),
                rule_id: None,
            },
            details.as_deref(),
        )
        .await
        .map_err(|e| e.to_string())
}

fn quoted_subject(subject: &str) -> String {
    if subject.is_empty() {
        "message".to_string()
    } else {
        format!("\"{subject}\"")
    }
}

fn emit_mutation_reconciliation_failed_if_needed(
    state: &AppState,
    client_correlation_id: Option<&str>,
    result: &MutationResultData,
) {
    let Some(cid) = client_correlation_id.filter(|s| !s.is_empty()) else {
        return;
    };
    if result.requested == 0 || result.succeeded >= result.requested {
        return;
    }
    let errors: Vec<&str> = result
        .accounts
        .iter()
        .filter_map(|account| account.error.as_deref())
        .collect();
    let header = format!(
        "mutation incomplete: {} succeeded, {} skipped",
        result.succeeded, result.skipped
    );
    let error_summary = if errors.is_empty() {
        header
    } else {
        format!("{header}: {}", errors.join("; "))
    };
    let event = IpcMessage {
        id: 0,
        payload: IpcPayload::Event(DaemonEvent::MutationReconciliationFailed {
            client_correlation_id: cid.to_string(),
            error_summary,
        }),
    };
    let _ = state.event_tx.send(event);
}

pub(super) async fn mutation(
    state: &AppState,
    cmd: &MutationCommand,
    client_correlation_id: Option<&str>,
) -> HandlerResult {
    let message_ids = mutation_message_ids(cmd);
    let undoable_kind = undoable_kind(cmd);
    let mut grouped: HashMap<mxr_core::AccountId, Vec<Envelope>> = HashMap::new();
    for message_id in message_ids {
        let envelope = state
            .store
            .get_envelope(message_id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("Message not found: {message_id}"))?;
        grouped
            .entry(envelope.account_id.clone())
            .or_default()
            .push(envelope);
    }

    // Snapshot the prior state of every envelope so that undoable
    // mutations can be reversed. Captured before `apply_mutation_to_envelope`
    // touches the store.
    let mut succeeded_snapshots: Vec<UndoEntrySnapshot> = Vec::new();

    let mut accounts = Vec::new();
    for (account_id, envelopes) in grouped {
        let account_name = state
            .store
            .get_account(&account_id)
            .await
            .ok()
            .flatten()
            .map(|account| account.name)
            .unwrap_or_else(|| account_id.to_string());

        let mut account_result = AccountMutationResultData {
            account_id: account_id.clone(),
            account_name,
            succeeded: 0,
            skipped: 0,
            failed: 0,
            error: None,
        };

        let provider = match state.get_provider(Some(&account_id)) {
            Ok(provider) => provider,
            Err(error) => {
                account_result.skipped = envelopes.len() as u32;
                account_result.error = Some(format!("account unavailable: {error}"));
                accounts.push(account_result);
                continue;
            }
        };

        for (index, envelope) in envelopes.iter().enumerate() {
            // Capture the prior state BEFORE the mutation runs so undo
            // can restore exactly what the user had. Cheap clone — flags
            // are u32 and label IDs are short strings.
            let snapshot = if undoable_kind.is_some() {
                Some(UndoEntrySnapshot {
                    message_id: envelope.id.clone(),
                    account_id: envelope.account_id.clone(),
                    provider_id: envelope.provider_id.clone(),
                    prior_flags_bits: envelope.flags.bits(),
                    prior_label_provider_ids: envelope.label_provider_ids.clone(),
                })
            } else {
                None
            };

            match apply_mutation_to_envelope(state, provider.as_ref(), cmd, envelope).await {
                Ok(()) => {
                    account_result.succeeded += 1;
                    if let Some(snapshot) = snapshot {
                        succeeded_snapshots.push(snapshot);
                    }
                    let (summary, details) = mutation_log_entry(cmd, envelope);
                    if let Err(error) = log_mutation(state, envelope, summary, details).await {
                        tracing::warn!(%error, "failed to record mutation event");
                    }
                }
                Err(error) => {
                    account_result.skipped += (envelopes.len() - index) as u32;
                    account_result.error = Some(error);
                    break;
                }
            }
        }

        accounts.push(account_result);
    }

    accounts.sort_by(|left, right| {
        left.account_name
            .to_lowercase()
            .cmp(&right.account_name.to_lowercase())
            .then_with(|| left.account_id.as_str().cmp(&right.account_id.as_str()))
    });
    let succeeded = accounts.iter().map(|account| account.succeeded).sum();
    let skipped = accounts.iter().map(|account| account.skipped).sum();
    let failed = accounts.iter().map(|account| account.failed).sum();

    let mutation_id = match (undoable_kind, succeeded_snapshots.is_empty()) {
        (Some(kind), false) => {
            let id = uuid::Uuid::now_v7().to_string();
            let now = chrono::Utc::now().timestamp();
            let entry = UndoEntry {
                mutation_id: id.clone(),
                kind,
                snapshots: succeeded_snapshots,
                applied_at: now,
                expires_at: now + UNDO_WINDOW_SECS,
            };
            if let Err(error) = state.store.write_undo_entry(&entry).await {
                // Non-fatal: the mutation already succeeded; the user
                // just loses the undo affordance.
                tracing::warn!(%error, "failed to write undo entry");
                None
            } else {
                Some(id)
            }
        }
        _ => None,
    };

    let result = MutationResultData {
        requested: message_ids.len() as u32,
        succeeded,
        skipped,
        failed,
        accounts,
        mutation_id,
    };

    emit_mutation_reconciliation_failed_if_needed(state, client_correlation_id, &result);

    Ok(ResponseData::MutationResult { result })
}

/// Map a `MutationCommand` to the `UndoableMutationKind` used to drive
/// the reverse op, or `None` if the mutation isn't reversible (Star /
/// ModifyLabels / Move — the user already has full control there).
fn undoable_kind(cmd: &MutationCommand) -> Option<UndoableMutationKind> {
    match cmd {
        MutationCommand::Archive { .. } => Some(UndoableMutationKind::Archive),
        MutationCommand::Trash { .. } => Some(UndoableMutationKind::Trash),
        MutationCommand::Spam { .. } => Some(UndoableMutationKind::Spam),
        MutationCommand::SetRead { .. } => Some(UndoableMutationKind::SetRead),
        MutationCommand::ReadAndArchive { .. } => Some(UndoableMutationKind::ReadAndArchive),
        MutationCommand::Star { .. }
        | MutationCommand::ModifyLabels { .. }
        | MutationCommand::Move { .. } => None,
    }
}

/// Reverse a recent undoable mutation by id.
///
/// Restores both local state (label memberships and read flag) and
/// provider state (via `modify_labels`). When the upstream message
/// has been hard-deleted (e.g. IMAP EXPUNGE) the provider call fails
/// and we surface that as an "irreversible" error rather than leaving
/// the local store and provider out of sync.
pub(super) async fn undo_mutation(state: &AppState, mutation_id: &str) -> HandlerResult {
    let entry = state
        .store
        .read_undo_entry(mutation_id)
        .await
        .map_err(|e| e.to_string())?;
    let Some(entry) = entry else {
        return Err(format!(
            "undo: mutation `{mutation_id}` not found (expired or already undone)"
        ));
    };
    let now = chrono::Utc::now().timestamp();
    if entry.expires_at <= now {
        let _ = state.store.delete_undo_entry(&entry.mutation_id).await;
        return Err(format!("undo: window expired for mutation `{mutation_id}`"));
    }

    // Group snapshots by account so we resolve the provider once per
    // account (not per message). Inside each account, restore each
    // message's labels and read flag.
    let mut by_account: HashMap<mxr_core::AccountId, Vec<&UndoEntrySnapshot>> = HashMap::new();
    for snapshot in &entry.snapshots {
        by_account
            .entry(snapshot.account_id.clone())
            .or_default()
            .push(snapshot);
    }

    let mut restored = 0u32;
    let mut irreversible = 0u32;
    let mut last_error: Option<String> = None;
    for (account_id, snapshots) in by_account {
        let provider = match state.get_provider(Some(&account_id)) {
            Ok(p) => p,
            Err(error) => {
                last_error = Some(format!("account unavailable: {error}"));
                irreversible += snapshots.len() as u32;
                continue;
            }
        };
        for snapshot in snapshots {
            match restore_snapshot(state, provider.as_ref(), entry.kind, snapshot).await {
                Ok(()) => restored += 1,
                Err(SnapshotError::Irreversible(msg)) => {
                    irreversible += 1;
                    last_error = Some(msg);
                }
                Err(SnapshotError::Other(msg)) => {
                    last_error = Some(msg);
                }
            }
        }
    }

    if restored == 0 {
        let detail = last_error.unwrap_or_else(|| "no messages restored".into());
        return Err(format!(
            "undo: irreversible ({irreversible} message(s)) — {detail}"
        ));
    }

    // Successful undo: drop the entry so the same id can't be replayed.
    let _ = state.store.delete_undo_entry(&entry.mutation_id).await;
    Ok(ResponseData::Ack)
}

enum SnapshotError {
    /// The upstream message can't be modified (e.g. EXPUNGE, deleted on
    /// the server). Surfacing this distinct from generic errors lets
    /// the UI explain why undo failed.
    Irreversible(String),
    Other(String),
}

async fn restore_snapshot(
    state: &AppState,
    provider: &dyn mxr_core::MailSyncProvider,
    kind: UndoableMutationKind,
    snapshot: &UndoEntrySnapshot,
) -> Result<(), SnapshotError> {
    // Read current state so we can compute the diff to apply against
    // the provider. If the message is gone locally, the provider almost
    // certainly does not have it either.
    let current = state
        .store
        .get_envelope(&snapshot.message_id)
        .await
        .map_err(|e| SnapshotError::Other(e.to_string()))?
        .ok_or_else(|| {
            SnapshotError::Irreversible(format!(
                "message `{}` no longer exists locally",
                snapshot.message_id
            ))
        })?;

    let prior_labels: std::collections::HashSet<&str> = snapshot
        .prior_label_provider_ids
        .iter()
        .map(String::as_str)
        .collect();
    let current_labels: std::collections::HashSet<&str> = current
        .label_provider_ids
        .iter()
        .map(String::as_str)
        .collect();

    let to_add: Vec<String> = prior_labels
        .difference(&current_labels)
        .map(|s| (*s).to_string())
        .collect();
    let to_remove: Vec<String> = current_labels
        .difference(&prior_labels)
        .map(|s| (*s).to_string())
        .collect();

    if !to_add.is_empty() || !to_remove.is_empty() {
        provider
            .modify_labels(&snapshot.provider_id, &to_add, &to_remove)
            .await
            .map_err(|error| classify_provider_error(error))?;
        reconcile_label_mutation(state, provider, &snapshot.message_id, &to_add, &to_remove)
            .await
            .map_err(SnapshotError::Other)?;
    }

    // Read flag: only relevant for SetRead and ReadAndArchive. For the
    // other kinds (Archive, Trash, Spam) we don't touch the read flag.
    if matches!(
        kind,
        UndoableMutationKind::SetRead | UndoableMutationKind::ReadAndArchive
    ) {
        let prior_flags = mxr_core::MessageFlags::from_bits_truncate(snapshot.prior_flags_bits);
        let prior_read = prior_flags.contains(mxr_core::MessageFlags::READ);
        let current_read = current.flags.contains(mxr_core::MessageFlags::READ);
        if prior_read != current_read {
            provider
                .set_read(&snapshot.provider_id, prior_read)
                .await
                .map_err(|error| classify_provider_error(error))?;
            state
                .store
                .set_read(
                    &snapshot.message_id,
                    prior_read,
                    mxr_core::EventSource::User,
                )
                .await
                .map_err(|e| SnapshotError::Other(e.to_string()))?;
        }
    }

    // Refresh the Tantivy index so `mxr search label:inbox` (and friends)
    // see the restored state immediately. Mirror what
    // `apply_mutation_to_envelope` does at the end of every mutation.
    reindex_message_in_search(state, &snapshot.message_id)
        .await
        .map_err(SnapshotError::Other)?;

    Ok(())
}

fn classify_provider_error(error: mxr_core::MxrError) -> SnapshotError {
    let message = error.to_string();
    let lower = message.to_lowercase();
    let irreversible = lower.contains("not found")
        || lower.contains("expunge")
        || lower.contains("does not exist")
        || lower.contains("no such message");
    if irreversible {
        SnapshotError::Irreversible(message)
    } else {
        SnapshotError::Other(message)
    }
}

fn mutation_message_ids(cmd: &MutationCommand) -> &[mxr_core::MessageId] {
    match cmd {
        MutationCommand::Archive { message_ids }
        | MutationCommand::ReadAndArchive { message_ids }
        | MutationCommand::Trash { message_ids }
        | MutationCommand::Spam { message_ids }
        | MutationCommand::Star { message_ids, .. }
        | MutationCommand::SetRead { message_ids, .. }
        | MutationCommand::ModifyLabels { message_ids, .. }
        | MutationCommand::Move { message_ids, .. } => message_ids,
    }
}

async fn apply_mutation_to_envelope(
    state: &AppState,
    provider: &dyn mxr_core::MailSyncProvider,
    cmd: &MutationCommand,
    envelope: &Envelope,
) -> Result<(), String> {
    let message_id = &envelope.id;
    let provider_id = &envelope.provider_id;
    match cmd {
        MutationCommand::Archive { .. } => {
            provider
                .modify_labels(provider_id, &[], &["INBOX".to_string()])
                .await
                .map_err(|e| e.to_string())?;
            reconcile_label_mutation(state, provider, message_id, &[], &["INBOX".to_string()])
                .await?;
        }
        MutationCommand::ReadAndArchive { .. } => {
            provider
                .set_read(provider_id, true)
                .await
                .map_err(|e| e.to_string())?;
            state
                .store
                .set_read(message_id, true, mxr_core::EventSource::User)
                .await
                .map_err(|e| e.to_string())?;
            provider
                .modify_labels(provider_id, &[], &["INBOX".to_string()])
                .await
                .map_err(|e| e.to_string())?;
            reconcile_label_mutation(state, provider, message_id, &[], &["INBOX".to_string()])
                .await?;
        }
        MutationCommand::Trash { .. } => {
            provider
                .trash(provider_id)
                .await
                .map_err(|e| e.to_string())?;
            // Trash is provider-specific (Gmail relabels, IMAP moves+expunges).
            // Mirror the common case locally: drop INBOX, add TRASH if labels-capable;
            // otherwise let reconcile_label_mutation re-sync from the provider.
            reconcile_label_mutation(
                state,
                provider,
                message_id,
                &["TRASH".to_string()],
                &["INBOX".to_string()],
            )
            .await?;
        }
        MutationCommand::Spam { .. } => {
            provider
                .modify_labels(provider_id, &["SPAM".to_string()], &["INBOX".to_string()])
                .await
                .map_err(|e| e.to_string())?;
            reconcile_label_mutation(
                state,
                provider,
                message_id,
                &["SPAM".to_string()],
                &["INBOX".to_string()],
            )
            .await?;
        }
        MutationCommand::Star { starred, .. } => {
            provider
                .set_starred(provider_id, *starred)
                .await
                .map_err(|e| e.to_string())?;
            state
                .store
                .set_starred(message_id, *starred, mxr_core::EventSource::User)
                .await
                .map_err(|e| e.to_string())?;
        }
        MutationCommand::SetRead { read, .. } => {
            provider
                .set_read(provider_id, *read)
                .await
                .map_err(|e| e.to_string())?;
            state
                .store
                .set_read(message_id, *read, mxr_core::EventSource::User)
                .await
                .map_err(|e| e.to_string())?;
        }
        MutationCommand::ModifyLabels { add, remove, .. } => {
            let labels = state
                .store
                .list_labels_by_account(&envelope.account_id)
                .await
                .map_err(|e| e.to_string())?;
            let resolved_add = resolve_to_provider_ids(&labels, add);
            let resolved_remove = resolve_to_provider_ids(&labels, remove);
            provider
                .modify_labels(provider_id, &resolved_add, &resolved_remove)
                .await
                .map_err(|e| e.to_string())?;
            reconcile_label_mutation(state, provider, message_id, &resolved_add, &resolved_remove)
                .await?;
        }
        MutationCommand::Move { target_label, .. } => {
            let labels = state
                .store
                .list_labels_by_account(&envelope.account_id)
                .await
                .map_err(|e| e.to_string())?;
            let resolved_target =
                resolve_to_provider_ids(&labels, std::slice::from_ref(target_label));
            provider
                .modify_labels(provider_id, &resolved_target, &["INBOX".to_string()])
                .await
                .map_err(|e| e.to_string())?;
            reconcile_label_mutation(
                state,
                provider,
                message_id,
                &resolved_target,
                &["INBOX".to_string()],
            )
            .await?;
        }
    }
    // Single point of search-index reconciliation: refresh the indexed envelope so
    // queries (`mxr search ...`) see the new flags/labels immediately.
    reindex_message_in_search(state, message_id).await
}

async fn reindex_message_in_search(
    state: &AppState,
    message_id: &mxr_core::MessageId,
) -> Result<(), String> {
    let envelope = state
        .store
        .get_envelope(message_id)
        .await
        .map_err(|e| e.to_string())?;
    let Some(envelope) = envelope else {
        // Message was removed locally (e.g. IMAP delete); drop from index.
        return state
            .search
            .apply_batch(mxr_search::SearchUpdateBatch {
                entries: Vec::new(),
                removed_message_ids: vec![message_id.clone()],
            })
            .await
            .map_err(|e| e.to_string());
    };
    let body = state
        .store
        .get_body(message_id)
        .await
        .map_err(|e| e.to_string())?;
    let reply_later = state
        .store
        .is_reply_later(message_id)
        .await
        .map_err(|e| e.to_string())?;
    state
        .search
        .apply_batch(mxr_search::SearchUpdateBatch {
            entries: vec![mxr_search::SearchIndexEntry {
                envelope,
                body,
                reply_later,
            }],
            removed_message_ids: Vec::new(),
        })
        .await
        .map_err(|e| e.to_string())
}

fn mutation_log_entry(cmd: &MutationCommand, envelope: &Envelope) -> (String, Option<String>) {
    match cmd {
        MutationCommand::Archive { .. } => (
            format!("Archived {}", quoted_subject(&envelope.subject)),
            Some(format!("from={}", envelope.from.email)),
        ),
        MutationCommand::ReadAndArchive { .. } => (
            format!(
                "Marked {} as read and archived",
                quoted_subject(&envelope.subject)
            ),
            Some(format!("from={}", envelope.from.email)),
        ),
        MutationCommand::Trash { .. } => (
            format!("Moved {} to trash", quoted_subject(&envelope.subject)),
            Some(format!("from={}", envelope.from.email)),
        ),
        MutationCommand::Spam { .. } => (
            format!("Marked {} as spam", quoted_subject(&envelope.subject)),
            Some(format!("from={}", envelope.from.email)),
        ),
        MutationCommand::Star { starred, .. } => (
            if *starred {
                format!("Starred {}", quoted_subject(&envelope.subject))
            } else {
                format!("Unstarred {}", quoted_subject(&envelope.subject))
            },
            Some(format!("from={}", envelope.from.email)),
        ),
        MutationCommand::SetRead { read, .. } => (
            if *read {
                format!("Marked {} as read", quoted_subject(&envelope.subject))
            } else {
                format!("Marked {} as unread", quoted_subject(&envelope.subject))
            },
            Some(format!("from={}", envelope.from.email)),
        ),
        MutationCommand::ModifyLabels { add, remove, .. } => (
            format!("Updated labels on {}", quoted_subject(&envelope.subject)),
            Some(format!(
                "from={} add={} remove={}",
                envelope.from.email,
                add.join(","),
                remove.join(",")
            )),
        ),
        MutationCommand::Move { target_label, .. } => (
            format!(
                "Moved {} to {}",
                quoted_subject(&envelope.subject),
                target_label
            ),
            Some(format!("from={}", envelope.from.email)),
        ),
    }
}

pub(super) async fn snooze(
    state: &AppState,
    message_id: &mxr_core::MessageId,
    wake_at: &chrono::DateTime<chrono::Utc>,
) -> HandlerResult {
    apply_snooze(state, message_id, wake_at).await?;
    if let Some(envelope) = state
        .store
        .get_envelope(message_id)
        .await
        .map_err(|e| e.to_string())?
    {
        if let Err(error) = log_mutation(
            state,
            &envelope,
            format!(
                "Snoozed {} until {}",
                quoted_subject(&envelope.subject),
                wake_at
            ),
            Some(format!("from={}", envelope.from.email)),
        )
        .await
        {
            tracing::warn!(%error, "failed to record snooze event");
        }
    }
    Ok(ResponseData::Ack)
}

pub(super) async fn unsnooze(state: &AppState, message_id: &mxr_core::MessageId) -> HandlerResult {
    let snoozed = state
        .store
        .get_snooze(message_id)
        .await
        .map_err(|e| e.to_string())?;
    if let Some(snoozed) = snoozed {
        let envelope = state
            .store
            .get_envelope(message_id)
            .await
            .map_err(|e| e.to_string())?;
        restore_snoozed_message(state, &snoozed).await?;
        if let Some(envelope) = envelope {
            if let Err(error) = log_mutation(
                state,
                &envelope,
                format!("Unsnoozed {}", quoted_subject(&envelope.subject)),
                Some(format!("from={}", envelope.from.email)),
            )
            .await
            {
                tracing::warn!(%error, "failed to record unsnooze event");
            }
        }
    }
    Ok(ResponseData::Ack)
}

pub(super) async fn list_snoozed(state: &AppState) -> HandlerResult {
    let snoozed = state
        .store
        .list_snoozed()
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::SnoozedMessages { snoozed })
}

pub(super) async fn list_drafts(state: &AppState) -> HandlerResult {
    let accounts = state
        .store
        .list_accounts()
        .await
        .map_err(|e| e.to_string())?;
    let mut drafts = Vec::new();
    for account in accounts {
        drafts.extend(
            state
                .store
                .list_drafts(&account.id)
                .await
                .map_err(|e| e.to_string())?,
        );
    }
    drafts.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    Ok(ResponseData::Drafts { drafts })
}

/// List drafts presumed orphaned mid-send. Mirrors the cutoff used by
/// the daemon's startup recovery loop (1h since last activity) so the
/// CLI surfaces what would be auto-reset, only earlier.
pub(super) async fn list_orphaned_drafts(state: &AppState) -> HandlerResult {
    let cutoff = chrono::Utc::now() - chrono::Duration::hours(1);
    let orphan_ids = state
        .store
        .list_orphaned_sending_drafts(cutoff)
        .await
        .map_err(|e| e.to_string())?;
    let mut drafts = Vec::with_capacity(orphan_ids.len());
    for id in &orphan_ids {
        if let Some(draft) = state.store.get_draft(id).await.map_err(|e| e.to_string())? {
            drafts.push(draft);
        }
    }
    drafts.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    Ok(ResponseData::Drafts { drafts })
}

/// Force-reset an orphaned draft from `'sending'` to `'draft'`.
/// Idempotent: the underlying CAS returns `false` when the draft is
/// already in `'draft'`, which we surface as an `Ack` rather than an
/// error so scripts can call this safely.
pub(super) async fn reset_orphaned_draft(
    state: &AppState,
    draft_id: &mxr_core::DraftId,
) -> HandlerResult {
    state
        .store
        .reset_orphaned_draft(draft_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::Ack)
}

pub(super) async fn save_draft(state: &AppState, draft: &Draft) -> HandlerResult {
    state
        .store
        .insert_draft(draft)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::Ack)
}

pub(super) async fn delete_draft(state: &AppState, draft_id: &mxr_core::DraftId) -> HandlerResult {
    state
        .store
        .delete_draft(draft_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::Ack)
}

pub(super) async fn prepare_reply(
    state: &AppState,
    message_id: &mxr_core::MessageId,
    reply_all: bool,
) -> HandlerResult {
    let envelope = state
        .store
        .get_envelope(message_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Message not found".to_string())?;

    let from = state
        .store
        .get_account(&envelope.account_id)
        .await
        .ok()
        .flatten()
        .map(|account| account.email)
        .unwrap_or_default();

    let thread_context = match state.sync_engine.get_body(message_id).await {
        Ok(body) => render_message_context(&body),
        Err(_) => String::new(),
    };

    let self_address = from.to_ascii_lowercase();
    // Envelope does not yet capture the Reply-To: header — using From: as the reply target
    // covers the common case. Capturing Reply-To: properly is a post-v1 envelope schema change.
    let reply_to = envelope.from.email.clone();

    let cc = if reply_all {
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        seen.insert(reply_to.to_ascii_lowercase());
        if !self_address.is_empty() {
            seen.insert(self_address.clone());
        }
        envelope
            .to
            .iter()
            .chain(envelope.cc.iter())
            .map(|address| address.email.clone())
            .filter(|email| {
                let key = email.to_ascii_lowercase();
                seen.insert(key)
            })
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        String::new()
    };

    Ok(ResponseData::ReplyContext {
        context: ReplyContext {
            account_id: envelope.account_id.clone(),
            in_reply_to: envelope.message_id_header.clone().unwrap_or_default(),
            references: build_reply_references(&envelope),
            reply_to,
            cc,
            subject: envelope.subject.clone(),
            from,
            thread_context,
            thread_id: None,
        },
    })
}

pub(super) async fn prepare_forward(
    state: &AppState,
    message_id: &mxr_core::MessageId,
) -> HandlerResult {
    let envelope = state
        .store
        .get_envelope(message_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Message not found".to_string())?;

    let from = state
        .store
        .get_account(&envelope.account_id)
        .await
        .ok()
        .flatten()
        .map(|account| account.email)
        .unwrap_or_default();

    let forwarded_content = match state.sync_engine.get_body(message_id).await {
        Ok(body) => render_message_context(&body),
        Err(_) => String::new(),
    };

    Ok(ResponseData::ForwardContext {
        context: ForwardContext {
            account_id: envelope.account_id.clone(),
            subject: envelope.subject.clone(),
            from,
            forwarded_content,
        },
    })
}

async fn resolve_from_address(state: &AppState, draft: &Draft) -> Address {
    let account = state
        .store
        .get_account(&draft.account_id)
        .await
        .ok()
        .flatten();
    Address {
        name: account.as_ref().map(|account| account.name.clone()),
        email: account
            .as_ref()
            .map(|account| account.email.clone())
            .unwrap_or_else(|| "user@example.com".to_string()),
    }
}

pub(super) async fn send_draft(
    state: &AppState,
    draft: &Draft,
    override_safety_token: Option<&str>,
) -> HandlerResult {
    enforce_draft_safety_with_override(state, draft, override_safety_token).await?;
    let sender = state.send_provider_for_account(&draft.account_id)?;
    let from = resolve_from_address(state, draft).await;
    let rfc2822_message_id = mxr_outbound::email::generate_message_id(&from);
    let receipt = sender
        .send(draft, &from, &rfc2822_message_id)
        .await
        .map_err(|e| e.to_string())?;
    clear_reply_later_for_reply_parent(state, draft).await;
    // Ingest synthetic Sent envelope so the message is searchable as `is:sent`
    // immediately, without waiting for the next sync. Failures here are
    // non-fatal: the send already succeeded on the wire and we surface the
    // local-ingest error to the caller via tracing.
    let local_message_id = match ingest_sent_message(state, draft, &from, &receipt).await {
        Ok(id) => Some(id),
        Err(e) => {
            tracing::warn!(error = %e, "ingest_sent_message failed after send_draft");
            None
        }
    };
    Ok(ResponseData::SendReceipt {
        local_message_id: local_message_id.unwrap_or_else(|| {
            mxr_core::MessageId::from_scoped_provider_id(
                &draft.account_id,
                "send-receipt",
                &receipt.rfc2822_message_id,
            )
        }),
        provider_message_id: receipt.provider_message_id,
        rfc2822_message_id: receipt.rfc2822_message_id,
    })
}

pub(super) async fn schedule_send(
    state: &AppState,
    draft_id: &mxr_core::DraftId,
    send_at: chrono::DateTime<chrono::Utc>,
) -> HandlerResult {
    state
        .store
        .schedule_send(draft_id, send_at)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::Ack)
}

pub(super) async fn cancel_scheduled_send(
    state: &AppState,
    draft_id: &mxr_core::DraftId,
) -> HandlerResult {
    state
        .store
        .cancel_scheduled_send(draft_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::Ack)
}

pub(crate) async fn send_stored_draft(
    state: &AppState,
    draft_id: &mxr_core::DraftId,
    override_safety_token: Option<&str>,
) -> HandlerResult {
    let draft = state
        .store
        .get_draft(draft_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Draft not found: {draft_id}"))?;

    enforce_draft_safety_with_override(state, &draft, override_safety_token).await?;

    // Compare-and-set: only the unique `Draft` -> `Sending` transition is
    // allowed to invoke the provider. A draft already in `Sending` (likely a
    // crashed prior attempt) or `Sent` (already delivered) refuses.
    let advanced = state
        .store
        .cas_draft_status(draft_id, DraftStatus::Draft, DraftStatus::Sending)
        .await
        .map_err(|e| e.to_string())?;
    if !advanced {
        let current = state
            .store
            .get_draft_status(draft_id)
            .await
            .map_err(|e| e.to_string())?
            .unwrap_or(DraftStatus::Draft);
        return Err(match current {
            DraftStatus::Sent => "draft already sent".to_string(),
            DraftStatus::Sending => {
                "draft is already being sent (resolve via `mxr drafts resolve`)".to_string()
            }
            DraftStatus::Draft => "draft state mismatch; retry".to_string(),
        });
    }

    // Mark the draft as actively in-flight so the 1h startup-recovery cutoff
    // doesn't re-claim a long but legitimate send. Failures here are
    // non-fatal — orphan recovery falls back to `status_updated_at`.
    let _ = state
        .store
        .touch_draft_heartbeat(draft_id, chrono::Utc::now())
        .await;

    let sender = match state.send_provider_for_account(&draft.account_id) {
        Ok(s) => s,
        Err(e) => {
            // Revert; the daemon never invoked the provider.
            let _ = state
                .store
                .update_draft_status(draft_id, DraftStatus::Draft)
                .await;
            return Err(e);
        }
    };
    let from = resolve_from_address(state, &draft).await;
    let rfc2822_message_id = match state
        .store
        .get_draft_message_id_header(draft_id)
        .await
        .map_err(|e| e.to_string())?
    {
        Some(existing) => existing,
        None => {
            let generated = mxr_outbound::email::generate_message_id(&from);
            if let Err(e) = state
                .store
                .set_draft_message_id_header(draft_id, &generated)
                .await
            {
                let _ = state
                    .store
                    .update_draft_status(draft_id, DraftStatus::Draft)
                    .await;
                return Err(e.to_string());
            }
            generated
        }
    };

    let receipt = match sender.send(&draft, &from, &rfc2822_message_id).await {
        Ok(r) => r,
        Err(e) => {
            let _ = state
                .store
                .update_draft_status(draft_id, DraftStatus::Draft)
                .await;
            return Err(e.to_string());
        }
    };
    clear_reply_later_for_reply_parent(state, &draft).await;

    let local_message_id = match ingest_sent_message(state, &draft, &from, &receipt).await {
        Ok(id) => id,
        Err(e) => {
            // Local ingest failed. Send already happened on the wire; we move
            // the draft to `Sent` (idempotent, prevents resend) but bubble the
            // error so the user can re-sync.
            let _ = state
                .store
                .update_draft_status(draft_id, DraftStatus::Sent)
                .await;
            return Err(format!("send succeeded but local ingest failed: {e}"));
        }
    };

    let _ = state
        .store
        .update_draft_status(draft_id, DraftStatus::Sent)
        .await;
    let _ = state.store.delete_draft(draft_id).await;
    Ok(ResponseData::SendReceipt {
        local_message_id,
        provider_message_id: receipt.provider_message_id,
        rfc2822_message_id: receipt.rfc2822_message_id,
    })
}

async fn clear_reply_later_for_reply_parent(state: &AppState, draft: &Draft) {
    let Some(reply_headers) = draft.reply_headers.as_ref() else {
        return;
    };
    let parent = match state
        .store
        .list_envelopes_by_message_id_header(&draft.account_id, &reply_headers.in_reply_to)
        .await
    {
        Ok(mut envelopes) => envelopes.pop(),
        Err(error) => {
            tracing::warn!(%error, "failed to resolve reply parent for reply-later clear");
            return;
        }
    };
    let Some(parent) = parent else {
        return;
    };
    if let Err(error) = state
        .store
        .clear_reply_later(&parent.id, chrono::Utc::now())
        .await
    {
        tracing::warn!(
            message_id = %parent.id,
            error = %error,
            "failed to clear reply-later flag after reply send"
        );
    }
}

/// Insert a synthetic envelope + body into the local store immediately after a
/// successful provider send so the message is searchable as `is:sent` without
/// waiting for the next sync. Keyed by `provider_message_id` (Gmail) or the
/// rendered Message-ID header (SMTP) so the next sync's `upsert_envelope` is
/// idempotent for Gmail (same UUID v5) — see `MessageId::from_scoped_provider_id`.
async fn ingest_sent_message(
    state: &AppState,
    draft: &Draft,
    from: &Address,
    receipt: &SendReceipt,
) -> Result<mxr_core::MessageId, String> {
    let (provider_namespace, provider_id_value) =
        if let Some(gmail_id) = receipt.provider_message_id.as_deref() {
            ("gmail", gmail_id.to_string())
        } else {
            ("smtp-local", receipt.rfc2822_message_id.clone())
        };
    let message_id = mxr_core::MessageId::from_scoped_provider_id(
        &draft.account_id,
        provider_namespace,
        &provider_id_value,
    );

    let snippet: String = draft
        .body_markdown
        .chars()
        .filter(|c| !c.is_control())
        .take(200)
        .collect();

    let envelope = Envelope {
        id: message_id.clone(),
        account_id: draft.account_id.clone(),
        provider_id: provider_id_value,
        thread_id: sent_thread_id(state, draft).await.unwrap_or_default(),
        message_id_header: Some(receipt.rfc2822_message_id.clone()),
        in_reply_to: draft.reply_headers.as_ref().map(|h| h.in_reply_to.clone()),
        references: draft
            .reply_headers
            .as_ref()
            .map(|h| h.references.clone())
            .unwrap_or_default(),
        from: from.clone(),
        to: draft.to.clone(),
        cc: draft.cc.clone(),
        bcc: draft.bcc.clone(),
        subject: draft.subject.clone(),
        date: receipt.sent_at,
        flags: MessageFlags::SENT | MessageFlags::READ,
        snippet: snippet.clone(),
        has_attachments: !draft.attachments.is_empty(),
        size_bytes: draft.body_markdown.len() as u64,
        unsubscribe: UnsubscribeMethod::None,
        label_provider_ids: if provider_namespace == "gmail" {
            vec!["SENT".to_string()]
        } else {
            Vec::new()
        },
    };
    let thread_id = envelope.thread_id.clone();

    state
        .store
        .upsert_envelope_with_direction(&envelope, MessageDirection::Outbound)
        .await
        .map_err(|e| e.to_string())?;

    let body = MessageBody {
        message_id: message_id.clone(),
        text_plain: Some(draft.body_markdown.clone()),
        text_html: None,
        attachments: Vec::new(),
        fetched_at: receipt.sent_at,
        metadata: MessageMetadata::default(),
    };
    state
        .store
        .insert_body(&body)
        .await
        .map_err(|e| e.to_string())?;

    // Apply Gmail SENT label if the account already knows about it (it will,
    // post-first-sync). Missing label is non-fatal: `is:sent` queries match on
    // MessageFlags::SENT regardless of the labels junction.
    if provider_namespace == "gmail" {
        if let Ok(Some(sent_label)) = state
            .store
            .find_label_by_provider_id(&draft.account_id, "SENT")
            .await
        {
            let _ = state
                .store
                .set_message_labels(
                    &message_id,
                    std::slice::from_ref(&sent_label.id),
                    mxr_core::EventSource::User,
                )
                .await;
        }
    }

    state
        .search
        .apply_batch(mxr_search::SearchUpdateBatch {
            entries: vec![mxr_search::SearchIndexEntry {
                envelope,
                body: Some(body),
                reply_later: false,
            }],
            removed_message_ids: Vec::new(),
        })
        .await
        .map_err(|e| e.to_string())?;

    if let Err(error) =
        crate::handler::summarize::refresh_thread_summary_if_enabled(state, &thread_id).await
    {
        tracing::warn!(%thread_id, error = %error, "failed to refresh sent thread summary");
    }

    Ok(message_id)
}

async fn sent_thread_id(state: &AppState, draft: &Draft) -> Option<mxr_core::ThreadId> {
    let in_reply_to = draft.reply_headers.as_ref()?.in_reply_to.as_str();
    state
        .store
        .list_envelopes_by_message_id_header(&draft.account_id, in_reply_to)
        .await
        .ok()?
        .into_iter()
        .next()
        .map(|envelope| envelope.thread_id)
}

pub(super) async fn save_draft_to_server(state: &AppState, draft: &Draft) -> HandlerResult {
    let sender = match state.send_provider_for_account(&draft.account_id) {
        Ok(sender) => sender,
        Err(error) => {
            tracing::info!(error, "No server draft provider; saving local draft");
            return save_draft(state, draft).await;
        }
    };
    let account = state
        .store
        .get_account(&draft.account_id)
        .await
        .ok()
        .flatten();
    let from = Address {
        name: account.as_ref().map(|account| account.name.clone()),
        email: account
            .as_ref()
            .map(|account| account.email.clone())
            .unwrap_or_else(|| "user@example.com".to_string()),
    };
    match sender.save_draft(draft, &from).await {
        Ok(Some(draft_id)) => {
            tracing::info!(draft_id, "Draft saved to server");
            Ok(ResponseData::Ack)
        }
        Ok(None) => {
            tracing::info!("Provider does not support server-side drafts; saving local draft");
            save_draft(state, draft).await
        }
        Err(error) => Err(format!("Failed to save draft: {error}")),
    }
}

pub(super) async fn unsubscribe(
    state: &AppState,
    message_id: &mxr_core::MessageId,
) -> HandlerResult {
    let envelope = state
        .store
        .get_envelope(message_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Message not found".to_string())?;
    match &envelope.unsubscribe {
        UnsubscribeMethod::Mailto { address, subject } => {
            let sender = state.send_provider_for_account(&envelope.account_id)?;
            let account = state
                .store
                .get_account(&envelope.account_id)
                .await
                .ok()
                .flatten();
            let from = Address {
                name: account.as_ref().map(|account| account.name.clone()),
                email: account
                    .as_ref()
                    .map(|account| account.email.clone())
                    .unwrap_or_else(|| "user@example.com".to_string()),
            };
            let now = chrono::Utc::now();
            let draft = Draft {
                id: mxr_core::DraftId::new(),
                account_id: envelope.account_id.clone(),
                reply_headers: None,
                intent: mxr_core::DraftIntent::New,
                to: vec![Address {
                    name: None,
                    email: address.clone(),
                }],
                cc: vec![],
                bcc: vec![],
                subject: subject.clone().unwrap_or_else(|| "unsubscribe".to_string()),
                body_markdown: "unsubscribe".to_string(),
                attachments: vec![],
                created_at: now,
                updated_at: now,
            };
            let rfc2822_message_id = mxr_outbound::email::generate_message_id(&from);
            sender
                .send(&draft, &from, &rfc2822_message_id)
                .await
                .map_err(|e| e.to_string())?;
            if let Err(error) = log_mutation(
                state,
                &envelope,
                format!(
                    "Sent unsubscribe request for {}",
                    quoted_subject(&envelope.subject)
                ),
                Some(format!("mailto={address} from={}", envelope.from.email)),
            )
            .await
            {
                tracing::warn!(%error, "failed to record unsubscribe event");
            }
            Ok(ResponseData::Ack)
        }
        _ => {
            let client = reqwest::Client::new();
            match crate::unsubscribe::execute_unsubscribe(&envelope.unsubscribe, &client).await {
                crate::unsubscribe::UnsubscribeResult::Success(result) => {
                    if let Err(error) = log_mutation(
                        state,
                        &envelope,
                        format!("Unsubscribed from {}", quoted_subject(&envelope.subject)),
                        Some(format!("result={result} from={}", envelope.from.email)),
                    )
                    .await
                    {
                        tracing::warn!(%error, "failed to record unsubscribe event");
                    }
                    Ok(ResponseData::Ack)
                }
                crate::unsubscribe::UnsubscribeResult::Failed(message) => Err(message),
                crate::unsubscribe::UnsubscribeResult::NoMethod => {
                    Err("No unsubscribe method available for this message".to_string())
                }
            }
        }
    }
}

/// Resolve label references (names or provider IDs) to provider IDs.
///
/// Both the TUI and CLI send label display names (e.g. "Follow Up") but
/// the Gmail API requires provider IDs (e.g. "Label_123"). This function
/// looks up each reference in the account's label list and returns the
/// corresponding provider_id. If no match is found the original string
/// is passed through (handles system labels like "INBOX", "SPAM" where
/// name == provider_id).
fn resolve_to_provider_ids(labels: &[mxr_core::types::Label], refs: &[String]) -> Vec<String> {
    refs.iter()
        .map(|label_ref| {
            labels
                .iter()
                .find(|l| l.name == *label_ref || l.provider_id == *label_ref)
                .map(|l| l.provider_id.clone())
                .unwrap_or_else(|| label_ref.clone())
        })
        .collect()
}

pub(super) async fn set_flags(
    state: &AppState,
    message_id: &mxr_core::MessageId,
    flags: mxr_core::MessageFlags,
) -> HandlerResult {
    let envelope = state
        .store
        .get_envelope(message_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Message not found: {message_id}"))?;

    let supported = MessageFlags::READ | MessageFlags::STARRED;
    let unsupported_changed_bits = (envelope.flags.bits() ^ flags.bits()) & !supported.bits();
    if unsupported_changed_bits != 0 {
        return Err(
            "SetFlags only supports provider-routed READ and STARRED changes; use typed mutations"
                .to_string(),
        );
    }

    let provider = state.get_provider(Some(&envelope.account_id))?;
    let provider_id = envelope.provider_id.as_str();

    let read = flags.contains(MessageFlags::READ);
    if envelope.flags.contains(MessageFlags::READ) != read {
        provider
            .set_read(provider_id, read)
            .await
            .map_err(|e| e.to_string())?;
        state
            .store
            .set_read(message_id, read, mxr_core::EventSource::User)
            .await
            .map_err(|e| e.to_string())?;
    }

    let starred = flags.contains(MessageFlags::STARRED);
    if envelope.flags.contains(MessageFlags::STARRED) != starred {
        provider
            .set_starred(provider_id, starred)
            .await
            .map_err(|e| e.to_string())?;
        state
            .store
            .set_starred(message_id, starred, mxr_core::EventSource::User)
            .await
            .map_err(|e| e.to_string())?;
    }

    reindex_message_in_search(state, message_id).await?;
    Ok(ResponseData::Ack)
}

#[cfg(test)]
mod reconciliation_failed_emit_tests {
    use super::*;
    use crate::state::AppState;
    use mxr_protocol::{DaemonEvent, IpcPayload};
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::timeout;

    fn partial_result() -> MutationResultData {
        MutationResultData {
            requested: 2,
            succeeded: 1,
            skipped: 1,
            failed: 0,
            accounts: vec![],
            mutation_id: None,
        }
    }

    #[tokio::test]
    async fn emits_ipc_event_when_partial_success_and_correlation_set() {
        let state = Arc::new(AppState::in_memory_without_accounts().await.unwrap());
        let mut rx = state.event_tx.subscribe();
        emit_mutation_reconciliation_failed_if_needed(&state, Some("42"), &partial_result());
        let msg = rx.recv().await.expect("broadcast should deliver");
        match msg.payload {
            IpcPayload::Event(DaemonEvent::MutationReconciliationFailed {
                client_correlation_id,
                ..
            }) => assert_eq!(client_correlation_id, "42"),
            other => panic!("expected MutationReconciliationFailed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn skips_emit_when_correlation_missing() {
        let state = Arc::new(AppState::in_memory_without_accounts().await.unwrap());
        let mut rx = state.event_tx.subscribe();
        emit_mutation_reconciliation_failed_if_needed(&state, None, &partial_result());
        assert!(
            timeout(Duration::from_millis(30), rx.recv()).await.is_err(),
            "no event without client_correlation_id"
        );
    }

    #[tokio::test]
    async fn skips_emit_when_mutation_fully_succeeded() {
        let state = Arc::new(AppState::in_memory_without_accounts().await.unwrap());
        let mut rx = state.event_tx.subscribe();
        let result = MutationResultData {
            requested: 2,
            succeeded: 2,
            skipped: 0,
            failed: 0,
            accounts: vec![],
            mutation_id: None,
        };
        emit_mutation_reconciliation_failed_if_needed(&state, Some("1"), &result);
        assert!(
            timeout(Duration::from_millis(30), rx.recv()).await.is_err(),
            "no event when succeeded >= requested"
        );
    }
}
