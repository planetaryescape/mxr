use mxr_core::id::*;
use mxr_core::types::*;
use serde::{Deserialize, Serialize};

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
}

fn default_allow_llm() -> bool {
    true
}

fn default_owed_reply_limit() -> u32 {
    50
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct IpcMessage {
    pub id: u64,
    pub payload: IpcPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "type")]
#[allow(clippy::large_enum_variant)]
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
    GetHtmlImageAssets {
        message_id: MessageId,
        allow_remote: bool,
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
        config: LlmConfigData,
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
    /// + LLM, gated by `LlmFeature::Commitments`). Persists into
    /// `draft_commitment_candidates`; on successful send, candidates
    /// promote to `contact_commitments`.
    ExtractDraftCommitments {
        draft: Draft,
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
            | Self::GetHtmlImageAssets { .. }
            | Self::DownloadAttachment { .. }
            | Self::OpenAttachment { .. }
            | Self::ListBodies { .. }
            | Self::GetThread { .. }
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
            | Self::ListSubscriptions { .. }
            | Self::ListStorageBreakdown { .. }
            | Self::ListLargestMessages { .. }
            | Self::Wrapped { .. }
            | Self::ListStaleThreads { .. }
            | Self::ListContactAsymmetry { .. }
            | Self::ListContactDecay { .. }
            | Self::RefreshContacts
            | Self::RebuildAnalytics
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
            | Self::RunSavedSearch { .. } => IpcCategory::MxrPlatform,
            Self::ListEvents { .. }
            | Self::GetLogs { .. }
            | Self::GetDoctorReport
            | Self::GenerateBugReport { .. }
            | Self::GetStatus
            | Self::Ping
            | Self::Shutdown => IpcCategory::AdminMaintenance,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[allow(clippy::large_enum_variant)]
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
            IpcErrorKind::InvalidRequest => "invalid_request",
            IpcErrorKind::NotFound => "not_found",
            IpcErrorKind::Auth => "auth",
            IpcErrorKind::Policy => "policy",
            IpcErrorKind::Provider => "provider",
            IpcErrorKind::RateLimited => "rate_limited",
            IpcErrorKind::Store => "store",
            IpcErrorKind::Unsupported => "unsupported",
            IpcErrorKind::Internal => "internal",
        }
    }

    pub fn is_retryable(self) -> bool {
        matches!(self, IpcErrorKind::Provider | IpcErrorKind::RateLimited)
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
#[allow(clippy::large_enum_variant)]
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
    /// Returned by `Request::CheckDraftSafety` and surfaced to CLI / TUI.
    DraftSafetyReportResponse {
        report: DraftSafetyReport,
    },
}

