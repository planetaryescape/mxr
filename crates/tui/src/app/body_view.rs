use super::*;

impl App {
    /// In ThreePane mode, auto-load the preview for the currently selected envelope.
    pub(super) fn auto_preview(&mut self) {
        if self.mailbox_view == MailboxView::Subscriptions {
            if let Some(entry) = self.selected_subscription_entry().cloned() {
                if self.viewing_envelope.as_ref().map(|e| &e.id) != Some(&entry.envelope.id) {
                    self.open_envelope(entry.envelope);
                }
            } else {
                self.pending_preview_read = None;
                self.viewing_envelope = None;
                self.viewed_thread = None;
                self.viewed_thread_messages.clear();
                self.body_view_state = BodyViewState::Empty { preview: None };
            }
            return;
        }

        if self.layout_mode == LayoutMode::ThreePane {
            if let Some(row) = self.selected_mail_row() {
                if self.viewing_envelope.as_ref().map(|e| &e.id) != Some(&row.representative.id) {
                    self.open_envelope(row.representative);
                }
            }
        }
    }

    pub fn auto_preview_search(&mut self) {
        if !self.search_page.result_selected {
            if self.screen == Screen::Search {
                self.clear_message_view_state();
            }
            return;
        }
        if let Some(env) = self.selected_search_envelope().cloned() {
            if self
                .viewing_envelope
                .as_ref()
                .map(|current| current.id.clone())
                != Some(env.id.clone())
            {
                self.open_envelope(env);
            }
        } else if self.screen == Screen::Search {
            self.search_page.result_selected = false;
            self.clear_message_view_state();
        }
    }

    /// Queue body prefetch for messages around the current cursor position.
    /// Only fetches bodies not already in cache.
    pub fn queue_body_window(&mut self) {
        const BUFFER: usize = 50;
        let source_envelopes: Vec<Envelope> = if self.mailbox_view == MailboxView::Subscriptions {
            self.subscriptions_page
                .entries
                .iter()
                .map(|entry| entry.envelope.clone())
                .collect()
        } else {
            self.envelopes.clone()
        };
        let len = source_envelopes.len();
        if len == 0 {
            return;
        }
        let start = self.selected_index.saturating_sub(BUFFER / 2);
        let end = (self.selected_index + BUFFER / 2).min(len);
        let ids: Vec<MessageId> = source_envelopes[start..end]
            .iter()
            .map(|e| e.id.clone())
            .collect();
        for id in ids {
            self.queue_body_fetch(id);
        }
    }

    pub(super) fn open_envelope(&mut self, env: Envelope) {
        self.close_attachment_panel();
        self.signature_expanded = false;
        self.viewed_thread = None;
        self.viewed_thread_messages = self.optimistic_thread_messages(&env);
        self.thread_selected_index = self.default_thread_selected_index();
        self.viewing_envelope = self.focused_thread_envelope().cloned();
        if let Some(viewing_envelope) = self.viewing_envelope.clone() {
            self.schedule_preview_read(&viewing_envelope);
        }
        for message in self.viewed_thread_messages.clone() {
            self.queue_body_fetch(message.id);
        }
        self.queue_thread_fetch(env.thread_id.clone());
        self.message_scroll_offset = 0;
        self.ensure_current_body_state();
    }

    pub(super) fn optimistic_thread_messages(&self, env: &Envelope) -> Vec<Envelope> {
        let mut messages: Vec<Envelope> = self
            .all_envelopes
            .iter()
            .filter(|candidate| candidate.thread_id == env.thread_id)
            .cloned()
            .collect();
        if messages.is_empty() {
            messages.push(env.clone());
        }
        messages.sort_by_key(|message| message.date);
        messages
    }

    pub(super) fn default_thread_selected_index(&self) -> usize {
        self.viewed_thread_messages
            .iter()
            .rposition(|message| !message.flags.contains(MessageFlags::READ))
            .or_else(|| self.viewed_thread_messages.len().checked_sub(1))
            .unwrap_or(0)
    }

