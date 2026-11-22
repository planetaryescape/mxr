use crate::app::{self, AttachmentOperation};
use crate::terminal_images::HtmlImageKey;
use image::DynamicImage;
use mxr_core::types::SubscriptionSummary;
use mxr_core::{Draft, Envelope, Label, MessageBody, MessageId, MxrError, Thread, ThreadId};
use mxr_protocol::{
    AccountOperationResult, AccountSummaryData, AccountSyncStatus, AttachmentFile, AuthSessionData,
    BodyFailure, DaemonEvent, DeliveryData, ReplyContext, Response, RuleFormData,
    ScreenerQueueEntryData, SenderProfileData, SnippetData, ThreadSummaryData,
};
use ratatui_image::thread::ResizeResponse;

pub(crate) enum AsyncResult {
    HtmlRendered,
    Assistant {
        call: crate::app::AssistantCall,
        result: Box<Result<mxr_protocol::ResponseData, MxrError>>,
    },
    SendIdentityRetained(mxr_core::AccountId, mxr_core::SendIdentity),
    ScheduledIdentityRetained(
        mxr_core::AccountId,
        mxr_core::SendIdentity,
        mxr_core::DraftId,
    ),
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
    PersonalCategory {
        generation: u64,
        report_if_stale: bool,
        view_category_id: Option<mxr_core::CategoryId>,
        result: Result<Box<mxr_protocol::ResponseData>, MxrError>,
    },
    RuleDetail {
        request_id: u64,
        result: Result<serde_json::Value, MxrError>,
    },
    RuleHistory {
        request_id: u64,
        result: Result<Vec<serde_json::Value>, MxrError>,
    },
    RuleDryRun {
        request_id: u64,
        result: Result<Vec<serde_json::Value>, MxrError>,
    },
    RuleTreatment(Result<serde_json::Value, MxrError>),
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
    Recovery {
        owner: mxr_protocol::ArchiveWorkspaceData,
        request: mxr_protocol::Request,
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
    Snoozed(Result<(Vec<mxr_core::types::Snoozed>, Vec<Envelope>), MxrError>),
    OwedReplies(
        Result<
            (
                Vec<mxr_protocol::OwedReplyRowData>,
                Option<mxr_protocol::PeopleSplitData>,
            ),
            MxrError,
        >,
    ),
    Desk(Result<crate::app::DeskPageState, MxrError>),
    FocusQueue {
        owed: Result<(mxr_protocol::DeskLaneData, mxr_protocol::PeopleSplitData), MxrError>,
        reply_later: Result<Vec<Envelope>, MxrError>,
    },
    Now(Result<Box<(mxr_protocol::NowData, Option<mxr_protocol::ModeGuideData>)>, MxrError>),
    Rail(Result<mxr_protocol::RailData, MxrError>),
    Arrivals(Result<Box<mxr_protocol::ArrivalsData>, MxrError>),
    ArrivalsList(
        crate::app::ArrivalsListFetch,
        Result<Box<mxr_protocol::ArrivalListData>, MxrError>,
    ),
    ArrivalModes(Result<Vec<mxr_protocol::ArrivalItemData>, MxrError>),
    Moved(Box<mxr_protocol::MoveOutcomeData>, bool),
    Messages(
        Result<
            Box<(
                mxr_protocol::MessagesData,
                Option<mxr_protocol::ModeGuideData>,
            )>,
            MxrError,
        >,
    ),
    PersonPage(
        String,
        Option<mxr_core::id::ThreadId>,
        Result<Box<mxr_protocol::PersonPageData>, MxrError>,
    ),
    MessagesAck(Result<Box<mxr_protocol::AckPlanData>, MxrError>),
    NowDigestPreview(Result<mxr_protocol::UpdatesLetGoData, MxrError>),
    Updates(
        Result<
            Box<(
                mxr_protocol::UpdatesDigestData,
                Option<mxr_protocol::ModeGuideData>,
            )>,
            MxrError,
        >,
    ),
    ReadingEdition(
        Result<
            Box<(
                mxr_protocol::ReadingEditionData,
                Option<mxr_protocol::ModeGuideData>,
            )>,
            MxrError,
        >,
    ),
    ReadingItem(
        String,
        Result<Box<mxr_protocol::ReadingItemDetailData>, MxrError>,
    ),
    ReadingArticle(String, Result<mxr_protocol::ReadingFetchData, MxrError>),
    ReadingLetGoPreview(
        Vec<mxr_core::id::ThreadId>,
        Result<Vec<mxr_protocol::ModeDoneOutcomeData>, MxrError>,
    ),
    ReadingUnsubscribePreview(
        crate::app::ReadingUnsubscribeTarget,
        Result<Box<mxr_protocol::UnsubscribePurgeResultData>, MxrError>,
    ),
    ReadingOriginal(Result<String, MxrError>),
    UpdatesLetGoPreview(Result<mxr_protocol::UpdatesLetGoData, MxrError>),
    LatestCodes(Result<Vec<mxr_protocol::OneTimeCodeData>, MxrError>),
    TodoRunway(
        Result<
            (
                mxr_protocol::TodoRunwayData,
                Option<mxr_protocol::ModeGuideData>,
            ),
            MxrError,
        >,
    ),
    TodoExpired(Result<Vec<mxr_protocol::TodoData>, MxrError>),
    MicrosoftTodo(Result<Box<mxr_protocol::ResponseData>, MxrError>),
    TodoCatchup(Result<mxr_protocol::TodoCatchupData, MxrError>),
    TodoCatchupPreview(Result<mxr_protocol::TodoChangeData, MxrError>),
    TodoResolutionPreview(
        String,
        String,
        Result<mxr_protocol::TodoChangeData, MxrError>,
    ),
    Place(
        crate::app::PlaceFetch,
        Result<crate::app::PlacePageState, MxrError>,
    ),
    SweepPreview(Result<crate::app::PendingSweepConfirm, MxrError>),
    PlaceSwept(Result<(u32, Vec<String>), MxrError>),
    CalendarInvites(Result<Vec<mxr_protocol::CalendarInviteData>, MxrError>),
    VoiceSourceOpened(u64, Result<Envelope, MxrError>),
    InviteEnvelopeOpened(Result<Envelope, MxrError>),
    RecordsLedger(
        Result<
            Box<(
                mxr_protocol::RecordLedgerData,
                Option<mxr_protocol::ModeGuideData>,
            )>,
            MxrError,
        >,
    ),
    RecordsAnswer(Result<Box<mxr_protocol::RecordAnswerData>, MxrError>),
    RecordSubscriptions(Result<Box<mxr_protocol::RecordSubscriptionsData>, MxrError>),
    RecordCard(Result<Box<mxr_protocol::RecordData>, MxrError>),
    RecordsExport(Result<Box<mxr_protocol::RecordExportData>, MxrError>),
    RecordFixPreview(Result<Box<mxr_protocol::RecordChangeData>, MxrError>),
    RecordFilePreview(Result<Box<mxr_protocol::RecordChangeData>, MxrError>),
    RecordEnvelopeOpened(Result<Envelope, MxrError>),
    TodoEnvelopeOpened(Result<Envelope, MxrError>, Option<String>),
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
        retry: Option<Box<app::QueuedMutation>>,
        sync_request_pending_failure: bool,
        outcome: Box<Result<app::MutationEffect, MxrError>>,
    },
    ComposeReady(Result<ComposeReadyData, MxrError>),
    ReplyContextWarmed {
        message_id: MessageId,
        generation: u64,
        reply: Box<Result<ReplyContext, MxrError>>,
        reply_all: Box<Result<ReplyContext, MxrError>>,
    },
    ExportResult(Result<String, String>),
    Unsubscribe(Result<UnsubscribeResultData, MxrError>),
    DraftCleanup {
        path: std::path::PathBuf,
        result: Result<(), String>,
    },
    LocalStateSaved(Result<(), String>),
    DaemonEvent(DaemonEvent),
    SyncBatch(Vec<mxr_protocol::SyncAccountResult>),
    WorkspaceOwner(mxr_protocol::ArchiveWorkspaceData),
    ConnectionState(app::ConnectionState),
    #[expect(
        dead_code,
        reason = "background producers can report through this path"
    )]
    ReportedError(app::UserError),
    UndoCaptured(app::PendingUndo),
    SavedSearchMutation(Result<(), MxrError>),
    SavedSearchListRefreshed(Result<Vec<mxr_core::types::SavedSearch>, MxrError>),
    SavedSearchUnreadCountsRefreshed(
        Result<std::collections::HashMap<mxr_core::id::SavedSearchId, u32>, MxrError>,
    ),
    SemanticOperationResult(Result<(), MxrError>),
    PlatformModalLoaded {
        send_recovery_preview: Option<(mxr_core::AccountId, String, String)>,
        compose: Option<Box<app::PendingSend>>,
        title: String,
        result: Result<String, MxrError>,
    },
    ComposeAssistLoaded {
        pending: Box<app::PendingSend>,
        model: Option<String>,
        request_id: u64,
        result: Result<mxr_protocol::ComposeProposalData, MxrError>,
    },
    ComposeModelsLoaded {
        pending: Box<app::PendingSend>,
        purpose: app::ComposeModelPickerPurpose,
        result: Result<mxr_protocol::AssistantModelsData, MxrError>,
    },
    ComposeAssistAccepted {
        pending: Box<app::PendingSend>,
        request_id: u64,
        reopen_editor: bool,
        result: Result<
            (
                mxr_core::Draft,
                Result<mxr_core::DraftSafetyReport, MxrError>,
            ),
            MxrError,
        >,
    },
    AnalyticsResult {
        view: app::AnalyticsView,
        result: Box<Result<AnalyticsResultPayload, MxrError>>,
    },
    SnippetsList(Result<Vec<SnippetData>, MxrError>),
    ReplyQueueList(Result<Vec<Envelope>, MxrError>),
    DeliveriesList(Result<Vec<DeliveryData>, MxrError>),
    DeliveryThreadOpened(Result<Vec<Envelope>, MxrError>),
    AssistantThreadOpened(Result<Vec<Envelope>, MxrError>),
    ActivityList(Result<Vec<mxr_protocol::ActivityEntry>, MxrError>),
    ActivityPauseToggled(Result<bool, MxrError>),
    SenderProfileLoaded {
        email: String,
        result: Box<Result<Option<SenderProfileData>, MxrError>>,
    },
    ScreenerQueueLoaded {
        account_id: mxr_core::AccountId,
        result: Result<Vec<ScreenerQueueEntryData>, MxrError>,
    },
    ScreenerDecisionApplied {
        account_id: mxr_core::AccountId,
        sender_email: String,
        result: Result<(), MxrError>,
    },
    ThreadSummaryLoaded {
        thread_id: mxr_core::ThreadId,
        result: Result<(String, String), MxrError>,
    },
    ThreadContextLoaded {
        thread_id: mxr_core::ThreadId,
        result: Result<Box<mxr_protocol::ThreadContextData>, MxrError>,
    },
    PromisesDetected {
        message_id: mxr_core::MessageId,
        result: Result<Box<mxr_protocol::PromiseDetectionData>, MxrError>,
    },
    RowGistsLoaded {
        thread_ids: Vec<mxr_core::ThreadId>,
        epoch: u64,
        result: Result<Box<mxr_protocol::ThreadGistBatchData>, MxrError>,
    },
    ThreadGistLoaded {
        thread_id: mxr_core::ThreadId,
        result: Result<Box<mxr_protocol::ThreadGistData>, MxrError>,
    },
    StoredDraftsLoaded(Result<(Vec<Draft>, Vec<mxr_protocol::ScheduledSendData>), MxrError>),
    StoredDraftUnscheduled {
        draft_id: mxr_core::DraftId,
        result: Result<(), MxrError>,
    },
    StoredDraftDeleted {
        draft_id: mxr_core::DraftId,
        result: Result<(), MxrError>,
    },
    StoredDraftPushed {
        draft_id: mxr_core::DraftId,
        provider: String,
        result: Result<(), MxrError>,
    },
    DraftEditReady(Result<DraftEditReadyData, MxrError>),
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
    SearchAggregation(Vec<mxr_protocol::SearchAggregationRow>),
    Wrapped(Box<mxr_core::types::WrappedSummary>),
    ContactsRefreshed { rows: u32 },
}

