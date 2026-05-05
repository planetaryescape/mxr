/// Phase 2.5: state for the unified analytics screen. Four logical
/// sub-views (Storage / Stale Threads / Contact Asymmetry / Response
/// Time) share a Screen variant and one render path because they're
/// shaped identically: filter row → table → footer. Per-view rows
/// live in dedicated fields so each refresh path is independent.

use mxr_core::types::{
    ContactAsymmetryRow, ResponseTimeDirection, ResponseTimeSummary, StaleBallInCourt,
    StaleThreadRow, StorageBucket, StorageGroupBy,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalyticsView {
    Storage,
    StaleThreads,
    ContactAsymmetry,
    ResponseTime,
}

impl AnalyticsView {
    pub fn label(self) -> &'static str {
        match self {
            Self::Storage => "Storage",
            Self::StaleThreads => "Stale Threads",
            Self::ContactAsymmetry => "Contact Asymmetry",
            Self::ResponseTime => "Response Time",
        }
    }
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

    pub storage_rows: Vec<StorageBucket>,
    pub storage_group_by: StorageGroupBy,

    pub stale_rows: Vec<StaleThreadRow>,
    pub stale_perspective: StaleBallInCourt,
    pub stale_older_than_days: u32,
    pub stale_within_days: u32,

    pub asymmetry_rows: Vec<ContactAsymmetryRow>,
    pub asymmetry_min_inbound: u32,

    pub response_time: Option<ResponseTimeSummary>,
    pub response_time_direction: ResponseTimeDirection,
}

impl Default for AnalyticsState {
    fn default() -> Self {
        Self {
            view: AnalyticsView::Storage,
            selected_index: 0,
            loading: false,
            error: None,
            refresh_pending: false,
            storage_rows: Vec::new(),
            storage_group_by: StorageGroupBy::Sender,
            stale_rows: Vec::new(),
            stale_perspective: StaleBallInCourt::Mine,
            stale_older_than_days: 30,
            stale_within_days: 365,
            asymmetry_rows: Vec::new(),
            asymmetry_min_inbound: 5,
            response_time: None,
            response_time_direction: ResponseTimeDirection::IReplied,
        }
    }
}
