use super::*;

impl App {
    pub(super) fn label_chips_for_envelope(&self, envelope: &Envelope) -> Vec<String> {
        envelope
            .label_provider_ids
            .iter()
            .filter_map(|provider_id| {
                self.labels
                    .iter()
                    .find(|label| &label.provider_id == provider_id)
                    .map(|label| {
                        crate::ui::sidebar::humanize_label(&label.name).to_string()
                    })
            })
            .collect()
    }

    pub(super) fn attachment_summaries_for_envelope(
        &self,
        envelope: &Envelope,
    ) -> Vec<AttachmentSummary> {
        self.body_cache
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
        self.viewed_thread_messages
            .iter()
            .map(|message| ui::message_view::ThreadMessageBlock {
                envelope: message.clone(),
                body_state: self.resolve_body_view_state(message),
                labels: self.label_chips_for_envelope(message),
                attachments: self.attachment_summaries_for_envelope(message),
                selected: self.viewing_envelope.as_ref().map(|env| env.id.clone())
                    == Some(message.id.clone()),
                bulk_selected: self.selected_set.contains(&message.id),
                has_unsubscribe: !matches!(message.unsubscribe, UnsubscribeMethod::None),
                signature_expanded: self.signature_expanded,
            })
            .collect()
    }

    pub fn apply_local_label_refs(
        &mut self,
        message_ids: &[MessageId],
        add: &[String],
        remove: &[String],
    ) {
        let add_provider_ids = self.resolve_label_provider_ids(add);
        let remove_provider_ids = self.resolve_label_provider_ids(remove);
        for envelope in self
            .envelopes
            .iter_mut()
            .chain(self.all_envelopes.iter_mut())
            .chain(self.search_page.results.iter_mut())
            .chain(self.viewed_thread_messages.iter_mut())
        {
            if message_ids
                .iter()
                .any(|message_id| message_id == &envelope.id)
            {
                apply_provider_label_changes(
                    &mut envelope.label_provider_ids,
                    &add_provider_ids,
                    &remove_provider_ids,
                );
            }
        }
        if let Some(ref mut envelope) = self.viewing_envelope {
            if message_ids
                .iter()
                .any(|message_id| message_id == &envelope.id)
            {
                apply_provider_label_changes(
                    &mut envelope.label_provider_ids,
                    &add_provider_ids,
                    &remove_provider_ids,
                );
            }
        }
    }

    pub fn apply_local_flags(&mut self, message_id: &MessageId, flags: MessageFlags) {
        for envelope in self
            .envelopes
            .iter_mut()
            .chain(self.all_envelopes.iter_mut())
            .chain(self.search_page.results.iter_mut())
            .chain(self.viewed_thread_messages.iter_mut())
        {
            if &envelope.id == message_id {
                envelope.flags = flags;
            }
        }
        if let Some(envelope) = self.viewing_envelope.as_mut() {
            if &envelope.id == message_id {
                envelope.flags = flags;
            }
        }
    }

    pub fn apply_local_flags_many(&mut self, updates: &[(MessageId, MessageFlags)]) {
        for (message_id, flags) in updates {
            self.apply_local_flags(message_id, *flags);
        }
    }

    pub(super) fn apply_local_mutation_effect(&mut self, effect: &MutationEffect) {
        match effect {
            MutationEffect::RemoveFromList(message_id) => {
                self.apply_removed_message_ids(std::slice::from_ref(message_id));
            }
            MutationEffect::RemoveFromListMany(message_ids) => {
                self.apply_removed_message_ids(message_ids);
            }
            MutationEffect::UpdateFlags { message_id, flags } => {
                self.apply_local_flags(message_id, *flags);
            }
            MutationEffect::UpdateFlagsMany { updates } => {
                self.apply_local_flags_many(updates);
            }
            MutationEffect::ModifyLabels {
                message_ids,
                add,
                remove,
                ..
            } => {
                self.apply_local_label_refs(message_ids, add, remove);
            }
            MutationEffect::RefreshList | MutationEffect::StatusOnly(_) => {}
        }
    }

    pub(super) fn queue_mutation(
        &mut self,
        request: Request,
        effect: MutationEffect,
        status_message: String,
    ) {
        self.pending_mutation_queue.push((request, effect));
        self.pending_mutation_count += 1;
        self.pending_mutation_status = Some(status_message.clone());
        self.status_message = Some(status_message);
    }

    pub fn finish_pending_mutation(&mut self) {
        self.pending_mutation_count = self.pending_mutation_count.saturating_sub(1);
        if self.pending_mutation_count == 0 {
            self.pending_mutation_status = None;
        }
    }

    pub(super) fn show_error_modal(&mut self, title: impl Into<String>, detail: impl Into<String>) {
        self.error_modal = Some(ErrorModalState::new(title, detail));
    }

