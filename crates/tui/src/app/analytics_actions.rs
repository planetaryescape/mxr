use super::*;
use crate::app::state::{
    AnalyticsFilterField, AnalyticsFilterModalState, ContactsMode, StorageMode, WrappedWindow,
};
use chrono::{Datelike, TimeZone, Utc};

impl App {
    pub(super) fn apply_analytics_action(&mut self, action: Action) {
        match action {
            Action::OpenAnalyticsScreen => {
                // Cancel the auto-mark-read timer scheduled by mailbox
                // preview. Otherwise it fires later while we're in
                // Analytics, queues a SetRead the user didn't ask for,
                // and surfaces as a "Mutation Failed" modal if the
                // daemon's connection pool is busy.
                self.mailbox.pending_preview_read = None;
                self.screen = Screen::Analytics;
                let view = self.analytics.view;
                if !self.analytics.has_data_for_view(view) || !self.analytics.cache_is_fresh(view) {
                    self.analytics.refresh_pending = true;
                }
            }
            Action::OpenAnalyticsView(view) => {
                self.mailbox.pending_preview_read = None;
                self.screen = Screen::Analytics;
                self.analytics.view = view;
                self.analytics.selected_index = 0;
                self.analytics.error = None;
                if !self.analytics.has_data_for_view(view) || !self.analytics.cache_is_fresh(view) {
                    self.analytics.refresh_pending = true;
                }
            }
            Action::NextAnalyticsView => {
                let next = next_analytics_view(self.analytics.view);
                self.analytics.view = next;
                self.analytics.selected_index = 0;
                self.analytics.error = None;
                // Keep tab cycling instant: only refetch if the
                // destination view has no cached data or its cache has
                // gone stale. Manual `r` and filter changes still
                // refresh unconditionally.
                if !self.analytics.has_data_for_view(next) || !self.analytics.cache_is_fresh(next) {
                    self.analytics.refresh_pending = true;
                }
            }
            Action::PrevAnalyticsView => {
                let prev = prev_analytics_view(self.analytics.view);
                self.analytics.view = prev;
                self.analytics.selected_index = 0;
                self.analytics.error = None;
                if !self.analytics.has_data_for_view(prev) || !self.analytics.cache_is_fresh(prev) {
                    self.analytics.refresh_pending = true;
                }
            }
            Action::RefreshAnalytics => {
                self.analytics.error = None;
                self.analytics.refresh_pending = true;
            }
            Action::CycleStorageMode => {
                self.analytics.storage_mode = match self.analytics.storage_mode {
                    StorageMode::Breakdown => StorageMode::LargestMessages,
                    StorageMode::LargestMessages => StorageMode::Breakdown,
                };
                self.analytics.selected_index = 0;
                self.analytics.refresh_pending = true;
            }
            Action::CycleStorageGroupBy => {
                use mxr_core::types::StorageGroupBy;
                self.analytics.storage_group_by = match self.analytics.storage_group_by {
                    StorageGroupBy::Sender => StorageGroupBy::Mimetype,
                    StorageGroupBy::Mimetype => StorageGroupBy::Label,
                    StorageGroupBy::Label => StorageGroupBy::Sender,
                };
                self.analytics.selected_index = 0;
                self.analytics.refresh_pending = true;
            }
            Action::ToggleStalePerspective => {
                use mxr_core::types::StaleBallInCourt;
                self.analytics.stale_perspective = match self.analytics.stale_perspective {
                    StaleBallInCourt::Mine => StaleBallInCourt::Theirs,
                    StaleBallInCourt::Theirs => StaleBallInCourt::Mine,
                };
                self.analytics.selected_index = 0;
                self.analytics.refresh_pending = true;
            }
            Action::AdjustStaleOlderThanDays(delta) => {
                let curr = self.analytics.stale_older_than_days as i64;
                let next = (curr + delta as i64).clamp(1, 3650);
                self.analytics.stale_older_than_days = next as u32;
                self.analytics.refresh_pending = true;
            }
            Action::AdjustStaleWithinDays(delta) => {
                let curr = self.analytics.stale_within_days as i64;
                let next = (curr + delta as i64).clamp(1, 36500);
                self.analytics.stale_within_days = next as u32;
                self.analytics.refresh_pending = true;
            }
            Action::CycleContactsMode => {
                self.analytics.contacts_mode = match self.analytics.contacts_mode {
                    ContactsMode::Asymmetry => ContactsMode::Decay,
                    ContactsMode::Decay => ContactsMode::Asymmetry,
                };
                self.analytics.selected_index = 0;
                self.analytics.refresh_pending = true;
            }
            Action::RefreshContacts => {
                self.analytics.pending_contacts_refresh = true;
            }
            Action::ToggleResponseTimeDirection => {
                use mxr_core::types::ResponseTimeDirection;
                self.analytics.response_time_direction =
                    match self.analytics.response_time_direction {
                        ResponseTimeDirection::IReplied => ResponseTimeDirection::TheyReplied,
                        ResponseTimeDirection::TheyReplied => ResponseTimeDirection::IReplied,
                    };
                self.analytics.refresh_pending = true;
            }
            Action::ToggleSubscriptionsRank => {
                self.analytics.subscriptions_rank = !self.analytics.subscriptions_rank;
            }
            Action::CycleWrappedWindow => {
                let now_year = Utc::now().year();
                self.analytics.wrapped_window = match self.analytics.wrapped_window {
                    WrappedWindow::Ytd => WrappedWindow::Year(now_year),
                    WrappedWindow::Year(_) => WrappedWindow::SinceDays(90),
                    WrappedWindow::SinceDays(d) if d <= 90 => WrappedWindow::SinceDays(365),
                    WrappedWindow::SinceDays(_) => WrappedWindow::Ytd,
                };
                self.analytics.refresh_pending = true;
            }
            Action::StepWrappedYear(delta) => {
                let now_year = Utc::now().year();
                let curr = match self.analytics.wrapped_window {
                    WrappedWindow::Year(y) => y,
                    _ => now_year,
                };
                self.analytics.wrapped_window = WrappedWindow::Year(curr + delta);
                self.analytics.refresh_pending = true;
            }
            Action::AnalyticsRowDrillDown => {
                self.handle_analytics_drill_down();
            }
            Action::AnalyticsUnsubscribe => {
                self.handle_analytics_unsubscribe();
            }
            Action::OpenAnalyticsFilterModal => {
                self.modals.analytics_filter = Some(filter_modal_for_view(&self.analytics));
            }
            Action::CloseAnalyticsFilterModal => {
                self.modals.analytics_filter = None;
            }
            Action::SubmitAnalyticsFilterModal => {
                self.submit_analytics_filter_modal();
            }
            _ => unreachable!("action routed to wrong handler"),
        }
    }

