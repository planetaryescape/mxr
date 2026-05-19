use crate::app::{self, AttachmentOperation};
use crate::terminal_images::HtmlImageKey;
use image::DynamicImage;
use mxr_core::types::SubscriptionSummary;
use mxr_core::{Envelope, Label, MessageBody, MessageId, MxrError, Thread, ThreadId};
use mxr_protocol::{
    AccountOperationResult, AccountSummaryData, AccountSyncStatus, AttachmentFile, AuthSessionData,
    BodyFailure, DaemonEvent, ReplyContext, Response, RuleFormData, ScreenerQueueEntryData,
    SenderProfileData, SnippetData, ThreadSummaryData,
};
use ratatui_image::thread::ResizeResponse;

pub(crate) enum AsyncResult {
    Search {
        target: app::SearchTarget,
        append: bool,
        session_id: u64,
        result: Result<SearchResultData, MxrError>,
    },
    SearchCount {
        session_id: u64,
        result: Result<u32, MxrError>,
    },
    Rules(Result<Vec<serde_json::Value>, MxrError>),
    RuleDetail {
        request_id: u64,
        result: Result<serde_json::Value, MxrError>,
    },
    RuleHistory {
        request_id: u64,
        result: Result<Vec<serde_json::Value>, MxrError>,
    },
    RuleDryRun(Result<Vec<serde_json::Value>, MxrError>),
    RuleForm {
        request_id: u64,
        result: Result<RuleFormData, MxrError>,
    },
    RuleDeleted(Result<(), MxrError>),
    RuleUpsert(Result<serde_json::Value, MxrError>),
    Diagnostics {
        request_id: u64,
        result: Box<Result<Response, MxrError>>,
    },
    Status {
        request_id: u64,
        result: Result<StatusSnapshot, MxrError>,
    },
    Accounts(Result<Vec<AccountSummaryData>, MxrError>),
    Labels(Result<Vec<Label>, MxrError>),
    AllEnvelopes(Result<Vec<Envelope>, MxrError>),
    Subscriptions(Result<Vec<SubscriptionSummary>, MxrError>),
    OwedReplies(Result<Vec<mxr_protocol::OwedReplyRowData>, MxrError>),
    Briefing(Result<mxr_protocol::ThreadBriefingData, MxrError>),
    Whois(Result<mxr_protocol::EntityExplanationData, MxrError>),
    Expert(Result<Vec<mxr_protocol::ExpertSuggestionData>, MxrError>),
    CommitmentCounts(std::collections::HashMap<(mxr_core::AccountId, mxr_core::ThreadId), u32>),
    AccountOperation(Result<AccountOperationResult, MxrError>),
    AuthSession(AuthSessionData),
    BugReport(Result<String, MxrError>),
    BugReportSaved(Result<std::path::PathBuf, String>),
    BrowserOpened(Result<std::path::PathBuf, String>),
    AttachmentFile {
        operation: AttachmentOperation,
        result: Result<AttachmentFile, MxrError>,
    },
    LabelEnvelopes(Result<Vec<Envelope>, MxrError>),
    Bodies {
        requested: Vec<MessageId>,
        result: Result<(Vec<MessageBody>, Vec<BodyFailure>), MxrError>,
    },
    HtmlImageAssets {
        message_id: MessageId,
        allow_remote: bool,
        result: Result<Vec<mxr_core::types::HtmlImageAsset>, MxrError>,
    },
    HtmlImageDecoded {
        key: HtmlImageKey,
        result: Result<DynamicImage, MxrError>,
    },
    HtmlImageResized {
        key: HtmlImageKey,
        result: Result<ResizeResponse, MxrError>,
    },
    Thread {
        thread_id: ThreadId,
        request_id: u64,
        result: Result<(Thread, Vec<Envelope>, Option<ThreadSummaryData>), MxrError>,
    },
    MutationResult {
        id: app::MutationId,
        best_effort: bool,
        retry: Option<app::QueuedMutation>,
        outcome: Result<app::MutationEffect, MxrError>,
    },
    ComposeReady(Result<ComposeReadyData, MxrError>),
    /// Result of a fire-and-forget prewarm task that runs when the
    /// user opens a message. Populates `reply_context_cache` so that
    /// pressing `r`/`a` skips the IPC round-trip. Failures are
    /// silently dropped — the cold path still works.
    ReplyContextWarmed {
        message_id: MessageId,
        reply: Result<ReplyContext, MxrError>,
        reply_all: Result<ReplyContext, MxrError>,
    },
    ExportResult(Result<String, MxrError>),
    Unsubscribe(Result<UnsubscribeResultData, MxrError>),
    DraftCleanup {
        path: std::path::PathBuf,
        result: Result<(), String>,
    },
    LocalStateSaved(Result<(), String>),
    DaemonEvent(DaemonEvent),
    /// Emitted by the IPC worker when the daemon connection state changes.
    /// Replaces the old "silent return on initial connect failure" behavior.
    ConnectionState(app::ConnectionState),
    /// A user-visible error/warn raised by a background task. Preferred
    /// over `let _ = ...` silent drops on async paths that affect what
    /// the user sees (HTML asset fetch, body parse, attachment fetch,
    /// search streaming).
    #[expect(
        dead_code,
        reason = "background producers can report through this path"
    )]
    ReportedError(app::UserError),
    /// Captured by the dispatch site when a mutation response carries a
    /// `mutation_id`. Drives the "u to undo" status-bar affordance.
    UndoCaptured(app::PendingUndo),
    /// One CreateSavedSearch / DeleteSavedSearch response. The dispatcher
    /// queues these one-shot per request; the success path triggers a
    /// follow-up `SavedSearchListRefreshed` to update the sidebar.
    SavedSearchMutation(Result<(), MxrError>),
    /// Refreshed sidebar list after a saved-search create/delete.
    SavedSearchListRefreshed(Result<Vec<mxr_core::types::SavedSearch>, MxrError>),
    /// Per-saved-search unread counts. Issued as a follow-up after a
    /// `SavedSearchListRefreshed` so the tab-strip badges can show
    /// `(N)` next to each label.
    SavedSearchUnreadCountsRefreshed(
        Result<std::collections::HashMap<mxr_core::id::SavedSearchId, u32>, MxrError>,
    ),
    /// One semantic-runtime operation response (Enable / Disable /
    /// Reindex / InstallProfile). Errors route through `report_error`
    /// so a missing `semantic-local` feature is visible to the user
    /// rather than silently swallowed.
    SemanticOperationResult(Result<(), MxrError>),
    PlatformModalLoaded {
        title: String,
        result: Result<String, MxrError>,
    },
    /// Result of one of the four analytics list requests. The
    /// dispatcher keys the response to the active view at the time
    /// the request was sent — late responses for a switched-away
    /// view are dropped by the handler.
    AnalyticsResult {
        view: app::AnalyticsView,
        result: Result<AnalyticsResultPayload, MxrError>,
    },
    /// Snapshot of the user's compose snippets, surfaced by the
    /// snippets browser modal.
    SnippetsList(Result<Vec<SnippetData>, MxrError>),
    /// Snapshot of the messages flagged for reply-later.
    ReplyQueueList(Result<Vec<Envelope>, MxrError>),
    /// Snapshot of recent activity log rows.
    ActivityList(Result<Vec<mxr_protocol::ActivityEntry>, MxrError>),
    /// Confirmation that a pause/resume toggle reached the daemon.
    ActivityPauseToggled(Result<bool, MxrError>),
    /// Per-sender relationship aggregates for the sender-view modal.
    /// `Ok(None)` when the sender is unknown to the contacts table.
    SenderProfileLoaded {
        email: String,
        result: Result<Option<SenderProfileData>, MxrError>,
    },
    /// Snapshot of senders awaiting a screener decision.
    ScreenerQueueLoaded {
        account_id: mxr_core::AccountId,
        result: Result<Vec<ScreenerQueueEntryData>, MxrError>,
    },
    /// Result of one `SetScreenerDecision` IPC. On success the modal
    /// has already optimistically removed the entry; on failure we
    /// re-fetch to recover the queue.
    ScreenerDecisionApplied {
        account_id: mxr_core::AccountId,
        sender_email: String,
        result: Result<(), MxrError>,
    },
    /// LLM summary for a thread. The tuple carries `(text, model)`.
    /// Late responses for a previously-focused thread are filtered at
    /// the dispatcher.
    ThreadSummaryLoaded {
        thread_id: mxr_core::ThreadId,
        result: Result<(String, String), MxrError>,
    },
}