    pub(super) fn open_account_result_details_modal(
        &mut self,
        result: &mxr_protocol::AccountOperationResult,
    ) {
        self.show_error_modal(
            account_result_modal_title(result),
            account_result_modal_detail(result),
        );
    }

    pub fn show_mutation_failure(&mut self, error: &MxrError) {
        self.show_error_modal(
            "Mutation Failed",
            format!(
                "Optimistic changes could not be applied.\nMailbox is refreshing to reconcile state.\n\n{error}"
            ),
        );
        self.status_message = Some(format!("Error: {error}"));
    }

    pub fn refresh_mailbox_after_mutation_failure(&mut self) {
        self.pending_labels_refresh = true;
        self.pending_all_envelopes_refresh = true;
        self.pending_status_refresh = true;
        self.pending_subscriptions_refresh = true;
        if let Some(label_id) = self
            .pending_active_label
            .clone()
            .or_else(|| self.active_label.clone())
        {
            self.pending_label_fetch = Some(label_id);
        }
    }

    pub(super) fn message_flags(&self, message_id: &MessageId) -> Option<MessageFlags> {
        self.envelopes
            .iter()
            .chain(self.all_envelopes.iter())
            .chain(self.search_page.results.iter())
            .chain(self.viewed_thread_messages.iter())
            .find(|envelope| &envelope.id == message_id)
            .map(|envelope| envelope.flags)
            .or_else(|| {
                self.viewing_envelope
                    .as_ref()
                    .filter(|envelope| &envelope.id == message_id)
                    .map(|envelope| envelope.flags)
            })
    }

    pub(super) fn flag_updates_for_ids<F>(
        &self,
        message_ids: &[MessageId],
        mut update: F,
    ) -> Vec<(MessageId, MessageFlags)>
    where
        F: FnMut(MessageFlags) -> MessageFlags,
    {
        message_ids
            .iter()
            .filter_map(|message_id| {
                self.message_flags(message_id)
                    .map(|flags| (message_id.clone(), update(flags)))
            })
            .collect()
    }

    pub(super) fn resolve_label_provider_ids(&self, refs: &[String]) -> Vec<String> {
        refs.iter()
            .filter_map(|label_ref| {
                self.labels
                    .iter()
                    .find(|label| label.provider_id == *label_ref || label.name == *label_ref)
                    .map(|label| label.provider_id.clone())
                    .or_else(|| Some(label_ref.clone()))
            })
            .collect()
    }

    pub fn resolve_thread_success(&mut self, thread: Thread, mut messages: Vec<Envelope>) {
        let thread_id = thread.id.clone();
        self.in_flight_thread_fetch = None;
        messages.sort_by_key(|message| message.date);

        if self
            .viewing_envelope
            .as_ref()
            .map(|env| env.thread_id.clone())
            == Some(thread_id)
        {
            let focused_message_id = self.focused_thread_envelope().map(|env| env.id.clone());
            for message in &messages {
                self.queue_body_fetch(message.id.clone());
            }
            self.viewed_thread = Some(thread);
            self.viewed_thread_messages = messages;
            self.thread_selected_index = focused_message_id
                .and_then(|message_id| {
                    self.viewed_thread_messages
                        .iter()
                        .position(|message| message.id == message_id)
                })
                .unwrap_or_else(|| self.default_thread_selected_index());
            self.sync_focused_thread_envelope();
        }
    }

    pub fn resolve_thread_fetch_error(&mut self, thread_id: &mxr_core::ThreadId) {
        if self.in_flight_thread_fetch.as_ref() == Some(thread_id) {
            self.in_flight_thread_fetch = None;
        }
    }

    /// Get IDs to mutate: selected_set if non-empty, else context_envelope.
    pub(super) fn mutation_target_ids(&self) -> Vec<MessageId> {
        if !self.selected_set.is_empty() {
            self.selected_set.iter().cloned().collect()
        } else if let Some(env) = self.context_envelope() {
            vec![env.id.clone()]
        } else {
            vec![]
        }
    }

