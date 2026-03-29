use super::*;

impl App {
    pub(super) fn apply_messages(&mut self, action: Action) {
        match action {
            Action::OpenSelected => {
                if let Some(pending) = self.pending_bulk_confirm.take() {
                    if let Some(effect) = pending.optimistic_effect.as_ref() {
                        self.apply_local_mutation_effect(effect);
                    }
                    self.queue_mutation(pending.request, pending.effect, pending.status_message);
                    self.clear_selection();
                    return;
                }
                if self.screen == Screen::Search {
                    self.open_selected_search_result();
                    return;
                }
                if self.mailbox_view == MailboxView::Subscriptions {
                    if let Some(entry) = self.selected_subscription_entry().cloned() {
                        self.open_envelope(entry.envelope);
                        self.layout_mode = LayoutMode::ThreePane;
                        self.active_pane = ActivePane::MessageView;
                    }
                    return;
                }
                if let Some(row) = self.selected_mail_row() {
                    self.open_envelope(row.representative);
                    self.layout_mode = LayoutMode::ThreePane;
                    self.active_pane = ActivePane::MessageView;
                }
            }
            Action::ClearSelection => {
                self.clear_selection();
                self.status_message = Some("Selection cleared".into());
            }
            Action::OpenMessageView => {
                if self.screen == Screen::Search {
                    self.open_selected_search_result();
                    return;
                }
                if self.mailbox_view == MailboxView::Subscriptions {
                    if let Some(entry) = self.selected_subscription_entry().cloned() {
                        self.open_envelope(entry.envelope);
                        self.layout_mode = LayoutMode::ThreePane;
                    }
                } else if let Some(row) = self.selected_mail_row() {
                    self.open_envelope(row.representative);
                    self.layout_mode = LayoutMode::ThreePane;
                }
            }
            Action::CloseMessageView => {
                if self.screen == Screen::Search {
                    self.reset_search_preview_selection();
                    return;
                }
                self.close_attachment_panel();
                self.layout_mode = LayoutMode::TwoPane;
                self.active_pane = ActivePane::MailList;
                self.pending_preview_read = None;
                self.viewing_envelope = None;
                self.viewed_thread = None;
                self.viewed_thread_messages.clear();
                self.thread_selected_index = 0;
                self.pending_thread_fetch = None;
                self.in_flight_thread_fetch = None;
                self.message_scroll_offset = 0;
                self.body_view_state = BodyViewState::Empty { preview: None };
            }
            Action::ToggleMailListMode => {
                if self.mailbox_view == MailboxView::Subscriptions {
                    return;
                }
                self.mail_list_mode = match self.mail_list_mode {
                    MailListMode::Threads => MailListMode::Messages,
                    MailListMode::Messages => MailListMode::Threads,
                };
                self.selected_index = self
                    .selected_index
                    .min(self.mail_row_count().saturating_sub(1));
            }
            // Compose
            Action::Compose => {
                // Build contacts from known envelopes (senders we've seen)
                let mut seen = std::collections::HashMap::new();
                for env in &self.all_envelopes {
                    seen.entry(env.from.email.clone()).or_insert_with(|| {
                        crate::ui::compose_picker::Contact {
                            name: env.from.name.clone().unwrap_or_default(),
                            email: env.from.email.clone(),
                        }
                    });
                }
                let mut contacts: Vec<_> = seen.into_values().collect();
                contacts.sort_by(|a, b| a.email.to_lowercase().cmp(&b.email.to_lowercase()));
                self.compose_picker.open(contacts);
            }
            Action::Reply => {
                if let Some(env) = self.context_envelope() {
                    self.pending_compose = Some(ComposeAction::Reply {
                        message_id: env.id.clone(),
                    });
                }
            }
            Action::ReplyAll => {
                if let Some(env) = self.context_envelope() {
                    self.pending_compose = Some(ComposeAction::ReplyAll {
                        message_id: env.id.clone(),
                    });
                }
            }
            Action::Forward => {
                if let Some(env) = self.context_envelope() {
                    self.pending_compose = Some(ComposeAction::Forward {
                        message_id: env.id.clone(),
                    });
                }
            }
            // Mutations
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
                if let Some((_, ref label_name)) = self.pending_label_action.take() {
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
                                status: format!("Applied label '{label_name}'"),
                            },
                            None,
                            format!("Applying label '{label_name}'..."),
                            ids.len(),
                        );
                    }
                } else {
                    // Open label picker
                    self.label_picker
                        .open(self.labels.clone(), LabelPickerMode::Apply);
                }
            }
            Action::MoveToLabel => {
                if let Some((_, ref label_name)) = self.pending_label_action.take() {
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
                            format!("Moving to '{label_name}'..."),
                            ids.len(),
                        );
                    }
                } else {
                    // Open label picker
                    self.label_picker
                        .open(self.labels.clone(), LabelPickerMode::Move);
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
                            .all_envelopes
                            .iter()
                            .filter(|candidate| {
                                candidate.account_id == env.account_id
                                    && candidate.from.email.eq_ignore_ascii_case(&sender_email)
                            })
                            .map(|candidate| candidate.id.clone())
                            .collect();
                        self.pending_unsubscribe_confirm = Some(PendingUnsubscribeConfirm {
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
                if let Some(pending) = self.pending_unsubscribe_confirm.take() {
                    self.pending_unsubscribe_action = Some(PendingUnsubscribeAction {
                        message_id: pending.message_id,
                        archive_message_ids: Vec::new(),
                        sender_email: pending.sender_email,
                    });
                    self.status_message = Some("Unsubscribing...".into());
                }
            }
            Action::ConfirmUnsubscribeAndArchiveSender => {
                if let Some(pending) = self.pending_unsubscribe_confirm.take() {
                    self.pending_unsubscribe_action = Some(PendingUnsubscribeAction {
                        message_id: pending.message_id,
                        archive_message_ids: pending.archive_message_ids,
                        sender_email: pending.sender_email,
                    });
                    self.status_message = Some("Unsubscribing and archiving sender...".into());
                }
            }
            Action::CancelUnsubscribe => {
                self.pending_unsubscribe_confirm = None;
                self.status_message = Some("Unsubscribe cancelled".into());
            }
            Action::Snooze => {
                if self.snooze_panel.visible {
                    if let Some(env) = self.context_envelope() {
                        let wake_at = resolve_snooze_preset(
                            snooze_presets()[self.snooze_panel.selected_index],
                            &self.snooze_config,
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
                    self.snooze_panel.visible = false;
                } else if self.context_envelope().is_some() {
                    self.snooze_panel.visible = true;
                    self.snooze_panel.selected_index = 0;
                } else {
                    self.status_message = Some("No message selected".into());
                }
            }
            Action::OpenInBrowser => {
                if let Some(env) = self.context_envelope() {
                    let url = format!(
                        "https://mail.google.com/mail/u/0/#inbox/{}",
                        env.provider_id
                    );
                    #[cfg(target_os = "macos")]
                    let _ = std::process::Command::new("open").arg(&url).spawn();
                    #[cfg(target_os = "linux")]
                    let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
                    self.status_message = Some("Opened in browser".into());
                }
            }
            // Batch operations
            Action::ToggleSelect => {
                if let Some(env) = self.context_envelope() {
                    let should_advance = matches!(
                        self.current_ui_context(),
                        UiContext::MailboxList | UiContext::SearchResults
                    );
                    let id = env.id.clone();
                    if self.selected_set.contains(&id) {
                        self.selected_set.remove(&id);
                    } else {
                        self.selected_set.insert(id);
                    }
                    if should_advance && self.screen == Screen::Search {
                        if self.search_page.selected_index + 1 < self.search_row_count() {
                            self.search_page.selected_index += 1;
                            self.ensure_search_visible();
                            self.maybe_load_more_search_results();
                        }
                    } else if should_advance && self.selected_index + 1 < self.mail_row_count() {
                        self.selected_index += 1;
                        self.ensure_visible();
                        self.auto_preview();
                    }
                    let count = self.selected_set.len();
                    self.status_message = Some(format!("{count} selected"));
                }
            }
            Action::VisualLineMode => {
                if self.visual_mode {
                    // Exit visual mode
                    self.visual_mode = false;
                    self.visual_anchor = None;
                    self.status_message = Some("Visual mode off".into());
                } else {
                    self.visual_mode = true;
                    self.visual_anchor = Some(if self.screen == Screen::Search {
                        self.search_page.selected_index
                    } else {
                        self.selected_index
                    });
                    // Add current to selection
                    if let Some(env) = self.context_envelope() {
                        self.selected_set.insert(env.id.clone());
                    }
                    self.status_message = Some("-- VISUAL LINE --".into());
                }
            }
            Action::PatternSelect(pattern) => {
                let envelopes = if self.screen == Screen::Search {
                    &self.search_page.results
                } else {
                    &self.envelopes
                };
                match pattern {
                    PatternKind::All => {
                        self.selected_set = envelopes.iter().map(|e| e.id.clone()).collect();
                    }
                    PatternKind::None => {
                        self.selected_set.clear();
                        self.visual_mode = false;
                        self.visual_anchor = None;
                    }
                    PatternKind::Read => {
                        self.selected_set = envelopes
                            .iter()
                            .filter(|e| e.flags.contains(MessageFlags::READ))
                            .map(|e| e.id.clone())
                            .collect();
                    }
                    PatternKind::Unread => {
                        self.selected_set = envelopes
                            .iter()
                            .filter(|e| !e.flags.contains(MessageFlags::READ))
                            .map(|e| e.id.clone())
                            .collect();
                    }
                    PatternKind::Starred => {
                        self.selected_set = envelopes
                            .iter()
                            .filter(|e| e.flags.contains(MessageFlags::STARRED))
                            .map(|e| e.id.clone())
                            .collect();
                    }
                    PatternKind::Thread => {
                        if let Some(env) = self.context_envelope() {
                            let tid = env.thread_id.clone();
                            self.selected_set = envelopes
                                .iter()
                                .filter(|e| e.thread_id == tid)
                                .map(|e| e.id.clone())
                                .collect();
                        }
                    }
                }
                let count = self.selected_set.len();
                self.status_message = Some(format!("{count} selected"));
            }
            // Attachments, links, fullscreen, export
            Action::AttachmentList => {
                if self.attachment_panel.visible {
                    self.close_attachment_panel();
                } else {
                    self.open_attachment_panel();
                }
            }
            Action::OpenLinks => {
                self.open_url_modal();
            }
            Action::ToggleFullscreen => {
                if self.layout_mode == LayoutMode::FullScreen {
                    self.layout_mode = LayoutMode::ThreePane;
                } else if self.viewing_envelope.is_some() {
                    self.layout_mode = LayoutMode::FullScreen;
                }
            }
            Action::ExportThread => {
                if let Some(env) = self.context_envelope() {
                    self.pending_export_thread = Some(env.thread_id.clone());
                    self.status_message = Some("Exporting thread...".into());
                } else {
                    self.status_message = Some("No message selected".into());
                }
            }
            _ => unreachable!(),
        }
    }
}
