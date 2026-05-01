use super::*;

impl App {
    pub(super) fn browser_document(body: &MessageBody) -> Option<String> {
        body.text_html
            .clone()
            .or_else(|| {
                body.text_plain
                    .as_deref()
                    .map(render_plain_text_browser_document)
            })
            .or_else(|| {
                body.best_effort_readable_summary()
                    .map(|text| render_plain_text_browser_document(&text))
            })
    }

    pub(super) fn queue_browser_open_for_body(
        &mut self,
        message_id: MessageId,
        body: &MessageBody,
    ) {
        let Some(document) = Self::browser_document(body) else {
            self.status_message = Some("No readable body available".into());
            return;
        };

        self.mailbox.pending_browser_open = Some(PendingBrowserOpen {
            message_id,
            document,
        });
        self.status_message = Some("Opening in browser...".into());
    }

    fn queue_current_message_browser_open(&mut self) {
        let Some(message_id) = self
            .mailbox
            .viewing_envelope
            .as_ref()
            .map(|env| env.id.clone())
        else {
            self.status_message = Some("No message selected".into());
            return;
        };

        let Some(body) = self.current_viewing_body() else {
            self.queue_body_fetch(message_id.clone());
            self.mailbox.pending_browser_open_after_load = Some(message_id);
            self.status_message = Some("Loading message body...".into());
            return;
        };

        let body = body.clone();
        self.queue_browser_open_for_body(message_id, &body);
    }

