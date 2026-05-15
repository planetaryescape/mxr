use super::*;

fn tab_titles() -> [&'static str; 6] {
    [
        "1 Mailbox",
        "2 Search",
        "3 Rules",
        "4 Accounts",
        "5 Diagnostics",
        "6 Analytics",
    ]
}

fn selected_tab(screen: Screen) -> usize {
    match screen {
        Screen::Mailbox => 0,
        Screen::Search => 1,
        Screen::Rules => 2,
        Screen::Accounts => 3,
        Screen::Diagnostics => 4,
        Screen::Analytics => 5,
    }
}

impl App {
    fn thread_summary_block(&self) -> Option<ui::message_view::ThreadSummaryBlock> {
        let current_thread_id = self.context_envelope().map(|env| env.thread_id.clone())?;
        let loading = self
            .mailbox
            .thread_summary_loading
            .as_ref()
            .is_some_and(|thread_id| thread_id == &current_thread_id);
        let text = self
            .mailbox
            .thread_summary
            .as_ref()
            .map(|summary| summary.text.clone());
        let model = self
            .mailbox
            .thread_summary
            .as_ref()
            .map(|summary| summary.model.clone());
        let error = self.mailbox.thread_summary_error.clone();

        (loading || text.is_some() || error.is_some()).then_some(
            ui::message_view::ThreadSummaryBlock {
                text,
                model,
                loading,
                error,
            },
        )
    }

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
        let viewing_invite = self
            .focused_thread_envelope()
            .and_then(|env| self.mailbox.body_cache.get(&env.id))
            .is_some_and(|body| body.metadata.calendar.is_some());
        ui::hint_bar::draw(
            frame,
            hint_bar_area,
            ui::hint_bar::HintBarState {
                ui_context,
                search_active: self.search.bar.active,
                help_modal_open: self.modals.help_open,
                selected_count: self.mailbox.selected_set.len(),
                bulk_confirm_open: self.modals.pending_bulk_confirm.is_some(),
                sync_status: self.last_sync_status.clone(),
                viewing_invite,
                _marker: std::marker::PhantomData,
            },
            theme,
        );

