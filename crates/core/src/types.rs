use crate::id::*;
use bitflags::bitflags;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// -- System Labels ------------------------------------------------------------

/// Well-known system label identifiers used across providers.
pub mod system_labels {
    pub const INBOX: &str = "INBOX";
    pub const SENT: &str = "SENT";
    pub const TRASH: &str = "TRASH";
    pub const STARRED: &str = "STARRED";
    pub const DRAFT: &str = "DRAFT";
    pub const ARCHIVE: &str = "ARCHIVE";
    pub const SPAM: &str = "SPAM";

    /// Returns true for the primary system labels shown in the sidebar.
    pub fn is_primary(name: &str) -> bool {
        matches!(
            name,
            "INBOX" | "STARRED" | "SENT" | "DRAFT" | "ARCHIVE" | "SPAM" | "TRASH"
        )
    }

    /// Deterministic sort order for system labels in the sidebar.
    pub fn display_order(name: &str) -> usize {
        match name {
            "INBOX" => 0,
            "STARRED" => 1,
            "SENT" => 2,
            "DRAFT" => 3,
            "ARCHIVE" => 4,
            "SPAM" => 5,
            "TRASH" => 6,
            _ => 100,
        }
    }
}

// -- Address ------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Address {
    pub name: Option<String>,
    pub email: String,
}

// -- Account ------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: AccountId,
    pub name: String,
    pub email: String,
    pub sync_backend: Option<BackendRef>,
    pub send_backend: Option<BackendRef>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendRef {
    pub provider_kind: ProviderKind,
    pub config_key: String,
}

/// One owned email address per account. Direction inference compares
/// `messages.from_email` against this set to decide inbound vs outbound.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountAddress {
    pub account_id: AccountId,
    pub email: String,
    pub is_primary: bool,
}

/// Inbound vs outbound classification for a message. Computed at sync time
/// from `from_email` against the account's owned addresses — `MessageFlags::SENT`
/// is provider-unreliable (Gmail label-based, IMAP fuzzy mailbox-name-based).
///
/// Named `MessageDirection` rather than `Direction` to avoid colliding with
/// ratatui's `Direction` in clients that glob-import this module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageDirection {
    Inbound,
    Outbound,
    /// Pre-Slice-8 rows or messages synced before the address table was
    /// populated. `mxr doctor --rebuild-analytics` reclassifies these.
    Unknown,
}

impl MessageDirection {
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::Inbound => "inbound",
            Self::Outbound => "outbound",
            Self::Unknown => "unknown",
        }
    }

    pub fn from_db_str(s: &str) -> Option<Self> {
        Some(match s {
            "inbound" => Self::Inbound,
            "outbound" => Self::Outbound,
            "unknown" => Self::Unknown,
            _ => return None,
        })
    }
}

/// Lookup interface used by sync engine to classify direction. Concrete
/// impl lives in the daemon (cache backed by `account_addresses`); test
/// code uses a stub.
pub trait AccountAddressLookup: Send + Sync {
    /// Returns true when `email` (case-insensitive) belongs to one of the
    /// account_addresses rows known to this lookup.
    fn is_account_address(&self, email: &str) -> bool;

    /// Returns false until `replace` has been called at least once with a
    /// non-empty set. While this returns false, sync writes `Direction::Unknown`
    /// rather than misclassifying every message as inbound.
    fn is_loaded(&self) -> bool;
}

/// Default in-memory implementation. Daemon owns an `Arc<Self>`, calls
/// `replace` after every successful mutation through `account_addresses`,
/// and passes a clone into `SyncEngine`.
#[derive(Default)]
pub struct InMemoryAccountAddressLookup {
    inner: std::sync::RwLock<std::collections::HashSet<String>>,
    loaded: std::sync::atomic::AtomicBool,
}