    /// Slice 11: Enter on a row routes to a context-appropriate
    /// destination — search by sender, open a thread/message, switch
    /// to a label. No-ops on tiles/views without a sensible
    /// destination (Volume, ResponseTime summary). The drill-down
    /// targets are deliberately read-only navigations; mutations live
    /// behind dedicated keys (`u` for unsubscribe).
    fn handle_analytics_drill_down(&mut self) {
        use crate::app::AnalyticsView;
        match self.analytics.view {
            AnalyticsView::Storage => match self.analytics.storage_mode {
                StorageMode::Breakdown => {
                    let Some(row) = self
                        .analytics
                        .storage_rows
                        .get(self.analytics.selected_index)
                    else {
                        return;
                    };
                    use mxr_core::types::StorageGroupBy;
                    // The lexical engine doesn't index mime types, so
                    // the mime drill-down falls back to a generic
                    // `has:attachment` jump and surfaces the chosen
                    // mime as a status hint. Sender + label both map
                    // to real query operators.
                    let (query, hint) = match self.analytics.storage_group_by {
                        StorageGroupBy::Sender => (format!("from:{}", row.key), None),
                        StorageGroupBy::Mimetype => (
                            "has:attachment".to_string(),
                            Some(format!(
                                "Showing all attachments (mime drill-down for {} not indexed)",
                                row.key
                            )),
                        ),
                        StorageGroupBy::Label => (format!("label:{}", row.key), None),
                    };
                    self.jump_to_search(query);
                    if let Some(hint) = hint {
                        self.status_message = Some(hint);
                    }
                }
                StorageMode::LargestMessages => {
                    let Some(row) = self
                        .analytics
                        .largest_message_rows
                        .get(self.analytics.selected_index)
                    else {
                        return;
                    };
                    // Jump to the sender's mail (same pattern as
                    // Storage / Sender). Direct envelope-open left the
                    // mailbox list out of sync with the preview pane.
                    self.jump_to_search(format!("from:{}", row.from_email));
                }
            },
            AnalyticsView::StaleThreads => {
                let Some(row) = self.analytics.stale_rows.get(self.analytics.selected_index) else {
                    return;
                };
                // Jump to a counterparty-scoped search (same pattern as
                // Contacts drill). Opening the envelope directly left the
                // mailbox list out of sync with the preview pane.
                self.jump_to_search(format!("from:{}", row.counterparty_email));
            }
            AnalyticsView::Contacts => {
                let email = match self.analytics.contacts_mode {
                    ContactsMode::Asymmetry => self
                        .analytics
                        .asymmetry_rows
                        .get(self.analytics.selected_index)
                        .map(|r| r.email.clone()),
                    ContactsMode::Decay => self
                        .analytics
                        .decay_rows
                        .get(self.analytics.selected_index)
                        .map(|r| r.email.clone()),
                };
                if let Some(email) = email {
                    self.jump_to_search(format!("from:{email}"));
                }
            }
            AnalyticsView::CadenceDrift => {
                if let Some(row) = self
                    .analytics
                    .cadence_drift_rows
                    .get(self.analytics.selected_index)
                {
                    self.jump_to_search(format!("from:{}", row.email));
                }
            }
            AnalyticsView::Subscriptions => {
                let Some(row) = self
                    .analytics
                    .subscriptions
                    .get(self.analytics.selected_index)
                else {
                    return;
                };
                // Jump to the sender's mail. Direct envelope-open left
                // the mailbox list out of sync with the preview pane.
                self.jump_to_search(format!("from:{}", row.sender_email));
            }
            AnalyticsView::ResponseTime => {
                // ResponseTime is a summary view; no per-row drill.
            }
            AnalyticsView::Wrapped => {
                self.handle_wrapped_tile_drill_down();
            }
        }
    }

