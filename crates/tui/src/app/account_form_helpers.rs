use super::*;

impl App {
    pub(super) fn selected_account_config(&self) -> Option<mxr_protocol::AccountConfigData> {
        self.selected_account().and_then(account_summary_to_config)
    }

    pub(super) fn account_form_field_count(&self) -> usize {
        match self.accounts.page.form.mode {
            AccountFormMode::Gmail => {
                if self.accounts.page.form.gmail_credential_source
                    == mxr_protocol::GmailCredentialSourceData::Custom
                {
                    8
                } else {
                    6
                }
            }
            AccountFormMode::ImapSmtp => 16,
            AccountFormMode::SmtpOnly => 10,
        }
    }

    pub(super) fn account_form_data(&self, is_default: bool) -> mxr_protocol::AccountConfigData {
        let form = &self.accounts.page.form;
        let key = form.key.trim().to_string();
        let name = if form.name.trim().is_empty() {
            key.clone()
        } else {
            form.name.trim().to_string()
        };
        let email = form.email.trim().to_string();
        let imap_username = if form.imap_auth_required && form.imap_username.trim().is_empty() {
            email.clone()
        } else {
            form.imap_username.trim().to_string()
        };
        let smtp_username = if form.smtp_auth_required && form.smtp_username.trim().is_empty() {
            email.clone()
        } else {
            form.smtp_username.trim().to_string()
        };
        let imap_password_ref =
            if form.imap_auth_required && form.imap_password_ref.trim().is_empty() {
                if key.is_empty() {
                    String::new()
                } else {
                    format!("mxr/{key}-imap")
                }
            } else {
                form.imap_password_ref.trim().to_string()
            };
        let smtp_password_ref =
            if form.smtp_auth_required && form.smtp_password_ref.trim().is_empty() {
                if key.is_empty() {
                    String::new()
                } else {
                    format!("mxr/{key}-smtp")
                }
            } else {
                form.smtp_password_ref.trim().to_string()
            };
        let gmail_token_ref = if form.gmail_token_ref.trim().is_empty() {
            format!("mxr/{key}-gmail")
        } else {
            form.gmail_token_ref.trim().to_string()
        };
        let sync = match form.mode {
            AccountFormMode::Gmail => Some(mxr_protocol::AccountSyncConfigData::Gmail {
                credential_source: form.gmail_credential_source.clone(),
                client_id: form.gmail_client_id.trim().to_string(),
                client_secret: if form.gmail_client_secret.trim().is_empty() {
                    None
                } else {
                    Some(form.gmail_client_secret.clone())
                },
                token_ref: gmail_token_ref,
            }),
            AccountFormMode::ImapSmtp => Some(mxr_protocol::AccountSyncConfigData::Imap {
                host: form.imap_host.trim().to_string(),
                port: form.imap_port.parse().unwrap_or(993),
                username: imap_username,
                password_ref: imap_password_ref,
                password: if form.imap_password.is_empty() {
                    None
                } else {
                    Some(form.imap_password.clone())
                },
                auth_required: form.imap_auth_required,
                use_tls: true,
            }),
            AccountFormMode::SmtpOnly => None,
        };
        let send = match form.mode {
            AccountFormMode::Gmail => Some(mxr_protocol::AccountSendConfigData::Gmail),
            AccountFormMode::ImapSmtp | AccountFormMode::SmtpOnly => {
                Some(mxr_protocol::AccountSendConfigData::Smtp {
                    host: form.smtp_host.trim().to_string(),
                    port: form.smtp_port.parse().unwrap_or(587),
                    username: smtp_username,
                    password_ref: smtp_password_ref,
                    password: if form.smtp_password.is_empty() {
                        None
                    } else {
                        Some(form.smtp_password.clone())
                    },
                    auth_required: form.smtp_auth_required,
                    use_tls: true,
                })
            }
        };
        mxr_protocol::AccountConfigData {
            key,
            name,
            email,
            enabled: true,
            sync,
            send,
            is_default,
        }
    }