impl InMemoryAccountAddressLookup {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the entire address set. Lower-cases on insert so lookups are
    /// case-insensitive without per-call allocation on the hot path.
    pub fn replace(&self, addresses: impl IntoIterator<Item = String>) {
        let normalized: std::collections::HashSet<String> = addresses
            .into_iter()
            .map(|s| s.to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        let mut guard = self
            .inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard = normalized;
        self.loaded.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

impl AccountAddressLookup for InMemoryAccountAddressLookup {
    fn is_account_address(&self, email: &str) -> bool {
        if !self.is_loaded() {
            return false;
        }
        let guard = self
            .inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.contains(&email.to_lowercase())
    }

    fn is_loaded(&self) -> bool {
        self.loaded.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderKind {
    Gmail,
    Imap,
    Smtp,
    Fake,
}

// -- Label --------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Label {
    pub id: LabelId,
    pub account_id: AccountId,
    pub name: String,
    pub kind: LabelKind,
    pub color: Option<String>,
    pub provider_id: String,
    pub unread_count: u32,
    pub total_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LabelKind {
    System,
    Folder,
    User,
}

// -- MessageFlags -------------------------------------------------------------

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct MessageFlags: u32 {
        const READ     = 0b0000_0001;
        const STARRED  = 0b0000_0010;
        const DRAFT    = 0b0000_0100;
        const SENT     = 0b0000_1000;
        const TRASH    = 0b0001_0000;
        const SPAM     = 0b0010_0000;
        const ARCHIVED = 0b0100_0000;
        const ANSWERED = 0b1000_0000;
    }
}

// -- EventSource --------------------------------------------------------------

/// Origin attribution for message-state mutations. Threaded through every
/// store mutation so analytics can distinguish user actions from rule-driven,
/// sync-driven, or reconciler-driven changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSource {
    /// User action via CLI, TUI, or web client.
    User,
    /// Mutation applied by the deterministic rule engine.
    RuleEngine,
    /// Mutation applied because remote sync observed a state change.
    Sync,
    /// Background reconciler (reply-pair backfill, contacts refresh, etc.).
    Reconciler,
    /// Doctor or maintenance command.
    Doctor,
    /// External-system trigger (webhooks, future automations).
    External,
}

impl EventSource {
    /// Stable string used in the `message_events.source` column.
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::RuleEngine => "rule_engine",
            Self::Sync => "sync",
            Self::Reconciler => "reconciler",
            Self::Doctor => "doctor",
            Self::External => "external",
        }
    }

    pub fn from_db_str(s: &str) -> Option<Self> {
        Some(match s {
            "user" => Self::User,
            "rule_engine" => Self::RuleEngine,
            "sync" => Self::Sync,
            "reconciler" => Self::Reconciler,
            "doctor" => Self::Doctor,
            "external" => Self::External,
            _ => return None,
        })
    }
}

// -- MessageEventType --------------------------------------------------------

/// Per-message state-transition events. Persisted to `message_events` so
/// analytics can answer time-bounded questions ("how long until I archived
/// it?", "what fraction of inbound messages from sender X get replied to?")
/// that the snapshot in `messages.flags` cannot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageEventType {
    Read,
    Unread,
    Starred,
    Unstarred,
    Archived,
    Unarchived,
    Trashed,
    Untrashed,
    Labeled,
    Unlabeled,
    Moved,
    Received,
    Sent,
    Replied,
    Forwarded,
    Snoozed,
    Unsnoozed,
    Unsubscribed,
}

impl MessageEventType {
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Unread => "unread",
            Self::Starred => "starred",
            Self::Unstarred => "unstarred",
            Self::Archived => "archived",
            Self::Unarchived => "unarchived",
            Self::Trashed => "trashed",
            Self::Untrashed => "untrashed",
            Self::Labeled => "labeled",
            Self::Unlabeled => "unlabeled",
            Self::Moved => "moved",
            Self::Received => "received",
            Self::Sent => "sent",
            Self::Replied => "replied",
            Self::Forwarded => "forwarded",
            Self::Snoozed => "snoozed",
            Self::Unsnoozed => "unsnoozed",
            Self::Unsubscribed => "unsubscribed",
        }
    }

    pub fn from_db_str(s: &str) -> Option<Self> {
        Some(match s {
            "read" => Self::Read,
            "unread" => Self::Unread,
            "starred" => Self::Starred,
            "unstarred" => Self::Unstarred,
            "archived" => Self::Archived,
            "unarchived" => Self::Unarchived,
            "trashed" => Self::Trashed,
            "untrashed" => Self::Untrashed,
            "labeled" => Self::Labeled,
            "unlabeled" => Self::Unlabeled,
            "moved" => Self::Moved,
            "received" => Self::Received,
            "sent" => Self::Sent,
            "replied" => Self::Replied,
            "forwarded" => Self::Forwarded,
            "snoozed" => Self::Snoozed,
            "unsnoozed" => Self::Unsnoozed,
            "unsubscribed" => Self::Unsubscribed,
            _ => return None,
        })
    }
}

