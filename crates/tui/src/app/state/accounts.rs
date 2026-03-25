use throbber_widgets_tui::ThrobberState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountFormMode {
    Gmail,
    ImapSmtp,
    SmtpOnly,
    OutlookPersonal,
    OutlookWork,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::app) enum AccountFormToggleField {
    GmailCredentialSource,
    ImapAuthRequired,
    SmtpAuthRequired,
}

#[derive(Debug, Clone)]
pub struct AccountFormState {
    pub visible: bool,
    pub is_new_account: bool,
    pub mode: AccountFormMode,
    pub pending_mode_switch: Option<AccountFormMode>,
    pub key: String,
    pub name: String,
    pub email: String,
    pub gmail_credential_source: mxr_protocol::GmailCredentialSourceData,
    pub gmail_client_id: String,
    pub gmail_client_secret: String,
    pub gmail_token_ref: String,
    pub gmail_authorized: bool,
    pub outlook_client_id: String,
    pub outlook_token_ref: String,
    pub outlook_authorized: bool,
    pub imap_host: String,
    pub imap_port: String,
    pub imap_username: String,
    pub imap_password_ref: String,
    pub imap_password: String,
    pub imap_auth_required: bool,
    pub smtp_host: String,
    pub smtp_port: String,
    pub smtp_username: String,
    pub smtp_password_ref: String,
    pub smtp_password: String,
    pub smtp_auth_required: bool,
    pub active_field: usize,
    pub editing_field: bool,
    pub field_cursor: usize,
    pub last_result: Option<mxr_protocol::AccountOperationResult>,
}

impl Default for AccountFormState {
    fn default() -> Self {
        Self {
            visible: false,
            is_new_account: false,
            mode: AccountFormMode::Gmail,
            pending_mode_switch: None,
            key: String::new(),
            name: String::new(),
            email: String::new(),
            gmail_credential_source: mxr_protocol::GmailCredentialSourceData::Bundled,
            gmail_client_id: String::new(),
            gmail_client_secret: String::new(),
            gmail_token_ref: String::new(),
            gmail_authorized: false,
            outlook_client_id: String::new(),
            outlook_token_ref: String::new(),
            outlook_authorized: false,
            imap_host: String::new(),
            imap_port: "993".into(),
            imap_username: String::new(),
            imap_password_ref: String::new(),
            imap_password: String::new(),
            imap_auth_required: true,
            smtp_host: String::new(),
            smtp_port: "587".into(),
            smtp_username: String::new(),
            smtp_password_ref: String::new(),
            smtp_password: String::new(),
            smtp_auth_required: true,
            active_field: 0,
            editing_field: false,
            field_cursor: 0,
            last_result: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AccountsPageState {
    pub accounts: Vec<mxr_protocol::AccountSummaryData>,
    pub selected_index: usize,
    pub status: Option<String>,
    pub last_result: Option<mxr_protocol::AccountOperationResult>,
    pub operation_in_flight: bool,
    pub throbber: ThrobberState,
    pub refresh_pending: bool,
    pub onboarding_required: bool,
    pub onboarding_modal_open: bool,
    pub new_account_draft: Option<AccountFormState>,
    pub resume_new_account_draft_prompt_open: bool,
    pub form: AccountFormState,
}

#[derive(Default)]
pub struct AccountsState {
    pub page: AccountsPageState,
    pub pending_save: Option<mxr_protocol::AccountConfigData>,
    pub pending_test: Option<mxr_protocol::AccountConfigData>,
    pub pending_authorize: Option<(mxr_protocol::AccountConfigData, bool)>,
    pub pending_set_default: Option<String>,
    /// True when the set-default was triggered from sidebar account switching
    /// (vs the Accounts tab). Used to trigger full state reset on completion.
    pub pending_switch: bool,
}