    pub(super) fn account_form_validation_failure(
        &self,
    ) -> Option<(mxr_protocol::AccountOperationResult, usize)> {
        let form = &self.accounts.page.form;
        let mut first_invalid = None;
        let mut remember_first_invalid = |field: usize| {
            if first_invalid.is_none() {
                first_invalid = Some(field);
            }
        };

        let mut form_issues = Vec::new();
        if form.key.trim().is_empty() {
            form_issues.push("Account key is required.".to_string());
            remember_first_invalid(1);
        }
        if form.email.trim().is_empty() {
            form_issues.push("Email is required.".to_string());
            remember_first_invalid(3);
        }

        let save = (!form_issues.is_empty()).then(|| mxr_protocol::AccountOperationStep {
            ok: false,
            detail: form_issues.join(" "),
        });

        let mut auth = None;
        let mut sync = None;
        let mut send = None;

        match form.mode {
            AccountFormMode::Gmail => {
                if form.gmail_credential_source == mxr_protocol::GmailCredentialSourceData::Custom {
                    let mut auth_issues = Vec::new();
                    if form.gmail_client_id.trim().is_empty() {
                        auth_issues
                            .push("Client ID is required for custom Gmail auth.".to_string());
                        remember_first_invalid(5);
                    }
                    if form.gmail_client_secret.trim().is_empty() {
                        auth_issues
                            .push("Client Secret is required for custom Gmail auth.".to_string());
                        remember_first_invalid(6);
                    }
                    if !auth_issues.is_empty() {
                        auth = Some(mxr_protocol::AccountOperationStep {
                            ok: false,
                            detail: auth_issues.join(" "),
                        });
                    }
                }
            }
            AccountFormMode::ImapSmtp => {
                let mut sync_issues = Vec::new();
                if form.imap_host.trim().is_empty() {
                    sync_issues.push("IMAP host is required.".to_string());
                    remember_first_invalid(4);
                }
                if form.imap_port.trim().is_empty() {
                    sync_issues.push("IMAP port is required.".to_string());
                    remember_first_invalid(5);
                } else if form.imap_port.trim().parse::<u16>().is_err() {
                    sync_issues.push("IMAP port must be a valid number.".to_string());
                    remember_first_invalid(5);
                }
                if form.imap_auth_required {
                    if form.email.trim().is_empty() && form.imap_username.trim().is_empty() {
                        sync_issues.push(
                            "IMAP auth is enabled, so Email or IMAP user is required.".to_string(),
                        );
                        remember_first_invalid(3);
                    }
                    if form.imap_password_ref.trim().is_empty() && form.imap_password.is_empty() {
                        sync_issues.push(
                            "IMAP auth is enabled, so IMAP password or IMAP pass ref is required."
                                .to_string(),
                        );
                        remember_first_invalid(9);
                    }
                }
                if !sync_issues.is_empty() {
                    sync = Some(mxr_protocol::AccountOperationStep {
                        ok: false,
                        detail: sync_issues.join(" "),
                    });
                }

                let mut send_issues = Vec::new();
                if form.smtp_host.trim().is_empty() {
                    send_issues.push("SMTP host is required.".to_string());
                    remember_first_invalid(10);
                }
                if form.smtp_port.trim().is_empty() {
                    send_issues.push("SMTP port is required.".to_string());
                    remember_first_invalid(11);
                } else if form.smtp_port.trim().parse::<u16>().is_err() {
                    send_issues.push("SMTP port must be a valid number.".to_string());
                    remember_first_invalid(11);
                }
                if form.smtp_auth_required {
                    if form.email.trim().is_empty() && form.smtp_username.trim().is_empty() {
                        send_issues.push(
                            "SMTP auth is enabled, so Email or SMTP user is required.".to_string(),
                        );
                        remember_first_invalid(3);
                    }
                    if form.smtp_password_ref.trim().is_empty() && form.smtp_password.is_empty() {
                        send_issues.push(
                            "SMTP auth is enabled, so SMTP password or SMTP pass ref is required."
                                .to_string(),
                        );
                        remember_first_invalid(15);
                    }
                }
                if !send_issues.is_empty() {
                    send = Some(mxr_protocol::AccountOperationStep {
                        ok: false,
                        detail: send_issues.join(" "),
                    });
                }
            }
            AccountFormMode::SmtpOnly => {
                let mut send_issues = Vec::new();
                if form.smtp_host.trim().is_empty() {
                    send_issues.push("SMTP host is required.".to_string());
                    remember_first_invalid(4);
                }
                if form.smtp_port.trim().is_empty() {
                    send_issues.push("SMTP port is required.".to_string());
                    remember_first_invalid(5);
                } else if form.smtp_port.trim().parse::<u16>().is_err() {
                    send_issues.push("SMTP port must be a valid number.".to_string());
                    remember_first_invalid(5);
                }
                if form.smtp_auth_required {
                    if form.email.trim().is_empty() && form.smtp_username.trim().is_empty() {
                        send_issues.push(
                            "SMTP auth is enabled, so Email or SMTP user is required.".to_string(),
                        );
                        remember_first_invalid(3);
                    }
                    if form.smtp_password_ref.trim().is_empty() && form.smtp_password.is_empty() {
                        send_issues.push(
                            "SMTP auth is enabled, so SMTP password or SMTP pass ref is required."
                                .to_string(),
                        );
                        remember_first_invalid(9);
                    }
                }
                if !send_issues.is_empty() {
                    send = Some(mxr_protocol::AccountOperationStep {
                        ok: false,
                        detail: send_issues.join(" "),
                    });
                }
            }
        }

        if save.is_none() && auth.is_none() && sync.is_none() && send.is_none() {
            return None;
        }

        Some((
            mxr_protocol::AccountOperationResult {
                ok: false,
                summary: "Account form has problems. Fix the listed fields and try again.".into(),
                save,
                auth,
                sync,
                send,
            },
            first_invalid.unwrap_or(0),
        ))
    }

