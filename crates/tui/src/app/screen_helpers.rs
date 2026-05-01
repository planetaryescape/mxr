use super::*;

impl App {
    pub fn current_ui_context(&self) -> UiContext {
        match self.screen {
            Screen::Mailbox => match self.mailbox.active_pane {
                ActivePane::Sidebar => UiContext::MailboxSidebar,
                ActivePane::MailList => UiContext::MailboxList,
                ActivePane::MessageView => UiContext::MailboxMessage,
            },
            Screen::Search => {
                if self.search.page.editing {
                    UiContext::SearchEditor
                } else {
                    match self.search.page.active_pane {
                        SearchPane::Results => UiContext::SearchResults,
                        SearchPane::Preview => UiContext::SearchPreview,
                    }
                }
            }
            Screen::Rules => {
                if self.rules.page.form.visible {
                    UiContext::RulesForm
                } else {
                    UiContext::RulesList
                }
            }
            Screen::Diagnostics => UiContext::Diagnostics,
            Screen::Accounts => {
                if self.accounts.page.form.visible {
                    UiContext::AccountsForm
                } else {
                    UiContext::AccountsList
                }
            }
        }
    }

    pub fn current_screen_context(&self) -> ScreenContext {
        self.current_ui_context().screen()
    }

    pub fn enter_account_setup_onboarding(&mut self) {
        self.accounts.page.onboarding_required = true;
        self.accounts.page.onboarding_modal_open = true;
        self.modals.onboarding.visible = false;
        self.mailbox.active_label = None;
        self.mailbox.pending_active_label = None;
        self.mailbox.pending_label_fetch = None;
        self.mailbox.desired_system_mailbox = None;
    }

    pub(super) fn complete_account_setup_onboarding(&mut self) {
        self.accounts.page.onboarding_modal_open = false;
        self.screen = Screen::Accounts;
        self.accounts.page.refresh_pending = true;
        self.apply(Action::OpenAccountFormNew);
    }

    pub fn sync_rule_form_editors(&mut self) {
        self.rules.condition_editor =
            TextArea::from(if self.rules.page.form.condition.is_empty() {
                vec![String::new()]
            } else {
                self.rules
                    .page
                    .form
                    .condition
                    .lines()
                    .map(ToString::to_string)
                    .collect()
            });
        self.rules.action_editor = TextArea::from(if self.rules.page.form.action.is_empty() {
            vec![String::new()]
        } else {
            self.rules
                .page
                .form
                .action
                .lines()
                .map(ToString::to_string)
                .collect()
        });
    }

    pub fn sync_rule_form_strings_from_editors(&mut self) {
        self.rules.page.form.condition = self.rules.condition_editor.lines().join("\n");
        self.rules.page.form.action = self.rules.action_editor.lines().join("\n");
    }

    pub fn maybe_show_feature_onboarding(&mut self) {
        if self.modals.onboarding.seen || self.accounts.page.accounts.is_empty() {
            return;
        }
        self.modals.onboarding.visible = true;
        self.modals.onboarding.step = 0;
    }

    pub fn dismiss_feature_onboarding(&mut self) {
        self.modals.onboarding.visible = false;
        if !self.modals.onboarding.seen {
            self.modals.onboarding.seen = true;
            self.pending_local_state_save = true;
        }
    }

    pub fn advance_feature_onboarding(&mut self) {
        if self.modals.onboarding.step >= 4 {
            self.dismiss_feature_onboarding();
        } else {
            self.modals.onboarding.step += 1;
        }
    }
}
