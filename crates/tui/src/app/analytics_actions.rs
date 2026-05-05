use super::*;

impl App {
    pub(super) fn apply_analytics_action(&mut self, action: Action) {
        match action {
            Action::OpenAnalyticsScreen => {
                self.screen = Screen::Analytics;
                self.analytics.refresh_pending = true;
            }
            Action::OpenAnalyticsView(view) => {
                self.screen = Screen::Analytics;
                self.analytics.view = view;
                self.analytics.selected_index = 0;
                self.analytics.error = None;
                self.analytics.refresh_pending = true;
            }
            Action::NextAnalyticsView => {
                self.analytics.view = next_analytics_view(self.analytics.view);
                self.analytics.selected_index = 0;
                self.analytics.error = None;
                self.analytics.refresh_pending = true;
            }
            Action::PrevAnalyticsView => {
                self.analytics.view = prev_analytics_view(self.analytics.view);
                self.analytics.selected_index = 0;
                self.analytics.error = None;
                self.analytics.refresh_pending = true;
            }
            Action::RefreshAnalytics => {
                self.analytics.error = None;
                self.analytics.refresh_pending = true;
            }
            _ => unreachable!("action routed to wrong handler"),
        }
    }

    /// Build the IPC request for the active analytics view. Returns
    /// `None` only if a programming error left an unhandled view —
    /// every variant is mapped today.
    pub(crate) fn analytics_request_for_active_view(&self) -> mxr_protocol::Request {
        match self.analytics.view {
            AnalyticsView::Storage => mxr_protocol::Request::ListStorageBreakdown {
                account_id: None,
                group_by: self.analytics.storage_group_by,
                limit: 100,
            },
            AnalyticsView::StaleThreads => mxr_protocol::Request::ListStaleThreads {
                account_id: None,
                perspective: self.analytics.stale_perspective,
                older_than_days: self.analytics.stale_older_than_days,
                within_days: self.analytics.stale_within_days,
                limit: 100,
            },
            AnalyticsView::ContactAsymmetry => mxr_protocol::Request::ListContactAsymmetry {
                account_id: None,
                min_inbound: self.analytics.asymmetry_min_inbound,
                limit: 100,
            },
            AnalyticsView::ResponseTime => mxr_protocol::Request::ListResponseTime {
                account_id: None,
                direction: self.analytics.response_time_direction,
                counterparty: None,
                since_days: None,
            },
        }
    }
}

fn next_analytics_view(view: AnalyticsView) -> AnalyticsView {
    match view {
        AnalyticsView::Storage => AnalyticsView::StaleThreads,
        AnalyticsView::StaleThreads => AnalyticsView::ContactAsymmetry,
        AnalyticsView::ContactAsymmetry => AnalyticsView::ResponseTime,
        AnalyticsView::ResponseTime => AnalyticsView::Storage,
    }
}

fn prev_analytics_view(view: AnalyticsView) -> AnalyticsView {
    match view {
        AnalyticsView::Storage => AnalyticsView::ResponseTime,
        AnalyticsView::StaleThreads => AnalyticsView::Storage,
        AnalyticsView::ContactAsymmetry => AnalyticsView::StaleThreads,
        AnalyticsView::ResponseTime => AnalyticsView::ContactAsymmetry,
    }
}
