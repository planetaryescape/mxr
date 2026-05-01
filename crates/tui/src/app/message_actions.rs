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
                    self.status_message = Some("Showing split view".into());
                } else if self.mailbox.viewing_envelope.is_some() {
                    self.mailbox.layout_mode = LayoutMode::FullScreen;
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
                    }
                }
            }
            Action::ExportThread => {
                if let Some(env) = self.context_envelope() {
                    self.mailbox.pending_export_thread = Some(env.thread_id.clone());
                    self.status_message = Some("Exporting thread...".into());
                } else {
                    self.status_message = Some("No message selected".into());
                }
            }
            _ => unreachable!("action routed to wrong handler"),
        }
    }
}