        match self.screen {
            Screen::Mailbox => {
                let content_area = self.draw_saved_search_tabs(frame, content_area, theme);
                match self.mailbox.layout_mode {
                    LayoutMode::TwoPane => {
                        let chunks = Layout::default()
                            .direction(Direction::Horizontal)
                            .constraints([Constraint::Percentage(20), Constraint::Percentage(80)])
                            .split(content_area);

                        ui::sidebar::draw(frame, chunks[0], &self.sidebar_view(), theme);

                        if self.mailbox.mailbox_view == MailboxView::Subscriptions {
                            let preview_blocks = self.thread_message_blocks();
                            ui::subscriptions_page::draw(
                                frame,
                                chunks[1],
                                &mut ui::subscriptions_page::SubscriptionsPageView {
                                    entries: &self.mailbox.subscriptions_page.entries,
                                    selected_index: self.mailbox.selected_index,
                                    scroll_offset: self.mailbox.scroll_offset,
                                    active_pane: &self.mailbox.active_pane,
                                    preview_blocks: &preview_blocks,
                                    message_scroll_offset: self.mailbox.message_scroll_offset,
                                    html_images: &mut self.html_image_assets,
                                },
                                theme,
                            );
                        } else if self.mailbox.mailbox_view == MailboxView::Owed {
                            ui::owed_lens::draw(
                                frame,
                                chunks[1],
                                &self.mailbox.owed_page.entries,
                                theme,
                            );
                        } else {
                            let mail_title = self.mail_list_title();
                            ui::mail_list::draw_view(
                                frame,
                                chunks[1],
                                &ui::mail_list::MailListView {
                                    rows: &self.mail_list_rows(),
                                    selected_index: self.mailbox.selected_index,
                                    scroll_offset: self.mailbox.scroll_offset,
                                    active_pane: &self.mailbox.active_pane,
                                    title: &mail_title,
                                    selected_set: &self.mailbox.selected_set,
                                    mode: self.mailbox.mail_list_mode,
                                    loading_message: self
                                        .mailbox
                                        .mailbox_loading_message
                                        .as_deref(),
                                    loading_throbber: self
                                        .mailbox
                                        .mailbox_loading_message
                                        .as_ref()
                                        .map(|_| &self.mailbox.mailbox_loading_throbber),
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

                        ui::sidebar::draw(frame, chunks[0], &self.sidebar_view(), theme);

                        if self.mailbox.mailbox_view == MailboxView::Subscriptions {
                            let preview_blocks = self.thread_message_blocks();
                            ui::subscriptions_page::draw(
                                frame,
                                chunks[1],
                                &mut ui::subscriptions_page::SubscriptionsPageView {
                                    entries: &self.mailbox.subscriptions_page.entries,
                                    selected_index: self.mailbox.selected_index,
                                    scroll_offset: self.mailbox.scroll_offset,
                                    active_pane: &self.mailbox.active_pane,
                                    preview_blocks: &preview_blocks,
                                    message_scroll_offset: self.mailbox.message_scroll_offset,
                                    html_images: &mut self.html_image_assets,
                                },
                                theme,
                            );
                        } else if self.mailbox.mailbox_view == MailboxView::Owed {
                            ui::owed_lens::draw(
                                frame,
                                chunks[1],
                                &self.mailbox.owed_page.entries,
                                theme,
                            );
                        } else {
                            let inner = Layout::default()
                                .direction(Direction::Horizontal)
                                .constraints([
                                    Constraint::Percentage(41),
                                    Constraint::Percentage(59),
                                ])
                                .split(chunks[1]);
                            let mail_title = self.mail_list_title();
                            ui::mail_list::draw_view(
                                frame,
                                inner[0],
                                &ui::mail_list::MailListView {
                                    rows: &self.mail_list_rows(),
                                    selected_index: self.mailbox.selected_index,
                                    scroll_offset: self.mailbox.scroll_offset,
                                    active_pane: &self.mailbox.active_pane,
                                    title: &mail_title,
                                    selected_set: &self.mailbox.selected_set,
                                    mode: self.mailbox.mail_list_mode,
                                    loading_message: self
                                        .mailbox
                                        .mailbox_loading_message
                                        .as_deref(),
                                    loading_throbber: self
                                        .mailbox
                                        .mailbox_loading_message
                                        .as_ref()
                                        .map(|_| &self.mailbox.mailbox_loading_throbber),
                                },
                                theme,
                            );
                            let preview_blocks = self.thread_message_blocks();
                            let summary = self.thread_summary_block();
                            ui::message_view::draw(
                                frame,
                                inner[1],
                                &preview_blocks,
                                summary,
                                self.mailbox.message_scroll_offset,
                                &self.mailbox.active_pane,
                                theme,
                                &mut self.html_image_assets,
                            );
                        }
                    }
                    LayoutMode::FullScreen => {
                        let chunks = Layout::default()
                            .direction(Direction::Horizontal)
                            .constraints([Constraint::Percentage(15), Constraint::Percentage(85)])
                            .split(content_area);

                        ui::sidebar::draw(frame, chunks[0], &self.sidebar_view(), theme);

                        let preview_blocks = self.thread_message_blocks();
                        let summary = self.thread_summary_block();
                        ui::message_view::draw(
                            frame,
                            chunks[1],
                            &preview_blocks,
                            summary,
                            self.mailbox.message_scroll_offset,
                            &self.mailbox.active_pane,
                            theme,
                            &mut self.html_image_assets,
                        );
                    }
                }
            }
            Screen::Search => {
                let rows = self.search_mail_list_rows();
                let preview_blocks = self.thread_message_blocks();
                ui::search_page::draw(
                    frame,
                    content_area,
                    &self.search.page,
                    &rows,
                    &self.mailbox.selected_set,
                    self.search_list_mode(),
                    &preview_blocks,
                    self.mailbox.message_scroll_offset,
                    &mut self.html_image_assets,
                    theme,
                );
            }
            Screen::Rules => {
                ui::rules_page::draw(
                    frame,
                    content_area,
                    &self.rules.page,
                    &self.rules.condition_editor,
                    &self.rules.action_editor,
                    theme,
                );
            }
            Screen::Diagnostics => {
                ui::diagnostics_page::draw(frame, content_area, &self.diagnostics.page, theme);
            }
            Screen::Accounts => {
                ui::accounts_page::draw(
                    frame,
                    content_area,
                    &self.accounts.page,
                    &self.diagnostics.page.sync_statuses,
                    theme,
                );
            }
            Screen::Analytics => {
                ui::analytics_page::draw(frame, content_area, &self.analytics, theme);
            }
        }

        let status_bar = self.status_bar_state();
        ui::status_bar::draw(frame, bottom_bar_area, &status_bar, theme);

        if self.search.bar.active {
            ui::search_bar::draw(frame, area, &self.search.bar, theme);
        }

        // Command palette overlay
        ui::command_palette::draw(frame, area, &self.command_palette.palette, theme);

        // Label picker overlay
        ui::label_picker::draw(frame, area, &self.modals.label_picker, theme);

        // Compose picker overlay
        ui::compose_picker::draw(frame, area, &self.compose.compose_picker, theme);

        // Attachment overlay
        ui::attachment_modal::draw(frame, area, &self.mailbox.attachment_panel, theme);

        // Save-attachment "where to?" modal — drawn after the attachment
        // modal so it appears on top.
        ui::save_attachment_modal::draw(frame, area, &self.modals.save_attachment, theme);

        // URL picker overlay
        ui::url_modal::draw(frame, area, self.mailbox.url_modal.as_ref(), theme);

        // Snooze overlay
        ui::snooze_modal::draw(
            frame,
            area,
            &self.modals.snooze_panel,
            &self.modals.snooze_config,
            theme,
        );

        // Send confirmation overlay
        ui::send_confirm_modal::draw(
            frame,
            area,
            self.compose.pending_send_confirm.as_ref(),
            self.compose.pending_send_at_input.as_deref(),
            self.compose.pending_remind_at_input.as_deref(),
            theme,
        );

        // Bulk confirmation overlay
        ui::bulk_confirm_modal::draw(
            frame,
            area,
            self.modals.pending_bulk_confirm.as_ref(),
            theme,
        );

        // Saved-search form overlay (above sidebar/mail list, below modals)
        ui::saved_search_form::draw(frame, area, self.modals.saved_search_form.as_ref(), theme);

        // Saved-search delete confirm overlay
        ui::saved_search_form::draw_delete_confirm(
            frame,
            area,
            self.modals.pending_saved_search_delete_confirm.as_deref(),
            theme,
        );

        // Analytics filter modal overlay
        ui::analytics_filter_modal::draw(frame, area, self.modals.analytics_filter.as_ref(), theme);

        // Error overlay
        ui::error_modal::draw(frame, area, self.modals.error.as_ref(), theme);

        // Unsubscribe confirmation overlay
        ui::unsubscribe_modal::draw(
            frame,
            area,
            self.modals.pending_unsubscribe_confirm.as_ref(),
            theme,
        );

        // Help overlay
        ui::help_modal::draw(
            frame,
            area,
            ui::help_modal::HelpModalState {
                open: self.modals.help_open,
                ui_context,
                selected_count: self.mailbox.selected_set.len(),
                scroll_offset: self.modals.help_scroll_offset,
                query: &self.modals.help_query,
                selected: self.modals.help_selected,
                _marker: std::marker::PhantomData,
            },
            theme,
        );

        ui::onboarding_modal::draw(frame, area, &self.modals.onboarding, theme);

        // Snippets browser modal — shown above mailbox/search and below
        // the connection error / global onboarding modal.
        ui::snippets_modal::draw(frame, area, &self.modals.snippets, theme);

        // Sender profile modal — same render layer as snippets.
        ui::sender_profile_modal::draw(frame, area, &self.modals.sender_profile, theme);

        // Screener triage modal — same layer as the others.
        ui::screener_modal::draw(frame, area, &self.modals.screener, theme);

        // Reply-later queue modal.
        ui::reply_queue_modal::draw(frame, area, &self.modals.reply_queue, theme);

        // Activity log modal (Phase 5).
        ui::activity_modal::draw(frame, area, &self.modals.activity, theme);

        // Thread summary modal (LLM result).
        ui::summary_modal::draw(frame, area, &self.modals.summary, theme);

        // Slice 5.1/5.2 (C2.6): briefing modal (returning to a
        // dormant thread or recipient).
        ui::briefing_modal::draw(frame, area, &self.modals.briefing, theme);

        // Slice 6.1 (C2.9): whois modal (entity explanation).
        ui::whois_modal::draw(frame, area, &self.modals.whois, theme);

        // Slice 5.4 (C2.8 cont): expert-finder modal.
        ui::expert_modal::draw(frame, area, &self.modals.expert, theme);

        // Generic platform/AI result modal (voice, commitments, draft suggestions).
        ui::platform_modal::draw(frame, area, &self.modals.platform, theme);

        // Account setup onboarding (shown on any page when no accounts configured)
        if self.accounts.page.onboarding_modal_open {
            ui::accounts_page::draw_account_setup_onboarding(frame, area, theme);
        }
    }

    fn draw_saved_search_tabs(&self, frame: &mut Frame, area: Rect, theme: &Theme) -> Rect {
        if self.mailbox.saved_searches.is_empty()
            || self.mailbox.mailbox_view == MailboxView::Subscriptions
        {
            return area;
        }

        // Height of 2 = 1 row for the labels + 1 row for the bottom
        // border drawn by saved_search_tabs::draw (Borders::BOTTOM).
        // Earlier this was Length(1), which produced an invisible strip
        // because the border consumed the only row.
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Min(0)])
            .split(area);
        let active_query = self.search.active.then_some(self.search.bar.query.as_str());
        ui::saved_search_tabs::draw(
            frame,
            chunks[0],
            &ui::saved_search_tabs::SavedSearchTabsView {
                searches: &self.mailbox.saved_searches,
                active_query,
                active_mode: self.search.active.then_some(self.search.bar.mode),
                unread_counts: &self.mailbox.saved_search_unread_counts,
            },
            theme,
        );
        chunks[1]
    }
}

#[cfg(test)]
mod tests {
    use super::{selected_tab, tab_titles, Screen};

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
                "6 Analytics",
            ]
        );
    }

    /// Slice 1 / B1.3: the screen → tab-index map is what the renderer
    /// hands to ratatui's `Tabs::select`. Each top-level screen owns
    /// exactly one tab index, and Analytics owns the new 6th slot.
    /// Before this slice, `Screen::Analytics` mapped to index 0 so the
    /// Mailbox tab lit up while the user was on Analytics — confusing
    /// enough that the screen had to stay hidden behind the command
    /// palette. This test pins the new mapping so a future `_ => 0`
    /// fallback can't silently re-introduce the bug.
    #[test]
    fn selected_tab_maps_each_screen_to_its_tab_index() {
        assert_eq!(selected_tab(Screen::Mailbox), 0);
        assert_eq!(selected_tab(Screen::Search), 1);
        assert_eq!(selected_tab(Screen::Rules), 2);
        assert_eq!(selected_tab(Screen::Accounts), 3);
        assert_eq!(selected_tab(Screen::Diagnostics), 4);
        assert_eq!(selected_tab(Screen::Analytics), 5);
    }
}