    pub(super) fn clear_selection(&mut self) {
        self.selected_set.clear();
        self.visual_mode = false;
        self.visual_anchor = None;
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "bulk confirmation inputs stay explicit for safety"
    )]
    pub(super) fn queue_or_confirm_bulk_action(
        &mut self,
        title: impl Into<String>,
        detail: impl Into<String>,
        request: Request,
        effect: MutationEffect,
        optimistic_effect: Option<MutationEffect>,
        status_message: String,
        count: usize,
    ) {
        if count > 1 {
            self.pending_bulk_confirm = Some(PendingBulkConfirm {
                title: title.into(),
                detail: detail.into(),
                request,
                effect,
                optimistic_effect,
                status_message,
            });
        } else {
            if let Some(effect) = optimistic_effect.as_ref() {
                self.apply_local_mutation_effect(effect);
            }
            self.queue_mutation(request, effect, status_message);
            self.clear_selection();
        }
    }

    /// Update visual selection range when moving in visual mode.
    pub(super) fn update_visual_selection(&mut self) {
        if self.visual_mode {
            if let Some(anchor) = self.visual_anchor {
                let (cursor, source) = if self.screen == Screen::Search {
                    (self.search_page.selected_index, &self.search_page.results)
                } else {
                    (self.selected_index, &self.envelopes)
                };
                let start = anchor.min(cursor);
                let end = anchor.max(cursor);
                self.selected_set.clear();
                for env in source.iter().skip(start).take(end - start + 1) {
                    self.selected_set.insert(env.id.clone());
                }
            }
        }
    }

    /// Ensure selected_index is visible within the scroll viewport.
    pub(super) fn ensure_visible(&mut self) {
        let h = self.visible_height.max(1);
        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        } else if self.selected_index >= self.scroll_offset + h {
            self.scroll_offset = self.selected_index + 1 - h;
        }
        // Prefetch bodies for messages near the cursor
        self.queue_body_window();
    }

    pub fn set_subscriptions(&mut self, subscriptions: Vec<SubscriptionSummary>) {
        let selected_id = self
            .selected_subscription_entry()
            .map(|entry| entry.summary.latest_message_id.clone());
        self.subscriptions_page.entries = subscriptions
            .into_iter()
            .map(|summary| SubscriptionEntry {
                envelope: subscription_summary_to_envelope(&summary),
                summary,
            })
            .collect();

        if self.subscriptions_page.entries.is_empty() {
            if self.mailbox_view == MailboxView::Subscriptions {
                self.selected_index = 0;
                self.scroll_offset = 0;
                self.viewing_envelope = None;
                self.viewed_thread = None;
                self.viewed_thread_messages.clear();
                self.body_view_state = BodyViewState::Empty { preview: None };
            }
            return;
        }

        if let Some(selected_id) = selected_id {
            if let Some(position) = self
                .subscriptions_page
                .entries
                .iter()
                .position(|entry| entry.summary.latest_message_id == selected_id)
            {
                self.selected_index = position;
            } else {
                self.selected_index = self
                    .selected_index
                    .min(self.subscriptions_page.entries.len().saturating_sub(1));
            }
        } else {
            self.selected_index = self
                .selected_index
                .min(self.subscriptions_page.entries.len().saturating_sub(1));
        }

        if self.mailbox_view == MailboxView::Subscriptions {
            self.ensure_visible();
            self.auto_preview();
        }
    }
}

pub(super) fn apply_provider_label_changes(
    label_provider_ids: &mut Vec<String>,
    add_provider_ids: &[String],
    remove_provider_ids: &[String],
) {
    label_provider_ids.retain(|provider_id| {
        !remove_provider_ids
            .iter()
            .any(|remove| remove == provider_id)
    });
    for provider_id in add_provider_ids {
        if !label_provider_ids
            .iter()
            .any(|existing| existing == provider_id)
        {
            label_provider_ids.push(provider_id.clone());
        }
    }
}

pub(super) fn remove_from_list_effect(ids: &[MessageId]) -> MutationEffect {
    if ids.len() == 1 {
        MutationEffect::RemoveFromList(ids[0].clone())
    } else {
        MutationEffect::RemoveFromListMany(ids.to_vec())
    }
}

pub(super) fn pluralize_messages(count: usize) -> &'static str {
    if count == 1 {
        "message"
    } else {
        "messages"
    }
}

pub(super) fn bulk_message_detail(verb: &str, count: usize) -> String {
    format!(
        "You are about to {verb} these {count} {}.",
        pluralize_messages(count)
    )
}

pub(super) fn subscription_summary_to_envelope(summary: &SubscriptionSummary) -> Envelope {
    Envelope {
        id: summary.latest_message_id.clone(),
        account_id: summary.account_id.clone(),
        provider_id: summary.latest_provider_id.clone(),
        thread_id: summary.latest_thread_id.clone(),
        message_id_header: None,
        in_reply_to: None,
        references: vec![],
        from: Address {
            name: summary.sender_name.clone(),
            email: summary.sender_email.clone(),
        },
        to: vec![],
        cc: vec![],
        bcc: vec![],
        subject: summary.latest_subject.clone(),
        date: summary.latest_date,
        flags: summary.latest_flags,
        snippet: summary.latest_snippet.clone(),
        has_attachments: summary.latest_has_attachments,
        size_bytes: summary.latest_size_bytes,
        unsubscribe: summary.unsubscribe.clone(),
        label_provider_ids: vec![],
    }
}

pub(super) fn unsubscribe_method_label(method: &UnsubscribeMethod) -> &'static str {
    match method {
        UnsubscribeMethod::OneClick { .. } => "one-click",
        UnsubscribeMethod::Mailto { .. } => "mailto",
        UnsubscribeMethod::HttpLink { .. } => "browser link",
        UnsubscribeMethod::BodyLink { .. } => "body link",
        UnsubscribeMethod::None => "none",
    }
}
