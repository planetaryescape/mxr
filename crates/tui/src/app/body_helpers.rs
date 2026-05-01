use super::*;

impl App {
    pub(super) fn active_body_status(&self) -> Option<String> {
        let BodyViewState::Ready {
            source, metadata, ..
        } = &self.mailbox.body_view_state
        else {
            return None;
        };

        Some(body_status_labels(metadata, source, self.mailbox.show_reader_stats).join(" "))
    }

    pub(crate) fn current_body_mode_status_message(&self) -> Option<String> {
        let BodyViewState::Ready {
            source, metadata, ..
        } = &self.mailbox.body_view_state
        else {
            return None;
        };

        Some(format!("Showing {}", primary_body_label(metadata, source)))
    }

    pub fn status_bar_state(&self) -> ui::status_bar::StatusBarState {
        let starred_count = self.global_starred_count();
        let body_status = self.active_body_status();

        if self.mailbox.mailbox_view == MailboxView::Subscriptions {
            let unread_count = self
                .mailbox
                .subscriptions_page
                .entries
                .iter()
                .filter(|entry| !entry.envelope.flags.contains(MessageFlags::READ))
                .count();
            return ui::status_bar::StatusBarState {
                mailbox_name: "SUBSCRIPTIONS".into(),
                total_count: self.mailbox.subscriptions_page.entries.len(),
                unread_count,
                starred_count,
                body_status: body_status.clone(),
                sync_status: self.last_sync_status.clone(),
                status_message: self.status_message.clone(),
                pending_mutation_count: self.pending_mutation_count,
                pending_mutation_status: self.pending_mutation_status.clone(),
            };
        }

        if self.screen == Screen::Search || self.search.active {
            let results = if self.screen == Screen::Search {
                &self.search.page.results
            } else {
                &self.mailbox.envelopes
            };
            let unread_count = results
                .iter()
                .filter(|envelope| !envelope.flags.contains(MessageFlags::READ))
                .count();
            return ui::status_bar::StatusBarState {
                mailbox_name: "SEARCH".into(),
                total_count: results.len(),
                unread_count,
                starred_count,
                body_status: body_status.clone(),
                sync_status: self.last_sync_status.clone(),
                status_message: self.status_message.clone(),
                pending_mutation_count: self.pending_mutation_count,
                pending_mutation_status: self.pending_mutation_status.clone(),
            };
        }

        if let Some(label) = self.active_label_record() {
            return ui::status_bar::StatusBarState {
                mailbox_name: label.name.clone(),
                total_count: label.total_count as usize,
                unread_count: label.unread_count as usize,
                starred_count,
                body_status: body_status.clone(),
                sync_status: self.last_sync_status.clone(),
                status_message: self.status_message.clone(),
                pending_mutation_count: self.pending_mutation_count,
                pending_mutation_status: self.pending_mutation_status.clone(),
            };
        }

        let unread_count = self
            .mailbox
            .envelopes
            .iter()
            .filter(|envelope| !envelope.flags.contains(MessageFlags::READ))
            .count();
        ui::status_bar::StatusBarState {
            mailbox_name: "ALL MAIL".into(),
            total_count: self
                .diagnostics
                .page
                .total_messages
                .map(|count| count as usize)
                .unwrap_or_else(|| self.mailbox.all_envelopes.len()),
            unread_count,
            starred_count,
            body_status,
            sync_status: self.last_sync_status.clone(),
            status_message: self.status_message.clone(),
            pending_mutation_count: self.pending_mutation_count,
            pending_mutation_status: self.pending_mutation_status.clone(),
        }
    }

    pub(super) fn summarize_sync_status(
        sync_statuses: &[mxr_protocol::AccountSyncStatus],
    ) -> String {
        if sync_statuses.is_empty() {
            return "not synced".into();
        }
        if sync_statuses.iter().any(|sync| sync.sync_in_progress) {
            return "syncing".into();
        }
        if sync_statuses
            .iter()
            .any(|sync| !sync.healthy || sync.last_error.is_some())
        {
            return "degraded".into();
        }
        sync_statuses
            .iter()
            .filter_map(|sync| sync.last_success_at.as_deref())
            .filter_map(Self::format_sync_age)
            .max_by_key(|(_, sort_key)| *sort_key)
            .map(|(display, _)| format!("synced {display}"))
            .unwrap_or_else(|| "not synced".into())
    }

