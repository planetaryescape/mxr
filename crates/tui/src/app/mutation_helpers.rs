use super::*;

impl App {
    pub fn apply_local_label_refs(
        &mut self,
        message_ids: &[MessageId],
        add: &[String],
        remove: &[String],
    ) {
        let add_provider_ids = self.resolve_label_provider_ids(add);
        let remove_provider_ids = self.resolve_label_provider_ids(remove);
        for envelopes in [
            &mut self.mailbox.envelopes,
            &mut self.mailbox.all_envelopes,
            &mut self.search.page.results,
            &mut self.mailbox.viewed_thread_messages,
        ] {
            for envelope in envelopes {
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
        if let Some(ref mut envelope) = self.mailbox.viewing_envelope {
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
        for envelopes in [
            &mut self.mailbox.envelopes,
            &mut self.mailbox.all_envelopes,
            &mut self.search.page.results,
            &mut self.mailbox.viewed_thread_messages,
        ] {
            for envelope in envelopes {
                if &envelope.id == message_id {
                    envelope.flags = flags;
                }
            }
        }
        if let Some(envelope) = self.mailbox.viewing_envelope.as_mut() {
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
        self.modals.error = Some(ErrorModalState::new(title, detail));
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
        self.mailbox.pending_labels_refresh = true;
        self.mailbox.pending_all_envelopes_refresh = true;
        self.diagnostics.pending_status_refresh = true;
        self.mailbox.pending_subscriptions_refresh = true;
        if let Some(label_id) = self
            .mailbox
            .pending_active_label
            .clone()
            .or_else(|| self.mailbox.active_label.clone())
        {
            self.mailbox.pending_label_fetch = Some(label_id);
        }
    }

    pub(crate) fn apply_removed_message_ids(&mut self, ids: &[MessageId]) {
        if ids.is_empty() {
            return;
        }

        let viewing_removed = self
            .mailbox
            .viewing_envelope
            .as_ref()
            .is_some_and(|envelope| ids.iter().any(|id| id == &envelope.id));
        let reader_was_open = self.mailbox.layout_mode == LayoutMode::ThreePane
            && self.mailbox.viewing_envelope.is_some();

        self.mailbox
            .envelopes
            .retain(|envelope| !ids.iter().any(|id| id == &envelope.id));
        self.mailbox
            .all_envelopes
            .retain(|envelope| !ids.iter().any(|id| id == &envelope.id));
        self.search
            .page
            .results
            .retain(|envelope| !ids.iter().any(|id| id == &envelope.id));
        self.mailbox
            .viewed_thread_messages
            .retain(|envelope| !ids.iter().any(|id| id == &envelope.id));
        self.mailbox
            .selected_set
            .retain(|message_id| !ids.iter().any(|id| id == message_id));

        self.mailbox.selected_index = self
            .mailbox
            .selected_index
            .min(self.mail_row_count().saturating_sub(1));
        self.search.page.selected_index = self
            .search
            .page
            .selected_index
            .min(self.search_row_count().saturating_sub(1));

        if viewing_removed {
            self.clear_message_view_state();

            if reader_was_open {
                match self.screen {
                    Screen::Search if self.search_row_count() > 0 => {
                        self.ensure_search_visible();
                        self.auto_preview_search();
                    }
                    Screen::Mailbox if self.mail_row_count() > 0 => {
                        self.ensure_visible();
                        self.auto_preview();
                    }
                    _ => {}
                }
            }

            if self.mailbox.viewing_envelope.is_none()
                && self.mailbox.layout_mode == LayoutMode::ThreePane
            {
                self.mailbox.layout_mode = LayoutMode::TwoPane;
                if self.mailbox.active_pane == ActivePane::MessageView {
                    self.mailbox.active_pane = ActivePane::MailList;
                }
            }
        } else {
            if self.screen == Screen::Mailbox && self.mail_row_count() > 0 {
                self.ensure_visible();
            } else if self.screen == Screen::Search && self.search_row_count() > 0 {
                self.ensure_search_visible();
            }
        }
    }

    pub(super) fn message_flags(&self, message_id: &MessageId) -> Option<MessageFlags> {
        self.mailbox
            .envelopes
            .iter()
            .chain(self.mailbox.all_envelopes.iter())
            .chain(self.search.page.results.iter())
            .chain(self.mailbox.viewed_thread_messages.iter())
            .find(|envelope| &envelope.id == message_id)
            .map(|envelope| envelope.flags)
            .or_else(|| {
                self.mailbox
                    .viewing_envelope
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
                self.mailbox
                    .labels
                    .iter()
                    .find(|label| label.provider_id == *label_ref || label.name == *label_ref)
                    .map(|label| label.provider_id.clone())
                    .or_else(|| Some(label_ref.clone()))
            })
            .collect()
    }

    pub(super) fn mutation_target_ids(&self) -> Vec<MessageId> {
        if !self.mailbox.selected_set.is_empty() {
            self.mailbox.selected_set.iter().cloned().collect()
        } else if let Some(env) = self.context_envelope() {
            vec![env.id.clone()]
        } else {
            vec![]
        }
    }

    pub(super) fn clear_selection(&mut self) {
        self.mailbox.selected_set.clear();
        self.mailbox.visual_mode = false;
        self.mailbox.visual_anchor = None;
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
            self.modals.pending_bulk_confirm = Some(PendingBulkConfirm {
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

    pub(super) fn update_visual_selection(&mut self) {
        if self.mailbox.visual_mode {
            if let Some(anchor) = self.mailbox.visual_anchor {
                let (cursor, source) = if self.screen == Screen::Search {
                    (self.search.page.selected_index, &self.search.page.results)
                } else {
                    (self.mailbox.selected_index, &self.mailbox.envelopes)
                };
                let start = anchor.min(cursor);
                let end = anchor.max(cursor);
                self.mailbox.selected_set.clear();
                for env in source.iter().skip(start).take(end - start + 1) {
                    self.mailbox.selected_set.insert(env.id.clone());
                }
            }
        }
    }
}