// -- ResponseTime ------------------------------------------------------------

/// Aggregate response-time summary for `mxr response-time`. p50/p90 in
/// seconds; business-hours percentiles are `None` until the reconciler has
/// backfilled the relevant rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResponseTimeSummary {
    pub direction: ResponseTimeDirection,
    pub sample_count: u32,
    pub clock_p50_seconds: u32,
    pub clock_p90_seconds: u32,
    pub business_hours_p50_seconds: Option<u32>,
    pub business_hours_p90_seconds: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseTimeDirection {
    /// I replied to inbound (`'i_replied'` rows in reply_pairs).
    IReplied,
    /// They replied to my outbound (`'they_replied'` rows).
    TheyReplied,
}

impl ResponseTimeDirection {
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::IReplied => "i_replied",
            Self::TheyReplied => "they_replied",
        }
    }
}

// -- Contacts ----------------------------------------------------------------

/// Materialized per-account contact row. Source of truth for `mxr contacts
/// asymmetry` and `mxr contacts decay`. Refreshed periodically.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContactRow {
    pub account_id: AccountId,
    pub email: String,
    pub display_name: Option<String>,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub last_inbound_at: Option<DateTime<Utc>>,
    pub last_outbound_at: Option<DateTime<Utc>>,
    pub total_inbound: u32,
    pub total_outbound: u32,
    pub replied_count: u32,
    pub cadence_days_p50: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContactAsymmetryRow {
    pub email: String,
    pub display_name: Option<String>,
    pub total_inbound: u32,
    pub total_outbound: u32,
    /// `|inbound - outbound| / max(inbound, outbound)` in `[0, 1]`. 0 means
    /// perfectly balanced; 1 means I never responded (or vice versa).
    pub asymmetry: f64,
    pub last_seen_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContactDecayRow {
    pub email: String,
    pub display_name: Option<String>,
    pub last_inbound_at: DateTime<Utc>,
    pub last_outbound_at: Option<DateTime<Utc>>,
    pub days_since_inbound: u32,
    pub days_since_outbound: Option<u32>,
}

// -- StaleThreads ------------------------------------------------------------

/// Single row of `mxr stale` output: a thread whose latest message points
/// at one party and has been silent past the threshold.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaleThreadRow {
    pub thread_id: ThreadId,
    pub latest_message_id: MessageId,
    pub latest_subject: String,
    pub counterparty_email: String,
    pub latest_date: DateTime<Utc>,
    pub days_stale: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StaleBallInCourt {
    /// Latest message is inbound; ball is in MY court (I owe a reply).
    Mine,
    /// Latest message is outbound; ball is in THEIR court (they owe a reply).
    Theirs,
}

impl StaleBallInCourt {
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::Mine => "inbound",
            Self::Theirs => "outbound",
        }
    }
}

// -- StorageBreakdown --------------------------------------------------------

/// Single row of `mxr storage` output: how many bytes / how many items
/// rolled up under a particular grouping key (sender, mimetype, label).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageBucket {
    pub key: String,
    pub bytes: u64,
    pub count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageGroupBy {
    /// Group attachments by `mime_type`.
    Mimetype,
    /// Group messages by `from_email`. Includes the whole message size, not
    /// just attachments — that's what actually consumes disk per sender.
    Sender,
    /// Group messages by label name (excludes messages with no labels).
    Label,
}

// -- MessageEvent ------------------------------------------------------------