    fn handle_wrapped_tile_drill_down(&mut self) {
        let Some(summary) = self.analytics.wrapped.as_ref() else {
            return;
        };
        // Tile order: 0=Volume, 1=When, 2=Contacts, 3=Reply,
        // 4=Storage, 5=Newsletters. The legacy "Superlatives" strip
        // (tile 6) was folded into Volume + Contacts; the
        // most-ghosted search now triggers from tile 2 (Contacts).
        let tile = self.analytics.wrapped_selected_tile;
        match tile {
            2 => {
                // "Most ghosted" gives an email but no message ID, so
                // a from:<email> search is the most precise surface —
                // the lexical engine indexes from-address.
                if let Some(g) = summary.superlatives.most_ghosted.as_ref() {
                    let email = g.email.clone();
                    self.jump_to_search(format!("from:{email}"));
                }
            }
            4 => {
                // Heaviest message: jump to its sender's mail. Direct
                // envelope-open left the centre mailbox list out of
                // sync with the preview pane (same fix as the other
                // analytics drills).
                if let Some(heaviest) = summary.storage.heaviest_message.as_ref() {
                    self.jump_to_search(format!("from:{}", heaviest.from_email));
                }
            }
            _ => {}
        }
    }

    /// Slice 6: 'u' on a Subscriptions row populates the existing
    /// unsubscribe-confirm modal with the row's metadata. The
    /// downstream confirm action (`ConfirmUnsubscribeOnly` /
    /// `ConfirmUnsubscribeAndArchiveSender`) is the same path the
    /// mailbox uses, so the IPC + side-effects are shared.
    fn handle_analytics_unsubscribe(&mut self) {
        use crate::app::PendingUnsubscribeConfirm;
        use mxr_core::types::UnsubscribeMethod;
        let Some(row) = self
            .analytics
            .subscriptions
            .get(self.analytics.selected_index)
        else {
            return;
        };
        let method_label = match &row.unsubscribe {
            UnsubscribeMethod::OneClick { url } => format!("one-click → {url}"),
            UnsubscribeMethod::HttpLink { url } => format!("link → {url}"),
            UnsubscribeMethod::Mailto { address, .. } => format!("mailto: {address}"),
            UnsubscribeMethod::BodyLink { url } => format!("body link ({url})"),
            UnsubscribeMethod::None => {
                self.report_warn("This sender has no unsubscribe method.");
                return;
            }
        };
        self.modals.pending_unsubscribe_confirm = Some(PendingUnsubscribeConfirm {
            message_id: row.latest_message_id.clone(),
            account_id: row.account_id.clone(),
            sender_email: row.sender_email.clone(),
            method_label,
            archive_message_ids: Vec::new(),
        });
    }

