use super::*;

impl App {
    pub(super) fn bump_search_session_id(current: &mut u64) -> u64 {
        *current = current.saturating_add(1).max(1);
        *current
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "queued search state stays explicit at call sites"
    )]
    pub(super) fn queue_search_request(
        &mut self,
        target: SearchTarget,
        append: bool,
        query: String,
        mode: SearchMode,
        sort: SortOrder,
        offset: u32,
        session_id: u64,
    ) {
        self.pending_search = Some(PendingSearchRequest {
            query,
            mode,
            sort,
            limit: SEARCH_PAGE_SIZE,
            offset,
            target,
            append,
            session_id,
        });
    }

    pub(super) fn queue_search_count_request(
        &mut self,
        query: String,
        mode: SearchMode,
        session_id: u64,
    ) {
        self.pending_search_count = Some(PendingSearchCountRequest {
            query,
            mode,
            session_id,
        });
    }

    pub(super) fn reset_search_page_workspace(&mut self) {
        Self::bump_search_session_id(&mut self.search_page.session_id);
        self.search_page.query.clear();
        self.search_page.results.clear();
        self.search_page.scores.clear();
        self.search_page.has_more = false;
        self.search_page.loading_more = false;
        self.search_page.total_count = None;
        self.search_page.count_pending = false;
        self.search_page.ui_status = SearchUiStatus::Idle;
        self.search_page.load_to_end = false;
        self.search_page.session_active = false;
        self.search_page.active_pane = SearchPane::Results;
        self.search_page.selected_index = 0;
        self.search_page.scroll_offset = 0;
        self.search_page.result_selected = false;
        self.search_page.throbber = ThrobberState::default();
        self.pending_search = None;
        self.pending_search_count = None;
        self.pending_search_debounce = None;
        self.clear_message_view_state();
    }

    pub(super) fn begin_search_page_request(&mut self, status: SearchUiStatus) -> u64 {
        self.search_page.results.clear();
        self.search_page.scores.clear();
        self.search_page.has_more = false;
        self.search_page.loading_more = matches!(
            status,
            SearchUiStatus::Searching | SearchUiStatus::LoadingMore
        );
        self.search_page.total_count = None;
        self.search_page.count_pending = matches!(status, SearchUiStatus::Searching);
        self.search_page.ui_status = status;
        self.search_page.load_to_end = false;
        self.search_page.session_active = !self.search_page.query.trim().is_empty();
        self.search_page.active_pane = SearchPane::Results;
        self.search_page.selected_index = 0;
        self.search_page.scroll_offset = 0;
        self.search_page.result_selected = false;
        self.search_page.throbber = ThrobberState::default();
        self.pending_search = None;
        self.pending_search_count = None;
        self.clear_message_view_state();
        Self::bump_search_session_id(&mut self.search_page.session_id)
    }

    pub(super) fn schedule_search_page_search(&mut self) {
        self.search_bar.query = self.search_page.query.clone();
        self.search_bar.mode = self.search_page.mode;
        self.search_page.sort = SortOrder::DateDesc;
        let query = self.search_page.query.trim().to_string();
        if query.is_empty() {
            self.reset_search_page_workspace();
            return;
        }

        let session_id = self.begin_search_page_request(SearchUiStatus::Debouncing);
        self.pending_search_debounce = Some(PendingSearchDebounce {
            query,
            mode: self.search_page.mode,
            session_id,
            due_at: Instant::now() + SEARCH_DEBOUNCE_DELAY,
        });
    }

    pub fn execute_search_page_search(&mut self) {
        self.search_bar.query = self.search_page.query.clone();
        self.search_bar.mode = self.search_page.mode;
        self.search_page.sort = SortOrder::DateDesc;
        let query = self.search_page.query.trim().to_string();
        self.pending_search_debounce = None;
        if query.is_empty() {
            self.reset_search_page_workspace();
            return;
        }

        let session_id = self.begin_search_page_request(SearchUiStatus::Searching);
        self.queue_search_request(
            SearchTarget::SearchPage,
            false,
            query.clone(),
            self.search_page.mode,
            self.search_page.sort.clone(),
            0,
            session_id,
        );
        self.queue_search_count_request(query, self.search_page.mode, session_id);
    }

    pub(super) fn process_pending_search_debounce(&mut self) {
        let Some(pending) = self.pending_search_debounce.clone() else {
            return;
        };
        if pending.due_at > Instant::now() || pending.session_id != self.search_page.session_id {
            return;
        }

        self.pending_search_debounce = None;
        self.search_page.loading_more = true;
        self.search_page.count_pending = true;
        self.search_page.ui_status = SearchUiStatus::Searching;
        self.queue_search_request(
            SearchTarget::SearchPage,
            false,
            pending.query.clone(),
            pending.mode,
            self.search_page.sort.clone(),
            0,
            pending.session_id,
        );
        self.queue_search_count_request(pending.query, pending.mode, pending.session_id);
    }

    pub fn maybe_load_more_search_results(&mut self) {
        if self.screen != Screen::Search || self.search_page.active_pane != SearchPane::Results {
            return;
        }
        let row_count = self.search_row_count();
        if row_count == 0 || !self.search_page.has_more || self.search_page.loading_more {
            return;
        }
        if self.search_page.selected_index.saturating_add(3) >= row_count {
            self.load_more_search_results();
        }
    }

    /// Live filter: instant client-side prefix matching on subject/from/snippet,
    /// plus async Tantivy search for full-text body matches.
    pub(super) fn trigger_live_search(&mut self) {
        if self.screen == Screen::Search {
            self.schedule_search_page_search();
            return;
        }

        let query_source = self.search_bar.query.clone();
        self.search_bar.query.clone_from(&query_source);
        self.search_page.mode = self.search_bar.mode;
        self.search_page.sort = SortOrder::DateDesc;
        let query = query_source.to_lowercase();
        if query.is_empty() {
            Self::bump_search_session_id(&mut self.mailbox_search_session_id);
            self.envelopes = self.all_mail_envelopes();
            self.search_active = false;
        } else {
            let query_words: Vec<&str> = query.split_whitespace().collect();
            // Instant client-side filter: every query word must prefix-match
            // some word in subject, from, or snippet
            let filtered: Vec<Envelope> = self
                .all_envelopes
                .iter()
                .filter(|e| !e.flags.contains(MessageFlags::TRASH))
                .filter(|e| {
                    let haystack = format!(
                        "{} {} {} {}",
                        e.subject,
                        e.from.email,
                        e.from.name.as_deref().unwrap_or(""),
                        e.snippet
                    )
                    .to_lowercase();
                    query_words.iter().all(|qw| {
                        haystack.split_whitespace().any(|hw| hw.starts_with(qw))
                            || haystack.contains(qw)
                    })
                })
                .cloned()
                .collect();
            let mut filtered = filtered;
            filtered.sort_by(|left, right| {
                sane_mail_sort_timestamp(&right.date)
                    .cmp(&sane_mail_sort_timestamp(&left.date))
                    .then_with(|| right.id.as_str().cmp(&left.id.as_str()))
            });
            self.envelopes = filtered;
            self.search_active = true;
            let session_id = Self::bump_search_session_id(&mut self.mailbox_search_session_id);
            self.queue_search_request(
                SearchTarget::Mailbox,
                false,
                query_source,
                self.search_bar.mode,
                SortOrder::DateDesc,
                0,
                session_id,
            );
        }
        self.selected_index = 0;
        self.scroll_offset = 0;
    }

    pub fn search_is_pending(&self) -> bool {
        matches!(
            self.search_page.ui_status,
            SearchUiStatus::Debouncing | SearchUiStatus::Searching | SearchUiStatus::LoadingMore
        )
    }

    pub fn open_selected_search_result(&mut self) {
        if let Some(env) = self.selected_search_envelope().cloned() {
            self.search_page.result_selected = true;
            self.open_envelope(env);
            self.search_page.active_pane = SearchPane::Preview;
        } else {
            self.reset_search_preview_selection();
        }
    }

    pub fn maybe_open_search_preview(&mut self) {
        if self.search_page.result_selected {
            self.search_page.active_pane = SearchPane::Preview;
        } else {
            self.open_selected_search_result();
        }
    }

    pub fn reset_search_preview_selection(&mut self) {
        self.search_page.result_selected = false;
        self.search_page.active_pane = SearchPane::Results;
        self.clear_message_view_state();
    }
}
