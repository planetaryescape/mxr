use super::mutation_helpers::BulkActionRequest;
use super::*;

impl App {
    pub(super) fn apply_mutation_action(&mut self, action: Action) {
        // On the desk and in a place, verbs act on the row under the cursor
        // (a desk row's account and whole thread, a place's one message),
        // never on whatever the mailbox or reader showed last. Verbs that
        // need more than a row ask to open it.
        let row_lens_hint = if self.desk_list_focused() {
            // Archive on a thread you wrote last is "done waiting": there is
            // nothing in the inbox for archive to remove.
            if action == Action::Archive && self.done_waiting_on_selected_desk_row() {
                return;
            }
            Some("Open the conversation (Enter) to do that from the desk")
        } else if self.place_list_focused() {
            Some("Open the message (Enter) to do that from here")
        } else {
            None
        };
        if let Some(hint) = row_lens_hint {
            if matches!(
                action,
                Action::ApplyLabel
                    | Action::MoveToLabel
                    | Action::RouteToLabel
                    | Action::Unsubscribe
                    | Action::Snooze
                    | Action::ToggleSelect
                    | Action::VisualLineMode
                    | Action::PatternSelect(_)
            ) {
                self.status_message = Some(hint.into());
                return;
            }
        }
        match action {
            Action::Archive => {
                let ids = self.mutation_target_ids();
                if !ids.is_empty() {
                    // Archive removes INBOX. The list should hide the row only
                    // when we're viewing INBOX; in Starred / All Mail / custom
                    // labels the message still belongs in the current view, so
                    // an optimistic removal would just flicker until the next
                    // sync put it back.
                    let removes_from_view = self.active_label_matches("INBOX");
                    let completion_effect = if removes_from_view {
                        remove_from_list_effect(&ids)
                    } else {
                        MutationEffect::StatusOnly(format!(
                            "Archived {} {}",
                            ids.len(),
                            pluralize_messages(ids.len())
                        ))
                    };
                    let optimistic_effect =
                        removes_from_view.then(|| remove_from_list_effect(&ids));
                    self.queue_or_confirm_bulk_action(BulkActionRequest {
                        title: "Archive messages".into(),
                        detail: bulk_message_detail("archive", ids.len()),
                        request: Request::mutation(MutationCommand::Archive {
                            message_ids: ids.clone(),
                        }),
                        effect: completion_effect,
                        optimistic_effect,
                        status_message: "Archiving...".into(),
                        count: ids.len(),
                    });
                }
            }
            Action::MarkReadAndArchive => {
                let ids = self.mutation_target_ids();
                if !ids.is_empty() {
                    let removes_from_view = self.active_label_matches("INBOX");
                    let completion_effect = if removes_from_view {
                        remove_from_list_effect(&ids)
                    } else {
                        MutationEffect::StatusOnly(format!(
                            "Marked {} {} as read and archived",
                            ids.len(),
                            pluralize_messages(ids.len())
                        ))
                    };
                    let optimistic_effect =
                        removes_from_view.then(|| remove_from_list_effect(&ids));
                    self.queue_or_confirm_bulk_action(BulkActionRequest {
                        title: "Mark messages as read and archive".into(),
                        detail: bulk_message_detail("mark as read and archive", ids.len()),
                        request: Request::mutation(MutationCommand::ReadAndArchive {
                            message_ids: ids.clone(),
                        }),
                        effect: completion_effect,
                        optimistic_effect,
                        status_message: format!(
                            "Marking {} {} as read and archiving...",
                            ids.len(),
                            pluralize_messages(ids.len())
                        ),
                        count: ids.len(),
                    });
                }
            }
            Action::Trash => {
                let ids = self.mutation_target_ids();
                if !ids.is_empty() {
                    let effect = remove_from_list_effect(&ids);
                    self.queue_or_confirm_bulk_action(BulkActionRequest {
                        title: "Delete messages".into(),
                        detail: bulk_message_detail("delete", ids.len()),
                        request: Request::mutation(MutationCommand::Trash {
                            message_ids: ids.clone(),
                        }),
                        effect: effect.clone(),
                        optimistic_effect: Some(effect),
                        status_message: "Trashing...".into(),
                        count: ids.len(),
                    });
                }
            }
            Action::Spam => {
                let ids = self.mutation_target_ids();
                if !ids.is_empty() {
                    let effect = remove_from_list_effect(&ids);
                    self.queue_or_confirm_bulk_action(BulkActionRequest {
                        title: "Mark as spam".into(),
                        detail: bulk_message_detail("mark as spam", ids.len()),
                        request: Request::mutation(MutationCommand::Spam {
                            message_ids: ids.clone(),
                        }),
                        effect: effect.clone(),
                        optimistic_effect: Some(effect),
                        status_message: "Marking as spam...".into(),
                        count: ids.len(),
                    });
                }
            }
            Action::UndoLastMutation => {
                if self.pending_invite_send.take().is_some() {
                    self.status_message =
                        Some(mxr_core::i18n::EN.status.invite_cancelled.to_string());
                } else if let Some(undo) = self.take_pending_undo() {
                    // A batch job (a sweep) left one undo per chunk; each
                    // chunk touched different messages, so order is free.
                    // RefreshList re-fetches the active label so restored
                    // messages reappear; the status reads "Undoing..." until
                    // the daemon acknowledges.
                    let status = format!(
                        "Undoing {} {}...",
                        undo.verb_past.to_lowercase(),
                        undo.count
                    );
                    match undo.action {
                        UndoAction::Mutations(mutation_ids) => {
                            for mutation_id in mutation_ids {
                                self.queue_mutation(
                                    Request::UndoMutation { mutation_id },
                                    MutationEffect::RefreshList,
                                    status.clone(),
                                );
                            }
                        }
                        // Setting the previous kind again is the undo; its
                        // own answer offers no further undo.
                        UndoAction::SenderKind {
                            account_id,
                            sender_email,
                            previous,
                        } => {
                            self.queue_mutation(
                                Request::SetSenderKind {
                                    account_id,
                                    sender_email,
                                    kind: previous,
                                },
                                MutationEffect::RefreshPlaces("Sender moved back".into()),
                                status,
                            );
                        }
                    }
                } else {
                    self.status_message =
                        Some("Nothing to undo (window expired or no recent mutation)".into());
                }
            }
            Action::Star => {
                let ids = self.mutation_target_ids();
                if !ids.is_empty() {
                    // For single selection, toggle. For multi, always star. A
                    // desk row is one conversation: toggle its thread star.
                    let starred = if let Some(row) = self
                        .desk_list_focused()
                        .then(|| self.selected_desk_row())
                        .flatten()
                    {
                        !row.starred
                    } else if let Some((_, message)) = self
                        .place_list_focused()
                        .then(|| self.selected_place_row())
                        .flatten()
                    {
                        !message.starred
                    } else if ids.len() == 1 {
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
                    self.queue_or_confirm_bulk_action(BulkActionRequest {
                        title: if starred {
                            "Star messages"
                        } else {
                            "Unstar messages"
                        }
                        .into(),
                        detail: bulk_message_detail(verb, ids.len()),
                        request: Request::mutation(MutationCommand::Star {
                            message_ids: ids.clone(),
                            starred,
                        }),
                        effect: MutationEffect::StatusOnly(if starred {
                            format!("Starred {} {}", ids.len(), pluralize_messages(ids.len()))
                        } else {
                            format!("Unstarred {} {}", ids.len(), pluralize_messages(ids.len()))
                        }),
                        optimistic_effect,
                        status_message: status,
                        count: ids.len(),
                    });
                }
            }
            Action::MarkRead => {
                let ids = self.mutation_target_ids();
                if !ids.is_empty() {
                    let updates = self.flag_updates_for_ids(&ids, |mut flags| {
                        flags.insert(MessageFlags::READ);
                        flags
                    });
                    self.queue_or_confirm_bulk_action(BulkActionRequest {
                        title: "Mark messages as read".into(),
                        detail: bulk_message_detail("mark as read", ids.len()),
                        request: Request::mutation(MutationCommand::SetRead {
                            message_ids: ids.clone(),
                            read: true,
                        }),
                        effect: MutationEffect::StatusOnly(format!(
                            "Marked {} {} as read",
                            ids.len(),
                            pluralize_messages(ids.len())
                        )),
                        optimistic_effect: (!updates.is_empty())
                            .then_some(MutationEffect::UpdateFlagsMany { updates }),
                        status_message: format!(
                            "Marking {} {} as read...",
                            ids.len(),
                            pluralize_messages(ids.len())
                        ),
                        count: ids.len(),
                    });
                }
            }
            Action::MarkUnread => {
                let ids = self.mutation_target_ids();
                if !ids.is_empty() {
                    let updates = self.flag_updates_for_ids(&ids, |mut flags| {
                        flags.remove(MessageFlags::READ);
                        flags
                    });
                    self.queue_or_confirm_bulk_action(BulkActionRequest {
                        title: "Mark messages as unread".into(),
                        detail: bulk_message_detail("mark as unread", ids.len()),
                        request: Request::mutation(MutationCommand::SetRead {
                            message_ids: ids.clone(),
                            read: false,
                        }),
                        effect: MutationEffect::StatusOnly(format!(
                            "Marked {} {} as unread",
                            ids.len(),
                            pluralize_messages(ids.len())
                        )),
                        optimistic_effect: (!updates.is_empty())
                            .then_some(MutationEffect::UpdateFlagsMany { updates }),
                        status_message: format!(
                            "Marking {} {} as unread...",
                            ids.len(),
                            pluralize_messages(ids.len())
                        ),
                        count: ids.len(),
                    });
                }
            }
            Action::ApplyLabel => {
                if let Some((_, ref label_name)) = self.modals.pending_label_action.take() {
                    // Label picker confirmed — dispatch mutation
                    let ids = self.mutation_target_ids();
                    if !ids.is_empty() {
                        let optimistic_effect = MutationEffect::ModifyLabels {
                            message_ids: ids.clone(),
                            add: vec![label_name.clone()],
                            remove: vec![],
                            status: String::new(),
                        };
                        self.queue_or_confirm_bulk_action(BulkActionRequest {
                            title: "Apply label".into(),
                            detail: format!(
                                "You are about to apply '{}' to {} {}.",
                                label_name,
                                ids.len(),
                                pluralize_messages(ids.len())
                            ),
                            request: Request::mutation(MutationCommand::ModifyLabels {
                                message_ids: ids.clone(),
                                add: vec![label_name.clone()],
                                remove: vec![],
                            }),
                            effect: MutationEffect::ModifyLabels {
                                message_ids: ids.clone(),
                                add: vec![label_name.clone()],
                                remove: vec![],
                                status: format!("Applied label '{label_name}'"),
                            },
                            optimistic_effect: Some(optimistic_effect),
                            status_message: format!("Applying label '{label_name}'..."),
                            count: ids.len(),
                        });
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
                        let optimistic_effect = remove_from_list_effect(&ids);
                        self.queue_or_confirm_bulk_action(BulkActionRequest {
                            title: "Move messages".into(),
                            detail: format!(
                                "You are about to move {} {} to '{}'.",
                                ids.len(),
                                pluralize_messages(ids.len()),
                                label_name
                            ),
                            request: Request::mutation(MutationCommand::Move {
                                message_ids: ids.clone(),
                                target_label: label_name.clone(),
                            }),
                            effect: remove_from_list_effect(&ids),
                            optimistic_effect: Some(optimistic_effect),
                            status_message: format!("Moving to '{label_name}'..."),
                            count: ids.len(),
                        });
                    }
                } else {
                    // Open label picker
                    self.modals
                        .label_picker
                        .open(self.mailbox.labels.clone(), LabelPickerMode::Move);
                }
            }
            Action::RouteToLabel => {
                if let Some((_, ref label_name)) = self.modals.pending_label_action.take() {
                    let from_queue_label = self
                        .active_label_record()
                        .map_or_else(|| "INBOX".to_string(), |label| label.name.clone());
                    let ids = self.mutation_target_ids();
                    if !ids.is_empty() {
                        let optimistic_effect = remove_from_list_effect(&ids);
                        self.queue_or_confirm_bulk_action(BulkActionRequest {
                            title: "Route messages".into(),
                            detail: format!(
                                "You are about to route {} {} from '{}' to '{}' and archive.",
                                ids.len(),
                                pluralize_messages(ids.len()),
                                from_queue_label,
                                label_name
                            ),
                            request: Request::mutation(MutationCommand::Route {
                                message_ids: ids.clone(),
                                to_label: label_name.clone(),
                                from_queue_label: from_queue_label.clone(),
                                archive: true,
                                dry_run: false,
                            }),
                            effect: remove_from_list_effect(&ids),
                            optimistic_effect: Some(optimistic_effect),
                            status_message: format!("Routing to '{label_name}'..."),
                            count: ids.len(),
                        });
                    }
                } else if self.active_label_record().is_some() {
                    self.modals
                        .label_picker
                        .open(self.mailbox.labels.clone(), LabelPickerMode::Route);
                } else {
                    self.status_message = Some("Open a queue label before routing".into());
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
                        account_id: pending.account_id,
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
                        account_id: pending.account_id,
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
                    self.handle_snooze_panel_confirm();
                } else if self.context_envelope().is_some() {
                    self.modals.snooze_panel.visible = true;
                    self.modals.snooze_panel.selected_index = 0;
                    self.modals.snooze_panel.custom_input = None;
                    self.modals.snooze_panel.custom_error = None;
                } else {
                    self.status_message = Some("No message selected".into());
                }
            }
            Action::RespondInvite(action) => {
                let message_id = if self.mailbox.mailbox_view == MailboxView::CalendarInvites {
                    // In the invites lens the selected row is known to be an
                    // invite, so skip the body-cache calendar check.
                    let Some(invite) = self.selected_invite() else {
                        self.status_message = Some("No invite selected".into());
                        return;
                    };
                    invite.message_id.clone()
                } else {
                    let Some(env) = self.context_envelope().cloned() else {
                        self.status_message = Some("No message selected".into());
                        return;
                    };
                    if self
                        .mailbox
                        .body_cache
                        .get(&env.id)
                        .and_then(|body| body.metadata.calendar.as_ref())
                        .is_none()
                    {
                        self.status_message =
                            Some("No calendar invite found for this message".into());
                        return;
                    }
                    env.id
                };
                let partstat = invite_action_partstat(action);
                let status_label = mxr_core::i18n::EN
                    .invite_status_pending_for(partstat)
                    .to_string();
                self.status_message = Some(status_label.clone());
                self.pending_invite_send = Some(crate::app::PendingInviteSend {
                    message_id,
                    action,
                    dispatch_at: std::time::Instant::now() + std::time::Duration::from_secs(1),
                    status_label,
                });
            }
            Action::RespondInviteWithComment(action) => {
                let (message_id, account_id) =
                    if self.mailbox.mailbox_view == MailboxView::CalendarInvites {
                        let Some(invite) = self.selected_invite() else {
                            self.status_message = Some("No invite selected".into());
                            return;
                        };
                        (invite.message_id.clone(), invite.account_id.clone())
                    } else {
                        let Some(env) = self.context_envelope().cloned() else {
                            self.status_message = Some("No message selected".into());
                            return;
                        };
                        if self
                            .mailbox
                            .body_cache
                            .get(&env.id)
                            .and_then(|body| body.metadata.calendar.as_ref())
                            .is_none()
                        {
                            self.status_message =
                                Some("No calendar invite found for this message".into());
                            return;
                        }
                        (env.id, env.account_id.clone())
                    };
                self.status_message = Some("Preparing invite reply...".into());
                self.compose.pending_compose =
                    Some(crate::app::ComposeAction::InviteReplyWithComment {
                        message_id,
                        account_id,
                        action,
                    });
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

fn invite_action_partstat(
    action: mxr_protocol::CalendarInviteActionData,
) -> mxr_core::i18n::SendableCalendarPartstat {
    use mxr_core::i18n::SendableCalendarPartstat;
    use mxr_protocol::CalendarInviteActionData;
    match action {
        CalendarInviteActionData::Accept => SendableCalendarPartstat::Accepted,
        CalendarInviteActionData::Tentative => SendableCalendarPartstat::Tentative,
        CalendarInviteActionData::Decline => SendableCalendarPartstat::Declined,
    }
}
