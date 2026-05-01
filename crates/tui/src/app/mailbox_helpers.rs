use super::*;

impl App {
    pub(super) fn mail_row_count(&self) -> usize {
        if self.mailbox.mailbox_view == MailboxView::Subscriptions {
            return self.mailbox.subscriptions_page.entries.len();
        }
        self.mail_list_rows().len()
    }

    pub(super) fn build_mail_list_rows(
        envelopes: &[Envelope],
        mode: MailListMode,
    ) -> Vec<MailListRow> {
        match mode {
            MailListMode::Messages => envelopes
                .iter()
                .map(|envelope| MailListRow {
                    thread_id: envelope.thread_id.clone(),
                    representative: envelope.clone(),
                    message_count: 1,
                    unread_count: usize::from(!envelope.flags.contains(MessageFlags::READ)),
                })
                .collect(),
            MailListMode::Threads => {
                let mut order: Vec<mxr_core::ThreadId> = Vec::new();
                let mut rows: HashMap<mxr_core::ThreadId, MailListRow> = HashMap::new();
                for envelope in envelopes {
                    let entry = rows.entry(envelope.thread_id.clone()).or_insert_with(|| {
                        order.push(envelope.thread_id.clone());
                        MailListRow {
                            thread_id: envelope.thread_id.clone(),
                            representative: envelope.clone(),
                            message_count: 0,
                            unread_count: 0,
                        }
                    });
                    entry.message_count += 1;
                    if !envelope.flags.contains(MessageFlags::READ) {
                        entry.unread_count += 1;
                    }
                    if sane_mail_sort_timestamp(&envelope.date)
                        > sane_mail_sort_timestamp(&entry.representative.date)
                    {
                        entry.representative = envelope.clone();
                    }
                }
                order
                    .into_iter()
                    .filter_map(|thread_id| rows.remove(&thread_id))
                    .collect()
            }
        }
    }

    pub(super) fn context_envelope(&self) -> Option<&Envelope> {
        if self.screen == Screen::Search {
            // In the results pane, prefer the selected search result so that
            // multi-select (ToggleSelect) targets the highlighted row rather
            // than a stale viewing_envelope left over from the mailbox.
            if self.search.page.active_pane == SearchPane::Results {
                return self
                    .selected_search_envelope()
                    .or_else(|| self.focused_thread_envelope())
                    .or(self.mailbox.viewing_envelope.as_ref());
            }
            return self
                .focused_thread_envelope()
                .or(self.mailbox.viewing_envelope.as_ref())
                .or_else(|| self.selected_search_envelope());
        }

        self.focused_thread_envelope()
            .or(self.mailbox.viewing_envelope.as_ref())
            .or_else(|| self.selected_envelope())
    }

    pub fn mail_list_title(&self) -> String {
        if self.mailbox.mailbox_view == MailboxView::Subscriptions {
            return format!(
                "Subscriptions ({})",
                self.mailbox.subscriptions_page.entries.len()
            );
        }

        let list_name = match self.mailbox.mail_list_mode {
            MailListMode::Threads => "Threads",
            MailListMode::Messages => "Messages",
        };
        let list_count = self.mail_row_count();
        if self.search.active {
            format!("Search: {} ({list_count})", self.search.bar.query)
        } else if let Some(label_id) = self
            .mailbox
            .pending_active_label
            .as_ref()
            .or(self.mailbox.active_label.as_ref())
        {
            if let Some(label) = self.mailbox.labels.iter().find(|l| &l.id == label_id) {
                let name = crate::ui::sidebar::humanize_label(&label.name);
                format!("{name} {list_name} ({list_count})")
            } else {
                format!("{list_name} ({list_count})")
            }
        } else {
            format!("All Mail {list_name} ({list_count})")
        }
    }

    pub(super) fn all_mail_envelopes(&self) -> Vec<Envelope> {
        self.mailbox
            .all_envelopes
            .iter()
            .filter(|envelope| !envelope.flags.contains(MessageFlags::TRASH))
            .cloned()
            .collect()
    }

