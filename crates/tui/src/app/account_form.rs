use super::*;

impl App {
    pub(super) fn selected_account_config(&self) -> Option<crate::mxr_protocol::AccountConfigData> {
        self.selected_account().and_then(account_summary_to_config)
    }

    pub(super) fn account_form_field_count(&self) -> usize {
        match self.accounts_page.form.mode {
            AccountFormMode::Gmail => {
                if self.accounts_page.form.gmail_credential_source
                    == crate::mxr_protocol::GmailCredentialSourceData::Custom
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

    pub(super) fn account_form_data(
        &self,
        is_default: bool,
    ) -> crate::mxr_protocol::AccountConfigData {
        let form = &self.accounts_page.form;
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
            AccountFormMode::Gmail => Some(crate::mxr_protocol::AccountSyncConfigData::Gmail {
                credential_source: form.gmail_credential_source.clone(),
                client_id: form.gmail_client_id.trim().to_string(),
                client_secret: if form.gmail_client_secret.trim().is_empty() {
                    None
                } else {
                    Some(form.gmail_client_secret.clone())
                },
                token_ref: gmail_token_ref,
            }),
            AccountFormMode::ImapSmtp => Some(crate::mxr_protocol::AccountSyncConfigData::Imap {
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
            AccountFormMode::Gmail => Some(crate::mxr_protocol::AccountSendConfigData::Gmail),
            AccountFormMode::ImapSmtp | AccountFormMode::SmtpOnly => {
                Some(crate::mxr_protocol::AccountSendConfigData::Smtp {
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
        crate::mxr_protocol::AccountConfigData {
            key,
            name,
            email,
            sync,
            send,
            is_default,
        }
    }

    pub(super) fn account_form_validation_failure(
        &self,
    ) -> Option<(crate::mxr_protocol::AccountOperationResult, usize)> {
        let form = &self.accounts_page.form;
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

        let save = (!form_issues.is_empty()).then(|| crate::mxr_protocol::AccountOperationStep {
            ok: false,
            detail: form_issues.join(" "),
        });

        let mut auth = None;
        let mut sync = None;
        let mut send = None;

        match form.mode {
            AccountFormMode::Gmail => {
                if form.gmail_credential_source
                    == crate::mxr_protocol::GmailCredentialSourceData::Custom
                {
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
                        auth = Some(crate::mxr_protocol::AccountOperationStep {
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
                    sync = Some(crate::mxr_protocol::AccountOperationStep {
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
                    send = Some(crate::mxr_protocol::AccountOperationStep {
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
                    send = Some(crate::mxr_protocol::AccountOperationStep {
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
            crate::mxr_protocol::AccountOperationResult {
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
        result: crate::mxr_protocol::AccountOperationResult,
        first_invalid_field: usize,
    ) {
        self.accounts_page.operation_in_flight = false;
        self.accounts_page.status = Some(result.summary.clone());
        self.accounts_page.last_result = Some(result.clone());
        self.accounts_page.form.last_result = Some(result);
        self.accounts_page.form.active_field =
            first_invalid_field.min(self.account_form_field_count().saturating_sub(1));
        self.accounts_page.form.editing_field = false;
        self.accounts_page.form.field_cursor = account_form_field_value(&self.accounts_page.form)
            .map_or(0, |value| value.chars().count());
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
        match (self.accounts_page.form.mode, forward) {
            (AccountFormMode::Gmail, true) => AccountFormMode::ImapSmtp,
            (AccountFormMode::ImapSmtp, true) => AccountFormMode::SmtpOnly,
            (AccountFormMode::SmtpOnly, true) => AccountFormMode::Gmail,
            (AccountFormMode::Gmail, false) => AccountFormMode::SmtpOnly,
            (AccountFormMode::ImapSmtp, false) => AccountFormMode::Gmail,
            (AccountFormMode::SmtpOnly, false) => AccountFormMode::ImapSmtp,
        }
    }

    pub(super) fn account_form_has_meaningful_input(&self) -> bool {
        let form = &self.accounts_page.form;
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
        self.accounts_page.form = AccountFormState {
            visible: true,
            is_new_account: true,
            ..AccountFormState::default()
        };
        self.accounts_page.resume_new_account_draft_prompt_open = false;
        self.refresh_account_form_derived_fields();
    }

    pub(super) fn restore_new_account_form_draft(&mut self) {
        if let Some(mut draft) = self.accounts_page.new_account_draft.take() {
            draft.visible = true;
            draft.pending_mode_switch = None;
            draft.editing_field = false;
            self.accounts_page.form = draft;
            self.accounts_page.resume_new_account_draft_prompt_open = false;
            self.refresh_account_form_derived_fields();
        } else {
            self.open_new_account_form();
        }
    }

    pub(super) fn maybe_preserve_new_account_form_draft(&mut self) {
        if !self.accounts_page.form.visible {
            return;
        }

        if self.accounts_page.form.is_new_account && self.account_form_has_meaningful_input() {
            let mut draft = self.accounts_page.form.clone();
            draft.visible = false;
            draft.pending_mode_switch = None;
            draft.editing_field = false;
            self.accounts_page.new_account_draft = Some(draft);
        }

        self.accounts_page.form.visible = false;
        self.accounts_page.form.pending_mode_switch = None;
        self.accounts_page.form.editing_field = false;
        self.accounts_page.resume_new_account_draft_prompt_open = false;
    }

    pub(super) fn apply_account_form_mode(&mut self, mode: AccountFormMode) {
        self.accounts_page.form.mode = mode;
        self.accounts_page.form.pending_mode_switch = None;
        self.accounts_page.form.active_field = self
            .accounts_page
            .form
            .active_field
            .min(self.account_form_field_count().saturating_sub(1));
        self.accounts_page.form.editing_field = false;
        self.accounts_page.form.field_cursor = 0;
        self.refresh_account_form_derived_fields();
    }

    pub(super) fn request_account_form_mode_change(&mut self, forward: bool) {
        let next_mode = self.next_account_form_mode(forward);
        if next_mode == self.accounts_page.form.mode {
            return;
        }
        if self.account_form_has_meaningful_input() {
            self.accounts_page.form.pending_mode_switch = Some(next_mode);
        } else {
            self.apply_account_form_mode(next_mode);
        }
    }

    pub(super) fn refresh_account_form_derived_fields(&mut self) {
        if matches!(self.accounts_page.form.mode, AccountFormMode::Gmail) {
            let key = self.accounts_page.form.key.trim();
            let token_ref = if key.is_empty() {
                String::new()
            } else {
                format!("mxr/{key}-gmail")
            };
            self.accounts_page.form.gmail_token_ref = token_ref;
        }
    }

    pub(super) fn current_account_form_toggle_field(&self) -> Option<AccountFormToggleField> {
        match (
            self.accounts_page.form.mode,
            self.accounts_page.form.active_field,
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
                self.accounts_page.form.gmail_credential_source = next_gmail_credential_source(
                    self.accounts_page.form.gmail_credential_source.clone(),
                    forward,
                );
                self.accounts_page.form.active_field = self
                    .accounts_page
                    .form
                    .active_field
                    .min(self.account_form_field_count().saturating_sub(1));
                true
            }
            Some(AccountFormToggleField::ImapAuthRequired) => {
                self.accounts_page.form.imap_auth_required =
                    !self.accounts_page.form.imap_auth_required;
                true
            }
            Some(AccountFormToggleField::SmtpAuthRequired) => {
                self.accounts_page.form.smtp_auth_required =
                    !self.accounts_page.form.smtp_auth_required;
                true
            }
            None => false,
        }
    }
}

pub(super) fn account_summary_to_config(
    account: &crate::mxr_protocol::AccountSummaryData,
) -> Option<crate::mxr_protocol::AccountConfigData> {
    Some(crate::mxr_protocol::AccountConfigData {
        key: account.key.clone()?,
        name: account.name.clone(),
        email: account.email.clone(),
        sync: account.sync.clone(),
        send: account.send.clone(),
        is_default: account.is_default,
    })
}

pub(super) fn account_form_from_config(
    account: crate::mxr_protocol::AccountConfigData,
) -> AccountFormState {
    let mut form = AccountFormState {
        visible: true,
        is_new_account: false,
        key: account.key,
        name: account.name,
        email: account.email,
        ..AccountFormState::default()
    };

    if let Some(sync) = account.sync {
        match sync {
            crate::mxr_protocol::AccountSyncConfigData::Gmail {
                credential_source,
                client_id,
                client_secret,
                token_ref,
            } => {
                form.mode = AccountFormMode::Gmail;
                form.gmail_credential_source = credential_source;
                form.gmail_client_id = client_id;
                form.gmail_client_secret = client_secret.unwrap_or_default();
                form.gmail_token_ref = token_ref;
            }
            crate::mxr_protocol::AccountSyncConfigData::Imap {
                host,
                port,
                username,
                password_ref,
                auth_required,
                ..
            } => {
                form.mode = AccountFormMode::ImapSmtp;
                form.imap_host = host;
                form.imap_port = port.to_string();
                form.imap_username = username;
                form.imap_password_ref = password_ref;
                form.imap_auth_required = auth_required;
            }
        }
    } else {
        form.mode = AccountFormMode::SmtpOnly;
    }

    match account.send {
        Some(crate::mxr_protocol::AccountSendConfigData::Smtp {
            host,
            port,
            username,
            password_ref,
            auth_required,
            ..
        }) => {
            form.smtp_host = host;
            form.smtp_port = port.to_string();
            form.smtp_username = username;
            form.smtp_password_ref = password_ref;
            form.smtp_auth_required = auth_required;
        }
        Some(crate::mxr_protocol::AccountSendConfigData::Gmail) => {
            if form.gmail_token_ref.is_empty() {
                form.gmail_token_ref = format!("mxr/{}-gmail", form.key);
            }
        }
        None => {}
    }

    form
}

pub(super) fn account_form_field_value(form: &AccountFormState) -> Option<&str> {
    match (form.mode, form.active_field) {
        (_, 0) => None,
        (_, 1) => Some(form.key.as_str()),
        (_, 2) => Some(form.name.as_str()),
        (_, 3) => Some(form.email.as_str()),
        (AccountFormMode::Gmail, 4) => None,
        (AccountFormMode::Gmail, 5)
            if form.gmail_credential_source
                == crate::mxr_protocol::GmailCredentialSourceData::Custom =>
        {
            Some(form.gmail_client_id.as_str())
        }
        (AccountFormMode::Gmail, 6)
            if form.gmail_credential_source
                == crate::mxr_protocol::GmailCredentialSourceData::Custom =>
        {
            Some(form.gmail_client_secret.as_str())
        }
        (AccountFormMode::Gmail, 5 | 6) => None,
        (AccountFormMode::Gmail, 7) => None,
        (AccountFormMode::ImapSmtp, 4) => Some(form.imap_host.as_str()),
        (AccountFormMode::ImapSmtp, 5) => Some(form.imap_port.as_str()),
        (AccountFormMode::ImapSmtp, 6) => Some(form.imap_username.as_str()),
        (AccountFormMode::ImapSmtp, 7) => None,
        (AccountFormMode::ImapSmtp, 8) => Some(form.imap_password_ref.as_str()),
        (AccountFormMode::ImapSmtp, 9) => Some(form.imap_password.as_str()),
        (AccountFormMode::ImapSmtp, 10) => Some(form.smtp_host.as_str()),
        (AccountFormMode::ImapSmtp, 11) => Some(form.smtp_port.as_str()),
        (AccountFormMode::ImapSmtp, 12) => Some(form.smtp_username.as_str()),
        (AccountFormMode::ImapSmtp, 13) => None,
        (AccountFormMode::ImapSmtp, 14) => Some(form.smtp_password_ref.as_str()),
        (AccountFormMode::ImapSmtp, 15) => Some(form.smtp_password.as_str()),
        (AccountFormMode::SmtpOnly, 4) => Some(form.smtp_host.as_str()),
        (AccountFormMode::SmtpOnly, 5) => Some(form.smtp_port.as_str()),
        (AccountFormMode::SmtpOnly, 6) => Some(form.smtp_username.as_str()),
        (AccountFormMode::SmtpOnly, 7) => None,
        (AccountFormMode::SmtpOnly, 8) => Some(form.smtp_password_ref.as_str()),
        (AccountFormMode::SmtpOnly, 9) => Some(form.smtp_password.as_str()),
        _ => None,
    }
}

pub(super) fn account_form_field_is_editable(form: &AccountFormState) -> bool {
    account_form_field_value(form).is_some()
}

pub(super) fn with_account_form_field_mut<F>(form: &mut AccountFormState, mut update: F)
where
    F: FnMut(&mut String),
{
    let field = match (form.mode, form.active_field) {
        (_, 1) => &mut form.key,
        (_, 2) => &mut form.name,
        (_, 3) => &mut form.email,
        (AccountFormMode::Gmail, 5)
            if form.gmail_credential_source
                == crate::mxr_protocol::GmailCredentialSourceData::Custom =>
        {
            &mut form.gmail_client_id
        }
        (AccountFormMode::Gmail, 6)
            if form.gmail_credential_source
                == crate::mxr_protocol::GmailCredentialSourceData::Custom =>
        {
            &mut form.gmail_client_secret
        }
        (AccountFormMode::ImapSmtp, 4) => &mut form.imap_host,
        (AccountFormMode::ImapSmtp, 5) => &mut form.imap_port,
        (AccountFormMode::ImapSmtp, 6) => &mut form.imap_username,
        (AccountFormMode::ImapSmtp, 8) => &mut form.imap_password_ref,
        (AccountFormMode::ImapSmtp, 9) => &mut form.imap_password,
        (AccountFormMode::ImapSmtp, 10) => &mut form.smtp_host,
        (AccountFormMode::ImapSmtp, 11) => &mut form.smtp_port,
        (AccountFormMode::ImapSmtp, 12) => &mut form.smtp_username,
        (AccountFormMode::ImapSmtp, 14) => &mut form.smtp_password_ref,
        (AccountFormMode::ImapSmtp, 15) => &mut form.smtp_password,
        (AccountFormMode::SmtpOnly, 4) => &mut form.smtp_host,
        (AccountFormMode::SmtpOnly, 5) => &mut form.smtp_port,
        (AccountFormMode::SmtpOnly, 6) => &mut form.smtp_username,
        (AccountFormMode::SmtpOnly, 8) => &mut form.smtp_password_ref,
        (AccountFormMode::SmtpOnly, 9) => &mut form.smtp_password,
        _ => return,
    };
    update(field);
}

pub(super) fn insert_account_form_char(form: &mut AccountFormState, c: char) {
    let cursor = form.field_cursor;
    with_account_form_field_mut(form, |value| {
        let insert_at = char_to_byte_index(value, cursor);
        value.insert(insert_at, c);
    });
    form.field_cursor = form.field_cursor.saturating_add(1);
}

pub(super) fn delete_account_form_char(form: &mut AccountFormState, backspace: bool) {
    let cursor = form.field_cursor;
    with_account_form_field_mut(form, |value| {
        if backspace {
            if cursor == 0 {
                return;
            }
            let start = char_to_byte_index(value, cursor - 1);
            let end = char_to_byte_index(value, cursor);
            value.replace_range(start..end, "");
        } else {
            let len = value.chars().count();
            if cursor >= len {
                return;
            }
            let start = char_to_byte_index(value, cursor);
            let end = char_to_byte_index(value, cursor + 1);
            value.replace_range(start..end, "");
        }
    });
    if backspace {
        form.field_cursor = form.field_cursor.saturating_sub(1);
    }
}

fn char_to_byte_index(value: &str, char_index: usize) -> usize {
    value
        .char_indices()
        .nth(char_index)
        .map_or(value.len(), |(index, _)| index)
}

pub(super) fn next_gmail_credential_source(
    current: crate::mxr_protocol::GmailCredentialSourceData,
    forward: bool,
) -> crate::mxr_protocol::GmailCredentialSourceData {
    match (current, forward) {
        (crate::mxr_protocol::GmailCredentialSourceData::Bundled, true) => {
            crate::mxr_protocol::GmailCredentialSourceData::Custom
        }
        (crate::mxr_protocol::GmailCredentialSourceData::Custom, true) => {
            crate::mxr_protocol::GmailCredentialSourceData::Bundled
        }
        (crate::mxr_protocol::GmailCredentialSourceData::Bundled, false) => {
            crate::mxr_protocol::GmailCredentialSourceData::Custom
        }
        (crate::mxr_protocol::GmailCredentialSourceData::Custom, false) => {
            crate::mxr_protocol::GmailCredentialSourceData::Bundled
        }
    }
}

pub(super) fn account_result_has_details(
    result: Option<&crate::mxr_protocol::AccountOperationResult>,
) -> bool {
    let Some(result) = result else {
        return false;
    };

    result.save.is_some() || result.auth.is_some() || result.sync.is_some() || result.send.is_some()
}

pub(super) fn account_result_modal_title(
    result: &crate::mxr_protocol::AccountOperationResult,
) -> String {
    if result.summary.contains("test failed") {
        "Account Test Failed".into()
    } else if result.summary.contains("test passed") {
        "Account Test Result".into()
    } else if result.summary.starts_with("Account form has problems.") {
        "Account Form Problems".into()
    } else {
        "Account Setup Details".into()
    }
}

pub(super) fn account_result_modal_detail(
    result: &crate::mxr_protocol::AccountOperationResult,
) -> String {
    let mut lines = vec![result.summary.clone()];
    for (label, step) in [
        ("Save", result.save.as_ref()),
        ("Auth", result.auth.as_ref()),
        ("Sync", result.sync.as_ref()),
        ("Send", result.send.as_ref()),
    ] {
        let Some(step) = step else {
            continue;
        };
        lines.push(String::new());
        lines.push(format!(
            "{label}: {}",
            if step.ok { "ok" } else { "failed" }
        ));
        lines.push(step.detail.clone());
        if let Some(hint) = App::account_result_modal_hint(label, &step.detail) {
            lines.push(format!("Hint: {hint}"));
        }
    }
    lines.join("\n")
}
