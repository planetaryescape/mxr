use super::*;

impl App {
    fn contextual_input_action(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        let action = self.input.handle_key(key)?;
        crate::mxr_tui::action::action_allowed_in_context(&action, self.current_ui_context())
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
            (KeyCode::Char('I'), KeyModifiers::SHIFT) => Some(Action::MarkRead),
            (KeyCode::Char('U'), KeyModifiers::SHIFT) => Some(Action::MarkUnread),
            (KeyCode::Char('l'), KeyModifiers::NONE) => Some(Action::ApplyLabel),
            (KeyCode::Char('v'), KeyModifiers::NONE) => Some(Action::MoveToLabel),
            (KeyCode::Char('D'), KeyModifiers::SHIFT) => Some(Action::Unsubscribe),
            (KeyCode::Char('Z'), KeyModifiers::SHIFT) => Some(Action::Snooze),
            (KeyCode::Char('O'), KeyModifiers::SHIFT) => Some(Action::OpenInBrowser),
            (KeyCode::Char('R'), KeyModifiers::SHIFT) => Some(Action::ToggleReaderMode),
            (KeyCode::Char('S'), KeyModifiers::SHIFT) => Some(Action::ToggleSignature),
            (KeyCode::Char('A'), KeyModifiers::SHIFT) => Some(Action::AttachmentList),
            (KeyCode::Char('L'), KeyModifiers::SHIFT) => Some(Action::OpenLinks),
            (KeyCode::Char('E'), KeyModifiers::SHIFT) => Some(Action::ExportThread),
            _ => None,
        }
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        if self.error_modal.is_some() {
            return match (key.code, key.modifiers) {
                (KeyCode::Esc | KeyCode::Enter, _)
                | (KeyCode::Char('q'), _)
                | (KeyCode::Char('x'), _) => {
                    self.error_modal = None;
                    None
                }
                _ => None,
            };
        }

        if self.help_modal_open {
            return match (key.code, key.modifiers) {
                (KeyCode::Esc | KeyCode::Enter, _)
                | (KeyCode::Char('?'), _)
                | (KeyCode::Char('q'), _) => Some(Action::Help),
                (KeyCode::Char('j') | KeyCode::Down, _) => {
                    self.help_scroll_offset = self.help_scroll_offset.saturating_add(1);
                    None
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    self.help_scroll_offset = self.help_scroll_offset.saturating_sub(1);
                    None
                }
                (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                    self.help_scroll_offset = self.help_scroll_offset.saturating_add(8);
                    None
                }
                (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                    self.help_scroll_offset = self.help_scroll_offset.saturating_sub(8);
                    None
                }
                (KeyCode::Char('o'), _) => Some(Action::ShowOnboarding),
                _ => None,
            };
        }

        if self.onboarding.visible {
            return match (key.code, key.modifiers) {
                (KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Right | KeyCode::Char('l'), _) => {
                    self.advance_feature_onboarding();
                    None
                }
                (KeyCode::Left | KeyCode::Char('h'), _) => {
                    self.onboarding.step = self.onboarding.step.saturating_sub(1);
                    None
                }
                (KeyCode::Esc | KeyCode::Char('q'), _) => {
                    self.dismiss_feature_onboarding();
                    None
                }
                _ => None,
            };
        }