    pub(super) fn active_label_record(&self) -> Option<&Label> {
        let label_id = self
            .mailbox
            .pending_active_label
            .as_ref()
            .or(self.mailbox.active_label.as_ref())?;
        self.mailbox
            .labels
            .iter()
            .find(|label| &label.id == label_id)
    }

    pub(super) fn global_starred_count(&self) -> usize {
        self.mailbox
            .labels
            .iter()
            .find(|label| label.name.eq_ignore_ascii_case("STARRED"))
            .map(|label| label.total_count as usize)
            .unwrap_or_else(|| {
                self.mailbox
                    .all_envelopes
                    .iter()
                    .filter(|envelope| envelope.flags.contains(MessageFlags::STARRED))
                    .count()
            })
    }

    pub fn resolve_desired_system_mailbox(&mut self) {
        let Some(target) = self.mailbox.desired_system_mailbox.as_deref() else {
            return;
        };
        if self.mailbox.pending_active_label.is_some() || self.mailbox.active_label.is_some() {
            return;
        }
        if let Some(label_id) = self
            .mailbox
            .labels
            .iter()
            .find(|label| label.name.eq_ignore_ascii_case(target))
            .map(|label| label.id.clone())
        {
            self.apply(Action::SelectLabel(label_id));
        }
    }

    pub(super) fn auto_preview(&mut self) {
        if self.mailbox.mailbox_view == MailboxView::Subscriptions {
            if let Some(entry) = self.selected_subscription_entry().cloned() {
                if self.mailbox.viewing_envelope.as_ref().map(|e| &e.id) != Some(&entry.envelope.id)
                {
                    self.open_envelope(entry.envelope);
                }
            } else {
                self.mailbox.pending_preview_read = None;
                self.mailbox.viewing_envelope = None;
                self.mailbox.viewed_thread = None;
                self.mailbox.viewed_thread_messages.clear();
                self.mailbox.body_view_state = BodyViewState::Empty { preview: None };
            }
            return;
        }

        if self.mailbox.layout_mode == LayoutMode::ThreePane {
            if let Some(row) = self.selected_mail_row() {
                if self.mailbox.viewing_envelope.as_ref().map(|e| &e.id)
                    != Some(&row.representative.id)
                {
                    self.open_envelope(row.representative);
                }
            }
        }
    }

    pub fn queue_body_window(&mut self) {
        const BUFFER: usize = 50;
        let source_envelopes: Vec<Envelope> =
            if self.mailbox.mailbox_view == MailboxView::Subscriptions {
                self.mailbox
                    .subscriptions_page
                    .entries
                    .iter()
                    .map(|entry| entry.envelope.clone())
                    .collect()
            } else {
                self.mailbox.envelopes.clone()
            };
        let len = source_envelopes.len();
        if len == 0 {
            return;
        }
        let start = self.mailbox.selected_index.saturating_sub(BUFFER / 2);
        let end = (self.mailbox.selected_index + BUFFER / 2).min(len);
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
        self.mailbox.signature_expanded = false;
        self.mailbox.viewed_thread = None;
        self.mailbox.viewed_thread_messages = self.optimistic_thread_messages(&env);
        self.mailbox.thread_selected_index = self.default_thread_selected_index();
        self.mailbox.viewing_envelope = self.focused_thread_envelope().cloned();
        if let Some(viewing_envelope) = self.mailbox.viewing_envelope.clone() {
            self.schedule_preview_read(&viewing_envelope);
        }
        for message in self.mailbox.viewed_thread_messages.clone() {
            self.queue_body_fetch(message.id);
        }
        self.queue_thread_fetch(env.thread_id.clone());
        self.queue_html_assets_for_current_view();
        self.mailbox.message_scroll_offset = 0;
        self.ensure_current_body_state();
    }

    pub(super) fn optimistic_thread_messages(&self, env: &Envelope) -> Vec<Envelope> {
        let mut messages: Vec<Envelope> = self
            .mailbox
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
        self.mailbox
            .viewed_thread_messages
            .iter()
            .rposition(|message| !message.flags.contains(MessageFlags::READ))
            .or_else(|| self.mailbox.viewed_thread_messages.len().checked_sub(1))
            .unwrap_or(0)
    }

