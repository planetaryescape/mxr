use super::*;

#[derive(Debug, Clone, Copy)]
pub enum SavePathPreset {
    Downloads,
    Desktop,
    Cwd,
}

impl App {
    pub fn selected_attachment(&self) -> Option<&AttachmentMeta> {
        self.mailbox
            .attachment_panel
            .attachments
            .get(self.mailbox.attachment_panel.selected_index)
    }

    pub fn open_attachment_panel(&mut self) {
        let Some(message_id) = self
            .mailbox
            .viewing_envelope
            .as_ref()
            .map(|env| env.id.clone())
        else {
            self.status_message = Some("No message selected".into());
            return;
        };
        let Some(attachments) = self
            .current_viewing_body()
            .map(Self::sorted_attachment_panel_attachments)
        else {
            self.status_message = Some("No message body loaded".into());
            return;
        };
        if attachments.is_empty() {
            self.status_message = Some("No attachments".into());
            return;
        }

        self.mailbox.attachment_panel.visible = true;
        self.mailbox.attachment_panel.message_id = Some(message_id);
        self.mailbox.attachment_panel.attachments = attachments;
        self.mailbox.attachment_panel.selected_index = 0;
        self.mailbox.attachment_panel.status = None;
    }

    pub fn open_url_modal(&mut self) {
        let body = self.current_viewing_body();
        let Some(body) = body else {
            self.status_message = Some("No message body loaded".into());
            return;
        };
        let text_plain = body.text_plain.as_deref();
        let text_html = body.text_html.as_deref();
        let urls = ui::url_modal::extract_urls(text_plain, text_html);
        if urls.is_empty() {
            self.status_message = Some("No links found".into());
            return;
        }
        self.mailbox.url_modal = Some(ui::url_modal::UrlModalState::new(urls));
    }

    pub fn close_attachment_panel(&mut self) {
        self.mailbox.attachment_panel = AttachmentPanelState::default();
        self.mailbox.pending_attachment_action = None;
    }

    /// Open the "save attachment as..." modal with a user-chosen destination.
    /// Triggered by `d` in the attachment list.
    pub fn open_save_attachment_modal(&mut self) {
        let Some(message_id) = self.mailbox.attachment_panel.message_id.clone() else {
            return;
        };
        let Some(attachment) = self.selected_attachment().cloned() else {
            return;
        };
        let filename = attachment.filename.clone();
        let prefilled = self.download_dir.join(&filename).display().to_string();
        self.modals
            .save_attachment
            .open(message_id, attachment.id, filename, prefilled);
    }

    /// Apply a numbered preset (1=Downloads, 2=Desktop, 3=cwd) to the
    /// save modal's input, preserving the current filename so users
    /// just swap the directory.
    pub fn save_attachment_apply_preset(&mut self, preset: SavePathPreset) {
        let filename = self.modals.save_attachment.filename.clone();
        if filename.is_empty() {
            return;
        }
        let dir = match preset {
            SavePathPreset::Downloads => self.download_dir.clone(),
            SavePathPreset::Desktop => dirs::desktop_dir()
                .or_else(|| dirs::home_dir().map(|h| h.join("Desktop")))
                .unwrap_or_else(|| std::path::PathBuf::from(".")),
            SavePathPreset::Cwd => {
                std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
            }
        };
        self.modals.save_attachment.input = dir.join(&filename).display().to_string();
        self.modals.save_attachment.error = None;
        self.modals.save_attachment.awaiting_overwrite_confirm = false;
    }

    /// Confirm the save modal: validate the path, request overwrite
    /// confirmation if the file exists, then queue the download IPC.
    /// Returns `true` if the modal should remain open (waiting for
    /// overwrite confirm or showing an error), `false` if the user
    /// can move on.
    pub fn save_attachment_confirm(&mut self) {
        let raw = self.modals.save_attachment.input.trim().to_string();
        if raw.is_empty() {
            self.modals.save_attachment.error = Some("Path cannot be empty".into());
            return;
        }
        let expanded = match shellexpand::full(&raw) {
            Ok(value) => std::path::PathBuf::from(value.into_owned()),
            Err(err) => {
                self.modals.save_attachment.error = Some(format!("Bad path: {err}"));
                return;
            }
        };
        if expanded.is_dir() {
            self.modals.save_attachment.error =
                Some("Destination is a directory — include a filename".into());
            return;
        }
        if expanded.exists() && !self.modals.save_attachment.awaiting_overwrite_confirm {
            self.modals.save_attachment.awaiting_overwrite_confirm = true;
            self.modals.save_attachment.error = None;
            return;
        }
        // Path resolved; queue the download with the user's chosen destination.
        let message_id = match self.modals.save_attachment.message_id.clone() {
            Some(id) => id,
            None => {
                self.modals.save_attachment.close();
                return;
            }
        };
        let attachment_id = match self.modals.save_attachment.attachment_id.clone() {
            Some(id) => id,
            None => {
                self.modals.save_attachment.close();
                return;
            }
        };
        // Re-select the attachment in the panel so queue_attachment_action_to picks it up.
        if let Some(idx) = self
            .mailbox
            .attachment_panel
            .attachments
            .iter()
            .position(|att| att.id == attachment_id)
        {
            self.mailbox.attachment_panel.selected_index = idx;
        }
        // Ensure the panel knows which message we're acting on.
        self.mailbox.attachment_panel.message_id = Some(message_id);
        self.queue_attachment_action_to(
            AttachmentOperation::Download,
            Some(expanded),
            self.modals.save_attachment.awaiting_overwrite_confirm,
        );
        self.modals.save_attachment.close();
    }

    pub fn queue_attachment_action(&mut self, operation: AttachmentOperation) {
        self.queue_attachment_action_to(operation, None, false);
    }