#[derive(Debug, Clone)]
pub(crate) enum AnalyticsResultPayload {
    Storage(Vec<mxr_core::types::StorageBucket>),
    LargestMessages(Vec<mxr_core::types::LargestMessageRow>),
    Stale(Vec<mxr_core::types::StaleThreadRow>),
    Asymmetry(Vec<mxr_core::types::ContactAsymmetryRow>),
    Decay(Vec<mxr_core::types::ContactDecayRow>),
    CadenceDrift(Vec<mxr_protocol::CadenceDriftRowData>),
    ResponseTime(mxr_core::types::ResponseTimeSummary),
    Subscriptions(Vec<mxr_core::types::SubscriptionSummary>),
    Wrapped(mxr_core::types::WrappedSummary),
    ContactsRefreshed { rows: u32 },
}

pub(crate) struct ComposeReadyData {
    pub(crate) account_id: mxr_core::AccountId,
    pub(crate) intent: mxr_core::DraftIntent,
    pub(crate) draft_path: std::path::PathBuf,
    pub(crate) cursor_line: usize,
    pub(crate) initial_content: String,
    /// Set when this compose flow was initiated from the iCal invite
    /// "respond with comment" path. Carried into `PendingSend.invite_reply`
    /// so the outgoing builder emits the multipart/alternative MIME layout.
    pub(crate) invite_reply: Option<mxr_core::types::InlineCalendarReply>,
}

pub(crate) struct SearchResultData {
    pub(crate) envelopes: Vec<Envelope>,
    pub(crate) scores: std::collections::HashMap<MessageId, f32>,
    pub(crate) has_more: bool,
}

pub(crate) struct StatusSnapshot {
    pub(crate) uptime_secs: u64,
    pub(crate) daemon_pid: Option<u32>,
    pub(crate) accounts: Vec<String>,
    pub(crate) total_messages: u32,
    pub(crate) sync_statuses: Vec<AccountSyncStatus>,
}

pub(crate) struct UnsubscribeResultData {
    pub(crate) archived_ids: Vec<MessageId>,
    pub(crate) message: String,
}
