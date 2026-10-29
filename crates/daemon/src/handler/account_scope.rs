//! Which accounts a request touches, for the agent and MCP profiles'
//! `allowed_accounts`.
//!
//! `request_scope` is an exhaustive match with no catch-all arm, so a new
//! `Request` variant does not compile until someone decides what it touches.
//! The old hand-kept list ended in `_ => None`, which let every request it
//! forgot through unchecked.

use super::{reading, records, request_kind, todos};
use crate::state::AppState;
use mxr_config::{AgentProfileConfig, MxrConfig};
use mxr_core::id::{AccountId, CategoryId, DeliveryId, DraftId, LabelId, MessageId, ThreadId};
use mxr_core::types::Draft;
use mxr_core::types::{Envelope, MessageFlags, Thread};
use mxr_protocol::{
    AuthSessionId, ClientKind, DaemonEvent, JobData, MutationCommand, PromiseSourceData, Request,
    ResponseData, ThreadSummaryData,
};
use mxr_store::SignatureScope;
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
    /// Global sync is narrowed to accounts on the caller profile's allowlist
    /// by the sync target selector before any provider is touched.
    AllAllowedSyncAccounts,
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
    Rule(&'a str),
    AuthSession(&'a AuthSessionId),
    /// A label that must belong to the account named beside it: listing by
    /// label ignores the account, so a foreign label would list another
    /// account's mail.
    LabelIn(&'a LabelId, &'a AccountId),
    CategoryIn(&'a CategoryId, &'a AccountId),
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
    /// A Reading item key; it belongs to its message's account.
    ReadingItem(&'a str),
    /// A Reading highlight id; it belongs to its own account.
    ReadingHighlight(&'a str),
    Delivery(&'a DeliveryId),
    Commitment(&'a str),
    Decision(&'a str),
    /// A move or Not-sure correction, by its row id.
    MoveCorrection(i64),
    Undo(&'a str),
    CategoryPreview(&'a str),
    CategoryUndo(&'a str),
    Job(&'a str),
    Archive(&'a mxr_protocol::ArchiveIdentityData),
}

use RequestScope::{AllAccounts, AllAllowedSyncAccounts, Targets, Unscoped};
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
        Request::RemoteRead { .. } | Request::GetRemoteReady | Request::RemoteAuthority { .. } => AllAccounts,
        Request::GetRecoveryStatus | Request::ActivateRecovery {..} | Request::PrepareRecoveredDraft {..} => AllAccounts,
        // Refused for agent profiles before scoping; a conversation can
        // read any account.
        Request::GetAssistantAvailability
        | Request::GetAssistantModels
        | Request::GetComposeModels
        | Request::SetAssistantModel { .. }
        | Request::AssistantSend { .. }
        | Request::AssistantDecide { .. }
        | Request::AssistantCancel { .. }
        | Request::SetAssistantAutoApprove { .. }
        | Request::ListAssistantConversations { .. }
        | Request::GetAssistantConversation { .. }
        | Request::GetAssistantStepRows { .. }
        | Request::DeleteAssistantConversation { .. } => AllAccounts,
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
        | Request::ListLabels { account_id, .. }
        | Request::SystemMailboxCounts { account_id }
        | Request::ListCategories { account_id }
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
        | Request::CommitUnsubscribePreview { account_id, .. }
        | Request::ListScheduledSends { account_id }
        | Request::ListDeliveries { account_id, .. }
        | Request::ScanDeliveries { account_id, .. }
        | Request::SetSignatureDefault { account_id, .. }
        | Request::ClearSignatureDefault { account_id, .. }
        | Request::ResolveSignature { account_id, .. }
        | Request::ListSenders { account_id, .. }
        | Request::TriageSearch { account_id, .. }
        | Request::DraftEval { account_id, .. }
        | Request::ClassifyReport { account_id, .. }
        | Request::ClassifyCompare { account_id, .. }
        | Request::ClassifyEval { account_id, .. }
        | Request::ExportSearch { account_id, .. }
        | Request::GetDesk { account_id, .. }
        | Request::GetTodoRunway { account_id, .. }
        | Request::ListTodos { account_id, .. }
        | Request::GetTodoCatchup { account_id }
        | Request::SetTodoCatchup { account_id, .. }
        | Request::GetNow { account_id }
        | Request::GetArrivals { account_id, .. }
        | Request::ListArrivals { account_id, .. }
        | Request::ListCorrections { account_id, .. }
        | Request::GetRail { account_id }
        | Request::ListOneTimeCodes { account_id, .. }
        | Request::GetUpdatesDigest { account_id, .. }
        | Request::LetGoDigest { account_id, .. }
        | Request::SetUpdateSource { account_id, .. }
        | Request::ListMessages { account_id, .. }
        | Request::ListMergeSuggestions { account_id }
        | Request::ListRecords { account_id, .. }
        | Request::ListRecordSubscriptions { account_id }
        | Request::AnswerFromRecords { account_id, .. }
        | Request::ExportRecords { account_id, .. }
        | Request::ListPlace { account_id, .. }
        | Request::SweepPlace { account_id, .. } => optional_account(account_id.as_ref()),
        Request::PreviewSyncAll | Request::SyncAll => AllAllowedSyncAccounts,
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
        | Request::SetSenderKind { account_id, .. }
        | Request::SetReadingSource { account_id, .. } => account(account_id),

        // ----- Account config, named by key -----
        Request::AuthorizeAccountConfig { account, .. }
        | Request::StartAuthSession { account, .. }
        | Request::UpsertAccountConfig { account }
        | Request::TestAccountConfig { account }
        | Request::RepairAccountConfig { account } => {
            Targets(vec![T::AccountKey(account.key.as_str())])
        }
        Request::SetAccountIdentity { key, .. } => Targets(vec![T::AccountKey(key.as_str())]),
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
        | Request::GetAttachmentTransferMetadata { message_id, .. }
        | Request::ReadAttachmentChunk { message_id, .. }
        | Request::OpenAttachment { message_id, .. }
        | Request::SetFlags { message_id, .. }
        | Request::GetHeaders { message_id }
        | Request::Unsubscribe { message_id }
        | Request::Snooze { message_id, .. }
        | Request::Unsnooze { message_id }
        | Request::SetReplyLater { message_id, .. }
        | Request::PrepareReply { message_id, .. }
        | Request::EnsureReplyDraft { message_id, .. }
        | Request::PrepareForward { message_id }
        | Request::RecordPromise { message_id, .. }
        | Request::CreateTodo { message_id, .. }
        | Request::FileRecord { message_id, .. }
        | Request::SetRecordSender { message_id, .. }
        | Request::GetMessageKind { message_id }
        | Request::MoveMessage { message_id, .. } => Targets(vec![T::Message(message_id)]),
        Request::SetAutoReminder {
            sent_message_id, ..
        }
        | Request::CancelAutoReminder { sent_message_id } => {
            Targets(vec![T::Message(sent_message_id)])
        }
        Request::ListEnvelopesByIds { message_ids }
        | Request::PlanUnsubscribe { message_ids }
        | Request::ListBodies { message_ids }
        | Request::PinMessages { message_ids, .. }
        | Request::GetArrivalModes { message_ids } => messages(message_ids),
        Request::Mutation { mutation, .. } | Request::StartMutationJob { mutation, .. } | Request::PreviewArchive { mutation, .. } | Request::SubmitArchive { mutation, .. } => {
            messages(mutation_messages(mutation))
        }
        // With no account these read only the caller's accounts: dispatch
        // narrows them with `allowed_accounts`, so Reading keeps working
        // for a scoped agent instead of being denied.
        Request::GetReadingEdition { account_id, .. }
        | Request::ExportReadingHighlights { account_id } => match account_id {
            Some(account_id) => account(account_id),
            None => Targets(Vec::new()),
        },
        Request::GetReadingItem { item_key }
        | Request::RecordReadingEngagement { item_key, .. }
        | Request::FetchArticle { item_key, .. }
        | Request::SaveHighlight { item_key, .. }
        | Request::SendReadingToMarginalia { item_key, .. }
        | Request::SyncMarginaliaHighlights {
            item_key: Some(item_key),
        } => Targets(vec![T::ReadingItem(item_key)]),
        Request::UpdateHighlight { highlight_id, .. }
        | Request::RemoveHighlight { highlight_id, .. } => {
            Targets(vec![T::ReadingHighlight(highlight_id)])
        }
        Request::SetReadingLater { item_keys, .. } => Targets(
            item_keys
                .iter()
                .map(|key| T::ReadingItem(key.as_str()))
                .collect(),
        ),
        Request::DetectPromises { source, .. } => match source {
            PromiseSourceData::Draft { draft } => Targets(vec![T::DraftBody(draft)]),
            PromiseSourceData::SentMessage { message_id } => Targets(vec![T::Message(message_id)]),
        },

        // ----- Named by thread -----
        Request::GetThread { thread_id, account_id } => {
            let mut targets = vec![T::Thread(thread_id)];
            targets.extend(account_id.as_ref().map(T::Account));
            Targets(targets)
        }
        Request::SummarizeThread { thread_id }
        | Request::GetThreadBriefing { thread_id, .. }
        | Request::ExportThread { thread_id, .. }
        | Request::GetThreadContext { thread_id }
        | Request::GetThreadGist { thread_id, .. }
        | Request::AckMessage { thread_id, .. } => Targets(vec![T::Thread(thread_id)]),
        Request::DismissDeskThreads { thread_ids, .. }
        | Request::RestoreDeskThreads { thread_ids }
        | Request::GetThreadGists { thread_ids, .. }
        | Request::GetThreadPreviews { thread_ids }
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
        Request::ListCategoryMessages { category_id, account_id, .. } => match account_id {
            Some(account_id) => Targets(vec![T::Account(account_id), T::CategoryIn(category_id, account_id)]),
            None => AllAccounts,
        },
        Request::GetCategoryMemberships { account_id, message_ids, category_ids } => {
            let mut targets: Vec<_> = message_ids.iter().map(T::Message).collect();
            if let Some(account_id) = account_id {
                targets.push(T::Account(account_id));
                targets.extend(category_ids.iter().map(|category_id| T::CategoryIn(category_id, account_id)));
            }
            Targets(targets)
        }
        Request::PreviewCategoryMembership { account_id, message_ids, add_category_ids, remove_category_ids } => {
            let mut targets: Vec<_> = message_ids.iter().map(T::Message).collect();
            if let Some(account_id) = account_id {
                targets.push(T::Account(account_id));
                targets.extend(add_category_ids.iter().map(|category_id| T::CategoryIn(category_id, account_id)));
                targets.extend(remove_category_ids.iter().map(|category_id| T::CategoryIn(category_id, account_id)));
            }
            Targets(targets)
        }
        Request::ApplyCategoryMembership { preview_token } => Targets(vec![T::CategoryPreview(preview_token)]),
        Request::UndoCategoryMembership { mutation_id } => Targets(vec![T::CategoryUndo(mutation_id)]),
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
        Request::PreviewDraftAttachment { metadata } | Request::BeginDraftAttachment { metadata } => Targets(vec![T::Account(&metadata.account_id), T::Draft(&metadata.draft_id)]),
        Request::WriteDraftAttachmentChunk { account_id, draft_id, .. } | Request::FinalizeDraftAttachment { account_id, draft_id, .. } => Targets(vec![T::Account(account_id), T::Draft(draft_id)]),
        Request::SendDraft { draft, .. }
        | Request::SaveDraft { draft }
        | Request::UpdateDraft { draft }
        | Request::SaveDraftToServer { draft }
        | Request::PreviewDraft { draft, .. }
        | Request::CheckDraftSafety { draft, .. }
        | Request::ExtractDraftCommitments { draft }
        | Request::SuggestCollaborators { draft, .. } => Targets(vec![T::DraftBody(draft)]),
        Request::GetSendOutcome { account_id, .. } => Targets(vec![T::Account(account_id)]),
        Request::RecoverSendOutcome { account_id, .. } => Targets(vec![T::Account(account_id)]),
        Request::ComposeAssist { draft, .. } => match draft {
            mxr_protocol::DraftTargetData::Stored { draft_id } => Targets(vec![T::Draft(draft_id)]),
            mxr_protocol::DraftTargetData::Reply { message_id, .. }
            | mxr_protocol::DraftTargetData::Forward { message_id, .. } => {
                Targets(vec![T::Message(message_id)])
            }
            mxr_protocol::DraftTargetData::New { account_id, .. } => {
                Targets(vec![T::Account(account_id)])
            }
        },
        Request::DraftRefine { draft_id, .. }
        | Request::ApplyComposeProposal { draft_id, .. }
        | Request::SendStoredDraft { draft_id, .. }
        | Request::ScheduleSend { draft_id, .. }
        | Request::CancelScheduledSend { draft_id }
        | Request::DeleteDraft { draft_id, .. }
        | Request::SetReplyDraftAudience { draft_id, .. }
        | Request::GetDraft { draft_id }
        | Request::ResetOrphanedDraft { draft_id } => Targets(vec![T::Draft(draft_id)]),

        // ----- Named by another entity -----
        Request::SyncMicrosoftTodos { todo_id, account_id, .. } => {
            let mut targets = Vec::new();
            if let Some(account_id) = account_id { targets.push(T::Account(account_id)); }
            if let Some(todo_id) = todo_id { targets.push(T::Todo(todo_id)); }
            if targets.is_empty() { AllAllowedSyncAccounts } else { Targets(targets) }
        },
        Request::SendTodoToMicrosoft { todo_id, account_id, .. } => {
            let mut targets = vec![T::Todo(todo_id)];
            if let Some(account_id) = account_id { targets.push(T::Account(account_id)); }
            Targets(targets)
        },
        Request::GetTodo { todo_id }
        | Request::ScheduleTodo { todo_id, .. }
        | Request::UpdateTodo { todo_id, .. }
        | Request::CorrectTodo { todo_id, .. }
        | Request::UndoTodoCorrection { todo_id, .. }
        | Request::ResolveTodoSuggestion { todo_id, .. } => Targets(vec![T::Todo(todo_id)]),
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
        Request::UndoMove { correction_id } => Targets(vec![T::MoveCorrection(*correction_id)]),
        Request::UndoMutation { mutation_id } => Targets(vec![T::Undo(mutation_id)]),
        Request::GetArchiveWorkspace | Request::GetMicrosoftTodoStatus => Unscoped,
        Request::GetArchiveOperation { identity } => Targets(vec![T::Archive(identity)]),
        Request::GetJob { job_id } => Targets(vec![T::Job(job_id)]),

        Request::ExplainCategoryMembership { message_id, .. } => Targets(vec![T::Message(message_id)]),
        Request::RuleCategories {form,..} | Request::RuleTreatment {form,..} => {
            if let Some(account_id) = &form.account_id {
                let mut targets = vec![T::Account(account_id)];
                targets.extend(form.id.as_deref().map(T::Rule));
                Targets(targets)
            } else { AllAccounts }
        }
        Request::GetRule {rule} | Request::GetRuleForm {rule} => Targets(vec![T::Rule(rule)]),

        // ----- Every account, or daemon-wide data that mixes them -----
        // Quick drafts a fixed corpus, so it reads no account's mail.
        Request::ListAccounts
        | Request::ListAccountsConfig
        | Request::DraftEvalQuick
        | Request::CreateCategory { .. }
        | Request::PreviewCategoryName { .. }
        | Request::RenameCategory { .. }
        | Request::PreviewDeleteCategory { .. }
        | Request::DeleteCategory { .. }
        | Request::ListRules
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
        | Request::ReloadLlmConfig
        | Request::SetLlmKey { .. }
        | Request::RemoveLlmKey { .. }
        // Decides where every account's Reading items may be sent, and
        // reads back highlights on every account's sent items.
        | Request::GetMarginaliaStatus
        | Request::SetMarginaliaConnection { .. }
        | Request::SyncMarginaliaHighlights { item_key: None }
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
        // Snippets are user text with no account binding, so a scoped
        // profile can't be given a part of them.
        | Request::ListSnippets
        | Request::SetSnippet { .. }
        | Request::DeleteSnippet { .. }
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
        // Every client calls status as the daemon version handshake and the
        // MCP status tool depends on it; `scope_response` cuts its account
        // rows down to the allowed accounts.
        // Freshness across every account is the status bar's poll:
        // `scope_response` keeps only the allowed accounts' sync health and
        // arrivals. Naming an account scopes it like any other request.
        Request::GetFreshness {
            account_id: Some(account_id),
            ..
        } => account(account_id),
        Request::GetFreshness {
            account_id: None, ..
        }
        | Request::GetStatus
        | Request::Ping
        | Request::Authenticate { .. }
        | Request::GetLlmStatus
        | Request::GetLlmConfig
        | Request::GetNotificationChimes
        | Request::UpdateNotificationChimes { .. }
        | Request::PatchNotificationChimes { .. }
        | Request::PreviewNotificationChime { .. }
        | Request::GetSemanticStatus
        // `scope_response` hides signatures bound to an excluded account.
        | Request::ListSignatures
        | Request::HumanizerScore { .. }
        | Request::HumanizerRewrite { .. }
        | Request::ResolveTime { .. }
        | Request::GetModeGuide { .. }
        | Request::GetKeymap
        | Request::UpdateKeys { .. }
        | Request::SetHintSeen { .. } => Unscoped,
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
    allow_global_categories: bool,
) -> Result<ResolvedAccounts, String> {
    let mut resolved = ResolvedAccounts::default();
    let mut thread_ids = Vec::new();
    let mut todo_ids = Vec::new();
    let mut record_ids = Vec::new();
    for target in targets {
        match target {
            T::Account(account_id) => resolved.push((*account_id).clone()),
            T::AccountKey(key) => resolved.keys.push((*key).to_string()),
            T::Rule(key) => {
                let row = state
                    .store
                    .get_rule_by_id_or_name(key)
                    .await
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("Rule not found: {key}"))?;
                let value = mxr_store::row_to_rule_json(&row);
                let account = value["account_id"]
                    .as_str()
                    .ok_or("Global rules require access to all accounts")?;
                resolved.push(account.parse::<AccountId>().map_err(|e| e.to_string())?);
            }
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
            T::CategoryIn(category_id, account_id) => {
                let visible = if allow_global_categories {
                    state.store.personal_category_exists(category_id).await?
                } else {
                    state
                        .store
                        .category_exists_in_account(category_id, account_id)
                        .await?
                };
                if !visible {
                    return Err(format!(
                        "Category {category_id} is not visible in account {account_id}"
                    ));
                }
            }
            T::Message(message_id) => resolved.push(message_account(state, message_id).await?),
            T::ReadingItem(key) => {
                let (message_id, _) =
                    reading::parse_item_key(key).map_err(|error| error.to_string())?;
                resolved.push(message_account(state, &message_id).await?);
            }
            T::ReadingHighlight(id) => {
                let highlight = state
                    .store
                    .reading_highlight(id)
                    .await
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("No highlight {id}; it may already be removed"))?;
                resolved.push(highlight.account_id);
            }
            T::Thread(thread_id) => thread_ids.push((*thread_id).clone()),
            T::Draft(draft_id) => {
                let draft = state
                    .store
                    .get_draft(draft_id)
                    .await
                    .map_err(|e| e.to_string())?;
                if let Some(draft) = draft {
                    resolved.push(draft.account_id);
                } else if let Some(outcome) = state
                    .store
                    .get_draft_send_outcome(draft_id)
                    .await
                    .map_err(|e| e.to_string())?
                {
                    resolved.push(outcome.account_id);
                } else {
                    return Err(format!("Draft not found: {draft_id}"));
                }
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
            T::MoveCorrection(id) => {
                let correction = state
                    .store
                    .get_correction(*id)
                    .await
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("Correction not found: {id}"))?;
                resolved.push(correction.fields.account_id);
            }
            T::Undo(mutation_id) => {
                resolve_undo(state, mutation_id, &mut resolved, &mut todo_ids).await?;
            }
            T::Archive(identity) => {
                let key = format!(
                    "recoverable_archive:{}:{}",
                    identity.caller_id, identity.operation_id
                );
                let json = state
                    .store
                    .get_mutation_job(&key)
                    .await
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| "Archive operation not found".to_string())?;
                let operation: mxr_protocol::ArchiveOperationData =
                    serde_json::from_str(&json).map_err(|e| e.to_string())?;
                for item in operation.items {
                    resolved.push(item.account_id);
                }
            }
            T::CategoryPreview(token) => {
                let (accounts, category_ids) =
                    state.category_previews.membership_scope_for_token(token)?;
                for account_id in &accounts {
                    for category_id in &category_ids {
                        let visible = if allow_global_categories {
                            state.store.personal_category_exists(category_id).await?
                        } else {
                            state
                                .store
                                .category_exists_in_account(category_id, account_id)
                                .await?
                        };
                        if !visible {
                            return Err(format!(
                                "Category {category_id} is no longer visible in account {account_id}"
                            ));
                        }
                    }
                    resolved.push(account_id.clone());
                }
            }
            T::CategoryUndo(mutation_id) => {
                for account_id in state.store.category_undo_accounts(mutation_id).await? {
                    resolved.push(account_id);
                }
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
    for prior in desk.highlights {
        resolved.push(prior.row.account_id);
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
    if matches!(
        req,
        Request::RuleCategories { .. }
            | Request::GetRule { .. }
            | Request::GetRuleForm { .. }
            | Request::GetRecoveryStatus
            | Request::ActivateRecovery { .. }
            | Request::PrepareRecoveredDraft { .. }
    ) && profile_covers_all_accounts(state, profile).await?
    {
        return Ok(());
    }
    let full_category_access =
        if is_global_category_catalog_request(req) || request_references_category(req) {
            profile_covers_all_accounts(state, profile).await?
        } else {
            false
        };
    if is_global_category_catalog_request(req) {
        return if full_category_access {
            Ok(())
        } else {
            Err(format!(
                "Request `{}` rejected by {profile_name} profile account allowlist; category catalog access requires every stored and configured account",
                request_kind(req)
            ))
        };
    }

    if !full_category_access
        && matches!(
            req,
            Request::GetCategoryMemberships {
                account_id: None,
                ..
            } | Request::PreviewCategoryMembership {
                account_id: None,
                ..
            }
        )
    {
        return Err(format!(
            "Request `{}` rejected by {profile_name} profile account allowlist; category membership requests must name an account",
            request_kind(req)
        ));
    }
    if full_category_access {
        let category_ids: Vec<&CategoryId> = match req {
            Request::GetCategoryMemberships {
                account_id: None,
                category_ids,
                ..
            } => category_ids.iter().collect(),
            Request::PreviewCategoryMembership {
                account_id: None,
                add_category_ids,
                remove_category_ids,
                ..
            } => add_category_ids.iter().chain(remove_category_ids).collect(),
            _ => Vec::new(),
        };
        for category_id in category_ids {
            if !state.store.personal_category_exists(category_id).await? {
                return Err(format!("Category {category_id} not found"));
            }
        }
    }

    let targets = match request_scope(req) {
        Unscoped => return Ok(()),
        AllAllowedSyncAccounts => return Ok(()),
        AllAccounts => {
            return Err(format!(
                "Request `{}` rejected by {profile_name} profile account allowlist; it spans every account, so name an allowed account where the request takes one",
                request_kind(req)
            ))
        }
        Targets(targets) => targets,
    };
    let resolved = resolve_targets(state, &targets, full_category_access).await?;
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

fn request_references_category(req: &Request) -> bool {
    matches!(
        req,
        Request::ListCategoryMessages { .. }
            | Request::GetCategoryMemberships { .. }
            | Request::PreviewCategoryMembership { .. }
            | Request::ApplyCategoryMembership { .. }
    )
}

fn is_global_category_catalog_request(req: &Request) -> bool {
    matches!(
        req,
        Request::ListCategories { account_id: None }
            | Request::ListCategoryMessages {
                account_id: None,
                ..
            }
            | Request::CreateCategory { .. }
            | Request::PreviewCategoryName { .. }
            | Request::RenameCategory { .. }
            | Request::PreviewDeleteCategory { .. }
            | Request::DeleteCategory { .. }
    )
}

async fn profile_covers_all_accounts(
    state: &AppState,
    profile: &AgentProfileConfig,
) -> Result<bool, String> {
    let config: MxrConfig = state.config_snapshot();
    let stored_accounts = state
        .store
        .list_all_accounts()
        .await
        .map_err(|error| error.to_string())?;
    if stored_accounts.is_empty() && config.accounts.is_empty() {
        return Ok(false);
    }

    for account in &stored_accounts {
        if !account_id_allowed(state, profile, &account.id).await? {
            return Ok(false);
        }
    }

    for (key, configured) in &config.accounts {
        if !account_token_allowed(profile, key)
            && !account_token_allowed(profile, &configured.email)
        {
            return Ok(false);
        }
    }

    Ok(true)
}

/// The enabled accounts a scoped profile may see.
pub(super) async fn allowed_accounts(
    state: &AppState,
    profile: &AgentProfileConfig,
) -> Result<Vec<AccountId>, String> {
    let mut allowed = Vec::new();
    for account in state
        .store
        .list_accounts()
        .await
        .map_err(|e| e.to_string())?
    {
        if account.enabled && account_id_allowed(state, profile, &account.id).await? {
            allowed.push(account.id);
        }
    }
    Ok(allowed)
}

/// Scope cached reads without changing the enabled-only SyncAll scope.
pub(super) async fn allowed_cached_accounts(
    state: &AppState,
    profile: &AgentProfileConfig,
) -> Result<Vec<AccountId>, String> {
    let mut allowed = Vec::new();
    for account in state
        .store
        .list_cached_mail_accounts()
        .await
        .map_err(|e| e.to_string())?
    {
        if account_id_allowed(state, profile, &account.id).await? {
            allowed.push(account.id);
        }
    }
    Ok(allowed)
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
        | ClientKind::Daemon
        | ClientKind::Assistant => None,
    }
}

/// Cut a response down to what a scoped profile may see. Request checks
/// already keep a scoped client to its accounts; this covers responses that
/// are allowed but carry other accounts' rows (status, signatures, threads
/// whose id another account shares) and is a second safety net for thread
/// loads.
pub(super) async fn scope_response(
    state: &AppState,
    profile: &AgentProfileConfig,
    data: ResponseData,
) -> Result<ResponseData, String> {
    match data {
        ResponseData::Categories { mut categories } => {
            if !profile_covers_all_accounts(state, profile).await? {
                categories.retain(|category| category.total_count > 0);
            }
            Ok(ResponseData::Categories { categories })
        }
        ResponseData::Status {
            recovery_supported,
            mut recovery,
            workspace_owner_supported,
            draft_revision_supported,
            send_outcome_supported,
            archive_recovery_supported,
            attachment_transfer_supported,
            draft_attachment_upload_supported,
            uptime_secs,
            accounts: _,
            total_messages,
            daemon_pid,
            sync_statuses,
            protocol_version,
            daemon_version,
            daemon_build_id,
            repair_required,
            semantic_runtime,
            feature_health,
            degraded,
        } => {
            let mut allowed = Vec::new();
            for status in sync_statuses {
                if account_id_allowed(state, profile, &status.account_id).await? {
                    allowed.push(status);
                }
            }
            let total_messages = if degraded {
                total_messages
            } else {
                let counts = state
                    .store
                    .count_messages_grouped_by_account()
                    .await
                    .map_err(|e| e.to_string())?;
                allowed
                    .iter()
                    .map(|status| counts.get(&status.account_id).copied().unwrap_or(0))
                    .sum()
            };
            if let Some(status) = &mut recovery {
                let mut held = Vec::new();
                for draft in std::mem::take(&mut status.held_drafts) {
                    if account_id_allowed(state, profile, &draft.account_id).await? {
                        held.push(draft);
                    }
                }
                status.held_drafts = held;
                let all = profile_covers_all_accounts(state, profile).await?;
                let mut intentions = Vec::new();
                for intent in std::mem::take(&mut status.retained_intentions) {
                    if all
                        || if let Some(account) = &intent.account_id {
                            account_id_allowed(state, profile, account).await?
                        } else {
                            false
                        }
                    {
                        intentions.push(intent);
                    }
                }
                status.retained_intentions = intentions;
                status.retired_intentions = status.retained_intentions.len() as u64;
            }
            Ok(ResponseData::Status {
                recovery_supported,
                recovery,
                workspace_owner_supported,
                draft_revision_supported,
                send_outcome_supported,
                archive_recovery_supported,
                attachment_transfer_supported,
                draft_attachment_upload_supported,
                uptime_secs,
                accounts: allowed
                    .iter()
                    .map(|status| status.account_name.clone())
                    .collect(),
                total_messages,
                daemon_pid,
                sync_statuses: allowed,
                protocol_version,
                daemon_version,
                daemon_build_id,
                repair_required,
                semantic_runtime,
                feature_health,
                degraded,
            })
        }
        ResponseData::Freshness { mut freshness } => {
            let mut accounts = Vec::new();
            for account in freshness.accounts {
                if account_id_allowed(state, profile, &account.account_id).await? {
                    accounts.push(account);
                }
            }
            freshness
                .arrivals
                .retain(|arrival| accounts.iter().any(|a| a.account_id == arrival.account_id));
            freshness.newest_message_at = accounts
                .iter()
                .filter_map(|account| account.newest_message_at)
                .max();
            freshness.worst_account_id = accounts
                .iter()
                .max_by_key(|account| account.health)
                .map(|account| account.account_id.clone());
            freshness.accounts = accounts;
            Ok(ResponseData::Freshness { freshness })
        }
        ResponseData::Thread {
            thread,
            messages,
            summary,
        } => scope_thread(state, profile, thread, messages, summary).await,
        ResponseData::Threads { threads } => Ok(ResponseData::Threads {
            threads: scope_threads(state, profile, threads).await?,
        }),
        ResponseData::Signatures { signatures } => {
            let mut hidden = Vec::new();
            for default in state
                .store
                .list_signature_defaults()
                .await
                .map_err(|e| e.to_string())?
            {
                let bound_to = match &default.scope {
                    SignatureScope::Global => None,
                    SignatureScope::Account(account_id)
                    | SignatureScope::Address { account_id, .. } => Some(account_id),
                };
                if let Some(account_id) = bound_to {
                    if !account_id_allowed(state, profile, account_id).await? {
                        hidden.push(default.signature.id);
                    }
                }
            }
            Ok(ResponseData::Signatures {
                signatures: signatures
                    .into_iter()
                    .filter(|signature| !hidden.contains(&signature.id))
                    .collect(),
            })
        }
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
    rebuild_thread(&mut thread, &kept)?;
    // A cached summary was written over every message, the hidden ones too.
    Ok(ResponseData::Thread {
        thread,
        messages: kept,
        summary: None,
    })
}

/// `Thread` and `Threads` are the responses that carry hydrated threads.
/// A listed thread whose id another account also holds is rebuilt from the
/// allowed accounts' messages, and one with none of them is dropped.
async fn scope_threads(
    state: &AppState,
    profile: &AgentProfileConfig,
    threads: Vec<Thread>,
) -> Result<Vec<Thread>, String> {
    let ids: Vec<_> = threads.iter().map(|thread| thread.id.clone()).collect();
    let pairs = state
        .store
        .thread_account_pairs(&ids)
        .await
        .map_err(|e| e.to_string())?;
    let mut excluded = Vec::new();
    for (thread_id, account_id) in &pairs {
        if !account_id_allowed(state, profile, account_id).await? {
            excluded.push(thread_id);
        }
    }
    let mut scoped = Vec::with_capacity(threads.len());
    for mut thread in threads {
        if !excluded.contains(&&thread.id) {
            scoped.push(thread);
            continue;
        }
        let mut kept = Vec::new();
        for message in state
            .store
            .get_thread_envelopes(&thread.id)
            .await
            .map_err(|e| e.to_string())?
        {
            if account_id_allowed(state, profile, &message.account_id).await? {
                kept.push(message);
            }
        }
        if !kept.is_empty() {
            rebuild_thread(&mut thread, &kept)?;
            scoped.push(thread);
        }
    }
    Ok(scoped)
}

/// Rebuild a thread's aggregate fields from the messages a scoped client
/// may see, so nothing of a hidden message shows through: the store
/// aggregates a shared thread id over every account's messages.
fn rebuild_thread(thread: &mut Thread, kept: &[Envelope]) -> Result<(), String> {
    let Some(latest) = kept.iter().max_by_key(|message| message.date) else {
        return Err(format!("Thread not found: {}", thread.id));
    };
    thread.account_id = latest.account_id.clone();
    thread.snippet = latest.snippet.clone();
    thread.latest_date = latest.date;
    // The store takes `MIN(subject)`; do the same over what's kept.
    if let Some(subject) = kept.iter().map(|message| &message.subject).min() {
        thread.subject = subject.clone();
    }
    thread.message_ids = kept.iter().map(|message| message.id.clone()).collect();
    thread.message_count = u32::try_from(kept.len()).unwrap_or(u32::MAX);
    thread.unread_count = u32::try_from(
        kept.iter()
            .filter(|message| !message.flags.contains(MessageFlags::READ))
            .count(),
    )
    .unwrap_or(u32::MAX);
    thread.participants = Vec::new();
    for message in kept {
        if !thread.participants.contains(&message.from) {
            thread.participants.push(message.from.clone());
        }
    }
    Ok(())
}

/// Cut a daemon event down to what a scoped profile may see, or drop it.
/// Events that concern no account (daemon health, lag without its count)
/// pass; events whose
/// account can't be worked out are dropped.
pub(crate) async fn scope_event(
    state: &AppState,
    profile: &AgentProfileConfig,
    event: DaemonEvent,
) -> Result<Option<DaemonEvent>, String> {
    let keep_if = |allowed: bool, event| if allowed { Some(event) } else { None };
    Ok(match event {
        DaemonEvent::SyncCompleted { ref account_id, .. }
        | DaemonEvent::SyncError { ref account_id, .. } => {
            let allowed = account_id_allowed(state, profile, account_id).await?;
            keep_if(allowed, event)
        }
        DaemonEvent::NewMessages { envelopes, .. } => {
            let mut kept = Vec::with_capacity(envelopes.len());
            for envelope in envelopes {
                if account_id_allowed(state, profile, &envelope.account_id).await? {
                    kept.push(envelope);
                }
            }
            // The full count would tell how much mail other accounts got,
            // so a scoped client's total is what it can see.
            (!kept.is_empty()).then_some(DaemonEvent::NewMessages {
                total: kept.len(),
                envelopes: kept,
            })
        }
        DaemonEvent::MessageUnsnoozed { ref message_id }
        | DaemonEvent::ReminderTriggered {
            sent_message_id: ref message_id,
        }
        | DaemonEvent::ReplyLaterReturned { ref message_id } => {
            let account_id = message_account(state, message_id).await?;
            let allowed = account_id_allowed(state, profile, &account_id).await?;
            keep_if(allowed, event)
        }
        DaemonEvent::LabelCountsUpdated { counts } => {
            let mut allowed_labels = Vec::new();
            for account in state
                .store
                .list_accounts()
                .await
                .map_err(|e| e.to_string())?
            {
                if account_id_allowed(state, profile, &account.id).await? {
                    allowed_labels.extend(
                        state
                            .store
                            .list_labels_by_account(&account.id)
                            .await
                            .map_err(|e| e.to_string())?
                            .into_iter()
                            .map(|label| label.id),
                    );
                }
            }
            let counts: Vec<_> = counts
                .into_iter()
                .filter(|count| allowed_labels.contains(&count.label_id))
                .collect();
            (!counts.is_empty()).then_some(DaemonEvent::LabelCountsUpdated { counts })
        }
        DaemonEvent::OperationStarted { ref account_id, .. }
        | DaemonEvent::OperationProgress { ref account_id, .. }
        | DaemonEvent::OperationCompleted { ref account_id, .. }
        | DaemonEvent::OperationFailed { ref account_id, .. }
        | DaemonEvent::OperationCancelled { ref account_id, .. }
        | DaemonEvent::ModesChanged { ref account_id } => {
            let allowed = match account_id {
                Some(account_id) => account_id_allowed(state, profile, account_id).await?,
                // An operation over every account, or one that didn't say.
                None => false,
            };
            keep_if(allowed, event)
        }
        DaemonEvent::AccountAddressesChanged { ref account_id } => {
            let allowed = account_id_allowed(state, profile, account_id).await?;
            keep_if(allowed, event)
        }
        // Its summary can name another account's messages, and nothing
        // ties it to an account.
        DaemonEvent::MutationReconciliationFailed { .. } => None,
        // The count covers every account's events, so a scoped client is
        // told to resync without learning how much the others got.
        DaemonEvent::EventsLagged { .. } => Some(DaemonEvent::EventsLagged { skipped: 0 }),
        // Agent profiles can't use the assistant, and a conversation can
        // touch any account.
        DaemonEvent::AssistantUpdated { .. } => None,
        DaemonEvent::ThreadGistReady { ref gist } => {
            let mut allowed = true;
            for account_id in thread_accounts(state, std::slice::from_ref(&gist.thread_id)).await? {
                if !account_id_allowed(state, profile, &account_id).await? {
                    allowed = false;
                    break;
                }
            }
            keep_if(allowed, event)
        }
    })
}