    fn submit_analytics_filter_modal(&mut self) {
        let Some(modal) = self.modals.analytics_filter.as_ref() else {
            return;
        };
        let view = modal.view;
        let fields = modal.fields.clone();
        // Each view's modal has a known field order. Parse and write
        // back; on failure, surface a validation error and leave the
        // modal open.
        let result = apply_filter_modal_fields(&mut self.analytics, view, &fields);
        match result {
            Ok(()) => {
                self.modals.analytics_filter = None;
                self.analytics.refresh_pending = true;
            }
            Err(err) => {
                if let Some(modal) = self.modals.analytics_filter.as_mut() {
                    modal.validation_error = Some(err);
                }
            }
        }
    }

    fn jump_to_search(&mut self, query: String) {
        // Replicate Action::OpenGlobalSearch but with a preset query.
        self.maybe_preserve_new_account_form_draft();
        self.mailbox.pending_preview_read = None;
        self.search.bar.deactivate();
        self.screen = Screen::Search;
        self.reset_search_page_workspace();
        self.search.page.query = query;
        self.search.page.editing = false;
        self.search.page.active_pane = crate::app::SearchPane::Results;
        // Trigger the search.
        self.apply(Action::SubmitSearch);
    }

    /// Build the IPC request for the active analytics view. Storage
    /// and Contacts dispatch on their respective sub-mode so a single
    /// `view` value can fire either of two daemon requests. Wrapped
    /// converts its window state into the same `(since_unix,
    /// until_unix, label)` tuple the CLI computes.
    pub(crate) fn analytics_request_for_active_view(&self) -> Option<mxr_protocol::Request> {
        match self.analytics.view {
            AnalyticsView::Storage => match self.analytics.storage_mode {
                StorageMode::Breakdown => Some(mxr_protocol::Request::ListStorageBreakdown {
                    account_id: None,
                    group_by: self.analytics.storage_group_by,
                    limit: 100,
                }),
                StorageMode::LargestMessages => Some(mxr_protocol::Request::ListLargestMessages {
                    account_id: None,
                    since_days: self.analytics.largest_since_days,
                    limit: self.analytics.largest_limit,
                }),
            },
            AnalyticsView::StaleThreads => Some(mxr_protocol::Request::ListStaleThreads {
                account_id: None,
                perspective: self.analytics.stale_perspective,
                older_than_days: self.analytics.stale_older_than_days,
                within_days: self.analytics.stale_within_days,
                limit: 100,
            }),
            AnalyticsView::Contacts => match self.analytics.contacts_mode {
                ContactsMode::Asymmetry => Some(mxr_protocol::Request::ListContactAsymmetry {
                    account_id: None,
                    min_inbound: self.analytics.asymmetry_min_inbound,
                    limit: 100,
                }),
                ContactsMode::Decay => Some(mxr_protocol::Request::ListContactDecay {
                    account_id: None,
                    threshold_days: self.analytics.decay_threshold_days,
                    max_lookback_days: self.analytics.decay_max_lookback_days,
                    limit: 100,
                }),
            },
            AnalyticsView::CadenceDrift => self
                .default_account_id()
                .cloned()
                .map(|account_id| mxr_protocol::Request::ListCadenceDrift { account_id }),
            AnalyticsView::ResponseTime => Some(mxr_protocol::Request::ListResponseTime {
                account_id: None,
                direction: self.analytics.response_time_direction,
                counterparty: self.analytics.response_time_counterparty.clone(),
                since_days: self.analytics.response_time_since_days,
            }),
            AnalyticsView::Subscriptions => Some(mxr_protocol::Request::ListSubscriptions {
                account_id: None,
                limit: self.analytics.subscriptions_limit,
            }),
            AnalyticsView::Wrapped => {
                let (since_unix, until_unix, label) =
                    wrapped_window_to_request(self.analytics.wrapped_window);
                Some(mxr_protocol::Request::Wrapped {
                    account_id: None,
                    since_unix,
                    until_unix,
                    label,
                })
            }
        }
    }
}

