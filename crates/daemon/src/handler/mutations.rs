use super::{
    apply_snooze, build_reply_references, get_or_render_reply_context, reconcile_label_mutation,
    restore_snoozed_message, HandlerError, HandlerResult,
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
    ForwardContext, JobData, JobProgressData, JobStatusData, MutationCommand, MutationResultData,
    ReplyContext, ResponseData, UnsubscribePurgeResultData, UnsubscribePurgeStatusData,
};
use mxr_store::{EventLogRefs, UndoEntry, UndoEntrySnapshot, UndoableMutationKind};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};

/// How long after the mutation the user can undo it. Matches the plan
/// (~60s) and pairs with `tick_connection_state`-style UI affordances on
/// the TUI side.
const UNDO_WINDOW_SECS: i64 = 60;
const MUTATION_JOB_CHUNK_SIZE: usize = 100;
const MAX_RETAINED_JOBS: usize = 100;

static JOBS: OnceLock<Mutex<Vec<JobData>>> = OnceLock::new();

fn jobs_store() -> &'static Mutex<Vec<JobData>> {
    JOBS.get_or_init(|| Mutex::new(Vec::new()))
}

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
            proposed_send_at: None,
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
                        "override token does not cover blocker(s): {unauthorized:?}"
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

/// Build a `mxr_safety::SafetyContext` from store data: self addresses,
/// known contacts for typo/first-time-external detection, per-recipient
/// style baselines for tone-mismatch, and the parent thread's display
/// names for reply-all vocative filtering.
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

    let known_contacts = state
        .store
        .list_known_contacts(&draft.account_id, KNOWN_CONTACTS_LIMIT)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|row| mxr_safety::KnownContact {
            email: row.email,
            display_name: row.display_name,
            total_inbound: row.total_inbound as u64,
            total_outbound: row.total_outbound as u64,
        })
        .collect();

    let mut contact_styles = Vec::new();
    let mut seen_style = HashSet::new();
    for addr in draft
        .to
        .iter()
        .chain(&draft.cc)
        .take(CONTACT_STYLE_LOOKUP_CAP)
    {
        let email = addr.email.to_ascii_lowercase();
        if email.is_empty() || !seen_style.insert(email.clone()) {
            continue;
        }
        let style = state
            .store
            .get_contact_style(&draft.account_id, &email)
            .await
            .map_err(|e| e.to_string())?;
        let Some(style) = style else { continue };
        if style.msg_count_used_theirs == 0 {
            continue;
        }
        let baseline: mxr_relationship::StylometryMetrics = match serde_json::from_str(
            &style.metrics_json_theirs,
        ) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(email = %email, "skip contact_style: parse metrics_json_theirs: {e}");
                continue;
            }
        };
        contact_styles.push(mxr_safety::ContactStyleBaseline {
            email,
            baseline,
            baseline_sample_count: style.msg_count_used_theirs,
        });
    }

    let mut thread_display_names = Vec::new();
    if let Some(thread_id) = context.thread_id.as_ref() {
        let envelopes = state
            .store
            .get_thread_envelopes(thread_id)
            .await
            .map_err(|e| e.to_string())?;
        let mut seen = HashSet::new();
        for envelope in envelopes {
            for participant in std::iter::once(&envelope.from)
                .chain(envelope.to.iter())
                .chain(envelope.cc.iter())
                .chain(envelope.bcc.iter())
            {
                if let Some(name) = participant.name.as_deref() {
                    // Push the full display ("Sam Carter") and each alpha
                    // token ("Sam", "Carter"). The reply-all vocative
                    // regex matches a single capitalized word, so we need
                    // tokens; full-name pushes future-proof callers that
                    // match exact display.
                    let trimmed = name.trim();
                    if !trimmed.is_empty() && seen.insert(trimmed.to_ascii_lowercase()) {
                        thread_display_names.push(trimmed.to_string());
                    }
                    for token in trimmed.split(|c: char| !c.is_alphabetic()) {
                        if token.len() < 2 {
                            continue;
                        }
                        let key = token.to_ascii_lowercase();
                        if seen.insert(key) {
                            thread_display_names.push(token.to_string());
                        }
                    }
                }
            }
        }
    }

    Ok(mxr_safety::SafetyContext {
        mode_reply_all: context.reply_all
            || matches!(draft.intent, mxr_core::types::DraftIntent::ReplyAll),
        self_addresses,
        known_contacts,
        contact_styles,
        thread_display_names,
    })
}

/// Cap how many contacts we pull into safety context. The typo check is
/// O(recipients * contacts) with a damerau-levenshtein early-exit, so a
/// few hundred rows is fine; large mailboxes don't get penalized.
const KNOWN_CONTACTS_LIMIT: u32 = 200;
/// Cap how many recipients we look up baselines for. Drafts with many
/// recipients are almost always replies-to-list; the tone check is most
/// useful for direct correspondence.
const CONTACT_STYLE_LOOKUP_CAP: usize = 10;

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

    if context.allow_llm {
        if let Some(thread_id) = context.thread_id.clone() {
            let issues = super::safety_llm::check_answer_coverage(state, draft, &thread_id).await;
            report.extend(issues);
        }
    }

    // Slice 4.1 wiring (C2.5): if the caller provided a
    // `proposed_send_at`, emit a Severity::Info hint when the slot is
    // materially slower than the recipient's fastest historic bucket.
    if let Some(proposed_at) = context.proposed_send_at {
        let issues = super::safety_timing::check_send_time(state, draft, proposed_at).await;
        report.extend(issues);
    }

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
    crate::chimes::emit_daemon_event(
        state,
        DaemonEvent::MutationReconciliationFailed {
            client_correlation_id: cid.to_string(),
            error_summary,
        },
    );
}

