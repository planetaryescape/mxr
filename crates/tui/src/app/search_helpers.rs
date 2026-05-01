use super::*;

impl App {
    pub(crate) fn apply_search_page_results(&mut self, append: bool, results: SearchResultData) {
        let SearchResultData {
            envelopes,
            scores,
            has_more,
        } = results;
        let selected_row_message_id = (!append && self.search.page.result_selected)
            .then(|| self.selected_search_envelope().map(|env| env.id.clone()))
            .flatten();

        if append {
            self.search.page.results.extend(envelopes);
            self.search.page.scores.extend(scores);
        } else {
            self.search.page.results = envelopes;
            self.search.page.scores = scores;
            self.search.page.selected_index = 0;
            self.search.page.scroll_offset = 0;

            if let Some(message_id) = selected_row_message_id {
                if let Some(index) = self.search_row_index_for_message(&message_id) {
                    self.search.page.selected_index = index;
                } else {
                    self.reset_search_preview_selection();
                }
            }
        }

        self.search.page.has_more = has_more;
        self.search.page.loading_more = false;
        self.search.page.ui_status = SearchUiStatus::Loaded;
        self.search.page.session_active =
            !self.search.page.query.is_empty() || !self.search.page.results.is_empty();

        if self.search.page.load_to_end {
            if self.search.page.has_more {
                self.load_more_search_results();
            } else {
                self.search.page.load_to_end = false;
                if self.search_row_count() > 0 {
                    self.search.page.selected_index = self.search_row_count() - 1;
                    self.sync_search_cursor_after_move();
                } else {
                    self.clear_message_view_state();
                }
            }
            return;
        }

        if self.screen == Screen::Search {
            if self.search.page.result_selected {
                self.sync_search_cursor_after_move();
            } else if self.search_row_count() > 0 {
                self.ensure_search_visible();
            } else {
                self.clear_message_view_state();
            }
        }
    }

    pub(crate) fn search_row_count(&self) -> usize {
        self.search_mail_list_rows().len()
    }

