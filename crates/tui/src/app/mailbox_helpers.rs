use super::*;

fn normalized_email(addr: &mxr_core::types::Address) -> String {
    addr.email.trim().to_lowercase()
}

fn ingest_participants_from_envelope(
    envelope: &Envelope,
    into: &mut std::collections::HashSet<String>,
) {
    into.insert(normalized_email(&envelope.from));
    for a in &envelope.to {
        into.insert(normalized_email(a));
    }
    for a in &envelope.cc {
        into.insert(normalized_email(a));
    }
    into.retain(|e| !e.is_empty());
}

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
                    other_participant_count: 0,
                    open_commitment_count: 0,
                    reply_later: false,
                    pending_mutation: false,
                })
                .collect(),
            MailListMode::Threads => {
                let mut order: Vec<mxr_core::ThreadId> = Vec::new();
                let mut rows: HashMap<mxr_core::ThreadId, MailListRow> = HashMap::new();
                let mut participants: HashMap<
                    mxr_core::ThreadId,
                    std::collections::HashSet<String>,
                > = HashMap::new();
                for envelope in envelopes {
                    ingest_participants_from_envelope(
                        envelope,
                        participants.entry(envelope.thread_id.clone()).or_default(),
                    );
                    let entry = rows.entry(envelope.thread_id.clone()).or_insert_with(|| {
                        order.push(envelope.thread_id.clone());
                        MailListRow {
                            thread_id: envelope.thread_id.clone(),
                            representative: envelope.clone(),
                            message_count: 0,
                            unread_count: 0,
                            other_participant_count: 0,
                            open_commitment_count: 0,
                            reply_later: false,
                            pending_mutation: false,
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
                let mut collected: Vec<MailListRow> = order
                    .into_iter()
                    .filter_map(|thread_id| rows.remove(&thread_id))
                    .collect();
                for row in &mut collected {
                    let primary = normalized_email(&row.representative.from);
                    row.other_participant_count = participants
                        .get(&row.thread_id)
                        .map(|set| set.iter().filter(|e| *e != &primary).count())
                        .unwrap_or(0);
                }
                collected
            }
        }
    }

    /// Apply a screener disposition to the currently-focused queue
    /// entry. Optimistically removes the entry and queues the IPC; the
    /// runtime fires `Request::SetScreenerDecision` and refreshes the
    /// queue on success. No-op when the modal is empty.
    pub(super) fn dispatch_screener_disposition(
        &mut self,
        disposition: mxr_protocol::ScreenerDispositionData,
    ) {
        let Some(account_id) = self.modals.screener.account_id.clone() else {
            return;
        };
        let Some(removed) = self.modals.screener.remove_selected() else {
            self.status_message = Some("Screener queue is empty".into());
            return;
        };
        let pretty = match disposition {
            mxr_protocol::ScreenerDispositionData::Allow => "allow",
            mxr_protocol::ScreenerDispositionData::Deny => "deny",
            mxr_protocol::ScreenerDispositionData::Feed => "feed",
            mxr_protocol::ScreenerDispositionData::PaperTrail => "paper-trail",
            mxr_protocol::ScreenerDispositionData::Unknown => "unknown",
        };
        self.status_message = Some(format!("Screener: {pretty} {}", removed.sender_email));
        self.pending_screener_decisions
            .push(PendingScreenerDecision {
                account_id,
                sender_email: removed.sender_email,
                disposition,
            });
    }

    pub(crate) fn context_envelope(&self) -> Option<&Envelope> {
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
        let base = if self.search.active {
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
        };
        // Slice 5.1 (C2.6 cont): when the focused thread qualifies as
        // dormant, append a discoverable hint to the title.
        if let Some(hint) = self.dormant_thread_hint() {
            format!("{base} · {hint}")
        } else {
            base
        }
    }

    /// "Dormant Nd. Press B for briefing" when the focused row's
    /// representative is older than 30d AND the thread has >=3
    /// messages. Per Slice 5.1 doc spec.
    pub fn dormant_thread_hint(&self) -> Option<String> {
        const DORMANT_DAYS: i64 = 30;
        const MIN_MESSAGES: usize = 3;
        let row = self
            .mail_list_rows()
            .into_iter()
            .nth(self.mailbox.selected_index)?;
        if row.message_count < MIN_MESSAGES {
            return None;
        }
        let age = chrono::Utc::now() - row.representative.date;
        let days = age.num_days();
        if days < DORMANT_DAYS {
            return None;
        }
        Some(format!("Dormant {days}d. Press B for briefing"))
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

    /// True when the active label is the one whose membership the operation
    /// removes. Used to decide whether removing a message from the current
    /// list is correct or would just bounce the row when the next sync re-
    /// fetches the (still-matching) view.
    pub(super) fn active_label_matches(&self, label_name: &str) -> bool {
        self.active_label_record()
            .is_some_and(|label| label.name.eq_ignore_ascii_case(label_name))
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
        self.mailbox.thread_summary = None;
        self.mailbox.thread_summary_loading = None;
        self.mailbox.thread_summary_error = None;
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
        self.pending_mutation_queue.iter().any(|queued| {
            matches!(
                &queued.request,
                Request::Mutation {
                    mutation: MutationCommand::SetRead {
                        message_ids,
                        read: queued_read,
                    },
                    ..
                } if *queued_read == read
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
        let optimistic_effect = MutationEffect::UpdateFlags {
            message_id: envelope.id.clone(),
            flags,
        };
        let snapshot = self.snapshot_for_effect(&optimistic_effect);
        self.apply_local_mutation_effect(&optimistic_effect);
        let id = self.queue_best_effort_mutation(
            Request::mutation(MutationCommand::SetRead {
                message_ids: vec![envelope.id.clone()],
                read: true,
            }),
            MutationEffect::StatusOnly("Marked message as read".into()),
            "Marking message as read...".into(),
        );
        self.pending_optimistic.record(id, &optimistic_effect);
        self.mutation_snapshots.insert(id, snapshot);
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
                self.queue_priority_body_fetch(env.id.clone());
            }
            self.mailbox.body_view_state = self.resolve_body_view_state(&env);
        } else {
            self.mailbox.body_view_state = BodyViewState::Empty { preview: None };
        }
    }

    pub(super) fn queue_body_fetch(&mut self, message_id: MessageId) {
        if self.mailbox.body_cache.contains_key(&message_id)
            || self.mailbox.in_flight_body_requests.contains(&message_id)
            || self.mailbox.priority_body_fetches.contains(&message_id)
            || self.mailbox.queued_body_fetches.contains(&message_id)
        {
            return;
        }

        self.mailbox
            .in_flight_body_requests
            .insert(message_id.clone());
        self.mailbox.queued_body_fetches.push(message_id);
    }

    pub(super) fn queue_priority_body_fetch(&mut self, message_id: MessageId) {
        if self.mailbox.body_cache.contains_key(&message_id)
            || self.mailbox.priority_body_fetches.contains(&message_id)
        {
            return;
        }

        self.mailbox
            .queued_body_fetches
            .retain(|id| id != &message_id);
        self.mailbox
            .in_flight_body_requests
            .insert(message_id.clone());
        self.mailbox.priority_body_fetches.push(message_id);
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
        self.mailbox.thread_summary = None;
        self.mailbox.thread_summary_loading = None;
        self.mailbox.thread_summary_error = None;
        self.mailbox.thread_selected_index = 0;
        self.mailbox.pending_thread_fetch = None;
        self.mailbox.in_flight_thread_fetch = None;
        self.mailbox.message_scroll_offset = 0;
        self.mailbox.body_view_state = BodyViewState::Empty { preview: None };
    }

    pub fn resolve_thread_success(
        &mut self,
        thread: Thread,
        mut messages: Vec<Envelope>,
        summary: Option<mxr_protocol::ThreadSummaryData>,
    ) {
        let thread_id = thread.id.clone();
        self.mailbox.in_flight_thread_fetch = None;
        messages.sort_by_key(|message| message.date);

        if self
            .mailbox
            .viewing_envelope
            .as_ref()
            .map(|env| env.thread_id.clone())
            == Some(thread_id.clone())
        {
            let focused_message_id = self.focused_thread_envelope().map(|env| env.id.clone());
            for message in &messages {
                self.queue_body_fetch(message.id.clone());
            }
            self.mailbox.viewed_thread = Some(thread);
            self.mailbox.viewed_thread_messages = messages;
            let summary_was_cached = summary.is_some();
            self.mailbox.thread_summary = summary.map(|summary| ThreadSummaryPreview {
                text: summary.text,
                model: summary.model,
            });
            self.mailbox.thread_summary_error = None;
            // Lazy summary backfill: if the daemon didn't have a cached
            // summary for this thread, schedule one — but *debounce* so
            // holding the down-arrow through the mail list doesn't fire
            // an LLM request for every row passed. The debounce parks
            // the thread_id for ~250ms; if the user keeps moving, this
            // call replaces the pending one with the new thread_id and
            // resets the timer. When they finally land, the timer
            // expires and `lib.rs` drains it into a real IPC request.
            // The request goes on a dedicated IPC connection so it
            // doesn't block body fetches or other navigation.
            if !summary_was_cached
                && self.mailbox.thread_summary_loading.as_ref() != Some(&thread_id)
                && self.pending_summary_request.as_ref() != Some(&thread_id)
            {
                let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(250);
                self.pending_summary_debounce = Some((thread_id.clone(), deadline));
            }
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

/// Reading speed ≈ 200 wpm; a sub-minute read isn't worth a 30-word LLM
/// summary plus a "Next steps:" line. The user can still press `y` to force.
const AUTO_SUMMARY_MIN_WORDS: u32 = 200;

/// Sender local-parts that smell automated. Conservative list — anything
/// that lands here from a real human will still be summarizable on demand.
const AUTO_SENDER_PATTERNS: &[&str] = &[
    "noreply",
    "no-reply",
    "donotreply",
    "do-not-reply",
    "notifications",
    "notification",
    "alerts",
    "automated",
    "mailer-daemon",
    "postmaster",
    "bounce",
];

/// Decide whether a thread is worth auto-summarizing. Returns `false` to
/// suppress the lazy backfill; the user can always trigger summary
/// explicitly with `y`.
///
/// Skip when:
/// - Any message carries a calendar `METHOD` (REQUEST/REPLY/CANCEL) — the
///   invite card already shows the relevant fields and the iCal payload
///   doesn't summarize meaningfully.
/// - The thread has no real body text (empty plain + empty html) across
///   all messages we currently have cached.
/// - Total body word count is under [`AUTO_SUMMARY_MIN_WORDS`].
/// - A single-message thread from an automated/notification sender — the
///   summary is almost guaranteed to be longer than the body.
pub(crate) fn auto_summary_eligible(
    envelopes: &[Envelope],
    body_cache: &HashMap<MessageId, MessageBody>,
) -> bool {
    if envelopes.is_empty() {
        return false;
    }

    let mut total_words: u32 = 0;
    let mut body_text_words: u32 = 0;
    let mut any_body_text = false;
    for envelope in envelopes {
        total_words = total_words.saturating_add(envelope.body_word_count);
        if let Some(body) = body_cache.get(&envelope.id) {
            if body.metadata.calendar.is_some() {
                return false;
            }
            let plain_words = body.text_plain.as_deref().map(count_words).unwrap_or(0);
            let html_words = body.text_html.as_deref().map(count_words).unwrap_or(0);
            let cached_words = plain_words.max(html_words);
            body_text_words = body_text_words.saturating_add(cached_words);
            if cached_words > 0 {
                any_body_text = true;
            }
        }
    }

    // If every cached body is empty *and* word counts agree, skip. Word
    // counts come from sync and are present even when bodies aren't yet
    // fetched — so we only treat "all bodies empty" as decisive when we
    // actually have all the bodies in cache.
    let all_bodies_cached = envelopes
        .iter()
        .all(|envelope| body_cache.contains_key(&envelope.id));
    if all_bodies_cached && !any_body_text {
        return false;
    }

    if envelopes.len() == 1 && is_automated_sender(&envelopes[0].from) {
        return false;
    }

    // Prefer the sync-computed `body_word_count` column, but fall back
    // to counting words in cached bodies when it's zero. Messages
    // synced before the column was added (or where the sync path
    // didn't compute it) persist as 0 and would otherwise gate out
    // every thread the user opens, even with full body text available.
    let effective_words = total_words.max(body_text_words);
    effective_words >= AUTO_SUMMARY_MIN_WORDS
}

fn count_words(text: &str) -> u32 {
    text.split_whitespace()
        .count()
        .try_into()
        .unwrap_or(u32::MAX)
}

fn is_automated_sender(addr: &Address) -> bool {
    let email = addr.email.trim().to_ascii_lowercase();
    let local = email.split('@').next().unwrap_or("");
    if local.is_empty() {
        return false;
    }
    AUTO_SENDER_PATTERNS
        .iter()
        .any(|pattern| local.contains(pattern))
}

#[cfg(test)]
mod auto_summary_tests {
    use super::*;
    use chrono::Utc;
    use mxr_core::id::{AccountId, MessageId, ThreadId};
    use mxr_core::types::{
        Address, CalendarMetadata, MessageBody, MessageFlags, MessageMetadata, UnsubscribeMethod,
    };

    fn envelope(word_count: u32, from_email: &str) -> Envelope {
        Envelope {
            id: MessageId::new(),
            account_id: AccountId::new(),
            provider_id: "prov".into(),
            thread_id: ThreadId::new(),
            message_id_header: None,
            in_reply_to: None,
            references: vec![],
            from: Address {
                name: None,
                email: from_email.into(),
            },
            to: vec![],
            cc: vec![],
            bcc: vec![],
            subject: "subject".into(),
            date: Utc::now(),
            flags: MessageFlags::empty(),
            snippet: String::new(),
            has_attachments: false,
            size_bytes: 0,
            unsubscribe: UnsubscribeMethod::None,
            link_count: 0,
            body_word_count: word_count,
            label_provider_ids: vec![],
            keywords: std::collections::BTreeSet::new(),
        }
    }

    fn body(message_id: MessageId, plain: Option<&str>) -> MessageBody {
        MessageBody {
            message_id,
            text_plain: plain.map(str::to_string),
            text_html: None,
            attachments: vec![],
            fetched_at: Utc::now(),
            metadata: MessageMetadata::default(),
        }
    }

    fn body_with_calendar(message_id: MessageId) -> MessageBody {
        MessageBody {
            message_id,
            text_plain: Some("Calendar invite payload".into()),
            text_html: None,
            attachments: vec![],
            fetched_at: Utc::now(),
            metadata: MessageMetadata {
                calendar: Some(CalendarMetadata {
                    method: Some("REQUEST".into()),
                    summary: Some("Sync".into()),
                    ..CalendarMetadata::default()
                }),
                ..MessageMetadata::default()
            },
        }
    }

    #[test]
    fn long_thread_is_eligible() {
        let envs = vec![envelope(250, "alice@example.com")];
        assert!(auto_summary_eligible(&envs, &HashMap::new()));
    }

    #[test]
    fn short_thread_is_skipped() {
        let envs = vec![envelope(40, "alice@example.com")];
        assert!(!auto_summary_eligible(&envs, &HashMap::new()));
    }

    #[test]
    fn calendar_invite_is_skipped() {
        let env = envelope(500, "alice@example.com");
        let mut bodies = HashMap::new();
        bodies.insert(env.id.clone(), body_with_calendar(env.id.clone()));
        assert!(!auto_summary_eligible(&[env], &bodies));
    }

    #[test]
    fn single_automated_sender_is_skipped() {
        let envs = vec![envelope(500, "no-reply@stripe.com")];
        assert!(!auto_summary_eligible(&envs, &HashMap::new()));
    }

    #[test]
    fn multi_message_with_automated_sender_still_eligible() {
        let envs = vec![
            envelope(150, "notifications@github.com"),
            envelope(150, "alice@example.com"),
        ];
        assert!(auto_summary_eligible(&envs, &HashMap::new()));
    }

    #[test]
    fn empty_bodies_when_all_cached_skip() {
        let env = envelope(0, "alice@example.com");
        let mut bodies = HashMap::new();
        bodies.insert(env.id.clone(), body(env.id.clone(), None));
        assert!(!auto_summary_eligible(&[env], &bodies));
    }

    #[test]
    fn empty_bodies_partially_cached_falls_through_to_word_count() {
        // No bodies cached at all → only word count gates. Sufficient
        // count keeps eligibility live so we don't suppress on every
        // first paint before bodies arrive.
        let envs = vec![envelope(500, "alice@example.com")];
        assert!(auto_summary_eligible(&envs, &HashMap::new()));
    }

    #[test]
    fn cached_body_text_overrides_zero_body_word_count() {
        // Messages synced before `body_word_count` was added (or where
        // the sync path didn't compute it) persist as 0. If we have a
        // substantial body in cache, count its words directly instead
        // of gating out every thread the user opens.
        let env = envelope(0, "alice@example.com");
        let long_text = "word ".repeat(300);
        let mut bodies = HashMap::new();
        bodies.insert(env.id.clone(), body(env.id.clone(), Some(&long_text)));
        assert!(
            auto_summary_eligible(&[env], &bodies),
            "fallback word count from cached body must override zero column"
        );
    }
}
