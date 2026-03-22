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
    ListEnvelopesByIds {
        message_ids: Vec<MessageId>,
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
    AuthorizeAccountConfig {
        account: AccountConfigData,
        reauthorize: bool,
    },
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
        #[serde(default)]
        offset: u32,
        mode: Option<SearchMode>,
        #[serde(default)]
        sort: Option<SortOrder>,
        explain: bool,
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
        mode: Option<SearchMode>,
    },
    GetHeaders {
        message_id: MessageId,
    },
    ListSavedSearches,
    ListSubscriptions {
        limit: u32,
    },
    GetSemanticStatus,
    EnableSemantic {
        enabled: bool,
    },
    InstallSemanticProfile {
        profile: SemanticProfile,
    },
    UseSemanticProfile {
        profile: SemanticProfile,
    },
    ReindexSemantic,
    CreateSavedSearch {
        name: String,
        query: String,
        search_mode: SearchMode,
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
    ReadAndArchive {
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
    pub references: Vec<String>,
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
    AccountOperation {
        result: AccountOperationResult,
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
        #[serde(default)]
        has_more: bool,
        explain: Option<SearchExplain>,
    },
    SyncStatus {
        sync: AccountSyncStatus,
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
    Subscriptions {
        subscriptions: Vec<mxr_core::types::SubscriptionSummary>,
    },
    SemanticStatus {
        snapshot: SemanticStatusSnapshot,
    },
    SavedSearchData {
        search: mxr_core::types::SavedSearch,
    },
    Status {
        uptime_secs: u64,
        accounts: Vec<String>,
        total_messages: u32,
        #[serde(default)]
        daemon_pid: Option<u32>,
        #[serde(default)]
        sync_statuses: Vec<AccountSyncStatus>,
        #[serde(default)]
        protocol_version: u32,
        #[serde(default)]
        daemon_version: Option<String>,
        #[serde(default)]
        daemon_build_id: Option<String>,
        #[serde(default)]
        repair_required: bool,
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
    pub mode: SearchMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchExplain {
    pub requested_mode: SearchMode,
    pub executed_mode: SearchMode,
    pub semantic_query: Option<String>,
    pub lexical_window: u32,
    pub dense_window: Option<u32>,
    pub lexical_candidates: u32,
    pub dense_candidates: u32,
    pub final_results: u32,
    pub rrf_k: Option<u32>,
    pub notes: Vec<String>,
    pub results: Vec<SearchExplainResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchExplainResult {
    pub rank: u32,
    pub message_id: MessageId,
    pub final_score: f32,
    pub lexical_rank: Option<u32>,
    pub lexical_score: Option<f32>,
    pub dense_rank: Option<u32>,
    pub dense_score: Option<f32>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DaemonHealthClass {
    #[default]
    Healthy,
    Degraded,
    RestartRequired,
    RepairRequired,
}

impl DaemonHealthClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Degraded => "degraded",
            Self::RestartRequired => "restart_required",
            Self::RepairRequired => "repair_required",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum IndexFreshness {
    #[default]
    Unknown,
    Current,
    Stale,
    Disabled,
    Indexing,
    Error,
    RepairRequired,
}

impl IndexFreshness {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Current => "current",
            Self::Stale => "stale",
            Self::Disabled => "disabled",
            Self::Indexing => "indexing",
            Self::Error => "error",
            Self::RepairRequired => "repair_required",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AccountSyncStatus {
    pub account_id: AccountId,
    pub account_name: String,
    pub last_attempt_at: Option<String>,
    pub last_success_at: Option<String>,
    pub last_error: Option<String>,
    pub failure_class: Option<String>,
    pub consecutive_failures: u32,
    pub backoff_until: Option<String>,
    pub sync_in_progress: bool,
    pub current_cursor_summary: Option<String>,
    pub last_synced_count: u32,
    pub healthy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorReport {
    pub healthy: bool,
    #[serde(default)]
    pub health_class: DaemonHealthClass,
    #[serde(default)]
    pub lexical_index_freshness: IndexFreshness,
    #[serde(default)]
    pub last_successful_sync_at: Option<String>,
    #[serde(default)]
    pub lexical_last_rebuilt_at: Option<String>,
    #[serde(default)]
    pub semantic_enabled: bool,
    #[serde(default)]
    pub semantic_active_profile: Option<String>,
    #[serde(default)]
    pub semantic_index_freshness: IndexFreshness,
    #[serde(default)]
    pub semantic_last_indexed_at: Option<String>,
    #[serde(default)]
    pub data_stats: DoctorDataStats,
    pub data_dir_exists: bool,
    pub database_exists: bool,
    pub index_exists: bool,
    pub socket_exists: bool,
    pub socket_reachable: bool,
    pub stale_socket: bool,
    pub daemon_running: bool,
    pub daemon_pid: Option<u32>,
    #[serde(default)]
    pub daemon_protocol_version: u32,
    #[serde(default)]
    pub daemon_version: Option<String>,
    #[serde(default)]
    pub daemon_build_id: Option<String>,
    pub index_lock_held: bool,
    pub index_lock_error: Option<String>,
    #[serde(default)]
    pub restart_required: bool,
    #[serde(default)]
    pub repair_required: bool,
    pub database_path: String,
    pub database_size_bytes: u64,
    pub index_path: String,
    pub index_size_bytes: u64,
    pub log_path: String,
    pub log_size_bytes: u64,
    pub sync_statuses: Vec<AccountSyncStatus>,
    pub recent_sync_events: Vec<EventLogEntry>,
    pub recent_error_logs: Vec<String>,
    pub recommended_next_steps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DoctorDataStats {
    pub accounts: u32,
    pub labels: u32,
    pub messages: u32,
    pub unread_messages: u32,
    pub starred_messages: u32,
    pub messages_with_attachments: u32,
    pub message_labels: u32,
    pub bodies: u32,
    pub attachments: u32,
    pub drafts: u32,
    pub snoozed: u32,
    pub saved_searches: u32,
    pub rules: u32,
    pub rule_logs: u32,
    pub sync_log: u32,
    pub sync_runtime_statuses: u32,
    pub event_log: u32,
    pub semantic_profiles: u32,
    pub semantic_chunks: u32,
    pub semantic_embeddings: u32,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum GmailCredentialSourceData {
    #[default]
    Bundled,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AccountSyncConfigData {
    Gmail {
        #[serde(default)]
        credential_source: GmailCredentialSourceData,
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
pub struct AccountOperationStep {
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountOperationResult {
    pub ok: bool,
    pub summary: String,
    pub save: Option<AccountOperationStep>,
    pub auth: Option<AccountOperationStep>,
    pub sync: Option<AccountOperationStep>,
    pub send: Option<AccountOperationStep>,
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
