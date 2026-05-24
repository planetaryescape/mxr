use mxr_core::id::*;
use mxr_core::types::*;
use serde::{Deserialize, Serialize};

mod platform;
pub use platform::*;

/// IPC items are grouped conceptually, even though the wire format stays flat.
///
/// The daemon serves reusable truth and workflows, not screen-specific payloads.
/// Provider-specific adaptation happens below this layer in adapter crates.
/// Client-specific shaping and view state belong in clients such as the TUI and web bridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "kebab-case")]
pub enum IpcCategory {
    CoreMail,
    MxrPlatform,
    AdminMaintenance,
    ClientSpecific,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum DraftSafetyModeData {
    #[default]
    Check,
    Send,
    ScheduledFlush,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DraftSafetyContextData {
    #[serde(default)]
    pub mode: DraftSafetyModeData,
    #[serde(default)]
    pub reply_all: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_message_id: Option<MessageId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<ThreadId>,
    #[serde(default = "default_allow_llm")]
    pub allow_llm: bool,
    /// When set, the safety pipeline runs the send-time check
    /// against the recipients and emits a `Severity::Info` issue
    /// if the proposed slot is materially slower than the fastest
    /// historic bucket. `None` = check skipped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_send_at: Option<chrono::DateTime<chrono::Utc>>,
}

fn default_allow_llm() -> bool {
    true
}

fn default_owed_reply_limit() -> u32 {
    50
}

fn default_archive_ask_limit() -> u32 {
    8
}

fn default_decision_log_limit() -> u32 {
    50
}

fn default_decision_log_since_days() -> u32 {
    180
}

fn default_suggest_recipients_limit() -> u32 {
    5
}

fn default_expert_limit() -> u32 {
    5
}

fn default_whois_limit() -> u32 {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct LlmStatusSnapshot {
    pub enabled: bool,
    pub provider: String,
    pub model: String,
    pub configured_model: String,
    pub base_url: Option<String>,
    pub api_key_env: Option<String>,
    pub api_key_present: bool,
    pub context_window: u32,
    pub supports_streaming: bool,
    pub request_timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ThreadSummaryData {
    pub text: String,
    pub model: String,
    pub generated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct LlmConfigData {
    pub enabled: bool,
    pub base_url: String,
    pub model: String,
    pub api_key_env: String,
    pub context_window: u32,
    pub request_timeout_secs: u64,
    #[serde(default)]
    pub allow_cloud_relationship_data: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overrides: Option<LlmOverridesData>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(default)]
pub struct LlmOverridesData {
    pub summarize: Option<LlmOverrideData>,
    pub relationship_summary: Option<LlmOverrideData>,
    pub commitments: Option<LlmOverrideData>,
    pub draft_assist: Option<LlmOverrideData>,
    pub draft_new: Option<LlmOverrideData>,
    pub draft_refine: Option<LlmOverrideData>,
    pub voice_match: Option<LlmOverrideData>,
    pub humanize_rewrite: Option<LlmOverrideData>,
    pub answer_coverage: Option<LlmOverrideData>,
    pub archive_ask: Option<LlmOverrideData>,
    pub decision_log: Option<LlmOverrideData>,
    pub briefing: Option<LlmOverrideData>,
    pub expert: Option<LlmOverrideData>,
    #[serde(default)]
    pub delivery_extraction: Option<LlmOverrideData>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(default)]
pub struct LlmOverrideData {
    pub enabled: Option<bool>,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub api_key_env: Option<String>,
    pub context_window: Option<u32>,
    pub request_timeout_secs: Option<u64>,
}

// =========================================================================
// Activity log types — shared with `crates/store` via the boundary
// converters at the bottom of this section. The `store` crate has its own
// `Tier`/`ActivityFilter` because `protocol` is a leaf crate; we duplicate
// the shape rather than pull in a heavier dep graph.
// =========================================================================

/// Mirror of `mxr_store::Tier`. Three retention buckets used by the
/// activity log: 30 / 90 / 365 days for ephemeral / standard / important.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum ActivityTier {
    Ephemeral,
    Standard,
    Important,
}

impl ActivityTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ephemeral => "ephemeral",
            Self::Standard => "standard",
            Self::Important => "important",
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ActivityFilter {
    /// Unix ms inclusive lower bound.
    pub since: Option<i64>,
    /// Unix ms exclusive upper bound.
    pub until: Option<i64>,
    pub account_id: Option<String>,
    /// Empty = any.
    #[serde(default)]
    pub sources: Vec<ClientKind>,
    #[serde(default)]
    pub actions: Vec<String>,
    pub action_prefix: Option<String>,
    pub target_kind: Option<String>,
    pub target_id: Option<String>,
    #[serde(default)]
    pub tiers: Vec<ActivityTier>,
    /// FTS5 expression against `context_json`.
    pub query: Option<String>,
    #[serde(default)]
    pub include_redacted: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ActivityCursor {
    pub ts: i64,
    pub id: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ActivityStatGroupBy {
    Action,
    Day,
    Source,
    TargetKind,
    Hour,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum ActivityExportFormat {
    Csv,
    Json,
    Ndjson,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ActivityEntry {
    pub id: i64,
    pub ts: i64,
    pub account_id: Option<String>,
    pub source: ClientKind,
    pub action: String,
    pub target_kind: Option<String>,
    pub target_id: Option<String>,
    pub tier: ActivityTier,
    /// Parsed context; clients see structured JSON, not the raw string.
    pub context: Option<serde_json::Value>,
    pub redacted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ActivityStatBucket {
    /// Group key — depends on `group_by`: action token, ISO date, source,
    /// target kind, or hour-of-day string `00`..`23`.
    pub key: String,
    pub count: i64,
}

/// Wire shape of a saved activity filter preset (Phase 8).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SavedActivityFilterEntry {
    pub slug: String,
    pub name: String,
    pub filter: ActivityFilter,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_used_at: Option<i64>,
}

/// Originating client for an IPC request. Carried on the envelope so the
/// daemon's activity recorder can tag rows with the surface that produced
/// them. Legacy clients (pre-source-field) decode as `Cli` — the most
/// realistic guess for scripts hand-rolled against the socket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum ClientKind {
    Tui,
    Cli,
    Web,
    /// Synthesized internally by the daemon (scheduled prunes, pause markers, etc.).
    Daemon,
}

impl ClientKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Tui => "tui",
            Self::Cli => "cli",
            Self::Web => "web",
            Self::Daemon => "daemon",
        }
    }

    pub fn default_for_legacy() -> Self {
        Self::Cli
    }
}

impl Default for ClientKind {
    fn default() -> Self {
        Self::default_for_legacy()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct IpcMessage {
    pub id: u64,
    /// Originating client surface. Optional on the wire for backwards
    /// compatibility with pre-Phase-2 clients; the default is `Cli`. New
    /// clients always set this explicitly.
    #[serde(default)]
    pub source: ClientKind,
    pub payload: IpcPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "type")]
#[expect(
    clippy::large_enum_variant,
    reason = "IPC envelope keeps request/response/event payloads transparent on the wire"
)]
pub enum IpcPayload {
    Request(Request),
    Response(Response),
    Event(DaemonEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "cmd")]
pub enum Request {
    // Core mail/runtime. This is the most stable bucket.
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
    GetInvite {
        message_id: MessageId,
    },
    ListInvites {
        limit: u32,
    },
    BackfillCalendarInvites,
    RespondInvite {
        message_id: MessageId,
        action: CalendarInviteActionData,
        dry_run: bool,
    },
    /// Build (without sending) the iCal REPLY for an invite — used by the
    /// "respond with comment" compose path so the SPA / TUI receive the same
    /// `CalendarInviteResponsePreview` the auto-send path uses internally.
    PrepareInviteResponse {
        message_id: MessageId,
        action: CalendarInviteActionData,
    },
    /// Mark a previously-received invite as answered in the local store.
    /// Called by the daemon's send-completion hook after a comment-compose
    /// draft (`Draft.inline_calendar_reply.is_some()`) finishes sending, since
    /// that path bypasses `RespondInvite`.
    MarkInviteAnswered {
        message_id: MessageId,
        attendee_email: String,
        partstat: mxr_core::CalendarPartstat,
    },
    GetHtmlImageAssets {
        message_id: MessageId,
        allow_remote: bool,
    },
    DownloadAttachment {
        message_id: MessageId,
        attachment_id: AttachmentId,
        /// Optional user-chosen destination path. When `None`, the daemon
        /// materializes into its internal attachment cache (used by the
        /// open-in-app flow). When `Some`, the daemon writes the bytes
        /// directly to this exact path (parent dirs are created; caller
        /// owns the filename in the path).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "openapi", schema(value_type = Option<String>))]
        destination: Option<std::path::PathBuf>,
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
    /// Paginated list of threads in date-descending order (most-recent
    /// first), with optional account/label filters. Each returned
    /// `Thread` carries its full `message_ids` member list.
    ListThreads {
        account_id: Option<AccountId>,
        label_id: Option<LabelId>,
        limit: u32,
        offset: u32,
        #[serde(default)]
        sort: Option<SortOrder>,
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

    // mxr app/platform. Product/runtime capabilities shared by multiple clients.
    ListAccounts,
    ListAccountsConfig,
    AuthorizeAccountConfig {
        account: AccountConfigData,
        reauthorize: bool,
    },
    StartAuthSession {
        account: AccountConfigData,
        reauthorize: bool,
        #[serde(default)]
        flow: AuthFlowData,
    },
    GetAuthSession {
        session_id: AuthSessionId,
    },
    CancelAuthSession {
        session_id: AuthSessionId,
    },
    CompleteAuthSession {
        session_id: AuthSessionId,
        save_account: bool,
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
    DisableAccountConfig {
        key: String,
    },
    RemoveAccountConfig {
        key: String,
        purge_local_data: bool,
        dry_run: bool,
    },
    RepairAccountConfig {
        account: AccountConfigData,
    },
    ListRules,
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

    ListSavedSearches,
    /// Return unread match counts per saved search. The TUI uses
    /// this to render `(N)` after each tab label. The handler runs
    /// each saved search's query ANDed with `is:unread` and counts
    /// hits; queries that can't be parsed return 0 so the tab strip
    /// stays visible.
    ListSavedSearchUnreadCounts,
    ListSubscriptions {
        account_id: Option<AccountId>,
        limit: u32,
    },
    ListStorageBreakdown {
        account_id: Option<AccountId>,
        group_by: StorageGroupBy,
        limit: u32,
    },
    ListLargestMessages {
        account_id: Option<AccountId>,
        since_days: Option<u32>,
        limit: u32,
    },
    Wrapped {
        account_id: Option<AccountId>,
        since_unix: i64,
        until_unix: i64,
        label: String,
    },
    ListStaleThreads {
        account_id: Option<AccountId>,
        perspective: StaleBallInCourt,
        older_than_days: u32,
        within_days: u32,
        limit: u32,
    },
    ListContactAsymmetry {
        account_id: Option<AccountId>,
        min_inbound: u32,
        limit: u32,
    },
    ListContactDecay {
        account_id: Option<AccountId>,
        threshold_days: u32,
        max_lookback_days: u32,
        limit: u32,
    },
    RefreshContacts,
    RebuildAnalytics,
    /// Walk every message body and recompute `link_count` + `body_word_count`
    /// using the current link-extractor. Drives `mxr doctor --recompute-link-counts`
    /// for users who want the link indicator/search filters to populate on
    /// pre-existing rows without waiting for the next sync.
    RecomputeLinkCounts,
    ListResponseTime {
        account_id: Option<AccountId>,
        direction: ResponseTimeDirection,
        counterparty: Option<String>,
        since_days: Option<u32>,
    },
    ListAccountAddresses {
        account_id: AccountId,
    },
    AddAccountAddress {
        account_id: AccountId,
        email: String,
        primary: bool,
    },
    RemoveAccountAddress {
        account_id: AccountId,
        email: String,
    },
    SetPrimaryAccountAddress {
        account_id: AccountId,
        email: String,
    },
    GetLlmStatus,
    GetLlmConfig,
    UpdateLlmConfig {
        config: Box<LlmConfigData>,
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
    BackfillSemantic,
    CreateSavedSearch {
        name: String,
        query: String,
        search_mode: SearchMode,
    },
    DeleteSavedSearch {
        name: String,
    },
    /// Patch a saved search by its current name. Any `None` field is left
    /// untouched. `icon` is overloaded to also carry a hex color (the web app
    /// uses it for the pinned-row color tag).
    UpdateSavedSearch {
        name: String,
        new_name: Option<String>,
        query: Option<String>,
        search_mode: Option<SearchMode>,
        sort: Option<mxr_core::types::SortOrder>,
        icon: Option<String>,
        position: Option<i32>,
    },
    RunSavedSearch {
        name: String,
        limit: u32,
    },

    // Admin / maintenance / operational. Legitimate daemon features, fenced off
    // from the core mail contract.
    ListEvents {
        limit: u32,
        level: Option<String>,
        category: Option<String>,
        /// Unix-seconds inclusive lower bound on `timestamp`. Optional.
        #[serde(default)]
        since: Option<i64>,
        /// Unix-seconds exclusive upper bound on `timestamp`. Optional.
        #[serde(default)]
        until: Option<i64>,
        /// Free-text `LIKE %term%` over the `summary` column. Optional.
        #[serde(default)]
        search: Option<String>,
        /// Category prefix (e.g. `mutation`, `sync.`). Applied as
        /// `category LIKE <prefix>%`. Optional.
        #[serde(default)]
        category_prefix: Option<String>,
        /// Result offset for paging. Defaults to 0.
        #[serde(default)]
        offset: u32,
    },
    GetLogs {
        limit: u32,
        level: Option<String>,
        /// Free-text substring filter against each log line. Case-insensitive.
        #[serde(default)]
        search: Option<String>,
    },
    /// Distinct categories present in `event_log`, ordered by recency.
    ListEventCategories,
    /// Count events matching the same filter shape as `ListEvents`.
    /// Powers pagination affordances in the diagnostics surfaces.
    CountEvents {
        #[serde(default)]
        level: Option<String>,
        #[serde(default)]
        category: Option<String>,
        #[serde(default)]
        category_prefix: Option<String>,
        #[serde(default)]
        since: Option<i64>,
        #[serde(default)]
        until: Option<i64>,
        #[serde(default)]
        search: Option<String>,
    },
    GetDoctorReport,
    GenerateBugReport {
        verbose: bool,
        full_logs: bool,
        since: Option<String>,
    },

    // Core mail/runtime. Reusable workflows and data, not screen payloads.
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
        #[cfg_attr(feature = "openapi", schema(value_type = u32))]
        flags: MessageFlags,
    },
    Count {
        query: String,
        mode: Option<SearchMode>,
    },
    GetHeaders {
        message_id: MessageId,
    },
    ListRuleHistory {
        rule: Option<String>,
        limit: u32,
    },
    Mutation {
        #[serde(flatten)]
        mutation: MutationCommand,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        client_correlation_id: Option<String>,
    },
    /// Reverse a recent undoable mutation by id. Available within ~60s of
    /// the mutation landing; daemon refuses with an `Error` response
    /// past that window.
    UndoMutation {
        mutation_id: String,
    },
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
    /// Toggle the reply-later flag on a message. Local-only metadata —
    /// never roundtrips to the provider. Setting a flag that is already
    /// set refreshes the timestamp, surfacing the message to the top of
    /// the queue.
    SetReplyLater {
        message_id: MessageId,
        flag: bool,
    },
    /// List messages currently flagged for reply-later, ordered by most
    /// recently flagged first.
    ListReplyQueue,
    /// Schedule a reminder for an outbound message: "remind me if no
    /// reply by `remind_at`." Re-sending overwrites any prior reminder
    /// on the same message.
    SetAutoReminder {
        sent_message_id: MessageId,
        remind_at: chrono::DateTime<chrono::Utc>,
    },
    /// Cancel a pending reminder.
    CancelAutoReminder {
        sent_message_id: MessageId,
    },
    /// Schedule an existing draft to be sent at `send_at`. The flusher
    /// loop scans due rows on a 60-second cadence and runs them through
    /// the same `send_stored_draft` pipeline that interactive sends use.
    ScheduleSend {
        draft_id: DraftId,
        send_at: chrono::DateTime<chrono::Utc>,
    },
    /// Cancel a pending scheduled send (the draft itself is preserved).
    CancelScheduledSend {
        draft_id: DraftId,
    },
    /// List all snippets, alphabetically by name.
    ListSnippets,
    /// Create or update a snippet by name.
    SetSnippet {
        name: String,
        body: String,
        vars: Vec<String>,
    },
    /// Delete a snippet by name. No-op if absent.
    DeleteSnippet {
        name: String,
    },
    /// List tracked deliveries. `filter` is one of "active" (default),
    /// "delivered", "all", "dismissed".
    ListDeliveries {
        #[serde(default)]
        filter: Option<String>,
    },
    /// Fetch a single delivery, including its source message ids.
    GetDelivery {
        delivery_id: DeliveryId,
    },
    /// Resolve a delivery (mark delivered/done so it leaves the active list).
    ResolveDelivery {
        delivery_id: DeliveryId,
    },
    /// Dismiss a delivery (hide a false positive).
    DismissDelivery {
        delivery_id: DeliveryId,
    },
    /// Re-scan recent mail for deliveries. `dry_run` reports what would be
    /// created/updated without writing. `since_days` bounds the window.
    ScanDeliveries {
        #[serde(default)]
        since_days: Option<u32>,
        #[serde(default)]
        dry_run: bool,
    },
    /// List outgoing compose signatures, alphabetically by name.
    ListSignatures,
    /// List scoped signature defaults.
    ListSignatureDefaults,
    /// Create or update a signature by name.
    SetSignature {
        name: String,
        body: String,
    },
    /// Delete a signature by name. Also clears defaults pointing at it.
    DeleteSignature {
        name: String,
    },
    /// Set the default signature for a global/account/from-address scope.
    SetSignatureDefault {
        name: String,
        kind: SignatureContextData,
        account_id: Option<AccountId>,
        from_email: Option<String>,
    },
    /// Clear the default signature for a global/account/from-address scope.
    ClearSignatureDefault {
        kind: SignatureContextData,
        account_id: Option<AccountId>,
        from_email: Option<String>,
    },
    /// Resolve the signature to insert into a compose draft.
    ResolveSignature {
        name: Option<String>,
        kind: SignatureContextData,
        account_id: Option<AccountId>,
        from_email: Option<String>,
    },
    /// Per-sender relationship aggregates: volume, response cadence,
    /// open threads. Returns `None` (via `Ok`/`SenderProfileData` with
    /// `present=false`) if the contact is unknown.
    GetSenderProfile {
        account_id: AccountId,
        email: String,
    },
    ListSenders {
        #[serde(default = "default_sender_limit")]
        limit: u32,
        /// Restrict counts to messages whose `date >= since_unix`.
        /// `None` means "no time bound" (legacy behavior).
        #[serde(default)]
        since_unix: Option<i64>,
    },
    GetRelationshipProfile {
        account_id: AccountId,
        email: String,
    },
    RebuildRelationshipProfile {
        account_id: AccountId,
        email: String,
    },
    ListCommitments {
        account_id: AccountId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        email: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        status: Option<CommitmentStatusData>,
    },
    ResolveCommitment {
        commitment_id: String,
    },
    GetUserVoice {
        account_id: AccountId,
    },
    RebuildUserVoice {
        account_id: AccountId,
    },
    HumanizerScore {
        text: String,
    },
    HumanizerRewrite {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_iterations: Option<u8>,
    },
    /// List senders who've sent inbound messages but don't have a
    /// screener decision yet.
    ListScreenerQueue {
        account_id: AccountId,
        #[serde(default = "default_screener_limit")]
        limit: u32,
    },
    /// All screener decisions for an account.
    ListScreenerDecisions {
        account_id: AccountId,
    },
    /// Set or update the screener disposition for one sender.
    SetScreenerDecision {
        account_id: AccountId,
        sender_email: String,
        disposition: ScreenerDispositionData,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        route_label: Option<String>,
    },
    /// Clear an existing screener decision (returns the sender to
    /// "no decision yet" state).
    ClearScreenerDecision {
        account_id: AccountId,
        sender_email: String,
    },
    /// Summarize a thread using the configured LLM (structured prompt in
    /// `handler/summarize.rs`; no fixed sentence-count contract). Returns
    /// `LlmDisabled` when LLM is not configured.
    SummarizeThread {
        thread_id: ThreadId,
    },
    /// Generate a draft reply grounded on the user's prior sent
    /// messages and the current thread context. Caller is responsible
    /// for opening the result in `$EDITOR` for review — the result is
    /// never auto-sent.
    DraftAssist {
        thread_id: ThreadId,
        instruction: String,
    },
    DraftNew {
        account_id: AccountId,
        to: Address,
        purpose: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        register: Option<VoiceRegisterData>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        length_hint: Option<DraftLengthHintData>,
    },
    DraftRefine {
        draft_id: DraftId,
        knobs: DraftRefineKnobsData,
    },
    PrepareReply {
        message_id: MessageId,
        reply_all: bool,
    },
    PrepareForward {
        message_id: MessageId,
    },
    SendDraft {
        draft: Draft,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        override_safety_token: Option<String>,
    },
    SaveDraft {
        draft: Draft,
    },
    SendStoredDraft {
        draft_id: DraftId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        override_safety_token: Option<String>,
    },
    /// Run the safety pipeline against a draft without actually sending.
    /// Mirrors the gate that `SendDraft` and `SendStoredDraft` apply
    /// before reaching the provider, so callers can preview the report
    /// from `mxr send --check`, `mxr compose --check`, or the TUI
    /// confirm modal.
    CheckDraftSafety {
        draft: Draft,
        #[serde(default)]
        context: DraftSafetyContextData,
    },
    /// Extract per-draft commitment candidates (deterministic prefilter
    /// plus LLM, gated by `LlmFeature::Commitments`). Persists into
    /// `draft_commitment_candidates`; on successful send, candidates
    /// promote to `contact_commitments`.
    ExtractDraftCommitments {
        draft: Draft,
    },
    /// Explain an entity (email or free-text term) using local
    /// evidence. Returns a person summary for emails, a citation-
    /// backed summary for terms, and a candidate list for ambiguous
    /// queries.
    ExplainEntity {
        account_id: AccountId,
        query: String,
        #[serde(default = "default_whois_limit")]
        limit: u32,
    },
    /// Find people in the local archive who have answered similar
    /// questions before. Ranking distinguishes answerers from askers.
    FindExpert {
        account_id: AccountId,
        query: String,
        #[serde(default)]
        include_self: bool,
        #[serde(default = "default_expert_limit")]
        limit: u32,
    },
    /// Suggest "maybe include" recipients given a draft. Excludes
    /// addresses already on the draft and never reveals Bcc'd
    /// addresses from prior threads.
    SuggestCollaborators {
        draft: Draft,
        #[serde(default = "default_suggest_recipients_limit")]
        limit: u32,
    },
    /// Render a thread briefing for someone returning to a dormant
    /// thread. Cached unless `refresh = true`.
    GetThreadBriefing {
        thread_id: ThreadId,
        #[serde(default)]
        refresh: bool,
    },
    /// Render a recipient briefing for compose-time context.
    GetRecipientBriefing {
        account_id: AccountId,
        email: String,
        #[serde(default)]
        refresh: bool,
    },
    /// Add a contact to the cadence watchlist.
    WatchCadence {
        account_id: AccountId,
        email: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_days: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
        #[serde(default)]
        allow_list_sender: bool,
    },
    /// Remove a contact from the cadence watchlist.
    UnwatchCadence {
        account_id: AccountId,
        email: String,
    },
    /// List currently watched contacts.
    ListCadenceWatch {
        account_id: AccountId,
    },
    /// List watched contacts whose interval since last contact has
    /// exceeded their cadence (override or contact p50, default 30d).
    ListCadenceDrift {
        account_id: AccountId,
    },
    /// Recommend the bucket (weekday, hour) at which the recipient
    /// is fastest to reply. When `proposed_at` is set, the response
    /// also includes the proposed slot's expected reply time so the
    /// caller can render a delta against the recipient's best bucket.
    SendTimeRecommendation {
        account_id: AccountId,
        recipients: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        proposed_at: Option<chrono::DateTime<chrono::Utc>>,
    },
    /// Walk every thread in the account whose latest message is
    /// within `since_days` days, run the LLM-backed decision-log
    /// extractor on each candidate, and persist new entries.
    /// Idempotent: re-running with the same thread content does
    /// not produce duplicate rows (source_hash dedupes).
    RebuildDecisionLog {
        account_id: AccountId,
        #[serde(default = "default_decision_log_since_days")]
        since_days: u32,
    },
    /// Fetch a single decision-log row by id. Returns
    /// `ResponseData::DecisionDetail` with `Option<DecisionLogEntryData>`
    /// — `None` means the id is unknown (not an error).
    GetDecision {
        id: String,
    },
    /// List entries from the decision log. Optional topic and
    /// since-days filters.
    ListDecisionLog {
        account_id: AccountId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        topic: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        since_days: Option<u32>,
        #[serde(default = "default_decision_log_limit")]
        limit: u32,
    },
    /// Citation-validated archive question. Returns a Markdown answer
    /// plus the cited source messages; LLM-fabricated citations
    /// (msg ids not in the retrieved set) are rejected at the daemon.
    ArchiveAsk {
        question: String,
        #[serde(default)]
        filters: ArchiveAskFiltersData,
        #[serde(default = "default_archive_ask_limit")]
        limit: u32,
    },
    /// List "owed reply" threads for an account: latest inbound has
    /// not been followed by an outbound, ranked by overdue ratio.
    ListOwedReplies {
        account_id: AccountId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        older_than_days: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        within_days: Option<u32>,
        #[serde(default = "default_owed_reply_limit")]
        limit: u32,
    },
    DeleteDraft {
        draft_id: DraftId,
    },
    /// Save draft to the mail server (e.g. Gmail Drafts folder).
    SaveDraftToServer {
        draft: Draft,
    },
    ListDrafts,
    /// List drafts that look orphaned mid-send: status `'sending'` with
    /// a stale `last_heartbeat_at` (or `status_updated_at` fallback)
    /// older than 1h. Surfaces what the daemon-startup recovery loop
    /// would also auto-reset.
    ListOrphanedDrafts,
    /// Force-reset an orphaned `'sending'` draft back to `'draft'` so
    /// the user can retry the send. Idempotent: already-`'draft'` rows
    /// return the no-op variant. `'sent'` rows refuse.
    ResetOrphanedDraft {
        draft_id: DraftId,
    },
    ExportThread {
        thread_id: ThreadId,
        format: ExportFormat,
    },
    ExportSearch {
        query: String,
        format: ExportFormat,
    },

    // Admin / maintenance / operational utilities.
    GetStatus,
    Ping,
    Shutdown,

    // ============================================================
    // Activity log — `user_activity` table queries and mutations.
    // See `docs/activity-log.md`. Strictly local; never
    // transmitted off-device.
    // ============================================================
    /// Paginated reverse-chron list of activity rows.
    ListActivity {
        filter: ActivityFilter,
        limit: u32,
        cursor: Option<ActivityCursor>,
    },
    /// Total matching rows (UI badge counts).
    CountActivity {
        filter: ActivityFilter,
    },
    /// Grouped counts over a time window.
    ActivityStats {
        since: i64,
        until: i64,
        group_by: ActivityStatGroupBy,
    },
    /// Export matching rows in CSV / JSON / NDJSON. When `path` is set the
    /// daemon writes the file; otherwise it returns the body inline (capped).
    ExportActivity {
        filter: ActivityFilter,
        format: ActivityExportFormat,
        path: Option<String>,
    },
    /// Tombstone rows (set `redacted=1`, clear `context_json`). Either
    /// `ids` is non-empty OR `filter` is `Some` — not both, not neither.
    RedactActivity {
        ids: Vec<i64>,
        filter: Option<ActivityFilter>,
        dry_run: bool,
    },
    /// Hard-delete rows older than `before_ts`. Retention pruner uses this.
    PruneActivity {
        before_ts: i64,
        tier: Option<ActivityTier>,
        dry_run: bool,
    },
    /// Stop recording new rows. `until_ts=None` is indefinite.
    PauseActivity {
        until_ts: Option<i64>,
    },
    /// Resume recording.
    ResumeActivity,

    // ----- Phase 8 — saved activity filters -----
    /// List all saved filter presets, most-recently-used first.
    ListSavedActivityFilters,
    /// Fetch a single preset by slug.
    GetSavedActivityFilter {
        slug: String,
    },
    /// Create or update a preset (slug is the primary key).
    UpsertSavedActivityFilter {
        slug: String,
        name: String,
        filter: ActivityFilter,
    },
    /// Delete a preset by slug.
    DeleteSavedActivityFilter {
        slug: String,
    },
}

impl Request {
    /// Convenience constructor — correlation id omitted (clients add when tracking optimism).
    pub fn mutation(command: MutationCommand) -> Self {
        Self::Mutation {
            mutation: command,
            client_correlation_id: None,
        }
    }

    pub const fn category(&self) -> IpcCategory {
        match self {
            Self::ListEnvelopes { .. }
            | Self::ListEnvelopesByIds { .. }
            | Self::GetEnvelope { .. }
            | Self::GetBody { .. }
            | Self::GetInvite { .. }
            | Self::ListInvites { .. }
            | Self::BackfillCalendarInvites
            | Self::RespondInvite { .. }
            | Self::PrepareInviteResponse { .. }
            | Self::MarkInviteAnswered { .. }
            | Self::GetHtmlImageAssets { .. }
            | Self::DownloadAttachment { .. }
            | Self::OpenAttachment { .. }
            | Self::ListBodies { .. }
            | Self::GetThread { .. }
            | Self::ListThreads { .. }
            | Self::ListLabels { .. }
            | Self::CreateLabel { .. }
            | Self::DeleteLabel { .. }
            | Self::RenameLabel { .. }
            | Self::Search { .. }
            | Self::SyncNow { .. }
            | Self::GetSyncStatus { .. }
            | Self::SetFlags { .. }
            | Self::Count { .. }
            | Self::GetHeaders { .. }
            | Self::ListRuleHistory { .. }
            | Self::Mutation { .. }
            | Self::UndoMutation { .. }
            | Self::Unsubscribe { .. }
            | Self::Snooze { .. }
            | Self::Unsnooze { .. }
            | Self::ListSnoozed
            | Self::SetReplyLater { .. }
            | Self::ListReplyQueue
            | Self::SetAutoReminder { .. }
            | Self::CancelAutoReminder { .. }
            | Self::ScheduleSend { .. }
            | Self::CancelScheduledSend { .. }
            | Self::ListSnippets
            | Self::SetSnippet { .. }
            | Self::DeleteSnippet { .. }
            | Self::ListDeliveries { .. }
            | Self::GetDelivery { .. }
            | Self::ResolveDelivery { .. }
            | Self::DismissDelivery { .. }
            | Self::ScanDeliveries { .. }
            | Self::ListSignatures
            | Self::ListSignatureDefaults
            | Self::SetSignature { .. }
            | Self::DeleteSignature { .. }
            | Self::SetSignatureDefault { .. }
            | Self::ClearSignatureDefault { .. }
            | Self::ResolveSignature { .. }
            | Self::GetSenderProfile { .. }
            | Self::ListSenders { .. }
            | Self::GetRelationshipProfile { .. }
            | Self::RebuildRelationshipProfile { .. }
            | Self::ListCommitments { .. }
            | Self::ResolveCommitment { .. }
            | Self::GetUserVoice { .. }
            | Self::RebuildUserVoice { .. }
            | Self::HumanizerScore { .. }
            | Self::HumanizerRewrite { .. }
            | Self::ListScreenerQueue { .. }
            | Self::ListScreenerDecisions { .. }
            | Self::SetScreenerDecision { .. }
            | Self::ClearScreenerDecision { .. }
            | Self::SummarizeThread { .. }
            | Self::DraftAssist { .. }
            | Self::DraftNew { .. }
            | Self::DraftRefine { .. }
            | Self::PrepareReply { .. }
            | Self::PrepareForward { .. }
            | Self::SendDraft { .. }
            | Self::SaveDraft { .. }
            | Self::SendStoredDraft { .. }
            | Self::CheckDraftSafety { .. }
            | Self::ExtractDraftCommitments { .. }
            | Self::ListOwedReplies { .. }
            | Self::ArchiveAsk { .. }
            | Self::ListDecisionLog { .. }
            | Self::GetDecision { .. }
            | Self::RebuildDecisionLog { .. }
            | Self::SendTimeRecommendation { .. }
            | Self::WatchCadence { .. }
            | Self::UnwatchCadence { .. }
            | Self::ListCadenceWatch { .. }
            | Self::ListCadenceDrift { .. }
            | Self::GetThreadBriefing { .. }
            | Self::GetRecipientBriefing { .. }
            | Self::SuggestCollaborators { .. }
            | Self::FindExpert { .. }
            | Self::ExplainEntity { .. }
            | Self::DeleteDraft { .. }
            | Self::SaveDraftToServer { .. }
            | Self::ListDrafts
            | Self::ListOrphanedDrafts
            | Self::ResetOrphanedDraft { .. }
            | Self::ExportThread { .. }
            | Self::ExportSearch { .. } => IpcCategory::CoreMail,
            Self::ListAccounts
            | Self::ListAccountsConfig
            | Self::AuthorizeAccountConfig { .. }
            | Self::StartAuthSession { .. }
            | Self::GetAuthSession { .. }
            | Self::CancelAuthSession { .. }
            | Self::CompleteAuthSession { .. }
            | Self::UpsertAccountConfig { .. }
            | Self::SetDefaultAccount { .. }
            | Self::TestAccountConfig { .. }
            | Self::DisableAccountConfig { .. }
            | Self::RemoveAccountConfig { .. }
            | Self::RepairAccountConfig { .. }
            | Self::ListRules
            | Self::GetRule { .. }
            | Self::GetRuleForm { .. }
            | Self::UpsertRule { .. }
            | Self::UpsertRuleForm { .. }
            | Self::DeleteRule { .. }
            | Self::DryRunRules { .. }
            | Self::ListSavedSearches
            | Self::ListSavedSearchUnreadCounts
            | Self::ListSubscriptions { .. }
            | Self::ListStorageBreakdown { .. }
            | Self::ListLargestMessages { .. }
            | Self::Wrapped { .. }
            | Self::ListStaleThreads { .. }
            | Self::ListContactAsymmetry { .. }
            | Self::ListContactDecay { .. }
            | Self::RefreshContacts
            | Self::RebuildAnalytics
            | Self::RecomputeLinkCounts
            | Self::ListResponseTime { .. }
            | Self::ListAccountAddresses { .. }
            | Self::AddAccountAddress { .. }
            | Self::RemoveAccountAddress { .. }
            | Self::SetPrimaryAccountAddress { .. }
            | Self::GetLlmStatus
            | Self::GetLlmConfig
            | Self::UpdateLlmConfig { .. }
            | Self::GetSemanticStatus
            | Self::EnableSemantic { .. }
            | Self::InstallSemanticProfile { .. }
            | Self::UseSemanticProfile { .. }
            | Self::ReindexSemantic
            | Self::BackfillSemantic
            | Self::CreateSavedSearch { .. }
            | Self::DeleteSavedSearch { .. }
            | Self::UpdateSavedSearch { .. }
            | Self::RunSavedSearch { .. } => IpcCategory::MxrPlatform,
            Self::ListEvents { .. }
            | Self::GetLogs { .. }
            | Self::GetDoctorReport
            | Self::GenerateBugReport { .. }
            | Self::GetStatus
            | Self::Ping
            | Self::Shutdown
            | Self::ListActivity { .. }
            | Self::CountActivity { .. }
            | Self::ActivityStats { .. }
            | Self::ExportActivity { .. }
            | Self::RedactActivity { .. }
            | Self::PruneActivity { .. }
            | Self::PauseActivity { .. }
            | Self::ResumeActivity
            | Self::ListSavedActivityFilters
            | Self::GetSavedActivityFilter { .. }
            | Self::UpsertSavedActivityFilter { .. }
            | Self::DeleteSavedActivityFilter { .. }
            | Self::ListEventCategories
            | Self::CountEvents { .. } => IpcCategory::AdminMaintenance,
        }
    }
}

/// Mutation commands for modifying messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ReplyContext {
    pub account_id: AccountId,
    pub in_reply_to: String,
    pub references: Vec<String>,
    pub reply_to: String,
    pub cc: String,
    pub subject: String,
    pub from: String,
    pub thread_context: String,
    /// Provider-native thread hint (e.g. Gmail thread id). None for IMAP.
    #[serde(default)]
    pub thread_id: Option<String>,
}

/// Forward context returned by PrepareForward.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ForwardContext {
    pub account_id: AccountId,
    pub subject: String,
    pub from: String,
    pub forwarded_content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AccountMutationResultData {
    pub account_id: AccountId,
    pub account_name: String,
    pub succeeded: u32,
    pub skipped: u32,
    pub failed: u32,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct MutationResultData {
    pub requested: u32,
    pub succeeded: u32,
    pub skipped: u32,
    pub failed: u32,
    pub accounts: Vec<AccountMutationResultData>,
    /// Set by undoable mutations (Archive / Trash / Spam / SetRead /
    /// ReadAndArchive). Identifies a row in the daemon's
    /// `mutation_undo_log` that the client can reference via
    /// `Request::UndoMutation` for ~60s after the mutation lands.
    /// `None` for non-undoable mutations (Star, ModifyLabels, Move).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mutation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "status")]
#[expect(
    clippy::large_enum_variant,
    reason = "IPC response shape is serialized directly; boxing would not change the wire shape but would add client-side allocation churn"
)]
pub enum Response {
    Ok {
        data: ResponseData,
    },
    Error {
        message: String,
        #[serde(default)]
        kind: IpcErrorKind,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        code: String,
        #[serde(default)]
        retryable: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        details: Option<serde_json::Value>,
    },
}

impl Response {
    pub fn error(message: impl Into<String>) -> Self {
        let message = message.into();
        Self::Error {
            kind: classify_error_kind(&message),
            code: classify_error_code(&message).to_string(),
            retryable: error_looks_retryable(&message),
            details: None,
            message,
        }
    }

    /// Construct an `Error` response with an explicitly-known `IpcErrorKind`,
    /// bypassing the substring-based classifier. Prefer this when the handler
    /// already knows the error category — e.g. when mapping a typed `MxrError`.
    pub fn error_kinded(message: impl Into<String>, kind: IpcErrorKind) -> Self {
        Self::Error {
            kind,
            code: kind.as_code().to_string(),
            retryable: kind.is_retryable(),
            details: None,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum IpcErrorKind {
    InvalidRequest,
    NotFound,
    Auth,
    Policy,
    Provider,
    RateLimited,
    Store,
    Unsupported,
    #[default]
    Internal,
}

impl IpcErrorKind {
    pub fn as_code(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::NotFound => "not_found",
            Self::Auth => "auth",
            Self::Policy => "policy",
            Self::Provider => "provider",
            Self::RateLimited => "rate_limited",
            Self::Store => "store",
            Self::Unsupported => "unsupported",
            Self::Internal => "internal",
        }
    }

    pub fn is_retryable(self) -> bool {
        matches!(self, Self::Provider | Self::RateLimited)
    }
}

fn classify_error_kind(message: &str) -> IpcErrorKind {
    let lower = message.to_ascii_lowercase();
    if lower.contains("not found") {
        IpcErrorKind::NotFound
    } else if lower.contains("rate limit") {
        IpcErrorKind::RateLimited
    } else if lower.contains("unauthorized") || lower.contains("auth") || lower.contains("oauth") {
        IpcErrorKind::Auth
    } else if lower.contains("safety policy") {
        IpcErrorKind::Policy
    } else if lower.contains("unsupported") || lower.contains("not supported") {
        IpcErrorKind::Unsupported
    } else if lower.contains("provider") || lower.contains("imap") || lower.contains("gmail") {
        IpcErrorKind::Provider
    } else if lower.contains("sqlite") || lower.contains("database") || lower.contains("store") {
        IpcErrorKind::Store
    } else if lower.contains("invalid") || lower.contains("expected") {
        IpcErrorKind::InvalidRequest
    } else {
        IpcErrorKind::Internal
    }
}

fn classify_error_code(message: &str) -> &'static str {
    classify_error_kind(message).as_code()
}

fn error_looks_retryable(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("timeout")
        || lower.contains("temporar")
        || lower.contains("rate limit")
        || lower.contains("unavailable")
        || lower.contains("connection")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "kind")]
#[expect(
    clippy::large_enum_variant,
    reason = "ResponseData is the tagged IPC contract; large variants are preserved inline to keep serde JSON compatibility obvious"
)]
pub enum ResponseData {
    // Core mail/runtime responses.
    Envelopes {
        envelopes: Vec<Envelope>,
    },
    Envelope {
        envelope: Envelope,
    },
    Body {
        body: MessageBody,
    },
    Invite {
        invite: CalendarInviteData,
    },
    Invites {
        invites: Vec<CalendarInviteData>,
    },
    CalendarInviteBackfill {
        backfilled: u64,
    },
    InviteResponsePreview {
        preview: CalendarInviteResponsePreview,
    },
    InviteResponseSent {
        result: CalendarInviteResponseResult,
    },
    HtmlImageAssets {
        message_id: MessageId,
        assets: Vec<HtmlImageAsset>,
    },
    AttachmentFile {
        file: AttachmentFile,
    },
    Bodies {
        bodies: Vec<MessageBody>,
        #[serde(default)]
        failures: Vec<BodyFailure>,
    },
    Thread {
        thread: Thread,
        messages: Vec<Envelope>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        summary: Option<ThreadSummaryData>,
    },
    Threads {
        threads: Vec<Thread>,
    },
    Labels {
        labels: Vec<Label>,
    },
    Label {
        label: Label,
    },

    SearchResults {
        results: Vec<SearchResultItem>,
        #[serde(default)]
        total: u32,
        #[serde(default)]
        has_more: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        next_offset: Option<u32>,
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
    /// List of messages currently flagged for reply-later.
    ReplyQueue {
        messages: Vec<Envelope>,
    },
    Snippets {
        snippets: Vec<SnippetData>,
    },
    SnippetData {
        snippet: SnippetData,
    },
    Deliveries {
        deliveries: Vec<DeliveryData>,
    },
    Delivery {
        delivery: DeliveryData,
    },
    DeliveryScan {
        summary: DeliveryScanSummary,
    },
    Signatures {
        signatures: Vec<SignatureData>,
    },
    SignatureData {
        signature: SignatureData,
    },
    SignatureDefaults {
        defaults: Vec<SignatureDefaultData>,
    },
    ResolvedSignature {
        signature: Option<SignatureData>,
    },
    SenderProfile {
        profile: Option<SenderProfileData>,
    },
    Senders {
        senders: Vec<SenderSummaryData>,
    },
    /// Per-saved-search unread counts. Map shape so callers can do
    /// fast id-keyed lookups when rendering the tab strip; missing
    /// entries render as a bare label (no `(0)` clutter).
    SavedSearchUnreadCounts {
        counts: std::collections::HashMap<SavedSearchId, u32>,
    },
    RelationshipProfile {
        profile: Option<RelationshipProfileData>,
    },
    CommitmentList {
        commitments: Vec<CommitmentData>,
    },
    UserVoice {
        profile: Option<UserVoiceProfileData>,
    },
    HumanizerReport {
        report: HumanizerReportSummaryData,
    },
    HumanizedText {
        text: String,
        report: HumanizerReportSummaryData,
        iterations: u8,
    },
    ScreenerQueue {
        entries: Vec<ScreenerQueueEntryData>,
    },
    ScreenerDecisions {
        decisions: Vec<ScreenerDecisionData>,
    },
    ThreadSummary {
        text: String,
        model: String,
    },
    DraftSuggestion {
        body: String,
        model: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        voice_match: Option<VoiceMatchData>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        humanizer: Option<HumanizerReportSummaryData>,
        #[serde(default)]
        rewrite_iterations: u8,
    },
    ExportResult {
        content: String,
    },
    MutationResult {
        result: MutationResultData,
    },

    // mxr app/platform responses.
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
    AuthSession {
        session: AuthSessionData,
    },
    RuleFormData {
        form: RuleFormData,
    },
    RuleDryRun {
        results: Vec<serde_json::Value>,
    },
    SavedSearches {
        searches: Vec<mxr_core::types::SavedSearch>,
    },
    Subscriptions {
        subscriptions: Vec<mxr_core::types::SubscriptionSummary>,
    },
    StorageBreakdown {
        rows: Vec<StorageBucket>,
    },
    LargestMessages {
        rows: Vec<LargestMessageRow>,
    },
    Wrapped {
        summary: WrappedSummary,
    },
    StaleThreads {
        rows: Vec<StaleThreadRow>,
    },
    ContactAsymmetry {
        rows: Vec<ContactAsymmetryRow>,
    },
    ContactDecay {
        rows: Vec<ContactDecayRow>,
    },
    RefreshedContacts {
        rows: u32,
    },
    AnalyticsRebuildSummary {
        directions_reclassified: u32,
        list_ids_backfilled: u32,
        reply_pairs_resolved: u32,
        business_hours_backfilled: u32,
        contacts_rows: u32,
    },
    ResponseTime {
        summary: ResponseTimeSummary,
    },
    AccountAddresses {
        addresses: Vec<mxr_core::types::AccountAddress>,
    },
    SemanticStatus {
        snapshot: SemanticStatusSnapshot,
    },
    LlmStatus {
        snapshot: LlmStatusSnapshot,
    },
    LlmConfig {
        config: LlmConfigData,
    },
    SavedSearchData {
        search: mxr_core::types::SavedSearch,
    },

    // Admin / maintenance / operational responses.
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
        #[serde(default)]
        semantic_runtime: Option<SemanticRuntimeMetrics>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        feature_health: Option<FeatureHealthReport>,
    },
    Pong,
    Ack,
    /// Returned by `Request::SendDraft` and `Request::SendStoredDraft` on
    /// success. Carries the IDs minted during synthetic Sent ingestion so
    /// callers can navigate to or reference the just-sent message without
    /// waiting for the next sync.
    SendReceipt {
        local_message_id: MessageId,
        provider_message_id: Option<String>,
        rfc2822_message_id: String,
    },
    /// Returned by `Request::ExtractDraftCommitments`.
    DraftCommitments {
        candidates: Vec<DraftCommitmentCandidateData>,
    },
    /// Returned by `Request::ListOwedReplies`.
    OwedReplies {
        rows: Vec<OwedReplyRowData>,
    },
    /// Returned by `Request::ArchiveAsk`.
    ArchiveAnswer {
        answer: ArchiveAnswerData,
    },
    /// Returned by `Request::ListDecisionLog`.
    DecisionLog {
        decisions: Vec<DecisionLogEntryData>,
    },
    /// Returned by `Request::GetDecision`. `None` when the id is
    /// unknown — CLI surfaces that as "not found", not as an error.
    DecisionDetail {
        decision: Option<DecisionLogEntryData>,
    },
    /// Returned by `Request::RebuildDecisionLog`.
    DecisionLogRebuildSummary {
        extracted: u32,
        skipped: u32,
        errors: u32,
    },
    /// Returned by `Request::SendTimeRecommendation`.
    SendTimeRecommendationResponse {
        recommendation: SendTimeRecommendationData,
    },
    /// Returned by `Request::ListCadenceWatch`.
    CadenceWatchList {
        entries: Vec<RelationshipWatchEntryData>,
    },
    /// Returned by `Request::ListCadenceDrift`.
    CadenceDriftList {
        rows: Vec<CadenceDriftRowData>,
    },
    /// Returned by `Request::GetThreadBriefing` or
    /// `Request::GetRecipientBriefing`.
    ThreadBriefing {
        briefing: ThreadBriefingData,
    },
    RecipientBriefing {
        briefing: ThreadBriefingData,
    },
    /// Returned by `Request::SuggestCollaborators`.
    SuggestedCollaborators {
        suggestions: Vec<SuggestedRecipientData>,
    },
    /// Returned by `Request::FindExpert`.
    ExpertSuggestions {
        experts: Vec<ExpertSuggestionData>,
    },
    /// Returned by `Request::ExplainEntity`.
    EntityExplanation {
        entity: EntityExplanationData,
    },
    /// Returned by `Request::CheckDraftSafety` and surfaced to CLI / TUI.
    DraftSafetyReportResponse {
        report: DraftSafetyReport,
    },

    // ------ activity log ------
    /// Paginated activity rows. See `Request::ListActivity`.
    ActivityEntries {
        entries: Vec<ActivityEntry>,
        next_cursor: Option<ActivityCursor>,
    },
    /// Returned by `Request::CountActivity`. (Distinct from the
    /// `Count { count: u32 }` variant returned by the mail `Count`
    /// request — different scale, different consumer.)
    ActivityCount {
        count: i64,
    },
    /// Returned by `Request::ActivityStats`. Buckets are pre-sorted.
    ActivityStatBuckets {
        buckets: Vec<ActivityStatBucket>,
    },
    /// Returned by `Request::ExportActivity`. Either `body` or `path` is set
    /// depending on whether the export was inline or written to disk.
    ActivityExportResult {
        format: ActivityExportFormat,
        count: i64,
        size_bytes: u64,
        body: Option<String>,
        path: Option<String>,
    },
    /// Returned by mutating activity verbs. `count` is the row count
    /// affected (or that *would* be affected when `dry_run=true`).
    ActivityAffected {
        count: i64,
        dry_run: bool,
    },
    /// Generic acknowledgement for verbs that have no return payload
    /// (`PauseActivity`, `ResumeActivity`).
    Acknowledged,

    // ----- Phase 8 — saved activity filters -----
    SavedActivityFilters {
        entries: Vec<SavedActivityFilterEntry>,
    },
    SavedActivityFilterDetail {
        entry: Option<SavedActivityFilterEntry>,
    },
    /// Distinct event-log categories, recency-ordered.
    EventCategories {
        categories: Vec<String>,
    },
    /// Count for an event-log filter.
    EventLogCount {
        count: i64,
    },
}

impl ResponseData {
    pub const fn category(&self) -> IpcCategory {
        match self {
            Self::Envelopes { .. }
            | Self::Envelope { .. }
            | Self::Body { .. }
            | Self::Invite { .. }
            | Self::Invites { .. }
            | Self::CalendarInviteBackfill { .. }
            | Self::InviteResponsePreview { .. }
            | Self::InviteResponseSent { .. }
            | Self::HtmlImageAssets { .. }
            | Self::AttachmentFile { .. }
            | Self::Bodies { .. }
            | Self::Thread { .. }
            | Self::Threads { .. }
            | Self::Labels { .. }
            | Self::Label { .. }
            | Self::SearchResults { .. }
            | Self::SyncStatus { .. }
            | Self::Count { .. }
            | Self::Headers { .. }
            | Self::ReplyContext { .. }
            | Self::ForwardContext { .. }
            | Self::Drafts { .. }
            | Self::SnoozedMessages { .. }
            | Self::ReplyQueue { .. }
            | Self::Snippets { .. }
            | Self::SnippetData { .. }
            | Self::Deliveries { .. }
            | Self::Delivery { .. }
            | Self::DeliveryScan { .. }
            | Self::Signatures { .. }
            | Self::SignatureData { .. }
            | Self::SignatureDefaults { .. }
            | Self::ResolvedSignature { .. }
            | Self::SenderProfile { .. }
            | Self::Senders { .. }
            | Self::SavedSearchUnreadCounts { .. }
            | Self::RelationshipProfile { .. }
            | Self::CommitmentList { .. }
            | Self::UserVoice { .. }
            | Self::HumanizerReport { .. }
            | Self::HumanizedText { .. }
            | Self::ScreenerQueue { .. }
            | Self::ScreenerDecisions { .. }
            | Self::ThreadSummary { .. }
            | Self::DraftSuggestion { .. }
            | Self::ExportResult { .. }
            | Self::MutationResult { .. }
            | Self::SendReceipt { .. }
            | Self::DraftSafetyReportResponse { .. }
            | Self::DraftCommitments { .. }
            | Self::OwedReplies { .. }
            | Self::ArchiveAnswer { .. }
            | Self::DecisionLog { .. }
            | Self::DecisionDetail { .. }
            | Self::SendTimeRecommendationResponse { .. }
            | Self::DecisionLogRebuildSummary { .. }
            | Self::CadenceWatchList { .. }
            | Self::CadenceDriftList { .. }
            | Self::ThreadBriefing { .. }
            | Self::RecipientBriefing { .. }
            | Self::SuggestedCollaborators { .. }
            | Self::ExpertSuggestions { .. }
            | Self::EntityExplanation { .. } => IpcCategory::CoreMail,
            Self::Rules { .. }
            | Self::RuleData { .. }
            | Self::Accounts { .. }
            | Self::AccountsConfig { .. }
            | Self::AccountOperation { .. }
            | Self::AuthSession { .. }
            | Self::RuleFormData { .. }
            | Self::RuleDryRun { .. }
            | Self::SavedSearches { .. }
            | Self::Subscriptions { .. }
            | Self::StorageBreakdown { .. }
            | Self::LargestMessages { .. }
            | Self::Wrapped { .. }
            | Self::StaleThreads { .. }
            | Self::ContactAsymmetry { .. }
            | Self::ContactDecay { .. }
            | Self::RefreshedContacts { .. }
            | Self::AnalyticsRebuildSummary { .. }
            | Self::ResponseTime { .. }
            | Self::AccountAddresses { .. }
            | Self::LlmStatus { .. }
            | Self::LlmConfig { .. }
            | Self::SemanticStatus { .. }
            | Self::SavedSearchData { .. } => IpcCategory::MxrPlatform,
            Self::EventLogEntries { .. }
            | Self::LogLines { .. }
            | Self::DoctorReport { .. }
            | Self::BugReport { .. }
            | Self::RuleHistory { .. }
            | Self::Status { .. }
            | Self::Pong
            | Self::Ack
            | Self::ActivityEntries { .. }
            | Self::ActivityCount { .. }
            | Self::ActivityStatBuckets { .. }
            | Self::ActivityExportResult { .. }
            | Self::ActivityAffected { .. }
            | Self::Acknowledged
            | Self::SavedActivityFilters { .. }
            | Self::SavedActivityFilterDetail { .. }
            | Self::EventCategories { .. }
            | Self::EventLogCount { .. } => IpcCategory::AdminMaintenance,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SearchResultItem {
    pub message_id: MessageId,
    pub account_id: AccountId,
    pub thread_id: ThreadId,
    pub score: f32,
    pub mode: SearchMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct BodyFailure {
    pub message_id: MessageId,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CalendarInviteData {
    pub id: CalendarInviteId,
    pub account_id: AccountId,
    pub message_id: MessageId,
    pub metadata: CalendarMetadata,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum CalendarInviteActionData {
    Accept,
    Tentative,
    Decline,
}

impl CalendarInviteActionData {
    pub const fn partstat(self) -> &'static str {
        match self {
            Self::Accept => "ACCEPTED",
            Self::Tentative => "TENTATIVE",
            Self::Decline => "DECLINED",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Accept => "Accepted",
            Self::Tentative => "Tentative",
            Self::Decline => "Declined",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CalendarInviteResponsePreview {
    pub message_id: MessageId,
    pub action: CalendarInviteActionData,
    pub attendee_email: String,
    pub organizer_email: String,
    pub subject: String,
    pub body_text: String,
    pub ics: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CalendarInviteResponseResult {
    pub message_id: MessageId,
    pub action: CalendarInviteActionData,
    pub provider_message_id: Option<String>,
    pub rfc2822_message_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AttachmentFile {
    pub attachment_id: AttachmentId,
    pub filename: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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

/// Default limit for `ListScreenerQueue`.
const fn default_screener_limit() -> u32 {
    100
}

/// Default limit for `ListSenders`.
const fn default_sender_limit() -> u32 {
    50
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum ScreenerDispositionData {
    Allow,
    Deny,
    Feed,
    PaperTrail,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ScreenerDecisionData {
    pub account_id: AccountId,
    pub sender_email: String,
    pub disposition: ScreenerDispositionData,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_label: Option<String>,
    pub decided_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ScreenerQueueEntryData {
    pub sender_email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub message_count: u32,
    pub latest_subject: String,
    pub latest_at: chrono::DateTime<chrono::Utc>,
}

/// Severity of a doctor finding. Drives whether the CLI exits non-zero
/// (`Error`) and how the TUI styles the entry.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum DoctorFindingSeverity {
    #[default]
    Info,
    Warning,
    Error,
}

/// Coarse category of a doctor finding. Lets clients group related
/// issues without parsing free text.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum DoctorFindingCategory {
    #[default]
    Generic,
    Sync,
    OAuth,
    Network,
    SearchIndex,
    Semantic,
    SqliteLock,
    Storage,
    Daemon,
}

/// One actionable issue identified by `mxr doctor`. Combines a short
/// human-readable message with optional shell commands the user can
/// run to remediate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DoctorFinding {
    pub category: DoctorFindingCategory,
    pub severity: DoctorFindingSeverity,
    pub message: String,
    /// Shell-runnable suggestions the user can copy-paste. Empty when
    /// no automated remediation is available.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remediation: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SnippetData {
    pub name: String,
    pub body: String,
    #[serde(default)]
    pub vars: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// One ordered/shipped item within a delivery.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DeliveryItemData {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantity: Option<i64>,
}

/// A tracked delivery for the Deliveries surface (CLI/web/TUI).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DeliveryData {
    pub id: DeliveryId,
    pub account_id: AccountId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merchant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub carrier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracking_number: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracking_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_number: Option<String>,
    /// Normalized lifecycle status, e.g. "in_transit", "delivered".
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eta_from: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eta_until: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivered_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    pub items: Vec<DeliveryItemData>,
    pub confidence: f64,
    /// Detection source: "schema" | "llm" | "heuristic".
    pub source: String,
    /// Latest contributing thread, for "open in mailbox".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<ThreadId>,
    pub last_event_at: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dismissed_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Source message ids (provenance). Populated by GetDelivery.
    #[serde(default)]
    pub message_ids: Vec<MessageId>,
}

/// Result of a `ScanDeliveries` run.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DeliveryScanSummary {
    /// Messages examined in the window.
    pub scanned: u32,
    /// Candidates the heuristic created (or would create on a dry run).
    pub created: u32,
    /// Candidates merged into an existing delivery.
    pub updated: u32,
    /// Candidates handed to the LLM for confirmation.
    pub shortlisted: u32,
    /// Whether this was a preview (no writes).
    pub dry_run: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "kebab-case")]
pub enum SignatureContextData {
    New,
    Reply,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SignatureData {
    pub id: SignatureId,
    pub name: String,
    pub body: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SignatureDefaultData {
    pub kind: SignatureContextData,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<AccountId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_email: Option<String>,
    pub signature: SignatureData,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feature_health: Option<FeatureHealthReport>,
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
    /// Structured findings: per-issue category, severity, and
    /// shell-runnable remediation steps. Replaces the freeform
    /// `recommended_next_steps` for clients that want to reason about
    /// individual problems (TUI, future agent integrations). The
    /// freeform field is preserved for backwards-compatibility.
    #[serde(default)]
    pub findings: Vec<DoctorFinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
    #[serde(default)]
    pub messages_missing_semantic_chunks: u32,
    #[serde(default)]
    pub semantic_chunks_missing_embeddings: u32,
    #[serde(default)]
    pub relationship_drifts: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RuleFormData {
    pub id: Option<String>,
    pub name: String,
    pub condition: String,
    pub action: String,
    pub priority: i32,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AccountConfigData {
    pub key: String,
    pub name: String,
    pub email: String,
    #[serde(default = "default_account_enabled")]
    pub enabled: bool,
    pub sync: Option<AccountSyncConfigData>,
    pub send: Option<AccountSendConfigData>,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AuthSessionId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AuthFlowData {
    #[default]
    Auto,
    Installed,
    Device,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AuthSessionStateData {
    Starting,
    WaitingForUser,
    Authorized,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AuthSessionData {
    pub session_id: AuthSessionId,
    pub state: AuthSessionStateData,
    pub flow: AuthFlowData,
    pub account_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification_uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at_unix: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll_interval_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn default_account_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AccountSourceData {
    Runtime,
    Config,
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AccountEditModeData {
    Full,
    RuntimeOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
    #[serde(default)]
    pub capabilities: AccountCapabilitiesData,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AccountCapabilitiesData {
    pub labels: bool,
    pub server_search: bool,
    pub delta_sync: bool,
    pub push: bool,
    pub batch_operations: bool,
    pub native_thread_ids: bool,
    pub supports_send: bool,
    pub supports_local_drafts: bool,
    pub supports_server_drafts: bool,
}

impl Default for AccountCapabilitiesData {
    fn default() -> Self {
        Self {
            labels: false,
            server_search: false,
            delta_sync: false,
            push: false,
            batch_operations: false,
            native_thread_ids: false,
            supports_send: false,
            supports_local_drafts: true,
            supports_server_drafts: false,
        }
    }
}

impl From<SyncCapabilities> for AccountCapabilitiesData {
    fn from(capabilities: SyncCapabilities) -> Self {
        Self {
            labels: capabilities.mutate.labels,
            server_search: capabilities.search.server_side,
            delta_sync: capabilities.sync.delta,
            push: capabilities.push.streaming,
            batch_operations: capabilities.mutate.batch_operations,
            native_thread_ids: capabilities.sync.native_threading,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum GmailCredentialSourceData {
    #[default]
    Bundled,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
        #[serde(default = "default_auth_required")]
        auth_required: bool,
        use_tls: bool,
    },
    OutlookPersonal {
        client_id: Option<String>,
        token_ref: String,
    },
    OutlookWork {
        client_id: Option<String>,
        token_ref: String,
    },
    /// In-memory provider used for CLI smoke tests. Not for production use.
    Fake,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AccountOperationStep {
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AccountOperationResult {
    pub ok: bool,
    pub summary: String,
    pub save: Option<AccountOperationStep>,
    pub auth: Option<AccountOperationStep>,
    pub sync: Option<AccountOperationStep>,
    pub send: Option<AccountOperationStep>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_code_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_code_user_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AccountSendConfigData {
    Gmail,
    OutlookPersonal {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        client_id: Option<String>,
        token_ref: String,
    },
    OutlookWork {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        client_id: Option<String>,
        token_ref: String,
    },
    Smtp {
        host: String,
        port: u16,
        username: String,
        password_ref: String,
        password: Option<String>,
        #[serde(default = "default_auth_required")]
        auth_required: bool,
        use_tls: bool,
    },
    /// In-memory send provider for tests. Not for production use.
    Fake,
}

fn default_auth_required() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "event")]
pub enum DaemonEvent {
    // Core mail/runtime events.
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
    /// A pending auto-reminder fired because its window elapsed without
    /// a reply being detected. Carries the original outbound message
    /// the user wanted to be nudged about.
    ReminderTriggered {
        sent_message_id: MessageId,
    },
    LabelCountsUpdated {
        counts: Vec<LabelCount>,
    },
    OperationStarted {
        operation_id: String,
        operation: String,
        account_id: Option<AccountId>,
        message: String,
    },
    OperationProgress {
        operation_id: String,
        operation: String,
        account_id: Option<AccountId>,
        current: u32,
        total: Option<u32>,
        message: String,
    },
    OperationCompleted {
        operation_id: String,
        operation: String,
        account_id: Option<AccountId>,
        message: String,
    },
    OperationFailed {
        operation_id: String,
        operation: String,
        account_id: Option<AccountId>,
        error: String,
        retryable: bool,
    },
    OperationCancelled {
        operation_id: String,
        operation: String,
        account_id: Option<AccountId>,
        message: String,
    },
    /// Optimistic UI rollback hint: provider/store rejected some or all of a
    /// tracked mutation (see `Request::Mutation.client_correlation_id`).
    MutationReconciliationFailed {
        client_correlation_id: String,
        error_summary: String,
    },
}

impl DaemonEvent {
    pub const fn category(&self) -> IpcCategory {
        match self {
            Self::SyncCompleted { .. }
            | Self::SyncError { .. }
            | Self::NewMessages { .. }
            | Self::MessageUnsnoozed { .. }
            | Self::ReminderTriggered { .. }
            | Self::LabelCountsUpdated { .. } => IpcCategory::CoreMail,
            Self::OperationStarted { .. }
            | Self::OperationProgress { .. }
            | Self::OperationCompleted { .. }
            | Self::OperationFailed { .. }
            | Self::OperationCancelled { .. } => IpcCategory::AdminMaintenance,
            Self::MutationReconciliationFailed { .. } => IpcCategory::CoreMail,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct LabelCount {
    pub label_id: LabelId,
    pub unread_count: u32,
    pub total_count: u32,
}
