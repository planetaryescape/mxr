use super::*;

impl App {
    pub(super) fn apply_account_action(&mut self, action: Action) {
        match action {
            Action::RefreshAccounts => {
                self.accounts.page.refresh_pending = true;
            }
            Action::OpenAccountFormNew => {
                if self.accounts.page.new_account_draft.is_some() {
                    self.accounts.page.resume_new_account_draft_prompt_open = true;
                    self.accounts.page.onboarding_modal_open = false;
                    self.screen = Screen::Accounts;
                    return;
                }
                self.open_new_account_form();
                self.accounts.page.onboarding_modal_open = false;
                self.screen = Screen::Accounts;
            }
            Action::SaveAccountForm => {
                let is_default = self
                    .selected_account()
                    .is_some_and(|account| account.is_default)
                    || self.accounts.page.accounts.is_empty();
                self.accounts.page.last_result = None;
                self.accounts.page.form.last_result = None;
                if let Some((result, first_invalid_field)) = self.account_form_validation_failure()
                {
                    self.fail_account_form_submission(result, first_invalid_field);
                    return;
                }
                self.accounts.page.operation_in_flight = true;
                self.accounts.page.throbber = ThrobberState::default();
                self.accounts.pending_save = Some(self.account_form_data(is_default));
                self.accounts.page.status = Some("Saving account...".into());
            }
            Action::TestAccountForm => {
                let account = if self.accounts.page.form.visible {
                    self.accounts.page.last_result = None;
                    self.accounts.page.form.last_result = None;
                    if let Some((result, first_invalid_field)) =
                        self.account_form_validation_failure()
                    {
                        self.fail_account_form_submission(result, first_invalid_field);
                        return;
                    }
                    self.account_form_data(false)
                } else if let Some(account) = self.selected_account_config() {
                    self.accounts.page.last_result = None;
                    self.accounts.page.form.last_result = None;
                    account
                } else {
                    self.accounts.page.status = Some("No editable account selected.".into());
                    return;
                };
                self.accounts.page.operation_in_flight = true;
                self.accounts.page.throbber = ThrobberState::default();
                self.accounts.pending_test = Some(account);
                self.accounts.page.status = Some("Testing account...".into());
            }
            Action::ReauthorizeAccountForm => {
                self.accounts.page.last_result = None;
                self.accounts.page.form.last_result = None;
                if let Some((result, first_invalid_field)) = self.account_form_validation_failure()
                {
                    self.fail_account_form_submission(result, first_invalid_field);
                    return;
                }
                let account = self.account_form_data(false);
                self.accounts.page.operation_in_flight = true;
                self.accounts.page.throbber = ThrobberState::default();
                self.accounts.pending_authorize = Some((account, true));
                self.accounts.page.status = Some("Authorizing Gmail account...".into());
            }
            Action::SetDefaultAccount => {
                if let Some(key) = self
                    .selected_account()
                    .and_then(|account| account.key.clone())
                {
                    self.accounts.page.operation_in_flight = true;
                    self.accounts.page.throbber = ThrobberState::default();
                    self.accounts.pending_set_default = Some(key);
                    self.accounts.page.status = Some("Setting default account...".into());
                } else {
                    self.accounts.page.status =
                        Some("Runtime-only account cannot be set default from TUI.".into());
                }
            }
            Action::SwitchAccount(key) => {
                self.accounts.pending_set_default = Some(key);
                self.accounts.pending_switch = true;
                self.status_message = Some("Switching account...".into());
            }
            _ => unreachable!("action routed to wrong handler"),
        }
    }
}