    pub fn tick(&mut self) {
        self.input.check_timeout();
        if self.search_is_pending() {
            self.search_page.throbber.calc_next();
        }
        if self.mailbox.mailbox_loading_message.is_some() {
            self.mailbox.mailbox_loading_throbber.calc_next();
        }
        if self.accounts_page.operation_in_flight {
            self.accounts_page.throbber.calc_next();
        }
        self.process_pending_search_debounce();
        self.process_pending_preview_read();
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
                self.maybe_preserve_new_account_form_draft();
                self.screen = Screen::Mailbox;
                self.mailbox.active_pane = if self.mailbox.layout_mode == LayoutMode::ThreePane {
                    ActivePane::MailList
                } else {
                    self.mailbox.active_pane
                };
            }
            Action::OpenSearchScreen => {
                self.maybe_preserve_new_account_form_draft();
                self.mailbox.pending_preview_read = None;
                self.screen = Screen::Search;
                if self.search_page.has_session() {
                    self.search_page.editing = false;
                    if !self.search_page.result_selected {
                        self.clear_message_view_state();
                    }
                } else {
                    self.reset_search_page_workspace();
                    self.search_page.editing = true;
                    self.search_page.sort = SortOrder::DateDesc;
                }
            }
            Action::OpenGlobalSearch => {
                self.maybe_preserve_new_account_form_draft();
                self.mailbox.pending_preview_read = None;
                self.search_bar.deactivate();
                self.screen = Screen::Search;
                if !self.search_page.has_session() {
                    self.reset_search_page_workspace();
                }
                self.search_page.editing = true;
                self.search_page.active_pane = SearchPane::Results;
                if !self.search_page.result_selected {
                    self.clear_message_view_state();
                }
            }
            Action::OpenRulesScreen => {
                self.maybe_preserve_new_account_form_draft();
                self.mailbox.pending_preview_read = None;
                self.screen = Screen::Rules;
                self.rules_page.refresh_pending = true;
            }
            Action::OpenDiagnosticsScreen => {
                self.maybe_preserve_new_account_form_draft();
                self.mailbox.pending_preview_read = None;
                self.screen = Screen::Diagnostics;
                self.diagnostics_page.refresh_pending = true;
            }
            Action::OpenAccountsScreen => {
                self.mailbox.pending_preview_read = None;
                self.screen = Screen::Accounts;
                self.accounts_page.refresh_pending = true;
            }
            Action::RefreshAccounts => {
                self.accounts_page.refresh_pending = true;
            }
            Action::OpenAccountFormNew => {
                if self.accounts_page.new_account_draft.is_some() {
                    self.accounts_page.resume_new_account_draft_prompt_open = true;
                    self.accounts_page.onboarding_modal_open = false;
                    self.screen = Screen::Accounts;
                    return;
                }
                self.open_new_account_form();
                self.accounts_page.onboarding_modal_open = false;
                self.screen = Screen::Accounts;
            }
            Action::SaveAccountForm => {
                let is_default = self
                    .selected_account()
                    .is_some_and(|account| account.is_default)
                    || self.accounts_page.accounts.is_empty();
                self.accounts_page.last_result = None;
                self.accounts_page.form.last_result = None;
                if let Some((result, first_invalid_field)) = self.account_form_validation_failure()
                {
                    self.fail_account_form_submission(result, first_invalid_field);
                    return;
                }
                self.accounts_page.operation_in_flight = true;
                self.accounts_page.throbber = ThrobberState::default();
                self.pending_account_save = Some(self.account_form_data(is_default));
                self.accounts_page.status = Some("Saving account...".into());
            }
            Action::TestAccountForm => {
                let account = if self.accounts_page.form.visible {
                    self.accounts_page.last_result = None;
                    self.accounts_page.form.last_result = None;
                    if let Some((result, first_invalid_field)) =
                        self.account_form_validation_failure()
                    {
                        self.fail_account_form_submission(result, first_invalid_field);
                        return;
                    }
                    self.account_form_data(false)
                } else if let Some(account) = self.selected_account_config() {
                    self.accounts_page.last_result = None;
                    self.accounts_page.form.last_result = None;
                    account
                } else {
                    self.accounts_page.status = Some("No editable account selected.".into());
                    return;
                };
                self.accounts_page.operation_in_flight = true;
                self.accounts_page.throbber = ThrobberState::default();
                self.pending_account_test = Some(account);
                self.accounts_page.status = Some("Testing account...".into());
            }
            Action::ReauthorizeAccountForm => {
                self.accounts_page.last_result = None;
                self.accounts_page.form.last_result = None;
                if let Some((result, first_invalid_field)) = self.account_form_validation_failure()
                {
                    self.fail_account_form_submission(result, first_invalid_field);
                    return;
                }
                let account = self.account_form_data(false);
                self.accounts_page.operation_in_flight = true;
                self.accounts_page.throbber = ThrobberState::default();
                self.pending_account_authorize = Some((account, true));
                self.accounts_page.status = Some("Authorizing Gmail account...".into());
            }
            Action::SetDefaultAccount => {
                if let Some(key) = self
                    .selected_account()
                    .and_then(|account| account.key.clone())
                {
                    self.accounts_page.operation_in_flight = true;
                    self.accounts_page.throbber = ThrobberState::default();
                    self.pending_account_set_default = Some(key);
                    self.accounts_page.status = Some("Setting default account...".into());
                } else {
                    self.accounts_page.status =
                        Some("Runtime-only account cannot be set default from TUI.".into());
                }
            }
            Action::SwitchAccount(key) => {
                self.pending_account_set_default = Some(key);
                self.pending_account_switch = true;
                self.status_message = Some("Switching account...".into());
            }
            Action::MoveDown => {
                if self.screen == Screen::Search {
                    if self.search_page.selected_index + 1 < self.search_row_count() {
                        self.search_page.selected_index += 1;
                    }
                    self.sync_search_cursor_after_move();
                    return;
                }
                if self.mailbox.selected_index + 1 < self.mail_row_count() {
                    self.mailbox.selected_index += 1;
                }
                self.ensure_visible();
                self.update_visual_selection();
                self.auto_preview();
            }
            Action::MoveUp => {
                if self.screen == Screen::Search {
                    if self.search_page.selected_index > 0 {
                        self.search_page.selected_index -= 1;
                    }
                    self.sync_search_cursor_after_move();
                    return;
                }
                if self.mailbox.selected_index > 0 {
                    self.mailbox.selected_index -= 1;
                }
                self.ensure_visible();
                self.update_visual_selection();
                self.auto_preview();
            }
            Action::JumpTop => {
                if self.screen == Screen::Search {
                    self.search_page.selected_index = 0;
                    self.sync_search_cursor_after_move();
                    return;
                }
                self.mailbox.selected_index = 0;
                self.mailbox.scroll_offset = 0;
                self.auto_preview();
            }
            Action::JumpBottom => {
                if self.screen == Screen::Search {
                    if self.search_page.has_more {
                        self.search_page.load_to_end = true;
                        self.load_more_search_results();
                    } else if self.search_row_count() > 0 {
                        self.search_page.selected_index = self.search_row_count() - 1;
                        self.sync_search_cursor_after_move();
                    }
                    return;
                }
                if self.mail_row_count() > 0 {
                    self.mailbox.selected_index = self.mail_row_count() - 1;
                }
                self.ensure_visible();
                self.auto_preview();
            }
            Action::PageDown => {
                if self.screen == Screen::Search {
                    let page = self.visible_height.max(1);
                    self.search_page.selected_index = (self.search_page.selected_index + page)
                        .min(self.search_row_count().saturating_sub(1));
                    self.sync_search_cursor_after_move();
                    return;
                }
                let page = self.visible_height.max(1);
                self.mailbox.selected_index = (self.mailbox.selected_index + page)
                    .min(self.mail_row_count().saturating_sub(1));
                self.ensure_visible();
                self.auto_preview();
            }
            Action::PageUp => {
                if self.screen == Screen::Search {
                    let page = self.visible_height.max(1);
                    self.search_page.selected_index =
                        self.search_page.selected_index.saturating_sub(page);
                    self.sync_search_cursor_after_move();
                    return;
                }
                let page = self.visible_height.max(1);
                self.mailbox.selected_index = self.mailbox.selected_index.saturating_sub(page);
                self.ensure_visible();
                self.auto_preview();
            }
            Action::ViewportTop => {
                self.mailbox.selected_index = self.mailbox.scroll_offset;
                self.auto_preview();
            }
            Action::ViewportMiddle => {
                let visible_height = 20;
                self.mailbox.selected_index = (self.mailbox.scroll_offset + visible_height / 2)
                    .min(self.mail_row_count().saturating_sub(1));
                self.auto_preview();
            }
            Action::ViewportBottom => {
                let visible_height = 20;
                self.mailbox.selected_index = (self.mailbox.scroll_offset + visible_height)
                    .min(self.mail_row_count().saturating_sub(1));
                self.auto_preview();
            }
            Action::CenterCurrent => {
                let visible_height = 20;
                self.mailbox.scroll_offset = self
                    .mailbox
                    .selected_index
                    .saturating_sub(visible_height / 2);
            }
            Action::SwitchPane => {
                if self.screen == Screen::Search {
                    self.search_page.active_pane = match self.search_page.active_pane {
                        SearchPane::Results => {
                            self.maybe_open_search_preview();
                            self.search_page.active_pane
                        }
                        SearchPane::Preview => SearchPane::Results,
                    };
                    return;
                }
                self.mailbox.active_pane =
                    match (self.mailbox.layout_mode, self.mailbox.active_pane) {
                        // ThreePane: Sidebar → MailList → MessageView → Sidebar
                        (LayoutMode::ThreePane, ActivePane::Sidebar) => ActivePane::MailList,
                        (LayoutMode::ThreePane, ActivePane::MailList) => ActivePane::MessageView,
                        (LayoutMode::ThreePane, ActivePane::MessageView) => ActivePane::Sidebar,
                        // FullScreen: Sidebar → MessageView → Sidebar
                        (LayoutMode::FullScreen, ActivePane::Sidebar) => ActivePane::MessageView,
                        (LayoutMode::FullScreen, ActivePane::MessageView) => ActivePane::Sidebar,
                        // TwoPane: Sidebar → MailList → Sidebar
                        (_, ActivePane::Sidebar) => ActivePane::MailList,
                        (_, ActivePane::MailList) => ActivePane::Sidebar,
                        (_, ActivePane::MessageView) => ActivePane::Sidebar,
                    };
            }
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
                if self.mailbox.mailbox_view == MailboxView::Subscriptions {
                    if let Some(entry) = self.selected_subscription_entry().cloned() {
                        self.open_envelope(entry.envelope);
                        self.mailbox.layout_mode = LayoutMode::ThreePane;
                        self.mailbox.active_pane = ActivePane::MessageView;
                    }
                    return;
                }
                if let Some(row) = self.selected_mail_row() {
                    self.open_envelope(row.representative);
                    self.mailbox.layout_mode = LayoutMode::ThreePane;
                    self.mailbox.active_pane = ActivePane::MessageView;
                }
            }
            Action::Back => match self.mailbox.active_pane {
                _ if self.screen != Screen::Mailbox => {
                    self.screen = Screen::Mailbox;
                }
                ActivePane::MessageView => {
                    self.apply(Action::CloseMessageView);
                }
                ActivePane::MailList => {
                    if !self.mailbox.selected_set.is_empty() {
                        self.apply(Action::ClearSelection);
                    } else if self.search_active {
                        self.apply(Action::CloseSearch);
                    } else if self.mailbox.active_label.is_some() {
                        self.apply(Action::ClearFilter);
                    } else if self.mailbox.layout_mode == LayoutMode::ThreePane {
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
            Action::OpenMailboxFilter => {
                if self.search_active {
                    self.search_bar.activate_existing();
                } else {
                    self.search_bar.activate();
                }
            }
            Action::SubmitSearch => {
                if self.screen == Screen::Search {
                    self.search_page.editing = false;
                    self.search_bar.query = self.search_page.query.clone();
                    self.execute_search_page_search();
                } else {
                    self.search_bar.deactivate();
                    if !self.search_bar.query.is_empty() {
                        self.search_active = true;
                        self.trigger_live_search();
                    }
                    // Return focus to mail list so j/k navigates results
                    self.mailbox.active_pane = ActivePane::MailList;
                }
            }
            Action::CycleSearchMode => {
                self.search_bar.cycle_mode();
                if self.screen == Screen::Search {
                    self.search_page.mode = self.search_bar.mode;
                }
                if self.screen == Screen::Search || self.search_bar.active {
                    self.trigger_live_search();
                }
            }
            Action::CloseSearch => {
                self.search_bar.deactivate();
                self.search_active = false;
                Self::bump_search_session_id(&mut self.mailbox_search_session_id);
                // Restore full envelope list
                self.mailbox.envelopes = self.all_mail_envelopes();
                self.mailbox.selected_index = 0;
                self.mailbox.scroll_offset = 0;
            }
            Action::NextSearchResult => {
                if self.search_active
                    && self.mailbox.selected_index + 1 < self.mailbox.envelopes.len()
                {
                    self.mailbox.selected_index += 1;
                    self.ensure_visible();
                    self.auto_preview();
                }
            }
            Action::PrevSearchResult => {
                if self.search_active && self.mailbox.selected_index > 0 {
                    self.mailbox.selected_index -= 1;
                    self.ensure_visible();
                    self.auto_preview();
                }
            }
            // Navigation
            Action::GoToInbox => {
                if let Some(label) = self.mailbox.labels.iter().find(|l| l.name == "INBOX") {
                    self.apply(Action::SelectLabel(label.id.clone()));
                } else {
                    self.mailbox.desired_system_mailbox = Some("INBOX".into());
                }
            }
            Action::GoToStarred => {
                if let Some(label) = self.mailbox.labels.iter().find(|l| l.name == "STARRED") {
                    self.apply(Action::SelectLabel(label.id.clone()));
                } else {
                    self.mailbox.desired_system_mailbox = Some("STARRED".into());
                }
            }
            Action::GoToSent => {
                if let Some(label) = self.mailbox.labels.iter().find(|l| l.name == "SENT") {
                    self.apply(Action::SelectLabel(label.id.clone()));
                } else {
                    self.mailbox.desired_system_mailbox = Some("SENT".into());
                }
            }
            Action::GoToDrafts => {
                if let Some(label) = self.mailbox.labels.iter().find(|l| l.name == "DRAFT") {
                    self.apply(Action::SelectLabel(label.id.clone()));
                } else {
                    self.mailbox.desired_system_mailbox = Some("DRAFT".into());
                }
            }
            Action::GoToAllMail => {
                self.mailbox.mailbox_view = MailboxView::Messages;
                self.apply(Action::ClearFilter);
            }
            Action::OpenSubscriptions => {
                self.mailbox.mailbox_view = MailboxView::Subscriptions;
                self.mailbox.active_label = None;
                self.mailbox.pending_active_label = None;
                self.mailbox.pending_label_fetch = None;
                self.mailbox.pending_preview_read = None;
                self.mailbox.desired_system_mailbox = None;
                self.search_active = false;
                self.screen = Screen::Mailbox;
                self.mailbox.active_pane = ActivePane::MailList;
                self.mailbox.selected_index = self.mailbox.selected_index.min(
                    self.mailbox
                        .subscriptions_page
                        .entries
                        .len()
                        .saturating_sub(1),
                );
                self.mailbox.scroll_offset = 0;
                if self.mailbox.subscriptions_page.entries.is_empty() {
                    self.mailbox.pending_subscriptions_refresh = true;
                }
                self.auto_preview();
            }
            Action::GoToLabel => {
                self.mailbox.mailbox_view = MailboxView::Messages;
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
                self.command_palette.toggle(self.current_ui_context());
            }
            Action::CloseCommandPalette => {
                self.command_palette.visible = false;
            }
            // Sync
            Action::SyncNow => {
                self.queue_mutation(
                    Request::SyncNow { account_id: None },
                    MutationEffect::RefreshList,
                    "Syncing...".into(),
                );
            }
            // Message view
            Action::OpenMessageView => {
                if self.screen == Screen::Search {
                    self.open_selected_search_result();
                    return;
                }
                if self.mailbox.mailbox_view == MailboxView::Subscriptions {
                    if let Some(entry) = self.selected_subscription_entry().cloned() {
                        self.open_envelope(entry.envelope);
                        self.mailbox.layout_mode = LayoutMode::ThreePane;
                    }
                } else if let Some(row) = self.selected_mail_row() {
                    self.open_envelope(row.representative);
                    self.mailbox.layout_mode = LayoutMode::ThreePane;
                }
            }
            Action::CloseMessageView => {
                if self.screen == Screen::Search {
                    self.reset_search_preview_selection();
                    return;
                }
                self.close_attachment_panel();
                self.mailbox.layout_mode = LayoutMode::TwoPane;
                self.mailbox.active_pane = ActivePane::MailList;
                self.mailbox.pending_preview_read = None;
                self.mailbox.viewing_envelope = None;
                self.mailbox.viewed_thread = None;
                self.mailbox.viewed_thread_messages.clear();
                self.mailbox.thread_selected_index = 0;
                self.mailbox.pending_thread_fetch = None;
                self.mailbox.in_flight_thread_fetch = None;
                self.mailbox.message_scroll_offset = 0;
                self.mailbox.body_view_state = BodyViewState::Empty { preview: None };
            }
            Action::ToggleMailListMode => {
                if self.mailbox.mailbox_view == MailboxView::Subscriptions {
                    return;
                }
                let search_row_message_id = (self.screen == Screen::Search)
                    .then(|| self.selected_search_envelope().map(|env| env.id.clone()))
                    .flatten();
                self.mailbox.mail_list_mode = match self.mailbox.mail_list_mode {
                    MailListMode::Threads => MailListMode::Messages,
                    MailListMode::Messages => MailListMode::Threads,
                };
                if self.screen == Screen::Search {
                    self.search_page.selected_index = search_row_message_id
                        .as_ref()
                        .and_then(|message_id| self.search_row_index_for_message(message_id))
                        .unwrap_or(0)
                        .min(self.search_row_count().saturating_sub(1));
                    if self.search_page.result_selected {
                        self.sync_search_cursor_after_move();
                    } else if self.search_row_count() > 0 {
                        self.ensure_search_visible();
                    }
                } else {
                    self.mailbox.selected_index = self
                        .mailbox
                        .selected_index
                        .min(self.mail_row_count().saturating_sub(1));
                }
            }
            Action::RefreshRules => {
                self.rules_page.refresh_pending = true;
                self.refresh_selected_rule_panel();
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
                self.refresh_selected_rule_panel();
            }
            Action::ShowRuleDryRun => {
                self.rules_page.panel = RulesPanel::DryRun;
                self.refresh_selected_rule_panel();
            }
            Action::OpenRuleFormNew => {
                self.rules_page.form = RuleFormState {
                    visible: true,
                    enabled: true,
                    priority: "100".to_string(),
                    active_field: 0,
                    ..RuleFormState::default()
                };
                self.sync_rule_form_editors();
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
                self.sync_rule_form_strings_from_editors();
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
            Action::EditConfig => {
                self.pending_config_edit = true;
                self.status_message = Some("Opening config in editor...".into());
            }
            Action::OpenLogs => {
                self.pending_log_open = true;
                self.status_message = Some("Opening log file in editor...".into());
            }
            Action::ShowOnboarding => {
                self.onboarding.visible = true;
                self.onboarding.step = 0;
            }
            Action::OpenDiagnosticsPaneDetails => {
                self.pending_diagnostics_details = Some(self.diagnostics_page.active_pane());
                self.status_message = Some("Opening diagnostics details...".into());
            }
            Action::SelectLabel(label_id) => {
                self.mailbox.mailbox_view = MailboxView::Messages;
                self.mailbox.pending_label_fetch = Some(label_id);
                self.mailbox.pending_active_label = self.mailbox.pending_label_fetch.clone();
                self.mailbox.desired_system_mailbox = None;
                self.mailbox.active_pane = ActivePane::MailList;
                self.screen = Screen::Mailbox;
            }
            Action::SelectSavedSearch(query, mode) => {
                self.mailbox.mailbox_view = MailboxView::Messages;
                if self.screen == Screen::Search {
                    self.search_page.query = query.clone();
                    self.search_page.editing = false;
                    self.search_page.mode = mode;
                    self.search_page.sort = SortOrder::DateDesc;
                    self.search_page.active_pane = SearchPane::Results;
                    self.search_bar.query = query.clone();
                    self.search_bar.mode = mode;
                    self.trigger_live_search();
                } else {
                    self.search_active = true;
                    self.mailbox.active_pane = ActivePane::MailList;
                    self.search_bar.query = query.clone();
                    self.search_bar.mode = mode;
                    self.trigger_live_search();
                }
            }
            Action::ClearFilter => {
                self.mailbox.mailbox_view = MailboxView::Messages;
                self.mailbox.active_label = None;
                self.mailbox.pending_active_label = None;
                self.mailbox.pending_preview_read = None;
                self.mailbox.desired_system_mailbox = None;
                self.search_active = false;
                self.mailbox.envelopes = self.all_mail_envelopes();
                self.mailbox.selected_index = 0;
                self.mailbox.scroll_offset = 0;
            }

            // Phase 2: Email actions (Gmail-native A005)
            Action::Compose => {
                // Build contacts from known envelopes (senders we've seen)
                let mut seen = std::collections::HashMap::new();
                for env in &self.mailbox.all_envelopes {
                    seen.entry(env.from.email.clone()).or_insert_with(|| {
                        crate::ui::compose_picker::Contact {
                            name: env.from.name.clone().unwrap_or_default(),
                            email: env.from.email.clone(),
                        }
                    });
                }
                let mut contacts: Vec<_> = seen.into_values().collect();
                contacts.sort_by(|a, b| a.email.to_lowercase().cmp(&b.email.to_lowercase()));
                self.compose_picker.open_to(contacts);
            }
            Action::Reply => {
                if let Some(env) = self.context_envelope() {
                    self.pending_compose = Some(ComposeAction::Reply {
                        message_id: env.id.clone(),
                        account_id: env.account_id.clone(),
                    });
                }
            }
            Action::ReplyAll => {
                if let Some(env) = self.context_envelope() {
                    self.pending_compose = Some(ComposeAction::ReplyAll {
                        message_id: env.id.clone(),
                        account_id: env.account_id.clone(),
                    });
                }
            }
            Action::Forward => {
                if let Some(env) = self.context_envelope() {
                    self.pending_compose = Some(ComposeAction::Forward {
                        message_id: env.id.clone(),
                        account_id: env.account_id.clone(),
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
                                status: format!("Applied label '{}'", label_name),
                            },
                            None,
                            format!("Applying label '{}'...", label_name),
                            ids.len(),
                        );
                    }
                } else {
                    // Open label picker
                    self.label_picker
                        .open(self.mailbox.labels.clone(), LabelPickerMode::Apply);
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
                            format!("Moving to '{}'...", label_name),
                            ids.len(),
                        );
                    }
                } else {
                    // Open label picker
                    self.label_picker
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
                self.queue_current_message_browser_open();
            }

            // Phase 2: Reader mode
            Action::ToggleReaderMode => {
                if self.mailbox.html_view {
                    self.status_message = Some("Switch to text view to use reading view".into());
                } else if let BodyViewState::Ready { .. } = self.mailbox.body_view_state {
                    self.mailbox.reader_mode = !self.mailbox.reader_mode;
                    if let Some(env) = self.mailbox.viewing_envelope.clone() {
                        self.mailbox.body_view_state = self.resolve_body_view_state(&env);
                    }
                    self.status_message = self.current_body_mode_status_message();
                }
            }
            Action::ToggleHtmlView => {
                self.mailbox.html_view = !self.mailbox.html_view;
                if self.mailbox.html_view {
                    self.queue_html_assets_for_current_view();
                }
                if let Some(env) = self.mailbox.viewing_envelope.clone() {
                    self.mailbox.body_view_state = self.resolve_body_view_state(&env);
                }
                self.status_message = self.current_body_mode_status_message();
            }
            Action::ToggleRemoteContent => {
                self.mailbox.remote_content_enabled = !self.mailbox.remote_content_enabled;
                self.invalidate_html_assets_for_current_view();
                self.queue_html_assets_for_current_view();
                if let Some(env) = self.mailbox.viewing_envelope.clone() {
                    self.mailbox.body_view_state = self.resolve_body_view_state(&env);
                }
                self.status_message = Some(if self.mailbox.remote_content_enabled {
                    "Remote images shown in HTML view".into()
                } else {
                    "Remote images blocked in HTML view".into()
                });
            }
            Action::ToggleSignature => {
                self.mailbox.signature_expanded = !self.mailbox.signature_expanded;
            }

            // Phase 2: Batch operations (A007)
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
                        if self.search_page.selected_index + 1 < self.search_row_count() {
                            self.search_page.selected_index += 1;
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
                        self.search_page.selected_index
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
                    &self.search_page.results
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
            Action::AttachmentList => {
                if self.mailbox.attachment_panel.visible {
                    self.close_attachment_panel();
                } else {
                    self.open_attachment_panel();
                }
            }
            Action::OpenLinks => {
                self.open_url_modal();
            }
            Action::ToggleFullscreen => {
                if self.screen == Screen::Search {
                    if self.search_page.preview_fullscreen
                        && self.search_page.active_pane == SearchPane::Preview
                    {
                        self.search_page.preview_fullscreen = false;
                        self.status_message = Some("Showing split view".into());
                    } else if self.search_page.result_selected
                        || self.selected_search_envelope().is_some()
                    {
                        if !self.search_page.result_selected {
                            self.open_selected_search_result();
                        }
                        if self.search_page.result_selected {
                            self.search_page.preview_fullscreen = true;
                            self.search_page.active_pane = SearchPane::Preview;
                            self.status_message = Some("Showing full message view".into());
                        }
                    }
                } else if self.mailbox.layout_mode == LayoutMode::FullScreen {
                    self.mailbox.layout_mode = LayoutMode::ThreePane;
                    self.status_message = Some("Showing split view".into());
                } else if self.mailbox.viewing_envelope.is_some() {
                    self.mailbox.layout_mode = LayoutMode::FullScreen;
                    self.status_message = Some("Showing full message view".into());
                } else if self.screen == Screen::Mailbox {
                    match self.mailbox.mailbox_view {
                        MailboxView::Subscriptions => {
                            if let Some(entry) = self.selected_subscription_entry().cloned() {
                                self.open_envelope(entry.envelope);
                                self.mailbox.layout_mode = LayoutMode::FullScreen;
                                self.mailbox.active_pane = ActivePane::MessageView;
                                self.status_message = Some("Showing full message view".into());
                            }
                        }
                        MailboxView::Messages => {
                            if let Some(row) = self.selected_mail_row() {
                                self.open_envelope(row.representative);
                                self.mailbox.layout_mode = LayoutMode::FullScreen;
                                self.mailbox.active_pane = ActivePane::MessageView;
                                self.status_message = Some("Showing full message view".into());
                            }
                        }
                    }
                }
            }
            Action::ExportThread => {
                if let Some(env) = self.context_envelope() {
                    self.mailbox.pending_export_thread = Some(env.thread_id.clone());
                    self.status_message = Some("Exporting thread...".into());
                } else {
                    self.status_message = Some("No message selected".into());
                }
            }
            Action::Help => {
                self.help_modal_open = !self.help_modal_open;
                self.help_scroll_offset = 0;
                self.help_query.clear();
                self.help_selected = 0;
            }
            Action::Noop => {}
        }
    }
}

fn render_plain_text_browser_document(text: &str) -> String {
    let escaped = htmlescape::encode_minimal(text);
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>mxr message</title><style>body{{margin:2rem;font:16px/1.5 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;background:#fafafa;color:#111;}}pre{{white-space:pre-wrap;word-break:break-word;}}</style></head><body><pre>{escaped}</pre></body></html>"
    )
}
