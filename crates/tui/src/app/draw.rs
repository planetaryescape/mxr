use super::*;

fn tab_titles() -> [&'static str; 5] {
    [
        "1 Mailbox",
        "2 Search",
        "3 Rules",
        "4 Accounts",
        "5 Diagnostics",
    ]
}

fn selected_tab(screen: Screen) -> usize {
    match screen {
        Screen::Mailbox => 0,
        Screen::Search => 1,
        Screen::Rules => 2,
        Screen::Accounts => 3,
        Screen::Diagnostics => 4,
    }
}

impl App {
    pub fn draw(&mut self, frame: &mut Frame) {
        let theme = &self.theme;
        let area = frame.area();

        // Layout: tabs (1 line) | hint bar (2 lines) | content | status bar (1 line)
        let outer_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // tab bar
                Constraint::Length(2), // hint bar
                Constraint::Min(0),    // content
                Constraint::Length(1), // status bar
            ])
            .split(area);

        let tab_bar_area = outer_chunks[0];
        let hint_bar_area = outer_chunks[1];
        let content_area = outer_chunks[2];
        let ui_context = self.current_ui_context();
        // Search results sit below a fixed 4-line query box, and the list itself has borders,
        // so the usable result viewport is shorter than the main content area.
        self.visible_height = match self.screen {
            Screen::Search => content_area.height.saturating_sub(6) as usize,
            _ => content_area.height.saturating_sub(2) as usize,
        };
        let bottom_bar_area = outer_chunks[3];

        // Tab bar
        let tabs = ratatui::widgets::Tabs::new(tab_titles())
            .select(selected_tab(self.screen))
            .style(Style::default().fg(theme.text_muted))
            .highlight_style(Style::default().fg(theme.accent).bold())
            .divider(Span::styled(" | ", Style::default().fg(theme.text_muted)));
        frame.render_widget(tabs, tab_bar_area);

        // Hint bar
        ui::hint_bar::draw(
            frame,
            hint_bar_area,
            ui::hint_bar::HintBarState {
                ui_context,
                search_active: self.search_bar.active,
                help_modal_open: self.help_modal_open,
                selected_count: self.selected_set.len(),
                bulk_confirm_open: self.pending_bulk_confirm.is_some(),
                sync_status: self.last_sync_status.clone(),
                _marker: std::marker::PhantomData,
            },
            theme,
        );

        match self.screen {
            Screen::Mailbox => match self.layout_mode {
                LayoutMode::TwoPane => {
                    let chunks = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([Constraint::Percentage(20), Constraint::Percentage(80)])
                        .split(content_area);

                    ui::sidebar::draw(
                        frame,
                        chunks[0],
                        &ui::sidebar::SidebarView {
                            labels: &self.labels,
                            active_pane: &self.active_pane,
                            saved_searches: &self.saved_searches,
                            sidebar_selected: self.sidebar_selected,
                            all_mail_active: !self.search_active
                                && self.mailbox_view == MailboxView::Messages
                                && self.active_label.is_none()
                                && self.pending_active_label.is_none(),
                            subscriptions_active: self.mailbox_view == MailboxView::Subscriptions,
                            subscription_count: self.subscriptions_page.entries.len(),
                            system_expanded: self.sidebar_system_expanded,
                            user_expanded: self.sidebar_user_expanded,
                            saved_searches_expanded: self.sidebar_saved_searches_expanded,
                            active_label: self
                                .pending_active_label
                                .as_ref()
                                .or(self.active_label.as_ref()),
                        },
                        theme,
                    );

                    if self.mailbox_view == MailboxView::Subscriptions {
                        ui::subscriptions_page::draw(
                            frame,
                            chunks[1],
                            &ui::subscriptions_page::SubscriptionsPageView {
                                entries: &self.subscriptions_page.entries,
                                selected_index: self.selected_index,
                                scroll_offset: self.scroll_offset,
                                active_pane: &self.active_pane,
                                preview_blocks: &self.thread_message_blocks(),
                                message_scroll_offset: self.message_scroll_offset,
                            },
                            theme,
                        );
                    } else {
                        let mail_title = self.mail_list_title();
                        ui::mail_list::draw_view(
                            frame,
                            chunks[1],
                            &ui::mail_list::MailListView {
                                rows: &self.mail_list_rows(),
                                selected_index: self.selected_index,
                                scroll_offset: self.scroll_offset,
                                active_pane: &self.active_pane,
                                title: &mail_title,
                                selected_set: &self.selected_set,
                                mode: self.mail_list_mode,
                            },
                            theme,
                        );
                    }
                }
                LayoutMode::ThreePane => {
                    let chunks = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([Constraint::Percentage(15), Constraint::Percentage(85)])
                        .split(content_area);

                    ui::sidebar::draw(
                        frame,
                        chunks[0],
                        &ui::sidebar::SidebarView {
                            labels: &self.labels,
                            active_pane: &self.active_pane,
                            saved_searches: &self.saved_searches,
                            sidebar_selected: self.sidebar_selected,
                            all_mail_active: !self.search_active
                                && self.mailbox_view == MailboxView::Messages
                                && self.active_label.is_none()
                                && self.pending_active_label.is_none(),
                            subscriptions_active: self.mailbox_view == MailboxView::Subscriptions,
                            subscription_count: self.subscriptions_page.entries.len(),
                            system_expanded: self.sidebar_system_expanded,
                            user_expanded: self.sidebar_user_expanded,
                            saved_searches_expanded: self.sidebar_saved_searches_expanded,
                            active_label: self
                                .pending_active_label
                                .as_ref()
                                .or(self.active_label.as_ref()),
                        },
                        theme,
                    );

                    if self.mailbox_view == MailboxView::Subscriptions {
                        ui::subscriptions_page::draw(
                            frame,
                            chunks[1],
                            &ui::subscriptions_page::SubscriptionsPageView {
                                entries: &self.subscriptions_page.entries,
                                selected_index: self.selected_index,
                                scroll_offset: self.scroll_offset,
                                active_pane: &self.active_pane,
                                preview_blocks: &self.thread_message_blocks(),
                                message_scroll_offset: self.message_scroll_offset,
                            },
                            theme,
                        );
                    } else {
                        let inner = Layout::default()
                            .direction(Direction::Horizontal)
                            .constraints([Constraint::Percentage(41), Constraint::Percentage(59)])
                            .split(chunks[1]);
                        let mail_title = self.mail_list_title();
                        ui::mail_list::draw_view(
                            frame,
                            inner[0],
                            &ui::mail_list::MailListView {
                                rows: &self.mail_list_rows(),
                                selected_index: self.selected_index,
                                scroll_offset: self.scroll_offset,
                                active_pane: &self.active_pane,
                                title: &mail_title,
                                selected_set: &self.selected_set,
                                mode: self.mail_list_mode,
                            },
                            theme,
                        );
                        ui::message_view::draw(
                            frame,
                            inner[1],
                            &self.thread_message_blocks(),
                            self.message_scroll_offset,
                            &self.active_pane,
                            theme,
                        );
                    }
                }
                LayoutMode::FullScreen => {
                    if self.mailbox_view == MailboxView::Subscriptions {
                        ui::subscriptions_page::draw(
                            frame,
                            content_area,
                            &ui::subscriptions_page::SubscriptionsPageView {
                                entries: &self.subscriptions_page.entries,
                                selected_index: self.selected_index,
                                scroll_offset: self.scroll_offset,
                                active_pane: &self.active_pane,
                                preview_blocks: &self.thread_message_blocks(),
                                message_scroll_offset: self.message_scroll_offset,
                            },
                            theme,
                        );
                    } else {
                        ui::message_view::draw(
                            frame,
                            content_area,
                            &self.thread_message_blocks(),
                            self.message_scroll_offset,
                            &self.active_pane,
                            theme,
                        );
                    }
                }
            },
            Screen::Search => {
                let rows = self.search_mail_list_rows();
                ui::search_page::draw(
                    frame,
                    content_area,
                    &self.search_page,
                    &rows,
                    &self.selected_set,
                    self.search_list_mode(),
                    &self.thread_message_blocks(),
                    self.message_scroll_offset,
                    theme,
                );
            }
            Screen::Rules => {
                ui::rules_page::draw(
                    frame,
                    content_area,
                    &self.rules_page,
                    &self.rule_condition_editor,
                    &self.rule_action_editor,
                    theme,
                );
            }
            Screen::Diagnostics => {
                ui::diagnostics_page::draw(frame, content_area, &self.diagnostics_page, theme);
            }
            Screen::Accounts => {
                ui::accounts_page::draw(frame, content_area, &self.accounts_page, theme);
            }
        }

        let status_bar = self.status_bar_state();
        ui::status_bar::draw(frame, bottom_bar_area, &status_bar, theme);

        if self.search_bar.active {
            ui::search_bar::draw(frame, area, &self.search_bar, theme);
        }

        // Command palette overlay
        ui::command_palette::draw(frame, area, &self.command_palette, theme);

        // Label picker overlay
        ui::label_picker::draw(frame, area, &self.label_picker, theme);

        // Compose picker overlay
        ui::compose_picker::draw(frame, area, &self.compose_picker, theme);

        // Attachment overlay
        ui::attachment_modal::draw(frame, area, &self.attachment_panel, theme);

        // URL picker overlay
        ui::url_modal::draw(frame, area, self.url_modal.as_ref(), theme);

        // Snooze overlay
        ui::snooze_modal::draw(frame, area, &self.snooze_panel, &self.snooze_config, theme);

        // Send confirmation overlay
        ui::send_confirm_modal::draw(frame, area, self.pending_send_confirm.as_ref(), theme);

        // Bulk confirmation overlay
        ui::bulk_confirm_modal::draw(frame, area, self.pending_bulk_confirm.as_ref(), theme);

        // Error overlay
        ui::error_modal::draw(frame, area, self.error_modal.as_ref(), theme);

        // Unsubscribe confirmation overlay
        ui::unsubscribe_modal::draw(
            frame,
            area,
            self.pending_unsubscribe_confirm.as_ref(),
            theme,
        );

        // Help overlay
        ui::help_modal::draw(
            frame,
            area,
            ui::help_modal::HelpModalState {
                open: self.help_modal_open,
                ui_context,
                selected_count: self.selected_set.len(),
                scroll_offset: self.help_scroll_offset,
                query: &self.help_query,
                selected: self.help_selected,
                _marker: std::marker::PhantomData,
            },
            theme,
        );

        ui::onboarding_modal::draw(frame, area, &self.onboarding, theme);
    }
}

#[cfg(test)]
mod tests {
    use super::tab_titles;

    #[test]
    fn tab_titles_include_numeric_shortcuts() {
        assert_eq!(
            tab_titles(),
            [
                "1 Mailbox",
                "2 Search",
                "3 Rules",
                "4 Accounts",
                "5 Diagnostics",
            ]
        );
    }
}
