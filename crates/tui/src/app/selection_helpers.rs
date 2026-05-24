use super::*;

impl App {
    pub fn selected_envelope(&self) -> Option<&Envelope> {
        if self.mailbox.mailbox_view == MailboxView::Subscriptions {
            return self
                .mailbox
                .subscriptions_page
                .entries
                .get(self.mailbox.selected_index)
                .map(|entry| &entry.envelope);
        }

        match self.mailbox.mail_list_mode {
            MailListMode::Messages => self.mailbox.envelopes.get(self.mailbox.selected_index),
            MailListMode::Threads => self.selected_mail_row().and_then(|row| {
                self.mailbox
                    .envelopes
                    .iter()
                    .find(|env| env.id == row.representative.id)
            }),
        }
    }

    pub fn mail_list_rows(&self) -> Vec<MailListRow> {
        self.with_pending_mutation_markers(Self::build_mail_list_rows(
            &self.mailbox.envelopes,
            self.mailbox.mail_list_mode,
        ))
    }

    pub fn search_mail_list_rows(&self) -> Vec<MailListRow> {
        self.with_pending_mutation_markers(Self::build_mail_list_rows(
            &self.search.page.results,
            self.search_list_mode(),
        ))
    }

    fn with_pending_mutation_markers(&self, mut rows: Vec<MailListRow>) -> Vec<MailListRow> {
        // Pre-compute the set of thread_ids that have any pending or
        // reply-later message in one pass over the envelope lists,
        // then mark rows via O(1) HashSet lookups. The previous
        // implementation iterated every envelope per row inside two
        // `any()` calls — O(rows × envelopes) per render, hit on every
        // J keystroke because the renderer rebuilds rows three times
        // per frame. For a mailbox with hundreds of threads this
        // dominates the per-frame budget and turns scroll into a
        // queue-and-drain experience.
        let pending_ids = self.pending_mutation_message_ids();
        let mut pending_thread_ids: HashSet<mxr_core::ThreadId> = HashSet::new();
        let mut reply_later_thread_ids: HashSet<mxr_core::ThreadId> = HashSet::new();
        if !pending_ids.is_empty() || !self.mailbox.reply_later_message_ids.is_empty() {
            for envelope in self
                .mailbox
                .envelopes
                .iter()
                .chain(self.search.page.results.iter())
            {
                if pending_ids.contains(&envelope.id) {
                    pending_thread_ids.insert(envelope.thread_id.clone());
                }
                if self.mailbox.reply_later_message_ids.contains(&envelope.id) {
                    reply_later_thread_ids.insert(envelope.thread_id.clone());
                }
            }
        }
        for row in &mut rows {
            row.open_commitment_count = self
                .mailbox
                .open_commitment_counts
                .get(&(row.representative.account_id.clone(), row.thread_id.clone()))
                .copied()
                .unwrap_or(0);
            row.pending_mutation = pending_thread_ids.contains(&row.thread_id);
            row.reply_later = reply_later_thread_ids.contains(&row.thread_id);
        }
        rows
    }

    fn pending_mutation_message_ids(&self) -> HashSet<MessageId> {
        self.pending_mutation_queue
            .iter()
            .flat_map(|queued| match &queued.effect {
                MutationEffect::RemoveFromList(message_id)
                | MutationEffect::UpdateFlags { message_id, .. }
                | MutationEffect::ReplyLater { message_id, .. } => vec![message_id.clone()],
                MutationEffect::RemoveFromListMany(message_ids) => message_ids.clone(),
                MutationEffect::UpdateFlagsMany { updates } => updates
                    .iter()
                    .map(|(message_id, _)| message_id.clone())
                    .collect(),
                MutationEffect::ModifyLabels { message_ids, .. } => message_ids.clone(),
                MutationEffect::RefreshList
                | MutationEffect::StatusOnly(_)
                | MutationEffect::SentSuccess { .. } => Vec::new(),
            })
            .collect()
    }

    pub fn selected_mail_row(&self) -> Option<MailListRow> {
        if matches!(
            self.mailbox.mailbox_view,
            MailboxView::Subscriptions | MailboxView::CalendarInvites
        ) {
            return None;
        }
        self.mail_list_rows()
            .get(self.mailbox.selected_index)
            .cloned()
    }

    pub fn selected_subscription_entry(&self) -> Option<&SubscriptionEntry> {
        self.mailbox
            .subscriptions_page
            .entries
            .get(self.mailbox.selected_index)
    }

    pub fn selected_invite(&self) -> Option<&mxr_protocol::CalendarInviteData> {
        self.mailbox
            .calendar_invites_page
            .entries
            .get(self.mailbox.selected_index)
    }

    pub fn focused_thread_envelope(&self) -> Option<&Envelope> {
        self.mailbox
            .viewed_thread_messages
            .get(self.mailbox.thread_selected_index)
    }

    pub fn selected_search_envelope(&self) -> Option<&Envelope> {
        match self.search_list_mode() {
            MailListMode::Messages => self
                .search
                .page
                .results
                .get(self.search.page.selected_index),
            MailListMode::Threads => self
                .search_mail_list_rows()
                .get(self.search.page.selected_index)
                .and_then(|row| {
                    self.search
                        .page
                        .results
                        .iter()
                        .find(|env| env.id == row.representative.id)
                }),
        }
    }

    pub(crate) fn search_row_index_for_message(&self, message_id: &MessageId) -> Option<usize> {
        match self.search_list_mode() {
            MailListMode::Messages => self
                .search
                .page
                .results
                .iter()
                .position(|env| &env.id == message_id),
            MailListMode::Threads => self
                .search
                .page
                .results
                .iter()
                .find(|env| &env.id == message_id)
                .and_then(|env| {
                    self.search_mail_list_rows()
                        .iter()
                        .position(|row| row.thread_id == env.thread_id)
                }),
        }
    }

    pub fn selected_rule(&self) -> Option<&serde_json::Value> {
        self.rules.page.rules.get(self.rules.page.selected_index)
    }

    pub fn selected_account(&self) -> Option<&mxr_protocol::AccountSummaryData> {
        self.accounts
            .page
            .accounts
            .get(self.accounts.page.selected_index)
    }

    pub fn refresh_selected_rule_panel(&mut self) {
        let selected_rule_id = self
            .selected_rule()
            .and_then(|rule| rule["id"].as_str())
            .map(ToString::to_string);

        self.rules.pending_detail = None;
        self.rules.pending_history = None;
        self.rules.pending_dry_run = None;

        if let Some(rule_id) = selected_rule_id {
            match self.rules.page.panel {
                RulesPanel::History => self.rules.pending_history = Some(rule_id),
                RulesPanel::DryRun => self.rules.pending_dry_run = Some(rule_id),
                RulesPanel::Details | RulesPanel::Form => self.rules.pending_detail = Some(rule_id),
            }
        }
    }
}
