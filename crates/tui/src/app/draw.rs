use super::*;

fn tab_titles() -> [&'static str; 7] {
    [
        "g i Mailbox",
        "/ Search",
        "g R Rules",
        "g C Accounts",
        "g D Diagnostics",
        "g A Analytics",
        "g P Deliveries",
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
        Screen::Deliveries => 6,
    }
}

const MIN_TERMINAL_WIDTH: u16 = 80;
const MIN_TERMINAL_HEIGHT: u16 = 20;

const ASSISTANT_COLUMN_MIN_WIDTH: u16 = 110;

fn assistant_layout(area: Rect) -> (Rect, Rect) {
    if area.width >= ASSISTANT_COLUMN_MIN_WIDTH {
        let width = (area.width * 34 / 100).clamp(44, 64);
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(width)])
            .split(area);
        (chunks[0], chunks[1])
    } else {
        let width = (area.width * 60 / 100).max(44).min(area.width);
        let pane = Rect {
            x: area.right() - width,
            width,
            ..area
        };
        (area, pane)
    }
}

struct TodoReaderContent {
    preview_blocks: Vec<ui::message_view::ThreadMessageBlock>,
    summary: Option<ui::message_view::ThreadSummaryBlock>,
    context: Vec<ratatui::text::Line<'static>>,
}

fn draw_terminal_too_small(frame: &mut Frame, area: Rect, theme: &Theme) {
    let lines = vec![
        Line::from(Span::styled(
            "Terminal too small",
            Style::default().fg(theme.warning).bold(),
        )),
        Line::from(Span::styled(
            format!(
                "Need at least {MIN_TERMINAL_WIDTH}x{MIN_TERMINAL_HEIGHT}, have {}x{}",
                area.width, area.height
            ),
            Style::default().fg(theme.text_secondary),
        )),
        Line::from(Span::styled(
            "Resize the window to continue",
            Style::default().fg(theme.text_muted),
        )),
    ];
    let top_pad = area.height.saturating_sub(lines.len() as u16) / 2;
    let mut target = area;
    target.y += top_pad;
    target.height = area.height.saturating_sub(top_pad);
    let paragraph = ratatui::widgets::Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(paragraph, target);
}

