use super::*;

impl App {
    pub(super) fn apply_search_action(&mut self, action: Action) {
        match action {
            Action::OpenMailboxFilter => {
                if self.search.active {
                    self.search.bar.activate_existing();
                } else {
                    self.search.bar.activate();
                }
            }
            Action::SubmitSearch => {
                if self.screen == Screen::Search {
                    self.search.page.editing = false;
                    self.search.bar.query = self.search.page.query.clone();
                    self.execute_search_page_search();
                } else {
                    self.search.bar.deactivate();
                    if !self.search.bar.query.is_empty() {
                        self.search.active = true;
                        self.trigger_live_search();
                    }
                    // Return focus to mail list so j/k navigates results
                    self.mailbox.active_pane = ActivePane::MailList;
                }
            }
            Action::CycleSearchMode => {
                self.search.bar.cycle_mode();
                if self.screen == Screen::Search {
                    self.search.page.mode = self.search.bar.mode;
                }
                if self.screen == Screen::Search || self.search.bar.active {
                    self.trigger_live_search();
                }
            }
            Action::CloseSearch => {
                self.search.bar.deactivate();
                self.search.active = false;
                Self::bump_search_session_id(&mut self.search.mailbox_session_id);
                // Restore full envelope list
                self.mailbox.envelopes = self.all_mail_envelopes();
                self.mailbox.selected_index = 0;
                self.mailbox.scroll_offset = 0;
            }
            // `n` and `N` in the reader: the next or previous conversation in
            // the list it came from, searched or not, as in the web reader
            // (blueprint 35). The names stay for `keys.toml`.
            Action::NextSearchResult => self.apply_mailbox_action(Action::MoveDown),
            Action::PrevSearchResult => self.apply_mailbox_action(Action::MoveUp),
            // Navigation
            _ => unreachable!("action routed to wrong handler"),
        }
    }
}