    pub(super) fn sync_focused_thread_envelope(&mut self) {
        self.close_attachment_panel();
        self.mailbox.viewing_envelope = self.focused_thread_envelope().cloned();
        if let Some(viewing_envelope) = self.mailbox.viewing_envelope.clone() {
            self.schedule_preview_read(&viewing_envelope);
        } else {
            self.mailbox.pending_preview_read = None;
        }
        self.mailbox.message_scroll_offset = 0;
        self.ensure_current_body_state();
    }

    pub(super) fn schedule_preview_read(&mut self, envelope: &Envelope) {
        if envelope.flags.contains(MessageFlags::READ)
            || self.has_pending_set_read(&envelope.id, true)
        {
            self.mailbox.pending_preview_read = None;
            return;
        }

        if self
            .mailbox
            .pending_preview_read
            .as_ref()
            .is_some_and(|pending| pending.message_id == envelope.id)
        {
            return;
        }

        self.mailbox.pending_preview_read = Some(PendingPreviewRead {
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
        let Some(pending) = self.mailbox.pending_preview_read.clone() else {
            return;
        };
        if Instant::now() < pending.due_at {
            return;
        }
        self.mailbox.pending_preview_read = None;

        let Some(envelope) = self
            .mailbox
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

    pub(super) fn move_thread_focus_down(&mut self) {
        if self.mailbox.thread_selected_index + 1 < self.mailbox.viewed_thread_messages.len() {
            self.mailbox.thread_selected_index += 1;
            self.sync_focused_thread_envelope();
        }
    }

    pub(super) fn move_thread_focus_up(&mut self) {
        if self.mailbox.thread_selected_index > 0 {
            self.mailbox.thread_selected_index -= 1;
            self.sync_focused_thread_envelope();
        }
    }

    pub(super) fn move_message_view_down(&mut self) {
        if self.mailbox.viewed_thread_messages.len() > 1 {
            self.move_thread_focus_down();
        } else {
            self.mailbox.message_scroll_offset =
                self.mailbox.message_scroll_offset.saturating_add(1);
        }
    }

    pub(super) fn move_message_view_up(&mut self) {
        if self.mailbox.viewed_thread_messages.len() > 1 {
            self.move_thread_focus_up();
        } else {
            self.mailbox.message_scroll_offset =
                self.mailbox.message_scroll_offset.saturating_sub(1);
        }
    }

    pub(super) fn ensure_current_body_state(&mut self) {
        if let Some(env) = self.mailbox.viewing_envelope.clone() {
            if !self.mailbox.body_cache.contains_key(&env.id) {
                self.queue_body_fetch(env.id.clone());
            }
            self.mailbox.body_view_state = self.resolve_body_view_state(&env);
        } else {
            self.mailbox.body_view_state = BodyViewState::Empty { preview: None };
        }
    }

    pub(super) fn queue_body_fetch(&mut self, message_id: MessageId) {
        if self.mailbox.body_cache.contains_key(&message_id)
            || self.mailbox.in_flight_body_requests.contains(&message_id)
            || self.mailbox.queued_body_fetches.contains(&message_id)
        {
            return;
        }

        self.mailbox
            .in_flight_body_requests
            .insert(message_id.clone());
        self.mailbox.queued_body_fetches.push(message_id);
    }

    pub(super) fn queue_thread_fetch(&mut self, thread_id: mxr_core::ThreadId) {
        if self.mailbox.pending_thread_fetch.as_ref() == Some(&thread_id)
            || self.mailbox.in_flight_thread_fetch.as_ref() == Some(&thread_id)
        {
            return;
        }
        self.mailbox.pending_thread_fetch = Some(thread_id);
    }

    pub(crate) fn clear_message_view_state(&mut self) {
        self.mailbox.pending_preview_read = None;
        self.mailbox.viewing_envelope = None;
        self.mailbox.viewed_thread = None;
        self.mailbox.viewed_thread_messages.clear();
        self.mailbox.thread_selected_index = 0;
        self.mailbox.pending_thread_fetch = None;
        self.mailbox.in_flight_thread_fetch = None;
        self.mailbox.message_scroll_offset = 0;
        self.mailbox.body_view_state = BodyViewState::Empty { preview: None };
    }

    pub fn resolve_thread_success(&mut self, thread: Thread, mut messages: Vec<Envelope>) {
        let thread_id = thread.id.clone();
        self.mailbox.in_flight_thread_fetch = None;
        messages.sort_by_key(|message| message.date);

        if self
            .mailbox
            .viewing_envelope
            .as_ref()
            .map(|env| env.thread_id.clone())
            == Some(thread_id)
        {
            let focused_message_id = self.focused_thread_envelope().map(|env| env.id.clone());
            for message in &messages {
                self.queue_body_fetch(message.id.clone());
            }
            self.mailbox.viewed_thread = Some(thread);
            self.mailbox.viewed_thread_messages = messages;
            self.mailbox.thread_selected_index = focused_message_id
                .and_then(|message_id| {
                    self.mailbox
                        .viewed_thread_messages
                        .iter()
                        .position(|message| message.id == message_id)
                })
                .unwrap_or_else(|| self.default_thread_selected_index());
            self.sync_focused_thread_envelope();
            self.queue_html_assets_for_current_view();
        }
    }

    pub fn resolve_thread_fetch_error(&mut self, thread_id: &mxr_core::ThreadId) {
        if self.mailbox.in_flight_thread_fetch.as_ref() == Some(thread_id) {
            self.mailbox.in_flight_thread_fetch = None;
        }
    }

    pub(super) fn ensure_visible(&mut self) {
        let h = self.visible_height.max(1);
        if self.mailbox.selected_index < self.mailbox.scroll_offset {
            self.mailbox.scroll_offset = self.mailbox.selected_index;
        } else if self.mailbox.selected_index >= self.mailbox.scroll_offset + h {
            self.mailbox.scroll_offset = self.mailbox.selected_index + 1 - h;
        }
        // Prefetch bodies for messages near the cursor
        self.queue_body_window();
    }

    pub fn set_subscriptions(&mut self, subscriptions: Vec<SubscriptionSummary>) {
        let selected_id = self
            .selected_subscription_entry()
            .map(|entry| entry.summary.latest_message_id.clone());
        self.mailbox.subscriptions_page.entries = subscriptions
            .into_iter()
            .map(|summary| SubscriptionEntry {
                envelope: subscription_summary_to_envelope(&summary),
                summary,
            })
            .collect();

        if self.mailbox.subscriptions_page.entries.is_empty() {
            if self.mailbox.mailbox_view == MailboxView::Subscriptions {
                self.mailbox.selected_index = 0;
                self.mailbox.scroll_offset = 0;
                self.mailbox.viewing_envelope = None;
                self.mailbox.viewed_thread = None;
                self.mailbox.viewed_thread_messages.clear();
                self.mailbox.body_view_state = BodyViewState::Empty { preview: None };
            }
            return;
        }

        if let Some(selected_id) = selected_id {
            if let Some(position) = self
                .mailbox
                .subscriptions_page
                .entries
                .iter()
                .position(|entry| entry.summary.latest_message_id == selected_id)
            {
                self.mailbox.selected_index = position;
            } else {
                self.mailbox.selected_index = self.mailbox.selected_index.min(
                    self.mailbox
                        .subscriptions_page
                        .entries
                        .len()
                        .saturating_sub(1),
                );
            }
        } else {
            self.mailbox.selected_index = self.mailbox.selected_index.min(
                self.mailbox
                    .subscriptions_page
                    .entries
                    .len()
                    .saturating_sub(1),
            );
        }

        if self.mailbox.mailbox_view == MailboxView::Subscriptions {
            self.ensure_visible();
            self.auto_preview();
        }
    }
}
