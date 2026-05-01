use super::*;

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

    pub fn queue_attachment_action(&mut self, operation: AttachmentOperation) {
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
            operation,
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
            })
            .collect()
    }
}
