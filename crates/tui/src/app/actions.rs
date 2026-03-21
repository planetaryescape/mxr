use super::*;

impl App {
    pub fn tick(&mut self) {
        self.input.check_timeout();
    }

    pub fn apply(&mut self, action: Action) {
        // Clear status message on any action
        self.status_message = None;

        match action {
            Action::OpenMailboxScreen => {
                if self.accounts_page.onboarding_required {
                    self.screen = Screen::Accounts;
                    self.accounts_page.onboarding_modal_open =
                        self.accounts_page.accounts.is_empty() && !self.accounts_page.form.visible;
                    return;
                }
                self.screen = Screen::Mailbox;
                self.active_pane = if self.layout_mode == LayoutMode::ThreePane {
                    ActivePane::MailList
                } else {
                    self.active_pane
                };
            }
            Action::OpenSearchScreen => {
                self.screen = Screen::Search;
                self.search_page.editing = true;
                self.search_page.query = self.search_bar.query.clone();
                self.search_page.results = if self.search_page.query.is_empty() {
                    self.all_envelopes.clone()
                } else {
                    self.search_page.results.clone()
                };
                self.search_page.selected_index = 0;
                self.search_page.scroll_offset = 0;
            }
            Action::OpenRulesScreen => {
                self.screen = Screen::Rules;
                self.rules_page.refresh_pending = true;
            }
            Action::OpenDiagnosticsScreen => {
                self.screen = Screen::Diagnostics;
                self.diagnostics_page.refresh_pending = true;
            }
            Action::OpenAccountsScreen => {
                self.screen = Screen::Accounts;
                self.accounts_page.refresh_pending = true;
            }
            Action::RefreshAccounts => {
                self.accounts_page.refresh_pending = true;
            }
            Action::OpenAccountFormNew => {
                self.accounts_page.form = AccountFormState::default();
                self.accounts_page.form.visible = true;
                self.accounts_page.onboarding_modal_open = false;
                self.refresh_account_form_derived_fields();
                self.screen = Screen::Accounts;
            }
            Action::SaveAccountForm => {
                let is_default = self
                    .selected_account()
                    .is_some_and(|account| account.is_default)
                    || self.accounts_page.accounts.is_empty();
                self.accounts_page.last_result = None;
                self.accounts_page.form.last_result = None;
                self.pending_account_save = Some(self.account_form_data(is_default));
                self.accounts_page.status = Some("Saving account...".into());
            }
            Action::TestAccountForm => {
                let account = if self.accounts_page.form.visible {
                    self.account_form_data(false)
                } else if let Some(account) = self.selected_account_config() {
                    account
                } else {
                    self.accounts_page.status = Some("No editable account selected.".into());
                    return;
                };
                self.accounts_page.last_result = None;
                self.accounts_page.form.last_result = None;
                self.pending_account_test = Some(account);
                self.accounts_page.status = Some("Testing account...".into());
            }
            Action::ReauthorizeAccountForm => {
                let account = self.account_form_data(false);
                self.accounts_page.last_result = None;
                self.accounts_page.form.last_result = None;
                self.pending_account_authorize = Some((account, true));
                self.accounts_page.status = Some("Authorizing Gmail account...".into());
            }
            Action::SetDefaultAccount => {
                if let Some(key) = self
                    .selected_account()
                    .and_then(|account| account.key.clone())
                {
                    self.pending_account_set_default = Some(key);
                    self.accounts_page.status = Some("Setting default account...".into());
                } else {
                    self.accounts_page.status =
                        Some("Runtime-only account cannot be set default from TUI.".into());
                }
            }
            Action::MoveDown => {
                if self.screen == Screen::Search {
                    if self.search_page.selected_index + 1 < self.search_row_count() {
                        self.search_page.selected_index += 1;
                        self.ensure_search_visible();
                        self.auto_preview_search();
                    }
                    return;
                }
                if self.selected_index + 1 < self.mail_row_count() {
                    self.selected_index += 1;
                }
                self.ensure_visible();
                self.update_visual_selection();
                self.auto_preview();
            }
            Action::MoveUp => {
                if self.screen == Screen::Search {
                    if self.search_page.selected_index > 0 {
                        self.search_page.selected_index -= 1;
                        self.ensure_search_visible();
                        self.auto_preview_search();
                    }
                    return;
                }
                if self.selected_index > 0 {
                    self.selected_index -= 1;
                }
                self.ensure_visible();
                self.update_visual_selection();
                self.auto_preview();
            }
            Action::JumpTop => {
                self.selected_index = 0;
                self.scroll_offset = 0;
                self.auto_preview();
            }
            Action::JumpBottom => {
                if self.mail_row_count() > 0 {
                    self.selected_index = self.mail_row_count() - 1;
                }
                self.ensure_visible();
                self.auto_preview();
            }
            Action::PageDown => {
                let page = self.visible_height.max(1);
                self.selected_index =
                    (self.selected_index + page).min(self.mail_row_count().saturating_sub(1));
                self.ensure_visible();
                self.auto_preview();
            }
            Action::PageUp => {
                let page = self.visible_height.max(1);
                self.selected_index = self.selected_index.saturating_sub(page);
                self.ensure_visible();
                self.auto_preview();
            }
            Action::ViewportTop => {
                self.selected_index = self.scroll_offset;
                self.auto_preview();
            }
            Action::ViewportMiddle => {
                let visible_height = 20;
                self.selected_index = (self.scroll_offset + visible_height / 2)
                    .min(self.mail_row_count().saturating_sub(1));
                self.auto_preview();
            }
            Action::ViewportBottom => {
                let visible_height = 20;
                self.selected_index = (self.scroll_offset + visible_height)
                    .min(self.mail_row_count().saturating_sub(1));
                self.auto_preview();
            }
            Action::CenterCurrent => {
                let visible_height = 20;
                self.scroll_offset = self.selected_index.saturating_sub(visible_height / 2);
            }
            Action::SwitchPane => {
                self.active_pane = match (self.layout_mode, self.active_pane) {
                    // ThreePane: Sidebar → MailList → MessageView → Sidebar
                    (LayoutMode::ThreePane, ActivePane::Sidebar) => ActivePane::MailList,
                    (LayoutMode::ThreePane, ActivePane::MailList) => ActivePane::MessageView,
                    (LayoutMode::ThreePane, ActivePane::MessageView) => ActivePane::Sidebar,
                    // TwoPane: Sidebar → MailList → Sidebar
                    (_, ActivePane::Sidebar) => ActivePane::MailList,
                    (_, ActivePane::MailList) => ActivePane::Sidebar,
                    (_, ActivePane::MessageView) => ActivePane::Sidebar,
                };
            }
            Action::OpenSelected => {
                if let Some(pending) = self.pending_bulk_confirm.take() {
                    self.pending_mutation_queue
                        .push((pending.request, pending.effect));
                    self.status_message = Some(pending.status_message);
                    self.clear_selection();
                    return;
                }
                if self.screen == Screen::Search {
                    if let Some(env) = self.selected_search_envelope().cloned() {
                        self.open_envelope(env);
                        self.screen = Screen::Mailbox;
                        self.layout_mode = LayoutMode::ThreePane;
                        self.active_pane = ActivePane::MessageView;
                    }
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
            Action::Back => match self.active_pane {
                _ if self.screen != Screen::Mailbox => {
                    self.screen = Screen::Mailbox;
                }
                ActivePane::MessageView => {
                    self.apply(Action::CloseMessageView);
                }
                ActivePane::MailList => {
                    if !self.selected_set.is_empty() {
                        self.apply(Action::ClearSelection);
                    } else if self.search_active {
                        self.apply(Action::CloseSearch);
                    } else if self.active_label.is_some() {
                        self.apply(Action::ClearFilter);
                    } else if self.layout_mode == LayoutMode::ThreePane {
                        self.apply(Action::CloseMessageView);
                    }
                }
                ActivePane::Sidebar => {}
            },
            Action::QuitView => {
                self.should_quit = true;
            }
            Action::ClearSelection => {
                self.clear_selection();
                self.status_message = Some("Selection cleared".into());
            }
            // Search
            Action::OpenSearch => {
                if self.search_active {
                    self.search_bar.activate_existing();
                } else {
                    self.search_bar.activate();
                }
            }
            Action::SubmitSearch => {
                if self.screen == Screen::Search {
                    self.search_page.editing = false;
                    self.pending_search = Some(self.search_page.query.clone());
                } else {
                    let query = self.search_bar.query.clone();
                    self.search_bar.deactivate();
                    if !query.is_empty() {
                        self.pending_search = Some(query);
                        self.search_active = true;
                    }
                    // Return focus to mail list so j/k navigates results
                    self.active_pane = ActivePane::MailList;
                }
            }
            Action::CloseSearch => {
                self.search_bar.deactivate();
                self.search_active = false;
                // Restore full envelope list
                self.envelopes = self.all_mail_envelopes();
                self.selected_index = 0;
                self.scroll_offset = 0;
            }
            Action::NextSearchResult => {
                if self.search_active && self.selected_index + 1 < self.envelopes.len() {
                    self.selected_index += 1;
                    self.ensure_visible();
                    self.auto_preview();
                }
            }
            Action::PrevSearchResult => {
                if self.search_active && self.selected_index > 0 {
                    self.selected_index -= 1;
                    self.ensure_visible();
                    self.auto_preview();
                }
            }
            // Navigation
            Action::GoToInbox => {
                if let Some(label) = self.labels.iter().find(|l| l.name == "INBOX") {
                    self.apply(Action::SelectLabel(label.id.clone()));
                } else {
                    self.desired_system_mailbox = Some("INBOX".into());
                }
            }
            Action::GoToStarred => {
                if let Some(label) = self.labels.iter().find(|l| l.name == "STARRED") {
                    self.apply(Action::SelectLabel(label.id.clone()));
                } else {
                    self.desired_system_mailbox = Some("STARRED".into());
                }
            }
            Action::GoToSent => {
                if let Some(label) = self.labels.iter().find(|l| l.name == "SENT") {
                    self.apply(Action::SelectLabel(label.id.clone()));
                } else {
                    self.desired_system_mailbox = Some("SENT".into());
                }
            }
            Action::GoToDrafts => {
                if let Some(label) = self.labels.iter().find(|l| l.name == "DRAFT") {
                    self.apply(Action::SelectLabel(label.id.clone()));
                } else {
                    self.desired_system_mailbox = Some("DRAFT".into());
                }
            }
            Action::GoToAllMail => {
                self.mailbox_view = MailboxView::Messages;
                self.apply(Action::ClearFilter);
            }
            Action::OpenSubscriptions => {
                self.mailbox_view = MailboxView::Subscriptions;
                self.active_label = None;
                self.pending_active_label = None;
                self.pending_label_fetch = None;
                self.desired_system_mailbox = None;
                self.search_active = false;
                self.screen = Screen::Mailbox;
                self.active_pane = ActivePane::MailList;
                self.selected_index = self
                    .selected_index
                    .min(self.subscriptions_page.entries.len().saturating_sub(1));
                self.scroll_offset = 0;
                if self.subscriptions_page.entries.is_empty() {
                    self.pending_subscriptions_refresh = true;
                }
                self.auto_preview();
            }
            Action::GoToLabel => {
                self.mailbox_view = MailboxView::Messages;
                self.apply(Action::ClearFilter);
            }
            Action::OpenTab1 => {
                self.apply(Action::OpenMailboxScreen);
            }
            Action::OpenTab2 => {
                self.apply(Action::OpenSearchScreen);
            }
            Action::OpenTab3 => {
                self.apply(Action::OpenRulesScreen);
            }
            Action::OpenTab4 => {
                self.apply(Action::OpenAccountsScreen);
            }
            Action::OpenTab5 => {
                self.apply(Action::OpenDiagnosticsScreen);
            }
            // Command palette
            Action::OpenCommandPalette => {
                self.command_palette.toggle();
            }
            Action::CloseCommandPalette => {
                self.command_palette.visible = false;
            }
            // Sync
            Action::SyncNow => {
                self.pending_mutation_queue.push((
                    Request::SyncNow { account_id: None },
                    MutationEffect::RefreshList,
                ));
                self.status_message = Some("Syncing...".into());
            }
            // Message view
            Action::OpenMessageView => {
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
                self.close_attachment_panel();
                self.layout_mode = LayoutMode::TwoPane;
                self.active_pane = ActivePane::MailList;
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
            Action::RefreshRules => {
                self.rules_page.refresh_pending = true;
                if let Some(id) = self.selected_rule().and_then(|rule| rule["id"].as_str()) {
                    self.pending_rule_detail = Some(id.to_string());
                }
            }
            Action::ToggleRuleEnabled => {
                if let Some(rule) = self.selected_rule().cloned() {
                    let mut updated = rule.clone();
                    if let Some(enabled) = updated.get("enabled").and_then(|v| v.as_bool()) {
                        updated["enabled"] = serde_json::Value::Bool(!enabled);
                        self.pending_rule_upsert = Some(updated);
                        self.rules_page.status = Some(if enabled {
                            "Disabling rule...".into()
                        } else {
                            "Enabling rule...".into()
                        });
                    }
                }
            }
            Action::DeleteRule => {
                if let Some(rule_id) = self
                    .selected_rule()
                    .and_then(|rule| rule["id"].as_str())
                    .map(ToString::to_string)
                {
                    self.pending_rule_delete = Some(rule_id.clone());
                    self.rules_page.status = Some(format!("Deleting {rule_id}..."));
                }
            }
            Action::ShowRuleHistory => {
                self.rules_page.panel = RulesPanel::History;
                self.pending_rule_history = self
                    .selected_rule()
                    .and_then(|rule| rule["id"].as_str())
                    .map(ToString::to_string);
            }
            Action::ShowRuleDryRun => {
                self.rules_page.panel = RulesPanel::DryRun;
                self.pending_rule_dry_run = self
                    .selected_rule()
                    .and_then(|rule| rule["id"].as_str())
                    .map(ToString::to_string);
            }
            Action::OpenRuleFormNew => {
                self.rules_page.form = RuleFormState {
                    visible: true,
                    enabled: true,
                    priority: "100".to_string(),
                    active_field: 0,
                    ..RuleFormState::default()
                };
                self.rules_page.panel = RulesPanel::Form;
            }
            Action::OpenRuleFormEdit => {
                if let Some(rule_id) = self
                    .selected_rule()
                    .and_then(|rule| rule["id"].as_str())
                    .map(ToString::to_string)
                {
                    self.pending_rule_form_load = Some(rule_id);
                }
            }
            Action::SaveRuleForm => {
                self.rules_page.status = Some("Saving rule...".into());
                self.pending_rule_form_save = true;
            }
            Action::RefreshDiagnostics => {
                self.diagnostics_page.refresh_pending = true;
            }
            Action::GenerateBugReport => {
                self.diagnostics_page.status = Some("Generating bug report...".into());
                self.pending_bug_report = true;
            }
            Action::SelectLabel(label_id) => {
                self.mailbox_view = MailboxView::Messages;
                self.pending_label_fetch = Some(label_id);
                self.pending_active_label = self.pending_label_fetch.clone();
                self.desired_system_mailbox = None;
                self.active_pane = ActivePane::MailList;
                self.screen = Screen::Mailbox;
            }
            Action::SelectSavedSearch(query) => {
                self.mailbox_view = MailboxView::Messages;
                if self.screen == Screen::Search {
                    self.search_page.query = query.clone();
                    self.search_page.editing = false;
                } else {
                    self.search_active = true;
                    self.active_pane = ActivePane::MailList;
                }
                self.pending_search = Some(query);
            }
            Action::ClearFilter => {
                self.mailbox_view = MailboxView::Messages;
                self.active_label = None;
                self.pending_active_label = None;
                self.desired_system_mailbox = None;
                self.search_active = false;
                self.envelopes = self.all_mail_envelopes();
                self.selected_index = 0;
                self.scroll_offset = 0;
            }

            // Phase 2: Email actions (Gmail-native A005)
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
                        effect,
                        "Archiving...".into(),
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
                        effect,
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
                        effect,
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
                    let first = ids[0].clone();
                    // For single message, provide flag update
                    let effect = if ids.len() == 1 {
                        if let Some(env) = self.context_envelope() {
                            let mut new_flags = env.flags;
                            if starred {
                                new_flags.insert(MessageFlags::STARRED);
                            } else {
                                new_flags.remove(MessageFlags::STARRED);
                            }
                            MutationEffect::UpdateFlags {
                                message_id: first.clone(),
                                flags: new_flags,
                            }
                        } else {
                            MutationEffect::RefreshList
                        }
                    } else {
                        MutationEffect::RefreshList
                    };
                    let verb = if starred { "star" } else { "unstar" };
                    let status = if starred {
                        "Starring..."
                    } else {
                        "Unstarring..."
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
                        effect,
                        status.into(),
                        ids.len(),
                    );
                }
            }
            Action::MarkRead => {
                let ids = self.mutation_target_ids();
                if !ids.is_empty() {
                    let first = ids[0].clone();
                    let effect = if ids.len() == 1 {
                        if let Some(env) = self.context_envelope() {
                            let mut new_flags = env.flags;
                            new_flags.insert(MessageFlags::READ);
                            MutationEffect::UpdateFlags {
                                message_id: first.clone(),
                                flags: new_flags,
                            }
                        } else {
                            MutationEffect::RefreshList
                        }
                    } else {
                        MutationEffect::RefreshList
                    };
                    self.queue_or_confirm_bulk_action(
                        "Mark messages as read",
                        bulk_message_detail("mark as read", ids.len()),
                        Request::Mutation(MutationCommand::SetRead {
                            message_ids: ids.clone(),
                            read: true,
                        }),
                        effect,
                        "Marking as read...".into(),
                        ids.len(),
                    );
                }
            }
            Action::MarkUnread => {
                let ids = self.mutation_target_ids();
                if !ids.is_empty() {
                    let first = ids[0].clone();
                    let effect = if ids.len() == 1 {
                        if let Some(env) = self.context_envelope() {
                            let mut new_flags = env.flags;
                            new_flags.remove(MessageFlags::READ);
                            MutationEffect::UpdateFlags {
                                message_id: first.clone(),
                                flags: new_flags,
                            }
                        } else {
                            MutationEffect::RefreshList
                        }
                    } else {
                        MutationEffect::RefreshList
                    };
                    self.queue_or_confirm_bulk_action(
                        "Mark messages as unread",
                        bulk_message_detail("mark as unread", ids.len()),
                        Request::Mutation(MutationCommand::SetRead {
                            message_ids: ids.clone(),
                            read: false,
                        }),
                        effect,
                        "Marking as unread...".into(),
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
                                status: format!("Applied label '{}'", label_name),
                            },
                            format!("Applying label '{}'...", label_name),
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
                            format!("Moving to '{}'...", label_name),
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
                        self.pending_mutation_queue.push((
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
                        ));
                        self.status_message = Some("Snoozing...".into());
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

            // Phase 2: Reader mode
            Action::ToggleReaderMode => {
                if let BodyViewState::Ready { .. } = self.body_view_state {
                    self.reader_mode = !self.reader_mode;
                    if let Some(env) = self.viewing_envelope.clone() {
                        self.body_view_state = self.resolve_body_view_state(&env);
                    }
                }
            }
            Action::ToggleSignature => {
                self.signature_expanded = !self.signature_expanded;
            }

            // Phase 2: Batch operations (A007)
            Action::ToggleSelect => {
                if let Some(env) = self.selected_envelope() {
                    let id = env.id.clone();
                    if self.selected_set.contains(&id) {
                        self.selected_set.remove(&id);
                    } else {
                        self.selected_set.insert(id);
                    }
                    // Move to next after toggling
                    if self.selected_index + 1 < self.mail_row_count() {
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
                    self.visual_anchor = Some(self.selected_index);
                    // Add current to selection
                    if let Some(env) = self.selected_envelope() {
                        self.selected_set.insert(env.id.clone());
                    }
                    self.status_message = Some("-- VISUAL LINE --".into());
                }
            }
            Action::PatternSelect(pattern) => {
                match pattern {
                    PatternKind::All => {
                        self.selected_set = self.envelopes.iter().map(|e| e.id.clone()).collect();
                    }
                    PatternKind::None => {
                        self.selected_set.clear();
                        self.visual_mode = false;
                        self.visual_anchor = None;
                    }
                    PatternKind::Read => {
                        self.selected_set = self
                            .envelopes
                            .iter()
                            .filter(|e| e.flags.contains(MessageFlags::READ))
                            .map(|e| e.id.clone())
                            .collect();
                    }
                    PatternKind::Unread => {
                        self.selected_set = self
                            .envelopes
                            .iter()
                            .filter(|e| !e.flags.contains(MessageFlags::READ))
                            .map(|e| e.id.clone())
                            .collect();
                    }
                    PatternKind::Starred => {
                        self.selected_set = self
                            .envelopes
                            .iter()
                            .filter(|e| e.flags.contains(MessageFlags::STARRED))
                            .map(|e| e.id.clone())
                            .collect();
                    }
                    PatternKind::Thread => {
                        if let Some(env) = self.context_envelope() {
                            let tid = env.thread_id.clone();
                            self.selected_set = self
                                .envelopes
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

            // Phase 2: Other actions
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
            Action::Help => {
                self.help_modal_open = !self.help_modal_open;
                if self.help_modal_open {
                    self.help_scroll_offset = 0;
                }
            }
            Action::Noop => {}
        }
    }
}
