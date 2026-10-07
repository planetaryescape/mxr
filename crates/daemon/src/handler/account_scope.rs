//! Which accounts a request touches, for the agent and MCP profiles'
//! `allowed_accounts`.
//!
//! `request_scope` is an exhaustive match with no catch-all arm, so a new
//! `Request` variant does not compile until someone decides what it touches.
//! The old hand-kept list ended in `_ => None`, which let every request it
//! forgot through unchecked.

use super::{records, request_kind, todos};
use crate::state::AppState;
use mxr_config::AgentProfileConfig;
use mxr_core::id::{AccountId, DeliveryId, DraftId, LabelId, MessageId, ThreadId};
use mxr_core::types::Draft;
use mxr_core::types::{Envelope, MessageFlags, Thread};
use mxr_protocol::{
    AuthSessionId, ClientKind, JobData, MutationCommand, PromiseSourceData, Request, ResponseData,
    ThreadSummaryData,
};
use std::sync::Arc;

/// What a request reads or changes, as far as accounts go.
#[derive(Debug)]
enum RequestScope<'a> {
    /// Touches no account's mail or account data: liveness, the daemon
    /// version handshake, text-only tools, UI hints.
    Unscoped,
    /// Reads or changes data across every account, or daemon-wide data
    /// that mixes accounts (logs, rules, the activity log). A scoped agent
    /// can't be given part of it, so it is denied.
    AllAccounts,
    /// Touches exactly these accounts, named directly or through the
    /// entities they own. An empty list touches no account.
    Targets(Vec<ScopeTarget<'a>>),
}

