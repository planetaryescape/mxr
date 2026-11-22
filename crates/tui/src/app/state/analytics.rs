use mxr_core::types::{
    ContactAsymmetryRow, ContactDecayRow, LargestMessageRow, ResponseTimeDirection,
    ResponseTimeSummary, StaleBallInCourt, StaleThreadRow, StorageBucket, StorageGroupBy,
    SubscriptionSummary, WrappedSummary,
};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use throbber_widgets_tui::ThrobberState;

pub const ANALYTICS_CACHE_TTL: Duration = Duration::from_secs(300);

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageMode {
    Breakdown,
    LargestMessages,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContactsMode {
    Asymmetry,
    Decay,
}

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
    pub loading: bool,
    pub error: Option<String>,
    pub refresh_pending: bool,
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

    pub pending_contacts_refresh: bool,

    pub wrapped_selected_tile: usize,

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
    pub fn current_cache_key(&self) -> AnalyticsCacheKey {
        Self::cache_key_for(self, self.view)
    }

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

    pub fn cache_is_fresh(&self, view: AnalyticsView) -> bool {
        let key = self.cache_key_for(view);
        match self.last_refresh_at.get(&key) {
            Some(t) => t.elapsed() < ANALYTICS_CACHE_TTL,
            None => false,
        }
    }

    pub fn should_show_cold_load(&self) -> bool {
        self.loading && !self.has_data_for_view(self.view)
    }

    pub fn is_refreshing_with_data(&self) -> bool {
        self.loading && self.has_data_for_view(self.view)
    }

    pub fn mark_refreshed(&mut self) {
        self.last_refresh_at
            .insert(self.current_cache_key(), Instant::now());
    }
}
