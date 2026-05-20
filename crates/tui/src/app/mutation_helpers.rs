use super::*;

impl App {
    /// Resolve the user's selection in the snooze panel into a wake-at
    /// instant and queue the snooze mutation. Handles both the preset
    /// rows and the trailing `Custom...` text-entry row. Custom-mode
    /// parser errors stay in the modal so the user can fix them
    /// without losing context.
    pub(super) fn handle_snooze_panel_confirm(&mut self) {
        // Custom-time text-entry path: parse the buffer and either snooze
        // or surface the error in-modal.
        if let Some(buffer) = self.modals.snooze_panel.custom_input.clone() {
            let trimmed = buffer.trim();
            if trimmed.is_empty() {
                self.modals.snooze_panel.custom_error =
                    Some("Type a time, e.g. `tomorrow 9am` or `in 2h`".into());
                return;
            }
            match mxr_core::time_parse::parse_relative_time(trimmed, chrono::Utc::now()) {
                Ok(wake_at) => {
                    if let Some(env) = self.context_envelope() {
                        let id = env.id.clone();
                        self.queue_mutation(
                            Request::Snooze {
                                message_id: id,
                                wake_at,
                            },
                            MutationEffect::StatusOnly(format!(
                                "Snoozed until {}",
                                wake_at
                                    .with_timezone(&chrono::Local)
                                    .format("%a %b %e %H:%M")
                            )),
                            "Snoozing...".into(),
                        );
                    }
                    self.modals.snooze_panel.visible = false;
                    self.modals.snooze_panel.custom_input = None;
                    self.modals.snooze_panel.custom_error = None;
                }
                Err(e) => {
                    self.modals.snooze_panel.custom_error = Some(e.to_string());
                }
            }
            return;
        }

        // Preset list path. The trailing index is the "Custom..." entry
        // — confirming it switches the panel into text-entry mode rather
        // than queueing a snooze.
        let selected = self.modals.snooze_panel.selected_index;
        let presets = snooze_presets();
        if selected >= presets.len() {
            self.modals.snooze_panel.custom_input = Some(String::new());
            self.modals.snooze_panel.custom_error = None;
            return;
        }
        if let Some(env) = self.context_envelope() {
            let wake_at = resolve_snooze_preset(presets[selected], &self.modals.snooze_config);
            let id = env.id.clone();
            self.queue_mutation(
                Request::Snooze {
                    message_id: id,
                    wake_at,
                },
                MutationEffect::StatusOnly(format!(
                    "Snoozed until {}",
                    wake_at
                        .with_timezone(&chrono::Local)
                        .format("%a %b %e %H:%M")
                )),
                "Snoozing...".into(),
            );
        }
        self.modals.snooze_panel.visible = false;
    }

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
            MutationEffect::ReplyLater {
                message_id, flag, ..
            } => {
                if *flag {
                    self.mailbox
                        .reply_later_message_ids
                        .insert(message_id.clone());
                } else {
                    self.mailbox.reply_later_message_ids.remove(message_id);
                }
            }
            MutationEffect::RefreshList
            | MutationEffect::StatusOnly(_)
            | MutationEffect::SentSuccess { .. } => {}
        }
    }

    /// Apply the completion of a mutation to UI state.
    ///
    /// Called from the main event loop after the daemon's `MutationResult`
    /// arrives. `show_completion_status` is true only when this is the last
    /// in-flight mutation — matches the existing main-loop gating semantics
    /// so a batch of archives doesn't overwrite the status mid-flight.
    pub fn apply_mutation_completion(
        &mut self,
        effect: MutationEffect,
        show_completion_status: bool,
    ) {
        match effect {
            MutationEffect::RemoveFromList(id) => {
                self.apply_removed_message_ids(std::slice::from_ref(&id));
                if show_completion_status {
                    self.status_message = Some("Done".into());
                }
                self.mailbox.pending_subscriptions_refresh = true;
            }
            MutationEffect::RemoveFromListMany(ids) => {
                self.apply_removed_message_ids(&ids);
                if show_completion_status {
                    self.status_message = Some("Done".into());
                }
                self.mailbox.pending_subscriptions_refresh = true;
            }
            MutationEffect::UpdateFlags { message_id, flags } => {
                self.apply_local_flags(&message_id, flags);
                if show_completion_status {
                    self.status_message = Some("Done".into());
                }
            }
            MutationEffect::UpdateFlagsMany { updates } => {
                self.apply_local_flags_many(&updates);
                if show_completion_status {
                    self.status_message = Some("Done".into());
                }
            }
            MutationEffect::RefreshList => {
                if let Some(label_id) = self.mailbox.active_label.clone() {
                    self.mailbox.pending_label_fetch = Some(label_id);
                }
                self.mailbox.pending_subscriptions_refresh = true;
                if show_completion_status {
                    self.status_message = Some("Synced".into());
                }
            }
            MutationEffect::ModifyLabels {
                message_ids,
                add,
                remove,
                status,
            } => {
                self.apply_local_label_refs(&message_ids, &add, &remove);
                if show_completion_status {
                    self.status_message = Some(status);
                }
            }
            MutationEffect::ReplyLater {
                message_id,
                flag,
                status,
            } => {
                if flag {
                    self.mailbox.reply_later_message_ids.insert(message_id);
                } else {
                    self.mailbox.reply_later_message_ids.remove(&message_id);
                }
                if show_completion_status {
                    self.status_message = Some(status);
                }
            }
            MutationEffect::StatusOnly(msg) => {
                if show_completion_status {
                    self.status_message = Some(msg);
                }
            }
            MutationEffect::SentSuccess {
                status,
                remind_at,
                sent_message_id,
            } => {
                // Refresh the active label so a Sent-view user sees the new
                // message immediately. Subscriptions also refresh because
                // some sends affect mailing-list-derived counts. Owed
                // refreshes too: a successful reply removes the thread
                // from the owed lens.
                if let Some(label_id) = self.mailbox.active_label.clone() {
                    self.mailbox.pending_label_fetch = Some(label_id);
                }
                self.mailbox.pending_subscriptions_refresh = true;
                self.mailbox.pending_owed_refresh = true;
                if show_completion_status {
                    self.status_message = Some(status);
                }
                if let (Some(sent_message_id), Some(remind_at)) = (sent_message_id, remind_at) {
                    self.queue_mutation(
                        Request::SetAutoReminder {
                            sent_message_id,
                            remind_at,
                        },
                        MutationEffect::StatusOnly("Reminder set".into()),
                        "Setting reminder...".into(),
                    );
                }
            }
        }
    }

    pub(super) fn queue_mutation(
        &mut self,
        request: Request,
        effect: MutationEffect,
        status_message: String,
    ) -> MutationId {
        self.queue_mutation_with_policy(request, effect, status_message, false)
    }

    pub(super) fn queue_best_effort_mutation(
        &mut self,
        request: Request,
        effect: MutationEffect,
        status_message: String,
    ) -> MutationId {
        self.queue_mutation_with_policy(request, effect, status_message, true)
    }

    fn queue_mutation_with_policy(
        &mut self,
        request: Request,
        effect: MutationEffect,
        status_message: String,
        best_effort: bool,
    ) -> MutationId {
        let id = self.mutation_id_generator.next_id();
        let request = match request {
            Request::Mutation {
                mutation,
                client_correlation_id: _,
            } => Request::Mutation {
                mutation,
                client_correlation_id: Some(id.raw().to_string()),
            },
            other => other,
        };
        self.pending_mutation_queue.push(QueuedMutation {
            id,
            request,
            effect,
            best_effort,
            attempts: 0,
            run_after: None,
        });
        self.pending_mutation_count += 1;
        self.pending_mutation_status = Some(status_message.clone());
        self.status_message = Some(status_message);
        id
    }

    pub fn schedule_mutation_retry(&mut self, mut queued: QueuedMutation, error: &MxrError) {
        queued.attempts = queued.attempts.saturating_add(1);
        let delay = QueuedMutation::retry_delay(queued.attempts);
        queued.run_after = Some(std::time::Instant::now() + delay);
        tracing::debug!(
            mutation_id = queued.id.raw(),
            attempt = queued.attempts,
            delay_ms = delay.as_millis() as u64,
            error = %error,
            "retrying optimistic mutation after transient failure"
        );
        self.pending_mutation_queue.push(queued);
        self.pending_mutation_count += 1;
        let status = format!("Retrying mailbox update in {}s...", delay.as_secs());
        self.pending_mutation_status = Some(status.clone());
        self.status_message = Some(status);
    }

    pub fn should_retry_mutation_failure(&self, error: &MxrError) -> bool {
        let error = error.to_string().to_lowercase();
        [
            "pool timed out",
            "database is locked",
            "database table is locked",
            "connection closed",
            "connection reset",
            "connection refused",
            "timed out",
            "timeout",
            "temporarily unavailable",
        ]
        .iter()
        .any(|needle| error.contains(needle))
    }

    /// Capture a snapshot of the state about to change, suitable for
    /// reversing the optimistic effect on reconciliation failure.
    ///
    /// Snapshots are read from the current envelope state. Effects without
    /// rollback semantics (`RefreshList`, `StatusOnly`, `SentSuccess`)
    /// produce a `None` snapshot.
    pub(super) fn snapshot_for_effect(&self, effect: &MutationEffect) -> MutationSnapshot {
        match effect {
            MutationEffect::UpdateFlags { message_id, .. } => {
                let prior = self
                    .message_flags(message_id)
                    .map(|flags| (message_id.clone(), flags));
                MutationSnapshot::Flags(prior.into_iter().collect())
            }
            MutationEffect::UpdateFlagsMany { updates } => {
                let prior = updates
                    .iter()
                    .filter_map(|(id, _new)| {
                        self.message_flags(id).map(|flags| (id.clone(), flags))
                    })
                    .collect();
                MutationSnapshot::Flags(prior)
            }
            MutationEffect::ModifyLabels { message_ids, .. } => {
                let prior = message_ids
                    .iter()
                    .filter_map(|id| {
                        self.label_provider_ids_for(id)
                            .map(|labels| (id.clone(), labels))
                    })
                    .collect();
                MutationSnapshot::Labels(prior)
            }
            MutationEffect::RemoveFromList(mid) => self
                .envelope_snapshot_clone(mid)
                .map(|env| MutationSnapshot::RemovedFromLists(vec![env]))
                .unwrap_or(MutationSnapshot::None),
            MutationEffect::RemoveFromListMany(ids) => {
                let mut seen = std::collections::HashSet::new();
                let mut captured = Vec::new();
                for id in ids {
                    if !seen.insert(id.clone()) {
                        continue;
                    }
                    if let Some(env) = self.envelope_snapshot_clone(id) {
                        captured.push(env);
                    }
                }
                if captured.is_empty() {
                    MutationSnapshot::None
                } else {
                    MutationSnapshot::RemovedFromLists(captured)
                }
            }
            MutationEffect::ReplyLater { message_id, .. } => MutationSnapshot::ReplyLater(vec![(
                message_id.clone(),
                self.mailbox.reply_later_message_ids.contains(message_id),
            )]),
            MutationEffect::RefreshList
            | MutationEffect::StatusOnly(_)
            | MutationEffect::SentSuccess { .. } => MutationSnapshot::None,
        }
    }

    /// Apply the daemon's reconciliation-failed signal for a previously
    /// queued mutation: replay the captured snapshot to restore the
    /// affected envelopes' pre-mutation state. If the snapshot has been
    /// evicted (capacity reached), this is a no-op — the higher-level
    /// failure UX still surfaces the error and the next sync reconciles.
    pub fn handle_mutation_reconciliation_failed(&mut self, id: MutationId) {
        let Some(snapshot) = self.mutation_snapshots.take(id) else {
            return;
        };
        match snapshot {
            MutationSnapshot::Flags(prior) => {
                for (message_id, flags) in prior {
                    self.apply_local_flags(&message_id, flags);
                }
            }
            MutationSnapshot::Labels(prior) => {
                for (message_id, label_provider_ids) in prior {
                    self.set_local_label_provider_ids(&message_id, &label_provider_ids);
                }
            }
            MutationSnapshot::RemovedFromLists(envelopes) => {
                self.restore_removed_from_lists(envelopes);
            }
            MutationSnapshot::ReplyLater(prior) => {
                for (message_id, was_flagged) in prior {
                    if was_flagged {
                        self.mailbox.reply_later_message_ids.insert(message_id);
                    } else {
                        self.mailbox.reply_later_message_ids.remove(&message_id);
                    }
                }
            }
            MutationSnapshot::None => {}
        }
    }

    fn label_provider_ids_for(&self, message_id: &MessageId) -> Option<Vec<String>> {
        self.mailbox
            .envelopes
            .iter()
            .chain(self.mailbox.all_envelopes.iter())
            .chain(self.search.page.results.iter())
            .chain(self.mailbox.viewed_thread_messages.iter())
            .find(|envelope| &envelope.id == message_id)
            .map(|envelope| envelope.label_provider_ids.clone())
            .or_else(|| {
                self.mailbox
                    .viewing_envelope
                    .as_ref()
                    .filter(|envelope| &envelope.id == message_id)
                    .map(|envelope| envelope.label_provider_ids.clone())
            })
    }

    fn set_local_label_provider_ids(
        &mut self,
        message_id: &MessageId,
        label_provider_ids: &[String],
    ) {
        for envelopes in [
            &mut self.mailbox.envelopes,
            &mut self.mailbox.all_envelopes,
            &mut self.search.page.results,
            &mut self.mailbox.viewed_thread_messages,
        ] {
            for envelope in envelopes {
                if &envelope.id == message_id {
                    envelope.label_provider_ids = label_provider_ids.to_vec();
                }
            }
        }
        if let Some(envelope) = self.mailbox.viewing_envelope.as_mut() {
            if &envelope.id == message_id {
                envelope.label_provider_ids = label_provider_ids.to_vec();
            }
        }
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

    pub fn handle_mutation_failure_result(
        &mut self,
        id: MutationId,
        best_effort: bool,
        error: &MxrError,
    ) {
        self.handle_mutation_reconciliation_failed(id);
        self.pending_optimistic.clear(id);
        self.refresh_mailbox_after_mutation_failure();
        if best_effort {
            self.status_message = Some("Mailbox refreshing to reconcile state".into());
        } else {
            self.show_mutation_failure(error);
        }
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

    /// Full envelope clone for optimistic list-removal rollback (same lookup
    /// order as [`Self::message_flags`]).
    fn envelope_snapshot_clone(&self, message_id: &MessageId) -> Option<Envelope> {
        self.mailbox
            .envelopes
            .iter()
            .chain(self.mailbox.all_envelopes.iter())
            .chain(self.search.page.results.iter())
            .chain(self.mailbox.viewed_thread_messages.iter())
            .find(|envelope| &envelope.id == message_id)
            .cloned()
            .or_else(|| {
                self.mailbox
                    .viewing_envelope
                    .as_ref()
                    .filter(|envelope| &envelope.id == message_id)
                    .cloned()
            })
    }

    /// Undo optimistic [`MutationEffect::RemoveFromList*`] by merging rows
    /// back into every live list and re-establishing date-desc order.
    fn restore_removed_from_lists(&mut self, envelopes: Vec<Envelope>) {
        if envelopes.is_empty() {
            return;
        }

        let merge = |list: &mut Vec<Envelope>| {
            for env in &envelopes {
                if list.iter().any(|e| e.id == env.id) {
                    continue;
                }
                list.push(env.clone());
            }
            list.sort_unstable_by_key(|envelope| std::cmp::Reverse(envelope.date));
        };

        merge(&mut self.mailbox.all_envelopes);
        merge(&mut self.mailbox.envelopes);
        merge(&mut self.search.page.results);
        merge(&mut self.mailbox.viewed_thread_messages);

        self.mailbox.selected_index = self
            .mailbox
            .selected_index
            .min(self.mail_row_count().saturating_sub(1));
        self.search.page.selected_index = self
            .search
            .page
            .selected_index
            .min(self.search_row_count().saturating_sub(1));

        if self.screen == Screen::Mailbox && self.mail_row_count() > 0 {
            self.ensure_visible();
        } else if self.screen == Screen::Search && self.search_row_count() > 0 {
            self.ensure_search_visible();
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
            return self.mailbox.selected_set.iter().cloned().collect();
        }
        let Some(env) = self.context_envelope() else {
            return vec![];
        };
        if self.mailbox.mail_list_mode != MailListMode::Threads {
            return vec![env.id.clone()];
        }
        // In Threads mode, a row represents the whole conversation. Mutations
        // operate on every visible message in that thread to match Gmail's
        // thread-level archive/trash/star semantics.
        let source: &[Envelope] = if self.screen == Screen::Search {
            &self.search.page.results
        } else {
            &self.mailbox.envelopes
        };
        let tid = env.thread_id.clone();
        let mut ids: Vec<MessageId> = source
            .iter()
            .filter(|candidate| candidate.thread_id == tid)
            .map(|candidate| candidate.id.clone())
            .collect();
        if ids.is_empty() {
            ids.push(env.id.clone());
        }
        ids
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
            let snapshot = optimistic_effect
                .as_ref()
                .map(|effect| self.snapshot_for_effect(effect));
            if let Some(effect) = optimistic_effect.as_ref() {
                self.apply_local_mutation_effect(effect);
            }
            let id = self.queue_mutation(request, effect, status_message);
            if let Some(effect) = optimistic_effect.as_ref() {
                self.pending_optimistic.record(id, effect);
            }
            if let Some(snapshot) = snapshot {
                self.mutation_snapshots.insert(id, snapshot);
            }
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
