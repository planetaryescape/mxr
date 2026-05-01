use super::*;

impl App {
    pub(super) fn apply_mutation_action(&mut self, action: Action) {
        match action {
            Action::Archive => {
                let ids = self.mutation_target_ids();
                if !ids.is_empty() {
                    let effect = remove_from_list_effect(&ids);
                    self.queue_or_confirm_bulk_action(
                        "Archive messages",
                        bulk_message_detail("archive", ids.len()),
                        Request::Mutation(MutationCommand::Archive {
                            message_ids: ids.clone(),
                        }),
                        effect.clone(),
                        Some(effect),
                        "Archiving...".into(),
                        ids.len(),
                    );
                }
            }
            Action::MarkReadAndArchive => {
                let ids = self.mutation_target_ids();
                if !ids.is_empty() {
                    let effect = remove_from_list_effect(&ids);
                    self.queue_or_confirm_bulk_action(
                        "Mark messages as read and archive",
                        bulk_message_detail("mark as read and archive", ids.len()),
                        Request::Mutation(MutationCommand::ReadAndArchive {
                            message_ids: ids.clone(),
                        }),
                        effect.clone(),
                        Some(effect),
                        format!(
                            "Marking {} {} as read and archiving...",
                            ids.len(),
                            pluralize_messages(ids.len())
                        ),
                        ids.len(),
                    );
                }
            }
            Action::Trash => {
                let ids = self.mutation_target_ids();
                if !ids.is_empty() {
                    let effect = remove_from_list_effect(&ids);
                    self.queue_or_confirm_bulk_action(
                        "Delete messages",
                        bulk_message_detail("delete", ids.len()),
                        Request::Mutation(MutationCommand::Trash {
                            message_ids: ids.clone(),
                        }),
                        effect.clone(),
                        Some(effect),
                        "Trashing...".into(),
                        ids.len(),
                    );
                }
            }
            Action::Spam => {
                let ids = self.mutation_target_ids();
                if !ids.is_empty() {
                    let effect = remove_from_list_effect(&ids);
                    self.queue_or_confirm_bulk_action(
                        "Mark as spam",
                        bulk_message_detail("mark as spam", ids.len()),
                        Request::Mutation(MutationCommand::Spam {
                            message_ids: ids.clone(),
                        }),
                        effect.clone(),
                        Some(effect),
                        "Marking as spam...".into(),
                        ids.len(),
                    );
                }
            }
            Action::Star => {
                let ids = self.mutation_target_ids();
                if !ids.is_empty() {
                    // For single selection, toggle. For multi, always star.
                    let starred = if ids.len() == 1 {
                        if let Some(env) = self.context_envelope() {
                            !env.flags.contains(MessageFlags::STARRED)
                        } else {
                            true
                        }
                    } else {
                        true
                    };
                    let updates = self.flag_updates_for_ids(&ids, |mut flags| {
                        if starred {
                            flags.insert(MessageFlags::STARRED);
                        } else {
                            flags.remove(MessageFlags::STARRED);
                        }
                        flags
                    });
                    let optimistic_effect = (!updates.is_empty())
                        .then_some(MutationEffect::UpdateFlagsMany { updates });
                    let verb = if starred { "star" } else { "unstar" };
                    let status = if starred {
                        format!(
                            "Starring {} {}...",
                            ids.len(),
                            pluralize_messages(ids.len())
                        )
                    } else {
                        format!(
                            "Unstarring {} {}...",
                            ids.len(),
                            pluralize_messages(ids.len())
                        )
                    };
                    self.queue_or_confirm_bulk_action(
                        if starred {
                            "Star messages"
                        } else {
                            "Unstar messages"
                        },
                        bulk_message_detail(verb, ids.len()),
                        Request::Mutation(MutationCommand::Star {
                            message_ids: ids.clone(),
                            starred,
                        }),
                        MutationEffect::StatusOnly(if starred {
                            format!("Starred {} {}", ids.len(), pluralize_messages(ids.len()))
                        } else {
                            format!("Unstarred {} {}", ids.len(), pluralize_messages(ids.len()))
                        }),
                        optimistic_effect,
                        status,
                        ids.len(),
                    );
                }
            }
            Action::MarkRead => {
                let ids = self.mutation_target_ids();
                if !ids.is_empty() {
                    let updates = self.flag_updates_for_ids(&ids, |mut flags| {
                        flags.insert(MessageFlags::READ);
                        flags
                    });
                    self.queue_or_confirm_bulk_action(
                        "Mark messages as read",
                        bulk_message_detail("mark as read", ids.len()),
                        Request::Mutation(MutationCommand::SetRead {
                            message_ids: ids.clone(),
                            read: true,
                        }),
                        MutationEffect::StatusOnly(format!(
                            "Marked {} {} as read",
                            ids.len(),
                            pluralize_messages(ids.len())
                        )),
                        (!updates.is_empty())
                            .then_some(MutationEffect::UpdateFlagsMany { updates }),
                        format!(
                            "Marking {} {} as read...",
                            ids.len(),
                            pluralize_messages(ids.len())
                        ),
                        ids.len(),
                    );
                }
            }
            Action::MarkUnread => {
                let ids = self.mutation_target_ids();
                if !ids.is_empty() {
                    let updates = self.flag_updates_for_ids(&ids, |mut flags| {
                        flags.remove(MessageFlags::READ);
                        flags
                    });
                    self.queue_or_confirm_bulk_action(
                        "Mark messages as unread",
                        bulk_message_detail("mark as unread", ids.len()),
                        Request::Mutation(MutationCommand::SetRead {
                            message_ids: ids.clone(),
                            read: false,
                        }),
                        MutationEffect::StatusOnly(format!(
                            "Marked {} {} as unread",
                            ids.len(),
                            pluralize_messages(ids.len())
                        )),
                        (!updates.is_empty())
                            .then_some(MutationEffect::UpdateFlagsMany { updates }),
                        format!(
                            "Marking {} {} as unread...",
                            ids.len(),
                            pluralize_messages(ids.len())
                        ),
                        ids.len(),
                    );
                }
            }
            Action::ApplyLabel => {
                if let Some((_, ref label_name)) = self.modals.pending_label_action.take() {
                    // Label picker confirmed — dispatch mutation
                    let ids = self.mutation_target_ids();
                    if !ids.is_empty() {
                        self.queue_or_confirm_bulk_action(
                            "Apply label",
                            format!(
                                "You are about to apply '{}' to {} {}.",
                                label_name,
                                ids.len(),
                                pluralize_messages(ids.len())
                            ),
                            Request::Mutation(MutationCommand::ModifyLabels {
                                message_ids: ids.clone(),
                                add: vec![label_name.clone()],
                                remove: vec![],
                            }),
                            MutationEffect::ModifyLabels {
                                message_ids: ids.clone(),
                                add: vec![label_name.clone()],
                                remove: vec![],
                                status: format!("Applied label '{}'", label_name),
                            },
                            None,
                            format!("Applying label '{}'...", label_name),
                            ids.len(),
                        );
                    }
                } else {
                    // Open label picker
                    self.modals
                        .label_picker
                        .open(self.mailbox.labels.clone(), LabelPickerMode::Apply);
                }
            }
            Action::MoveToLabel => {
                if let Some((_, ref label_name)) = self.modals.pending_label_action.take() {
                    // Label picker confirmed — dispatch move
                    let ids = self.mutation_target_ids();
                    if !ids.is_empty() {
                        self.queue_or_confirm_bulk_action(
                            "Move messages",
                            format!(
                                "You are about to move {} {} to '{}'.",
                                ids.len(),
                                pluralize_messages(ids.len()),
                                label_name
                            ),
                            Request::Mutation(MutationCommand::Move {
                                message_ids: ids.clone(),
                                target_label: label_name.clone(),
                            }),
                            remove_from_list_effect(&ids),
                            None,
                            format!("Moving to '{}'...", label_name),
                            ids.len(),
                        );
                    }
                } else {
                    // Open label picker
                    self.modals
                        .label_picker
                        .open(self.mailbox.labels.clone(), LabelPickerMode::Move);
                }
            }
            Action::Unsubscribe => {
                if let Some(env) = self.context_envelope() {
                    if matches!(env.unsubscribe, UnsubscribeMethod::None) {
                        self.status_message =
                            Some("No unsubscribe option found for this message".into());
                    } else {
                        let sender_email = env.from.email.clone();
                        let archive_message_ids = self
                            .mailbox
                            .all_envelopes
                            .iter()
                            .filter(|candidate| {
                                candidate.account_id == env.account_id
                                    && candidate.from.email.eq_ignore_ascii_case(&sender_email)
                            })
                            .map(|candidate| candidate.id.clone())
                            .collect();
                        self.modals.pending_unsubscribe_confirm = Some(PendingUnsubscribeConfirm {
                            message_id: env.id.clone(),
                            account_id: env.account_id.clone(),
                            sender_email,
                            method_label: unsubscribe_method_label(&env.unsubscribe).to_string(),
                            archive_message_ids,
                        });
                    }
                }
            }
            Action::ConfirmUnsubscribeOnly => {
                if let Some(pending) = self.modals.pending_unsubscribe_confirm.take() {
                    self.modals.pending_unsubscribe_action = Some(PendingUnsubscribeAction {
                        message_id: pending.message_id,
                        archive_message_ids: Vec::new(),
                        sender_email: pending.sender_email,
                    });
                    self.status_message = Some("Unsubscribing...".into());
                }
            }
            Action::ConfirmUnsubscribeAndArchiveSender => {
                if let Some(pending) = self.modals.pending_unsubscribe_confirm.take() {
                    self.modals.pending_unsubscribe_action = Some(PendingUnsubscribeAction {
                        message_id: pending.message_id,
                        archive_message_ids: pending.archive_message_ids,
                        sender_email: pending.sender_email,
                    });
                    self.status_message = Some("Unsubscribing and archiving sender...".into());
                }
            }
            Action::CancelUnsubscribe => {
                self.modals.pending_unsubscribe_confirm = None;
                self.status_message = Some("Unsubscribe cancelled".into());
            }
            Action::Snooze => {
                if self.modals.snooze_panel.visible {
                    if let Some(env) = self.context_envelope() {
                        let wake_at = resolve_snooze_preset(
                            snooze_presets()[self.modals.snooze_panel.selected_index],
                            &self.modals.snooze_config,
                        );
                        self.queue_mutation(
                            Request::Snooze {
                                message_id: env.id.clone(),
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
                } else if self.context_envelope().is_some() {
                    self.modals.snooze_panel.visible = true;
                    self.modals.snooze_panel.selected_index = 0;
                } else {
                    self.status_message = Some("No message selected".into());
                }
            }
            Action::ToggleSelect => {
                if let Some(env) = self.context_envelope() {
                    let should_advance = matches!(
                        self.current_ui_context(),
                        UiContext::MailboxList | UiContext::SearchResults
                    );
                    let id = env.id.clone();
                    if self.mailbox.selected_set.contains(&id) {
                        self.mailbox.selected_set.remove(&id);
                    } else {
                        self.mailbox.selected_set.insert(id);
                    }
                    if should_advance && self.screen == Screen::Search {
                        if self.search.page.selected_index + 1 < self.search_row_count() {
                            self.search.page.selected_index += 1;
                        }
                        self.sync_search_cursor_after_move();
                    } else if should_advance
                        && self.mailbox.selected_index + 1 < self.mail_row_count()
                    {
                        self.mailbox.selected_index += 1;
                        self.ensure_visible();
                        self.auto_preview();
                    }
                    let count = self.mailbox.selected_set.len();
                    self.status_message = Some(format!("{count} selected"));
                }
            }
            Action::VisualLineMode => {
                if self.mailbox.visual_mode {
                    // Exit visual mode
                    self.mailbox.visual_mode = false;
                    self.mailbox.visual_anchor = None;
                    self.status_message = Some("Visual mode off".into());
                } else {
                    self.mailbox.visual_mode = true;
                    self.mailbox.visual_anchor = Some(if self.screen == Screen::Search {
                        self.search.page.selected_index
                    } else {
                        self.mailbox.selected_index
                    });
                    // Add current to selection
                    if let Some(env) = self.context_envelope() {
                        self.mailbox.selected_set.insert(env.id.clone());
                    }
                    self.status_message = Some("-- VISUAL LINE --".into());
                }
            }
            Action::PatternSelect(pattern) => {
                let envelopes = if self.screen == Screen::Search {
                    &self.search.page.results
                } else {
                    &self.mailbox.envelopes
                };
                match pattern {
                    PatternKind::All => {
                        self.mailbox.selected_set =
                            envelopes.iter().map(|e| e.id.clone()).collect();
                    }
                    PatternKind::None => {
                        self.mailbox.selected_set.clear();
                        self.mailbox.visual_mode = false;
                        self.mailbox.visual_anchor = None;
                    }
                    PatternKind::Read => {
                        self.mailbox.selected_set = envelopes
                            .iter()
                            .filter(|e| e.flags.contains(MessageFlags::READ))
                            .map(|e| e.id.clone())
                            .collect();
                    }
                    PatternKind::Unread => {
                        self.mailbox.selected_set = envelopes
                            .iter()
                            .filter(|e| !e.flags.contains(MessageFlags::READ))
                            .map(|e| e.id.clone())
                            .collect();
                    }
                    PatternKind::Starred => {
                        self.mailbox.selected_set = envelopes
                            .iter()
                            .filter(|e| e.flags.contains(MessageFlags::STARRED))
                            .map(|e| e.id.clone())
                            .collect();
                    }
                    PatternKind::Thread => {
                        if let Some(env) = self.context_envelope() {
                            let tid = env.thread_id.clone();
                            self.mailbox.selected_set = envelopes
                                .iter()
                                .filter(|e| e.thread_id == tid)
                                .map(|e| e.id.clone())
                                .collect();
                        }
                    }
                }
                let count = self.mailbox.selected_set.len();
                self.status_message = Some(format!("{count} selected"));
            }

            // Phase 2: Other actions
            _ => unreachable!("action routed to wrong handler"),
        }
    }
}
