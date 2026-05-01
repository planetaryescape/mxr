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
        Self::build_mail_list_rows(&self.mailbox.envelopes, self.mailbox.mail_list_mode)
    }

    pub fn search_mail_list_rows(&self) -> Vec<MailListRow> {
        Self::build_mail_list_rows(&self.search.page.results, self.search_list_mode())
    }

    pub fn selected_mail_row(&self) -> Option<MailListRow> {
        if self.mailbox.mailbox_view == MailboxView::Subscriptions {
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
