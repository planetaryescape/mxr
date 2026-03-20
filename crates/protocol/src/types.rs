use mxr_core::id::*;
use mxr_core::types::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcMessage {
    pub id: u64,
    pub payload: IpcPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
#[allow(clippy::large_enum_variant)]
pub enum IpcPayload {
    Request(Request),
    Response(Response),
    Event(DaemonEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd")]
pub enum Request {
    ListEnvelopes {
        label_id: Option<LabelId>,
        account_id: Option<AccountId>,
        limit: u32,
        offset: u32,
    },
    GetEnvelope {
        message_id: MessageId,
    },
    GetBody {
        message_id: MessageId,
    },
    DownloadAttachment {
        message_id: MessageId,
        attachment_id: AttachmentId,
    },
    OpenAttachment {
        message_id: MessageId,
        attachment_id: AttachmentId,
    },
    ListBodies {
        message_ids: Vec<MessageId>,
    },
    GetThread {
        thread_id: ThreadId,
    },
    ListLabels {
        account_id: Option<AccountId>,
    },
    CreateLabel {
        name: String,
        color: Option<String>,
        account_id: Option<AccountId>,
    },
    DeleteLabel {
        name: String,
        account_id: Option<AccountId>,
    },
    RenameLabel {
        old: String,
        new: String,
        account_id: Option<AccountId>,
    },
    ListRules,
    ListAccounts,
    ListAccountsConfig,
    UpsertAccountConfig {
        account: AccountConfigData,
    },
    SetDefaultAccount {
        key: String,
    },
    TestAccountConfig {
        account: AccountConfigData,
    },
    GetRule {
        rule: String,
    },
    GetRuleForm {
        rule: String,
    },
    UpsertRule {
        rule: serde_json::Value,
    },
    UpsertRuleForm {
        existing_rule: Option<String>,
        name: String,
        condition: String,
        action: String,
        priority: i32,
        enabled: bool,
    },
    DeleteRule {
        rule: String,
    },
    DryRunRules {
        rule: Option<String>,
        all: bool,
        after: Option<String>,
    },
    ListEvents {
        limit: u32,
        level: Option<String>,
        category: Option<String>,
    },
    GetLogs {
        limit: u32,
        level: Option<String>,
    },
    GetDoctorReport,
    GenerateBugReport {
        verbose: bool,
        full_logs: bool,
        since: Option<String>,
    },
    ListRuleHistory {
        rule: Option<String>,
        limit: u32,
    },
    Search {
        query: String,
        limit: u32,
    },
    SyncNow {
        account_id: Option<AccountId>,
    },
    GetSyncStatus {
        account_id: AccountId,
    },
    SetFlags {
        message_id: MessageId,
        flags: MessageFlags,
    },
    Count {
        query: String,
    },
    GetHeaders {
        message_id: MessageId,
    },
    ListSavedSearches,
    CreateSavedSearch {
        name: String,
        query: String,
    },
    DeleteSavedSearch {
        name: String,
    },
    RunSavedSearch {
        name: String,
        limit: u32,
    },
    // Mutations (Phase 2)
    Mutation(MutationCommand),
    Unsubscribe {
        message_id: MessageId,
    },
    Snooze {
        message_id: MessageId,
        wake_at: chrono::DateTime<chrono::Utc>,
    },
    Unsnooze {
        message_id: MessageId,
    },
    ListSnoozed,
    // Compose (Phase 2)
    PrepareReply {
        message_id: MessageId,
        reply_all: bool,
    },
    PrepareForward {
        message_id: MessageId,
    },
    SendDraft {
        draft: Draft,
    },
    /// Save draft to the mail server (e.g. Gmail Drafts folder).
    SaveDraftToServer {
        draft: Draft,
    },
    ListDrafts,

    // Export (Phase 3)
    ExportThread {
        thread_id: ThreadId,
        format: ExportFormat,
    },
    ExportSearch {
        query: String,
        format: ExportFormat,
    },

    GetStatus,
    Ping,
    Shutdown,
}

/// Mutation commands for modifying messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mutation")]
pub enum MutationCommand {
    Archive {
        message_ids: Vec<MessageId>,
    },
    Trash {
        message_ids: Vec<MessageId>,
    },
    Spam {
        message_ids: Vec<MessageId>,
    },
    Star {
        message_ids: Vec<MessageId>,
        starred: bool,
    },
    SetRead {
        message_ids: Vec<MessageId>,
        read: bool,
    },
    ModifyLabels {
        message_ids: Vec<MessageId>,
        add: Vec<String>,
        remove: Vec<String>,
    },
    Move {
        message_ids: Vec<MessageId>,
        target_label: String,
    },
}

/// Reply context returned by PrepareReply.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplyContext {
    pub in_reply_to: String,
    pub reply_to: String,
    pub cc: String,
    pub subject: String,
    pub from: String,
    pub thread_context: String,
}

/// Forward context returned by PrepareForward.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardContext {
    pub subject: String,
    pub from: String,
    pub forwarded_content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
#[allow(clippy::large_enum_variant)]
pub enum Response {
    Ok { data: ResponseData },
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
#[allow(clippy::large_enum_variant)]
pub enum ResponseData {
    Envelopes {
        envelopes: Vec<Envelope>,
    },
    Envelope {
        envelope: Envelope,
    },
    Body {
        body: MessageBody,
    },
    AttachmentFile {
        file: AttachmentFile,
    },
    Bodies {
        bodies: Vec<MessageBody>,
    },
    Thread {
        thread: Thread,
        messages: Vec<Envelope>,
    },
    Labels {
        labels: Vec<Label>,
    },
    Label {
        label: Label,
    },
    Rules {
        rules: Vec<serde_json::Value>,
    },
    RuleData {
        rule: serde_json::Value,
    },
    Accounts {
        accounts: Vec<AccountSummaryData>,
    },
    AccountsConfig {
        accounts: Vec<AccountConfigData>,
    },
    AccountStatus {
        message: String,
    },
    RuleFormData {
        form: RuleFormData,
    },
    RuleDryRun {
        results: Vec<serde_json::Value>,
    },
    EventLogEntries {
        entries: Vec<EventLogEntry>,
    },
    LogLines {
        lines: Vec<String>,
    },
    DoctorReport {
        report: DoctorReport,
    },
    BugReport {
        content: String,
    },
    RuleHistory {
        entries: Vec<serde_json::Value>,
    },
    SearchResults {
        results: Vec<SearchResultItem>,
    },
    SyncStatus {
        last_sync: Option<String>,
        status: String,
    },
    Count {
        count: u32,
    },
    Headers {
        headers: Vec<(String, String)>,
    },
    SavedSearches {
        searches: Vec<mxr_core::types::SavedSearch>,
    },
    SavedSearchData {
        search: mxr_core::types::SavedSearch,
    },
    Status {
        uptime_secs: u64,
        accounts: Vec<String>,
        total_messages: u32,
    },
    ReplyContext {
        context: ReplyContext,
    },
    ForwardContext {
        context: ForwardContext,
    },
    Drafts {
        drafts: Vec<Draft>,
    },
    SnoozedMessages {
        snoozed: Vec<Snoozed>,
    },
    ExportResult {
        content: String,
    },
    Pong,
    Ack,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultItem {
    pub message_id: MessageId,
    pub account_id: AccountId,
    pub thread_id: ThreadId,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentFile {
    pub attachment_id: AttachmentId,
    pub filename: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventLogEntry {
    pub timestamp: i64,
    pub level: String,
    pub category: String,
    pub account_id: Option<AccountId>,
    pub message_id: Option<String>,
    pub rule_id: Option<String>,
    pub summary: String,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorReport {
    pub healthy: bool,
    pub data_dir_exists: bool,
    pub database_exists: bool,
    pub index_exists: bool,
    pub socket_exists: bool,
    pub database_path: String,
    pub database_size_bytes: u64,
    pub index_path: String,
    pub index_size_bytes: u64,
    pub log_path: String,
    pub log_size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleFormData {
    pub id: Option<String>,
    pub name: String,
    pub condition: String,
    pub action: String,
    pub priority: i32,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountConfigData {
    pub key: String,
    pub name: String,
    pub email: String,
    pub sync: Option<AccountSyncConfigData>,
    pub send: Option<AccountSendConfigData>,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AccountSourceData {
    Runtime,
    Config,
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AccountEditModeData {
    Full,
    RuntimeOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountSummaryData {
    pub account_id: AccountId,
    pub key: Option<String>,
    pub name: String,
    pub email: String,
    pub provider_kind: String,
    pub sync_kind: Option<String>,
    pub send_kind: Option<String>,
    pub enabled: bool,
    pub is_default: bool,
    pub source: AccountSourceData,
    pub editable: AccountEditModeData,
    pub sync: Option<AccountSyncConfigData>,
    pub send: Option<AccountSendConfigData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AccountSyncConfigData {
    Gmail {
        client_id: String,
        client_secret: Option<String>,
        token_ref: String,
    },
    Imap {
        host: String,
        port: u16,
        username: String,
        password_ref: String,
        password: Option<String>,
        use_tls: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AccountSendConfigData {
    Gmail,
    Smtp {
        host: String,
        port: u16,
        username: String,
        password_ref: String,
        password: Option<String>,
        use_tls: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event")]
pub enum DaemonEvent {
    SyncCompleted {
        account_id: AccountId,
        messages_synced: u32,
    },
    SyncError {
        account_id: AccountId,
        error: String,
    },
    NewMessages {
        envelopes: Vec<Envelope>,
    },
    MessageUnsnoozed {
        message_id: MessageId,
    },
    LabelCountsUpdated {
        counts: Vec<LabelCount>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelCount {
    pub label_id: LabelId,
    pub unread_count: u32,
    pub total_count: u32,
}