        if self.command_palette.visible {
            match (key.code, key.modifiers) {
                (KeyCode::Enter, _) => return self.command_palette.confirm(),
                (KeyCode::Esc, _) => return Some(Action::CloseCommandPalette),
                (KeyCode::Backspace, _) => {
                    self.command_palette.on_backspace();
                    return None;
                }
                (KeyCode::Down, _) | (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
                    self.command_palette.select_next();
                    return None;
                }
                (KeyCode::Up, _) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                    self.command_palette.select_prev();
                    return None;
                }
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    self.command_palette.on_char(c);
                    return None;
                }
                _ => return None,
            }
        }

        // Route keys to search bar when active
        if self.search_bar.active {
            match (key.code, key.modifiers) {
                (KeyCode::Enter, _) => return Some(Action::SubmitSearch),
                (KeyCode::Tab, _) => return Some(Action::CycleSearchMode),
                (KeyCode::Esc, _) => return Some(Action::CloseSearch),
                (KeyCode::Backspace, _) => {
                    self.search_bar.on_backspace();
                    // Live filter as you type
                    self.trigger_live_search();
                    return None;
                }
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    self.search_bar.on_char(c);
                    // Live filter as you type
                    self.trigger_live_search();
                    return None;
                }
                _ => return None,
            }
        }

        // Route keys to send confirmation prompt
        if self.pending_send_confirm.is_some() {
            match (key.code, key.modifiers) {
                (KeyCode::Char('s'), KeyModifiers::NONE) => {
                    // Send
                    if let Some(pending) = self.pending_send_confirm.take() {
                        if !pending.allow_send {
                            self.pending_send_confirm = Some(pending);
                            return None;
                        }
                        let parse_addrs =
                            |s: &str| crate::mxr_compose::parse::parse_address_list(s);
                        let reply_headers = pending.fm.in_reply_to.as_ref().map(|in_reply_to| {
                            crate::mxr_core::types::ReplyHeaders {
                                in_reply_to: in_reply_to.clone(),
                                references: pending.fm.references.clone(),
                            }
                        });
                        let account_id = self
                            .envelopes
                            .first()
                            .or(self.all_envelopes.first())
                            .map(|e| e.account_id.clone())
                            .unwrap_or_default();
                        let now = chrono::Utc::now();
                        let draft = crate::mxr_core::Draft {
                            id: crate::mxr_core::id::DraftId::new(),
                            account_id,
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
                        let _ = std::fs::remove_file(&pending.draft_path);
                    }
                    return None;
                }
                (KeyCode::Char('d'), KeyModifiers::NONE) => {
                    // Save as draft to mail server
                    if let Some(pending) = self.pending_send_confirm.take() {
                        if !pending.allow_send {
                            self.pending_send_confirm = Some(pending);
                            return None;
                        }
                        let parse_addrs =
                            |s: &str| crate::mxr_compose::parse::parse_address_list(s);
                        let reply_headers = pending.fm.in_reply_to.as_ref().map(|in_reply_to| {
                            crate::mxr_core::types::ReplyHeaders {
                                in_reply_to: in_reply_to.clone(),
                                references: pending.fm.references.clone(),
                            }
                        });
                        let account_id = self
                            .envelopes
                            .first()
                            .or(self.all_envelopes.first())
                            .map(|e| e.account_id.clone())
                            .unwrap_or_default();
                        let now = chrono::Utc::now();
                        let draft = crate::mxr_core::Draft {
                            id: crate::mxr_core::id::DraftId::new(),
                            account_id,
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
                        let _ = std::fs::remove_file(&pending.draft_path);
                    }
                    return None;
                }
                (KeyCode::Char('e'), KeyModifiers::NONE) => {
                    // Edit again — reopen editor
                    if let Some(pending) = self.pending_send_confirm.take() {
                        self.pending_compose = Some(ComposeAction::EditDraft(pending.draft_path));
                    }
                    return None;
                }
                (KeyCode::Esc, _) => {
                    // Discard
                    if let Some(pending) = self.pending_send_confirm.take() {
                        let _ = std::fs::remove_file(&pending.draft_path);
                        self.status_message = Some("Discarded".into());
                    }
                    return None;
                }
                _ => return None,
            }
        }

        if self.pending_bulk_confirm.is_some() {
            return match (key.code, key.modifiers) {
                (KeyCode::Enter, _)
                | (KeyCode::Char('y'), KeyModifiers::NONE)
                | (KeyCode::Char('Y'), KeyModifiers::SHIFT) => Some(Action::OpenSelected),
                (KeyCode::Esc, _) | (KeyCode::Char('n'), KeyModifiers::NONE) => {
                    self.pending_bulk_confirm = None;
                    self.status_message = Some("Bulk action cancelled".into());
                    None
                }
                _ => None,
            };
        }

        if self.pending_unsubscribe_confirm.is_some() {
            return match (key.code, key.modifiers) {
                (KeyCode::Enter, _)
                | (KeyCode::Char('u'), KeyModifiers::NONE)
                | (KeyCode::Char('U'), KeyModifiers::SHIFT) => Some(Action::ConfirmUnsubscribeOnly),
                (KeyCode::Char('a'), KeyModifiers::NONE)
                | (KeyCode::Char('A'), KeyModifiers::SHIFT) => {
                    Some(Action::ConfirmUnsubscribeAndArchiveSender)
                }
                (KeyCode::Esc, _) => Some(Action::CancelUnsubscribe),
                _ => None,
            };
        }

        if self.snooze_panel.visible {
            match (key.code, key.modifiers) {
                (KeyCode::Enter, _) => return Some(Action::Snooze),
                (KeyCode::Esc, _) => {
                    self.snooze_panel.visible = false;
                    return None;
                }
                (KeyCode::Char('j') | KeyCode::Down, _) => {
                    self.snooze_panel.selected_index =
                        (self.snooze_panel.selected_index + 1) % snooze_presets().len();
                    return None;
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    self.snooze_panel.selected_index = self
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
        if let Some(ref mut url_state) = self.url_modal {
            match (key.code, key.modifiers) {
                (KeyCode::Enter | KeyCode::Char('o'), _) => {
                    if let Some(url) = url_state.selected_url().map(|s| s.to_string()) {
                        ui::url_modal::open_url(&url);
                        self.status_message = Some(format!("Opening {url}"));
                    }
                    self.url_modal = None;
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
                    self.url_modal = None;
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
                    self.url_modal = None;
                    return None;
                }
                _ => return None,
            }
        }

        // Route keys to compose picker when active
        if self.attachment_panel.visible {
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
                    if self.attachment_panel.selected_index + 1
                        < self.attachment_panel.attachments.len()
                    {
                        self.attachment_panel.selected_index += 1;
                    }
                    return None;
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    self.attachment_panel.selected_index =
                        self.attachment_panel.selected_index.saturating_sub(1);
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
        if self.compose_picker.visible {
            match (key.code, key.modifiers) {
                (KeyCode::Enter, _) => {
                    // Confirm all recipients and trigger compose
                    let to = self.compose_picker.confirm();
                    if to.is_empty() {
                        self.pending_compose = Some(ComposeAction::New);
                    } else {
                        self.pending_compose = Some(ComposeAction::NewWithTo(to));
                    }
                    return None;
                }
                (KeyCode::Tab, _) => {
                    // Tab adds selected contact to recipients
                    self.compose_picker.add_recipient();
                    return None;
                }
                (KeyCode::Esc, _) => {
                    self.compose_picker.close();
                    return None;
                }
                (KeyCode::Backspace, _) => {
                    self.compose_picker.on_backspace();
                    return None;
                }
                (KeyCode::Down, _) | (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
                    self.compose_picker.select_next();
                    return None;
                }
                (KeyCode::Up, _) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                    self.compose_picker.select_prev();
                    return None;
                }
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    self.compose_picker.on_char(c);
                    return None;
                }
                _ => return None,
            }
        }

        // Route keys to label picker when active
        if self.label_picker.visible {
            match (key.code, key.modifiers) {
                (KeyCode::Enter, _) => {
                    let mode = self.label_picker.mode;
                    if let Some(label_name) = self.label_picker.confirm() {
                        self.pending_label_action = Some((mode, label_name));
                        return match mode {
                            LabelPickerMode::Apply => Some(Action::ApplyLabel),
                            LabelPickerMode::Move => Some(Action::MoveToLabel),
                        };
                    }
                    return None;
                }
                (KeyCode::Esc, _) => {
                    self.label_picker.close();
                    return None;
                }
                (KeyCode::Backspace, _) => {
                    self.label_picker.on_backspace();
                    return None;
                }
                (KeyCode::Down, _) | (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
                    self.label_picker.select_next();
                    return None;
                }
                (KeyCode::Up, _) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                    self.label_picker.select_prev();
                    return None;
                }
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    self.label_picker.on_char(c);
                    return None;
                }
                _ => return None,
            }
        }

        if self.screen != Screen::Mailbox {
            return self.handle_screen_key(key);
        }

        // Route keys based on active pane
        match self.active_pane {
            ActivePane::MessageView => match (key.code, key.modifiers) {
                (KeyCode::Char('/'), KeyModifiers::NONE) => Some(Action::OpenGlobalSearch),
                (KeyCode::Char('f'), KeyModifiers::CONTROL) => Some(Action::OpenMailboxFilter),
                (KeyCode::Char('j') | KeyCode::Down, _) => {
                    self.move_thread_focus_down();
                    None
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    self.move_thread_focus_up();
                    None
                }
                (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                    self.message_scroll_offset = self.message_scroll_offset.saturating_add(20);
                    None
                }
                (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                    self.message_scroll_offset = self.message_scroll_offset.saturating_sub(20);
                    None
                }
                (KeyCode::Char('G'), KeyModifiers::SHIFT) => {
                    self.message_scroll_offset = u16::MAX;
                    None
                }
                // h = move left to mail list
                (KeyCode::Char('h') | KeyCode::Left, KeyModifiers::NONE) => {
                    self.active_pane = ActivePane::MailList;
                    None
                }
                // o = open in browser (message already open in pane)
                (KeyCode::Char('o'), KeyModifiers::NONE) => Some(Action::OpenInBrowser),
                // L = open links picker
                (KeyCode::Char('L'), KeyModifiers::SHIFT) => Some(Action::OpenLinks),
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
                    self.active_pane = ActivePane::Sidebar;
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
        if self.search_page.editing {
            return match (key.code, key.modifiers) {
                (KeyCode::Enter, _) => Some(Action::SubmitSearch),
                (KeyCode::Esc, _) => {
                    self.search_page.editing = false;
                    None
                }
                (KeyCode::Backspace, _) => {
                    self.search_page.query.pop();
                    self.trigger_live_search();
                    None
                }
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    self.search_page.query.push(c);
                    self.trigger_live_search();
                    None
                }
                _ => None,
            };
        }

        match self.search_page.active_pane {
            SearchPane::Results => match (key.code, key.modifiers) {
                (KeyCode::Char('/'), _) => {
                    self.search_page.editing = true;
                    None
                }
                (KeyCode::Char('j') | KeyCode::Down, _) => {
                    if self.search_page.selected_index + 1 < self.search_row_count() {
                        self.search_page.selected_index += 1;
                        self.ensure_search_visible();
                    }
                    self.maybe_load_more_search_results();
                    None
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    if self.search_page.selected_index > 0 {
                        self.search_page.selected_index -= 1;
                        self.ensure_search_visible();
                    }
                    None
                }
                (KeyCode::Char('l') | KeyCode::Right, KeyModifiers::NONE)
                | (KeyCode::Enter | KeyCode::Char('o'), _) => Some(Action::OpenSelected),
                (KeyCode::Esc, _) => Some(Action::OpenMailboxScreen),
                _ => self.contextual_input_action(key),
            },
            SearchPane::Preview => match (key.code, key.modifiers) {
                (KeyCode::Char('/'), _) => {
                    self.search_page.editing = true;
                    None
                }
                (KeyCode::Char('h') | KeyCode::Left, KeyModifiers::NONE) => {
                    self.search_page.active_pane = SearchPane::Results;
                    None
                }
                (KeyCode::Char('j') | KeyCode::Down, _) => {
                    self.move_thread_focus_down();
                    None
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    self.move_thread_focus_up();
                    None
                }
                (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                    self.message_scroll_offset = self.message_scroll_offset.saturating_add(20);
                    None
                }
                (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                    self.message_scroll_offset = self.message_scroll_offset.saturating_sub(20);
                    None
                }
                (KeyCode::Char('G'), KeyModifiers::SHIFT) => {
                    self.message_scroll_offset = u16::MAX;
                    None
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
        if self.rules_page.form.visible {
            return self.handle_rule_form_key(key);
        }

        match (key.code, key.modifiers) {
            (KeyCode::Char('j') | KeyCode::Down, _) => {
                if self.rules_page.selected_index + 1 < self.rules_page.rules.len() {
                    self.rules_page.selected_index += 1;
                    self.refresh_selected_rule_panel();
                }
                None
            }
            (KeyCode::Char('k') | KeyCode::Up, _) => {
                self.rules_page.selected_index = self.rules_page.selected_index.saturating_sub(1);
                self.refresh_selected_rule_panel();
                None
            }
            (KeyCode::Enter | KeyCode::Char('o'), _) => Some(Action::RefreshRules),
            (KeyCode::Char('e'), _) => Some(Action::ToggleRuleEnabled),
            (KeyCode::Char('D'), KeyModifiers::SHIFT) => Some(Action::ShowRuleDryRun),
            (KeyCode::Char('H'), KeyModifiers::SHIFT) => Some(Action::ShowRuleHistory),
            (KeyCode::Char('#'), _) => Some(Action::DeleteRule),
            (KeyCode::Char('n'), _) => Some(Action::OpenRuleFormNew),
            (KeyCode::Char('E'), KeyModifiers::SHIFT) => Some(Action::OpenRuleFormEdit),
            (KeyCode::Esc, _) => Some(Action::OpenMailboxScreen),
            _ => self.contextual_input_action(key),
        }
    }

    fn handle_rule_form_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => {
                self.rules_page.form.visible = false;
                self.rules_page.panel = RulesPanel::Details;
                None
            }
            (KeyCode::Tab, _) => {
                self.rules_page.form.active_field = (self.rules_page.form.active_field + 1) % 5;
                None
            }
            (KeyCode::BackTab, _) => {
                self.rules_page.form.active_field = if self.rules_page.form.active_field == 0 {
                    4
                } else {
                    self.rules_page.form.active_field - 1
                };
                None
            }
            (KeyCode::Char(' '), _) if self.rules_page.form.active_field == 4 => {
                self.rules_page.form.enabled = !self.rules_page.form.enabled;
                None
            }
            (KeyCode::Char('s'), KeyModifiers::CONTROL) => Some(Action::SaveRuleForm),
            (_, _) if self.rules_page.form.active_field == 1 => {
                self.rule_condition_editor.input(key);
                self.sync_rule_form_strings_from_editors();
                None
            }
            (_, _) if self.rules_page.form.active_field == 2 => {
                self.rule_action_editor.input(key);
                self.sync_rule_form_strings_from_editors();
                None
            }
            (KeyCode::Backspace, _) => {
                match self.rules_page.form.active_field {
                    0 => {
                        self.rules_page.form.name.pop();
                    }
                    3 => {
                        self.rules_page.form.priority.pop();
                    }
                    _ => {}
                }
                None
            }
            (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                match self.rules_page.form.active_field {
                    0 => self.rules_page.form.name.push(c),
                    3 => self.rules_page.form.priority.push(c),
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
                self.diagnostics_page.selected_pane = self.diagnostics_page.selected_pane.next();
                None
            }
            (KeyCode::BackTab | KeyCode::Left, _) => {
                self.diagnostics_page.selected_pane = self.diagnostics_page.selected_pane.prev();
                None
            }
            (KeyCode::Char('j') | KeyCode::Down, _) => {
                self.diagnostics_page.selected_pane = self.diagnostics_page.selected_pane.next();
                None
            }
            (KeyCode::Char('k') | KeyCode::Up, _) => {
                self.diagnostics_page.selected_pane = self.diagnostics_page.selected_pane.prev();
                None
            }
            (KeyCode::Char('d'), KeyModifiers::CONTROL) | (KeyCode::PageDown, _) => {
                let pane = self.diagnostics_page.active_pane();
                *self.diagnostics_page.scroll_offset_mut(pane) =
                    self.diagnostics_page.scroll_offset(pane).saturating_add(8);
                None
            }
            (KeyCode::Char('u'), KeyModifiers::CONTROL) | (KeyCode::PageUp, _) => {
                let pane = self.diagnostics_page.active_pane();
                *self.diagnostics_page.scroll_offset_mut(pane) =
                    self.diagnostics_page.scroll_offset(pane).saturating_sub(8);
                None
            }
            (KeyCode::Enter | KeyCode::Char('o'), _) => {
                self.diagnostics_page.toggle_fullscreen();
                None
            }
            (KeyCode::Char('d'), _) => Some(Action::OpenDiagnosticsPaneDetails),
            (KeyCode::Char('r'), _) => Some(Action::RefreshDiagnostics),
            (KeyCode::Char('b'), _) => Some(Action::GenerateBugReport),
            (KeyCode::Char('c'), _) => Some(Action::EditConfig),
            (KeyCode::Char('L'), KeyModifiers::SHIFT) => Some(Action::OpenLogs),
            (KeyCode::Esc, _) => Some(Action::OpenMailboxScreen),
            _ => self.contextual_input_action(key),
        }
    }

    fn handle_accounts_screen_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        if self.accounts_page.onboarding_modal_open {
            return match (key.code, key.modifiers) {
                (KeyCode::Enter | KeyCode::Char(' '), _) => {
                    self.complete_account_setup_onboarding();
                    None
                }
                (KeyCode::Char('q'), _) => Some(Action::QuitView),
                (KeyCode::Esc, _) => {
                    self.accounts_page.onboarding_modal_open = false;
                    None
                }
                _ => None,
            };
        }

        if self.accounts_page.form.visible {
            return self.handle_account_form_key(key);
        }

        match (key.code, key.modifiers) {
            (KeyCode::Char('j') | KeyCode::Down, _) => {
                if self.accounts_page.selected_index + 1 < self.accounts_page.accounts.len() {
                    self.accounts_page.selected_index += 1;
                }
                None
            }
            (KeyCode::Char('k') | KeyCode::Up, _) => {
                self.accounts_page.selected_index =
                    self.accounts_page.selected_index.saturating_sub(1);
                None
            }
            (KeyCode::Char('n'), _) => Some(Action::OpenAccountFormNew),
            (KeyCode::Char('r'), _) => Some(Action::RefreshAccounts),
            (KeyCode::Char('t'), _) => Some(Action::TestAccountForm),
            (KeyCode::Char('d'), _) => Some(Action::SetDefaultAccount),
            (KeyCode::Char('c'), _) => Some(Action::EditConfig),
            (KeyCode::Enter | KeyCode::Char('o'), _) => {
                if let Some(account) = self.selected_account().cloned() {
                    if let Some(config) = account_summary_to_config(&account) {
                        self.accounts_page.form = account_form_from_config(config);
                        self.accounts_page.form.visible = true;
                    } else {
                        self.accounts_page.status = Some(
                            "Runtime-only account is inspectable but not editable here.".into(),
                        );
                    }
                }
                None
            }
            (KeyCode::Esc, _) if self.accounts_page.onboarding_required => None,
            (KeyCode::Esc, _) => Some(Action::OpenMailboxScreen),
            _ => self.contextual_input_action(key),
        }
    }

    fn handle_account_form_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        if self.accounts_page.form.pending_mode_switch.is_some() {
            return match (key.code, key.modifiers) {
                (KeyCode::Enter | KeyCode::Char('y'), _) => {
                    if let Some(mode) = self.accounts_page.form.pending_mode_switch {
                        self.apply_account_form_mode(mode);
                    }
                    None
                }
                (KeyCode::Esc | KeyCode::Char('n'), _) => {
                    self.accounts_page.form.pending_mode_switch = None;
                    None
                }
                _ => None,
            };
        }

        if self.accounts_page.form.editing_field {
            return match (key.code, key.modifiers) {
                (KeyCode::Esc, _) | (KeyCode::Enter, _) => {
                    self.accounts_page.form.editing_field = false;
                    None
                }
                (KeyCode::Tab, _) => {
                    self.accounts_page.form.editing_field = false;
                    self.accounts_page.form.active_field = (self.accounts_page.form.active_field
                        + 1)
                        % self.account_form_field_count();
                    self.accounts_page.form.field_cursor =
                        account_form_field_value(&self.accounts_page.form)
                            .map(|value| value.chars().count())
                            .unwrap_or(0);
                    None
                }
                (KeyCode::BackTab, _) => {
                    self.accounts_page.form.editing_field = false;
                    self.accounts_page.form.active_field =
                        self.accounts_page.form.active_field.saturating_sub(1);
                    self.accounts_page.form.field_cursor =
                        account_form_field_value(&self.accounts_page.form)
                            .map(|value| value.chars().count())
                            .unwrap_or(0);
                    None
                }
                (KeyCode::Left, _) => {
                    self.accounts_page.form.field_cursor =
                        self.accounts_page.form.field_cursor.saturating_sub(1);
                    None
                }
                (KeyCode::Right, _) => {
                    if let Some(value) = account_form_field_value(&self.accounts_page.form) {
                        self.accounts_page.form.field_cursor =
                            (self.accounts_page.form.field_cursor + 1).min(value.chars().count());
                    }
                    None
                }
                (KeyCode::Home, _) => {
                    self.accounts_page.form.field_cursor = 0;
                    None
                }
                (KeyCode::End, _) => {
                    self.accounts_page.form.field_cursor =
                        account_form_field_value(&self.accounts_page.form)
                            .map(|value| value.chars().count())
                            .unwrap_or(0);
                    None
                }
                (KeyCode::Backspace, _) => {
                    delete_account_form_char(&mut self.accounts_page.form, true);
                    self.refresh_account_form_derived_fields();
                    None
                }
                (KeyCode::Delete, _) => {
                    delete_account_form_char(&mut self.accounts_page.form, false);
                    self.refresh_account_form_derived_fields();
                    None
                }
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    insert_account_form_char(&mut self.accounts_page.form, c);
                    self.refresh_account_form_derived_fields();
                    None
                }
                _ => None,
            };
        }

        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => {
                self.accounts_page.form.visible = false;
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
                self.accounts_page.form.active_field =
                    (self.accounts_page.form.active_field + 1) % self.account_form_field_count();
                None
            }
            (KeyCode::Char('k') | KeyCode::Up, _) => {
                self.accounts_page.form.active_field = if self.accounts_page.form.active_field == 0
                {
                    self.account_form_field_count().saturating_sub(1)
                } else {
                    self.accounts_page.form.active_field - 1
                };
                None
            }
            (KeyCode::Tab, _) => {
                if self.accounts_page.form.active_field == 0 {
                    self.request_account_form_mode_change(true);
                } else {
                    self.accounts_page.form.active_field = (self.accounts_page.form.active_field
                        + 1)
                        % self.account_form_field_count();
                }
                None
            }
            (KeyCode::BackTab, _) => {
                if self.accounts_page.form.active_field == 0 {
                    self.request_account_form_mode_change(false);
                } else {
                    self.accounts_page.form.active_field =
                        self.accounts_page.form.active_field.saturating_sub(1);
                }
                None
            }
            (KeyCode::Enter | KeyCode::Char('i'), _) => {
                if account_form_field_is_editable(&self.accounts_page.form) {
                    self.accounts_page.form.editing_field = true;
                    self.accounts_page.form.field_cursor =
                        account_form_field_value(&self.accounts_page.form)
                            .map(|value| value.chars().count())
                            .unwrap_or(0);
                    None
                } else if self.accounts_page.form.active_field == 0 {
                    self.request_account_form_mode_change(true);
                    None
                } else if matches!(self.accounts_page.form.mode, AccountFormMode::Gmail)
                    && self.accounts_page.form.active_field == 4
                {
                    self.accounts_page.form.gmail_credential_source = next_gmail_credential_source(
                        self.accounts_page.form.gmail_credential_source.clone(),
                        true,
                    );
                    self.accounts_page.form.active_field = self
                        .accounts_page
                        .form
                        .active_field
                        .min(self.account_form_field_count().saturating_sub(1));
                    None
                } else {
                    None
                }
            }
            (KeyCode::Char('t'), _) => Some(Action::TestAccountForm),
            (KeyCode::Char('r'), _)
                if matches!(self.accounts_page.form.mode, AccountFormMode::Gmail) =>
            {
                Some(Action::ReauthorizeAccountForm)
            }
            (KeyCode::Char('s'), _) => Some(Action::SaveAccountForm),
            (KeyCode::Char(' '), _) if self.accounts_page.form.active_field == 0 => {
                self.request_account_form_mode_change(true);
                None
            }
            (KeyCode::Char(' '), _)
                if matches!(self.accounts_page.form.mode, AccountFormMode::Gmail)
                    && self.accounts_page.form.active_field == 4 =>
            {
                self.accounts_page.form.gmail_credential_source = next_gmail_credential_source(
                    self.accounts_page.form.gmail_credential_source.clone(),
                    true,
                );
                self.accounts_page.form.active_field = self
                    .accounts_page
                    .form
                    .active_field
                    .min(self.account_form_field_count().saturating_sub(1));
                None
            }
            _ => None,
        }
    }
}