pub(super) async fn mutation(
    state: &AppState,
    cmd: &MutationCommand,
    client_correlation_id: Option<&str>,
) -> HandlerResult {
    let message_ids = mutation_message_ids(cmd);
    let undoable_kind = undoable_kind(cmd);
    // One id per MutationCommand batch — drives both the dedup log
    // (retry safety) and the undo log (user-undo). Generated up-front
    // so dedup applies even to non-undoable commands.
    let mutation_id = uuid::Uuid::now_v7().to_string();
    let mut grouped: HashMap<mxr_core::AccountId, Vec<Envelope>> = HashMap::new();
    for message_id in message_ids {
        let envelope = state
            .store
            .get_envelope(message_id)
            .await?
            .ok_or_else(|| format!("Message not found: {message_id}"))?;
        grouped
            .entry(envelope.account_id.clone())
            .or_default()
            .push(envelope);
    }

    if matches!(cmd, MutationCommand::Route { dry_run: true, .. }) {
        let mut accounts = Vec::new();
        for (account_id, envelopes) in grouped {
            let account_name = state
                .store
                .get_account(&account_id)
                .await
                .ok()
                .flatten()
                .map_or_else(|| account_id.to_string(), |account| account.name);
            accounts.push(AccountMutationResultData {
                account_id,
                account_name,
                succeeded: 0,
                skipped: envelopes.len() as u32,
                failed: 0,
                error: None,
            });
        }
        accounts.sort_by(|left, right| {
            left.account_name
                .to_lowercase()
                .cmp(&right.account_name.to_lowercase())
                .then_with(|| left.account_id.as_str().cmp(&right.account_id.as_str()))
        });
        return Ok(ResponseData::MutationResult {
            result: MutationResultData {
                requested: message_ids.len() as u32,
                succeeded: 0,
                skipped: message_ids.len() as u32,
                failed: 0,
                accounts,
                mutation_id: None,
            },
        });
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
            .map_or_else(|| account_id.to_string(), |account| account.name);

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

            match apply_mutation_to_envelope(state, provider.as_ref(), &mutation_id, cmd, envelope)
                .await
            {
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

    // Persist an undo entry only for undoable kinds and only if at
    // least one envelope succeeded. The mutation_id is the same one
    // used for dedup above; undo + dedup share a key by design.
    let mutation_id = match (undoable_kind, succeeded_snapshots.is_empty()) {
        (Some(kind), false) => {
            let now = chrono::Utc::now().timestamp();
            let entry = UndoEntry {
                mutation_id: mutation_id.clone(),
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
                Some(mutation_id)
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

pub(super) async fn start_mutation_job(
    state: Arc<AppState>,
    cmd: MutationCommand,
    client_correlation_id: Option<String>,
) -> HandlerResult {
    let total = mutation_message_ids(&cmd).len() as u32;
    if total == 0 {
        return Err("No messages matched".into());
    }

    let job_id = uuid::Uuid::now_v7().to_string();
    let now = chrono::Utc::now().timestamp_millis();
    let job = JobData {
        job_id: job_id.clone(),
        kind: mutation_job_kind(&cmd).to_string(),
        status: JobStatusData::Queued,
        progress: JobProgressData {
            total,
            completed: 0,
            succeeded: 0,
            skipped: 0,
            failed: 0,
        },
        undo_ids: Vec::new(),
        error: None,
        started_at: now,
        finished_at: None,
        result: None,
    };
    upsert_job(job.clone());

    let background_job_id = job_id.clone();
    tokio::spawn(async move {
        run_mutation_job(state, background_job_id, cmd, client_correlation_id).await;
    });

    Ok(ResponseData::JobStarted { job })
}

pub(super) fn list_jobs() -> HandlerResult {
    let mut jobs = jobs_store()
        .lock()
        .map_err(|_| HandlerError::from("jobs store poisoned"))?
        .clone();
    jobs.sort_by_key(|job| std::cmp::Reverse(job.started_at));
    Ok(ResponseData::Jobs { jobs })
}

pub(super) fn get_job(job_id: &str) -> HandlerResult {
    let jobs = jobs_store()
        .lock()
        .map_err(|_| HandlerError::from("jobs store poisoned"))?;
    let job = jobs
        .iter()
        .find(|job| job.job_id == job_id)
        .cloned()
        .ok_or_else(|| HandlerError::from(format!("job not found: {job_id}")))?;
    Ok(ResponseData::Job { job })
}

async fn run_mutation_job(
    state: Arc<AppState>,
    job_id: String,
    cmd: MutationCommand,
    client_correlation_id: Option<String>,
) {
    update_job(&job_id, |job| {
        job.status = JobStatusData::Running;
    });
    crate::chimes::emit_daemon_event(
        &state,
        DaemonEvent::OperationStarted {
            operation_id: job_id.clone(),
            operation: mutation_job_kind(&cmd).to_string(),
            account_id: None,
            message: format!(
                "Starting mutation job over {} message(s)",
                mutation_message_ids(&cmd).len()
            ),
        },
    );

    let total = mutation_message_ids(&cmd).len() as u32;
    let mut aggregate = empty_mutation_result(total);
    let mut undo_ids = Vec::new();
    let mut terminal_error: Option<String> = None;

    for ids in mutation_message_ids(&cmd).chunks(MUTATION_JOB_CHUNK_SIZE) {
        let chunk_cmd = mutation_command_with_ids(&cmd, ids.to_vec());
        match mutation(&state, &chunk_cmd, client_correlation_id.as_deref()).await {
            Ok(ResponseData::MutationResult { result }) => {
                if let Some(mutation_id) = result.mutation_id.as_ref() {
                    undo_ids.push(mutation_id.clone());
                }
                merge_mutation_result(&mut aggregate, &result);
                update_job(&job_id, |job| {
                    job.progress.completed =
                        aggregate.succeeded + aggregate.skipped + aggregate.failed;
                    job.progress.succeeded = aggregate.succeeded;
                    job.progress.skipped = aggregate.skipped;
                    job.progress.failed = aggregate.failed;
                    job.undo_ids = undo_ids.clone();
                    job.result = Some(aggregate.clone());
                });
                crate::chimes::emit_daemon_event(
                    &state,
                    DaemonEvent::OperationProgress {
                        operation_id: job_id.clone(),
                        operation: mutation_job_kind(&cmd).to_string(),
                        account_id: None,
                        current: aggregate.succeeded + aggregate.skipped + aggregate.failed,
                        total: Some(total),
                        message: format!(
                            "{} of {} processed ({} succeeded, {} skipped)",
                            aggregate.succeeded + aggregate.skipped + aggregate.failed,
                            total,
                            aggregate.succeeded,
                            aggregate.skipped
                        ),
                    },
                );
                if result.skipped > 0 || result.failed > 0 {
                    terminal_error = Some(format!(
                        "mutation job stopped after partial progress: {} succeeded, {} skipped, {} failed",
                        aggregate.succeeded, aggregate.skipped, aggregate.failed
                    ));
                    break;
                }
            }
            Ok(_) => {
                terminal_error =
                    Some("daemon returned unexpected mutation job response".to_string());
                break;
            }
            Err(error) => {
                terminal_error = Some(error.to_string());
                break;
            }
        }
    }

    let finished_at = chrono::Utc::now().timestamp_millis();
    if aggregate.mutation_id.is_none() && undo_ids.len() == 1 {
        aggregate.mutation_id = undo_ids.first().cloned();
    }
    let status = if terminal_error.is_some() {
        JobStatusData::Failed
    } else {
        JobStatusData::Succeeded
    };
    update_job(&job_id, |job| {
        job.status = status;
        job.error = terminal_error.clone();
        job.finished_at = Some(finished_at);
        job.undo_ids = undo_ids.clone();
        job.progress.completed = aggregate.succeeded + aggregate.skipped + aggregate.failed;
        job.progress.succeeded = aggregate.succeeded;
        job.progress.skipped = aggregate.skipped;
        job.progress.failed = aggregate.failed;
        job.result = Some(aggregate.clone());
    });

    match terminal_error {
        Some(error) => crate::chimes::emit_daemon_event(
            &state,
            DaemonEvent::OperationFailed {
                operation_id: job_id,
                operation: mutation_job_kind(&cmd).to_string(),
                account_id: None,
                error,
                retryable: false,
            },
        ),
        None => crate::chimes::emit_daemon_event(
            &state,
            DaemonEvent::OperationCompleted {
                operation_id: job_id,
                operation: mutation_job_kind(&cmd).to_string(),
                account_id: None,
                message: format!(
                    "Mutation job completed: {} succeeded, {} skipped",
                    aggregate.succeeded, aggregate.skipped
                ),
            },
        ),
    }
}

fn upsert_job(job: JobData) {
    if let Ok(mut jobs) = jobs_store().lock() {
        jobs.retain(|existing| existing.job_id != job.job_id);
        jobs.push(job);
        if jobs.len() > MAX_RETAINED_JOBS {
            let overflow = jobs.len() - MAX_RETAINED_JOBS;
            jobs.drain(0..overflow);
        }
    }
}

fn update_job(job_id: &str, update: impl FnOnce(&mut JobData)) {
    if let Ok(mut jobs) = jobs_store().lock() {
        if let Some(job) = jobs.iter_mut().find(|job| job.job_id == job_id) {
            update(job);
        }
    }
}

fn empty_mutation_result(requested: u32) -> MutationResultData {
    MutationResultData {
        requested,
        succeeded: 0,
        skipped: 0,
        failed: 0,
        accounts: Vec::new(),
        mutation_id: None,
    }
}

fn merge_mutation_result(aggregate: &mut MutationResultData, chunk: &MutationResultData) {
    aggregate.succeeded += chunk.succeeded;
    aggregate.skipped += chunk.skipped;
    aggregate.failed += chunk.failed;
    for account in &chunk.accounts {
        if let Some(existing) = aggregate
            .accounts
            .iter_mut()
            .find(|existing| existing.account_id == account.account_id)
        {
            existing.succeeded += account.succeeded;
            existing.skipped += account.skipped;
            existing.failed += account.failed;
            if existing.error.is_none() {
                existing.error = account.error.clone();
            }
        } else {
            aggregate.accounts.push(account.clone());
        }
    }
    aggregate.accounts.sort_by(|left, right| {
        left.account_name
            .to_lowercase()
            .cmp(&right.account_name.to_lowercase())
            .then_with(|| left.account_id.as_str().cmp(&right.account_id.as_str()))
    });
}

fn mutation_command_with_ids(
    cmd: &MutationCommand,
    message_ids: Vec<mxr_core::MessageId>,
) -> MutationCommand {
    match cmd {
        MutationCommand::Archive { .. } => MutationCommand::Archive { message_ids },
        MutationCommand::ReadAndArchive { .. } => MutationCommand::ReadAndArchive { message_ids },
        MutationCommand::Trash { .. } => MutationCommand::Trash { message_ids },
        MutationCommand::Spam { .. } => MutationCommand::Spam { message_ids },
        MutationCommand::Star { starred, .. } => MutationCommand::Star {
            message_ids,
            starred: *starred,
        },
        MutationCommand::SetRead { read, .. } => MutationCommand::SetRead {
            message_ids,
            read: *read,
        },
        MutationCommand::ModifyLabels { add, remove, .. } => MutationCommand::ModifyLabels {
            message_ids,
            add: add.clone(),
            remove: remove.clone(),
        },
        MutationCommand::Move { target_label, .. } => MutationCommand::Move {
            message_ids,
            target_label: target_label.clone(),
        },
        MutationCommand::Route {
            to_label,
            from_queue_label,
            archive,
            dry_run,
            ..
        } => MutationCommand::Route {
            message_ids,
            to_label: to_label.clone(),
            from_queue_label: from_queue_label.clone(),
            archive: *archive,
            dry_run: *dry_run,
        },
    }
}

fn mutation_job_kind(cmd: &MutationCommand) -> &'static str {
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
        MutationCommand::Route { archive, .. } => Some(if *archive {
            UndoableMutationKind::ReadAndArchive
        } else {
            UndoableMutationKind::Archive
        }),
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
    let entry = state.store.read_undo_entry(mutation_id).await?;
    let Some(entry) = entry else {
        return Err(format!(
            "undo: mutation `{mutation_id}` not found (expired or already undone)"
        )
        .into());
    };
    let now = chrono::Utc::now().timestamp();
    if entry.expires_at <= now {
        let _ = state.store.delete_undo_entry(&entry.mutation_id).await;
        return Err(format!("undo: window expired for mutation `{mutation_id}`").into());
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
        return Err(format!("undo: irreversible ({irreversible} message(s)) — {detail}").into());
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

    // Undo runs under a fresh mutation_id (a retry of undo would be a
    // distinct operation in the user's mind). Dedup against the reverse
    // op's provider call.
    let undo_mutation_id = uuid::Uuid::now_v7().to_string();

    if !to_add.is_empty() || !to_remove.is_empty() {
        provider
            .apply_mutation(
                &undo_mutation_id,
                &mxr_core::Mutation::ModifyLabels {
                    provider_message_id: snapshot.provider_id.clone(),
                    add: to_add.clone(),
                    remove: to_remove.clone(),
                },
            )
            .await
            .map_err(classify_provider_error)?;
        let now = chrono::Utc::now().timestamp();
        if let Err(error) = state
            .store
            .record_mutation_applied(
                &undo_mutation_id,
                &snapshot.provider_id,
                &snapshot.account_id,
                now,
            )
            .await
        {
            tracing::warn!(%error, "undo failed to record dedup row");
        }
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
            // Suffix dedup key so this co-exists with the ModifyLabels
            // call above when undoing a ReadAndArchive.
            let read_dedup_key = format!("{}#read", snapshot.provider_id);
            provider
                .apply_mutation(
                    &undo_mutation_id,
                    &mxr_core::Mutation::SetRead {
                        provider_message_id: snapshot.provider_id.clone(),
                        read: prior_read,
                    },
                )
                .await
                .map_err(classify_provider_error)?;
            let now = chrono::Utc::now().timestamp();
            if let Err(error) = state
                .store
                .record_mutation_applied(
                    &undo_mutation_id,
                    &read_dedup_key,
                    &snapshot.account_id,
                    now,
                )
                .await
            {
                tracing::warn!(%error, "undo failed to record dedup row");
            }
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
        | MutationCommand::Move { message_ids, .. }
        | MutationCommand::Route { message_ids, .. } => message_ids,
    }
}

async fn apply_mutation_to_envelope(
    state: &AppState,
    provider: &dyn mxr_core::MailSyncProvider,
    mutation_id: &str,
    cmd: &MutationCommand,
    envelope: &Envelope,
) -> Result<(), String> {
    let message_id = &envelope.id;
    let provider_id = &envelope.provider_id;
    match cmd {
        MutationCommand::Archive { .. } => {
            apply_one_mutation(
                state,
                provider,
                mutation_id,
                provider_id,
                mxr_core::Mutation::ModifyLabels {
                    provider_message_id: provider_id.clone(),
                    add: vec![],
                    remove: vec!["INBOX".to_string()],
                },
                &envelope.account_id,
            )
            .await?;
            reconcile_label_mutation(state, provider, message_id, &[], &["INBOX".to_string()])
                .await?;
        }
        MutationCommand::ReadAndArchive { .. } => {
            // Two provider calls under one mutation_id; suffix the dedup
            // key so the rows don't collide.
            apply_one_mutation(
                state,
                provider,
                mutation_id,
                &format!("{provider_id}#read"),
                mxr_core::Mutation::SetRead {
                    provider_message_id: provider_id.clone(),
                    read: true,
                },
                &envelope.account_id,
            )
            .await?;
            state
                .store
                .set_read(message_id, true, mxr_core::EventSource::User)
                .await
                .map_err(|e| e.to_string())?;
            apply_one_mutation(
                state,
                provider,
                mutation_id,
                &format!("{provider_id}#labels"),
                mxr_core::Mutation::ModifyLabels {
                    provider_message_id: provider_id.clone(),
                    add: vec![],
                    remove: vec!["INBOX".to_string()],
                },
                &envelope.account_id,
            )
            .await?;
            reconcile_label_mutation(state, provider, message_id, &[], &["INBOX".to_string()])
                .await?;
        }
        MutationCommand::Trash { .. } => {
            apply_one_mutation(
                state,
                provider,
                mutation_id,
                provider_id,
                mxr_core::Mutation::Trash {
                    provider_message_id: provider_id.clone(),
                },
                &envelope.account_id,
            )
            .await?;
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
            apply_one_mutation(
                state,
                provider,
                mutation_id,
                provider_id,
                mxr_core::Mutation::ModifyLabels {
                    provider_message_id: provider_id.clone(),
                    add: vec!["SPAM".to_string()],
                    remove: vec!["INBOX".to_string()],
                },
                &envelope.account_id,
            )
            .await?;
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
            apply_one_mutation(
                state,
                provider,
                mutation_id,
                provider_id,
                mxr_core::Mutation::SetStarred {
                    provider_message_id: provider_id.clone(),
                    starred: *starred,
                },
                &envelope.account_id,
            )
            .await?;
            state
                .store
                .set_starred(message_id, *starred, mxr_core::EventSource::User)
                .await
                .map_err(|e| e.to_string())?;
        }
        MutationCommand::SetRead { read, .. } => {
            apply_one_mutation(
                state,
                provider,
                mutation_id,
                provider_id,
                mxr_core::Mutation::SetRead {
                    provider_message_id: provider_id.clone(),
                    read: *read,
                },
                &envelope.account_id,
            )
            .await?;
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
            apply_one_mutation(
                state,
                provider,
                mutation_id,
                provider_id,
                mxr_core::Mutation::ModifyLabels {
                    provider_message_id: provider_id.clone(),
                    add: resolved_add.clone(),
                    remove: resolved_remove.clone(),
                },
                &envelope.account_id,
            )
            .await?;
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
            apply_one_mutation(
                state,
                provider,
                mutation_id,
                provider_id,
                mxr_core::Mutation::ModifyLabels {
                    provider_message_id: provider_id.clone(),
                    add: resolved_target.clone(),
                    remove: vec!["INBOX".to_string()],
                },
                &envelope.account_id,
            )
            .await?;
            reconcile_label_mutation(
                state,
                provider,
                message_id,
                &resolved_target,
                &["INBOX".to_string()],
            )
            .await?;
        }
        MutationCommand::Route {
            to_label,
            from_queue_label,
            archive,
            dry_run,
            ..
        } => {
            if *dry_run {
                return Ok(());
            }
            let labels = state
                .store
                .list_labels_by_account(&envelope.account_id)
                .await
                .map_err(|e| e.to_string())?;
            let resolved_add = resolve_to_provider_ids(&labels, std::slice::from_ref(to_label));
            let mut remove_refs = vec![from_queue_label.clone()];
            if *archive && !from_queue_label.eq_ignore_ascii_case("INBOX") {
                remove_refs.push("INBOX".to_string());
            }
            let resolved_remove = resolve_to_provider_ids(&labels, &remove_refs);
            apply_one_mutation(
                state,
                provider,
                mutation_id,
                &format!("{provider_id}#route-labels"),
                mxr_core::Mutation::ModifyLabels {
                    provider_message_id: provider_id.clone(),
                    add: resolved_add.clone(),
                    remove: resolved_remove.clone(),
                },
                &envelope.account_id,
            )
            .await?;
            reconcile_label_mutation(state, provider, message_id, &resolved_add, &resolved_remove)
                .await?;
            if *archive {
                apply_one_mutation(
                    state,
                    provider,
                    mutation_id,
                    &format!("{provider_id}#route-read"),
                    mxr_core::Mutation::SetRead {
                        provider_message_id: provider_id.clone(),
                        read: true,
                    },
                    &envelope.account_id,
                )
                .await?;
                state
                    .store
                    .set_read(message_id, true, mxr_core::EventSource::User)
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    // Single point of search-index reconciliation: refresh the indexed envelope so
    // queries (`mxr search ...`) see the new flags/labels immediately.
    reindex_message_in_search(state, message_id).await
}

/// Dedup-aware provider mutation: skips the provider call if a row for
/// `(mutation_id, dedup_key)` already exists in `mutation_dedup_log`,
/// otherwise calls the provider and records the apply.
///
/// `dedup_key` is normally the envelope's provider id, but
/// ReadAndArchive uses suffixed keys (`${pid}#read`, `${pid}#labels`)
/// so its two provider calls don't collide in the dedup table.
async fn apply_one_mutation(
    state: &AppState,
    provider: &dyn mxr_core::MailSyncProvider,
    mutation_id: &str,
    dedup_key: &str,
    mutation: mxr_core::Mutation,
    account_id: &mxr_core::AccountId,
) -> Result<(), String> {
    let already = state
        .store
        .was_mutation_applied(mutation_id, dedup_key)
        .await
        .map_err(|e| e.to_string())?;
    if already {
        tracing::debug!(
            mutation_id,
            dedup_key,
            "mutation already applied within dedup window; skipping provider call"
        );
        return Ok(());
    }
    provider
        .apply_mutation(mutation_id, &mutation)
        .await
        .map_err(|e| e.to_string())?;
    let now = chrono::Utc::now().timestamp();
    if let Err(error) = state
        .store
        .record_mutation_applied(mutation_id, dedup_key, account_id, now)
        .await
    {
        // Non-fatal: provider already applied the mutation. A future
        // retry of the same mutation_id will be safe because Gmail/IMAP
        // ops are set-semantics; logged so it's visible.
        tracing::warn!(%error, mutation_id, dedup_key, "failed to record mutation dedup row");
    }
    Ok(())
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
        MutationCommand::Route {
            to_label,
            from_queue_label,
            archive,
            ..
        } => (
            format!(
                "Routed {} to {}",
                quoted_subject(&envelope.subject),
                to_label
            ),
            Some(format!(
                "from={} queue={} archive={}",
                envelope.from.email, from_queue_label, archive
            )),
        ),
    }
}

pub(super) async fn snooze(
    state: &AppState,
    message_id: &mxr_core::MessageId,
    wake_at: &chrono::DateTime<chrono::Utc>,
) -> HandlerResult {
    apply_snooze(state, message_id, wake_at).await?;
    if let Some(envelope) = state.store.get_envelope(message_id).await? {
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
    let snoozed = state.store.get_snooze(message_id).await?;
    if let Some(snoozed) = snoozed {
        let envelope = state.store.get_envelope(message_id).await?;
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
    let snoozed = state.store.list_snoozed().await?;
    Ok(ResponseData::SnoozedMessages { snoozed })
}

pub(super) async fn list_drafts(state: &AppState) -> HandlerResult {
    let accounts = state.store.list_accounts().await?;
    let mut drafts = Vec::new();
    for account in accounts {
        drafts.extend(state.store.list_drafts(&account.id).await?);
    }
    drafts.sort_by_key(|draft| std::cmp::Reverse(draft.updated_at));
    Ok(ResponseData::Drafts { drafts })
}

/// List drafts presumed orphaned mid-send. Mirrors the cutoff used by
/// the daemon's startup recovery loop (1h since last activity) so the
/// CLI surfaces what would be auto-reset, only earlier.
pub(super) async fn list_orphaned_drafts(state: &AppState) -> HandlerResult {
    let cutoff = chrono::Utc::now() - chrono::Duration::hours(1);
    let orphan_ids = state.store.list_orphaned_sending_drafts(cutoff).await?;
    let mut drafts = Vec::with_capacity(orphan_ids.len());
    for id in &orphan_ids {
        if let Some(draft) = state.store.get_draft(id).await? {
            drafts.push(draft);
        }
    }
    drafts.sort_by_key(|draft| std::cmp::Reverse(draft.updated_at));
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
    state.store.reset_orphaned_draft(draft_id).await?;
    Ok(ResponseData::Ack)
}

pub(super) async fn save_draft(state: &AppState, draft: &Draft) -> HandlerResult {
    state.store.insert_draft(draft).await?;
    Ok(ResponseData::Ack)
}

pub(super) async fn delete_draft(state: &AppState, draft_id: &mxr_core::DraftId) -> HandlerResult {
    state.store.delete_draft(draft_id).await?;
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
        .await?
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
        Ok(body) => (*get_or_render_reply_context(state, message_id, &body)).clone(),
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
        .await?
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
        Ok(body) => (*get_or_render_reply_context(state, message_id, &body)).clone(),
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
        email: account.as_ref().map_or_else(
            || "user@example.com".to_string(),
            |account| account.email.clone(),
        ),
    }
}

pub(super) async fn send_draft(
    state: &AppState,
    draft: &Draft,
    override_safety_token: Option<&str>,
) -> HandlerResult {
    if let Some(receipt) = state.store.get_sent_draft_receipt(&draft.id).await? {
        return Ok(sent_draft_receipt_response(receipt));
    }

    if state.store.get_draft_status(&draft.id).await?.is_none() {
        state.store.insert_draft_if_absent(draft).await?;
    }

    send_stored_draft(state, &draft.id, override_safety_token).await
}

pub(super) async fn schedule_send(
    state: &AppState,
    draft_id: &mxr_core::DraftId,
    send_at: chrono::DateTime<chrono::Utc>,
) -> HandlerResult {
    state.store.schedule_send(draft_id, send_at).await?;
    Ok(ResponseData::Ack)
}

pub(super) async fn cancel_scheduled_send(
    state: &AppState,
    draft_id: &mxr_core::DraftId,
) -> HandlerResult {
    state.store.cancel_scheduled_send(draft_id).await?;
    Ok(ResponseData::Ack)
}

pub(crate) async fn send_stored_draft(
    state: &AppState,
    draft_id: &mxr_core::DraftId,
    override_safety_token: Option<&str>,
) -> HandlerResult {
    if let Some(receipt) = state.store.get_sent_draft_receipt(draft_id).await? {
        return Ok(sent_draft_receipt_response(receipt));
    }

    let draft = state
        .store
        .get_draft(draft_id)
        .await?
        .ok_or_else(|| format!("Draft not found: {draft_id}"))?;

    enforce_draft_safety_with_override(state, &draft, override_safety_token).await?;

    // Compare-and-set: only the unique `Draft` -> `Sending` transition is
    // allowed to invoke the provider. A draft already in `Sending` (likely a
    // crashed prior attempt) or `Sent` (already delivered) refuses.
    let advanced = state
        .store
        .cas_draft_status(draft_id, DraftStatus::Draft, DraftStatus::Sending)
        .await?;
    if !advanced {
        let current = state
            .store
            .get_draft_status(draft_id)
            .await?
            .unwrap_or(DraftStatus::Draft);
        return Err(match current {
            DraftStatus::Sent => {
                crate::handler::HandlerError::Message("draft already sent".to_string())
            }
            DraftStatus::Sending => crate::handler::HandlerError::Message(
                "draft is already being sent (resolve via `mxr drafts resolve`)".to_string(),
            ),
            DraftStatus::Draft => {
                crate::handler::HandlerError::Message("draft state mismatch; retry".to_string())
            }
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
            return Err(crate::handler::HandlerError::Message(e));
        }
    };
    let from = resolve_from_address(state, &draft).await;
    let rfc2822_message_id = match state.store.get_draft_message_id_header(draft_id).await? {
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
                return Err(crate::handler::HandlerError::Message(e.to_string()));
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
            return Err(crate::handler::HandlerError::Message(e.to_string()));
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
            return Err(format!("send succeeded but local ingest failed: {e}").into());
        }
    };

    let _ = state
        .store
        .update_draft_status(draft_id, DraftStatus::Sent)
        .await;
    if let Some(inline_reply) = draft.inline_calendar_reply.as_ref() {
        if let Err(error) = state
            .store
            .update_calendar_invite_partstat(
                &inline_reply.source_message_id,
                &inline_reply.attendee_email,
                inline_reply.partstat.as_ical(),
            )
            .await
        {
            tracing::warn!(%error, "inline invite-reply PARTSTAT update failed after send_stored_draft");
        }
    }
    if let Err(e) =
        super::commitments_extract::promote_after_send(state, &draft, &local_message_id).await
    {
        tracing::warn!(error = %e, "commitments promotion failed after send_stored_draft");
    }
    state
        .store
        .record_sent_draft_receipt(
            draft_id,
            &draft.account_id,
            &local_message_id,
            receipt.provider_message_id.as_deref(),
            &receipt.rfc2822_message_id,
            receipt.sent_at,
        )
        .await
        .map_err(|e| format!("send succeeded but receipt persistence failed: {e}"))?;
    let _ = state.store.delete_draft(draft_id).await;
    Ok(ResponseData::SendReceipt {
        local_message_id,
        provider_message_id: receipt.provider_message_id,
        rfc2822_message_id: receipt.rfc2822_message_id,
    })
}

fn sent_draft_receipt_response(receipt: mxr_store::SentDraftReceipt) -> ResponseData {
    ResponseData::SendReceipt {
        local_message_id: receipt.local_message_id,
        provider_message_id: receipt.provider_message_id,
        rfc2822_message_id: receipt.rfc2822_message_id,
    }
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
        link_count: 0,
        body_word_count: 0,
        label_provider_ids: if provider_namespace == "gmail" {
            vec!["SENT".to_string()]
        } else {
            Vec::new()
        },
        keywords: std::collections::BTreeSet::new(),
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

    // Refresh the now-stale thread summary in the background. Sends
    // are rare enough that the LLM cost doesn't matter, but we spawn
    // fire-and-forget so the user's "sent" confirmation lands
    // immediately rather than waiting on the LLM round-trip.
    if state.config_snapshot().llm.enabled {
        let summary_store = state.store.clone();
        let summary_llm = state.llm.clone();
        let summary_thread_id = thread_id.clone();
        tokio::spawn(async move {
            if let Err(error) = crate::handler::summarize::summarize_thread_cached(
                summary_store,
                summary_llm,
                &summary_thread_id,
            )
            .await
            {
                tracing::warn!(%summary_thread_id, error = %error, "post-send summary refresh failed");
            }
        });
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
        email: account.as_ref().map_or_else(
            || "user@example.com".to_string(),
            |account| account.email.clone(),
        ),
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
        Err(error) => Err(format!("Failed to save draft: {error}").into()),
    }
}

pub(super) async fn unsubscribe_purge(
    state: &AppState,
    address: &str,
    account_id: Option<&mxr_core::AccountId>,
    dry_run: bool,
    archive_on_no_method: bool,
) -> HandlerResult {
    let selection = select_sender_footprint(state, address, account_id).await?;
    let method_envelope = selection
        .envelopes
        .iter()
        .find(|envelope| !matches!(envelope.unsubscribe, UnsubscribeMethod::None))
        .or_else(|| selection.envelopes.first());
    let method = method_envelope.map_or(UnsubscribeMethod::None, |envelope| {
        envelope.unsubscribe.clone()
    });
    let message_ids: Vec<_> = selection
        .envelopes
        .iter()
        .map(|envelope| envelope.id.clone())
        .collect();

    if dry_run {
        return Ok(ResponseData::UnsubscribePurgeResult {
            result: UnsubscribePurgeResultData {
                address: selection.address,
                query: selection.query,
                account_id: account_id.cloned(),
                dry_run: true,
                method,
                status: UnsubscribePurgeStatusData::Preview,
                message_count: message_ids.len() as u32,
                archived_count: 0,
                message_ids,
                mutation_id: None,
                error: None,
            },
        });
    }

    if message_ids.is_empty() {
        return Ok(ResponseData::UnsubscribePurgeResult {
            result: UnsubscribePurgeResultData {
                address: selection.address,
                query: selection.query,
                account_id: account_id.cloned(),
                dry_run: false,
                method,
                status: UnsubscribePurgeStatusData::NoMethod,
                message_count: 0,
                archived_count: 0,
                message_ids,
                mutation_id: None,
                error: Some("No messages matched this sender".to_string()),
            },
        });
    }

    let mut status = UnsubscribePurgeStatusData::Unsubscribed;
    let mut error = None;
    if matches!(method, UnsubscribeMethod::None) {
        if archive_on_no_method {
            status = UnsubscribePurgeStatusData::ArchiveOnly;
            error = Some("No unsubscribe method available; archived sender footprint only".into());
        } else {
            return Ok(ResponseData::UnsubscribePurgeResult {
                result: UnsubscribePurgeResultData {
                    address: selection.address,
                    query: selection.query,
                    account_id: account_id.cloned(),
                    dry_run: false,
                    method,
                    status: UnsubscribePurgeStatusData::NoMethod,
                    message_count: message_ids.len() as u32,
                    archived_count: 0,
                    message_ids,
                    mutation_id: None,
                    error: Some("No unsubscribe method available; rerun with archive-on-no-method to clear the footprint".into()),
                },
            });
        }
    } else if let Some(envelope) = method_envelope {
        if let Err(err) = unsubscribe(state, &envelope.id).await {
            return Ok(ResponseData::UnsubscribePurgeResult {
                result: UnsubscribePurgeResultData {
                    address: selection.address,
                    query: selection.query,
                    account_id: account_id.cloned(),
                    dry_run: false,
                    method,
                    status: UnsubscribePurgeStatusData::Failed,
                    message_count: message_ids.len() as u32,
                    archived_count: 0,
                    message_ids,
                    mutation_id: None,
                    error: Some(err.to_string()),
                },
            });
        }
    }

    let mutation_response = mutation(
        state,
        &MutationCommand::ReadAndArchive {
            message_ids: message_ids.clone(),
        },
        None,
    )
    .await?;
    let (archived_count, mutation_id) = match mutation_response {
        ResponseData::MutationResult { result } => (result.succeeded, result.mutation_id),
        _ => (0, None),
    };

    Ok(ResponseData::UnsubscribePurgeResult {
        result: UnsubscribePurgeResultData {
            address: selection.address,
            query: selection.query,
            account_id: account_id.cloned(),
            dry_run: false,
            method,
            status,
            message_count: message_ids.len() as u32,
            archived_count,
            message_ids,
            mutation_id,
            error,
        },
    })
}

struct SenderFootprintSelection {
    address: String,
    query: String,
    envelopes: Vec<Envelope>,
}

async fn select_sender_footprint(
    state: &AppState,
    address: &str,
    account_id: Option<&mxr_core::AccountId>,
) -> Result<SenderFootprintSelection, crate::handler::HandlerError> {
    let address = address.trim().to_ascii_lowercase();
    if address.is_empty() || !address.contains('@') {
        return Err("unsubscribe purge requires a sender email address".into());
    }
    let query = format!("from:{address}");
    let mut envelopes = Vec::new();
    let mut offset = 0usize;
    const PAGE_SIZE: usize = 500;
    let ast = mxr_search::parse_query(&query).map_err(|e| e.to_string())?;
    let schema = mxr_search::MxrSchema::build();
    loop {
        let query_ast = mxr_search::QueryBuilder::new(&schema).build(&ast);
        let page = state
            .search
            .search_ast(
                query_ast,
                PAGE_SIZE,
                offset,
                mxr_core::types::SortOrder::DateDesc,
            )
            .await?;
        for hit in page.results {
            let message_id: mxr_core::MessageId = hit
                .message_id
                .parse()
                .map_err(|e| format!("invalid search result message id: {e}"))?;
            if let Some(envelope) = state.store.get_envelope(&message_id).await? {
                if account_id.is_none_or(|account_id| envelope.account_id == *account_id)
                    && envelope.from.email.eq_ignore_ascii_case(&address)
                {
                    envelopes.push(envelope);
                }
            }
        }
        match page.next_offset {
            Some(next) if next > offset => offset = next,
            _ => break,
        }
    }
    Ok(SenderFootprintSelection {
        address,
        query,
        envelopes,
    })
}

pub(super) async fn unsubscribe(
    state: &AppState,
    message_id: &mxr_core::MessageId,
) -> HandlerResult {
    let envelope = state
        .store
        .get_envelope(message_id)
        .await?
        .ok_or_else(|| "Message not found".to_string())?;

    // Idempotency: if we already logged a successful unsubscribe for
    // this message, return Ack without re-firing the side effect. The
    // event-log entries written by this same handler ("Unsubscribed
    // from …" / "Sent unsubscribe request for …") both contain the
    // substring "unsubscrib", which is what we match on.
    if state
        .store
        .has_event_for_message_with_summary(&message_id.as_str(), "mutation", "unsubscrib")
        .await?
    {
        return Ok(ResponseData::Ack);
    }

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
                email: account.as_ref().map_or_else(
                    || "user@example.com".to_string(),
                    |account| account.email.clone(),
                ),
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
                inline_calendar_reply: None,
                created_at: now,
                updated_at: now,
            };
            let rfc2822_message_id = mxr_outbound::email::generate_message_id(&from);
            sender.send(&draft, &from, &rfc2822_message_id).await?;
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
                crate::unsubscribe::UnsubscribeResult::Failed(message) => {
                    Err(crate::handler::HandlerError::Message(message))
                }
                crate::unsubscribe::UnsubscribeResult::NoMethod => {
                    Err(crate::handler::HandlerError::Message(
                        "No unsubscribe method available for this message".to_string(),
                    ))
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
                .map_or_else(|| label_ref.clone(), |l| l.provider_id.clone())
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
        .await?
        .ok_or_else(|| format!("Message not found: {message_id}"))?;

    let supported = MessageFlags::READ | MessageFlags::STARRED;
    let unsupported_changed_bits = (envelope.flags.bits() ^ flags.bits()) & !supported.bits();
    if unsupported_changed_bits != 0 {
        return Err(crate::handler::HandlerError::Message(
            "SetFlags only supports provider-routed READ and STARRED changes; use typed mutations"
                .to_string(),
        ));
    }

    let provider = state.get_provider(Some(&envelope.account_id))?;
    let provider_id = envelope.provider_id.clone();
    let mutation_id = uuid::Uuid::now_v7().to_string();

    let read = flags.contains(MessageFlags::READ);
    if envelope.flags.contains(MessageFlags::READ) != read {
        provider
            .apply_mutation(
                &mutation_id,
                &mxr_core::Mutation::SetRead {
                    provider_message_id: provider_id.clone(),
                    read,
                },
            )
            .await?;
        let now = chrono::Utc::now().timestamp();
        if let Err(error) = state
            .store
            .record_mutation_applied(
                &mutation_id,
                &format!("{provider_id}#read"),
                &envelope.account_id,
                now,
            )
            .await
        {
            tracing::warn!(%error, mutation_id, "set_flags failed to record dedup row");
        }
        state
            .store
            .set_read(message_id, read, mxr_core::EventSource::User)
            .await?;
    }

    let starred = flags.contains(MessageFlags::STARRED);
    if envelope.flags.contains(MessageFlags::STARRED) != starred {
        provider
            .apply_mutation(
                &mutation_id,
                &mxr_core::Mutation::SetStarred {
                    provider_message_id: provider_id.clone(),
                    starred,
                },
            )
            .await?;
        let now = chrono::Utc::now().timestamp();
        if let Err(error) = state
            .store
            .record_mutation_applied(
                &mutation_id,
                &format!("{provider_id}#starred"),
                &envelope.account_id,
                now,
            )
            .await
        {
            tracing::warn!(%error, mutation_id, "set_flags failed to record dedup row");
        }
        state
            .store
            .set_starred(message_id, starred, mxr_core::EventSource::User)
            .await?;
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

#[cfg(test)]
mod safety_context_wiring_tests {
    //! Slice 1.3 integration tests for `build_safety_context`.
    //!
    //! These exercise the daemon through the public IPC handler
    //! `check_draft_safety_request`. They assert the *observable*
    //! consequence of populating each `SafetyContext` field: the safety
    //! report contains the issue that the field's data should trigger.
    //! If any of these regress, the deterministic check (recipients /
    //! reply-all / tone) is running on an empty context.
    use super::*;
    use crate::state::AppState;
    use chrono::Utc;
    use mxr_core::types::{Address, ContactRow, DraftIntent};
    use mxr_core::{AccountId, DraftId, MessageId, ThreadId};
    use mxr_protocol::{DraftSafetyContextData, DraftSafetyModeData};
    use std::sync::Arc;

    fn draft_to(account_id: AccountId, to: Vec<Address>, body: &str) -> Draft {
        Draft {
            id: DraftId::new(),
            account_id,
            reply_headers: None,
            intent: DraftIntent::New,
            to,
            cc: Vec::new(),
            bcc: Vec::new(),
            subject: "subject".into(),
            body_markdown: body.into(),
            attachments: Vec::new(),
            inline_calendar_reply: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn addr(email: &str, name: Option<&str>) -> Address {
        Address {
            email: email.into(),
            name: name.map(str::to_string),
        }
    }

    fn check_context() -> DraftSafetyContextData {
        DraftSafetyContextData {
            mode: DraftSafetyModeData::Check,
            reply_all: false,
            original_message_id: None,
            thread_id: None,
            allow_llm: false,
            proposed_send_at: None,
        }
    }

    fn report(resp: ResponseData) -> DraftSafetyReport {
        match resp {
            ResponseData::DraftSafetyReportResponse { report } => report,
            other => panic!("expected DraftSafetyReportResponse, got {other:?}"),
        }
    }

    /// known_contacts wiring: seed a strong contact for `alice@example.com`,
    /// draft to `alcie@example.com` (one-edit transposition). The
    /// recipient typo check must see the strong contact and emit a
    /// WrongRecipient warning naming both addresses.
    ///
    /// FAILS today: build_safety_context returns `known_contacts =
    /// Vec::new()` so `best_typo_candidate` finds no candidates.
    #[tokio::test]
    async fn known_contacts_loaded_emits_typo_warning() {
        let (state, _fake) = AppState::in_memory_with_fake().await.unwrap();
        let state = Arc::new(state);
        let account_id = state.store.list_accounts().await.unwrap()[0].id.clone();
        let now = Utc::now();

        state
            .store
            .upsert_contact(&ContactRow {
                account_id: account_id.clone(),
                email: "alice@example.com".into(),
                display_name: Some("Alice".into()),
                first_seen_at: now,
                last_seen_at: now,
                last_inbound_at: Some(now),
                last_outbound_at: Some(now),
                total_inbound: 12,
                total_outbound: 6,
                replied_count: 6,
                cadence_days_p50: None,
            })
            .await
            .unwrap();

        let draft = draft_to(
            account_id.clone(),
            vec![addr("alcie@example.com", None)],
            "Hi,\n\nFollowing up.",
        );
        let resp = check_draft_safety_request(&state, &draft, &check_context())
            .await
            .expect("check_draft_safety_request");
        let r = report(resp);
        let typo = r.issues.iter().find(|i| {
            i.code == DraftSafetyIssueCode::WrongRecipient
                && i.severity == DraftSafetySeverity::Warning
        });
        let Some(typo) = typo else {
            panic!(
                "expected WrongRecipient warning, got issues: {:?}",
                r.issues
            );
        };
        assert!(
            typo.message.contains("alcie@example.com"),
            "warning omits typed recipient: {}",
            typo.message
        );
        assert!(
            typo.message.contains("alice@example.com"),
            "warning omits suggested candidate: {}",
            typo.message
        );
    }

    /// thread_display_names wiring: seed a thread whose envelope's
    /// `from.name` is "Sam", then issue a reply-all draft of size >2
    /// with body "Hi Sam,". The reply-all check should treat Sam as
    /// thread context (ambiguous) and NOT warn.
    ///
    /// FAILS today: build_safety_context returns `thread_display_names
    /// = Vec::new()`, so the reply_all check fires the warning.
    #[tokio::test]
    async fn thread_display_names_loaded_suppresses_reply_all_warning() {
        let (state, _fake) = AppState::in_memory_with_fake().await.unwrap();
        let state = Arc::new(state);
        let account_id = state.store.list_accounts().await.unwrap()[0].id.clone();

        let thread_id = ThreadId::new();
        let envelope = mxr_core::types::Envelope {
            id: MessageId::new(),
            account_id: account_id.clone(),
            provider_id: "sam-1".into(),
            thread_id: thread_id.clone(),
            message_id_header: Some("<sam-1@example.com>".into()),
            in_reply_to: None,
            references: vec![],
            from: Address {
                email: "sam@example.com".into(),
                name: Some("Sam Carter".into()),
            },
            to: vec![Address {
                email: "user@example.com".into(),
                name: None,
            }],
            cc: vec![],
            bcc: vec![],
            subject: "let's sync".into(),
            date: Utc::now(),
            flags: mxr_core::types::MessageFlags::empty(),
            snippet: "ping".into(),
            has_attachments: false,
            size_bytes: 256,
            unsubscribe: mxr_core::types::UnsubscribeMethod::None,
            link_count: 0,
            body_word_count: 0,
            label_provider_ids: vec![],
            keywords: std::collections::BTreeSet::new(),
        };
        state
            .store
            .upsert_envelope_with_direction(&envelope, MessageDirection::Inbound)
            .await
            .unwrap();

        let draft = draft_to(
            account_id.clone(),
            vec![
                addr("sam@example.com", Some("Sam Carter")),
                addr("dave@example.com", Some("Dave")),
                addr("eve@example.com", Some("Eve")),
                addr("frank@example.com", Some("Frank")),
            ],
            "Hi Sam,\n\nThanks, that works.",
        );
        let mut context = check_context();
        context.reply_all = true;
        context.thread_id = Some(thread_id);

        let resp = check_draft_safety_request(&state, &draft, &context)
            .await
            .expect("check_draft_safety_request");
        let r = report(resp);
        let reply_all_warns: Vec<_> = r
            .issues
            .iter()
            .filter(|i| i.code == DraftSafetyIssueCode::ReplyAll)
            .collect();
        assert!(
            reply_all_warns.is_empty(),
            "Sam is a thread participant; reply-all warning should be suppressed, got {reply_all_warns:?}"
        );
    }

    /// contact_styles wiring: seed a high-confidence baseline for
    /// `alice@example.com` skewed casual, draft her in formal voice.
    /// The tone check should fire a ToneMismatch warning.
    ///
    /// FAILS today: build_safety_context returns `contact_styles =
    /// Vec::new()`, so the tone check short-circuits at the top.
    #[tokio::test]
    async fn contact_styles_loaded_emits_tone_warning() {
        use mxr_relationship::StylometryMetrics;
        let (state, _fake) = AppState::in_memory_with_fake().await.unwrap();
        let state = Arc::new(state);
        let account_id = state.store.list_accounts().await.unwrap()[0].id.clone();

        // Seed a casual baseline (low formality_score) with high sample.
        let their_baseline = StylometryMetrics {
            formality_score: 0.1,
            avg_sentence_len: 6.0,
            sentence_count: 10,
            word_count: 60,
            ..Default::default()
        };
        let metrics_json_theirs = serde_json::to_string(&their_baseline).unwrap();
        state
            .store
            .upsert_contact_style(&mxr_store::ContactStyleRecord {
                account_id: account_id.clone(),
                email: "alice@example.com".into(),
                formality_score: 0.0,
                formality_score_theirs: 0.1,
                avg_sentence_len: 0.0,
                avg_sentence_len_theirs: 6.0,
                msg_count_used: 0,
                msg_count_used_theirs: 30,
                metrics_json: "{}".into(),
                metrics_json_theirs,
                computed_at: Utc::now(),
                source_hash: "test".into(),
                drift_detected: false,
                drift_reason: None,
                drift_detected_at: None,
            })
            .await
            .unwrap();

        let very_formal = "Dear Alice,\n\nFurthermore, I would respectfully request that you kindly furnish the aforementioned documentation at your earliest convenience.\n\nSincerely,\nMe";
        let draft = draft_to(
            account_id.clone(),
            vec![addr("alice@example.com", None)],
            very_formal,
        );
        let resp = check_draft_safety_request(&state, &draft, &check_context())
            .await
            .expect("check_draft_safety_request");
        let r = report(resp);
        let tone = r
            .issues
            .iter()
            .find(|i| i.code == DraftSafetyIssueCode::ToneMismatch);
        assert!(
            tone.is_some(),
            "expected ToneMismatch warning, got issues: {:?}",
            r.issues
        );
    }
}