    /// Queue an attachment Open/Download with an explicit destination
    /// path. Pass `Some(path)` when the user has chosen where to save
    /// (the save-attachment modal flow); pass `None` for the TUI client's
    /// local cache (used by Open).
    pub fn queue_attachment_action_to(
        &mut self,
        operation: AttachmentOperation,
        destination: Option<std::path::PathBuf>,
        replace_existing: bool,
    ) {
        let Some(message_id) = self.mailbox.attachment_panel.message_id.clone() else {
            return;
        };
        let Some(attachment) = self.selected_attachment().cloned() else {
            return;
        };

        self.mailbox.attachment_panel.status = Some(match operation {
            AttachmentOperation::Open => format!("Opening {}...", attachment.filename),
            AttachmentOperation::Download => format!("Downloading {}...", attachment.filename),
        });
        self.mailbox.pending_attachment_action = Some(PendingAttachmentAction {
            message_id,
            attachment_id: attachment.id,
            filename: attachment.filename.clone(),
            operation,
            destination,
            replace_existing,
        });
    }

    pub fn resolve_attachment_file(&mut self, file: &mxr_protocol::AttachmentFile) {
        let path = std::path::PathBuf::from(&file.path);
        for attachment in &mut self.mailbox.attachment_panel.attachments {
            if attachment.id == file.attachment_id {
                attachment.local_path = Some(path.clone());
            }
        }
        for body in self.mailbox.body_cache.values_mut() {
            for attachment in &mut body.attachments {
                if attachment.id == file.attachment_id {
                    attachment.local_path = Some(path.clone());
                }
            }
        }
    }

    pub(super) fn label_chips_for_envelope(&self, envelope: &Envelope) -> Vec<String> {
        envelope
            .label_provider_ids
            .iter()
            .filter_map(|provider_id| {
                self.mailbox
                    .labels
                    .iter()
                    .find(|label| &label.provider_id == provider_id)
                    .map(|label| crate::ui::sidebar::humanize_label(&label.name).to_string())
            })
            .collect()
    }

    pub(super) fn attachment_summaries_for_envelope(
        &self,
        envelope: &Envelope,
    ) -> Vec<AttachmentSummary> {
        self.mailbox
            .body_cache
            .get(&envelope.id)
            .map(|body| {
                body.attachments
                    .iter()
                    .map(|attachment| AttachmentSummary {
                        filename: attachment.filename.clone(),
                        size_bytes: attachment.size_bytes,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn thread_message_blocks(&self) -> Vec<ui::message_view::ThreadMessageBlock> {
        self.mailbox
            .viewed_thread_messages
            .iter()
            .map(|message| ui::message_view::ThreadMessageBlock {
                envelope: message.clone(),
                body_state: self.resolve_body_view_state(message),
                labels: self.label_chips_for_envelope(message),
                attachments: self.attachment_summaries_for_envelope(message),
                selected: self
                    .mailbox
                    .viewing_envelope
                    .as_ref()
                    .map(|env| env.id.clone())
                    == Some(message.id.clone()),
                bulk_selected: self.mailbox.selected_set.contains(&message.id),
                has_unsubscribe: !matches!(message.unsubscribe, UnsubscribeMethod::None),
                signature_expanded: self.mailbox.signature_expanded,
                highlight_link: self
                    .mailbox
                    .todo_link
                    .as_ref()
                    .filter(|(id, _)| *id == message.id)
                    .map(|(_, link)| link.clone()),
                // Phase 3.4: true while a remote-asset fetch is queued
                // or in-flight for this message. Drives the
                // "Loading external assets…" chip in the message
                // header so users see *something* while the network
                // round-trip resolves.
                assets_loading: self.queued_html_image_asset_fetches.contains(&message.id)
                    || self
                        .in_flight_html_image_asset_requests
                        .contains(&message.id),
            })
            .collect()
    }
}

#[cfg(test)]
mod attachment_save_tests {
    use super::*;

    #[test]
    fn save_modal_carries_only_explicit_overwrite_confirmation() {
        let directory = tempfile::tempdir().expect("temporary directory is created");
        let destination = directory.path().join("note.txt");
        let message_id = mxr_core::MessageId::new();
        let attachment_id = mxr_core::AttachmentId::new();
        let mut app = App::new();
        app.mailbox.attachment_panel.message_id = Some(message_id.clone());
        app.mailbox.attachment_panel.attachments = vec![mxr_core::AttachmentMeta {
            id: attachment_id.clone(),
            message_id: message_id.clone(),
            filename: "note.txt".into(),
            mime_type: "text/plain".into(),
            disposition: Default::default(),
            content_id: None,
            content_location: None,
            size_bytes: 3,
            local_path: None,
            provider_id: "fixture".into(),
        }];

        app.modals.save_attachment.open(
            message_id.clone(),
            attachment_id.clone(),
            "note.txt".into(),
            destination.display().to_string(),
        );
        app.save_attachment_confirm();
        assert!(
            !app.mailbox
                .pending_attachment_action
                .as_ref()
                .expect("unconfirmed save is queued")
                .replace_existing
        );

        app.mailbox.pending_attachment_action = None;
        std::fs::write(&destination, b"existing").expect("existing file is created");
        app.modals.save_attachment.open(
            message_id,
            attachment_id,
            "note.txt".into(),
            destination.display().to_string(),
        );
        app.save_attachment_confirm();
        assert!(app.modals.save_attachment.awaiting_overwrite_confirm);
        assert!(app.mailbox.pending_attachment_action.is_none());

        app.save_attachment_confirm();
        assert!(
            app.mailbox
                .pending_attachment_action
                .as_ref()
                .expect("confirmed save is queued")
                .replace_existing
        );
    }
}