/// A single per-message state-transition event. Ordered by `occurred_at` ASC
/// when read back; analytics consumers should not assume monotonic IDs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageEvent {
    pub message_id: MessageId,
    pub account_id: AccountId,
    pub event_type: MessageEventType,
    pub source: EventSource,
    /// Set for `labeled` / `unlabeled` events; otherwise `None`.
    pub label_id: Option<LabelId>,
    /// Unix timestamp in seconds (UTC).
    pub occurred_at: i64,
    /// Free-form JSON for event-type-specific context (e.g. moved-from/to
    /// label IDs). Kept opt-in to avoid bloat on the common transitions.
    pub metadata_json: Option<String>,
}

// -- Envelope -----------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub id: MessageId,
    pub account_id: AccountId,
    /// Provider-instance identity used for sync and mutations.
    ///
    /// Stable for Gmail message IDs.
    /// For IMAP, this is mailbox-scoped today and may change across moves/copies.
    pub provider_id: String,
    pub thread_id: ThreadId,
    pub message_id_header: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub from: Address,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub bcc: Vec<Address>,
    pub subject: String,
    pub date: DateTime<Utc>,
    pub flags: MessageFlags,
    pub snippet: String,
    pub has_attachments: bool,
    pub size_bytes: u64,
    pub unsubscribe: UnsubscribeMethod,
    /// Provider-specific label IDs (e.g. "INBOX", "SENT", "Label_123").
    /// Transient: used during sync to populate the message_labels junction table.
    #[serde(default)]
    pub label_provider_ids: Vec<String>,
}

// -- UnsubscribeMethod --------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum UnsubscribeMethod {
    OneClick {
        url: String,
    },
    HttpLink {
        url: String,
    },
    Mailto {
        address: String,
        subject: Option<String>,
    },
    BodyLink {
        url: String,
    },
    None,
}

// -- ReplyHeaders ------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplyHeaders {
    pub in_reply_to: String,
    #[serde(default)]
    pub references: Vec<String>,
    /// Provider-native thread hint. Gmail uses this to keep replies in-thread;
    /// IMAP relies on the In-Reply-To/References headers and ignores it.
    #[serde(default)]
    pub thread_id: Option<String>,
}

// -- MessageMetadata ---------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct MessageMetadata {
    pub list_id: Option<String>,
    #[serde(default)]
    pub auth_results: Vec<String>,
    #[serde(default)]
    pub content_language: Vec<String>,
    pub text_plain_format: Option<TextPlainFormat>,
    pub text_plain_source: Option<BodyPartSource>,
    pub text_html_source: Option<BodyPartSource>,
    pub calendar: Option<CalendarMetadata>,
    pub raw_headers: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BodyPartSource {
    Exact,
    DerivedFromPlain,
    DerivedFromHtml,
    BestEffortSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TextPlainFormat {
    Fixed,
    Flowed { delsp: bool },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CalendarMetadata {
    pub method: Option<String>,
    pub summary: Option<String>,
}

// -- MessageBody --------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageBody {
    pub message_id: MessageId,
    pub text_plain: Option<String>,
    pub text_html: Option<String>,
    pub attachments: Vec<AttachmentMeta>,
    pub fetched_at: DateTime<Utc>,
    #[serde(default)]
    pub metadata: MessageMetadata,
}

impl MessageBody {
    pub fn ensure_best_effort_readable(&mut self) -> bool {
        if self.text_plain.is_some() || self.text_html.is_some() {
            return false;
        }

        let Some(summary) = self.computed_best_effort_readable_summary() else {
            return false;
        };

        self.text_plain = Some(summary);
        self.metadata.text_plain_source = Some(BodyPartSource::BestEffortSummary);
        true
    }

    pub fn best_effort_readable_summary(&self) -> Option<String> {
        if self.text_plain.is_some() || self.text_html.is_some() {
            return None;
        }

        self.computed_best_effort_readable_summary()
    }

    pub fn is_legacy_best_effort_plain_summary(&self) -> bool {
        self.text_html.is_none()
            && self.metadata.text_plain_source.is_none()
            && self.computed_best_effort_readable_summary().as_deref() == self.text_plain.as_deref()
    }

    pub fn mark_best_effort_summary_source(&mut self) -> bool {
        if self.text_html.is_some()
            || self.metadata.text_plain_source.is_some()
            || self.computed_best_effort_readable_summary().as_deref() != self.text_plain.as_deref()
        {
            return false;
        }

        self.metadata.text_plain_source = Some(BodyPartSource::BestEffortSummary);
        true
    }

    fn computed_best_effort_readable_summary(&self) -> Option<String> {
        let mut sections = Vec::new();

        if let Some(calendar) = &self.metadata.calendar {
            sections.push("Calendar invite".to_string());
            if let Some(summary) = calendar
                .summary
                .as_deref()
                .filter(|value| !value.is_empty())
            {
                sections.push(format!("Summary: {summary}"));
            }
            if let Some(method) = calendar.method.as_deref().filter(|value| !value.is_empty()) {
                sections.push(format!("Method: {method}"));
            }
        }

        let has_encrypted = self.attachments.iter().any(AttachmentMeta::looks_encrypted);
        let has_signature = self.attachments.iter().any(AttachmentMeta::looks_signed);

        if has_encrypted {
            sections.push("Encrypted message body. mxr cannot decrypt this message yet.".into());
        } else if has_signature {
            sections.push("Signed message without a readable text body.".into());
        } else if !self.attachments.is_empty() {
            sections.push(
                "Attachment-only message. No text/plain or text/html body was provided.".into(),
            );
        } else if sections.is_empty() {
            sections.push("No readable body content was available for this message.".into());
        }

        if !self.attachments.is_empty() {
            let attachment_lines = self
                .attachments
                .iter()
                .map(AttachmentMeta::summary_line)
                .collect::<Vec<_>>()
                .join("\n");
            sections.push(format!("Attachments:\n{attachment_lines}"));
        }

        Some(sections.join("\n\n"))
    }
}

// -- AttachmentMeta -----------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentMeta {
    pub id: AttachmentId,
    pub message_id: MessageId,
    pub filename: String,
    pub mime_type: String,
    #[serde(default)]
    pub disposition: AttachmentDisposition,
    pub content_id: Option<String>,
    pub content_location: Option<String>,
    pub size_bytes: u64,
    pub local_path: Option<PathBuf>,
    pub provider_id: String,
}

