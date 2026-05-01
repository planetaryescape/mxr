use super::*;

impl App {
    pub(super) fn apply_mailbox_action(&mut self, action: Action) {
        match action {
            Action::MoveDown => {
                if self.screen == Screen::Search {
                    if self.search.page.selected_index + 1 < self.search_row_count() {
                        self.search.page.selected_index += 1;
                    }
                    self.sync_search_cursor_after_move();
                    return;
                }
                if self.mailbox.selected_index + 1 < self.mail_row_count() {
                    self.mailbox.selected_index += 1;
                }
                self.ensure_visible();
                self.update_visual_selection();
                self.auto_preview();
            }
            Action::MoveUp => {
                if self.screen == Screen::Search {
                    if self.search.page.selected_index > 0 {
                        self.search.page.selected_index -= 1;
                    }
                    self.sync_search_cursor_after_move();
                    return;
                }
                if self.mailbox.selected_index > 0 {
                    self.mailbox.selected_index -= 1;
                }
                self.ensure_visible();
                self.update_visual_selection();
                self.auto_preview();
            }
            Action::JumpTop => {
                if self.screen == Screen::Search {
                    self.search.page.selected_index = 0;
                    self.sync_search_cursor_after_move();
                    return;
                }
                self.mailbox.selected_index = 0;
                self.mailbox.scroll_offset = 0;
                self.auto_preview();
            }
            Action::JumpBottom => {
                if self.screen == Screen::Search {
                    if self.search.page.has_more {
                        self.search.page.load_to_end = true;
                        self.load_more_search_results();
                    } else if self.search_row_count() > 0 {
                        self.search.page.selected_index = self.search_row_count() - 1;
                        self.sync_search_cursor_after_move();
                    }
                    return;
                }
                if self.mail_row_count() > 0 {
                    self.mailbox.selected_index = self.mail_row_count() - 1;
                }
                self.ensure_visible();
                self.auto_preview();
            }
            Action::PageDown => {
                if self.screen == Screen::Search {
                    let page = self.visible_height.max(1);
                    self.search.page.selected_index = (self.search.page.selected_index + page)
                        .min(self.search_row_count().saturating_sub(1));
                    self.sync_search_cursor_after_move();
                    return;
                }
                let page = self.visible_height.max(1);
                self.mailbox.selected_index = (self.mailbox.selected_index + page)
                    .min(self.mail_row_count().saturating_sub(1));
                self.ensure_visible();
                self.auto_preview();
            }
            Action::PageUp => {
                if self.screen == Screen::Search {
                    let page = self.visible_height.max(1);
                    self.search.page.selected_index =
                        self.search.page.selected_index.saturating_sub(page);
                    self.sync_search_cursor_after_move();
                    return;
                }
                let page = self.visible_height.max(1);
                self.mailbox.selected_index = self.mailbox.selected_index.saturating_sub(page);
                self.ensure_visible();
                self.auto_preview();
            }
            Action::ViewportTop => {
                self.mailbox.selected_index = self.mailbox.scroll_offset;
                self.auto_preview();
            }
            Action::ViewportMiddle => {
                let visible_height = 20;
                self.mailbox.selected_index = (self.mailbox.scroll_offset + visible_height / 2)
                    .min(self.mail_row_count().saturating_sub(1));
                self.auto_preview();
            }
            Action::ViewportBottom => {
                let visible_height = 20;
                self.mailbox.selected_index = (self.mailbox.scroll_offset + visible_height)
                    .min(self.mail_row_count().saturating_sub(1));
                self.auto_preview();
            }
            Action::CenterCurrent => {
                let visible_height = 20;
                self.mailbox.scroll_offset = self
                    .mailbox
                    .selected_index
                    .saturating_sub(visible_height / 2);
            }
            Action::SwitchPane => {
                if self.screen == Screen::Search {
                    self.search.page.active_pane = match self.search.page.active_pane {
                        SearchPane::Results => {
                            self.maybe_open_search_preview();
                            self.search.page.active_pane
                        }
                        SearchPane::Preview => SearchPane::Results,
                    };
                    return;
                }
                self.mailbox.active_pane =
                    match (self.mailbox.layout_mode, self.mailbox.active_pane) {
                        // ThreePane: Sidebar → MailList → MessageView → Sidebar
                        (LayoutMode::ThreePane, ActivePane::Sidebar) => ActivePane::MailList,
                        (LayoutMode::ThreePane, ActivePane::MailList) => ActivePane::MessageView,
                        (LayoutMode::ThreePane, ActivePane::MessageView) => ActivePane::Sidebar,
                        // FullScreen: Sidebar → MessageView → Sidebar
                        (LayoutMode::FullScreen, ActivePane::Sidebar) => ActivePane::MessageView,
                        (LayoutMode::FullScreen, ActivePane::MessageView) => ActivePane::Sidebar,
                        // TwoPane: Sidebar → MailList → Sidebar
                        (_, ActivePane::Sidebar) => ActivePane::MailList,
                        (_, ActivePane::MailList) => ActivePane::Sidebar,
                        (_, ActivePane::MessageView) => ActivePane::Sidebar,
                    };
            }
            Action::OpenSelected => {
                if let Some(pending) = self.modals.pending_bulk_confirm.take() {
                    if let Some(effect) = pending.optimistic_effect.as_ref() {
                        self.apply_local_mutation_effect(effect);
                    }
                    self.queue_mutation(pending.request, pending.effect, pending.status_message);
                    self.clear_selection();
                    return;
                }
                if self.screen == Screen::Search {
                    self.open_selected_search_result();
                    return;
                }
                if self.mailbox.mailbox_view == MailboxView::Subscriptions {
                    if let Some(entry) = self.selected_subscription_entry().cloned() {
                        self.open_envelope(entry.envelope);
                        self.mailbox.layout_mode = LayoutMode::ThreePane;
                        self.mailbox.active_pane = ActivePane::MessageView;
                    }
                    return;
                }
                if let Some(row) = self.selected_mail_row() {
                    self.open_envelope(row.representative);
                    self.mailbox.layout_mode = LayoutMode::ThreePane;
                    self.mailbox.active_pane = ActivePane::MessageView;
                }
            }
            Action::Back => match self.mailbox.active_pane {
                _ if self.screen != Screen::Mailbox => {
                    self.screen = Screen::Mailbox;
                }
                ActivePane::MessageView => {
                    self.apply(Action::CloseMessageView);
                }
                ActivePane::MailList => {
                    if !self.mailbox.selected_set.is_empty() {
                        self.apply(Action::ClearSelection);
                    } else if self.search.active {
                        self.apply(Action::CloseSearch);
                    } else if self.mailbox.active_label.is_some() {
                        self.apply(Action::ClearFilter);
                    } else if self.mailbox.layout_mode == LayoutMode::ThreePane {
                        self.apply(Action::CloseMessageView);
                    }
                }
                ActivePane::Sidebar => {}
            },
            Action::QuitView => {
                self.should_quit = true;
            }
            Action::ClearSelection => {
                self.clear_selection();
                self.status_message = Some("Selection cleared".into());
            }
            // Search
            Action::GoToInbox => {
                if let Some(label) = self.mailbox.labels.iter().find(|l| l.name == "INBOX") {
                    self.apply(Action::SelectLabel(label.id.clone()));
                } else {
                    self.mailbox.desired_system_mailbox = Some("INBOX".into());
                }
            }
            Action::GoToStarred => {
                if let Some(label) = self.mailbox.labels.iter().find(|l| l.name == "STARRED") {
                    self.apply(Action::SelectLabel(label.id.clone()));
                } else {
                    self.mailbox.desired_system_mailbox = Some("STARRED".into());
                }
            }
            Action::GoToSent => {
                if let Some(label) = self.mailbox.labels.iter().find(|l| l.name == "SENT") {
                    self.apply(Action::SelectLabel(label.id.clone()));
                } else {
                    self.mailbox.desired_system_mailbox = Some("SENT".into());
                }
            }
            Action::GoToDrafts => {
                if let Some(label) = self.mailbox.labels.iter().find(|l| l.name == "DRAFT") {
                    self.apply(Action::SelectLabel(label.id.clone()));
                } else {
                    self.mailbox.desired_system_mailbox = Some("DRAFT".into());
                }
            }
            Action::GoToAllMail => {
                self.mailbox.mailbox_view = MailboxView::Messages;
                self.apply(Action::ClearFilter);
            }
            Action::OpenSubscriptions => {
                self.mailbox.mailbox_view = MailboxView::Subscriptions;
                self.mailbox.active_label = None;
                self.mailbox.pending_active_label = None;
                self.mailbox.pending_label_fetch = None;
                self.mailbox.pending_preview_read = None;
                self.mailbox.desired_system_mailbox = None;
                self.search.active = false;
                self.screen = Screen::Mailbox;
                self.mailbox.active_pane = ActivePane::MailList;
                self.mailbox.selected_index = self.mailbox.selected_index.min(
                    self.mailbox
                        .subscriptions_page
                        .entries
                        .len()
                        .saturating_sub(1),
                );
                self.mailbox.scroll_offset = 0;
                if self.mailbox.subscriptions_page.entries.is_empty() {
                    self.mailbox.pending_subscriptions_refresh = true;
                }
                self.auto_preview();
            }
            Action::GoToLabel => {
                self.mailbox.mailbox_view = MailboxView::Messages;
                self.apply(Action::ClearFilter);
            }
            Action::OpenMessageView => {
                if self.screen == Screen::Search {
                    self.open_selected_search_result();
                    return;
                }
                if self.mailbox.mailbox_view == MailboxView::Subscriptions {
                    if let Some(entry) = self.selected_subscription_entry().cloned() {
                        self.open_envelope(entry.envelope);
                        self.mailbox.layout_mode = LayoutMode::ThreePane;
                    }
                } else if let Some(row) = self.selected_mail_row() {
                    self.open_envelope(row.representative);
                    self.mailbox.layout_mode = LayoutMode::ThreePane;
                }
            }
            Action::CloseMessageView => {
                if self.screen == Screen::Search {
                    self.reset_search_preview_selection();
                    return;
                }
                self.close_attachment_panel();
                self.mailbox.layout_mode = LayoutMode::TwoPane;
                self.mailbox.active_pane = ActivePane::MailList;
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
            Action::ToggleMailListMode => {
                if self.mailbox.mailbox_view == MailboxView::Subscriptions {
                    return;
                }
                let search_row_message_id = (self.screen == Screen::Search)
                    .then(|| self.selected_search_envelope().map(|env| env.id.clone()))
                    .flatten();
                self.mailbox.mail_list_mode = match self.mailbox.mail_list_mode {
                    MailListMode::Threads => MailListMode::Messages,
                    MailListMode::Messages => MailListMode::Threads,
                };
                if self.screen == Screen::Search {
                    self.search.page.selected_index = search_row_message_id
                        .as_ref()
                        .and_then(|message_id| self.search_row_index_for_message(message_id))
                        .unwrap_or(0)
                        .min(self.search_row_count().saturating_sub(1));
                    if self.search.page.result_selected {
                        self.sync_search_cursor_after_move();
                    } else if self.search_row_count() > 0 {
                        self.ensure_search_visible();
                    }
                } else {
                    self.mailbox.selected_index = self
                        .mailbox
                        .selected_index
                        .min(self.mail_row_count().saturating_sub(1));
                }
            }
            Action::SelectLabel(label_id) => {
                self.mailbox.mailbox_view = MailboxView::Messages;
                self.mailbox.pending_label_fetch = Some(label_id);
                self.mailbox.pending_active_label = self.mailbox.pending_label_fetch.clone();
                self.mailbox.desired_system_mailbox = None;
                self.mailbox.active_pane = ActivePane::MailList;
                self.screen = Screen::Mailbox;
            }
            Action::SelectSavedSearch(query, mode) => {
                self.mailbox.mailbox_view = MailboxView::Messages;
                if self.screen == Screen::Search {
                    self.search.page.query = query.clone();
                    self.search.page.editing = false;
                    self.search.page.mode = mode;
                    self.search.page.sort = SortOrder::DateDesc;
                    self.search.page.active_pane = SearchPane::Results;
                    self.search.bar.query = query.clone();
                    self.search.bar.mode = mode;
                    self.trigger_live_search();
                } else {
                    self.search.active = true;
                    self.mailbox.active_pane = ActivePane::MailList;
                    self.search.bar.query = query.clone();
                    self.search.bar.mode = mode;
                    self.trigger_live_search();
                }
            }
            Action::ClearFilter => {
                self.mailbox.mailbox_view = MailboxView::Messages;
                self.mailbox.active_label = None;
                self.mailbox.pending_active_label = None;
                self.mailbox.pending_preview_read = None;
                self.mailbox.desired_system_mailbox = None;
                self.search.active = false;
                self.mailbox.envelopes = self.all_mail_envelopes();
                self.mailbox.selected_index = 0;
                self.mailbox.scroll_offset = 0;
            }

            // Phase 2: Email actions (Gmail-native A005)
            _ => unreachable!("action routed to wrong handler"),
        }
    }
}