fn filter_modal_for_view(state: &AnalyticsState) -> AnalyticsFilterModalState {
    use crate::app::AnalyticsView;
    let fields = match state.view {
        AnalyticsView::Storage => match state.storage_mode {
            StorageMode::Breakdown => vec![select_filter_field(
                "group_by",
                match state.storage_group_by {
                    mxr_core::types::StorageGroupBy::Sender => "sender".into(),
                    mxr_core::types::StorageGroupBy::Mimetype => "mimetype".into(),
                    mxr_core::types::StorageGroupBy::Label => "label".into(),
                },
                &["sender", "mimetype", "label"],
            )],
            StorageMode::LargestMessages => vec![
                text_filter_field("limit", state.largest_limit.to_string()),
                text_filter_field(
                    "since_days (blank = all)",
                    state
                        .largest_since_days
                        .map(|d| d.to_string())
                        .unwrap_or_default(),
                ),
            ],
        },
        AnalyticsView::StaleThreads => vec![
            select_filter_field(
                "perspective",
                match state.stale_perspective {
                    mxr_core::types::StaleBallInCourt::Mine => "mine".into(),
                    mxr_core::types::StaleBallInCourt::Theirs => "theirs".into(),
                },
                &["mine", "theirs"],
            ),
            text_filter_field("older_than_days", state.stale_older_than_days.to_string()),
            text_filter_field("within_days", state.stale_within_days.to_string()),
        ],
        AnalyticsView::Contacts => match state.contacts_mode {
            ContactsMode::Asymmetry => vec![text_filter_field(
                "min_inbound",
                state.asymmetry_min_inbound.to_string(),
            )],
            ContactsMode::Decay => vec![
                text_filter_field("threshold_days", state.decay_threshold_days.to_string()),
                text_filter_field(
                    "max_lookback_days",
                    state.decay_max_lookback_days.to_string(),
                ),
            ],
        },
        AnalyticsView::CadenceDrift => Vec::new(),
        AnalyticsView::ResponseTime => vec![
            select_filter_field(
                "direction",
                match state.response_time_direction {
                    mxr_core::types::ResponseTimeDirection::IReplied => "i_replied".into(),
                    mxr_core::types::ResponseTimeDirection::TheyReplied => "they_replied".into(),
                },
                &["i_replied", "they_replied"],
            ),
            text_filter_field(
                "counterparty (blank = all)",
                state.response_time_counterparty.clone().unwrap_or_default(),
            ),
            text_filter_field(
                "since_days (blank = all)",
                state
                    .response_time_since_days
                    .map(|d| d.to_string())
                    .unwrap_or_default(),
            ),
        ],
        AnalyticsView::Subscriptions => vec![
            text_filter_field("limit", state.subscriptions_limit.to_string()),
            select_filter_field(
                "rank",
                state.subscriptions_rank.to_string(),
                &["true", "false"],
            ),
        ],
        AnalyticsView::Wrapped => vec![
            select_filter_field(
                "window",
                match state.wrapped_window {
                    WrappedWindow::Ytd => "ytd".into(),
                    WrappedWindow::Year(_) => "year".into(),
                    WrappedWindow::SinceDays(_) => "since_days".into(),
                },
                &["ytd", "year", "since_days"],
            ),
            text_filter_field(
                "year (used when window=year)",
                match state.wrapped_window {
                    WrappedWindow::Year(y) => y.to_string(),
                    _ => Utc::now().year().to_string(),
                },
            ),
            text_filter_field(
                "days (used when window=since_days)",
                match state.wrapped_window {
                    WrappedWindow::SinceDays(d) => d.to_string(),
                    _ => "90".into(),
                },
            ),
        ],
    };
    AnalyticsFilterModalState {
        view: state.view,
        active_field: 0,
        fields,
        validation_error: None,
    }
}