impl AttachmentMeta {
    fn summary_line(&self) -> String {
        let filename = if self.filename.is_empty() {
            "(unnamed attachment)"
        } else {
            self.filename.as_str()
        };
        format!(
            "- {} ({}, {} bytes)",
            filename, self.mime_type, self.size_bytes
        )
    }

    fn looks_encrypted(&self) -> bool {
        let mime = self.mime_type.to_ascii_lowercase();
        let filename = self.filename.to_ascii_lowercase();
        matches!(
            mime.as_str(),
            "application/pkcs7-mime" | "application/x-pkcs7-mime" | "application/pgp-encrypted"
        ) || filename.ends_with(".p7m")
            || filename.ends_with(".pgp")
            || filename.ends_with(".gpg")
    }

    fn looks_signed(&self) -> bool {
        let mime = self.mime_type.to_ascii_lowercase();
        let filename = self.filename.to_ascii_lowercase();
        matches!(
            mime.as_str(),
            "application/pkcs7-signature"
                | "application/x-pkcs7-signature"
                | "application/pgp-signature"
        ) || filename.ends_with(".p7s")
            || filename.ends_with(".asc")
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentDisposition {
    Attachment,
    Inline,
    #[default]
    Unspecified,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HtmlImageAsset {
    pub source: String,
    pub kind: HtmlImageSourceKind,
    pub status: HtmlImageAssetStatus,
    pub mime_type: Option<String>,
    pub path: Option<PathBuf>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HtmlImageSourceKind {
    Cid,
    DataUri,
    Remote,
    ContentLocation,
    File,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HtmlImageAssetStatus {
    Ready,
    Blocked,
    Missing,
    Unsupported,
    Failed,
}

// -- Thread -------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thread {
    pub id: ThreadId,
    pub account_id: AccountId,
    pub subject: String,
    pub participants: Vec<Address>,
    pub message_count: u32,
    pub unread_count: u32,
    pub latest_date: DateTime<Utc>,
    pub snippet: String,
}

// -- Draft --------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Draft {
    pub id: DraftId,
    pub account_id: AccountId,
    pub reply_headers: Option<ReplyHeaders>,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub bcc: Vec<Address>,
    pub subject: String,
    pub body_markdown: String,
    pub attachments: Vec<PathBuf>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DraftStatus {
    #[default]
    Draft,
    Sending,
    Sent,
}

impl DraftStatus {
    pub fn as_db_str(self) -> &'static str {
        match self {
            DraftStatus::Draft => "draft",
            DraftStatus::Sending => "sending",
            DraftStatus::Sent => "sent",
        }
    }

    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "draft" => Some(DraftStatus::Draft),
            "sending" => Some(DraftStatus::Sending),
            "sent" => Some(DraftStatus::Sent),
            _ => None,
        }
    }
}

// -- SavedSearch --------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode {
    #[default]
    Lexical,
    Hybrid,
    Semantic,
}

