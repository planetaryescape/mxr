use super::*;

impl App {
    pub(super) fn apply_message_action(&mut self, action: Action) {
        match action {
            Action::OpenInBrowser => {
                self.queue_current_message_browser_open();
            }

            // Phase 2: Reader mode
            Action::ToggleReaderMode => {
                if self.mailbox.html_view {
                    self.status_message = Some("Switch to text view to use reading view".into());
                } else if let BodyViewState::Ready { .. } = self.mailbox.body_view_state {
                    self.mailbox.reader_mode = !self.mailbox.reader_mode;
                    if let Some(env) = self.mailbox.viewing_envelope.clone() {
                        self.mailbox.body_view_state = self.resolve_body_view_state(&env);
                    }
                    self.status_message = self.current_body_mode_status_message();
                }
            }
            Action::ToggleHtmlView => {
                self.mailbox.html_view = !self.mailbox.html_view;
                if self.mailbox.html_view {
                    self.queue_html_assets_for_current_view();
                }
                if let Some(env) = self.mailbox.viewing_envelope.clone() {
                    self.mailbox.body_view_state = self.resolve_body_view_state(&env);
                }
                self.status_message = self.current_body_mode_status_message();
            }
            Action::ToggleRemoteContent => {
                self.mailbox.remote_content_enabled = !self.mailbox.remote_content_enabled;
                self.invalidate_html_assets_for_current_view();
                self.queue_html_assets_for_current_view();
                if let Some(env) = self.mailbox.viewing_envelope.clone() {
                    self.mailbox.body_view_state = self.resolve_body_view_state(&env);
                }
                self.status_message = Some(if self.mailbox.remote_content_enabled {
                    "Remote images shown in HTML view".into()
                } else {
                    "Remote images blocked in HTML view".into()
                });
            }
            Action::ToggleSignature => {
                self.mailbox.signature_expanded = !self.mailbox.signature_expanded;
            }
            Action::ToggleQuotes => self.toggle_quotes(),
            Action::ToggleMessage => self.toggle_focused_message(),
            Action::ToggleAllMessages => self.toggle_all_messages(),

            // Phase 2: Batch operations (A007)
            Action::AttachmentList => {
                if self.mailbox.attachment_panel.visible {
                    self.close_attachment_panel();
                } else {
                    self.open_attachment_panel();
                }
            }
            Action::OpenLinks => {
                self.open_url_modal();
            }
            Action::ToggleFullscreen => {
                if self.screen == Screen::Search {
                    if self.search.page.preview_fullscreen
                        && self.search.page.active_pane == SearchPane::Preview
                    {
                        self.search.page.preview_fullscreen = false;
                        self.status_message = Some("Showing split view".into());
                    } else if self.search.page.result_selected
                        || self.selected_search_envelope().is_some()
                    {
                        if !self.search.page.result_selected {
                            self.open_selected_search_result();
                        }
                        if self.search.page.result_selected {
                            self.search.page.preview_fullscreen = true;
                            self.search.page.active_pane = SearchPane::Preview;
                            self.status_message = Some("Showing full message view".into());
                        }
                    }
                } else if self.mailbox.layout_mode == LayoutMode::FullScreen {
                    self.mailbox.layout_mode = LayoutMode::ThreePane;
                    self.mailbox.active_pane = ActivePane::MessageView;
                    self.status_message = Some("Showing split view".into());
                } else if self.mailbox.viewing_envelope.is_some() {
                    self.mailbox.layout_mode = LayoutMode::FullScreen;
                    self.mailbox.active_pane = ActivePane::MessageView;
                    self.status_message = Some("Showing full message view".into());
                } else if self.screen == Screen::Mailbox {
                    match self.mailbox.mailbox_view {
                        MailboxView::Subscriptions => {
                            if let Some(entry) = self.selected_subscription_entry().cloned() {
                                self.open_envelope(entry.envelope);
                                self.mailbox.layout_mode = LayoutMode::FullScreen;
                                self.mailbox.active_pane = ActivePane::MessageView;
                                self.status_message = Some("Showing full message view".into());
                            }
                        }
                        MailboxView::Messages => {
                            if let Some(row) = self.selected_mail_row() {
                                self.open_envelope(row.representative);
                                self.mailbox.layout_mode = LayoutMode::FullScreen;
                                self.mailbox.active_pane = ActivePane::MessageView;
                                self.status_message = Some("Showing full message view".into());
                            }
                        }
                        MailboxView::Owed => {
                            // Owed lens has no per-row "open" affordance
                            // for now; selecting a row just navigates within
                            // the lens. (A future slice can wire enter →
                            // navigate to the underlying thread.)
                        }
                        MailboxView::CalendarInvites => {
                            self.open_selected_invite_message();
                        }
                        MailboxView::Desk => {
                            self.open_selected_desk_row();
                        }
                        MailboxView::Place(_) => {
                            self.open_selected_place_row();
                        }
                        MailboxView::Todo => {
                            self.apply(Action::TodoOpenEmail);
                        }
                        MailboxView::Now => {
                            self.apply(Action::NowOpen);
                        }
                        MailboxView::People => {
                            self.apply(Action::MessagesOpen);
                        }
                        MailboxView::Updates => {
                            self.apply(Action::UpdatesOpen);
                        }
                        MailboxView::ArchiveMode => {
                            self.apply(Action::RecordsOpenCard);
                        }
                        MailboxView::Reading => {
                            self.apply(Action::ReadingRead);
                        }
                    }
                }
            }
            Action::ExportThread => self.open_export_modal(),
            _ => unreachable!("action routed to wrong handler"),
        }
    }
}

