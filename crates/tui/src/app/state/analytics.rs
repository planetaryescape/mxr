/// State for the unified analytics screen. Six top-level views share
/// one Screen variant: Storage, Stale Threads, Contacts, Response
/// Time, Subscriptions, and Wrapped. Storage and Contacts each carry
/// a sub-mode toggle (Storage: Breakdown vs LargestMessages; Contacts:
/// Asymmetry vs Decay) so a single tab can surface multiple CLI
/// surfaces without exploding the top-level cycle. Per-view rows live
/// in dedicated fields so each refresh path is independent.
use mxr_core::types::{
    ContactAsymmetryRow, ContactDecayRow, LargestMessageRow, ResponseTimeDirection,
    ResponseTimeSummary, StaleBallInCourt, StaleThreadRow, StorageBucket, StorageGroupBy,
    SubscriptionSummary, WrappedSummary,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalyticsView {
    Storage,
    StaleThreads,
    Contacts,
    ResponseTime,
    Subscriptions,
    Wrapped,
}

impl AnalyticsView {
    pub fn label(self) -> &'static str {
        match self {
            Self::Storage => "Storage",
            Self::StaleThreads => "Stale Threads",
            Self::Contacts => "Contacts",
            Self::ResponseTime => "Response Time",
            Self::Subscriptions => "Subscriptions",
            Self::Wrapped => "Wrapped",
        }
    }
}

/// Storage sub-mode. `Breakdown` aggregates by sender/mimetype/label
/// (the three group_by axes the CLI exposes); `LargestMessages` lists
/// individual messages by size (CLI: `mxr storage --by message`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageMode {
    Breakdown,
    LargestMessages,
}

/// Contacts sub-mode. `Asymmetry` ranks correspondents by inbound vs
/// outbound imbalance; `Decay` lists going-cold relationships
/// (inbound newer than outbound by a threshold). CLI surfaces:
/// `mxr contacts asymmetry` and `mxr contacts decay`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContactsMode {
    Asymmetry,
    Decay,
}

/// Wrapped time window. Mirrors the three CLI window flags
/// (`--ytd`, `--year YYYY`, `--since-days N`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WrappedWindow {
    Ytd,
    Year(i32),
    SinceDays(u32),
}

#[derive(Debug, Clone)]
pub struct AnalyticsState {
    pub view: AnalyticsView,
    pub selected_index: usize,
    pub loading: bool,
    pub error: Option<String>,
    /// Set when navigation enters the Analytics screen so the dispatcher
    /// fires the corresponding `List*` request next tick. Cleared after
    /// dispatch.
    pub refresh_pending: bool,

    pub storage_mode: StorageMode,
    pub storage_rows: Vec<StorageBucket>,
    pub storage_group_by: StorageGroupBy,
    pub largest_message_rows: Vec<LargestMessageRow>,
    pub largest_since_days: Option<u32>,
    pub largest_limit: u32,

    pub stale_rows: Vec<StaleThreadRow>,
    pub stale_perspective: StaleBallInCourt,
    pub stale_older_than_days: u32,
    pub stale_within_days: u32,

    pub contacts_mode: ContactsMode,
    pub asymmetry_rows: Vec<ContactAsymmetryRow>,
    pub asymmetry_min_inbound: u32,
    pub decay_rows: Vec<ContactDecayRow>,
    pub decay_threshold_days: u32,
    pub decay_max_lookback_days: u32,

    pub response_time: Option<ResponseTimeSummary>,
    pub response_time_direction: ResponseTimeDirection,
    pub response_time_counterparty: Option<String>,
    pub response_time_since_days: Option<u32>,

    pub subscriptions: Vec<SubscriptionSummary>,
    pub subscriptions_limit: u32,
    pub subscriptions_rank: bool,

    pub wrapped: Option<WrappedSummary>,
    pub wrapped_window: WrappedWindow,

    /// Set when the user presses `R` on the Contacts view; lib.rs
    /// dispatcher fires `Request::RefreshContacts` next tick. Cleared
    /// after dispatch. Separate from `refresh_pending` because
    /// RefreshContacts is a side-effecting maintenance call, not a
    /// view-data load.
    pub pending_contacts_refresh: bool,

    /// Index of the currently selected Wrapped tile (0..=6). Updated
    /// by h/j/k/l in tile-grid layout. Drill-down uses this to know
    /// which tile's destination to follow on Enter.
    pub wrapped_selected_tile: usize,
}

impl Default for AnalyticsState {
    fn default() -> Self {
        Self {
            view: AnalyticsView::Storage,
            selected_index: 0,
            loading: false,
            error: None,
            refresh_pending: false,

            storage_mode: StorageMode::Breakdown,
            storage_rows: Vec::new(),
            storage_group_by: StorageGroupBy::Sender,
            largest_message_rows: Vec::new(),
            largest_since_days: None,
            largest_limit: 50,

            stale_rows: Vec::new(),
            stale_perspective: StaleBallInCourt::Mine,
            stale_older_than_days: 30,
            stale_within_days: 365,

            contacts_mode: ContactsMode::Asymmetry,
            asymmetry_rows: Vec::new(),
            asymmetry_min_inbound: 5,
            decay_rows: Vec::new(),
            decay_threshold_days: 30,
            decay_max_lookback_days: 1095,

            response_time: None,
            response_time_direction: ResponseTimeDirection::IReplied,
            response_time_counterparty: None,
            response_time_since_days: None,

            subscriptions: Vec::new(),
            subscriptions_limit: 200,
            subscriptions_rank: false,

            wrapped: None,
            wrapped_window: WrappedWindow::Ytd,

            pending_contacts_refresh: false,
            wrapped_selected_tile: 0,
        }
    }
}