    pub(super) fn sync_focused_thread_envelope(&mut self) {
        self.close_attachment_panel();
        self.viewing_envelope = self.focused_thread_envelope().cloned();
        if let Some(viewing_envelope) = self.viewing_envelope.clone() {
            self.schedule_preview_read(&viewing_envelope);
        } else {
            self.pending_preview_read = None;
        }
        self.message_scroll_offset = 0;
        self.ensure_current_body_state();
    }

    pub(super) fn schedule_preview_read(&mut self, envelope: &Envelope) {
        if envelope.flags.contains(MessageFlags::READ)
            || self.has_pending_set_read(&envelope.id, true)
        {
            self.pending_preview_read = None;
            return;
        }

        if self
            .pending_preview_read
            .as_ref()
            .is_some_and(|pending| pending.message_id == envelope.id)
        {
            return;
        }

        self.pending_preview_read = Some(PendingPreviewRead {
            message_id: envelope.id.clone(),
            due_at: Instant::now() + PREVIEW_MARK_READ_DELAY,
        });
    }

    pub(super) fn has_pending_set_read(&self, message_id: &MessageId, read: bool) -> bool {
        self.pending_mutation_queue.iter().any(|(request, _)| {
            matches!(
                request,
                Request::Mutation(MutationCommand::SetRead { message_ids, read: queued_read })
                    if *queued_read == read
                        && message_ids.len() == 1
                        && message_ids[0] == *message_id
            )
        })
    }

    pub(super) fn process_pending_preview_read(&mut self) {
        let Some(pending) = self.pending_preview_read.clone() else {
            return;
        };
        if Instant::now() < pending.due_at {
            return;
        }
        self.pending_preview_read = None;

        let Some(envelope) = self
            .viewing_envelope
            .clone()
            .filter(|envelope| envelope.id == pending.message_id)
        else {
            return;
        };

        if envelope.flags.contains(MessageFlags::READ)
            || self.has_pending_set_read(&envelope.id, true)
        {
            return;
        }

        let mut flags = envelope.flags;
        flags.insert(MessageFlags::READ);
        self.apply_local_flags(&envelope.id, flags);
        self.queue_mutation(
            Request::Mutation(MutationCommand::SetRead {
                message_ids: vec![envelope.id.clone()],
                read: true,
            }),
            MutationEffect::StatusOnly("Marked message as read".into()),
            "Marking message as read...".into(),
        );
    }

    pub fn next_background_timeout(&self, fallback: Duration) -> Duration {
        let mut timeout = fallback;
        if let Some(pending) = self.pending_preview_read.as_ref() {
            timeout = timeout.min(
                pending
                    .due_at
                    .checked_duration_since(Instant::now())
                    .unwrap_or(Duration::ZERO),
            );
        }
        if let Some(pending) = self.pending_search_debounce.as_ref() {
            timeout = timeout.min(
                pending
                    .due_at
                    .checked_duration_since(Instant::now())
                    .unwrap_or(Duration::ZERO),
            );
        }
        if self.search_is_pending() {
            timeout = timeout.min(SEARCH_SPINNER_TICK);
        }
        timeout
    }

    #[cfg(test)]
    pub fn expire_pending_preview_read_for_tests(&mut self) {
        if let Some(pending) = self.pending_preview_read.as_mut() {
            pending.due_at = Instant::now();
        }
    }

    pub(super) fn move_thread_focus_down(&mut self) {
        if self.thread_selected_index + 1 < self.viewed_thread_messages.len() {
            self.thread_selected_index += 1;
            self.sync_focused_thread_envelope();
        }
    }

    pub(super) fn move_thread_focus_up(&mut self) {
        if self.thread_selected_index > 0 {
            self.thread_selected_index -= 1;
            self.sync_focused_thread_envelope();
        }
    }

    pub(super) fn move_message_view_down(&mut self) {
        if self.viewed_thread_messages.len() > 1 {
            self.move_thread_focus_down();
        } else {
            self.message_scroll_offset = self.message_scroll_offset.saturating_add(1);
        }
    }