impl App {
    fn todo_reader_content(&self) -> Option<TodoReaderContent> {
        (self.mailbox.layout_mode == LayoutMode::ThreePane
            && self.mailbox.active_pane == ActivePane::MessageView
            && self.mailbox.viewing_envelope.is_some())
        .then(|| TodoReaderContent {
            preview_blocks: self.thread_message_blocks(),
            summary: self.thread_summary_block(),
            context: self.thread_context_lines(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_todo_content(
        frame: &mut Frame,
        area: Rect,
        view: ui::todo_lens::TodoView<'_>,
        reader: Option<TodoReaderContent>,
        message_scroll_offset: &mut u16,
        html_image_assets: &mut std::collections::HashMap<
            mxr_core::id::MessageId,
            std::collections::HashMap<String, crate::terminal_images::HtmlImageEntry>,
        >,
        account_marks: &crate::ui::account_mark::AccountMarks,
        theme: &Theme,
    ) {
        if let Some(reader) = reader {
            *message_scroll_offset = ui::message_view::draw(
                frame,
                area,
                &reader.preview_blocks,
                ui::message_view::DrawOptions {
                    zone: crate::ui::received_label::DisplayZone::Local,
                    context: reader.context,
                    summary: reader.summary,
                    scroll_offset: *message_scroll_offset,
                    active_pane: view.active_pane,
                    theme,
                    html_images: html_image_assets,
                    now: chrono::Utc::now(),
                    account_marks,
                },
            );
        } else {
            ui::todo_lens::draw(frame, area, &view, theme);
        }
    }

    fn thread_context_lines(&self) -> Vec<ratatui::text::Line<'static>> {
        let Some(thread_id) = self.context_envelope().map(|env| env.thread_id.clone()) else {
            return Vec::new();
        };
        let context = self
            .mailbox
            .thread_context
            .as_ref()
            .filter(|context| context.thread_id == thread_id);
        let gist = self
            .mailbox
            .thread_gist
            .as_ref()
            .filter(|gist| gist.thread_id == thread_id);
        ui::thread_context_view::context_lines(context, gist, &self.theme, chrono::Utc::now())
    }

    fn thread_summary_block(&self) -> Option<ui::message_view::ThreadSummaryBlock> {
        let current_thread_id = self.context_envelope().map(|env| env.thread_id.clone())?;
        let loading = self
            .mailbox
            .thread_summary_in_flight
            .contains(&current_thread_id);
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
        self.draw_frame(frame);
        self.mailbox.scroll_to_focused_message = false;
    }

    fn draw_frame(&mut self, frame: &mut Frame) {
        self.mailbox.todo_page.microsoft_review_visible.set(false);
        self.mailbox
            .todo_page
            .microsoft_sync_review_visible
            .set(false);
        let theme = &self.theme;
        let area = frame.area();
        let account_marks = self.account_marks();

        if area.width < MIN_TERMINAL_WIDTH || area.height < MIN_TERMINAL_HEIGHT {
            draw_terminal_too_small(frame, area, theme);
            return;
        }

        let outer_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(2),
                Constraint::Min(0),
                Constraint::Length(1),
            ])
            .split(area);

        let tab_bar_area = outer_chunks[0];
        let hint_bar_area = outer_chunks[1];
        let content_area = outer_chunks[2];
        let ui_context = self.current_ui_context();
        self.visible_height = match self.screen {
            Screen::Search => content_area.height.saturating_sub(6) as usize,
            _ => content_area.height.saturating_sub(2) as usize,
        };
        let bottom_bar_area = outer_chunks[3];
        let (content_area, assistant_area) = if self.assistant.open {
            let (content, pane) = assistant_layout(content_area);
            (content, Some(pane))
        } else {
            (content_area, None)
        };

        let tabs = ratatui::widgets::Tabs::new(tab_titles())
            .select(selected_tab(self.screen))
            .style(Style::default().fg(theme.text_muted))
            .highlight_style(Style::default().fg(theme.accent).bold())
            .divider(Span::styled(" | ", Style::default().fg(theme.text_muted)));
        frame.render_widget(tabs, tab_bar_area);

        let viewing_invite = self
            .focused_thread_envelope()
            .and_then(|env| self.mailbox.body_cache.get(&env.id))
            .is_some_and(|body| body.metadata.calendar.is_some());
        ui::hint_bar::draw(
            frame,
            hint_bar_area,
            ui::hint_bar::HintBarState {
                keybindings: Some(&self.keybindings),
                binding_context: Some(self.keybinding_context()),
                ui_context,
                search_active: self.search.bar.active,
                help_modal_open: self.modals.help_open,
                selected_count: self.mailbox.selected_set.len(),
                bulk_confirm_open: self.modals.pending_bulk_confirm.is_some(),
                sync_status: self.last_sync_status.clone(),
                viewing_invite,
                pending_prefix: self.pending_input_prefix(),
                mode_strip: self.mode_strip_hints(),
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
                                    zone: crate::ui::received_label::DisplayZone::Local,
                                    entries: &self.mailbox.subscriptions_page.entries,
                                    selected_index: self.mailbox.selected_index,
                                    scroll_offset: self.mailbox.scroll_offset,
                                    active_pane: &self.mailbox.active_pane,
                                    preview_blocks: &preview_blocks,
                                    message_scroll_offset: self.mailbox.message_scroll_offset,
                                    html_images: &mut self.html_image_assets,
                                    account_marks: &account_marks,
                                    now: chrono::Utc::now(),
                                },
                                theme,
                            );
                        } else if self.mailbox.mailbox_view == MailboxView::Desk {
                            ui::desk_lens::draw(
                                frame,
                                chunks[1],
                                &ui::desk_lens::DeskView {
                                    desk: &self.mailbox.desk_page,
                                    selected_index: self.mailbox.selected_index,
                                    active_pane: &self.mailbox.active_pane,
                                    row_gists: &self.row_gists.gists,
                                    gist_lines: self.row_gists.lines_reserved(),
                                    quiet: self.rail_entry("messages").and_then(|e| e.quiet),
                                    account_marks: account_marks.clone(),
                                },
                                theme,
                            );
                        } else if let MailboxView::Place(place) = self.mailbox.mailbox_view {
                            ui::place_lens::draw(
                                frame,
                                chunks[1],
                                &ui::place_lens::PlaceView {
                                    zone: crate::ui::received_label::DisplayZone::Local,
                                    page: &self.mailbox.place_page,
                                    place,
                                    selected_index: self.mailbox.selected_index,
                                    active_pane: &self.mailbox.active_pane,
                                    account_marks: &account_marks,
                                },
                                theme,
                            );
                        } else if self.mailbox.mailbox_view == MailboxView::Now {
                            ui::now_lens::draw(
                                frame,
                                chunks[1],
                                &ui::now_lens::NowView {
                                    now: chrono::Utc::now(),
                                    page: &self.mailbox.now_page,
                                    selected_index: self.mailbox.selected_index,
                                    active_pane: &self.mailbox.active_pane,
                                    updates_since: self.now_updates_since(),
                                    account_marks: account_marks.clone(),
                                },
                                theme,
                            );
                        } else if self.mailbox.mailbox_view == MailboxView::Focus {
                            ui::focus_lens::draw(frame, chunks[1], &self.focus_view(), theme);
                        } else if self.mailbox.mailbox_view == MailboxView::People {
                            ui::messages_lens::draw(frame, chunks[1], &self.messages_view(), theme);
                        } else if self.mailbox.mailbox_view == MailboxView::Updates {
                            ui::updates_lens::draw(
                                frame,
                                chunks[1],
                                &ui::updates_lens::UpdatesView {
                                    now: chrono::Utc::now(),
                                    page: &self.mailbox.updates_page,
                                    selected_index: self.mailbox.selected_index,
                                    active_pane: &self.mailbox.active_pane,
                                    account_marks: account_marks.clone(),
                                    peek_body: self.updates_peek_body(),
                                },
                                theme,
                            );
                        } else if self.mailbox.mailbox_view == MailboxView::ArchiveMode {
                            ui::records_lens::draw(
                                frame,
                                chunks[1],
                                &ui::records_lens::RecordsView {
                                    zone: crate::ui::received_label::DisplayZone::Local,
                                    now: chrono::Utc::now(),
                                    page: &self.mailbox.records_page,
                                    selected_index: self.mailbox.selected_index,
                                    active_pane: &self.mailbox.active_pane,
                                    account_marks: account_marks.clone(),
                                },
                                theme,
                            );
                        } else if self.mailbox.mailbox_view == MailboxView::Reading {
                            ui::reading_lens::draw(
                                frame,
                                chunks[1],
                                &ui::reading_lens::ReadingLensView {
                                    zone: crate::ui::received_label::DisplayZone::Local,
                                    now: chrono::Utc::now(),
                                    page: &self.mailbox.reading_page,
                                    selected_index: self.mailbox.selected_index,
                                    active_pane: &self.mailbox.active_pane,
                                },
                                theme,
                            );
                        } else if self.mailbox.mailbox_view == MailboxView::Todo {
                            let reader = self.todo_reader_content();
                            Self::draw_todo_content(
                                frame,
                                chunks[1],
                                ui::todo_lens::TodoView {
                                    zone: crate::ui::received_label::DisplayZone::Local,
                                    now: chrono::Utc::now(),
                                    page: &self.mailbox.todo_page,
                                    selected_index: self.mailbox.selected_index,
                                    active_pane: &self.mailbox.active_pane,
                                    account_marks: account_marks.clone(),
                                    peek_body: self
                                        .mailbox
                                        .todo_page
                                        .peek_body(&self.mailbox.body_cache),
                                },
                                reader,
                                &mut self.mailbox.message_scroll_offset,
                                &mut self.html_image_assets,
                                &account_marks,
                                theme,
                            );
                        } else if self.mailbox.mailbox_view == MailboxView::Owed {
                            ui::owed_lens::draw(
                                frame,
                                chunks[1],
                                &self.mailbox.owed_page.entries,
                                self.mailbox.owed_page.people,
                                &account_marks,
                                theme,
                            );
                        } else if self.mailbox.mailbox_view == MailboxView::CalendarInvites {
                            ui::calendar_invites_lens::draw(
                                frame,
                                chunks[1],
                                &ui::calendar_invites_lens::CalendarInvitesView {
                                    entries: &self.mailbox.calendar_invites_page.entries,
                                    selected_index: self.mailbox.selected_index,
                                    scroll_offset: self.mailbox.scroll_offset,
                                    active_pane: &self.mailbox.active_pane,
                                    account_marks: account_marks.clone(),
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
                                    row_gists: &self.row_gists.gists,
                                    account_marks: &account_marks,
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
                                    zone: crate::ui::received_label::DisplayZone::Local,
                                    entries: &self.mailbox.subscriptions_page.entries,
                                    selected_index: self.mailbox.selected_index,
                                    scroll_offset: self.mailbox.scroll_offset,
                                    active_pane: &self.mailbox.active_pane,
                                    preview_blocks: &preview_blocks,
                                    message_scroll_offset: self.mailbox.message_scroll_offset,
                                    html_images: &mut self.html_image_assets,
                                    account_marks: &account_marks,
                                    now: chrono::Utc::now(),
                                },
                                theme,
                            );
                        } else if self.mailbox.mailbox_view == MailboxView::Desk {
                            ui::desk_lens::draw(
                                frame,
                                chunks[1],
                                &ui::desk_lens::DeskView {
                                    desk: &self.mailbox.desk_page,
                                    selected_index: self.mailbox.selected_index,
                                    active_pane: &self.mailbox.active_pane,
                                    row_gists: &self.row_gists.gists,
                                    gist_lines: self.row_gists.lines_reserved(),
                                    quiet: self.rail_entry("messages").and_then(|e| e.quiet),
                                    account_marks: account_marks.clone(),
                                },
                                theme,
                            );
                        } else if let MailboxView::Place(place) = self.mailbox.mailbox_view {
                            ui::place_lens::draw(
                                frame,
                                chunks[1],
                                &ui::place_lens::PlaceView {
                                    zone: crate::ui::received_label::DisplayZone::Local,
                                    page: &self.mailbox.place_page,
                                    place,
                                    selected_index: self.mailbox.selected_index,
                                    active_pane: &self.mailbox.active_pane,
                                    account_marks: &account_marks,
                                },
                                theme,
                            );
                        } else if self.mailbox.mailbox_view == MailboxView::Now {
                            ui::now_lens::draw(
                                frame,
                                chunks[1],
                                &ui::now_lens::NowView {
                                    now: chrono::Utc::now(),
                                    page: &self.mailbox.now_page,
                                    selected_index: self.mailbox.selected_index,
                                    active_pane: &self.mailbox.active_pane,
                                    updates_since: self.now_updates_since(),
                                    account_marks: account_marks.clone(),
                                },
                                theme,
                            );
                        } else if self.mailbox.mailbox_view == MailboxView::Focus {
                            ui::focus_lens::draw(frame, chunks[1], &self.focus_view(), theme);
                        } else if self.mailbox.mailbox_view == MailboxView::People {
                            ui::messages_lens::draw(frame, chunks[1], &self.messages_view(), theme);
                        } else if self.mailbox.mailbox_view == MailboxView::Updates {
                            ui::updates_lens::draw(
                                frame,
                                chunks[1],
                                &ui::updates_lens::UpdatesView {
                                    now: chrono::Utc::now(),
                                    page: &self.mailbox.updates_page,
                                    selected_index: self.mailbox.selected_index,
                                    active_pane: &self.mailbox.active_pane,
                                    account_marks: account_marks.clone(),
                                    peek_body: self.updates_peek_body(),
                                },
                                theme,
                            );
                        } else if self.mailbox.mailbox_view == MailboxView::ArchiveMode {
                            ui::records_lens::draw(
                                frame,
                                chunks[1],
                                &ui::records_lens::RecordsView {
                                    zone: crate::ui::received_label::DisplayZone::Local,
                                    now: chrono::Utc::now(),
                                    page: &self.mailbox.records_page,
                                    selected_index: self.mailbox.selected_index,
                                    active_pane: &self.mailbox.active_pane,
                                    account_marks: account_marks.clone(),
                                },
                                theme,
                            );
                        } else if self.mailbox.mailbox_view == MailboxView::Reading {
                            ui::reading_lens::draw(
                                frame,
                                chunks[1],
                                &ui::reading_lens::ReadingLensView {
                                    zone: crate::ui::received_label::DisplayZone::Local,
                                    now: chrono::Utc::now(),
                                    page: &self.mailbox.reading_page,
                                    selected_index: self.mailbox.selected_index,
                                    active_pane: &self.mailbox.active_pane,
                                },
                                theme,
                            );
                        } else if self.mailbox.mailbox_view == MailboxView::Todo {
                            let reader = self.todo_reader_content();
                            Self::draw_todo_content(
                                frame,
                                chunks[1],
                                ui::todo_lens::TodoView {
                                    zone: crate::ui::received_label::DisplayZone::Local,
                                    now: chrono::Utc::now(),
                                    page: &self.mailbox.todo_page,
                                    selected_index: self.mailbox.selected_index,
                                    active_pane: &self.mailbox.active_pane,
                                    account_marks: account_marks.clone(),
                                    peek_body: self
                                        .mailbox
                                        .todo_page
                                        .peek_body(&self.mailbox.body_cache),
                                },
                                reader,
                                &mut self.mailbox.message_scroll_offset,
                                &mut self.html_image_assets,
                                &account_marks,
                                theme,
                            );
                        } else if self.mailbox.mailbox_view == MailboxView::Owed {
                            ui::owed_lens::draw(
                                frame,
                                chunks[1],
                                &self.mailbox.owed_page.entries,
                                self.mailbox.owed_page.people,
                                &account_marks,
                                theme,
                            );
                        } else if self.mailbox.mailbox_view == MailboxView::CalendarInvites {
                            ui::calendar_invites_lens::draw(
                                frame,
                                chunks[1],
                                &ui::calendar_invites_lens::CalendarInvitesView {
                                    entries: &self.mailbox.calendar_invites_page.entries,
                                    selected_index: self.mailbox.selected_index,
                                    scroll_offset: self.mailbox.scroll_offset,
                                    active_pane: &self.mailbox.active_pane,
                                    account_marks: account_marks.clone(),
                                },
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
                                    row_gists: &self.row_gists.gists,
                                    account_marks: &account_marks,
                                },
                                theme,
                            );
                            let preview_blocks = self.thread_message_blocks();
                            let summary = self.thread_summary_block();
                            let context = self.thread_context_lines();
                            let clamped_scroll = ui::message_view::draw(
                                frame,
                                inner[1],
                                &preview_blocks,
                                ui::message_view::DrawOptions {
                                    zone: crate::ui::received_label::DisplayZone::Local,
                                    context,
                                    summary,
                                    scroll_offset: self.mailbox.message_scroll_offset,
                                    active_pane: &self.mailbox.active_pane,
                                    theme,
                                    html_images: &mut self.html_image_assets,
                                    account_marks: &account_marks,
                                    now: chrono::Utc::now(),
                                },
                            );
                            self.mailbox.message_scroll_offset = clamped_scroll;
                        }
                    }
                    LayoutMode::FullScreen => {
                        let preview_blocks = self.thread_message_blocks();
                        let summary = self.thread_summary_block();
                        let context = self.thread_context_lines();
                        let clamped_scroll = ui::message_view::draw(
                            frame,
                            content_area,
                            &preview_blocks,
                            ui::message_view::DrawOptions {
                                zone: crate::ui::received_label::DisplayZone::Local,
                                context,
                                summary,
                                scroll_offset: self.mailbox.message_scroll_offset,
                                active_pane: &self.mailbox.active_pane,
                                theme,
                                html_images: &mut self.html_image_assets,
                                account_marks: &account_marks,
                                now: chrono::Utc::now(),
                            },
                        );
                        self.mailbox.message_scroll_offset = clamped_scroll;
                    }
                }
            }
            Screen::Search => {
                let rows = self.search_mail_list_rows();
                let preview_blocks = self.thread_message_blocks();
                let preview_scroll = ui::search_page::draw(
                    frame,
                    content_area,
                    ui::search_page::SearchPageView {
                        zone: crate::ui::received_label::DisplayZone::Local,
                        state: &self.search.page,
                        rows: &rows,
                        selected_set: &self.mailbox.selected_set,
                        mail_list_mode: self.search_list_mode(),
                        preview_messages: &preview_blocks,
                        preview_scroll: self.mailbox.message_scroll_offset,
                        html_images: &mut self.html_image_assets,
                        account_marks: &account_marks,
                        now: chrono::Utc::now(),
                    },
                    theme,
                );
                if let Some(scroll) = preview_scroll {
                    self.mailbox.message_scroll_offset = scroll;
                }
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
                    &account_marks,
                );
            }
            Screen::Analytics => {
                ui::analytics_page::draw(frame, content_area, &self.analytics, theme);
            }
            Screen::Deliveries => {
                if self.deliveries.preview_active && self.mailbox.viewing_envelope.is_some() {
                    let cols = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                        .split(content_area);
                    ui::deliveries_page::draw(frame, cols[0], &self.deliveries, theme);
                    let preview_blocks = self.thread_message_blocks();
                    let summary = self.thread_summary_block();
                    let context = self.thread_context_lines();
                    let clamped_scroll = ui::message_view::draw(
                        frame,
                        cols[1],
                        &preview_blocks,
                        ui::message_view::DrawOptions {
                            zone: crate::ui::received_label::DisplayZone::Local,
                            context,
                            summary,
                            scroll_offset: self.mailbox.message_scroll_offset,
                            active_pane: &self.mailbox.active_pane,
                            theme,
                            html_images: &mut self.html_image_assets,
                            account_marks: &account_marks,
                            now: chrono::Utc::now(),
                        },
                    );
                    self.mailbox.message_scroll_offset = clamped_scroll;
                } else {
                    ui::deliveries_page::draw(frame, content_area, &self.deliveries, theme);
                }
            }
        }

        if let Some(pane) = assistant_area {
            self.assistant.max_scroll = ui::assistant_pane::draw(
                frame,
                pane,
                &self.assistant,
                std::time::Instant::now(),
                &account_marks,
                theme,
            );
        }

        let status_bar = self.status_bar_state();
        ui::status_bar::draw(
            frame,
            bottom_bar_area,
            &status_bar,
            Some(&self.status_throbber),
            theme,
        );

        if self.search.bar.active {
            ui::search_bar::draw(frame, area, &self.search.bar, theme);
        }

        ui::command_palette::draw(frame, area, &self.command_palette.palette, theme);

        ui::label_picker::draw(frame, area, &self.modals.label_picker, theme);

        ui::personal_category_picker::draw(frame, area, &self.modals.personal_categories, theme);

        ui::compose_picker::draw(frame, area, &self.compose.compose_picker, theme);

        ui::attachment_modal::draw(frame, area, &self.mailbox.attachment_panel, theme);

        ui::save_attachment_modal::draw(frame, area, &self.modals.save_attachment, theme);

        ui::export_modal::draw(frame, area, &self.modals.export, theme);

        ui::url_modal::draw(frame, area, self.mailbox.url_modal.as_ref(), theme);

        ui::snooze_modal::draw(
            frame,
            area,
            &self.modals.snooze_panel,
            &self.modals.snooze_config,
            theme,
        );

        ui::reply_later_prompt::draw(frame, area, self.modals.reply_later_prompt.as_ref(), theme);
        ui::todo_lens::draw_prompt(frame, area, self.mailbox.todo_page.prompt.as_ref(), theme);
        ui::records_lens::draw_overlays(
            frame,
            area,
            &self.mailbox.records_page,
            theme,
            chrono::Utc::now(),
            crate::ui::received_label::DisplayZone::Local,
        );

        let time_preview = self.compose.pending_time.lines();
        ui::send_confirm_modal::draw(
            frame,
            area,
            self.compose.pending_send_confirm.as_ref(),
            self.compose.pending_send_at_input.as_deref(),
            self.compose.pending_remind_at_input.as_deref(),
            &time_preview,
            theme,
            &account_marks,
        );
        if let Some(review) = self
            .compose
            .assist_review
            .as_mut()
            .filter(|_| !self.compose.viewing_voice_source)
        {
            if let Some(proposal) = review.proposal() {
                let scroll = ui::compose_assist_proof::draw(
                    frame,
                    area,
                    proposal,
                    review.note_input.as_deref(),
                    (review.current_index, review.proposals.len()),
                    review.proof_scroll_offset,
                    theme,
                );
                review.proof_scroll_offset = scroll;
            }
        }
        if let Some(picker) = self.compose.model_picker.as_ref() {
            ui::compose_assist_proof::draw_model_picker(
                frame,
                area,
                &picker.choices,
                picker.selected_index,
                theme,
            );
        }

        ui::bulk_confirm_modal::draw(
            frame,
            area,
            self.modals.pending_bulk_confirm.as_ref(),
            theme,
        );

        ui::place_lens::draw_sweep_confirm(
            frame,
            area,
            self.mailbox.sweep_confirm.as_ref(),
            &self.accounts.page.accounts,
            theme,
        );
        ui::place_lens::draw_sender_kind_menu(
            frame,
            area,
            self.mailbox.sender_kind_menu.as_ref(),
            theme,
        );
        ui::move_menu::draw_arrivals_list(
            frame,
            area,
            self.mailbox.trust.arrivals_list.as_ref(),
            theme,
        );
        ui::move_menu::draw_move_menu(frame, area, self.mailbox.trust.move_menu.as_ref(), theme);

        ui::saved_search_form::draw(frame, area, self.modals.saved_search_form.as_ref(), theme);

        ui::saved_search_form::draw_delete_confirm(
            frame,
            area,
            self.modals.pending_saved_search_delete_confirm.as_deref(),
            self.modals.saved_search_delete_choice,
            theme,
        );

        ui::analytics_filter_modal::draw(frame, area, self.modals.analytics_filter.as_ref(), theme);
        ui::messages_filter_modal::draw(
            frame,
            area,
            self.modals.messages_filter,
            self.mailbox.messages_page.filter,
            theme,
        );

        ui::draft_options_modal::draw(frame, area, &self.modals.draft_options, theme);

        ui::error_modal::draw(frame, area, self.modals.error.as_ref(), theme);

        ui::unsubscribe_modal::draw(
            frame,
            area,
            self.modals.pending_unsubscribe_confirm.as_ref(),
            theme,
        );

        ui::help_modal::draw(
            frame,
            area,
            ui::help_modal::HelpModalState {
                keybindings: Some(&self.keybindings),
                binding_context: Some(self.help_keybinding_context()),
                open: self.modals.help_open,
                ui_context: self.help_modal_context(),
                selected_count: self.mailbox.selected_set.len(),
                scroll_offset: self.modals.help_scroll_offset,
                query: &self.modals.help_query,
                selected: self.modals.help_selected,
                mode_guide: self.help_mode_guide(),
                key_places: self.help_key_places(),
                mode_common: self.mode_strip_hints(),
                _marker: std::marker::PhantomData,
            },
            theme,
        );

        ui::snippets_modal::draw(frame, area, &self.modals.snippets, theme);

        ui::sender_profile_modal::draw(frame, area, &self.modals.sender_profile, theme);

        ui::screener_modal::draw(frame, area, &self.modals.screener, theme);

        ui::reply_queue_modal::draw(frame, area, &self.modals.reply_queue, theme);

        ui::drafts_modal::draw(frame, area, &self.modals.drafts, theme);

        ui::activity_modal::draw(frame, area, &self.modals.activity, theme);

        ui::summary_modal::draw(frame, area, &self.modals.summary, theme);

        ui::briefing_modal::draw(frame, area, &self.modals.briefing, theme);

        ui::whois_modal::draw(frame, area, &self.modals.whois, theme);

        ui::expert_modal::draw(frame, area, &self.modals.expert, theme);

        ui::platform_modal::draw(frame, area, &self.modals.platform, theme);

        if self.accounts.page.onboarding_modal_open {
            ui::accounts_page::draw_account_setup_onboarding(frame, area, theme);
        }

        let now = std::time::Instant::now();
        let undo_toast = self.pending_undo_toast(now);
        let promise_toast = self.pending_promise_toast(now);
        let mut visible_toasts = self.toasts.visible(now);
        for pinned in [undo_toast.as_ref(), promise_toast.as_ref()]
            .into_iter()
            .flatten()
        {
            visible_toasts.insert(0, pinned);
        }
        visible_toasts.truncate(crate::app::TOAST_MAX_VISIBLE);
        let toast_area = assistant_area.map_or(area, |pane| Rect {
            width: pane.x.saturating_sub(area.x),
            ..area
        });
        if let Some((prefix, choices)) = self.prefix_guide() {
            ui::prefix_guide::draw(frame, toast_area, prefix, &choices, theme);
        }
        ui::toasts::draw(frame, toast_area, &visible_toasts, now, theme);
    }

    fn draw_saved_search_tabs(&self, frame: &mut Frame, area: Rect, theme: &Theme) -> Rect {
        if self.mailbox.saved_searches.is_empty()
            || self.mailbox.mailbox_view == MailboxView::Subscriptions
            || self.mailbox.mailbox_view == MailboxView::CalendarInvites
        {
            return area;
        }

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