    pub(super) fn format_sync_age(timestamp: &str) -> Option<(String, i64)> {
        let parsed = chrono::DateTime::parse_from_rfc3339(timestamp).ok()?;
        let synced_at = parsed.with_timezone(&chrono::Utc);
        let elapsed = chrono::Utc::now().signed_duration_since(synced_at);
        let seconds = elapsed.num_seconds().max(0);
        let display = if seconds < 60 {
            "just now".to_string()
        } else if seconds < 3_600 {
            format!("{}m ago", seconds / 60)
        } else if seconds < 86_400 {
            format!("{}h ago", seconds / 3_600)
        } else {
            format!("{}d ago", seconds / 86_400)
        };
        Some((display, synced_at.timestamp()))
    }

    pub(super) fn envelope_preview(envelope: &Envelope) -> Option<String> {
        let snippet = envelope.snippet.trim();
        if snippet.is_empty() {
            None
        } else {
            Some(envelope.snippet.clone())
        }
    }

    pub(super) fn reader_config(&self) -> mxr_reader::ReaderConfig {
        mxr_reader::ReaderConfig {
            html_command: self.mailbox.render_html_command.clone(),
            ..Default::default()
        }
    }

    pub(super) fn render_body(
        &self,
        raw: &str,
        source: BodySource,
    ) -> (String, Option<(usize, usize)>) {
        if !self.mailbox.reader_mode || source == BodySource::Snippet {
            return (raw.to_string(), None);
        }

        let output = match source {
            BodySource::Plain => mxr_reader::clean(Some(raw), None, &self.reader_config()),
            BodySource::Html => mxr_reader::clean(None, Some(raw), &self.reader_config()),
            BodySource::Fallback => mxr_reader::clean(Some(raw), None, &self.reader_config()),
            BodySource::Snippet => unreachable!("snippet bodies bypass reader mode"),
        };

        (
            output.content,
            Some((output.original_lines, output.cleaned_lines)),
        )
    }

    pub(super) fn body_inline_images(body: &MessageBody) -> bool {
        body.attachments.iter().any(Self::attachment_is_inlineish)
    }

    pub(super) fn attachment_is_inlineish(attachment: &AttachmentMeta) -> bool {
        attachment.disposition == AttachmentDisposition::Inline
            || attachment.content_id.is_some()
            || attachment.content_location.is_some()
    }

    pub(super) fn sorted_attachment_panel_attachments(body: &MessageBody) -> Vec<AttachmentMeta> {
        let mut attachments = body.attachments.clone();
        attachments.sort_by_key(Self::attachment_is_inlineish);
        attachments
    }

