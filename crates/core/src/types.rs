use crate::id::*;
use bitflags::bitflags;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Address {
    pub name: Option<String>,
    pub email: String,
}

// -- Account ------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Account {
    pub id: AccountId,
    pub name: String,
    pub email: String,
    pub sync_backend: Option<BackendRef>,
    pub send_backend: Option<BackendRef>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct BackendRef {
    pub provider_kind: ProviderKind,
    pub config_key: String,
}

/// One owned email address per account. Direction inference compares
/// `messages.from_email` against this set to decide inbound vs outbound.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
    /// Returns true when `email` (case-insensitive) is an owned address of
    /// `account_id`. Scoped per account: an address owned by a *different*
    /// account must not match, otherwise a genuine inbound message whose
    /// sender happens to be another account's own address is misclassified
    /// outbound.
    fn is_account_address(&self, account_id: &AccountId, email: &str) -> bool;

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
    inner:
        std::sync::RwLock<std::collections::HashMap<AccountId, std::collections::HashSet<String>>>,
    loaded: std::sync::atomic::AtomicBool,
}

impl InMemoryAccountAddressLookup {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the entire per-account address map. Trims and lower-cases each
    /// email on insert so lookups are case-insensitive without per-call
    /// allocation on the hot path. Addresses are keyed by `account_id` so a
    /// lookup only ever matches the receiving account's own addresses.
    pub fn replace(&self, addresses: impl IntoIterator<Item = (AccountId, String)>) {
        let mut normalized: std::collections::HashMap<
            AccountId,
            std::collections::HashSet<String>,
        > = std::collections::HashMap::new();
        for (account_id, email) in addresses {
            let email = email.trim().to_lowercase();
            if email.is_empty() {
                continue;
            }
            normalized.entry(account_id).or_default().insert(email);
        }
        let mut guard = self
            .inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard = normalized;
        self.loaded.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

impl AccountAddressLookup for InMemoryAccountAddressLookup {
    fn is_account_address(&self, account_id: &AccountId, email: &str) -> bool {
        if !self.is_loaded() {
            return false;
        }
        let needle = email.trim().to_lowercase();
        let guard = self
            .inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard
            .get(account_id)
            .is_some_and(|set| set.contains(&needle))
    }

    fn is_loaded(&self) -> bool {
        self.loaded.load(std::sync::atomic::Ordering::SeqCst)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum ProviderKind {
    Gmail,
    Imap,
    Smtp,
    OutlookPersonal,
    OutlookWork,
    Fake,
}

// -- Label --------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Label {
    pub id: LabelId,
    pub account_id: AccountId,
    pub name: String,
    pub kind: LabelKind,
    pub color: Option<String>,
    pub provider_id: String,
    pub unread_count: u32,
    pub total_count: u32,
    /// Well-known semantic role this label/folder plays in the provider's
    /// mailbox model (Inbox, Sent, Drafts, …). Derived from IMAP SPECIAL-USE
    /// attributes (RFC 6154) or Gmail system labels at mapping time, and
    /// surfaced to clients so they can render the right icon/affordance
    /// without re-parsing names. `None` for user-defined labels/folders.
    /// Matches MSP §2.3 Folder.role.
    #[serde(default)]
    pub role: Option<Role>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum LabelKind {
    System,
    Folder,
    User,
}

/// Well-known role for a `Label`/folder, mirroring MSP §2.3. The set
/// covers the roles every consumer-mail provider exposes; provider-specific
/// roles outside this set are represented as `None` so clients fall back
/// to generic styling. New variants land additively under
/// `#[non_exhaustive]` to keep wire compatibility cheap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Role {
    Inbox,
    Sent,
    Drafts,
    Trash,
    Spam,
    Archive,
    AllMail,
    Important,
    Starred,
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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

// -- LargestMessages ---------------------------------------------------------

/// Single message ranked by its envelope `size_bytes`. Powers
/// `mxr storage --by message`: lets users find and act on the single
/// biggest emails (the 250 MB attachment from a courier service, the
/// massive zip a colleague sent in 2017) instead of just the bucket totals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct LargestMessageRow {
    pub message_id: MessageId,
    pub from_email: String,
    pub subject: String,
    pub size_bytes: u64,
    pub date: DateTime<Utc>,
}

// -- Wrapped -----------------------------------------------------------------

/// Year-in-review summary returned by `mxr wrapped`. Combines volume,
/// time-pattern, contact, reply-discipline, storage, newsletter, and
/// superlative sections so the CLI can render a single narrative panel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WrappedSummary {
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub label: String,
    pub volume: WrappedVolume,
    pub time_patterns: WrappedTimePatterns,
    pub top_contacts: WrappedTopContacts,
    pub reply_discipline: Option<WrappedReplyDiscipline>,
    pub storage: WrappedStorage,
    pub newsletters: WrappedNewsletters,
    pub superlatives: WrappedSuperlatives,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WrappedVolume {
    pub inbound_count: u32,
    pub outbound_count: u32,
    pub thread_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WrappedTimePatterns {
    /// Day name and message count for the busiest day-of-week (Mon–Sun).
    pub busiest_day_of_week: Option<String>,
    pub busiest_day_of_week_count: u32,
    /// Hour 0–23 (UTC) and message count for the busiest hour-of-day.
    pub busiest_hour_utc: Option<u8>,
    pub busiest_hour_count: u32,
    /// The single calendar day with the most activity, and its count.
    pub busiest_date: Option<DateTime<Utc>>,
    pub busiest_date_count: u32,
    /// Per-hour message counts (UTC), indexed 0..=23. Powers the
    /// hour-of-day chart in the TUI Wrapped view.
    /// `#[serde(default)]` keeps older daemons compatible.
    #[serde(default = "default_hour_distribution")]
    #[cfg_attr(feature = "openapi", schema(value_type = Vec<u32>))]
    pub hour_distribution: [u32; 24],
    /// Per-day-of-week message counts. Index 0 = Monday … 6 = Sunday.
    /// `#[serde(default)]` keeps older daemons compatible.
    #[serde(default = "default_day_of_week_distribution")]
    #[cfg_attr(feature = "openapi", schema(value_type = Vec<u32>))]
    pub day_of_week_distribution: [u32; 7],
}

fn default_hour_distribution() -> [u32; 24] {
    [0; 24]
}

fn default_day_of_week_distribution() -> [u32; 7] {
    [0; 7]
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WrappedTopContacts {
    /// Top 5 senders to me by inbound count.
    pub most_emailed_to_me: Vec<WrappedContactRank>,
    /// Top 5 recipients I emailed by outbound count.
    pub most_emailed_by_me: Vec<WrappedContactRank>,
    /// Top 3 most-asymmetric counterparties (inbound-heavy).
    pub most_asymmetric: Vec<ContactAsymmetryRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WrappedContactRank {
    pub email: String,
    pub display_name: Option<String>,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WrappedReplyDiscipline {
    pub sample_count: u32,
    pub clock_p50_seconds: u32,
    pub clock_p90_seconds: u32,
    pub business_hours_p50_seconds: Option<u32>,
    pub business_hours_p90_seconds: Option<u32>,
    /// Single fastest reply pair in the window.
    pub fastest: Option<WrappedReplyExtreme>,
    /// Single slowest reply pair in the window. Capped at 30 days to
    /// exclude the pathological "I replied 8 years later" cases.
    pub slowest: Option<WrappedReplyExtreme>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WrappedReplyExtreme {
    pub counterparty_email: String,
    pub latency_seconds: u32,
    pub replied_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WrappedStorage {
    pub total_bytes: u64,
    pub top_mimetype: Option<WrappedStorageBucket>,
    pub heaviest_message: Option<LargestMessageRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WrappedStorageBucket {
    pub key: String,
    pub bytes: u64,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WrappedNewsletters {
    pub unique_lists: u32,
    pub top_list: Option<WrappedTopList>,
    /// 0.0–100.0; share of inbound messages that came via a list_id.
    pub list_share_of_inbound_pct: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WrappedTopList {
    pub list_id: String,
    pub message_count: u32,
    pub opened_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WrappedSuperlatives {
    pub longest_thread: Option<WrappedLongestThread>,
    pub most_ghosted: Option<WrappedMostGhosted>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WrappedLongestThread {
    pub thread_id: ThreadId,
    pub subject: String,
    pub message_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct WrappedMostGhosted {
    pub email: String,
    pub inbound_count: u32,
    pub outbound_count: u32,
}

// -- ResponseTime ------------------------------------------------------------

/// Aggregate response-time summary for `mxr response-time`. p50/p90 in
/// seconds; business-hours percentiles are `None` until the reconciler has
/// backfilled the relevant rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ResponseTimeSummary {
    pub direction: ResponseTimeDirection,
    pub sample_count: u32,
    pub clock_p50_seconds: u32,
    pub clock_p90_seconds: u32,
    pub business_hours_p50_seconds: Option<u32>,
    pub business_hours_p90_seconds: Option<u32>,
    /// Distribution of clock latencies (seconds) bucketed for the
    /// histogram view in the TUI / future CLI surface. Ordered from
    /// shortest to longest by `upper_bound_seconds`. `#[serde(default)]`
    /// so older daemons returning the previous payload deserialize
    /// cleanly during a rolling upgrade.
    #[serde(default)]
    pub histogram: Vec<ResponseTimeBucket>,
}

/// A single bucket of the response-time histogram. `count` rows had a
/// clock latency in `(prev_upper, upper_bound_seconds]`. The last
/// bucket uses `u32::MAX` as `upper_bound_seconds` to mean "no upper
/// limit".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ResponseTimeBucket {
    pub upper_bound_seconds: u32,
    pub count: u32,
}

/// Fixed bucket edges for the response-time histogram. Tuned for
/// email reply latency: <1m, <5m, <30m, <1h, <6h, <1d, <3d, ≥3d.
/// Edges are exclusive upper bounds; the final bucket uses
/// `u32::MAX` to capture everything ≥ 3 days.
pub const RESPONSE_TIME_HISTOGRAM_EDGES: [u32; 8] =
    [60, 300, 1800, 3600, 21_600, 86_400, 259_200, u32::MAX];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct StaleThreadRow {
    pub thread_id: ThreadId,
    pub latest_message_id: MessageId,
    pub latest_subject: String,
    pub counterparty_email: String,
    pub latest_date: DateTime<Utc>,
    pub days_stale: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct StorageBucket {
    pub key: String,
    pub bytes: u64,
    pub count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
    #[cfg_attr(feature = "openapi", schema(value_type = u32))]
    pub flags: MessageFlags,
    pub snippet: String,
    pub has_attachments: bool,
    pub size_bytes: u64,
    pub unsubscribe: UnsubscribeMethod,
    /// Count of external links in the body after filtering out tracker /
    /// unsubscribe / list-management hostnames. Drives the tri-state
    /// "link" indicator on the mail list and the `has:link` search filter.
    /// `0` means no link indicator.
    #[serde(default)]
    pub link_count: u32,
    /// Word count of the rendered body, snapshot at sync time. Used together
    /// with `link_count` to classify newsletter-shaped mail into
    /// `LinkDensity::Heavy`. `0` if the body word count wasn't computed
    /// (older rows pre-backfill).
    #[serde(default)]
    pub body_word_count: u32,
    /// Provider-specific label IDs (e.g. "INBOX", "SENT", "Label_123").
    /// Transient: used during sync to populate the message_labels junction table.
    #[serde(default)]
    pub label_provider_ids: Vec<String>,
    /// Custom IMAP-style keywords (`$Forwarded`, `$NotJunk`, user-defined
    /// `$Work`, etc.). System flags continue to live in `flags`. Stored
    /// case-preserved as received; equality is case-sensitive to match
    /// IMAP atom semantics.
    #[serde(default)]
    pub keywords: BTreeSet<String>,
}

/// Tri-state classification of how link-bearing a message body is. Renders
/// as: blank / single link icon / double-or-emphasized link icon.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum LinkDensity {
    /// No external links survived filtering.
    #[default]
    None,
    /// At least one external link, body is not dominated by them.
    Some,
    /// Many links and/or high link-to-word ratio (newsletter-shaped).
    Heavy,
}

/// Word-count divisor used in the heavy-density ratio. Keep in one place so
/// the threshold logic stays consistent between sync, search, and tests.
const LINK_DENSITY_WORDS_PER_LINK: u32 = 50;

/// Absolute link-count above which a body counts as `Heavy` regardless of
/// length. Picked so that newsletters reliably trip the heavier tier without
/// flagging short "here's the doc:" replies.
const LINK_DENSITY_HEAVY_ABSOLUTE: u32 = 5;

impl Envelope {
    /// Classify this envelope's link presence into the tri-state used by the
    /// mail-list indicator and rule conditions. Derived from `link_count` and
    /// `body_word_count` so the threshold can be retuned without re-running
    /// sync.
    pub fn link_density(&self) -> LinkDensity {
        Self::classify_link_density(self.link_count, self.body_word_count)
    }

    /// Pure helper exposed so the sync / search / doctor crates can classify
    /// counts without constructing an `Envelope`.
    pub fn classify_link_density(link_count: u32, body_word_count: u32) -> LinkDensity {
        if link_count == 0 {
            return LinkDensity::None;
        }
        if link_count >= LINK_DENSITY_HEAVY_ABSOLUTE {
            return LinkDensity::Heavy;
        }
        // Density = links / (words / WORDS_PER_LINK). Heavy when ≥ 1.0,
        // implemented in integer arithmetic to avoid floating-point in the hot
        // path. Equivalent to `link_count * WORDS_PER_LINK >= max(1, words)`.
        let denominator = body_word_count.max(1);
        if link_count * LINK_DENSITY_WORDS_PER_LINK >= denominator {
            return LinkDensity::Heavy;
        }
        LinkDensity::Some
    }
}

impl LinkDensity {
    pub fn as_db_u8(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Some => 1,
            Self::Heavy => 2,
        }
    }

    pub fn from_db_u8(value: u8) -> Self {
        match value {
            2 => Self::Heavy,
            1 => Self::Some,
            _ => Self::None,
        }
    }
}

// -- UnsubscribeMethod --------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
    /// Addresses from the `Reply-To:` header, if the sender set one.
    /// Replies target these instead of `From:` (mailing lists and
    /// no-reply senders rely on it). Empty when absent.
    #[serde(default)]
    pub reply_to: Vec<Address>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum BodyPartSource {
    Exact,
    DerivedFromPlain,
    DerivedFromHtml,
    BestEffortSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum TextPlainFormat {
    Fixed,
    Flowed { delsp: bool },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CalendarMetadata {
    pub method: Option<String>,
    pub summary: Option<String>,
    #[serde(default)]
    pub component_kind: Option<String>,
    #[serde(default)]
    pub uid: Option<String>,
    #[serde(default)]
    pub sequence: Option<i64>,
    #[serde(default)]
    pub recurrence_id: Option<String>,
    #[serde(default)]
    pub dtstamp: Option<String>,
    #[serde(default)]
    pub starts_at: Option<String>,
    #[serde(default)]
    pub ends_at: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub rrule: Option<String>,
    #[serde(default)]
    pub organizer: Option<CalendarPerson>,
    #[serde(default)]
    pub attendees: Vec<CalendarAttendee>,
    #[serde(default)]
    pub rsvp_requested: bool,
    #[serde(default)]
    pub raw_ics: Option<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    /// Derived: the PARTSTAT of the attendee whose address matches one of the
    /// viewing account's addresses. `None` if no attendee matched or if
    /// multiple matched ambiguously (the strict send path errors instead).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewer_partstat: Option<CalendarPartstat>,
    /// Derived: the email of the matched viewer attendee, as it appears in
    /// the iCal `ATTENDEE` property. Used by the comment-compose path so the
    /// REPLY's `ATTENDEE` matches exactly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewer_attendee_email: Option<String>,
    /// Derived: true when the daemon has seen a prior REQUEST with the same
    /// UID and a lower SEQUENCE — i.e. this is a rescheduled / amended
    /// invite.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_update: bool,
}

/// Typed view of an iCalendar `PARTSTAT` value, derived for the viewing
/// account's own attendee row. Maps 1:1 to the strings defined in RFC 5545.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum CalendarPartstat {
    NeedsAction,
    Accepted,
    Tentative,
    Declined,
    Delegated,
}

impl CalendarPartstat {
    /// Parse the raw iCal `PARTSTAT=…` value (case-insensitive).
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "NEEDS-ACTION" => Some(Self::NeedsAction),
            "ACCEPTED" => Some(Self::Accepted),
            "TENTATIVE" => Some(Self::Tentative),
            "DECLINED" => Some(Self::Declined),
            "DELEGATED" => Some(Self::Delegated),
            _ => None,
        }
    }

    /// The wire iCal representation.
    pub fn as_ical(self) -> &'static str {
        match self {
            Self::NeedsAction => "NEEDS-ACTION",
            Self::Accepted => "ACCEPTED",
            Self::Tentative => "TENTATIVE",
            Self::Declined => "DECLINED",
            Self::Delegated => "DELEGATED",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CalendarPerson {
    pub email: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CalendarAttendee {
    pub email: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub uri: Option<String>,
    #[serde(default)]
    pub partstat: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub rsvp: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CalendarReplyMessage {
    pub to: Address,
    pub subject: String,
    pub body_text: String,
    pub ics: String,
}

// -- MessageBody --------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
    /// Point the body and every attachment on it at `message_id`.
    ///
    /// `attachments.message_id` is a foreign key onto the same row the body
    /// belongs to, so the two can never disagree. Needed when the store
    /// resolves a message to an id other than the one the provider computed.
    pub fn set_message_id(&mut self, message_id: MessageId) {
        for attachment in &mut self.attachments {
            attachment.message_id = message_id.clone();
        }
        self.message_id = message_id;
    }

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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
    #[cfg_attr(feature = "openapi", schema(value_type = Option<String>))]
    pub local_path: Option<PathBuf>,
    pub provider_id: String,
}

impl AttachmentMeta {
    /// Whether this attachment carries an iCalendar payload. Some providers
    /// (notably Gmail for self-invites/forwards) deliver an invite's `.ics`
    /// only as an attachment with no inline `text/calendar` part.
    pub fn is_calendar(&self) -> bool {
        let mime = self.mime_type.to_ascii_lowercase();
        mime.starts_with("text/calendar")
            || mime == "application/ics"
            || self.filename.to_ascii_lowercase().ends_with(".ics")
    }

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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum AttachmentDisposition {
    Attachment,
    Inline,
    #[default]
    Unspecified,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct HtmlImageAsset {
    pub source: String,
    pub kind: HtmlImageSourceKind,
    pub status: HtmlImageAssetStatus,
    pub mime_type: Option<String>,
    #[cfg_attr(feature = "openapi", schema(value_type = Option<String>))]
    pub path: Option<PathBuf>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum HtmlImageSourceKind {
    Cid,
    DataUri,
    Remote,
    ContentLocation,
    File,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Thread {
    pub id: ThreadId,
    pub account_id: AccountId,
    pub subject: String,
    pub participants: Vec<Address>,
    pub message_count: u32,
    pub unread_count: u32,
    pub latest_date: DateTime<Utc>,
    pub snippet: String,
    /// Constituent message IDs ordered by `date` ascending, ties
    /// broken by `MessageId` ascending. Empty `message_ids` denotes
    /// a tombstoned thread (typically the loser side of a
    /// mail-threading merge); clients receiving such a Thread in
    /// `SyncBatch.threads_changed` SHOULD drop any cached metadata
    /// for that id.
    #[serde(default)]
    pub message_ids: Vec<MessageId>,
}

// -- Draft --------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum DraftIntent {
    #[default]
    New,
    Reply,
    ReplyAll,
    Forward,
}

impl DraftIntent {
    pub fn is_new(&self) -> bool {
        matches!(self, Self::New)
    }

    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Reply => "reply",
            Self::ReplyAll => "reply_all",
            Self::Forward => "forward",
        }
    }

    pub fn from_db_str(value: &str) -> Self {
        match value {
            "reply" => Self::Reply,
            "reply_all" => Self::ReplyAll,
            "forward" => Self::Forward,
            _ => Self::New,
        }
    }
}

/// An image referenced from an HTML body by `<img src="cid:NAME">`.
///
/// Only the path crosses IPC; the daemon reads the bytes at send time, the
/// same contract `Draft::attachments` uses. Keeping bytes off the wire matters
/// because the IPC frame cap is 16 MiB.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct InlineAsset {
    /// The `cid:` token the HTML references, without angle brackets.
    pub cid: String,
    #[cfg_attr(feature = "openapi", schema(value_type = String))]
    pub path: PathBuf,
}

/// A draft's body, as one of two mutually exclusive kinds.
///
/// Modelled as a variant type rather than a bag of optional strings so a draft
/// cannot hold both a markdown source and a supplied HTML document. Callers
/// that need text for analysis (safety checks, LLM assist, export, previews)
/// go through [`DraftContent::analysis_text`]; callers that render for the
/// wire match on the variant explicitly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DraftContent {
    /// The historical path: markdown authored in `$EDITOR`, rendered to
    /// `text/plain` + `text/html` by the outbound builder.
    Markdown { source: String },
    /// A supplied HTML document, preserved exactly. `text` is the caller's
    /// `text/plain` alternative; when `None` the outbound builder generates a
    /// deterministic one from the HTML without touching the HTML itself.
    Html { html: String, text: Option<String> },
}

impl Default for DraftContent {
    fn default() -> Self {
        Self::Markdown {
            source: String::new(),
        }
    }
}

impl DraftContent {
    pub fn markdown(source: impl Into<String>) -> Self {
        Self::Markdown {
            source: source.into(),
        }
    }

    pub fn html(html: impl Into<String>, text: Option<String>) -> Self {
        Self::Html {
            html: html.into(),
            text,
        }
    }

    pub fn is_html(&self) -> bool {
        matches!(self, Self::Html { .. })
    }

    /// Discriminator persisted in `drafts.content_kind`.
    pub fn kind_str(&self) -> &'static str {
        match self {
            Self::Markdown { .. } => "markdown",
            Self::Html { .. } => "html",
        }
    }

    /// The markdown source, or `None` for an HTML draft.
    ///
    /// Deliberately not a `&str` that falls back to `""`: an empty-string
    /// fallback would make safety checks silently analyse nothing on HTML
    /// drafts. Use [`Self::analysis_text`] when you want text regardless of
    /// kind.
    pub fn markdown_source(&self) -> Option<&str> {
        match self {
            Self::Markdown { source } => Some(source),
            Self::Html { .. } => None,
        }
    }

    /// Size of the authored body in bytes, for size accounting and previews.
    ///
    /// Measures the markdown source or the HTML document — the thing the user
    /// actually wrote — not the generated text alternative.
    pub fn byte_len(&self) -> usize {
        match self {
            Self::Markdown { source } => source.len(),
            Self::Html { html, .. } => html.len(),
        }
    }

    /// The body as reader-pipeline input: `(text, html)`.
    ///
    /// Lets HTML drafts feed their real document into `mxr_reader::clean`
    /// rather than a flattened approximation, so safety checks and commitment
    /// extraction see what the recipient will see.
    pub fn reader_input(&self) -> (Option<&str>, Option<&str>) {
        match self {
            Self::Markdown { source } => (Some(source.as_str()), None),
            Self::Html { html, text } => (text.as_deref(), Some(html.as_str())),
        }
    }

    /// Text suitable for reading, analysis, and preview, for either kind.
    ///
    /// Markdown drafts yield their source. HTML drafts yield the supplied
    /// `text/plain` alternative when there is one; otherwise the caller should
    /// fall back to the generated alternative, which lives in `mxr-outbound`
    /// (this crate has no HTML dependency by design). Returning the empty
    /// string here is honest: there is no text yet.
    pub fn analysis_text(&self) -> &str {
        match self {
            Self::Markdown { source } => source,
            Self::Html { text, .. } => text.as_deref().unwrap_or(""),
        }
    }
}

/// Flat wire shape for [`DraftContent`].
///
/// Keeps the JSON contract backward compatible: a markdown draft serialises to
/// exactly the historical `{"body_markdown": "..."}`, so existing clients see
/// byte-identical output. HTML drafts carry `body_html`/`body_text`, which
/// legacy clients simply do not model.
#[derive(Serialize, Deserialize)]
struct DraftContentWire {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    body_markdown: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    body_html: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    body_text: Option<String>,
}

impl Serialize for DraftContent {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let wire = match self {
            Self::Markdown { source } => DraftContentWire {
                // Always emitted, even when empty, so the markdown wire shape
                // is unchanged from before HTML drafts existed.
                body_markdown: Some(source.clone()),
                body_html: None,
                body_text: None,
            },
            Self::Html { html, text } => DraftContentWire {
                body_markdown: None,
                body_html: Some(html.clone()),
                body_text: text.clone(),
            },
        };
        wire.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for DraftContent {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = DraftContentWire::deserialize(deserializer)?;
        match (wire.body_markdown, wire.body_html) {
            // Rejected rather than resolved by precedence: a payload carrying
            // both is a caller bug, and silently dropping one body is how the
            // wrong message gets sent.
            (Some(_), Some(_)) => Err(serde::de::Error::custom(
                "draft carries both body_markdown and body_html; they are mutually exclusive",
            )),
            (_, Some(html)) => Ok(Self::Html {
                html,
                text: wire.body_text,
            }),
            // Symmetric with the rejection above. A `body_text` with no
            // `body_html` used to fall through to empty markdown, silently
            // discarding the caller's text and — when the payload targeted an
            // existing HTML draft — its stored document too. Strict in one
            // direction and lossy in the other is indefensible.
            (_, None) if wire.body_text.is_some() => Err(serde::de::Error::custom(
                "draft carries body_text without body_html; a text alternative \
                 has no meaning without the document it accompanies",
            )),
            (markdown, None) => Ok(Self::Markdown {
                source: markdown.unwrap_or_default(),
            }),
        }
    }
}

/// Documents the flattened [`DraftContent`] wire shape for OpenAPI.
///
/// `DraftContent` hand-rolls its serde impls to stay backward compatible, so
/// a derived schema on the enum itself would describe the Rust shape rather
/// than the JSON. This mirrors what actually goes over the wire.
#[cfg(feature = "openapi")]
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct DraftContentSchema {
    /// Present on markdown drafts.
    pub body_markdown: Option<String>,
    /// Present on HTML drafts. Mutually exclusive with `body_markdown`.
    pub body_html: Option<String>,
    /// Optional `text/plain` alternative supplied alongside `body_html`.
    pub body_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Draft {
    pub id: DraftId,
    pub account_id: AccountId,
    /// Per-message From override. `None` sends from the account's primary
    /// address (default). `Some(addr)` sends from a specific owned address
    /// (the primary or a registered alias); the daemon validates ownership at
    /// send time and rejects a `from` the account does not own. The name falls
    /// back to the account's display name when the override carries none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<Address>,
    pub reply_headers: Option<ReplyHeaders>,
    #[serde(default)]
    pub intent: DraftIntent,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub bcc: Vec<Address>,
    pub subject: String,
    /// The body, as either a markdown source or a supplied HTML document.
    /// Flattened so the JSON stays flat: `body_markdown`, or
    /// `body_html`+`body_text`.
    #[serde(flatten)]
    #[cfg_attr(feature = "openapi", schema(value_type = DraftContentSchema))]
    pub content: DraftContent,
    #[cfg_attr(feature = "openapi", schema(value_type = Vec<String>))]
    pub attachments: Vec<PathBuf>,
    /// CID-referenced inline images for an HTML body. Always empty for a
    /// markdown draft.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inline_assets: Vec<InlineAsset>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Set when this draft is the user-comment compose path for an iCal
    /// invite REPLY. The outbound builder emits the special
    /// `multipart/alternative` (text/plain + text/calendar;method=REPLY)
    /// layout when this is `Some`. The daemon's send-completion hook reads
    /// `source_message_id`+`partstat` to update the local store's PARTSTAT
    /// after the email lands in Sent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inline_calendar_reply: Option<InlineCalendarReply>,
}

/// Current provider representation of a linked server-side draft.
/// `revision` changes whenever the provider replaces the nested message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerDraftSnapshot {
    pub revision: String,
    pub raw_rfc822: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct InlineCalendarReply {
    pub source_message_id: MessageId,
    pub attendee_email: String,
    pub partstat: CalendarPartstat,
    pub ics_body: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum DraftSafetySeverity {
    Info,
    Warning,
    Blocker,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum DraftSafetyVerdict {
    #[default]
    Safe,
    Warn,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum DraftSafetyIssueCode {
    NoRecipients,
    InvalidRecipient,
    MissingReplyAllRecipient,
    WrongRecipient,
    MissingAttachment,
    ReplyAll,
    PiiSecret,
    ToneMismatch,
    AnswerCoverage,
    CommitmentCandidate,
    SendTimeNote,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CitationRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub field: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub quote: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DraftSafetyIssue {
    pub code: DraftSafetyIssueCode,
    pub severity: DraftSafetySeverity,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub citations: Vec<CitationRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_token: Option<String>,
}

impl DraftSafetyIssue {
    pub fn new(
        code: DraftSafetyIssueCode,
        severity: DraftSafetySeverity,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            severity,
            message: message.into(),
            detail: None,
            citations: Vec::new(),
            override_token: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn with_citations(mut self, citations: Vec<CitationRef>) -> Self {
        self.citations = citations;
        self
    }

    pub fn with_override_token(mut self, token: impl Into<String>) -> Self {
        self.override_token = Some(token.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct DraftSafetyReport {
    pub allowed: bool,
    pub issues: Vec<DraftSafetyIssue>,
    #[serde(default)]
    pub verdict: DraftSafetyVerdict,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked_at: Option<DateTime<Utc>>,
}

impl DraftSafetyReport {
    pub fn from_issues(issues: Vec<DraftSafetyIssue>) -> Self {
        let verdict = if issues
            .iter()
            .any(|i| i.severity == DraftSafetySeverity::Blocker)
        {
            DraftSafetyVerdict::Blocked
        } else if issues
            .iter()
            .any(|i| i.severity == DraftSafetySeverity::Warning)
        {
            DraftSafetyVerdict::Warn
        } else {
            DraftSafetyVerdict::Safe
        };
        Self {
            allowed: !matches!(verdict, DraftSafetyVerdict::Blocked),
            issues,
            verdict,
            checked_at: Some(Utc::now()),
        }
    }

    pub fn safe() -> Self {
        Self {
            allowed: true,
            issues: Vec::new(),
            verdict: DraftSafetyVerdict::Safe,
            checked_at: Some(Utc::now()),
        }
    }

    pub fn extend(&mut self, more: Vec<DraftSafetyIssue>) {
        self.issues.extend(more);
        self.verdict = if self
            .issues
            .iter()
            .any(|i| i.severity == DraftSafetySeverity::Blocker)
        {
            DraftSafetyVerdict::Blocked
        } else if self
            .issues
            .iter()
            .any(|i| i.severity == DraftSafetySeverity::Warning)
        {
            DraftSafetyVerdict::Warn
        } else {
            DraftSafetyVerdict::Safe
        };
        self.allowed = !matches!(self.verdict, DraftSafetyVerdict::Blocked);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
            Self::Draft => "draft",
            Self::Sending => "sending",
            Self::Sent => "sent",
        }
    }

    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "draft" => Some(Self::Draft),
            "sending" => Some(Self::Sending),
            "sent" => Some(Self::Sent),
            _ => None,
        }
    }
}

// -- SavedSearch --------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum SemanticProfileStatus {
    #[default]
    Pending,
    Ready,
    Indexing,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum SemanticChunkSourceKind {
    Header,
    Body,
    AttachmentSummary,
    AttachmentText,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum SemanticEmbeddingStatus {
    #[default]
    Pending,
    Ready,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SemanticStatusSnapshot {
    pub enabled: bool,
    pub active_profile: SemanticProfile,
    pub profiles: Vec<SemanticProfileRecord>,
    #[serde(default)]
    pub runtime: SemanticRuntimeMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
    #[cfg_attr(feature = "openapi", schema(value_type = u32))]
    pub latest_flags: MessageFlags,
    pub latest_has_attachments: bool,
    pub latest_size_bytes: u64,
    pub unsubscribe: UnsubscribeMethod,
    /// Number of messages from this sender that have been marked READ.
    /// Combined with `message_count` gives the open-rate used by `unsub --rank`.
    #[serde(default)]
    pub opened_count: u32,
    /// Stable JSON field. The subscriptions query currently returns zero;
    /// reply-pair counts power sender/contact analytics, not this ranker.
    #[serde(default)]
    pub replied_count: u32,
    /// Messages that landed in ARCHIVE without ever being read. Strong
    /// "this is noise" signal for the unsubscribe ranker.
    #[serde(default)]
    pub archived_unread_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum SortOrder {
    DateDesc,
    DateAsc,
    Relevance,
}

// -- Snoozed ------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Snoozed {
    pub message_id: MessageId,
    pub account_id: AccountId,
    pub snoozed_at: DateTime<Utc>,
    pub wake_at: DateTime<Utc>,
    pub original_labels: Vec<LabelId>,
}

// -- Sync types ---------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ImapMailboxCursor {
    pub mailbox: String,
    pub uid_validity: u32,
    pub uid_next: u32,
    #[serde(default)]
    pub highest_modseq: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
    #[serde(default)]
    pub x_gm_ext_1: bool,
}

/// Opaque resume token for `MailSyncProvider::sync_messages`. MSP §2.2:
/// the daemon persists and replays this without inspecting its contents.
/// Adapters own the serialisation — typically a versioned JSON envelope
/// like `{"v":1,"history_id":12345}` encoded as UTF-8 bytes. An empty
/// `Vec<u8>` is the initial-sync sentinel (no prior cursor).
///
/// The store column stays TEXT; the bytes round-trip as a UTF-8 string.
/// Adapters MUST emit valid UTF-8 (typically via `serde_json::to_vec`).
#[derive(Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(transparent)]
pub struct SyncCursor(pub Vec<u8>);

impl SyncCursor {
    pub const fn empty() -> Self {
        Self(Vec::new())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

// Custom Debug keeps logs bounded and avoids leaking account-scoped
// tokens that some adapters embed in their cursor payload (e.g. Gmail
// page tokens). Use `provider.describe_cursor(&cursor)` when a richer
// representation is wanted.
impl fmt::Debug for SyncCursor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SyncCursor(len={})", self.0.len())
    }
}

// -- SyncedMessage ------------------------------------------------------------

/// A message with both envelope and body, returned by sync.
/// Bodies are always fetched eagerly during sync — no lazy hydration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SyncedMessage {
    pub envelope: Envelope,
    pub body: MessageBody,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SyncBatch {
    pub upserted: Vec<SyncedMessage>,
    pub deleted_provider_ids: Vec<String>,
    pub label_changes: Vec<LabelChange>,
    pub next_cursor: SyncCursor,
    /// True iff the provider deliberately truncated this batch and
    /// the next `sync_messages()` call will yield more data
    /// immediately. False = caught up; the daemon may sleep normally.
    #[serde(default)]
    pub has_more: bool,
    /// MSP §2.8 — threads whose membership or metadata changed
    /// during this batch. Populated by the daemon-side sync engine;
    /// the provider trait does NOT populate this. Empty
    /// `Thread.message_ids` denotes a tombstoned/merged-away thread
    /// — clients SHOULD drop any cached metadata for that id.
    #[serde(default)]
    pub threads_changed: Vec<Thread>,
    /// How many messages this sync still has to deliver, counting the
    /// ones in this batch — the denominator a client needs for
    /// "3,000 of 50,000". Counted from the cursor this batch was fetched
    /// with, not from the start of the mailbox, so it stays right when a
    /// backfill resumes part-way through.
    ///
    /// `None` when the provider cannot say without paying for it: Gmail's
    /// `resultSizeEstimate` is not accurate enough to show a user, and IMAP
    /// would need a SEARCH over the remaining UID range. Only set it from a
    /// number the provider actually knows.
    #[serde(default)]
    pub remaining_estimate: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct LabelChange {
    pub provider_message_id: String,
    pub added_labels: Vec<String>,
    pub removed_labels: Vec<String>,
}

/// Adapter-facing mutation request. The daemon constructs one of
/// these per envelope from a higher-level `MutationCommand` and
/// passes it to [`crate::provider::MailSyncProvider::apply_mutation`] together with
/// a client-supplied `mutation_id` for idempotent retry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum Mutation {
    ModifyLabels {
        provider_message_id: String,
        add: Vec<String>,
        remove: Vec<String>,
    },
    Trash {
        provider_message_id: String,
    },
    SetRead {
        provider_message_id: String,
        read: bool,
    },
    SetStarred {
        provider_message_id: String,
        starred: bool,
    },
    /// Add/remove custom IMAP-style keywords on a message. Adapters
    /// whose `capabilities().mutate.custom_keywords` is false must
    /// return `MxrError::Provider` rather than silently dropping.
    SetKeywords {
        provider_message_id: String,
        add: Vec<String>,
        remove: Vec<String>,
    },
}

impl Mutation {
    pub fn provider_message_id(&self) -> &str {
        match self {
            Self::ModifyLabels {
                provider_message_id,
                ..
            }
            | Self::Trash {
                provider_message_id,
            }
            | Self::SetRead {
                provider_message_id,
                ..
            }
            | Self::SetStarred {
                provider_message_id,
                ..
            }
            | Self::SetKeywords {
                provider_message_id,
                ..
            } => provider_message_id,
        }
    }
}

// -- Export -------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum ExportFormat {
    Markdown,
    Json,
    Mbox,
    LlmContext,
}

// -- ProviderMeta -------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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

/// Namespaced provider capabilities, MSP-shaped (see `docs/msp/spec.md` §4).
///
/// Grouped into `sync`, `mutate`, `search`, and `push` namespaces so callers
/// can negotiate against the same shape MSP adapters will advertise on the
/// wire. The boolean fields inside each group are intentionally additive —
/// a missing field never means "unsupported," only "the older shape doesn't
/// carry this signal yet." Every namespace defaults to all-false.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SyncCapabilities {
    #[serde(default)]
    pub sync: SyncCaps,
    #[serde(default)]
    pub mutate: MutateCaps,
    #[serde(default)]
    pub search: SearchCaps,
    #[serde(default)]
    pub push: PushCaps,
}

/// Sync-namespace capabilities.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SyncCaps {
    /// Adapter supports incremental delta sync (cursor-driven); false means
    /// the daemon must full-sync every cycle.
    pub delta: bool,
    /// Adapter surfaces thread ids natively (Gmail `threadId`, JMAP
    /// `threadId`); false means the daemon falls back to JWZ via
    /// `mail-threading`.
    pub native_threading: bool,
}

/// Mutate-namespace capabilities.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct MutateCaps {
    /// True only for providers with stable multi-assign label semantics.
    /// False means placement is folder/mailbox-based, even if `sync_labels()`
    /// exposes folders through the shared `Label` type.
    pub labels: bool,
    /// Adapter exposes a batch mutation API (Gmail `batchModify`, IMAP
    /// `UID STORE` over ranges, JMAP `set` with multiple ids). The daemon
    /// uses this to coalesce multi-message operations into one round-trip.
    pub batch_operations: bool,
    /// Adapter persists and round-trips arbitrary `$Foo` keywords on
    /// message flags (IMAP RFC 3501 §2.3.2 atoms / JMAP keywords). The
    /// daemon refuses `Mutation::SetKeywords` against providers where
    /// this is false; Gmail historically has no keyword surface, so its
    /// adapter keeps the default `false`.
    pub custom_keywords: bool,
}

/// Search-namespace capabilities.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SearchCaps {
    /// Adapter can execute search server-side and return matching ids;
    /// otherwise the daemon falls back to the local Tantivy index.
    pub server_side: bool,
}

/// Push-namespace capabilities.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PushCaps {
    /// Adapter can open a long-lived provider-side change channel instead
    /// of being polled. Gmail Pub/Sub/webhook-style push is deferred until
    /// the product and security model for provider push is validated.
    pub streaming: bool,
}

// -- SendReceipt --------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SendReceipt {
    pub provider_message_id: Option<String>,
    pub sent_at: DateTime<Utc>,
    /// RFC 5322 Message-ID header rendered into the outgoing message. Used by
    /// the daemon to anchor the synthetic local Sent envelope, and by IMAP
    /// dedupe on subsequent sync.
    #[serde(default)]
    pub rfc2822_message_id: String,
}

#[cfg(test)]
mod draft_content_tests {
    use super::*;

    #[test]
    fn markdown_draft_keeps_the_historical_wire_shape() {
        let json = serde_json::to_value(DraftContent::markdown("hello")).unwrap();
        assert_eq!(json, serde_json::json!({ "body_markdown": "hello" }));
    }

    #[test]
    fn empty_markdown_still_emits_the_key() {
        // Legacy clients read `body_markdown` unconditionally; omitting it on
        // an empty body would break them.
        let json = serde_json::to_value(DraftContent::markdown("")).unwrap();
        assert_eq!(json, serde_json::json!({ "body_markdown": "" }));
    }

    #[test]
    fn legacy_payload_without_html_keys_deserialises_as_markdown() {
        let content: DraftContent =
            serde_json::from_value(serde_json::json!({ "body_markdown": "hi" })).unwrap();
        assert_eq!(content, DraftContent::markdown("hi"));
    }

    #[test]
    fn payload_with_no_body_key_at_all_is_empty_markdown() {
        let content: DraftContent = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(content, DraftContent::markdown(""));
    }

    #[test]
    fn html_roundtrips_with_its_text_alternative() {
        let content = DraftContent::html("<p>hi</p>", Some("hi".to_string()));
        let json = serde_json::to_value(&content).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "body_html": "<p>hi</p>", "body_text": "hi" })
        );
        assert_eq!(
            serde_json::from_value::<DraftContent>(json).unwrap(),
            content
        );
    }

    #[test]
    fn html_without_a_text_alternative_omits_the_key() {
        let json = serde_json::to_value(DraftContent::html("<p>hi</p>", None)).unwrap();
        assert_eq!(json, serde_json::json!({ "body_html": "<p>hi</p>" }));
    }

    #[test]
    fn carrying_both_bodies_is_rejected_rather_than_resolved_by_precedence() {
        let err = serde_json::from_value::<DraftContent>(
            serde_json::json!({ "body_markdown": "hi", "body_html": "<p>hi</p>" }),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("mutually exclusive"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn a_text_alternative_without_a_document_is_rejected() {
        // Previously fell through to empty markdown, silently discarding the
        // caller's text — and, when the payload targeted an existing HTML
        // draft, that draft's stored document with it.
        let err = serde_json::from_value::<DraftContent>(
            serde_json::json!({ "body_text": "plain words" }),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("without body_html"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn body_text_alongside_markdown_is_also_rejected() {
        let err = serde_json::from_value::<DraftContent>(
            serde_json::json!({ "body_markdown": "hi", "body_text": "hi" }),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("without body_html"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn the_strictness_is_symmetric_in_both_directions() {
        // Both bodies present -> rejected. Text without a document -> rejected.
        // Neither silently wins; that asymmetry was the root cause of a
        // data-loss bug.
        for payload in [
            serde_json::json!({ "body_markdown": "a", "body_html": "<p>a</p>" }),
            serde_json::json!({ "body_text": "a" }),
        ] {
            assert!(
                serde_json::from_value::<DraftContent>(payload.clone()).is_err(),
                "should have rejected: {payload}"
            );
        }
    }

    #[test]
    fn markdown_source_is_none_for_html_rather_than_an_empty_string() {
        // An empty-string fallback here would make safety checks analyse
        // nothing on HTML drafts.
        assert_eq!(
            DraftContent::html("<p>hi</p>", Some("hi".into())).markdown_source(),
            None
        );
        assert_eq!(DraftContent::markdown("hi").markdown_source(), Some("hi"));
    }

    #[test]
    fn analysis_text_prefers_the_supplied_alternative() {
        assert_eq!(DraftContent::markdown("md").analysis_text(), "md");
        assert_eq!(
            DraftContent::html("<p>x</p>", Some("plain".into())).analysis_text(),
            "plain"
        );
        assert_eq!(DraftContent::html("<p>x</p>", None).analysis_text(), "");
    }

    #[test]
    fn kind_str_matches_the_persisted_discriminator() {
        assert_eq!(DraftContent::markdown("").kind_str(), "markdown");
        assert_eq!(DraftContent::html("", None).kind_str(), "html");
    }

    #[test]
    fn is_html_follows_the_variant_not_the_body() {
        // Empty bodies on both sides, so only the variant can decide.
        assert!(DraftContent::html("", None).is_html());
        assert!(!DraftContent::markdown("").is_html());
        assert!(!DraftContent::markdown("<p>looks like html</p>").is_html());
    }

    #[test]
    fn an_empty_markdown_key_alongside_html_is_still_a_conflict() {
        // The boundary an "ignore it if it's empty" precedence rule would let
        // through. `body_markdown: ""` is exactly what the store writes beside
        // an HTML body, so a payload carrying both is still a caller bug.
        let err = serde_json::from_value::<DraftContent>(
            serde_json::json!({ "body_markdown": "", "body_html": "<p>hi</p>" }),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("mutually exclusive"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn default_is_the_empty_markdown_draft() {
        // `Draft` relies on this for the no-body-key case; a `Default` that
        // produced an HTML draft would flip an absent body into the HTML path.
        assert_eq!(DraftContent::default(), DraftContent::markdown(""));
    }

    #[test]
    fn byte_len_measures_the_authored_body_not_the_alternative() {
        // "é" is two bytes: bytes, not chars.
        assert_eq!(DraftContent::markdown("é").byte_len(), 2);
        assert_eq!(DraftContent::markdown("").byte_len(), 0);
        // The alternative is much longer than the HTML; byte_len must ignore it.
        assert_eq!(
            DraftContent::html("<p>x</p>", Some("a much longer alternative".into())).byte_len(),
            "<p>x</p>".len()
        );
    }

    #[test]
    fn reader_input_pairs_text_with_the_real_html_document() {
        assert_eq!(
            DraftContent::markdown("md").reader_input(),
            (Some("md"), None)
        );
        assert_eq!(
            DraftContent::html("<p>x</p>", Some("plain".into())).reader_input(),
            (Some("plain"), Some("<p>x</p>"))
        );
        // No supplied alternative means no text side, not an empty one — the
        // reader pipeline must fall back to the HTML rather than analyse "".
        assert_eq!(
            DraftContent::html("<p>x</p>", None).reader_input(),
            (None, Some("<p>x</p>"))
        );
    }

    fn draft_fixture(content: DraftContent) -> Draft {
        Draft {
            id: DraftId::new(),
            account_id: AccountId::new(),
            from: None,
            reply_headers: None,
            intent: DraftIntent::New,
            to: vec![Address {
                name: None,
                email: "bob@example.com".to_string(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "Subject".to_string(),
            content,
            attachments: vec![],
            inline_assets: vec![],
            inline_calendar_reply: None,
            created_at: DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
            updated_at: DateTime::from_timestamp(1_700_000_500, 0).unwrap(),
        }
    }

    #[test]
    fn markdown_draft_carries_body_markdown_at_the_top_level() {
        let draft = draft_fixture(DraftContent::markdown("# Hi"));
        let json = serde_json::to_value(&draft).unwrap();

        assert_eq!(json["body_markdown"], serde_json::json!("# Hi"));
        assert!(json.get("content").is_none(), "content must stay flattened");
        assert!(json.get("body_html").is_none());

        let back: Draft = serde_json::from_value(json).unwrap();
        assert_eq!(back.content, draft.content);
        assert_eq!(back.subject, draft.subject);
        assert_eq!(back.created_at, draft.created_at);
    }

    #[test]
    fn html_draft_roundtrips_through_the_flattened_draft_shape() {
        let mut draft = draft_fixture(DraftContent::html("<p>hi</p>", Some("hi".into())));
        draft.inline_assets = vec![InlineAsset {
            cid: "logo".to_string(),
            path: PathBuf::from("/tmp/logo.png"),
        }];
        let json = serde_json::to_value(&draft).unwrap();

        assert_eq!(json["body_html"], serde_json::json!("<p>hi</p>"));
        assert_eq!(json["body_text"], serde_json::json!("hi"));
        assert!(json.get("body_markdown").is_none());

        let back: Draft = serde_json::from_value(json).unwrap();
        assert_eq!(back.content, draft.content);
        assert_eq!(back.inline_assets, draft.inline_assets);
    }

    #[test]
    fn a_draft_carrying_both_bodies_is_rejected_too() {
        // The flattened path buffers fields separately from the standalone
        // one, so the mutual exclusion has to be proven here as well.
        let mut json = serde_json::to_value(draft_fixture(DraftContent::markdown("hi"))).unwrap();
        json["body_html"] = serde_json::json!("<p>hi</p>");

        let err = serde_json::from_value::<Draft>(json).unwrap_err();
        assert!(
            err.to_string().contains("mutually exclusive"),
            "unexpected error: {err}"
        );
    }
}