impl SearchMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lexical => "lexical",
            Self::Hybrid => "hybrid",
            Self::Semantic => "semantic",
        }
    }

    pub fn uses_semantic(self) -> bool {
        matches!(self, Self::Hybrid | Self::Semantic)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SemanticProfile {
    #[default]
    #[serde(rename = "bge-small-en-v1.5")]
    BgeSmallEnV15,
    #[serde(rename = "multilingual-e5-small")]
    MultilingualE5Small,
    #[serde(rename = "bge-m3")]
    BgeM3,
}

impl SemanticProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BgeSmallEnV15 => "bge-small-en-v1.5",
            Self::MultilingualE5Small => "multilingual-e5-small",
            Self::BgeM3 => "bge-m3",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticProfileStatus {
    #[default]
    Pending,
    Ready,
    Indexing,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticChunkSourceKind {
    Header,
    Body,
    AttachmentSummary,
    AttachmentText,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticEmbeddingStatus {
    #[default]
    Pending,
    Ready,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticProfileRecord {
    pub id: SemanticProfileId,
    pub profile: SemanticProfile,
    pub backend: String,
    pub model_revision: String,
    pub dimensions: u32,
    pub status: SemanticProfileStatus,
    pub installed_at: Option<DateTime<Utc>>,
    pub activated_at: Option<DateTime<Utc>>,
    pub last_indexed_at: Option<DateTime<Utc>>,
    pub progress_completed: u32,
    pub progress_total: u32,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticChunkRecord {
    pub id: SemanticChunkId,
    pub message_id: MessageId,
    pub source_kind: SemanticChunkSourceKind,
    pub ordinal: u32,
    pub normalized: String,
    pub content_hash: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticEmbeddingRecord {
    pub chunk_id: SemanticChunkId,
    pub profile_id: SemanticProfileId,
    pub dimensions: u32,
    #[serde(with = "serde_bytes")]
    pub vector: Vec<u8>,
    pub status: SemanticEmbeddingStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticStatusSnapshot {
    pub enabled: bool,
    pub active_profile: SemanticProfile,
    pub profiles: Vec<SemanticProfileRecord>,
    #[serde(default)]
    pub runtime: SemanticRuntimeMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SemanticRuntimeMetrics {
    #[serde(default)]
    pub queue_depth: u32,
    #[serde(default)]
    pub in_flight: u32,
    #[serde(default)]
    pub last_queue_wait_ms: Option<u64>,
    #[serde(default)]
    pub last_extract_ms: Option<u64>,
    #[serde(default)]
    pub last_embedding_prep_ms: Option<u64>,
    #[serde(default)]
    pub last_ingest_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedSearch {
    pub id: SavedSearchId,
    pub account_id: Option<AccountId>,
    pub name: String,
    pub query: String,
    #[serde(default)]
    pub search_mode: SearchMode,
    pub sort: SortOrder,
    pub icon: Option<String>,
    pub position: i32,
    pub created_at: DateTime<Utc>,
}

// -- Subscriptions ------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionSummary {
    pub account_id: AccountId,
    pub sender_name: Option<String>,
    pub sender_email: String,
    pub message_count: u32,
    pub latest_message_id: MessageId,
    pub latest_provider_id: String,
    pub latest_thread_id: ThreadId,
    pub latest_subject: String,
    pub latest_snippet: String,
    pub latest_date: DateTime<Utc>,
    pub latest_flags: MessageFlags,
    pub latest_has_attachments: bool,
    pub latest_size_bytes: u64,
    pub unsubscribe: UnsubscribeMethod,
    /// Number of messages from this sender that have been marked READ.
    /// Combined with `message_count` gives the open-rate used by `unsub --rank`.
    #[serde(default)]
    pub opened_count: u32,
    /// Number of messages where I replied. Placeholder zero until Slice 9
    /// wires `reply_pairs`. Field exists so the JSON contract is stable.
    #[serde(default)]
    pub replied_count: u32,
    /// Messages that landed in ARCHIVE without ever being read. Strong
    /// "this is noise" signal for the unsubscribe ranker.
    #[serde(default)]
    pub archived_unread_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortOrder {
    DateDesc,
    DateAsc,
    Relevance,
}

// -- Snoozed ------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snoozed {
    pub message_id: MessageId,
    pub account_id: AccountId,
    pub snoozed_at: DateTime<Utc>,
    pub wake_at: DateTime<Utc>,
    pub original_labels: Vec<LabelId>,
}

// -- Sync types ---------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImapMailboxCursor {
    pub mailbox: String,
    pub uid_validity: u32,
    pub uid_next: u32,
    #[serde(default)]
    pub highest_modseq: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImapCapabilityState {
    pub move_ext: bool,
    pub uidplus: bool,
    pub idle: bool,
    pub condstore: bool,
    pub qresync: bool,
    pub namespace: bool,
    pub list_status: bool,
    pub utf8_accept: bool,
    pub imap4rev2: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncCursor {
    Gmail {
        history_id: u64,
    },
    GmailBackfill {
        history_id: u64,
        page_token: String,
    },
    Imap {
        uid_validity: u32,
        uid_next: u32,
        #[serde(default)]
        mailboxes: Vec<ImapMailboxCursor>,
        #[serde(default)]
        capabilities: Option<ImapCapabilityState>,
    },
    Initial,
}

// -- SyncedMessage ------------------------------------------------------------

/// A message with both envelope and body, returned by sync.
/// Bodies are always fetched eagerly during sync — no lazy hydration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncedMessage {
    pub envelope: Envelope,
    pub body: MessageBody,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncBatch {
    pub upserted: Vec<SyncedMessage>,
    pub deleted_provider_ids: Vec<String>,
    pub label_changes: Vec<LabelChange>,
    pub next_cursor: SyncCursor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabelChange {
    pub provider_message_id: String,
    pub added_labels: Vec<String>,
    pub removed_labels: Vec<String>,
}

// -- Export -------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportFormat {
    Markdown,
    Json,
    Mbox,
    LlmContext,
}

// -- ProviderMeta -------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderMeta {
    /// Reserved provider-truth escape hatch.
    ///
    /// The type and schema exist, but current sync/store flows do not materially depend on this
    /// record at runtime. Treat it as dormant until a concrete need reactivates it.
    pub message_id: MessageId,
    pub provider: ProviderKind,
    pub remote_id: String,
    pub thread_remote_id: Option<String>,
    pub sync_token: Option<String>,
    pub raw_labels: Option<String>,
    pub mailbox_id: Option<String>,
    pub uid_validity: Option<u32>,
    pub raw_json: Option<String>,
}

// -- SyncCapabilities ---------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncCapabilities {
    /// True only for providers with stable multi-assign label semantics.
    /// False means placement is folder/mailbox-based, even if `sync_labels()` exposes folders
    /// through the shared `Label` type.
    pub labels: bool,
    pub server_search: bool,
    pub delta_sync: bool,
    pub push: bool,
    pub batch_operations: bool,
    pub native_thread_ids: bool,
}

// -- SendReceipt --------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendReceipt {
    pub provider_message_id: Option<String>,
    pub sent_at: DateTime<Utc>,
    /// RFC 5322 Message-ID header rendered into the outgoing message. Used by
    /// the daemon to anchor the synthetic local Sent envelope, and by IMAP
    /// dedupe on subsequent sync.
    #[serde(default)]
    pub rfc2822_message_id: String,
}