    pub(super) fn fail_account_form_submission(
        &mut self,
        result: mxr_protocol::AccountOperationResult,
        first_invalid_field: usize,
    ) {
        self.accounts.page.operation_in_flight = false;
        self.accounts.page.status = Some(result.summary.clone());
        self.accounts.page.last_result = Some(result.clone());
        self.accounts.page.form.last_result = Some(result);
        self.accounts.page.form.active_field =
            first_invalid_field.min(self.account_form_field_count().saturating_sub(1));
        self.accounts.page.form.editing_field = false;
        self.accounts.page.form.field_cursor = account_form_field_value(&self.accounts.page.form)
            .map(|value| value.chars().count())
            .unwrap_or(0);
    }

    pub(super) fn account_result_modal_hint(label: &str, detail: &str) -> Option<&'static str> {
        let detail = detail.to_ascii_lowercase();
        if label == "Sync"
            && (detail.contains("namespace response")
                || detail.contains("could not parse")
                || detail.contains("unsupported format"))
        {
            return Some("This looks like an IMAP server compatibility issue, not a bad password.");
        }
        None
    }

    pub(super) fn next_account_form_mode(&self, forward: bool) -> AccountFormMode {
        match (self.accounts.page.form.mode, forward) {
            (AccountFormMode::Gmail, true) => AccountFormMode::ImapSmtp,
            (AccountFormMode::ImapSmtp, true) => AccountFormMode::SmtpOnly,
            (AccountFormMode::SmtpOnly, true) => AccountFormMode::Gmail,
            (AccountFormMode::Gmail, false) => AccountFormMode::SmtpOnly,
            (AccountFormMode::ImapSmtp, false) => AccountFormMode::Gmail,
            (AccountFormMode::SmtpOnly, false) => AccountFormMode::ImapSmtp,
        }
    }

    pub(super) fn account_form_has_meaningful_input(&self) -> bool {
        let form = &self.accounts.page.form;
        [
            form.key.trim(),
            form.name.trim(),
            form.email.trim(),
            form.gmail_client_id.trim(),
            form.gmail_client_secret.trim(),
            form.imap_host.trim(),
            form.imap_username.trim(),
            form.imap_password_ref.trim(),
            form.imap_password.trim(),
            form.smtp_host.trim(),
            form.smtp_username.trim(),
            form.smtp_password_ref.trim(),
            form.smtp_password.trim(),
        ]
        .iter()
        .any(|value| !value.is_empty())
    }

    pub(super) fn open_new_account_form(&mut self) {
        self.accounts.page.form = AccountFormState {
            visible: true,
            is_new_account: true,
            ..AccountFormState::default()
        };
        self.accounts.page.resume_new_account_draft_prompt_open = false;
        self.refresh_account_form_derived_fields();
    }

    pub(super) fn restore_new_account_form_draft(&mut self) {
        if let Some(mut draft) = self.accounts.page.new_account_draft.take() {
            draft.visible = true;
            draft.pending_mode_switch = None;
            draft.editing_field = false;
            self.accounts.page.form = draft;
            self.accounts.page.resume_new_account_draft_prompt_open = false;
            self.refresh_account_form_derived_fields();
        } else {
            self.open_new_account_form();
        }
    }

    pub(super) fn maybe_preserve_new_account_form_draft(&mut self) {
        if !self.accounts.page.form.visible {
            return;
        }

        if self.accounts.page.form.is_new_account && self.account_form_has_meaningful_input() {
            let mut draft = self.accounts.page.form.clone();
            draft.visible = false;
            draft.pending_mode_switch = None;
            draft.editing_field = false;
            self.accounts.page.new_account_draft = Some(draft);
        }

        self.accounts.page.form.visible = false;
        self.accounts.page.form.pending_mode_switch = None;
        self.accounts.page.form.editing_field = false;
        self.accounts.page.resume_new_account_draft_prompt_open = false;
    }

    pub(super) fn apply_account_form_mode(&mut self, mode: AccountFormMode) {
        self.accounts.page.form.mode = mode;
        self.accounts.page.form.pending_mode_switch = None;
        self.accounts.page.form.active_field = self
            .accounts
            .page
            .form
            .active_field
            .min(self.account_form_field_count().saturating_sub(1));
        self.accounts.page.form.editing_field = false;
        self.accounts.page.form.field_cursor = 0;
        self.refresh_account_form_derived_fields();
    }

    pub(super) fn request_account_form_mode_change(&mut self, forward: bool) {
        let next_mode = self.next_account_form_mode(forward);
        if next_mode == self.accounts.page.form.mode {
            return;
        }
        if self.account_form_has_meaningful_input() {
            self.accounts.page.form.pending_mode_switch = Some(next_mode);
        } else {
            self.apply_account_form_mode(next_mode);
        }
    }

    pub(super) fn refresh_account_form_derived_fields(&mut self) {
        if matches!(self.accounts.page.form.mode, AccountFormMode::Gmail) {
            let key = self.accounts.page.form.key.trim();
            let token_ref = if key.is_empty() {
                String::new()
            } else {
                format!("mxr/{key}-gmail")
            };
            self.accounts.page.form.gmail_token_ref = token_ref;
        }
    }

    pub(super) fn current_account_form_toggle_field(&self) -> Option<AccountFormToggleField> {
        match (
            self.accounts.page.form.mode,
            self.accounts.page.form.active_field,
        ) {
            (AccountFormMode::Gmail, 4) => Some(AccountFormToggleField::GmailCredentialSource),
            (AccountFormMode::ImapSmtp, 7) => Some(AccountFormToggleField::ImapAuthRequired),
            (AccountFormMode::ImapSmtp, 13) => Some(AccountFormToggleField::SmtpAuthRequired),
            (AccountFormMode::SmtpOnly, 7) => Some(AccountFormToggleField::SmtpAuthRequired),
            _ => None,
        }
    }

    pub(super) fn toggle_current_account_form_field(&mut self, forward: bool) -> bool {
        match self.current_account_form_toggle_field() {
            Some(AccountFormToggleField::GmailCredentialSource) => {
                self.accounts.page.form.gmail_credential_source = next_gmail_credential_source(
                    self.accounts.page.form.gmail_credential_source.clone(),
                    forward,
                );
                self.accounts.page.form.active_field = self
                    .accounts
                    .page
                    .form
                    .active_field
                    .min(self.account_form_field_count().saturating_sub(1));
                true
            }
            Some(AccountFormToggleField::ImapAuthRequired) => {
                self.accounts.page.form.imap_auth_required =
                    !self.accounts.page.form.imap_auth_required;
                true
            }
            Some(AccountFormToggleField::SmtpAuthRequired) => {
                self.accounts.page.form.smtp_auth_required =
                    !self.accounts.page.form.smtp_auth_required;
                true
            }
            None => false,
        }
    }

    pub(crate) fn apply_account_operation_result(
        &mut self,
        result: mxr_protocol::AccountOperationResult,
    ) {
        self.accounts.page.operation_in_flight = false;
        self.accounts.page.throbber = Default::default();
        self.accounts.page.status = Some(result.summary.clone());
        self.accounts.page.last_result = Some(result.clone());
        self.accounts.page.form.last_result = Some(result.clone());
        self.accounts.page.form.gmail_authorized = result
            .auth
            .as_ref()
            .map(|step| step.ok)
            .unwrap_or(self.accounts.page.form.gmail_authorized);
        if result.save.as_ref().is_some_and(|step| step.ok) {
            self.accounts.page.new_account_draft = None;
            self.accounts.page.resume_new_account_draft_prompt_open = false;
            self.accounts.page.form.visible = false;
        }
        if !result.ok && account_result_has_details(Some(&result)) {
            self.open_account_result_details_modal(&result);
        }
        self.accounts.page.refresh_pending = true;
    }

    pub(crate) fn open_last_account_result_details_modal(&mut self) {
        if let Some(result) = self
            .accounts
            .page
            .form
            .last_result
            .clone()
            .or_else(|| self.accounts.page.last_result.clone())
        {
            self.open_account_result_details_modal(&result);
        }
    }

    pub(super) fn open_account_result_details_modal(
        &mut self,
        result: &mxr_protocol::AccountOperationResult,
    ) {
        self.show_error_modal(
            account_result_modal_title(result),
            account_result_modal_detail(result),
        );
    }

    pub(crate) fn handle_account_switch_complete(&mut self) {
        self.clear_message_view_state();
        self.close_attachment_panel();
        self.mailbox.mailbox_view = MailboxView::Messages;
        self.mailbox.layout_mode = LayoutMode::TwoPane;
        if self.mailbox.active_pane == ActivePane::MessageView {
            self.mailbox.active_pane = ActivePane::MailList;
        }
        self.mailbox.envelopes.clear();
        self.mailbox.all_envelopes.clear();
        self.search.page.results.clear();
        self.mailbox.subscriptions_page.entries.clear();
        self.mailbox.selected_set.clear();
        self.mailbox.active_label = None;
        self.mailbox.pending_active_label = None;
        self.mailbox.pending_label_fetch = None;
        self.mailbox.selected_index = 0;
        self.mailbox.scroll_offset = 0;
        self.mailbox.pending_labels_refresh = true;
        self.mailbox.pending_all_envelopes_refresh = true;
        self.mailbox.pending_subscriptions_refresh = true;
        self.diagnostics.pending_status_refresh = true;
        self.mailbox.desired_system_mailbox = Some("INBOX".into());
        self.mailbox.mailbox_loading_message = Some("Loading selected account...".into());
        self.mailbox.mailbox_loading_throbber = ThrobberState::default();
        self.status_message = Some("Loading selected account...".into());
    }
}