impl App {
    /// `Q`, as in the web reader: the reading and HTML views fold quoted
    /// text until asked. Plain text is the sender's exact text, so it
    /// already shows every quoted line.
    fn toggle_quotes(&mut self) {
        if !self.mailbox.html_view && !self.mailbox.reader_mode {
            self.status_message =
                Some("Plain text shows quoted text in full; R folds it in the reading view".into());
            return;
        }
        self.mailbox.quotes_expanded = !self.mailbox.quotes_expanded;
        if let Some(env) = self.mailbox.viewing_envelope.clone() {
            self.mailbox.body_view_state = self.resolve_body_view_state(&env);
        }
        self.status_message = Some(if self.mailbox.quotes_expanded {
            "Showing quoted text".into()
        } else {
            "Quoted text folded".into()
        });
    }

    /// `o`, as in the web reader: the focused message (`J`/`K`) folds to
    /// its one header line, or opens again.
    fn toggle_focused_message(&mut self) {
        let Some(id) = self.focused_thread_envelope().map(|env| env.id.clone()) else {
            return;
        };
        self.status_message = Some(if self.mailbox.collapsed_messages.remove(&id) {
            "Message open".into()
        } else {
            self.mailbox.collapsed_messages.insert(id);
            // `o` once opened the browser, so name `O`, which still does.
            "Message folded; o opens it, O opens it in the browser".into()
        });
        self.mailbox.scroll_to_focused_message = true;
    }

    /// `;`, as in the web reader: open every message, or, when all are
    /// open, fold all but the newest and the unread ones, which is where
    /// the web reader starts.
    fn toggle_all_messages(&mut self) {
        let messages = &self.mailbox.viewed_thread_messages;
        if messages.is_empty() {
            return;
        }
        if self.mailbox.collapsed_messages.is_empty() {
            let newest = messages.len() - 1;
            let folded: HashSet<MessageId> = messages
                .iter()
                .enumerate()
                .filter(|(index, env)| *index != newest && env.flags.contains(MessageFlags::READ))
                .map(|(_, env)| env.id.clone())
                .collect();
            if folded.is_empty() {
                self.status_message = Some("Every message here is new or unread".into());
                return;
            }
            self.mailbox.collapsed_messages = folded;
            self.status_message = Some("Earlier messages collapsed".into());
        } else {
            self.mailbox.collapsed_messages.clear();
            self.status_message = Some("Every message open".into());
        }
        self.mailbox.scroll_to_focused_message = true;
    }
}
