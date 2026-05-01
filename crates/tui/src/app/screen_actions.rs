use super::*;

impl App {
    pub(super) fn apply_screen_action(&mut self, action: Action) {
        match action {
            Action::OpenMailboxScreen => {
                if self.accounts.page.onboarding_required {
                    self.screen = Screen::Accounts;
                    self.accounts.page.onboarding_modal_open =
                        self.accounts.page.accounts.is_empty() && !self.accounts.page.form.visible;
                    return;
                }
                self.maybe_preserve_new_account_form_draft();
                self.screen = Screen::Mailbox;
                self.mailbox.active_pane = if self.mailbox.layout_mode == LayoutMode::ThreePane {
                    ActivePane::MailList
                } else {
                    self.mailbox.active_pane
                };
            }
            Action::OpenSearchScreen => {
                self.maybe_preserve_new_account_form_draft();
                self.mailbox.pending_preview_read = None;
                self.screen = Screen::Search;
                if self.search.page.has_session() {
                    self.search.page.editing = false;
                    if !self.search.page.result_selected {
                        self.clear_message_view_state();
                    }
                } else {
                    self.reset_search_page_workspace();
                    self.search.page.editing = true;
                    self.search.page.sort = SortOrder::DateDesc;
                }
            }
            Action::OpenGlobalSearch => {
                self.maybe_preserve_new_account_form_draft();
                self.mailbox.pending_preview_read = None;
                self.search.bar.deactivate();
                self.screen = Screen::Search;
                if !self.search.page.has_session() {
                    self.reset_search_page_workspace();
                }
                self.search.page.editing = true;
                self.search.page.active_pane = SearchPane::Results;
                if !self.search.page.result_selected {
                    self.clear_message_view_state();
                }
            }
            Action::OpenRulesScreen => {
                self.maybe_preserve_new_account_form_draft();
                self.mailbox.pending_preview_read = None;
                self.screen = Screen::Rules;
                self.rules.page.refresh_pending = true;
            }
            Action::OpenDiagnosticsScreen => {
                self.maybe_preserve_new_account_form_draft();
                self.mailbox.pending_preview_read = None;
                self.screen = Screen::Diagnostics;
                self.diagnostics.page.refresh_pending = true;
            }
            Action::OpenAccountsScreen => {
                self.mailbox.pending_preview_read = None;
                self.screen = Screen::Accounts;
                self.accounts.page.refresh_pending = true;
            }
            Action::OpenTab1 => {
                self.apply(Action::OpenMailboxScreen);
            }
            Action::OpenTab2 => {
                self.apply(Action::OpenSearchScreen);
            }
            Action::OpenTab3 => {
                self.apply(Action::OpenRulesScreen);
            }
            Action::OpenTab4 => {
                self.apply(Action::OpenAccountsScreen);
            }
            Action::OpenTab5 => {
                self.apply(Action::OpenDiagnosticsScreen);
            }
            // Command palette
            Action::SyncNow => {
                self.queue_mutation(
                    Request::SyncNow { account_id: None },
                    MutationEffect::RefreshList,
                    "Syncing...".into(),
                );
            }
            // Message view
            Action::Noop => {}
            _ => unreachable!("action routed to wrong handler"),
        }
    }
}