fn text_filter_field(label: &str, value: String) -> AnalyticsFilterField {
    AnalyticsFilterField {
        label: label.into(),
        value,
        options: Vec::new(),
    }
}

fn select_filter_field(label: &str, value: String, options: &[&str]) -> AnalyticsFilterField {
    AnalyticsFilterField {
        label: label.into(),
        value,
        options: options.iter().map(|option| (*option).to_string()).collect(),
    }
}

fn apply_filter_modal_fields(
    state: &mut AnalyticsState,
    view: crate::app::AnalyticsView,
    fields: &[AnalyticsFilterField],
) -> Result<(), String> {
    use crate::app::AnalyticsView;
    let get = |idx: usize| -> &str { fields.get(idx).map_or("", |f| f.value.as_str()) };
    match view {
        AnalyticsView::Storage => match state.storage_mode {
            StorageMode::Breakdown => {
                use mxr_core::types::StorageGroupBy;
                state.storage_group_by = match get(0).trim().to_ascii_lowercase().as_str() {
                    "sender" => StorageGroupBy::Sender,
                    "mimetype" => StorageGroupBy::Mimetype,
                    "label" => StorageGroupBy::Label,
                    other => return Err(format!("invalid group_by: {other}")),
                };
            }
            StorageMode::LargestMessages => {
                state.largest_limit = parse_u32(get(0), "limit")?;
                state.largest_since_days = parse_optional_u32(get(1), "since_days")?;
            }
        },
        AnalyticsView::StaleThreads => {
            use mxr_core::types::StaleBallInCourt;
            state.stale_perspective = match get(0).trim().to_ascii_lowercase().as_str() {
                "mine" => StaleBallInCourt::Mine,
                "theirs" => StaleBallInCourt::Theirs,
                other => return Err(format!("invalid perspective: {other}")),
            };
            state.stale_older_than_days = parse_u32(get(1), "older_than_days")?;
            state.stale_within_days = parse_u32(get(2), "within_days")?;
        }
        AnalyticsView::Contacts => match state.contacts_mode {
            ContactsMode::Asymmetry => {
                state.asymmetry_min_inbound = parse_u32(get(0), "min_inbound")?;
            }
            ContactsMode::Decay => {
                state.decay_threshold_days = parse_u32(get(0), "threshold_days")?;
                state.decay_max_lookback_days = parse_u32(get(1), "max_lookback_days")?;
            }
        },
        AnalyticsView::CadenceDrift => {}
        AnalyticsView::ResponseTime => {
            use mxr_core::types::ResponseTimeDirection;
            state.response_time_direction = match get(0).trim().to_ascii_lowercase().as_str() {
                "i_replied" => ResponseTimeDirection::IReplied,
                "they_replied" => ResponseTimeDirection::TheyReplied,
                other => return Err(format!("invalid direction: {other}")),
            };
            let cp = get(1).trim();
            state.response_time_counterparty = if cp.is_empty() { None } else { Some(cp.into()) };
            state.response_time_since_days = parse_optional_u32(get(2), "since_days")?;
        }
        AnalyticsView::Subscriptions => {
            state.subscriptions_limit = parse_u32(get(0), "limit")?;
            state.subscriptions_rank = match get(1).trim().to_ascii_lowercase().as_str() {
                "true" => true,
                "false" => false,
                other => return Err(format!("invalid rank: {other}")),
            };
        }
        AnalyticsView::Wrapped => {
            let kind = get(0).trim().to_ascii_lowercase();
            let year = parse_i32(get(1), "year")?;
            let days = parse_u32(get(2), "days")?;
            state.wrapped_window = match kind.as_str() {
                "ytd" => WrappedWindow::Ytd,
                "year" => WrappedWindow::Year(year),
                "since_days" => WrappedWindow::SinceDays(days),
                other => return Err(format!("invalid window: {other}")),
            };
        }
    }
    Ok(())
}

