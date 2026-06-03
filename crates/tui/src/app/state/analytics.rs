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
use std::collections::HashMap;
use std::time::{Duration, Instant};
use throbber_widgets_tui::ThrobberState;

/// How long an analytics view's cached data is considered fresh
/// before a tab-switch will trigger a background refresh. Manual
/// refresh (`r`) and filter changes always refresh regardless.
pub const ANALYTICS_CACHE_TTL: Duration = Duration::from_secs(300);

/// Cache key disambiguating an analytics view by its filter combo.
/// Two requests with different filters (e.g., Storage grouped by
/// Sender vs Mimetype) must not share a freshness timestamp.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AnalyticsCacheKey {
    StorageBreakdown {
        group_by: StorageGroupBy,
    },
    StorageLargest {
        since_days: Option<u32>,
        limit: u32,
    },
    Stale {
        perspective: StaleBallInCourt,
        older_than_days: u32,
        within_days: u32,
    },
    ContactsAsymmetry {
        min_inbound: u32,
    },
    ContactsDecay {
        threshold_days: u32,
        max_lookback_days: u32,
    },
    CadenceDrift,
    ResponseTime {
        direction: ResponseTimeDirection,
        counterparty: Option<String>,
        since_days: Option<u32>,
    },
    Subscriptions {
        limit: u32,
    },
    SearchAggregation {
        query: String,
        group_by: mxr_protocol::SearchAggregationGroupBy,
        limit: u32,
    },
    Wrapped {
        window: WrappedWindow,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalyticsView {
    Storage,
    StaleThreads,
    Contacts,
    CadenceDrift,
    ResponseTime,
    Subscriptions,
    SearchAggregation,
    Wrapped,
}

impl AnalyticsView {
    pub fn label(self) -> &'static str {
        match self {
            Self::Storage => "Storage",
            Self::StaleThreads => "Stale Threads",
            Self::Contacts => "Contacts",
            Self::CadenceDrift => "Cadence Drift",
            Self::ResponseTime => "Response Time",
            Self::Subscriptions => "Subscriptions",
            Self::SearchAggregation => "Search Groups",
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WrappedWindow {
    Ytd,
    Year(i32),
    SinceDays(u32),
}

#[derive(Debug, Clone)]
pub struct AnalyticsState {
    pub view: AnalyticsView,
    pub selected_index: usize,
    /// True iff the dispatcher has fired a request and is waiting for a
    /// response. The renderer only paints a full "Computing analytics..."
    /// blank when `loading && !has_data_for_view(view)` — i.e., a true
    /// cold load. With cached data present, rendering keeps the prior
    /// view and shows a small refreshing indicator instead.
    pub loading: bool,
    pub error: Option<String>,
    /// Set when navigation enters the Analytics screen so the dispatcher
    /// fires the corresponding `List*` request next tick. Cleared after
    /// dispatch.
    pub refresh_pending: bool,
    /// Per-(view + filter) freshness timestamps. A pure tab switch
    /// (no filter change, no manual `r`) skips the refetch when the
    /// cached data for the destination view is younger than
    /// `ANALYTICS_CACHE_TTL`. Populated on every successful response.
    pub last_refresh_at: HashMap<AnalyticsCacheKey, Instant>,

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

    pub cadence_drift_rows: Vec<mxr_protocol::CadenceDriftRowData>,

    pub response_time: Option<ResponseTimeSummary>,
    pub response_time_direction: ResponseTimeDirection,
    pub response_time_counterparty: Option<String>,
    pub response_time_since_days: Option<u32>,

    pub subscriptions: Vec<SubscriptionSummary>,
    pub subscriptions_limit: u32,
    pub subscriptions_rank: bool,

    pub search_aggregation_rows: Vec<mxr_protocol::SearchAggregationRow>,
    pub search_aggregation_query: String,
    pub search_aggregation_group_by: mxr_protocol::SearchAggregationGroupBy,
    pub search_aggregation_limit: u32,

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

    /// Animated throbber state for the cold-load "Computing analytics..."
    /// indicator. Advanced by the tick handler whenever `loading` is set
    /// and no cached data exists for the active view.
    pub loading_throbber: ThrobberState,
}

impl Default for AnalyticsState {
    fn default() -> Self {
        Self {
            view: AnalyticsView::Storage,
            selected_index: 0,
            loading: false,
            error: None,
            refresh_pending: false,
            last_refresh_at: HashMap::new(),

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
            cadence_drift_rows: Vec::new(),

            response_time: None,
            response_time_direction: ResponseTimeDirection::IReplied,
            response_time_counterparty: None,
            response_time_since_days: None,

            subscriptions: Vec::new(),
            subscriptions_limit: 200,
            subscriptions_rank: false,

            search_aggregation_rows: Vec::new(),
            search_aggregation_query: "is:unread label:inbox".into(),
            search_aggregation_group_by: mxr_protocol::SearchAggregationGroupBy::From,
            search_aggregation_limit: 100,

            wrapped: None,
            wrapped_window: WrappedWindow::Ytd,

            pending_contacts_refresh: false,
            wrapped_selected_tile: 0,
            loading_throbber: ThrobberState::default(),
        }
    }
}

impl AnalyticsState {
    /// Cache key for the *currently active* view + filter combo. Used
    /// to look up freshness and to record a successful response.
    pub fn current_cache_key(&self) -> AnalyticsCacheKey {
        Self::cache_key_for(self, self.view)
    }

    /// Cache key for a *specific* view at the current filter values.
    /// Used by the tab-switch path to ask whether the destination view
    /// is fresh before deciding to refetch.
    pub fn cache_key_for(&self, view: AnalyticsView) -> AnalyticsCacheKey {
        match view {
            AnalyticsView::Storage => match self.storage_mode {
                StorageMode::Breakdown => AnalyticsCacheKey::StorageBreakdown {
                    group_by: self.storage_group_by,
                },
                StorageMode::LargestMessages => AnalyticsCacheKey::StorageLargest {
                    since_days: self.largest_since_days,
                    limit: self.largest_limit,
                },
            },
            AnalyticsView::StaleThreads => AnalyticsCacheKey::Stale {
                perspective: self.stale_perspective,
                older_than_days: self.stale_older_than_days,
                within_days: self.stale_within_days,
            },
            AnalyticsView::Contacts => match self.contacts_mode {
                ContactsMode::Asymmetry => AnalyticsCacheKey::ContactsAsymmetry {
                    min_inbound: self.asymmetry_min_inbound,
                },
                ContactsMode::Decay => AnalyticsCacheKey::ContactsDecay {
                    threshold_days: self.decay_threshold_days,
                    max_lookback_days: self.decay_max_lookback_days,
                },
            },
            AnalyticsView::CadenceDrift => AnalyticsCacheKey::CadenceDrift,
            AnalyticsView::ResponseTime => AnalyticsCacheKey::ResponseTime {
                direction: self.response_time_direction,
                counterparty: self.response_time_counterparty.clone(),
                since_days: self.response_time_since_days,
            },
            AnalyticsView::Subscriptions => AnalyticsCacheKey::Subscriptions {
                limit: self.subscriptions_limit,
            },
            AnalyticsView::SearchAggregation => AnalyticsCacheKey::SearchAggregation {
                query: self.search_aggregation_query.clone(),
                group_by: self.search_aggregation_group_by,
                limit: self.search_aggregation_limit,
            },
            AnalyticsView::Wrapped => AnalyticsCacheKey::Wrapped {
                window: self.wrapped_window,
            },
        }
    }

    /// Does the given view already have data populated? Used to
    /// decide whether a tab switch should trigger a refetch and
    /// whether to render a cold-load blank vs the cached view.
    pub fn has_data_for_view(&self, view: AnalyticsView) -> bool {
        match view {
            AnalyticsView::Storage => match self.storage_mode {
                StorageMode::Breakdown => !self.storage_rows.is_empty(),
                StorageMode::LargestMessages => !self.largest_message_rows.is_empty(),
            },
            AnalyticsView::StaleThreads => !self.stale_rows.is_empty(),
            AnalyticsView::Contacts => match self.contacts_mode {
                ContactsMode::Asymmetry => !self.asymmetry_rows.is_empty(),
                ContactsMode::Decay => !self.decay_rows.is_empty(),
            },
            AnalyticsView::CadenceDrift => !self.cadence_drift_rows.is_empty(),
            AnalyticsView::ResponseTime => self.response_time.is_some(),
            AnalyticsView::Subscriptions => !self.subscriptions.is_empty(),
            AnalyticsView::SearchAggregation => !self.search_aggregation_rows.is_empty(),
            AnalyticsView::Wrapped => self.wrapped.is_some(),
        }
    }

    /// True when the cached data for `view` (under current filters)
    /// was refreshed within `ANALYTICS_CACHE_TTL`. Returns `false` if
    /// no entry exists.
    pub fn cache_is_fresh(&self, view: AnalyticsView) -> bool {
        let key = self.cache_key_for(view);
        match self.last_refresh_at.get(&key) {
            Some(t) => t.elapsed() < ANALYTICS_CACHE_TTL,
            None => false,
        }
    }

    /// True iff the renderer should blank the analytics pane with a
    /// cold-load message. False when stale data exists; the renderer
    /// then keeps painting the cached view + a refreshing indicator.
    pub fn should_show_cold_load(&self) -> bool {
        self.loading && !self.has_data_for_view(self.view)
    }

    /// True iff a refresh is in flight while stale data is still on
    /// screen. Triggers the small "↻ refreshing" badge in the header.
    pub fn is_refreshing_with_data(&self) -> bool {
        self.loading && self.has_data_for_view(self.view)
    }

    /// Record a successful refresh for the active view + filter combo.
    /// Called from the response handler in the dispatcher.
    pub fn mark_refreshed(&mut self) {
        self.last_refresh_at
            .insert(self.current_cache_key(), Instant::now());
    }
}
