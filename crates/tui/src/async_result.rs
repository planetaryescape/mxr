use crate::app::{self, AttachmentOperation};
use crate::terminal_images::HtmlImageKey;
use image::DynamicImage;
use mxr_core::types::SubscriptionSummary;
use mxr_core::{Envelope, Label, MessageBody, MessageId, MxrError, Thread, ThreadId};
use mxr_protocol::{
    AccountOperationResult, AccountSummaryData, AccountSyncStatus, AttachmentFile, DaemonEvent,
    Response, RuleFormData,
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
    AccountOperation(Result<AccountOperationResult, MxrError>),
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
        result: Result<Vec<MessageBody>, MxrError>,
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
        result: Result<(Thread, Vec<Envelope>), MxrError>,
    },
    MutationResult(Result<app::MutationEffect, MxrError>),
    ComposeReady(Result<ComposeReadyData, MxrError>),
    ExportResult(Result<String, MxrError>),
    Unsubscribe(Result<UnsubscribeResultData, MxrError>),
    DraftCleanup {
        path: std::path::PathBuf,
        result: Result<(), String>,
    },
    LocalStateSaved(Result<(), String>),
    DaemonEvent(DaemonEvent),
}

pub(crate) struct ComposeReadyData {
    pub(crate) account_id: mxr_core::AccountId,
    pub(crate) draft_path: std::path::PathBuf,
    pub(crate) cursor_line: usize,
    pub(crate) initial_content: String,
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