fn parse_u32(s: &str, name: &str) -> Result<u32, String> {
    s.trim()
        .parse::<u32>()
        .map_err(|_| format!("{name} must be a non-negative integer"))
}

fn parse_optional_u32(s: &str, name: &str) -> Result<Option<u32>, String> {
    let s = s.trim();
    if s.is_empty() {
        Ok(None)
    } else {
        s.parse::<u32>()
            .map(Some)
            .map_err(|_| format!("{name} must be a non-negative integer"))
    }
}

fn parse_i32(s: &str, name: &str) -> Result<i32, String> {
    s.trim()
        .parse::<i32>()
        .map_err(|_| format!("{name} must be an integer"))
}

fn next_analytics_view(view: AnalyticsView) -> AnalyticsView {
    match view {
        AnalyticsView::Storage => AnalyticsView::StaleThreads,
        AnalyticsView::StaleThreads => AnalyticsView::Contacts,
        AnalyticsView::Contacts => AnalyticsView::CadenceDrift,
        AnalyticsView::CadenceDrift => AnalyticsView::ResponseTime,
        AnalyticsView::ResponseTime => AnalyticsView::Subscriptions,
        AnalyticsView::Subscriptions => AnalyticsView::Wrapped,
        AnalyticsView::Wrapped => AnalyticsView::Storage,
    }
}

fn prev_analytics_view(view: AnalyticsView) -> AnalyticsView {
    match view {
        AnalyticsView::Storage => AnalyticsView::Wrapped,
        AnalyticsView::StaleThreads => AnalyticsView::Storage,
        AnalyticsView::Contacts => AnalyticsView::StaleThreads,
        AnalyticsView::CadenceDrift => AnalyticsView::Contacts,
        AnalyticsView::ResponseTime => AnalyticsView::CadenceDrift,
        AnalyticsView::Subscriptions => AnalyticsView::ResponseTime,
        AnalyticsView::Wrapped => AnalyticsView::Subscriptions,
    }
}

/// Mirrors `crates/daemon/src/commands/wrapped.rs` window logic so
/// the TUI request matches the CLI byte-for-byte (label included).
pub(crate) fn wrapped_window_to_request(window: WrappedWindow) -> (i64, i64, String) {
    let now = Utc::now();
    match window {
        WrappedWindow::Year(y) => {
            let start = Utc
                .with_ymd_and_hms(y, 1, 1, 0, 0, 0)
                .single()
                .unwrap_or(now);
            let end = Utc
                .with_ymd_and_hms(y, 12, 31, 23, 59, 59)
                .single()
                .unwrap_or(now);
            (start.timestamp(), end.timestamp(), format!("{y}"))
        }
        WrappedWindow::SinceDays(d) => {
            let start = now - chrono::Duration::days(d as i64);
            (start.timestamp(), now.timestamp(), format!("last {d} days"))
        }
        WrappedWindow::Ytd => {
            let start = Utc
                .with_ymd_and_hms(now.year(), 1, 1, 0, 0, 0)
                .single()
                .unwrap_or(now);
            (
                start.timestamp(),
                now.timestamp(),
                format!("{} year-to-date", now.year()),
            )
        }
    }
}
