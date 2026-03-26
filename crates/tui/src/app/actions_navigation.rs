use super::*;

impl App {
    pub(super) fn apply_navigation(&mut self, action: Action) {
        match action {
            Action::OpenMailboxScreen => {
                if self.accounts_page.onboarding_required {
                    self.screen = Screen::Accounts;
                    self.accounts_page.onboarding_modal_open =
                        self.accounts_page.accounts.is_empty() && !self.accounts_page.form.visible;
                    return;
                }
                self.maybe_preserve_new_account_form_draft();
                self.screen = Screen::Mailbox;
                self.active_pane = if self.layout_mode == LayoutMode::ThreePane {
                    ActivePane::MailList
                } else {
                    self.active_pane
                };
            }
            Action::OpenSearchScreen => {
                self.maybe_preserve_new_account_form_draft();
                self.pending_preview_read = None;
                self.screen = Screen::Search;
                if self.search_page.has_session() {
                    self.search_page.editing = false;
                    if !self.search_page.result_selected {
                        self.clear_message_view_state();
                    }
                } else {
                    self.reset_search_page_workspace();
                    self.search_page.editing = true;
                    self.search_page.sort = SortOrder::DateDesc;
                }
            }
            Action::OpenGlobalSearch => {
                self.maybe_preserve_new_account_form_draft();
                self.pending_preview_read = None;
                self.search_bar.deactivate();
                self.screen = Screen::Search;
                if !self.search_page.has_session() {
                    self.reset_search_page_workspace();
                }
                self.search_page.editing = true;
                self.search_page.active_pane = SearchPane::Results;
                if !self.search_page.result_selected {
                    self.clear_message_view_state();
                }
            }
            Action::OpenRulesScreen => {
                self.maybe_preserve_new_account_form_draft();
                self.pending_preview_read = None;
                self.screen = Screen::Rules;
                self.rules_page.refresh_pending = true;
            }
            Action::OpenDiagnosticsScreen => {
                self.maybe_preserve_new_account_form_draft();
                self.pending_preview_read = None;
                self.screen = Screen::Diagnostics;
                self.diagnostics_page.refresh_pending = true;
            }
            Action::OpenAccountsScreen => {
                self.pending_preview_read = None;
                self.screen = Screen::Accounts;
                self.accounts_page.refresh_pending = true;
            }
            Action::MoveDown => {
                if self.screen == Screen::Search {
                    if self.search_page.selected_index + 1 < self.search_row_count() {
                        self.search_page.selected_index += 1;
                        self.ensure_search_visible();
                    }
                    self.maybe_load_more_search_results();
                    return;
                }
                if self.selected_index + 1 < self.mail_row_count() {
                    self.selected_index += 1;
                }
                self.ensure_visible();
                self.update_visual_selection();
                self.auto_preview();
            }
            Action::MoveUp => {
                if self.screen == Screen::Search {
                    if self.search_page.selected_index > 0 {
                        self.search_page.selected_index -= 1;
                        self.ensure_search_visible();
                    }
                    return;
                }
                if self.selected_index > 0 {
                    self.selected_index -= 1;
                }
                self.ensure_visible();
                self.update_visual_selection();
                self.auto_preview();
            }
            Action::JumpTop => {
                if self.screen == Screen::Search {
                    self.search_page.selected_index = 0;
                    self.search_page.scroll_offset = 0;
                    return;
                }
                self.selected_index = 0;
                self.scroll_offset = 0;
                self.auto_preview();
            }
            Action::JumpBottom => {
                if self.screen == Screen::Search {
                    if self.search_page.has_more {
                        self.search_page.load_to_end = true;
                        self.load_more_search_results();
                    } else if self.search_row_count() > 0 {
                        self.search_page.selected_index = self.search_row_count() - 1;
                        self.ensure_search_visible();
                    }
                    return;
                }
                if self.mail_row_count() > 0 {
                    self.selected_index = self.mail_row_count() - 1;
                }
                self.ensure_visible();
                self.auto_preview();
            }
            Action::PageDown => {
                if self.screen == Screen::Search {
                    let page = self.visible_height.max(1);
                    self.search_page.selected_index = (self.search_page.selected_index + page)
                        .min(self.search_row_count().saturating_sub(1));
                    self.ensure_search_visible();
                    self.maybe_load_more_search_results();
                    return;
                }
                let page = self.visible_height.max(1);
                self.selected_index =
                    (self.selected_index + page).min(self.mail_row_count().saturating_sub(1));
                self.ensure_visible();
                self.auto_preview();
            }
            Action::PageUp => {
                if self.screen == Screen::Search {
                    let page = self.visible_height.max(1);
                    self.search_page.selected_index =
                        self.search_page.selected_index.saturating_sub(page);
                    self.ensure_search_visible();
                    return;
                }
                let page = self.visible_height.max(1);
                self.selected_index = self.selected_index.saturating_sub(page);
                self.ensure_visible();
                self.auto_preview();
            }
            Action::ViewportTop => {
                self.selected_index = self.scroll_offset;
                self.auto_preview();
            }
            Action::ViewportMiddle => {
                let visible_height = 20;
                self.selected_index = (self.scroll_offset + visible_height / 2)
                    .min(self.mail_row_count().saturating_sub(1));
                self.auto_preview();
            }
            Action::ViewportBottom => {
                let visible_height = 20;
                self.selected_index = (self.scroll_offset + visible_height)
                    .min(self.mail_row_count().saturating_sub(1));
                self.auto_preview();
            }
            Action::CenterCurrent => {
                let visible_height = 20;
                self.scroll_offset = self.selected_index.saturating_sub(visible_height / 2);
            }
            Action::SwitchPane => {
                if self.screen == Screen::Search {
                    self.search_page.active_pane = match self.search_page.active_pane {
                        SearchPane::Results => {
                            self.maybe_open_search_preview();
                            self.search_page.active_pane
                        }
                        SearchPane::Preview => SearchPane::Results,
                    };
                    return;
                }
                self.active_pane = match (self.layout_mode, self.active_pane) {
                    // ThreePane: Sidebar → MailList → MessageView → Sidebar
                    (LayoutMode::ThreePane, ActivePane::Sidebar) => ActivePane::MailList,
                    (LayoutMode::ThreePane, ActivePane::MailList) => ActivePane::MessageView,
                    (LayoutMode::ThreePane, ActivePane::MessageView) => ActivePane::Sidebar,
                    // TwoPane: Sidebar → MailList → Sidebar
                    (_, ActivePane::Sidebar) => ActivePane::MailList,
                    (_, ActivePane::MailList) => ActivePane::Sidebar,
                    (_, ActivePane::MessageView) => ActivePane::Sidebar,
                };
            }
            Action::Back => match self.active_pane {
                _ if self.screen != Screen::Mailbox => {
                    self.screen = Screen::Mailbox;
                }
                ActivePane::MessageView => {
                    self.apply(Action::CloseMessageView);
                }
                ActivePane::MailList => {
                    if !self.selected_set.is_empty() {
                        self.apply(Action::ClearSelection);
                    } else if self.search_active {
                        self.apply(Action::CloseSearch);
                    } else if self.active_label.is_some() {
                        self.apply(Action::ClearFilter);
                    } else if self.layout_mode == LayoutMode::ThreePane {
                        self.apply(Action::CloseMessageView);
                    }
                }
                ActivePane::Sidebar => {}
            },
            Action::QuitView => {
                self.should_quit = true;
            }
            // Search
            Action::OpenMailboxFilter => {
                if self.search_active {
                    self.search_bar.activate_existing();
                } else {
                    self.search_bar.activate();
                }
            }
            Action::SubmitSearch => {
                if self.screen == Screen::Search {
                    self.search_page.editing = false;
                    self.search_bar.query = self.search_page.query.clone();
                    self.execute_search_page_search();
                } else {
                    self.search_bar.deactivate();
                    if !self.search_bar.query.is_empty() {
                        self.search_active = true;
                        self.trigger_live_search();
                    }
                    // Return focus to mail list so j/k navigates results
                    self.active_pane = ActivePane::MailList;
                }
            }
            Action::CycleSearchMode => {
                self.search_bar.cycle_mode();
                if self.screen == Screen::Search {
                    self.search_page.mode = self.search_bar.mode;
                }
                if self.screen == Screen::Search || self.search_bar.active {
                    self.trigger_live_search();
                }
            }
            Action::CloseSearch => {
                self.search_bar.deactivate();
                self.search_active = false;
                Self::bump_search_session_id(&mut self.mailbox_search_session_id);
                // Restore full envelope list
                self.envelopes = self.all_mail_envelopes();
                self.selected_index = 0;
                self.scroll_offset = 0;
            }
            Action::NextSearchResult => {
                if self.search_active && self.selected_index + 1 < self.envelopes.len() {
                    self.selected_index += 1;
                    self.ensure_visible();
                    self.auto_preview();
                }
            }
            Action::PrevSearchResult => {
                if self.search_active && self.selected_index > 0 {
                    self.selected_index -= 1;
                    self.ensure_visible();
                    self.auto_preview();
                }
            }
            // Go-to shortcuts
            Action::GoToInbox => {
                if let Some(label) = self.labels.iter().find(|l| l.name == system_labels::INBOX) {
                    self.apply(Action::SelectLabel(label.id.clone()));
                } else {
                    self.desired_system_mailbox = Some(system_labels::INBOX.into());
                }
            }
            Action::GoToStarred => {
                if let Some(label) = self.labels.iter().find(|l| l.name == system_labels::STARRED) {
                    self.apply(Action::SelectLabel(label.id.clone()));
                } else {
                    self.desired_system_mailbox = Some(system_labels::STARRED.into());
                }
            }
            Action::GoToSent => {
                if let Some(label) = self.labels.iter().find(|l| l.name == system_labels::SENT) {
                    self.apply(Action::SelectLabel(label.id.clone()));
                } else {
                    self.desired_system_mailbox = Some(system_labels::SENT.into());
                }
            }
            Action::GoToDrafts => {
                if let Some(label) = self.labels.iter().find(|l| l.name == system_labels::DRAFT) {
                    self.apply(Action::SelectLabel(label.id.clone()));
                } else {
                    self.desired_system_mailbox = Some(system_labels::DRAFT.into());
                }
            }
            Action::GoToAllMail => {
                self.mailbox_view = MailboxView::Messages;
                self.apply(Action::ClearFilter);
            }
            Action::OpenSubscriptions => {
                self.mailbox_view = MailboxView::Subscriptions;
                self.active_label = None;
                self.pending_active_label = None;
                self.pending_label_fetch = None;
                self.pending_preview_read = None;
                self.desired_system_mailbox = None;
                self.search_active = false;
                self.screen = Screen::Mailbox;
                self.active_pane = ActivePane::MailList;
                self.selected_index = self
                    .selected_index
                    .min(self.subscriptions_page.entries.len().saturating_sub(1));
                self.scroll_offset = 0;
                if self.subscriptions_page.entries.is_empty() {
                    self.pending_subscriptions_refresh = true;
                }
                self.auto_preview();
            }
            Action::GoToLabel => {
                self.mailbox_view = MailboxView::Messages;
                self.apply(Action::ClearFilter);
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
            Action::SelectLabel(label_id) => {
                self.mailbox_view = MailboxView::Messages;
                self.pending_label_fetch = Some(label_id);
                self.pending_active_label = self.pending_label_fetch.clone();
                self.desired_system_mailbox = None;
                self.active_pane = ActivePane::MailList;
                self.screen = Screen::Mailbox;
            }
            Action::SelectSavedSearch(query, mode) => {
                self.mailbox_view = MailboxView::Messages;
                if self.screen == Screen::Search {
                    self.search_page.query.clone_from(&query);
                    self.search_page.editing = false;
                    self.search_page.mode = mode;
                    self.search_page.sort = SortOrder::DateDesc;
                    self.search_page.active_pane = SearchPane::Results;
                    self.search_bar.query.clone_from(&query);
                    self.search_bar.mode = mode;
                    self.trigger_live_search();
                } else {
                    self.search_active = true;
                    self.active_pane = ActivePane::MailList;
                    self.search_bar.query.clone_from(&query);
                    self.search_bar.mode = mode;
                    self.trigger_live_search();
                }
            }
            Action::ClearFilter => {
                self.mailbox_view = MailboxView::Messages;
                self.active_label = None;
                self.pending_active_label = None;
                self.pending_preview_read = None;
                self.desired_system_mailbox = None;
                self.search_active = false;
                self.envelopes = self.all_mail_envelopes();
                self.selected_index = 0;
                self.scroll_offset = 0;
            }
            _ => unreachable!(),
        }
    }
}
