use super::*;
use ratatui::crossterm;

fn plain_or_shift(modifiers: KeyModifiers) -> bool {
    modifiers.is_empty() || modifiers == KeyModifiers::SHIFT
}

impl App {
    fn help_modal_state(&self) -> crate::ui::help_modal::HelpModalState<'_> {
        crate::ui::help_modal::HelpModalState {
            open: self.modals.help_open,
            ui_context: self.current_ui_context(),
            selected_count: self.mailbox.selected_set.len(),
            scroll_offset: self.modals.help_scroll_offset,
            query: &self.modals.help_query,
            selected: self.modals.help_selected,
            _marker: std::marker::PhantomData,
        }
    }

    fn help_search_active(&self) -> bool {
        !self.modals.help_query.is_empty()
    }

    fn help_search_result_count(&self) -> usize {
        crate::ui::help_modal::search_result_count(&self.help_modal_state())
    }

    fn clamp_help_selected(&mut self) {
        let count = self.help_search_result_count();
        self.modals.help_selected = self.modals.help_selected.min(count.saturating_sub(1));
    }

    fn push_help_query(&mut self, c: char) {
        self.modals.help_query.push(c);
        self.modals.help_selected = 0;
    }

    fn pop_help_query(&mut self) {
        self.modals.help_query.pop();
        if self.modals.help_query.is_empty() {
            self.modals.help_selected = 0;
        } else {
            self.clamp_help_selected();
        }
    }

    fn help_select_next(&mut self) {
        let count = self.help_search_result_count();
        if count > 0 {
            self.modals.help_selected = (self.modals.help_selected + 1).min(count - 1);
        }
    }

    fn help_select_prev(&mut self) {
        self.modals.help_selected = self.modals.help_selected.saturating_sub(1);
    }

    fn help_page_down(&mut self) {
        let count = self.help_search_result_count();
        if count > 0 {
            self.modals.help_selected = (self.modals.help_selected + 8).min(count - 1);
        }
    }

    fn help_page_up(&mut self) {
        self.modals.help_selected = self.modals.help_selected.saturating_sub(8);
    }

    fn contextual_input_action(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        let action = self.input.handle_key(key)?;
        crate::action::action_allowed_in_context(&action, self.current_ui_context())
            .then_some(action)
    }

    fn mail_action_key(&self, key: crossterm::event::KeyEvent) -> Option<Action> {
        match (key.code, key.modifiers) {
            (KeyCode::Char('c'), KeyModifiers::NONE) => Some(Action::Compose),
            (KeyCode::Char('r'), KeyModifiers::NONE) => Some(Action::Reply),
            (KeyCode::Char('a'), KeyModifiers::NONE) => Some(Action::ReplyAll),
            (KeyCode::Char('f'), KeyModifiers::NONE) => Some(Action::Forward),
            (KeyCode::Char('e'), KeyModifiers::NONE) => Some(Action::Archive),
            (KeyCode::Char('m'), KeyModifiers::NONE) => Some(Action::MarkReadAndArchive),
            (KeyCode::Char('#'), _) => Some(Action::Trash),
            (KeyCode::Char('!'), _) => Some(Action::Spam),
            (KeyCode::Char('s'), KeyModifiers::NONE) => Some(Action::Star),
            (KeyCode::Char('I'), modifiers) if plain_or_shift(modifiers) => Some(Action::MarkRead),
            (KeyCode::Char('U'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::MarkUnread)
            }
            (KeyCode::Char('l'), KeyModifiers::NONE) => Some(Action::ApplyLabel),
            (KeyCode::Char('v'), KeyModifiers::NONE) => Some(Action::MoveToLabel),
            (KeyCode::Char('D'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::Unsubscribe)
            }
            (KeyCode::Char('Z'), modifiers) if plain_or_shift(modifiers) => Some(Action::Snooze),
            (KeyCode::Char('O'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::OpenInBrowser)
            }
            (KeyCode::Char('R'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::ToggleReaderMode)
            }
            (KeyCode::Char('S'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::ToggleSignature)
            }
            (KeyCode::Char('A'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::AttachmentList)
            }
            (KeyCode::Char('L'), modifiers) if plain_or_shift(modifiers) => Some(Action::OpenLinks),
            (KeyCode::Char('E'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::ExportThread)
            }
            _ => None,
        }
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        #[cfg(debug_assertions)]
        if key.code == KeyCode::Char('d')
            && key.modifiers.contains(KeyModifiers::CONTROL)
            && key.modifiers.contains(KeyModifiers::ALT)
        {
            return Some(Action::DumpActionTrace);
        }

        if self.modals.error.is_some() {
            return match (key.code, key.modifiers) {
                (KeyCode::Char('j') | KeyCode::Down, _) => {
                    if let Some(error) = self.modals.error.as_mut() {
                        error.scroll_offset = error.scroll_offset.saturating_add(1);
                    }
                    None
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    if let Some(error) = self.modals.error.as_mut() {
                        error.scroll_offset = error.scroll_offset.saturating_sub(1);
                    }
                    None
                }
                (KeyCode::Char('d'), KeyModifiers::CONTROL) | (KeyCode::PageDown, _) => {
                    if let Some(error) = self.modals.error.as_mut() {
                        error.scroll_offset = error.scroll_offset.saturating_add(8);
                    }
                    None
                }
                (KeyCode::Char('u'), KeyModifiers::CONTROL) | (KeyCode::PageUp, _) => {
                    if let Some(error) = self.modals.error.as_mut() {
                        error.scroll_offset = error.scroll_offset.saturating_sub(8);
                    }
                    None
                }
                (KeyCode::Esc | KeyCode::Enter, _)
                | (KeyCode::Char('q'), _)
                | (KeyCode::Char('x'), _) => {
                    self.modals.error = None;
                    None
                }
                _ => None,
            };
        }

        if self.accounts.page.onboarding_modal_open {
            return match (key.code, key.modifiers) {
                (KeyCode::Enter | KeyCode::Char(' '), _) => {
                    self.complete_account_setup_onboarding();
                    None
                }
                (KeyCode::Char('q'), _) => Some(Action::QuitView),
                (KeyCode::Esc, _) => {
                    self.accounts.page.onboarding_modal_open = false;
                    None
                }
                _ => None,
            };
        }

        if self.modals.help_open {
            return match (key.code, key.modifiers) {
                (KeyCode::Esc | KeyCode::Enter, _)
                | (KeyCode::Char('?'), _)
                | (KeyCode::Char('q'), _) => Some(Action::Help),
                (KeyCode::Char('j') | KeyCode::Down, _) => {
                    if self.help_search_active() {
                        self.help_select_next();
                    } else {
                        self.modals.help_scroll_offset =
                            self.modals.help_scroll_offset.saturating_add(1);
                    }
                    None
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    if self.help_search_active() {
                        self.help_select_prev();
                    } else {
                        self.modals.help_scroll_offset =
                            self.modals.help_scroll_offset.saturating_sub(1);
                    }
                    None
                }
                (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                    if self.help_search_active() {
                        self.help_page_down();
                    } else {
                        self.modals.help_scroll_offset =
                            self.modals.help_scroll_offset.saturating_add(8);
                    }
                    None
                }
                (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                    if self.help_search_active() {
                        self.help_page_up();
                    } else {
                        self.modals.help_scroll_offset =
                            self.modals.help_scroll_offset.saturating_sub(8);
                    }
                    None
                }
                (KeyCode::Backspace, _) => {
                    self.pop_help_query();
                    None
                }
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    self.push_help_query(c);
                    None
                }
                _ => None,
            };
        }

        if self.modals.onboarding.visible {
            return match (key.code, key.modifiers) {
                (KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Right | KeyCode::Char('l'), _) => {
                    self.advance_feature_onboarding();
                    None
                }
                (KeyCode::Left | KeyCode::Char('h'), _) => {
                    self.modals.onboarding.step = self.modals.onboarding.step.saturating_sub(1);
                    None
                }
                (KeyCode::Esc | KeyCode::Char('q'), _) => {
                    self.dismiss_feature_onboarding();
                    None
                }
                _ => None,
            };
        }

        if self.command_palette.palette.visible {
            match (key.code, key.modifiers) {
                (KeyCode::Enter, _) => return self.command_palette.palette.confirm(),
                (KeyCode::Esc, _) => return Some(Action::CloseCommandPalette),
                (KeyCode::Backspace, _) => {
                    self.command_palette.palette.on_backspace();
                    return None;
                }
                (KeyCode::Down, _) | (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
                    self.command_palette.palette.select_next();
                    return None;
                }
                (KeyCode::Up, _) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                    self.command_palette.palette.select_prev();
                    return None;
                }
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    self.command_palette.palette.on_char(c);
                    return None;
                }
                _ => return None,
            }
        }

        // Route keys to search bar when active
        if self.search.bar.active {
            match (key.code, key.modifiers) {
                (KeyCode::Enter, _) => return Some(Action::SubmitSearch),
                (KeyCode::Tab, _) => return Some(Action::CycleSearchMode),
                (KeyCode::Esc, _) => return Some(Action::CloseSearch),
                (KeyCode::Backspace, _) => {
                    self.search.bar.on_backspace();
                    // Live filter as you type
                    self.trigger_live_search();
                    return None;
                }
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    self.search.bar.on_char(c);
                    // Live filter as you type
                    self.trigger_live_search();
                    return None;
                }
                _ => return None,
            }
        }

        // Route keys to send confirmation prompt
        if self.compose.pending_send_confirm.is_some() {
            match (key.code, key.modifiers) {
                (KeyCode::Char('s'), KeyModifiers::NONE) => {
                    // Send
                    if let Some(pending) = self.compose.pending_send_confirm.take() {
                        if pending.mode != PendingSendMode::SendOrSave {
                            self.compose.pending_send_confirm = Some(pending);
                            return None;
                        }
                        let parse_addrs = |s: &str| mxr_mail_parse::parse_address_list(s);
                        let reply_headers = pending.fm.in_reply_to.as_ref().map(|in_reply_to| {
                            mxr_core::types::ReplyHeaders {
                                in_reply_to: in_reply_to.clone(),
                                references: pending.fm.references.clone(),
                                thread_id: pending.fm.thread_id.clone(),
                            }
                        });
                        let now = chrono::Utc::now();
                        let draft = mxr_core::Draft {
                            id: mxr_core::id::DraftId::new(),
                            account_id: pending.account_id.clone(),
                            reply_headers,
                            to: parse_addrs(&pending.fm.to),
                            cc: parse_addrs(&pending.fm.cc),
                            bcc: parse_addrs(&pending.fm.bcc),
                            subject: pending.fm.subject,
                            body_markdown: pending.body,
                            attachments: pending
                                .fm
                                .attach
                                .iter()
                                .map(std::path::PathBuf::from)
                                .collect(),
                            created_at: now,
                            updated_at: now,
                        };
                        self.queue_mutation(
                            Request::SendDraft { draft },
                            MutationEffect::StatusOnly("Sent!".into()),
                            "Sending...".into(),
                        );
                        self.schedule_draft_cleanup(pending.draft_path);
                    }
                    return None;
                }
                (KeyCode::Char('d'), KeyModifiers::NONE) => {
                    // Save as draft to mail server
                    if let Some(pending) = self.compose.pending_send_confirm.take() {
                        if pending.mode == PendingSendMode::Unchanged {
                            self.compose.pending_send_confirm = Some(pending);
                            return None;
                        }
                        let parse_addrs = |s: &str| mxr_mail_parse::parse_address_list(s);
                        let reply_headers = pending.fm.in_reply_to.as_ref().map(|in_reply_to| {
                            mxr_core::types::ReplyHeaders {
                                in_reply_to: in_reply_to.clone(),
                                references: pending.fm.references.clone(),
                                thread_id: pending.fm.thread_id.clone(),
                            }
                        });
                        let now = chrono::Utc::now();
                        let draft = mxr_core::Draft {
                            id: mxr_core::id::DraftId::new(),
                            account_id: pending.account_id.clone(),
                            reply_headers,
                            to: parse_addrs(&pending.fm.to),
                            cc: parse_addrs(&pending.fm.cc),
                            bcc: parse_addrs(&pending.fm.bcc),
                            subject: pending.fm.subject,
                            body_markdown: pending.body,
                            attachments: pending
                                .fm
                                .attach
                                .iter()
                                .map(std::path::PathBuf::from)
                                .collect(),
                            created_at: now,
                            updated_at: now,
                        };
                        self.queue_mutation(
                            Request::SaveDraftToServer { draft },
                            MutationEffect::StatusOnly("Draft saved to server".into()),
                            "Saving draft...".into(),
                        );
                        self.schedule_draft_cleanup(pending.draft_path);
                    }
                    return None;
                }
                (KeyCode::Char('e'), KeyModifiers::NONE) => {
                    // Edit again — reopen editor
                    if let Some(pending) = self.compose.pending_send_confirm.take() {
                        self.compose.pending_compose = Some(ComposeAction::EditDraft {
                            path: pending.draft_path,
                            account_id: pending.account_id,
                        });
                    }
                    return None;
                }
                (KeyCode::Esc, _) => {
                    // Discard
                    if let Some(pending) = self.compose.pending_send_confirm.take() {
                        self.schedule_draft_cleanup(pending.draft_path);
                        self.status_message = Some("Discarded".into());
                    }
                    return None;
                }
                _ => return None,
            }
        }

        if self.modals.pending_bulk_confirm.is_some() {
            return match (key.code, key.modifiers) {
                (KeyCode::Enter, _) => Some(Action::OpenSelected),
                (KeyCode::Char('y'), KeyModifiers::NONE) => Some(Action::OpenSelected),
                (KeyCode::Char('Y'), modifiers) if plain_or_shift(modifiers) => {
                    Some(Action::OpenSelected)
                }
                (KeyCode::Esc, _) | (KeyCode::Char('n'), KeyModifiers::NONE) => {
                    self.modals.pending_bulk_confirm = None;
                    self.status_message = Some("Bulk action cancelled".into());
                    None
                }
                _ => None,
            };
        }

        if self.modals.pending_unsubscribe_confirm.is_some() {
            return match (key.code, key.modifiers) {
                (KeyCode::Enter, _) => Some(Action::ConfirmUnsubscribeOnly),
                (KeyCode::Char('u'), KeyModifiers::NONE) => Some(Action::ConfirmUnsubscribeOnly),
                (KeyCode::Char('U'), modifiers) if plain_or_shift(modifiers) => {
                    Some(Action::ConfirmUnsubscribeOnly)
                }
                (KeyCode::Char('a'), KeyModifiers::NONE) => {
                    Some(Action::ConfirmUnsubscribeAndArchiveSender)
                }
                (KeyCode::Char('A'), modifiers) if plain_or_shift(modifiers) => {
                    Some(Action::ConfirmUnsubscribeAndArchiveSender)
                }
                (KeyCode::Esc, _) => Some(Action::CancelUnsubscribe),
                _ => None,
            };
        }

        if self.modals.snooze_panel.visible {
            match (key.code, key.modifiers) {
                (KeyCode::Enter, _) => return Some(Action::Snooze),
                (KeyCode::Esc, _) => {
                    self.modals.snooze_panel.visible = false;
                    return None;
                }
                (KeyCode::Char('j') | KeyCode::Down, _) => {
                    self.modals.snooze_panel.selected_index =
                        (self.modals.snooze_panel.selected_index + 1) % snooze_presets().len();
                    return None;
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    self.modals.snooze_panel.selected_index = self
                        .modals
                        .snooze_panel
                        .selected_index
                        .checked_sub(1)
                        .unwrap_or(snooze_presets().len() - 1);
                    return None;
                }
                _ => return None,
            }
        }

        // Route keys to URL modal when active
        if let Some(ref mut url_state) = self.mailbox.url_modal {
            match (key.code, key.modifiers) {
                (KeyCode::Enter | KeyCode::Char('o'), _) => {
                    if let Some(url) = url_state.selected_url().map(|s| s.to_string()) {
                        ui::url_modal::open_url(&url);
                        self.status_message = Some(format!("Opening {url}"));
                    }
                    self.mailbox.url_modal = None;
                    return None;
                }
                (KeyCode::Char('y'), _) => {
                    if let Some(url) = url_state.selected_url().map(|s| s.to_string()) {
                        // Copy to clipboard via pbcopy (macOS) or xclip (Linux)
                        #[cfg(target_os = "macos")]
                        {
                            use std::io::Write;
                            if let Ok(mut child) = std::process::Command::new("pbcopy")
                                .stdin(std::process::Stdio::piped())
                                .spawn()
                            {
                                if let Some(mut stdin) = child.stdin.take() {
                                    let _ = stdin.write_all(url.as_bytes());
                                }
                                let _ = child.wait();
                            }
                        }
                        #[cfg(target_os = "linux")]
                        {
                            use std::io::Write;
                            if let Ok(mut child) = std::process::Command::new("xclip")
                                .args(["-selection", "clipboard"])
                                .stdin(std::process::Stdio::piped())
                                .spawn()
                            {
                                if let Some(mut stdin) = child.stdin.take() {
                                    let _ = stdin.write_all(url.as_bytes());
                                }
                                let _ = child.wait();
                            }
                        }
                        self.status_message = Some(format!("Copied: {url}"));
                    }
                    self.mailbox.url_modal = None;
                    return None;
                }
                (KeyCode::Char('j') | KeyCode::Down, _) => {
                    url_state.next();
                    return None;
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    url_state.prev();
                    return None;
                }
                (KeyCode::Esc | KeyCode::Char('q'), _) => {
                    self.mailbox.url_modal = None;
                    return None;
                }
                _ => return None,
            }
        }

        // Route keys to compose picker when active
        if self.mailbox.attachment_panel.visible {
            match (key.code, key.modifiers) {
                (KeyCode::Enter | KeyCode::Char('o'), _) => {
                    self.queue_attachment_action(AttachmentOperation::Open);
                    return None;
                }
                (KeyCode::Char('d'), _) => {
                    self.queue_attachment_action(AttachmentOperation::Download);
                    return None;
                }
                (KeyCode::Char('j') | KeyCode::Down, _) => {
                    if self.mailbox.attachment_panel.selected_index + 1
                        < self.mailbox.attachment_panel.attachments.len()
                    {
                        self.mailbox.attachment_panel.selected_index += 1;
                    }
                    return None;
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    self.mailbox.attachment_panel.selected_index = self
                        .mailbox
                        .attachment_panel
                        .selected_index
                        .saturating_sub(1);
                    return None;
                }
                (KeyCode::Esc | KeyCode::Char('A'), _) => {
                    self.close_attachment_panel();
                    return None;
                }
                _ => return None,
            }
        }

        // Route keys to compose picker when active
        if self.compose.compose_picker.visible {
            match (key.code, key.modifiers) {
                (KeyCode::Enter, _) => {
                    if self.compose.compose_picker.mode
                        == crate::ui::compose_picker::ComposePickerMode::To
                    {
                        self.compose.compose_picker.open_subject();
                    } else {
                        let (to, subject) = self.compose.compose_picker.confirm_subject();
                        self.compose.pending_compose = Some(ComposeAction::New { to, subject });
                    }
                    return None;
                }
                (KeyCode::Tab, _) => {
                    // Tab adds selected contact to recipients
                    self.compose.compose_picker.add_recipient();
                    return None;
                }
                (KeyCode::Esc, _) => {
                    self.compose.compose_picker.close();
                    return None;
                }
                (KeyCode::Backspace, _) => {
                    self.compose.compose_picker.on_backspace();
                    return None;
                }
                (KeyCode::Down, _) | (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
                    self.compose.compose_picker.select_next();
                    return None;
                }
                (KeyCode::Up, _) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                    self.compose.compose_picker.select_prev();
                    return None;
                }
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    self.compose.compose_picker.on_char(c);
                    return None;
                }
                _ => return None,
            }
        }

        // Route keys to label picker when active
        if self.modals.label_picker.visible {
            match (key.code, key.modifiers) {
                (KeyCode::Enter, _) => {
                    let mode = self.modals.label_picker.mode;
                    if let Some(label_name) = self.modals.label_picker.confirm() {
                        self.modals.pending_label_action = Some((mode, label_name));
                        return match mode {
                            LabelPickerMode::Apply => Some(Action::ApplyLabel),
                            LabelPickerMode::Move => Some(Action::MoveToLabel),
                        };
                    }
                    return None;
                }
                (KeyCode::Esc, _) => {
                    self.modals.label_picker.close();
                    return None;
                }
                (KeyCode::Backspace, _) => {
                    self.modals.label_picker.on_backspace();
                    return None;
                }
                (KeyCode::Down, _) | (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
                    self.modals.label_picker.select_next();
                    return None;
                }
                (KeyCode::Up, _) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                    self.modals.label_picker.select_prev();
                    return None;
                }
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    self.modals.label_picker.on_char(c);
                    return None;
                }
                _ => return None,
            }
        }

        if self.screen != Screen::Mailbox {
            return self.handle_screen_key(key);
        }

        // Route keys based on active pane
        match self.mailbox.active_pane {
            ActivePane::MessageView => match (key.code, key.modifiers) {
                (KeyCode::Char('/'), KeyModifiers::NONE) => Some(Action::OpenGlobalSearch),
                (KeyCode::Char('f'), KeyModifiers::CONTROL) => Some(Action::OpenMailboxFilter),
                (KeyCode::Char('j') | KeyCode::Down, _) => {
                    self.move_message_view_down();
                    None
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    self.move_message_view_up();
                    None
                }
                (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                    self.mailbox.message_scroll_offset =
                        self.mailbox.message_scroll_offset.saturating_add(20);
                    None
                }
                (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                    self.mailbox.message_scroll_offset =
                        self.mailbox.message_scroll_offset.saturating_sub(20);
                    None
                }
                (KeyCode::Char('G'), modifiers) if plain_or_shift(modifiers) => {
                    self.mailbox.message_scroll_offset = u16::MAX;
                    None
                }
                (KeyCode::Char('H'), modifiers) if plain_or_shift(modifiers) => {
                    Some(Action::ToggleHtmlView)
                }
                (KeyCode::Char('M'), modifiers) if plain_or_shift(modifiers) => {
                    Some(Action::ToggleRemoteContent)
                }
                // h = move left to mail list
                (KeyCode::Char('h') | KeyCode::Left, KeyModifiers::NONE) => {
                    self.mailbox.active_pane = ActivePane::MailList;
                    None
                }
                // o = open in browser (message already open in pane)
                (KeyCode::Char('o'), KeyModifiers::NONE) => Some(Action::OpenInBrowser),
                // L = open links picker
                (KeyCode::Char('L'), modifiers) if plain_or_shift(modifiers) => {
                    Some(Action::OpenLinks)
                }
                _ if self.mail_action_key(key).is_some() => self.mail_action_key(key),
                _ => self.contextual_input_action(key),
            },
            ActivePane::Sidebar => match (key.code, key.modifiers) {
                (KeyCode::Char('/'), KeyModifiers::NONE) => Some(Action::OpenGlobalSearch),
                (KeyCode::Char('f'), KeyModifiers::CONTROL) => Some(Action::OpenMailboxFilter),
                (KeyCode::Char('j') | KeyCode::Down, _) => {
                    self.sidebar_move_down();
                    None
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    self.sidebar_move_up();
                    None
                }
                (KeyCode::Char('['), _) => {
                    self.collapse_current_sidebar_section();
                    None
                }
                (KeyCode::Char(']'), _) => {
                    self.expand_current_sidebar_section();
                    None
                }
                (KeyCode::Enter | KeyCode::Char('o'), _) => self.sidebar_select(),
                // l = select label and move to mail list
                (KeyCode::Char('l') | KeyCode::Right, KeyModifiers::NONE) => self.sidebar_select(),
                _ => self.contextual_input_action(key),
            },
            ActivePane::MailList => match (key.code, key.modifiers) {
                (KeyCode::Char('/'), KeyModifiers::NONE) => Some(Action::OpenGlobalSearch),
                (KeyCode::Char('f'), KeyModifiers::CONTROL) => Some(Action::OpenMailboxFilter),
                // h = move left to sidebar
                (KeyCode::Char('h') | KeyCode::Left, KeyModifiers::NONE) => {
                    self.mailbox.active_pane = ActivePane::Sidebar;
                    None
                }
                // Right arrow opens selected message
                (KeyCode::Right, KeyModifiers::NONE) => Some(Action::OpenSelected),
                _ if self.mail_action_key(key).is_some() => self.mail_action_key(key),
                _ => self.contextual_input_action(key),
            },
        }
    }

    fn handle_screen_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        match self.screen {
            Screen::Search => self.handle_search_screen_key(key),
            Screen::Rules => self.handle_rules_screen_key(key),
            Screen::Diagnostics => self.handle_diagnostics_screen_key(key),
            Screen::Accounts => self.handle_accounts_screen_key(key),
            Screen::Mailbox => None,
        }
    }

    fn handle_search_screen_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        if self.search.page.editing {
            return match (key.code, key.modifiers) {
                (KeyCode::Enter, _) => Some(Action::SubmitSearch),
                (KeyCode::Esc, _) => {
                    self.search.page.editing = false;
                    None
                }
                (KeyCode::Backspace, _) => {
                    self.search.page.query.pop();
                    self.trigger_live_search();
                    None
                }
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    self.search.page.query.push(c);
                    self.trigger_live_search();
                    None
                }
                _ => None,
            };
        }

        match self.search.page.active_pane {
            SearchPane::Results => match (key.code, key.modifiers) {
                (KeyCode::Char('/'), _) => {
                    self.search.page.editing = true;
                    None
                }
                (KeyCode::Char('j') | KeyCode::Down, _) => {
                    if self.search.page.selected_index + 1 < self.search_row_count() {
                        self.search.page.selected_index += 1;
                    }
                    self.sync_search_cursor_after_move();
                    None
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    if self.search.page.selected_index > 0 {
                        self.search.page.selected_index -= 1;
                    }
                    self.sync_search_cursor_after_move();
                    None
                }
                (KeyCode::Right, KeyModifiers::NONE) | (KeyCode::Enter | KeyCode::Char('o'), _) => {
                    Some(Action::OpenSelected)
                }
                _ if self.mail_action_key(key).is_some() => self.mail_action_key(key),
                (KeyCode::Esc, _) => Some(Action::OpenMailboxScreen),
                _ => self.contextual_input_action(key),
            },
            SearchPane::Preview => match (key.code, key.modifiers) {
                (KeyCode::Char('/'), _) => {
                    self.search.page.editing = true;
                    None
                }
                (KeyCode::Char('o'), KeyModifiers::NONE) => Some(Action::OpenInBrowser),
                (KeyCode::Char('h') | KeyCode::Left, KeyModifiers::NONE) => {
                    self.search.page.active_pane = SearchPane::Results;
                    None
                }
                (KeyCode::Char('j') | KeyCode::Down, _) => {
                    self.move_message_view_down();
                    None
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    self.move_message_view_up();
                    None
                }
                (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                    self.mailbox.message_scroll_offset =
                        self.mailbox.message_scroll_offset.saturating_add(20);
                    None
                }
                (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                    self.mailbox.message_scroll_offset =
                        self.mailbox.message_scroll_offset.saturating_sub(20);
                    None
                }
                (KeyCode::Char('G'), modifiers) if plain_or_shift(modifiers) => {
                    self.mailbox.message_scroll_offset = u16::MAX;
                    None
                }
                (KeyCode::Char('H'), modifiers) if plain_or_shift(modifiers) => {
                    Some(Action::ToggleHtmlView)
                }
                (KeyCode::Char('M'), modifiers) if plain_or_shift(modifiers) => {
                    Some(Action::ToggleRemoteContent)
                }
                (KeyCode::Esc, _) => {
                    self.reset_search_preview_selection();
                    None
                }
                _ if self.mail_action_key(key).is_some() => self.mail_action_key(key),
                _ => self.contextual_input_action(key),
            },
        }
    }

    fn handle_rules_screen_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        if self.rules.page.form.visible {
            return self.handle_rule_form_key(key);
        }

        match (key.code, key.modifiers) {
            (KeyCode::Char('j') | KeyCode::Down, _) => {
                if self.rules.page.selected_index + 1 < self.rules.page.rules.len() {
                    self.rules.page.selected_index += 1;
                    self.refresh_selected_rule_panel();
                }
                None
            }
            (KeyCode::Char('k') | KeyCode::Up, _) => {
                self.rules.page.selected_index = self.rules.page.selected_index.saturating_sub(1);
                self.refresh_selected_rule_panel();
                None
            }
            (KeyCode::Enter | KeyCode::Char('o'), _) => Some(Action::RefreshRules),
            (KeyCode::Char('e'), _) => Some(Action::ToggleRuleEnabled),
            (KeyCode::Char('D'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::ShowRuleDryRun)
            }
            (KeyCode::Char('H'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::ShowRuleHistory)
            }
            (KeyCode::Char('#'), _) => Some(Action::DeleteRule),
            (KeyCode::Char('n'), _) => Some(Action::OpenRuleFormNew),
            (KeyCode::Char('E'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::OpenRuleFormEdit)
            }
            (KeyCode::Esc, _) => Some(Action::OpenMailboxScreen),
            _ => self.contextual_input_action(key),
        }
    }

    fn handle_rule_form_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => {
                self.rules.page.form.visible = false;
                self.rules.page.panel = RulesPanel::Details;
                None
            }
            (KeyCode::Tab, _) => {
                self.rules.page.form.active_field = (self.rules.page.form.active_field + 1) % 5;
                None
            }
            (KeyCode::BackTab, _) => {
                self.rules.page.form.active_field = if self.rules.page.form.active_field == 0 {
                    4
                } else {
                    self.rules.page.form.active_field - 1
                };
                None
            }
            (KeyCode::Char(' '), _) if self.rules.page.form.active_field == 4 => {
                self.rules.page.form.enabled = !self.rules.page.form.enabled;
                None
            }
            (KeyCode::Char('s'), KeyModifiers::CONTROL) => Some(Action::SaveRuleForm),
            (_, _) if self.rules.page.form.active_field == 1 => {
                self.rules.condition_editor.input(key);
                self.sync_rule_form_strings_from_editors();
                None
            }
            (_, _) if self.rules.page.form.active_field == 2 => {
                self.rules.action_editor.input(key);
                self.sync_rule_form_strings_from_editors();
                None
            }
            (KeyCode::Backspace, _) => {
                match self.rules.page.form.active_field {
                    0 => {
                        self.rules.page.form.name.pop();
                    }
                    3 => {
                        self.rules.page.form.priority.pop();
                    }
                    _ => {}
                }
                None
            }
            (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                match self.rules.page.form.active_field {
                    0 => self.rules.page.form.name.push(c),
                    3 => self.rules.page.form.priority.push(c),
                    _ => {}
                }
                None
            }
            (KeyCode::Enter, _) => Some(Action::SaveRuleForm),
            _ => None,
        }
    }

    fn handle_diagnostics_screen_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        match (key.code, key.modifiers) {
            (KeyCode::Tab | KeyCode::Right, _) => {
                self.diagnostics.page.selected_pane = self.diagnostics.page.selected_pane.next();
                None
            }
            (KeyCode::BackTab | KeyCode::Left, _) => {
                self.diagnostics.page.selected_pane = self.diagnostics.page.selected_pane.prev();
                None
            }
            (KeyCode::Char('j') | KeyCode::Down, _) => {
                self.diagnostics.page.selected_pane = self.diagnostics.page.selected_pane.next();
                None
            }
            (KeyCode::Char('k') | KeyCode::Up, _) => {
                self.diagnostics.page.selected_pane = self.diagnostics.page.selected_pane.prev();
                None
            }
            (KeyCode::Char('d'), KeyModifiers::CONTROL) | (KeyCode::PageDown, _) => {
                let pane = self.diagnostics.page.active_pane();
                *self.diagnostics.page.scroll_offset_mut(pane) =
                    self.diagnostics.page.scroll_offset(pane).saturating_add(8);
                None
            }
            (KeyCode::Char('u'), KeyModifiers::CONTROL) | (KeyCode::PageUp, _) => {
                let pane = self.diagnostics.page.active_pane();
                *self.diagnostics.page.scroll_offset_mut(pane) =
                    self.diagnostics.page.scroll_offset(pane).saturating_sub(8);
                None
            }
            (KeyCode::Enter | KeyCode::Char('o'), _) => {
                self.diagnostics.page.toggle_fullscreen();
                None
            }
            (KeyCode::Char('d'), _) => Some(Action::OpenDiagnosticsPaneDetails),
            (KeyCode::Char('r'), _) => Some(Action::RefreshDiagnostics),
            (KeyCode::Char('b'), _) => Some(Action::GenerateBugReport),
            (KeyCode::Char('c'), _) => Some(Action::EditConfig),
            (KeyCode::Char('L'), modifiers) if plain_or_shift(modifiers) => Some(Action::OpenLogs),
            (KeyCode::Esc, _) => Some(Action::OpenMailboxScreen),
            _ => self.contextual_input_action(key),
        }
    }

    fn handle_accounts_screen_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        if self.accounts.page.resume_new_account_draft_prompt_open {
            return match (key.code, key.modifiers) {
                (KeyCode::Enter | KeyCode::Char('c'), _) => {
                    self.restore_new_account_form_draft();
                    None
                }
                (KeyCode::Char('n'), _) => {
                    self.accounts.page.new_account_draft = None;
                    self.open_new_account_form();
                    None
                }
                (KeyCode::Esc, _) => {
                    self.accounts.page.resume_new_account_draft_prompt_open = false;
                    None
                }
                _ => None,
            };
        }

        if self.accounts.page.form.visible {
            return self.handle_account_form_key(key);
        }

        match (key.code, key.modifiers) {
            (KeyCode::Char('j') | KeyCode::Down, _) => {
                if self.accounts.page.selected_index + 1 < self.accounts.page.accounts.len() {
                    self.accounts.page.selected_index += 1;
                }
                None
            }
            (KeyCode::Char('k') | KeyCode::Up, _) => {
                self.accounts.page.selected_index =
                    self.accounts.page.selected_index.saturating_sub(1);
                None
            }
            (KeyCode::Char('n'), _) => Some(Action::OpenAccountFormNew),
            (KeyCode::Char('r'), _) => Some(Action::RefreshAccounts),
            (KeyCode::Char('t'), _) => Some(Action::TestAccountForm),
            (KeyCode::Char('O'), modifiers)
                if plain_or_shift(modifiers)
                    && super::account_result_has_details(
                        self.accounts.page.last_result.as_ref(),
                    ) =>
            {
                self.open_last_account_result_details_modal();
                None
            }
            (KeyCode::Char('d'), _) => Some(Action::SetDefaultAccount),
            (KeyCode::Char('c'), _) => Some(Action::EditConfig),
            (KeyCode::Enter | KeyCode::Char('o'), _) => {
                if let Some(account) = self.selected_account().cloned() {
                    if let Some(config) = account_summary_to_config(&account) {
                        self.accounts.page.form = account_form_from_config(config);
                        self.accounts.page.form.visible = true;
                    } else {
                        self.accounts.page.status = Some(
                            "Runtime-only account is inspectable but not editable here.".into(),
                        );
                    }
                }
                None
            }
            (KeyCode::Esc, _) if self.accounts.page.onboarding_required => None,
            (KeyCode::Esc, _) => Some(Action::OpenMailboxScreen),
            _ => self.contextual_input_action(key),
        }
    }

    fn handle_account_form_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        if self.accounts.page.form.pending_mode_switch.is_some() {
            return match (key.code, key.modifiers) {
                (KeyCode::Enter | KeyCode::Char('y'), _) => {
                    if let Some(mode) = self.accounts.page.form.pending_mode_switch {
                        self.apply_account_form_mode(mode);
                    }
                    None
                }
                (KeyCode::Esc | KeyCode::Char('n'), _) => {
                    self.accounts.page.form.pending_mode_switch = None;
                    None
                }
                _ => None,
            };
        }

        if self.accounts.page.form.editing_field {
            return match (key.code, key.modifiers) {
                (KeyCode::Esc, _) | (KeyCode::Enter, _) => {
                    self.accounts.page.form.editing_field = false;
                    None
                }
                (KeyCode::Tab, _) => {
                    self.accounts.page.form.editing_field = false;
                    self.accounts.page.form.active_field = (self.accounts.page.form.active_field
                        + 1)
                        % self.account_form_field_count();
                    self.accounts.page.form.field_cursor =
                        account_form_field_value(&self.accounts.page.form)
                            .map(|value| value.chars().count())
                            .unwrap_or(0);
                    None
                }
                (KeyCode::BackTab, _) => {
                    self.accounts.page.form.editing_field = false;
                    self.accounts.page.form.active_field =
                        self.accounts.page.form.active_field.saturating_sub(1);
                    self.accounts.page.form.field_cursor =
                        account_form_field_value(&self.accounts.page.form)
                            .map(|value| value.chars().count())
                            .unwrap_or(0);
                    None
                }
                (KeyCode::Left, _) => {
                    self.accounts.page.form.field_cursor =
                        self.accounts.page.form.field_cursor.saturating_sub(1);
                    None
                }
                (KeyCode::Right, _) => {
                    if let Some(value) = account_form_field_value(&self.accounts.page.form) {
                        self.accounts.page.form.field_cursor =
                            (self.accounts.page.form.field_cursor + 1).min(value.chars().count());
                    }
                    None
                }
                (KeyCode::Home, _) => {
                    self.accounts.page.form.field_cursor = 0;
                    None
                }
                (KeyCode::End, _) => {
                    self.accounts.page.form.field_cursor =
                        account_form_field_value(&self.accounts.page.form)
                            .map(|value| value.chars().count())
                            .unwrap_or(0);
                    None
                }
                (KeyCode::Backspace, _) => {
                    delete_account_form_char(&mut self.accounts.page.form, true);
                    self.refresh_account_form_derived_fields();
                    None
                }
                (KeyCode::Delete, _) => {
                    delete_account_form_char(&mut self.accounts.page.form, false);
                    self.refresh_account_form_derived_fields();
                    None
                }
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    insert_account_form_char(&mut self.accounts.page.form, c);
                    self.refresh_account_form_derived_fields();
                    None
                }
                _ => None,
            };
        }

        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => {
                self.maybe_preserve_new_account_form_draft();
                None
            }
            (KeyCode::Left | KeyCode::Char('h'), _) => {
                self.request_account_form_mode_change(false);
                None
            }
            (KeyCode::Right | KeyCode::Char('l'), _) => {
                self.request_account_form_mode_change(true);
                None
            }
            (KeyCode::Char('j') | KeyCode::Down, _) => {
                self.accounts.page.form.active_field =
                    (self.accounts.page.form.active_field + 1) % self.account_form_field_count();
                None
            }
            (KeyCode::Char('k') | KeyCode::Up, _) => {
                self.accounts.page.form.active_field = if self.accounts.page.form.active_field == 0
                {
                    self.account_form_field_count().saturating_sub(1)
                } else {
                    self.accounts.page.form.active_field - 1
                };
                None
            }
            (KeyCode::Tab, _) => {
                if self.accounts.page.form.active_field == 0 {
                    self.request_account_form_mode_change(true);
                } else {
                    self.accounts.page.form.active_field = (self.accounts.page.form.active_field
                        + 1)
                        % self.account_form_field_count();
                }
                None
            }
            (KeyCode::BackTab, _) => {
                if self.accounts.page.form.active_field == 0 {
                    self.request_account_form_mode_change(false);
                } else {
                    self.accounts.page.form.active_field =
                        self.accounts.page.form.active_field.saturating_sub(1);
                }
                None
            }
            (KeyCode::Enter | KeyCode::Char('i'), _) => {
                if account_form_field_is_editable(&self.accounts.page.form) {
                    self.accounts.page.form.editing_field = true;
                    self.accounts.page.form.field_cursor =
                        account_form_field_value(&self.accounts.page.form)
                            .map(|value| value.chars().count())
                            .unwrap_or(0);
                } else {
                    if self.accounts.page.form.active_field == 0 {
                        self.request_account_form_mode_change(true);
                    } else {
                        let _ = self.toggle_current_account_form_field(true);
                    }
                }
                None
            }
            (KeyCode::Char('t'), _) => Some(Action::TestAccountForm),
            (KeyCode::Char('o'), _)
                if super::account_result_has_details(
                    self.accounts.page.form.last_result.as_ref(),
                ) =>
            {
                self.open_last_account_result_details_modal();
                None
            }
            (KeyCode::Char('r'), _)
                if matches!(self.accounts.page.form.mode, AccountFormMode::Gmail) =>
            {
                Some(Action::ReauthorizeAccountForm)
            }
            (KeyCode::Char('s'), _) => Some(Action::SaveAccountForm),
            (KeyCode::Char(' '), _) if self.accounts.page.form.active_field == 0 => {
                self.request_account_form_mode_change(true);
                None
            }
            (KeyCode::Char(' '), _) if self.toggle_current_account_form_field(true) => None,
            _ => None,
        }
    }
}