    pub(super) fn html_has_remote_content(html: &str) -> bool {
        static REMOTE_IMAGE_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
        REMOTE_IMAGE_RE
            .get_or_init(|| {
                regex::Regex::new(r#"(?is)<img\b[^>]*\bsrc\s*=\s*["']https?://[^"']+["']"#)
                    .expect("valid remote image regex")
            })
            .is_match(html)
    }

    pub(super) fn body_view_metadata(
        &self,
        body: &MessageBody,
        source: BodySource,
        mode: BodyViewMode,
        reader_applied: bool,
        stats: Option<(usize, usize)>,
    ) -> BodyViewMetadata {
        BodyViewMetadata {
            mode,
            provenance: match source {
                BodySource::Plain => body.metadata.text_plain_source,
                BodySource::Html => body.metadata.text_html_source,
                BodySource::Fallback | BodySource::Snippet => None,
            },
            reader_applied,
            flowed: matches!(
                body.metadata.text_plain_format,
                Some(TextPlainFormat::Flowed { .. })
            ),
            inline_images: Self::body_inline_images(body),
            remote_content_available: body
                .text_html
                .as_deref()
                .is_some_and(Self::html_has_remote_content),
            remote_content_enabled: self.mailbox.remote_content_enabled,
            original_lines: stats.map(|(original, _)| original),
            cleaned_lines: stats.map(|(_, cleaned)| cleaned),
        }
    }

    pub(super) fn resolve_body_view_state(&self, envelope: &Envelope) -> BodyViewState {
        let preview = Self::envelope_preview(envelope);

        if let Some(body) = self.mailbox.body_cache.get(&envelope.id) {
            if self.mailbox.html_view {
                if let Some(raw) = body.text_html.clone() {
                    let metadata = self.body_view_metadata(
                        body,
                        BodySource::Html,
                        BodyViewMode::Html,
                        false,
                        None,
                    );
                    return BodyViewState::Ready {
                        rendered: raw.clone(),
                        raw,
                        source: BodySource::Html,
                        metadata,
                    };
                }

                if let Some(raw) = body.text_plain.clone() {
                    let metadata = self.body_view_metadata(
                        body,
                        BodySource::Plain,
                        BodyViewMode::Html,
                        false,
                        None,
                    );
                    return BodyViewState::Ready {
                        rendered: raw.clone(),
                        raw,
                        source: BodySource::Plain,
                        metadata,
                    };
                }
            }

            if let Some(raw) = body.text_plain.clone() {
                let (rendered, stats) = self.render_body(&raw, BodySource::Plain);
                let metadata = self.body_view_metadata(
                    body,
                    BodySource::Plain,
                    BodyViewMode::Text,
                    self.mailbox.reader_mode,
                    stats,
                );
                return BodyViewState::Ready {
                    raw,
                    rendered,
                    source: BodySource::Plain,
                    metadata,
                };
            }

            if let Some(raw) = body.text_html.clone() {
                let (rendered, stats) = self.render_body(&raw, BodySource::Html);
                let metadata = self.body_view_metadata(
                    body,
                    BodySource::Html,
                    BodyViewMode::Text,
                    self.mailbox.reader_mode,
                    stats,
                );
                return BodyViewState::Ready {
                    raw,
                    rendered,
                    source: BodySource::Html,
                    metadata,
                };
            }

            if let Some(raw) = body.best_effort_readable_summary() {
                let (rendered, stats) = self.render_body(&raw, BodySource::Fallback);
                let metadata = self.body_view_metadata(
                    body,
                    BodySource::Fallback,
                    BodyViewMode::Text,
                    self.mailbox.reader_mode,
                    stats,
                );
                return BodyViewState::Ready {
                    raw,
                    rendered,
                    source: BodySource::Fallback,
                    metadata,
                };
            }

            return BodyViewState::Empty { preview };
        }

        if self.mailbox.in_flight_body_requests.contains(&envelope.id) {
            BodyViewState::Loading { preview }
        } else {
            BodyViewState::Empty { preview }
        }
    }

    pub fn resolve_body_success(&mut self, body: MessageBody) {
        let message_id = body.message_id.clone();
        self.mailbox.in_flight_body_requests.remove(&message_id);
        self.mailbox.body_cache.insert(message_id.clone(), body);
        self.queue_html_assets_for_message(&message_id);

        if self.mailbox.pending_browser_open_after_load.as_ref() == Some(&message_id) {
            self.mailbox.pending_browser_open_after_load = None;
            if let Some(body) = self.mailbox.body_cache.get(&message_id).cloned() {
                self.queue_browser_open_for_body(message_id.clone(), &body);
            }
        }

        if self
            .mailbox
            .viewing_envelope
            .as_ref()
            .map(|env| env.id.clone())
            == Some(message_id)
        {
            self.ensure_current_body_state();
        }
    }

    pub fn resolve_body_fetch_error(&mut self, message_id: &MessageId, message: String) {
        self.mailbox.in_flight_body_requests.remove(message_id);
        if self.mailbox.pending_browser_open_after_load.as_ref() == Some(message_id) {
            self.mailbox.pending_browser_open_after_load = None;
        }

        if let Some(env) = self
            .mailbox
            .viewing_envelope
            .as_ref()
            .filter(|env| &env.id == message_id)
        {
            self.mailbox.body_view_state = BodyViewState::Error {
                message,
                preview: Self::envelope_preview(env),
            };
        }
    }

    pub fn queue_html_assets_for_current_view(&mut self) {
        if !self.mailbox.html_view {
            return;
        }

        let message_ids = self
            .mailbox
            .viewed_thread_messages
            .iter()
            .map(|message| message.id.clone())
            .collect::<Vec<_>>();
        for message_id in message_ids {
            self.queue_html_assets_for_message(&message_id);
        }
    }

    pub fn queue_html_assets_for_message(&mut self, message_id: &MessageId) {
        if !self.mailbox.html_view {
            return;
        }
        let Some(body) = self.mailbox.body_cache.get(message_id) else {
            return;
        };
        if body.text_html.is_none() {
            return;
        }
        if self
            .in_flight_html_image_asset_requests
            .contains(message_id)
            || self
                .queued_html_image_asset_fetches
                .iter()
                .any(|queued| queued == message_id)
        {
            return;
        }
        self.queued_html_image_asset_fetches
            .push(message_id.clone());
    }

    pub fn invalidate_html_assets_for_current_view(&mut self) {
        let message_ids = self
            .mailbox
            .viewed_thread_messages
            .iter()
            .map(|message| message.id.clone())
            .collect::<Vec<_>>();
        self.invalidate_html_assets_for_messages(&message_ids);
    }

    pub fn invalidate_html_assets_for_messages(&mut self, message_ids: &[MessageId]) {
        for message_id in message_ids {
            self.html_image_assets.remove(message_id);
            self.in_flight_html_image_asset_requests.remove(message_id);
            self.queued_html_image_asset_fetches
                .retain(|queued| queued != message_id);
            self.queued_html_image_decodes
                .retain(|queued| &queued.message_id != message_id);
        }
    }

    pub fn resolve_html_image_assets_success(
        &mut self,
        message_id: MessageId,
        assets: Vec<HtmlImageAsset>,
        allow_remote: bool,
    ) {
        self.in_flight_html_image_asset_requests.remove(&message_id);
        let mut entries = HashMap::new();
        for asset in assets {
            let source = asset.source.clone();
            let should_decode = asset.status == HtmlImageAssetStatus::Ready
                && asset.path.is_some()
                && !self
                    .queued_html_image_decodes
                    .iter()
                    .any(|queued| queued.message_id == message_id && queued.source == source);
            if should_decode {
                self.queued_html_image_decodes.push(HtmlImageKey {
                    message_id: message_id.clone(),
                    source: source.clone(),
                });
            }
            entries.insert(source, HtmlImageEntry::new(asset));
        }
        self.html_image_assets.insert(message_id.clone(), entries);

        if self.mailbox.remote_content_enabled != allow_remote {
            return;
        }
        if self
            .mailbox
            .viewing_envelope
            .as_ref()
            .map(|env| env.id.clone())
            == Some(message_id)
        {
            self.ensure_current_body_state();
        }
    }

    pub fn resolve_html_image_assets_error(&mut self, message_id: &MessageId, message: String) {
        self.in_flight_html_image_asset_requests.remove(message_id);
        let mut entries = HashMap::new();
        entries.insert(
            "__error__".into(),
            HtmlImageEntry {
                asset: HtmlImageAsset {
                    source: "__error__".into(),
                    kind: HtmlImageSourceKind::File,
                    status: HtmlImageAssetStatus::Failed,
                    mime_type: None,
                    path: None,
                    detail: Some(message),
                },
                render: crate::terminal_images::HtmlImageRenderState::Failed(
                    "asset resolution failed".into(),
                ),
            },
        );
        self.html_image_assets.insert(message_id.clone(), entries);
    }

    pub fn resolve_html_image_protocol(
        &mut self,
        key: &HtmlImageKey,
        protocol: ratatui_image::thread::ThreadProtocol,
    ) {
        if let Some(entry) = self
            .html_image_assets
            .get_mut(&key.message_id)
            .and_then(|assets| assets.get_mut(&key.source))
        {
            entry.render = crate::terminal_images::HtmlImageRenderState::Ready(Box::new(protocol));
        }
    }

    pub fn resolve_html_image_resize(
        &mut self,
        key: &HtmlImageKey,
        response: ratatui_image::thread::ResizeResponse,
    ) {
        if let Some(protocol) = self
            .html_image_assets
            .get_mut(&key.message_id)
            .and_then(|assets| assets.get_mut(&key.source))
            .and_then(HtmlImageEntry::ready_protocol_mut)
        {
            protocol.update_resized_protocol(response);
        }
    }

    pub fn resolve_html_image_failure(&mut self, key: &HtmlImageKey, message: String) {
        if let Some(entry) = self
            .html_image_assets
            .get_mut(&key.message_id)
            .and_then(|assets| assets.get_mut(&key.source))
        {
            entry.render = crate::terminal_images::HtmlImageRenderState::Failed(message);
        }
    }

    pub fn current_viewing_body(&self) -> Option<&MessageBody> {
        self.mailbox
            .viewing_envelope
            .as_ref()
            .and_then(|env| self.mailbox.body_cache.get(&env.id))
    }
}