pub(crate) struct ComposeReadyData {
    pub(crate) account_id: mxr_core::AccountId,
    pub(crate) draft_revision: Option<i64>,
    pub(crate) intent: mxr_core::DraftIntent,
    pub(crate) draft_path: std::path::PathBuf,
    pub(crate) cursor_line: usize,
    pub(crate) initial_content: String,
    pub(crate) invite_reply: Option<mxr_core::types::InlineCalendarReply>,
}

pub(crate) struct SearchResultData {
    pub(crate) envelopes: Vec<Envelope>,
    pub(crate) scores: std::collections::HashMap<MessageId, f32>,
    pub(crate) triage_verdicts: std::collections::HashMap<MessageId, String>,
    pub(crate) has_more: bool,
    pub(crate) degraded: Option<mxr_protocol::SearchDegraded>,
}

pub(crate) struct StatusSnapshot {
    pub(crate) uptime_secs: u64,
    pub(crate) daemon_pid: Option<u32>,
    pub(crate) accounts: Vec<String>,
    pub(crate) total_messages: u32,
    pub(crate) sync_statuses: Vec<AccountSyncStatus>,
    pub(crate) degraded: bool,
    pub(crate) freshness: Option<mxr_protocol::FreshnessData>,
}

pub(crate) struct UnsubscribeResultData {
    pub(crate) archived_ids: Vec<MessageId>,
    pub(crate) message: String,
}

pub(crate) struct DraftEditReadyData {
    pub(crate) existing: Box<Draft>,
    pub(crate) path: std::path::PathBuf,
}
