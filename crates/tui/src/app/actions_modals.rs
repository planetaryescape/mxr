use super::*;

impl App {
    pub(super) fn apply_modals(&mut self, action: Action) {
        match action {
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
            // Account management
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
            // Rules
            Action::RefreshRules => {
                self.rules_page.refresh_pending = true;
                self.refresh_selected_rule_panel();
            }
            Action::ToggleRuleEnabled => {
                if let Some(rule) = self.selected_rule().cloned() {
                    let mut updated = rule.clone();
                    if let Some(enabled) = updated.get("enabled").and_then(serde_json::Value::as_bool) {
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
            // Diagnostics
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
            // Reader mode
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
            // Help
            Action::Help => {
                self.help_modal_open = !self.help_modal_open;
                self.help_scroll_offset = 0;
                self.help_query.clear();
                self.help_selected = 0;
            }
            Action::Noop => {}
            _ => unreachable!(),
        }
    }
}
