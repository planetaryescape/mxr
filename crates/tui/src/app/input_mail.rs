use super::*;

impl App {
    pub(super) fn mail_action_key(&self, key: crossterm::event::KeyEvent) -> Option<Action> {
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

    pub(super) fn handle_send_confirm_key(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> Option<Action> {
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
                None
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
                None
            }
            (KeyCode::Char('e'), KeyModifiers::NONE) => {
                // Edit again — reopen editor
                if let Some(pending) = self.pending_send_confirm.take() {
                    self.pending_compose = Some(ComposeAction::EditDraft(pending.draft_path));
                }
                None
            }
            (KeyCode::Esc, _) => {
                // Discard
                if let Some(pending) = self.pending_send_confirm.take() {
                    let _ = std::fs::remove_file(&pending.draft_path);
                    self.status_message = Some("Discarded".into());
                }
                None
            }
            _ => None,
        }
    }

    pub(super) fn handle_bulk_confirm_key(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> Option<Action> {
        match (key.code, key.modifiers) {
            (KeyCode::Enter, _)
            | (KeyCode::Char('y'), KeyModifiers::NONE)
            | (KeyCode::Char('Y'), KeyModifiers::SHIFT) => Some(Action::OpenSelected),
            (KeyCode::Esc, _) | (KeyCode::Char('n'), KeyModifiers::NONE) => {
                self.pending_bulk_confirm = None;
                self.status_message = Some("Bulk action cancelled".into());
                None
            }
            _ => None,
        }
    }

    pub(super) fn handle_unsubscribe_confirm_key(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> Option<Action> {
        match (key.code, key.modifiers) {
            (KeyCode::Enter, _)
            | (KeyCode::Char('u'), KeyModifiers::NONE)
            | (KeyCode::Char('U'), KeyModifiers::SHIFT) => Some(Action::ConfirmUnsubscribeOnly),
            (KeyCode::Char('a'), KeyModifiers::NONE)
            | (KeyCode::Char('A'), KeyModifiers::SHIFT) => {
                Some(Action::ConfirmUnsubscribeAndArchiveSender)
            }
            (KeyCode::Esc, _) => Some(Action::CancelUnsubscribe),
            _ => None,
        }
    }

    pub(super) fn handle_snooze_panel_key(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> Option<Action> {
        match (key.code, key.modifiers) {
            (KeyCode::Enter, _) => Some(Action::Snooze),
            (KeyCode::Esc, _) => {
                self.snooze_panel.visible = false;
                None
            }
            (KeyCode::Char('j') | KeyCode::Down, _) => {
                self.snooze_panel.selected_index =
                    (self.snooze_panel.selected_index + 1) % snooze_presets().len();
                None
            }
            (KeyCode::Char('k') | KeyCode::Up, _) => {
                self.snooze_panel.selected_index = self
                    .snooze_panel
                    .selected_index
                    .checked_sub(1)
                    .unwrap_or(snooze_presets().len() - 1);
                None
            }
            _ => None,
        }
    }

    pub(super) fn handle_url_modal_key(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> Option<Action> {
        let url_state = self.url_modal.as_mut().unwrap();
        match (key.code, key.modifiers) {
            (KeyCode::Enter | KeyCode::Char('o'), _) => {
                if let Some(url) = url_state.selected_url().map(ToString::to_string) {
                    ui::url_modal::open_url(&url);
                    self.status_message = Some(format!("Opening {url}"));
                }
                self.url_modal = None;
                None
            }
            (KeyCode::Char('y'), _) => {
                if let Some(url) = url_state.selected_url().map(ToString::to_string) {
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
                None
            }
            (KeyCode::Char('j') | KeyCode::Down, _) => {
                url_state.next();
                None
            }
            (KeyCode::Char('k') | KeyCode::Up, _) => {
                url_state.prev();
                None
            }
            (KeyCode::Esc | KeyCode::Char('q'), _) => {
                self.url_modal = None;
                None
            }
            _ => None,
        }
    }

    pub(super) fn handle_attachment_panel_key(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> Option<Action> {
        match (key.code, key.modifiers) {
            (KeyCode::Enter | KeyCode::Char('o'), _) => {
                self.queue_attachment_action(AttachmentOperation::Open);
                None
            }
            (KeyCode::Char('d'), _) => {
                self.queue_attachment_action(AttachmentOperation::Download);
                None
            }
            (KeyCode::Char('j') | KeyCode::Down, _) => {
                if self.attachment_panel.selected_index + 1
                    < self.attachment_panel.attachments.len()
                {
                    self.attachment_panel.selected_index += 1;
                }
                None
            }
            (KeyCode::Char('k') | KeyCode::Up, _) => {
                self.attachment_panel.selected_index =
                    self.attachment_panel.selected_index.saturating_sub(1);
                None
            }
            (KeyCode::Esc | KeyCode::Char('A'), _) => {
                self.close_attachment_panel();
                None
            }
            _ => None,
        }
    }

    pub(super) fn handle_compose_picker_key(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> Option<Action> {
        match (key.code, key.modifiers) {
            (KeyCode::Enter, _) => {
                // Confirm all recipients and trigger compose
                let to = self.compose_picker.confirm();
                if to.is_empty() {
                    self.pending_compose = Some(ComposeAction::New);
                } else {
                    self.pending_compose = Some(ComposeAction::NewWithTo(to));
                }
                None
            }
            (KeyCode::Tab, _) => {
                // Tab adds selected contact to recipients
                self.compose_picker.add_recipient();
                None
            }
            (KeyCode::Esc, _) => {
                self.compose_picker.close();
                None
            }
            (KeyCode::Backspace, _) => {
                self.compose_picker.on_backspace();
                None
            }
            (KeyCode::Down, _) | (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
                self.compose_picker.select_next();
                None
            }
            (KeyCode::Up, _) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                self.compose_picker.select_prev();
                None
            }
            (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                self.compose_picker.on_char(c);
                None
            }
            _ => None,
        }
    }

    pub(super) fn handle_label_picker_key(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> Option<Action> {
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
                None
            }
            (KeyCode::Esc, _) => {
                self.label_picker.close();
                None
            }
            (KeyCode::Backspace, _) => {
                self.label_picker.on_backspace();
                None
            }
            (KeyCode::Down, _) | (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
                self.label_picker.select_next();
                None
            }
            (KeyCode::Up, _) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                self.label_picker.select_prev();
                None
            }
            (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                self.label_picker.on_char(c);
                None
            }
            _ => None,
        }
    }
}