    pub(super) fn move_message_view_up(&mut self) {
        if self.viewed_thread_messages.len() > 1 {
            self.move_thread_focus_up();
        } else {
            self.message_scroll_offset = self.message_scroll_offset.saturating_sub(1);
        }
    }

    pub(super) fn ensure_current_body_state(&mut self) {
        if let Some(env) = self.viewing_envelope.clone() {
            if !self.body_cache.contains_key(&env.id) {
                self.queue_body_fetch(env.id.clone());
            }
            self.body_view_state = self.resolve_body_view_state(&env);
        } else {
            self.body_view_state = BodyViewState::Empty { preview: None };
        }
    }

    pub(super) fn queue_body_fetch(&mut self, message_id: MessageId) {
        if self.body_cache.contains_key(&message_id)
            || self.in_flight_body_requests.contains(&message_id)
            || self.queued_body_fetches.contains(&message_id)
        {
            return;
        }

        self.in_flight_body_requests.insert(message_id.clone());
        self.queued_body_fetches.push(message_id);
    }

    pub(super) fn queue_thread_fetch(&mut self, thread_id: mxr_core::ThreadId) {
        if self.pending_thread_fetch.as_ref() == Some(&thread_id)
            || self.in_flight_thread_fetch.as_ref() == Some(&thread_id)
        {
            return;
        }
        self.pending_thread_fetch = Some(thread_id);
    }

    pub(super) fn envelope_preview(envelope: &Envelope) -> Option<String> {
        let snippet = envelope.snippet.trim();
        if snippet.is_empty() {
            None
        } else {
            Some(envelope.snippet.clone())
        }
    }

    pub(super) fn render_body(raw: &str, source: BodySource, reader_mode: bool) -> String {
        if !reader_mode {
            return raw.to_string();
        }

        let config = mxr_reader::ReaderConfig::default();
        match source {
            BodySource::Plain => mxr_reader::clean(Some(raw), None, &config).content,
            BodySource::Html => mxr_reader::clean(None, Some(raw), &config).content,
            BodySource::Snippet => raw.to_string(),
        }
    }

    pub(super) fn resolve_body_view_state(&self, envelope: &Envelope) -> BodyViewState {
        let preview = Self::envelope_preview(envelope);

        if let Some(body) = self.body_cache.get(&envelope.id) {
            if let Some(raw) = body.text_plain.clone() {
                let rendered = Self::render_body(&raw, BodySource::Plain, self.reader_mode);
                return BodyViewState::Ready {
                    raw,
                    rendered,
                    source: BodySource::Plain,
                };
            }

            if let Some(raw) = body.text_html.clone() {
                let rendered = Self::render_body(&raw, BodySource::Html, self.reader_mode);
                return BodyViewState::Ready {
                    raw,
                    rendered,
                    source: BodySource::Html,
                };
            }

            return BodyViewState::Empty { preview };
        }

        if self.in_flight_body_requests.contains(&envelope.id) {
            BodyViewState::Loading { preview }
        } else {
            BodyViewState::Empty { preview }
        }
    }

    pub fn resolve_body_success(&mut self, body: MessageBody) {
        let message_id = body.message_id.clone();
        self.in_flight_body_requests.remove(&message_id);
        self.body_cache.insert(message_id.clone(), body);

        if self.viewing_envelope.as_ref().map(|env| env.id.clone()) == Some(message_id) {
            self.ensure_current_body_state();
        }
    }

    pub fn resolve_body_fetch_error(&mut self, message_id: &MessageId, message: String) {
        self.in_flight_body_requests.remove(message_id);

        if let Some(env) = self
            .viewing_envelope
            .as_ref()
            .filter(|env| &env.id == message_id)
        {
            self.body_view_state = BodyViewState::Error {
                message,
                preview: Self::envelope_preview(env),
            };
        }
    }

    pub fn current_viewing_body(&self) -> Option<&MessageBody> {
        self.viewing_envelope
            .as_ref()
            .and_then(|env| self.body_cache.get(&env.id))
    }
}