impl ResponseData {
    pub const fn category(&self) -> IpcCategory {
        match self {
            Self::Envelopes { .. }
            | Self::Envelope { .. }
            | Self::Body { .. }
            | Self::HtmlImageAssets { .. }
            | Self::AttachmentFile { .. }
            | Self::Bodies { .. }
            | Self::Thread { .. }
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
            | Self::Signatures { .. }
            | Self::SignatureData { .. }
            | Self::SignatureDefaults { .. }
            | Self::ResolvedSignature { .. }
            | Self::SenderProfile { .. }
            | Self::Senders { .. }
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
            | Self::OwedReplies { .. } => IpcCategory::CoreMail,
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
            | Self::Ack => IpcCategory::AdminMaintenance,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ScreenerDecisionData {
    pub account_id: AccountId,
    pub sender_email: String,
    pub disposition: ScreenerDispositionData,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route_label: Option<String>,
    pub decided_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SenderEmailReferenceData {
    pub message_id: MessageId,
    pub thread_id: ThreadId,
    pub subject: String,
    pub snippet: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_name: Option<String>,
    pub from_email: String,
    pub date: chrono::DateTime<chrono::Utc>,
    pub direction: String,
    pub has_attachments: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SenderSummaryData {
    pub account_id: AccountId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub sender_email: String,
    pub message_count: u32,
    pub unread_count: u32,
    pub latest_subject: String,
    pub latest_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SenderProfileData {
    pub account_id: AccountId,
    pub email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub first_seen_at: chrono::DateTime<chrono::Utc>,
    pub last_seen_at: chrono::DateTime<chrono::Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_inbound_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_outbound_at: Option<chrono::DateTime<chrono::Utc>>,
    pub total_inbound: u32,
    pub total_outbound: u32,
    pub replied_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cadence_days_p50: Option<f64>,
    pub is_list_sender: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list_id: Option<String>,
    pub open_thread_count: u32,
    #[serde(default)]
    pub inbound_storage_bytes: u64,
    #[serde(default)]
    pub outbound_storage_bytes: u64,
    #[serde(default)]
    pub attachment_count: u32,
    #[serde(default)]
    pub attachment_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unanswered_question: Option<SenderUnansweredQuestionData>,
    #[serde(default)]
    pub response_histogram: Vec<ResponseTimeBucket>,
    #[serde(default)]
    pub weekly_activity: Vec<SenderWeeklyActivityData>,
    #[serde(default)]
    pub recent_messages: Vec<SenderEmailReferenceData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relationship: Option<RelationshipProfileData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SenderUnansweredQuestionData {
    pub message_id: MessageId,
    pub thread_id: ThreadId,
    pub subject: String,
    pub received_at: chrono::DateTime<chrono::Utc>,
    pub days_waiting: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SenderWeeklyActivityData {
    pub week_start: chrono::DateTime<chrono::Utc>,
    pub inbound_count: u32,
    pub outbound_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RelationshipProfileData {
    pub account_id: AccountId,
    pub email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<ContactStyleData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<ContactRelationshipSummaryData>,
    #[serde(default)]
    pub open_commitments: Vec<CommitmentData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drift: Option<RelationshipDriftData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RelationshipDriftData {
    pub detected_at: chrono::DateTime<chrono::Utc>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ContactStyleData {
    pub formality_score: f64,
    pub formality_score_theirs: f64,
    pub avg_sentence_len: f64,
    pub avg_sentence_len_theirs: f64,
    pub msg_count_used: u32,
    pub msg_count_used_theirs: u32,
    pub computed_at: chrono::DateTime<chrono::Utc>,
    pub source_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ContactRelationshipSummaryData {
    pub text: String,
    pub model: String,
    pub known_topics: Vec<String>,
    pub computed_at: chrono::DateTime<chrono::Utc>,
    pub source_hash: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum CommitmentDirectionData {
    Yours,
    Theirs,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum CommitmentStatusData {
    Open,
    Resolved,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CommitmentData {
    pub id: String,
    pub account_id: AccountId,
    pub email: String,
    pub thread_id: ThreadId,
    pub direction: CommitmentDirectionData,
    pub status: CommitmentStatusData,
    pub who_owes: String,
    pub what: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by_when: Option<chrono::DateTime<chrono::Utc>>,
    pub evidence_msg_id: MessageId,
    pub extracted_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct OwedReplyRowData {
    pub thread_id: ThreadId,
    pub latest_inbound_msg_id: MessageId,
    pub from_email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_name: Option<String>,
    pub subject: String,
    pub latest_inbound_at: chrono::DateTime<chrono::Utc>,
    pub waiting_days: f64,
    pub expected_days: f64,
    pub overdue_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DraftCommitmentCandidateData {
    pub id: String,
    pub draft_id: DraftId,
    pub account_id: AccountId,
    pub email: String,
    pub direction: CommitmentDirectionData,
    pub who_owes: String,
    pub what: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by_when: Option<chrono::DateTime<chrono::Utc>>,
    pub extracted_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum VoiceRegisterData {
    Casual,
    Neutral,
    Formal,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum DraftLengthHintData {
    Short,
    Medium,
    Long,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DraftRefineKnobsData {
    #[serde(default)]
    pub shorter: bool,
    #[serde(default)]
    pub warmer: bool,
    #[serde(default)]
    pub more_formal: bool,
    #[serde(default)]
    pub less_emoji: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub add_context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UserVoiceRegisterModeData {
    pub register: VoiceRegisterData,
    pub formality_score: f64,
    pub avg_sentence_len: f64,
    pub exemplar_message_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UserVoiceProfileData {
    pub account_id: AccountId,
    pub formality_score: f64,
    pub avg_sentence_len: f64,
    pub msg_count_used: u32,
    pub register_modes: Vec<UserVoiceRegisterModeData>,
    pub computed_at: chrono::DateTime<chrono::Utc>,
    pub source_hash: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum VoiceMatchConfidenceData {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct VoiceMatchData {
    pub score: f64,
    pub confidence: VoiceMatchConfidenceData,
    pub notable_deltas: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct HumanizerHitData {
    pub category: String,
    pub matched: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct HumanizerReportSummaryData {
    pub score: u8,
    pub hits: Vec<HumanizerHitData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum FeatureHealth {
    Healthy,
    Degraded { reason: String },
    Disabled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct FeatureHealthReport {
    pub semantic: FeatureHealth,
    pub summarize: FeatureHealth,
    pub relationship_profile: FeatureHealth,
    pub commitments: FeatureHealth,
    pub draft_assist: FeatureHealth,
    pub voice_match: FeatureHealth,
    pub humanizer: FeatureHealth,
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
            labels: capabilities.labels,
            server_search: capabilities.server_search,
            delta_sync: capabilities.delta_sync,
            push: capabilities.push,
            batch_operations: capabilities.batch_operations,
            native_thread_ids: capabilities.native_thread_ids,
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