/// One thing a request names, to be resolved to its account.
#[derive(Debug)]
enum ScopeTarget<'a> {
    Account(&'a AccountId),
    /// A config account key; matched against the allowlist as written.
    AccountKey(&'a str),
    AuthSession(&'a AuthSessionId),
    /// A label that must belong to the account named beside it: listing by
    /// label ignores the account, so a foreign label would list another
    /// account's mail.
    LabelIn(&'a LabelId, &'a AccountId),
    Message(&'a MessageId),
    Thread(&'a ThreadId),
    Draft(&'a DraftId),
    /// A draft sent by value: its own `account_id`, plus the stored draft
    /// with the same id when there is one, so a write can't overwrite
    /// another account's draft by claiming a different account.
    DraftBody(&'a Draft),
    /// A to-do id or unique prefix.
    Todo(&'a str),
    /// An Archive record id or unique prefix.
    Record(&'a str),
    Delivery(&'a DeliveryId),
    Commitment(&'a str),
    Decision(&'a str),
    Undo(&'a str),
    Job(&'a str),
}

use RequestScope::{AllAccounts, Targets, Unscoped};
use ScopeTarget as T;

fn account(account_id: &AccountId) -> RequestScope<'_> {
    Targets(vec![T::Account(account_id)])
}

/// `None` means every account.
fn optional_account(account_id: Option<&AccountId>) -> RequestScope<'_> {
    account_id.map_or(AllAccounts, account)
}

fn messages(message_ids: &[MessageId]) -> RequestScope<'_> {
    Targets(message_ids.iter().map(T::Message).collect())
}

fn threads(thread_ids: &[ThreadId]) -> RequestScope<'_> {
    Targets(thread_ids.iter().map(T::Thread).collect())
}

fn mutation_messages(mutation: &MutationCommand) -> &[MessageId] {
    match mutation {
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

fn request_scope(req: &Request) -> RequestScope<'_> {
    match req {
        // ----- Named by account -----
        Request::ListEnvelopes {
            account_id,
            label_id,
            ..
        }
        | Request::ListThreads {
            account_id,
            label_id,
            ..
        } => match (account_id, label_id) {
            (None, _) => AllAccounts,
            (Some(account_id), None) => account(account_id),
            (Some(account_id), Some(label_id)) => Targets(vec![
                T::Account(account_id),
                T::LabelIn(label_id, account_id),
            ]),
        },
        Request::ListInvites { account_id, .. }
        | Request::BackfillCalendarInvites { account_id }
        | Request::ListLabels { account_id }
        | Request::CreateLabel { account_id, .. }
        | Request::DeleteLabel { account_id, .. }
        | Request::RenameLabel { account_id, .. }
        | Request::CreateSavedSearch { account_id, .. }
        | Request::RunSavedSearch { account_id, .. }
        | Request::ListSubscriptions { account_id, .. }
        | Request::ListStorageBreakdown { account_id, .. }
        | Request::ListLargestMessages { account_id, .. }
        | Request::Wrapped { account_id, .. }
        | Request::ListStaleThreads { account_id, .. }
        | Request::ListContactAsymmetry { account_id, .. }
        | Request::ListContactDecay { account_id, .. }
        | Request::ListResponseTime { account_id, .. }
        | Request::Search { account_id, .. }
        | Request::SyncNow { account_id, .. }
        | Request::Count { account_id, .. }
        | Request::SearchAggregation { account_id, .. }
        | Request::UnsubscribePurge { account_id, .. }
        | Request::ListScheduledSends { account_id }
        | Request::ListDeliveries { account_id, .. }
        | Request::ScanDeliveries { account_id, .. }
        | Request::SetSignatureDefault { account_id, .. }
        | Request::ClearSignatureDefault { account_id, .. }
        | Request::ResolveSignature { account_id, .. }
        | Request::ListSenders { account_id, .. }
        | Request::TriageSearch { account_id, .. }
        | Request::DraftEval { account_id, .. }
        | Request::ExportSearch { account_id, .. }
        | Request::GetDesk { account_id, .. }
        | Request::GetTodoRunway { account_id, .. }
        | Request::ListTodos { account_id, .. }
        | Request::GetTodoCatchup { account_id }
        | Request::SetTodoCatchup { account_id, .. }
        | Request::GetNow { account_id }
        | Request::GetRail { account_id }
        | Request::ListMessages { account_id, .. }
        | Request::ListMergeSuggestions { account_id }
        | Request::ListRecords { account_id, .. }
        | Request::AnswerFromRecords { account_id, .. }
        | Request::ExportRecords { account_id, .. }
        | Request::ListPlace { account_id, .. }
        | Request::SweepPlace { account_id, .. } => optional_account(account_id.as_ref()),
        Request::ArchiveAsk { filters, .. } => optional_account(filters.account_id.as_ref()),
        Request::GetPerson {
            account_id, topic, ..
        } => match account_id {
            None => AllAccounts,
            Some(account_id) => {
                let mut targets = vec![T::Account(account_id)];
                targets.extend(topic.as_ref().map(T::Thread));
                Targets(targets)
            }
        },
        Request::ListAccountAddresses { account_id }
        | Request::AddAccountAddress { account_id, .. }
        | Request::RemoveAccountAddress { account_id, .. }
        | Request::SetPrimaryAccountAddress { account_id, .. }
        | Request::ResolveSendFrom { account_id, .. }
        | Request::GetSyncStatus { account_id }
        | Request::GetSenderProfile { account_id, .. }
        | Request::GetRelationshipProfile { account_id, .. }
        | Request::RebuildRelationshipProfile { account_id, .. }
        | Request::ListCommitments { account_id, .. }
        | Request::GetUserVoice { account_id }
        | Request::RebuildUserVoice { account_id }
        | Request::ListScreenerQueue { account_id, .. }
        | Request::ListScreenerDecisions { account_id }
        | Request::SetScreenerDecision { account_id, .. }
        | Request::ClearScreenerDecision { account_id, .. }
        | Request::ExplainEntity { account_id, .. }
        | Request::FindExpert { account_id, .. }
        | Request::GetRecipientBriefing { account_id, .. }
        | Request::WatchCadence { account_id, .. }
        | Request::UnwatchCadence { account_id, .. }
        | Request::ListCadenceWatch { account_id }
        | Request::ListCadenceDrift { account_id }
        | Request::SendTimeRecommendation { account_id, .. }
        | Request::RebuildDecisionLog { account_id, .. }
        | Request::ListDecisionLog { account_id, .. }
        | Request::ListOwedReplies { account_id, .. }
        | Request::MergePeople { account_id, .. }
        | Request::SplitPerson { account_id, .. }
        | Request::SetSenderKind { account_id, .. } => account(account_id),

        // ----- Account config, named by key -----
        Request::AuthorizeAccountConfig { account, .. }
        | Request::StartAuthSession { account, .. }
        | Request::UpsertAccountConfig { account }
        | Request::TestAccountConfig { account }
        | Request::RepairAccountConfig { account } => {
            Targets(vec![T::AccountKey(account.key.as_str())])
        }
        Request::SetDefaultAccount { key }
        | Request::DisableAccountConfig { key }
        | Request::RemoveAccountConfig { key, .. } => Targets(vec![T::AccountKey(key.as_str())]),
        Request::GetAuthSession { session_id }
        | Request::CancelAuthSession { session_id }
        | Request::CompleteAuthSession { session_id, .. } => {
            Targets(vec![T::AuthSession(session_id)])
        }

        // ----- Named by message -----
        Request::GetEnvelope { message_id }
        | Request::GetBody { message_id }
        | Request::GetInvite { message_id }
        | Request::RespondInvite { message_id, .. }
        | Request::PrepareInviteResponse { message_id, .. }
        | Request::MarkInviteAnswered { message_id, .. }
        | Request::GetHtmlImageAssets { message_id, .. }
        | Request::DownloadAttachment { message_id, .. }
        | Request::OpenAttachment { message_id, .. }
        | Request::SetFlags { message_id, .. }
        | Request::GetHeaders { message_id }
        | Request::Unsubscribe { message_id }
        | Request::Snooze { message_id, .. }
        | Request::Unsnooze { message_id }
        | Request::SetReplyLater { message_id, .. }
        | Request::PrepareReply { message_id, .. }
        | Request::PrepareForward { message_id }
        | Request::RecordPromise { message_id, .. }
        | Request::CreateTodo { message_id, .. }
        | Request::FileRecord { message_id, .. }
        | Request::SetRecordSender { message_id, .. }
        | Request::GetMessageKind { message_id } => Targets(vec![T::Message(message_id)]),
        Request::SetAutoReminder {
            sent_message_id, ..
        }
        | Request::CancelAutoReminder { sent_message_id } => {
            Targets(vec![T::Message(sent_message_id)])
        }
        Request::ListEnvelopesByIds { message_ids }
        | Request::ListBodies { message_ids }
        | Request::PinMessages { message_ids, .. } => messages(message_ids),
        Request::Mutation { mutation, .. } | Request::StartMutationJob { mutation, .. } => {
            messages(mutation_messages(mutation))
        }
        Request::DetectPromises { source, .. } => match source {
            PromiseSourceData::Draft { draft } => Targets(vec![T::DraftBody(draft)]),
            PromiseSourceData::SentMessage { message_id } => Targets(vec![T::Message(message_id)]),
        },

        // ----- Named by thread -----
        Request::GetThread { thread_id }
        | Request::SummarizeThread { thread_id }
        | Request::GetThreadBriefing { thread_id, .. }
        | Request::ExportThread { thread_id, .. }
        | Request::GetThreadContext { thread_id }
        | Request::GetThreadGist { thread_id, .. }
        | Request::AckMessage { thread_id, .. } => Targets(vec![T::Thread(thread_id)]),
        Request::DismissDeskThreads { thread_ids, .. }
        | Request::RestoreDeskThreads { thread_ids }
        | Request::GetThreadGists { thread_ids, .. }
        | Request::DeferThreads { thread_ids, .. } => threads(thread_ids),
        Request::ResolveDeskItems { items, .. } => Targets(
            items
                .iter()
                .flat_map(|item| {
                    std::iter::once(T::Thread(&item.thread_id))
                        .chain(item.commitment_id.as_deref().map(T::Commitment))
                })
                .collect(),
        ),
        Request::GetModeMembership {
            message_id,
            thread_id,
            thread_ids,
        } => {
            let mut targets: Vec<_> = thread_ids.iter().map(T::Thread).collect();
            targets.extend(thread_id.as_ref().map(T::Thread));
            targets.extend(message_id.as_ref().map(T::Message));
            Targets(targets)
        }
        Request::SetModeDone {
            thread_ids,
            todo_ids,
            sender,
            ..
        } => {
            let mut targets: Vec<_> = thread_ids.iter().map(T::Thread).collect();
            targets.extend(todo_ids.iter().map(|id| T::Todo(id)));
            targets.extend(sender.as_ref().map(|sender| T::Account(&sender.account_id)));
            Targets(targets)
        }

        // ----- Named by draft -----
        Request::DraftCompose {
            account_id,
            source_message_id,
            thread_id,
            ..
        } => {
            let mut targets: Vec<_> = account_id.iter().map(T::Account).collect();
            targets.extend(source_message_id.as_ref().map(T::Message));
            targets.extend(thread_id.as_ref().map(T::Thread));
            // A new message with no account falls back to the default
            // account, which a scoped agent did not name.
            if targets.is_empty() {
                AllAccounts
            } else {
                Targets(targets)
            }
        }
        Request::SendDraft { draft, .. }
        | Request::SaveDraft { draft }
        | Request::UpdateDraft { draft }
        | Request::SaveDraftToServer { draft }
        | Request::CheckDraftSafety { draft, .. }
        | Request::ExtractDraftCommitments { draft }
        | Request::SuggestCollaborators { draft, .. } => Targets(vec![T::DraftBody(draft)]),
        Request::DraftRefine { draft_id, .. }
        | Request::SendStoredDraft { draft_id, .. }
        | Request::ScheduleSend { draft_id, .. }
        | Request::CancelScheduledSend { draft_id }
        | Request::DeleteDraft { draft_id }
        | Request::GetDraft { draft_id }
        | Request::ResetOrphanedDraft { draft_id } => Targets(vec![T::Draft(draft_id)]),

        // ----- Named by another entity -----
        Request::GetTodo { todo_id }
        | Request::ScheduleTodo { todo_id, .. }
        | Request::UpdateTodo { todo_id, .. } => Targets(vec![T::Todo(todo_id)]),
        Request::SetTodoState { todo_ids, .. } => {
            Targets(todo_ids.iter().map(|id| T::Todo(id)).collect())
        }
        Request::GetRecord { record_id } | Request::SetRecordField { record_id, .. } => {
            Targets(vec![T::Record(record_id)])
        }
        Request::DismissRecord { record_ids, .. } => {
            Targets(record_ids.iter().map(|id| T::Record(id)).collect())
        }
        Request::GetDelivery { delivery_id }
        | Request::ResolveDelivery { delivery_id }
        | Request::DismissDelivery { delivery_id } => Targets(vec![T::Delivery(delivery_id)]),
        Request::ResolveCommitment { commitment_id } => {
            Targets(vec![T::Commitment(commitment_id)])
        }
        Request::GetDecision { id } => Targets(vec![T::Decision(id)]),
        Request::UndoMutation { mutation_id } => Targets(vec![T::Undo(mutation_id)]),
        Request::GetJob { job_id } => Targets(vec![T::Job(job_id)]),

        // ----- Every account, or daemon-wide data that mixes them -----
        Request::ListAccounts
        | Request::ListAccountsConfig
        | Request::ListRules
        | Request::GetRule { .. }
        | Request::GetRuleForm { .. }
        | Request::UpsertRule { .. }
        | Request::UpsertRuleForm { .. }
        | Request::DeleteRule { .. }
        | Request::DryRunRules { .. }
        | Request::ListRuleHistory { .. }
        | Request::ListSavedSearches
        | Request::ListSavedSearchUnreadCounts
        | Request::DeleteSavedSearch { .. }
        | Request::UpdateSavedSearch { .. }
        | Request::RefreshContacts
        | Request::RebuildAnalytics
        | Request::RecomputeLinkCounts
        // Decides which model every account's mail is sent to.
        | Request::UpdateLlmConfig { .. }
        | Request::EnableSemantic { .. }
        | Request::InstallSemanticProfile { .. }
        | Request::UseSemanticProfile { .. }
        | Request::ReindexSemantic { .. }
        | Request::BackfillSemantic
        | Request::ListEvents { .. }
        | Request::GetLogs { .. }
        | Request::ListEventCategories
        | Request::CountEvents { .. }
        | Request::GetDoctorReport
        | Request::GenerateBugReport { .. }
        | Request::ListJobs
        | Request::ListSnoozed
        | Request::ListReplyQueue
        | Request::ListSignatureDefaults
        // Signatures are shared by every account's defaults.
        | Request::SetSignature { .. }
        | Request::DeleteSignature { .. }
        | Request::ListDrafts
        | Request::ListOrphanedDrafts
        | Request::Shutdown
        | Request::ListActivity { .. }
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
        | Request::DeleteSavedActivityFilter { .. } => AllAccounts,

        // ----- No account data -----
        // Status lists account names and sync health but no mail, and every
        // client calls it as the daemon version handshake; the MCP server's
        // status tool depends on it.
        Request::GetStatus
        | Request::Ping
        | Request::Authenticate { .. }
        | Request::GetLlmStatus
        | Request::GetLlmConfig
        | Request::GetNotificationChimes
        | Request::UpdateNotificationChimes { .. }
        | Request::PatchNotificationChimes { .. }
        | Request::PreviewNotificationChime { .. }
        | Request::GetSemanticStatus
        | Request::ListSnippets
        | Request::SetSnippet { .. }
        | Request::DeleteSnippet { .. }
        | Request::ListSignatures
        | Request::HumanizerScore { .. }
        | Request::HumanizerRewrite { .. }
        | Request::ResolveTime { .. }
        | Request::GetModeGuide { .. }
        | Request::SetModeGuideSeen { .. } => Unscoped,
    }
}

/// The accounts a request's targets resolve to.
#[derive(Debug, Default)]
struct ResolvedAccounts {
    keys: Vec<String>,
    accounts: Vec<AccountId>,
}

impl ResolvedAccounts {
    fn push(&mut self, account_id: AccountId) {
        if !self.accounts.contains(&account_id) {
            self.accounts.push(account_id);
        }
    }
}

/// Look up the account behind every target. A target that names something
/// that doesn't exist is an error: the request is denied rather than let
/// through with nothing checked.
async fn resolve_targets(
    state: &Arc<AppState>,
    targets: &[ScopeTarget<'_>],
) -> Result<ResolvedAccounts, String> {
    let mut resolved = ResolvedAccounts::default();
    let mut thread_ids = Vec::new();
    let mut todo_ids = Vec::new();
    let mut record_ids = Vec::new();
    for target in targets {
        match target {
            T::Account(account_id) => resolved.push((*account_id).clone()),
            T::AccountKey(key) => resolved.keys.push((*key).to_string()),
            T::AuthSession(session_id) => {
                let key = state
                    .auth_sessions
                    .lock()
                    .get(*session_id)
                    .map(|session| session.account.key.clone())
                    .ok_or_else(|| format!("auth session '{}' not found", session_id.0))?;
                resolved.keys.push(key);
            }
            T::LabelIn(label_id, account_id) => {
                let labels = state
                    .store
                    .list_labels_by_account(account_id)
                    .await
                    .map_err(|e| e.to_string())?;
                if !labels.iter().any(|label| &label.id == *label_id) {
                    return Err(format!("Label {label_id} is not in account {account_id}"));
                }
            }
            T::Message(message_id) => resolved.push(message_account(state, message_id).await?),
            T::Thread(thread_id) => thread_ids.push((*thread_id).clone()),
            T::Draft(draft_id) => {
                let draft = state
                    .store
                    .get_draft(draft_id)
                    .await
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("Draft not found: {draft_id}"))?;
                resolved.push(draft.account_id);
            }
            T::DraftBody(draft) => {
                resolved.push(draft.account_id.clone());
                if let Some(stored) = state
                    .store
                    .get_draft(&draft.id)
                    .await
                    .map_err(|e| e.to_string())?
                {
                    resolved.push(stored.account_id);
                }
            }
            T::Todo(id) => todo_ids.push((*id).to_string()),
            T::Record(id) => record_ids.push((*id).to_string()),
            T::Delivery(delivery_id) => {
                let delivery = state
                    .store
                    .get_delivery(delivery_id)
                    .await
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("Delivery not found: {delivery_id}"))?;
                resolved.push(delivery.account_id);
            }
            T::Commitment(id) => {
                let commitment = state
                    .store
                    .get_contact_commitment(id)
                    .await
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("Commitment not found: {id}"))?;
                resolved.push(commitment.account_id);
            }
            T::Decision(id) => {
                let decision = state
                    .store
                    .get_decision(id)
                    .await
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("Decision not found: {id}"))?;
                resolved.push(decision.account_id);
            }
            T::Undo(mutation_id) => {
                resolve_undo(state, mutation_id, &mut resolved, &mut todo_ids).await?;
            }
            T::Job(job_id) => {
                for account_id in job_accounts(state, job_id).await? {
                    resolved.push(account_id);
                }
            }
        }
    }

    if !thread_ids.is_empty() {
        for account_id in thread_accounts(state, &thread_ids).await? {
            resolved.push(account_id);
        }
    }
    if !todo_ids.is_empty() {
        for account_id in todos::todo_accounts(state, &todo_ids)
            .await
            .map_err(|error| error.to_string())?
        {
            resolved.push(account_id);
        }
    }
    if !record_ids.is_empty() {
        for account_id in records::record_accounts(state, &record_ids)
            .await
            .map_err(|error| error.to_string())?
        {
            resolved.push(account_id);
        }
    }
    Ok(resolved)
}

async fn message_account(state: &AppState, message_id: &MessageId) -> Result<AccountId, String> {
    state
        .store
        .get_envelope(message_id)
        .await
        .map_err(|e| e.to_string())?
        .map(|envelope| envelope.account_id)
        .ok_or_else(|| format!("Message not found: {message_id}"))
}

/// Every account holding each thread. Legacy Gmail thread ids aren't
/// account-scoped, so one id can span accounts; all of them must be in
/// scope, not just the one the thread row reports.
pub(super) async fn thread_accounts(
    state: &AppState,
    thread_ids: &[ThreadId],
) -> Result<Vec<AccountId>, String> {
    let pairs = state
        .store
        .thread_account_pairs(thread_ids)
        .await
        .map_err(|e| e.to_string())?;
    if let Some(missing) = thread_ids
        .iter()
        .find(|id| !pairs.iter().any(|(thread_id, _)| thread_id == *id))
    {
        return Err(format!("Thread not found: {missing}"));
    }
    Ok(pairs
        .into_iter()
        .map(|(_, account_id)| account_id)
        .collect())
}

/// An undo puts back messages, desk dismissals, promises, mode-done marks
/// and to-dos; every one of them must be in scope.
async fn resolve_undo(
    state: &Arc<AppState>,
    mutation_id: &str,
    resolved: &mut ResolvedAccounts,
    todo_ids: &mut Vec<String>,
) -> Result<(), String> {
    let entry = state
        .store
        .read_undo_entry(mutation_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| {
            format!("undo: mutation `{mutation_id}` not found (expired or already undone)")
        })?;
    for snapshot in entry.snapshots {
        resolved.push(snapshot.account_id);
    }
    let Some(desk) = entry.desk else {
        return Ok(());
    };
    for dismissal in desk.dismissals {
        resolved.push(dismissal.account_id);
    }
    for commitment in desk.commitments {
        resolved.push(commitment.account_id);
    }
    for mark in desk.mode_done {
        resolved.push(mark.account_id);
    }
    let flagged = desk
        .reply_later
        .iter()
        .map(|(id, _)| id)
        .chain(desk.reply_later_priors.iter().map(|(id, _)| id))
        .chain(desk.reminder_priors.iter().map(|(id, _)| id));
    for message_id in flagged {
        resolved.push(message_account(state, message_id).await?);
    }
    todo_ids.extend(desk.todos_ticked.into_iter().map(|tick| tick.id));
    Ok(())
}

/// A job only knows its accounts once its result is in; until then it
/// can't be attributed, so a scoped agent can't read it.
async fn job_accounts(state: &Arc<AppState>, job_id: &str) -> Result<Vec<AccountId>, String> {
    let json = state
        .store
        .get_mutation_job(job_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("job not found: {job_id}"))?;
    let job = serde_json::from_str::<JobData>(&json).map_err(|e| e.to_string())?;
    let result = job
        .result
        .ok_or_else(|| format!("job `{job_id}` has not reported which accounts it touches yet"))?;
    Ok(result
        .accounts
        .into_iter()
        .map(|account| account.account_id)
        .collect())
}

pub(super) async fn enforce_account_allowlist(
    state: &Arc<AppState>,
    profile_name: &str,
    profile: &AgentProfileConfig,
    req: &Request,
) -> Result<(), String> {
    let targets = match request_scope(req) {
        Unscoped => return Ok(()),
        AllAccounts => {
            return Err(format!(
                "Request `{}` rejected by {profile_name} profile account allowlist; it spans every account, so name an allowed account where the request takes one",
                request_kind(req)
            ))
        }
        Targets(targets) => targets,
    };
    let resolved = resolve_targets(state, &targets).await?;
    let denied = || {
        format!(
            "Request `{}` rejected by {profile_name} profile account allowlist",
            request_kind(req)
        )
    };
    for key in &resolved.keys {
        if !account_token_allowed(profile, key) {
            return Err(denied());
        }
    }
    for account_id in &resolved.accounts {
        if !account_id_allowed(state, profile, account_id).await? {
            return Err(denied());
        }
    }
    Ok(())
}

async fn account_id_allowed(
    state: &AppState,
    profile: &AgentProfileConfig,
    account_id: &AccountId,
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

/// Which profile governs a client kind, if any.
pub(crate) fn profile_name(source: ClientKind) -> Option<&'static str> {
    match source {
        ClientKind::Agent | ClientKind::Mcp => Some(source.as_str()),
        ClientKind::Human
        | ClientKind::Tui
        | ClientKind::Cli
        | ClientKind::Script
        | ClientKind::Web
        | ClientKind::Daemon => None,
    }
}

/// Cut a response down to what a scoped profile may see. Request checks
/// already keep a scoped client to its accounts; this covers responses that
/// are allowed but carry other accounts' rows, and is a second safety net for
/// thread loads.
pub(super) async fn scope_response(
    state: &AppState,
    profile: &AgentProfileConfig,
    data: ResponseData,
) -> Result<ResponseData, String> {
    match data {
        ResponseData::Thread {
            thread,
            messages,
            summary,
        } => scope_thread(state, profile, thread, messages, summary).await,
        other => Ok(other),
    }
}

/// Keep only the allowed accounts' messages of a thread, and rebuild the
/// thread's summary fields from them so nothing of the rest shows through.
async fn scope_thread(
    state: &AppState,
    profile: &AgentProfileConfig,
    mut thread: Thread,
    messages: Vec<Envelope>,
    summary: Option<ThreadSummaryData>,
) -> Result<ResponseData, String> {
    let total = messages.len();
    let mut kept = Vec::with_capacity(total);
    for message in messages {
        if account_id_allowed(state, profile, &message.account_id).await? {
            kept.push(message);
        }
    }
    if kept.len() == total {
        return Ok(ResponseData::Thread {
            thread,
            messages: kept,
            summary,
        });
    }
    let Some(latest) = kept.iter().max_by_key(|message| message.date) else {
        return Err(format!("Thread not found: {}", thread.id));
    };
    thread.account_id = latest.account_id.clone();
    thread.snippet = latest.snippet.clone();
    thread.latest_date = latest.date;
    thread.message_ids = kept.iter().map(|message| message.id.clone()).collect();
    thread.message_count = u32::try_from(kept.len()).unwrap_or(u32::MAX);
    thread.unread_count = u32::try_from(
        kept.iter()
            .filter(|message| !message.flags.contains(MessageFlags::READ))
            .count(),
    )
    .unwrap_or(u32::MAX);
    thread.participants = Vec::new();
    for message in &kept {
        if !thread.participants.contains(&message.from) {
            thread.participants.push(message.from.clone());
        }
    }
    // A cached summary was written over every message, the hidden ones too.
    Ok(ResponseData::Thread {
        thread,
        messages: kept,
        summary: None,
    })
}