    pub(super) fn search_list_mode(&self) -> MailListMode {
        self.mailbox.mail_list_mode
    }

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
        self.search.pending = Some(PendingSearchRequest {
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
        self.search.pending_count = Some(PendingSearchCountRequest {
            query,
            mode,
            session_id,
        });
    }

    pub(super) fn reset_search_page_workspace(&mut self) {
        Self::bump_search_session_id(&mut self.search.page.session_id);
        self.search.page.query.clear();
        self.search.page.results.clear();
        self.search.page.scores.clear();
        self.search.page.has_more = false;
        self.search.page.loading_more = false;
        self.search.page.total_count = None;
        self.search.page.count_pending = false;
        self.search.page.ui_status = SearchUiStatus::Idle;
        self.search.page.load_to_end = false;
        self.search.page.session_active = false;
        self.search.page.active_pane = SearchPane::Results;
        self.search.page.preview_fullscreen = false;
        self.search.page.selected_index = 0;
        self.search.page.scroll_offset = 0;
        self.search.page.result_selected = false;
        self.search.page.throbber = ThrobberState::default();
        self.search.pending = None;
        self.search.pending_count = None;
        self.search.pending_debounce = None;
        self.clear_message_view_state();
    }

    pub(super) fn begin_search_page_request(&mut self, status: SearchUiStatus) -> u64 {
        self.search.page.results.clear();
        self.search.page.scores.clear();
        self.search.page.has_more = false;
        self.search.page.loading_more = matches!(
            status,
            SearchUiStatus::Searching | SearchUiStatus::LoadingMore
        );
        self.search.page.total_count = None;
        self.search.page.count_pending = matches!(status, SearchUiStatus::Searching);
        self.search.page.ui_status = status;
        self.search.page.load_to_end = false;
        self.search.page.session_active = !self.search.page.query.trim().is_empty();
        self.search.page.active_pane = SearchPane::Results;
        self.search.page.preview_fullscreen = false;
        self.search.page.selected_index = 0;
        self.search.page.scroll_offset = 0;
        self.search.page.result_selected = false;
        self.search.page.throbber = ThrobberState::default();
        self.search.pending = None;
        self.search.pending_count = None;
        self.clear_message_view_state();
        Self::bump_search_session_id(&mut self.search.page.session_id)
    }

    pub(super) fn schedule_search_page_search(&mut self) {
        self.search.bar.query = self.search.page.query.clone();
        self.search.bar.mode = self.search.page.mode;
        self.search.page.sort = SortOrder::DateDesc;
        let query = self.search.page.query.trim().to_string();
        if query.is_empty() {
            self.reset_search_page_workspace();
            return;
        }

        let session_id = self.begin_search_page_request(SearchUiStatus::Debouncing);
        self.search.pending_debounce = Some(PendingSearchDebounce {
            query,
            mode: self.search.page.mode,
            session_id,
            due_at: Instant::now() + SEARCH_DEBOUNCE_DELAY,
        });
    }

    pub fn execute_search_page_search(&mut self) {
        self.search.bar.query = self.search.page.query.clone();
        self.search.bar.mode = self.search.page.mode;
        self.search.page.sort = SortOrder::DateDesc;
        let query = self.search.page.query.trim().to_string();
        self.search.pending_debounce = None;
        if query.is_empty() {
            self.reset_search_page_workspace();
            return;
        }

        let session_id = self.begin_search_page_request(SearchUiStatus::Searching);
        self.queue_search_request(
            SearchTarget::SearchPage,
            false,
            query.clone(),
            self.search.page.mode,
            self.search.page.sort.clone(),
            0,
            session_id,
        );
        self.queue_search_count_request(query, self.search.page.mode, session_id);
    }

    pub(super) fn process_pending_search_debounce(&mut self) {
        let Some(pending) = self.search.pending_debounce.clone() else {
            return;
        };
        if pending.due_at > Instant::now() || pending.session_id != self.search.page.session_id {
            return;
        }

        self.search.pending_debounce = None;
        self.search.page.loading_more = true;
        self.search.page.count_pending = true;
        self.search.page.ui_status = SearchUiStatus::Searching;
        self.queue_search_request(
            SearchTarget::SearchPage,
            false,
            pending.query.clone(),
            pending.mode,
            self.search.page.sort.clone(),
            0,
            pending.session_id,
        );
        self.queue_search_count_request(pending.query, pending.mode, pending.session_id);
    }

    pub(crate) fn load_more_search_results(&mut self) {
        if self.search.page.loading_more
            || !self.search.page.has_more
            || self.search.page.query.is_empty()
        {
            return;
        }
        self.search.page.loading_more = true;
        self.search.page.ui_status = SearchUiStatus::LoadingMore;
        self.queue_search_request(
            SearchTarget::SearchPage,
            true,
            self.search.page.query.clone(),
            self.search.page.mode,
            self.search.page.sort.clone(),
            self.search.page.results.len() as u32,
            self.search.page.session_id,
        );
    }

    pub fn maybe_load_more_search_results(&mut self) {
        if self.screen != Screen::Search || self.search.page.active_pane != SearchPane::Results {
            return;
        }
        let row_count = self.search_row_count();
        if row_count == 0 || !self.search.page.has_more || self.search.page.loading_more {
            return;
        }
        if self.search.page.selected_index.saturating_add(3) >= row_count {
            self.load_more_search_results();
        }
    }

    pub(super) fn trigger_live_search(&mut self) {
        if self.screen == Screen::Search {
            self.schedule_search_page_search();
            return;
        }

        let query_source = self.search.bar.query.clone();
        self.search.bar.query = query_source.clone();
        self.search.page.mode = self.search.bar.mode;
        self.search.page.sort = SortOrder::DateDesc;
        let query = query_source.to_lowercase();
        if query.is_empty() {
            Self::bump_search_session_id(&mut self.search.mailbox_session_id);
            self.mailbox.envelopes = self.all_mail_envelopes();
            self.search.active = false;
        } else {
            let query_words: Vec<&str> = query.split_whitespace().collect();
            // Instant client-side filter: every query word must prefix-match
            // some word in subject, from, or snippet
            let filtered: Vec<Envelope> = self
                .mailbox
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
            self.mailbox.envelopes = filtered;
            self.search.active = true;
            let session_id = Self::bump_search_session_id(&mut self.search.mailbox_session_id);
            self.queue_search_request(
                SearchTarget::Mailbox,
                false,
                query_source,
                self.search.bar.mode,
                SortOrder::DateDesc,
                0,
                session_id,
            );
        }
        self.mailbox.selected_index = 0;
        self.mailbox.scroll_offset = 0;
    }

    pub fn search_is_pending(&self) -> bool {
        matches!(
            self.search.page.ui_status,
            SearchUiStatus::Debouncing | SearchUiStatus::Searching | SearchUiStatus::LoadingMore
        )
    }

    pub fn open_selected_search_result(&mut self) {
        if let Some(env) = self.selected_search_envelope().cloned() {
            self.search.page.result_selected = true;
            self.open_envelope(env);
            self.search.page.active_pane = SearchPane::Preview;
        } else {
            self.reset_search_preview_selection();
        }
    }

    pub fn maybe_open_search_preview(&mut self) {
        if self.search.page.result_selected {
            self.search.page.active_pane = SearchPane::Preview;
        } else {
            self.open_selected_search_result();
        }
    }

    pub fn reset_search_preview_selection(&mut self) {
        self.search.page.result_selected = false;
        self.search.page.active_pane = SearchPane::Results;
        self.search.page.preview_fullscreen = false;
        self.clear_message_view_state();
    }

    pub fn auto_preview_search(&mut self) {
        if !self.search.page.result_selected {
            if self.screen == Screen::Search {
                self.clear_message_view_state();
            }
            return;
        }
        if let Some(env) = self.selected_search_envelope().cloned() {
            if self
                .mailbox
                .viewing_envelope
                .as_ref()
                .map(|current| current.id.clone())
                != Some(env.id.clone())
            {
                self.open_envelope(env);
            }
        } else if self.screen == Screen::Search {
            self.search.page.result_selected = false;
            self.clear_message_view_state();
        }
    }

    pub(crate) fn sync_search_cursor_after_move(&mut self) {
        let row_count = self.search_row_count();
        if row_count == 0 {
            self.search.page.selected_index = 0;
            self.search.page.scroll_offset = 0;
            self.search.page.result_selected = false;
            self.clear_message_view_state();
            return;
        }

        self.search.page.selected_index = self
            .search
            .page
            .selected_index
            .min(row_count.saturating_sub(1));
        self.ensure_search_visible();
        self.update_visual_selection();
        self.maybe_load_more_search_results();
        if self.search.page.result_selected {
            self.auto_preview_search();
        }
    }

    pub(crate) fn ensure_search_visible(&mut self) {
        let h = self.visible_height.max(1);
        if self.search.page.selected_index < self.search.page.scroll_offset {
            self.search.page.scroll_offset = self.search.page.selected_index;
        } else if self.search.page.selected_index >= self.search.page.scroll_offset + h {
            self.search.page.scroll_offset = self.search.page.selected_index + 1 - h;
        }
    }
}
