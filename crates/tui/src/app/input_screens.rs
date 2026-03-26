use super::*;

impl App {
    pub(super) fn handle_screen_key(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> Option<Action> {
        match self.screen {
            Screen::Search => self.handle_search_screen_key(key),
            Screen::Rules => self.handle_rules_screen_key(key),
            Screen::Diagnostics => self.handle_diagnostics_screen_key(key),
            Screen::Accounts => self.handle_accounts_screen_key(key),
            Screen::Mailbox => None,
        }
    }

    fn handle_search_screen_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        if self.search_page.editing {
            return match (key.code, key.modifiers) {
                (KeyCode::Enter, _) => Some(Action::SubmitSearch),
                (KeyCode::Esc, _) => {
                    self.search_page.editing = false;
                    None
                }
                (KeyCode::Backspace, _) => {
                    self.search_page.query.pop();
                    self.trigger_live_search();
                    None
                }
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    self.search_page.query.push(c);
                    self.trigger_live_search();
                    None
                }
                _ => None,
            };
        }

        match self.search_page.active_pane {
            SearchPane::Results => match (key.code, key.modifiers) {
                (KeyCode::Char('/'), _) => {
                    self.search_page.editing = true;
                    None
                }
                (KeyCode::Char('j') | KeyCode::Down, _) => {
                    if self.search_page.selected_index + 1 < self.search_row_count() {
                        self.search_page.selected_index += 1;
                        self.ensure_search_visible();
                    }
                    self.maybe_load_more_search_results();
                    None
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    if self.search_page.selected_index > 0 {
                        self.search_page.selected_index -= 1;
                        self.ensure_search_visible();
                    }
                    None
                }
                (KeyCode::Char('l') | KeyCode::Right, KeyModifiers::NONE)
                | (KeyCode::Enter | KeyCode::Char('o'), _) => Some(Action::OpenSelected),
                (KeyCode::Esc, _) => Some(Action::OpenMailboxScreen),
                _ => self.contextual_input_action(key),
            },
            SearchPane::Preview => match (key.code, key.modifiers) {
                (KeyCode::Char('/'), _) => {
                    self.search_page.editing = true;
                    None
                }
                (KeyCode::Char('h') | KeyCode::Left, KeyModifiers::NONE) => {
                    self.search_page.active_pane = SearchPane::Results;
                    None
                }
                (KeyCode::Char('j') | KeyCode::Down, _) => {
                    self.move_message_view_down();
                    None
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    self.move_message_view_up();
                    None
                }
                (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                    self.message_scroll_offset = self.message_scroll_offset.saturating_add(20);
                    None
                }
                (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                    self.message_scroll_offset = self.message_scroll_offset.saturating_sub(20);
                    None
                }
                (KeyCode::Char('G'), KeyModifiers::SHIFT) => {
                    self.message_scroll_offset = u16::MAX;
                    None
                }
                (KeyCode::Esc, _) => {
                    self.reset_search_preview_selection();
                    None
                }
                _ if self.mail_action_key(key).is_some() => self.mail_action_key(key),
                _ => self.contextual_input_action(key),
            },
        }
    }

    fn handle_rules_screen_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        if self.rules_page.form.visible {
            return self.handle_rule_form_key(key);
        }

        match (key.code, key.modifiers) {
            (KeyCode::Char('j') | KeyCode::Down, _) => {
                if self.rules_page.selected_index + 1 < self.rules_page.rules.len() {
                    self.rules_page.selected_index += 1;
                    self.refresh_selected_rule_panel();
                }
                None
            }
            (KeyCode::Char('k') | KeyCode::Up, _) => {
                self.rules_page.selected_index = self.rules_page.selected_index.saturating_sub(1);
                self.refresh_selected_rule_panel();
                None
            }
            (KeyCode::Enter | KeyCode::Char('o'), _) => Some(Action::RefreshRules),
            (KeyCode::Char('e'), _) => Some(Action::ToggleRuleEnabled),
            (KeyCode::Char('D'), KeyModifiers::SHIFT) => Some(Action::ShowRuleDryRun),
            (KeyCode::Char('H'), KeyModifiers::SHIFT) => Some(Action::ShowRuleHistory),
            (KeyCode::Char('#'), _) => Some(Action::DeleteRule),
            (KeyCode::Char('n'), _) => Some(Action::OpenRuleFormNew),
            (KeyCode::Char('E'), KeyModifiers::SHIFT) => Some(Action::OpenRuleFormEdit),
            (KeyCode::Esc, _) => Some(Action::OpenMailboxScreen),
            _ => self.contextual_input_action(key),
        }
    }

    fn handle_rule_form_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => {
                self.rules_page.form.visible = false;
                self.rules_page.panel = RulesPanel::Details;
                None
            }
            (KeyCode::Tab, _) => {
                self.rules_page.form.active_field = (self.rules_page.form.active_field + 1) % 5;
                None
            }
            (KeyCode::BackTab, _) => {
                self.rules_page.form.active_field = if self.rules_page.form.active_field == 0 {
                    4
                } else {
                    self.rules_page.form.active_field - 1
                };
                None
            }
            (KeyCode::Char(' '), _) if self.rules_page.form.active_field == 4 => {
                self.rules_page.form.enabled = !self.rules_page.form.enabled;
                None
            }
            (KeyCode::Char('s'), KeyModifiers::CONTROL) => Some(Action::SaveRuleForm),
            (_, _) if self.rules_page.form.active_field == 1 => {
                self.rule_condition_editor.input(key);
                self.sync_rule_form_strings_from_editors();
                None
            }
            (_, _) if self.rules_page.form.active_field == 2 => {
                self.rule_action_editor.input(key);
                self.sync_rule_form_strings_from_editors();
                None
            }
            (KeyCode::Backspace, _) => {
                match self.rules_page.form.active_field {
                    0 => {
                        self.rules_page.form.name.pop();
                    }
                    3 => {
                        self.rules_page.form.priority.pop();
                    }
                    _ => {}
                }
                None
            }
            (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                match self.rules_page.form.active_field {
                    0 => self.rules_page.form.name.push(c),
                    3 => self.rules_page.form.priority.push(c),
                    _ => {}
                }
                None
            }
            (KeyCode::Enter, _) => Some(Action::SaveRuleForm),
            _ => None,
        }
    }

    fn handle_diagnostics_screen_key(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> Option<Action> {
        match (key.code, key.modifiers) {
            (KeyCode::Tab | KeyCode::Right, _) => {
                self.diagnostics_page.selected_pane = self.diagnostics_page.selected_pane.next();
                None
            }
            (KeyCode::BackTab | KeyCode::Left, _) => {
                self.diagnostics_page.selected_pane = self.diagnostics_page.selected_pane.prev();
                None
            }
            (KeyCode::Char('j') | KeyCode::Down, _) => {
                self.diagnostics_page.selected_pane = self.diagnostics_page.selected_pane.next();
                None
            }
            (KeyCode::Char('k') | KeyCode::Up, _) => {
                self.diagnostics_page.selected_pane = self.diagnostics_page.selected_pane.prev();
                None
            }
            (KeyCode::Char('d'), KeyModifiers::CONTROL) | (KeyCode::PageDown, _) => {
                let pane = self.diagnostics_page.active_pane();
                *self.diagnostics_page.scroll_offset_mut(pane) =
                    self.diagnostics_page.scroll_offset(pane).saturating_add(8);
                None
            }
            (KeyCode::Char('u'), KeyModifiers::CONTROL) | (KeyCode::PageUp, _) => {
                let pane = self.diagnostics_page.active_pane();
                *self.diagnostics_page.scroll_offset_mut(pane) =
                    self.diagnostics_page.scroll_offset(pane).saturating_sub(8);
                None
            }
            (KeyCode::Enter | KeyCode::Char('o'), _) => {
                self.diagnostics_page.toggle_fullscreen();
                None
            }
            (KeyCode::Char('d'), _) => Some(Action::OpenDiagnosticsPaneDetails),
            (KeyCode::Char('r'), _) => Some(Action::RefreshDiagnostics),
            (KeyCode::Char('b'), _) => Some(Action::GenerateBugReport),
            (KeyCode::Char('c'), _) => Some(Action::EditConfig),
            (KeyCode::Char('L'), KeyModifiers::SHIFT) => Some(Action::OpenLogs),
            (KeyCode::Esc, _) => Some(Action::OpenMailboxScreen),
            _ => self.contextual_input_action(key),
        }
    }

    fn handle_accounts_screen_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        if self.accounts_page.resume_new_account_draft_prompt_open {
            return match (key.code, key.modifiers) {
                (KeyCode::Enter | KeyCode::Char('c'), _) => {
                    self.restore_new_account_form_draft();
                    None
                }
                (KeyCode::Char('n'), _) => {
                    self.accounts_page.new_account_draft = None;
                    self.open_new_account_form();
                    None
                }
                (KeyCode::Esc, _) => {
                    self.accounts_page.resume_new_account_draft_prompt_open = false;
                    None
                }
                _ => None,
            };
        }

        if self.accounts_page.form.visible {
            return self.handle_account_form_key(key);
        }

        match (key.code, key.modifiers) {
            (KeyCode::Char('j') | KeyCode::Down, _) => {
                if self.accounts_page.selected_index + 1 < self.accounts_page.accounts.len() {
                    self.accounts_page.selected_index += 1;
                }
                None
            }
            (KeyCode::Char('k') | KeyCode::Up, _) => {
                self.accounts_page.selected_index =
                    self.accounts_page.selected_index.saturating_sub(1);
                None
            }
            (KeyCode::Char('n'), _) => Some(Action::OpenAccountFormNew),
            (KeyCode::Char('r'), _) => Some(Action::RefreshAccounts),
            (KeyCode::Char('t'), _) => Some(Action::TestAccountForm),
            (KeyCode::Char('O'), KeyModifiers::SHIFT)
                if super::account_result_has_details(self.accounts_page.last_result.as_ref()) =>
            {
                self.open_last_account_result_details_modal();
                None
            }
            (KeyCode::Char('d'), _) => Some(Action::SetDefaultAccount),
            (KeyCode::Char('c'), _) => Some(Action::EditConfig),
            (KeyCode::Enter | KeyCode::Char('o'), _) => {
                if let Some(account) = self.selected_account().cloned() {
                    if let Some(config) = account_summary_to_config(&account) {
                        self.accounts_page.form = account_form_from_config(config);
                        self.accounts_page.form.visible = true;
                    } else {
                        self.accounts_page.status = Some(
                            "Runtime-only account is inspectable but not editable here.".into(),
                        );
                    }
                }
                None
            }
            (KeyCode::Esc, _) if self.accounts_page.onboarding_required => None,
            (KeyCode::Esc, _) => Some(Action::OpenMailboxScreen),
            _ => self.contextual_input_action(key),
        }
    }

    fn handle_account_form_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        if self.accounts_page.form.pending_mode_switch.is_some() {
            return match (key.code, key.modifiers) {
                (KeyCode::Enter | KeyCode::Char('y'), _) => {
                    if let Some(mode) = self.accounts_page.form.pending_mode_switch {
                        self.apply_account_form_mode(mode);
                    }
                    None
                }
                (KeyCode::Esc | KeyCode::Char('n'), _) => {
                    self.accounts_page.form.pending_mode_switch = None;
                    None
                }
                _ => None,
            };
        }

        if self.accounts_page.form.editing_field {
            return match (key.code, key.modifiers) {
                (KeyCode::Esc, _) | (KeyCode::Enter, _) => {
                    self.accounts_page.form.editing_field = false;
                    None
                }
                (KeyCode::Tab, _) => {
                    self.accounts_page.form.editing_field = false;
                    self.accounts_page.form.active_field = (self.accounts_page.form.active_field
                        + 1)
                        % self.account_form_field_count();
                    self.accounts_page.form.field_cursor =
                        account_form_field_value(&self.accounts_page.form)
                            .map_or(0, |value| value.chars().count());
                    None
                }
                (KeyCode::BackTab, _) => {
                    self.accounts_page.form.editing_field = false;
                    self.accounts_page.form.active_field =
                        self.accounts_page.form.active_field.saturating_sub(1);
                    self.accounts_page.form.field_cursor =
                        account_form_field_value(&self.accounts_page.form)
                            .map_or(0, |value| value.chars().count());
                    None
                }
                (KeyCode::Left, _) => {
                    self.accounts_page.form.field_cursor =
                        self.accounts_page.form.field_cursor.saturating_sub(1);
                    None
                }
                (KeyCode::Right, _) => {
                    if let Some(value) = account_form_field_value(&self.accounts_page.form) {
                        self.accounts_page.form.field_cursor =
                            (self.accounts_page.form.field_cursor + 1).min(value.chars().count());
                    }
                    None
                }
                (KeyCode::Home, _) => {
                    self.accounts_page.form.field_cursor = 0;
                    None
                }
                (KeyCode::End, _) => {
                    self.accounts_page.form.field_cursor =
                        account_form_field_value(&self.accounts_page.form)
                            .map_or(0, |value| value.chars().count());
                    None
                }
                (KeyCode::Backspace, _) => {
                    delete_account_form_char(&mut self.accounts_page.form, true);
                    self.refresh_account_form_derived_fields();
                    None
                }
                (KeyCode::Delete, _) => {
                    delete_account_form_char(&mut self.accounts_page.form, false);
                    self.refresh_account_form_derived_fields();
                    None
                }
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    insert_account_form_char(&mut self.accounts_page.form, c);
                    self.refresh_account_form_derived_fields();
                    None
                }
                _ => None,
            };
        }

        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => {
                self.maybe_preserve_new_account_form_draft();
                None
            }
            (KeyCode::Left | KeyCode::Char('h'), _) => {
                self.request_account_form_mode_change(false);
                None
            }
            (KeyCode::Right | KeyCode::Char('l'), _) => {
                self.request_account_form_mode_change(true);
                None
            }
            (KeyCode::Char('j') | KeyCode::Down, _) => {
                self.accounts_page.form.active_field =
                    (self.accounts_page.form.active_field + 1) % self.account_form_field_count();
                None
            }
            (KeyCode::Char('k') | KeyCode::Up, _) => {
                self.accounts_page.form.active_field = if self.accounts_page.form.active_field == 0
                {
                    self.account_form_field_count().saturating_sub(1)
                } else {
                    self.accounts_page.form.active_field - 1
                };
                None
            }
            (KeyCode::Tab, _) => {
                if self.accounts_page.form.active_field == 0 {
                    self.request_account_form_mode_change(true);
                } else {
                    self.accounts_page.form.active_field = (self.accounts_page.form.active_field
                        + 1)
                        % self.account_form_field_count();
                }
                None
            }
            (KeyCode::BackTab, _) => {
                if self.accounts_page.form.active_field == 0 {
                    self.request_account_form_mode_change(false);
                } else {
                    self.accounts_page.form.active_field =
                        self.accounts_page.form.active_field.saturating_sub(1);
                }
                None
            }
            (KeyCode::Enter | KeyCode::Char('i'), _) => {
                if account_form_field_is_editable(&self.accounts_page.form) {
                    self.accounts_page.form.editing_field = true;
                    self.accounts_page.form.field_cursor =
                        account_form_field_value(&self.accounts_page.form)
                            .map_or(0, |value| value.chars().count());
                    None
                } else if self.accounts_page.form.active_field == 0 {
                    self.request_account_form_mode_change(true);
                    None
                } else if self.toggle_current_account_form_field(true) {
                    None
                } else {
                    None
                }
            }
            (KeyCode::Char('t'), _) => Some(Action::TestAccountForm),
            (KeyCode::Char('o'), _)
                if super::account_result_has_details(
                    self.accounts_page.form.last_result.as_ref(),
                ) =>
            {
                self.open_last_account_result_details_modal();
                None
            }
            (KeyCode::Char('r'), _)
                if matches!(self.accounts_page.form.mode, AccountFormMode::Gmail) =>
            {
                Some(Action::ReauthorizeAccountForm)
            }
            (KeyCode::Char('s'), _) => Some(Action::SaveAccountForm),
            (KeyCode::Char(' '), _) if self.accounts_page.form.active_field == 0 => {
                self.request_account_form_mode_change(true);
                None
            }
            (KeyCode::Char(' '), _) if self.toggle_current_account_form_field(true) => None,
            _ => None,
        }
    }
}
