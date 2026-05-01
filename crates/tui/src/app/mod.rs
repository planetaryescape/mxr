mod account_actions;
mod account_form_helpers;
mod actions;
mod attachment_helpers;
mod body_helpers;
mod compose_actions;
mod compose_helpers;
mod diagnostics_actions;
mod draw;
mod input;
mod mailbox_actions;
mod mailbox_helpers;
mod message_actions;
mod modal_actions;
mod mutation_actions;
mod mutation_helpers;
mod rule_actions;
mod runtime_helpers;
mod screen_actions;
mod screen_helpers;
mod search_actions;
mod search_helpers;
mod selection_helpers;
mod sidebar_helpers;
mod state;
use crate::action::{Action, PatternKind, ScreenContext, UiContext};
use crate::async_result::SearchResultData;
use crate::client::Client;
use crate::input::InputHandler;
use crate::terminal_images::{HtmlImageEntry, HtmlImageKey, TerminalImageSupport};
use crate::theme::Theme;
use crate::ui;
use mxr_config::RenderConfig;
use mxr_core::id::MessageId;
use mxr_core::types::*;
use mxr_core::MxrError;
use mxr_protocol::{MutationCommand, Request, Response, ResponseData};
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use ratatui::prelude::*;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use throbber_widgets_tui::ThrobberState;
use tui_textarea::TextArea;

pub(in crate::app) use crate::ui::label_picker::LabelPickerMode;
use state::PendingPreviewRead;
pub use state::*;

const PREVIEW_MARK_READ_DELAY: Duration = Duration::from_secs(5);
pub const SEARCH_PAGE_SIZE: u32 = 200;
const SEARCH_DEBOUNCE_DELAY: Duration = Duration::from_millis(250);
const SEARCH_SPINNER_TICK: Duration = Duration::from_millis(120);

fn sane_mail_sort_timestamp(date: &chrono::DateTime<chrono::Utc>) -> i64 {
    let cutoff = (chrono::Utc::now() + chrono::Duration::days(1)).timestamp();
    let timestamp = date.timestamp();
    if timestamp > cutoff {
        0
    } else {
        timestamp
    }
}

#[derive(Debug, Clone)]
pub enum MutationEffect {
    RemoveFromList(MessageId),
    RemoveFromListMany(Vec<MessageId>),
    UpdateFlags {
        message_id: MessageId,
        flags: MessageFlags,
    },
    UpdateFlagsMany {
        updates: Vec<(MessageId, MessageFlags)>,
    },
    ModifyLabels {
        message_ids: Vec<MessageId>,
        add: Vec<String>,
        remove: Vec<String>,
        status: String,
    },
    RefreshList,
    StatusOnly(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Mailbox,
    Search,
    Rules,
    Diagnostics,
    Accounts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SidebarGroup {
    SystemLabels,
    UserLabels,
    SavedSearches,
}

pub struct App {
    pub theme: Theme,
    pub mailbox: MailboxState,
    pub search: SearchState,
    pub accounts: AccountsState,
    pub rules: RulesState,
    pub diagnostics: DiagnosticsState,
    pub modals: ModalsState,
    pub compose: ComposeState,
    pub screen: Screen,
    pub should_quit: bool,
    pub command_palette: CommandPaletteState,
    pub last_sync_status: Option<String>,
    pub visible_height: usize,
    pub html_image_support: Option<TerminalImageSupport>,
    pub html_image_assets: HashMap<MessageId, HashMap<String, HtmlImageEntry>>,
    pub queued_html_image_asset_fetches: Vec<MessageId>,
    pub queued_html_image_decodes: Vec<HtmlImageKey>,
    pub in_flight_html_image_asset_requests: HashSet<MessageId>,
    pub pending_local_state_save: bool,
    pub status_message: Option<String>,
    pub pending_mutation_count: usize,
    pub pending_mutation_status: Option<String>,
    pub pending_mutation_queue: Vec<(Request, MutationEffect)>,
    input: InputHandler,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        Self::from_render_and_snooze(
            &RenderConfig::default(),
            &mxr_config::SnoozeConfig::default(),
        )
    }

    pub fn from_config(config: &mxr_config::MxrConfig) -> Self {
        let mut app = Self::from_render_and_snooze(&config.render, &config.snooze);
        app.apply_runtime_config(config);
        if config.accounts.is_empty() {
            app.enter_account_setup_onboarding();
        }
        app
    }

    pub fn apply_runtime_config(&mut self, config: &mxr_config::MxrConfig) {
        self.theme = Theme::from_spec(&config.appearance.theme);
        self.mailbox.reader_mode = config.render.reader_mode;
        self.mailbox.render_html_command = config.render.html_command.clone();
        self.mailbox.show_reader_stats = config.render.show_reader_stats;
        self.mailbox.remote_content_enabled = config.render.html_remote_content;
        self.modals.snooze_config = config.snooze.clone();
    }

    pub fn from_render_config(render: &RenderConfig) -> Self {
        Self::from_render_and_snooze(render, &mxr_config::SnoozeConfig::default())
    }

    fn from_render_and_snooze(
        render: &RenderConfig,
        snooze_config: &mxr_config::SnoozeConfig,
    ) -> Self {
        Self {
            theme: Theme::default(),
            mailbox: MailboxState::from_render_config(render),
            search: SearchState::default(),
            accounts: AccountsState::default(),
            rules: RulesState::default(),
            diagnostics: DiagnosticsState::default(),
            modals: ModalsState {
                snooze_config: snooze_config.clone(),
                ..ModalsState::default()
            },
            compose: ComposeState::default(),
            screen: Screen::Mailbox,
            should_quit: false,
            command_palette: CommandPaletteState::default(),
            last_sync_status: None,
            visible_height: 20,
            html_image_support: None,
            html_image_assets: HashMap::new(),
            queued_html_image_asset_fetches: Vec::new(),
            queued_html_image_decodes: Vec::new(),
            in_flight_html_image_asset_requests: HashSet::new(),
            pending_local_state_save: false,
            status_message: None,
            pending_mutation_count: 0,
            pending_mutation_status: None,
            pending_mutation_queue: Vec::new(),
            input: InputHandler::new(),
        }
    }
}

fn apply_provider_label_changes(
    label_provider_ids: &mut Vec<String>,
    add_provider_ids: &[String],
    remove_provider_ids: &[String],
) {
    label_provider_ids.retain(|provider_id| {
        !remove_provider_ids
            .iter()
            .any(|remove| remove == provider_id)
    });
    for provider_id in add_provider_ids {
        if !label_provider_ids
            .iter()
            .any(|existing| existing == provider_id)
        {
            label_provider_ids.push(provider_id.clone());
        }
    }
}

fn unsubscribe_method_label(method: &UnsubscribeMethod) -> &'static str {
    match method {
        UnsubscribeMethod::OneClick { .. } => "one-click",
        UnsubscribeMethod::Mailto { .. } => "mailto",
        UnsubscribeMethod::HttpLink { .. } => "browser link",
        UnsubscribeMethod::BodyLink { .. } => "body link",
        UnsubscribeMethod::None => "none",
    }
}

pub(crate) fn body_status_labels(
    metadata: &BodyViewMetadata,
    source: &BodySource,
    show_reader_stats: bool,
) -> Vec<String> {
    let mut chips = vec![primary_body_label(metadata, source).to_string()];

    if metadata.reader_applied {
        let origin = match source {
            BodySource::Plain => "from plain text",
            BodySource::Html => "from html",
            BodySource::Fallback => "from summary",
            BodySource::Snippet => "from snippet",
        };
        chips.push(origin.to_string());
    }
    if metadata.inline_images {
        chips.push("inline images".into());
    }
    if metadata.flowed {
        chips.push("wrapped text".into());
    }
    if metadata.mode == BodyViewMode::Html && metadata.remote_content_available {
        chips.push(if metadata.remote_content_enabled {
            "remote images shown".into()
        } else {
            "remote images blocked".into()
        });
    }
    if show_reader_stats {
        if let Some(label) = reader_trim_label(metadata) {
            chips.push(label);
        }
    }

    chips
}

pub(crate) fn unsubscribe_banner_label(method: &UnsubscribeMethod) -> Option<&'static str> {
    match method {
        UnsubscribeMethod::OneClick { .. } => Some("One-click unsubscribe"),
        UnsubscribeMethod::HttpLink { .. } | UnsubscribeMethod::BodyLink { .. } => {
            Some("Open unsubscribe page")
        }
        UnsubscribeMethod::Mailto { .. } => Some("Email unsubscribe"),
        UnsubscribeMethod::None => None,
    }
}

fn reader_trim_label(metadata: &BodyViewMetadata) -> Option<String> {
    if !metadata.reader_applied {
        return None;
    }

    let (Some(original), Some(cleaned)) = (metadata.original_lines, metadata.cleaned_lines) else {
        return None;
    };

    if cleaned >= original {
        return None;
    }

    let trimmed = original - cleaned;
    Some(format!(
        "trimmed {trimmed} {}",
        if trimmed == 1 { "line" } else { "lines" }
    ))
}

fn primary_body_label(metadata: &BodyViewMetadata, source: &BodySource) -> &'static str {
    match (metadata.mode, metadata.reader_applied, source) {
        (BodyViewMode::Html, _, BodySource::Html) => "original html",
        (BodyViewMode::Html, _, BodySource::Plain) => "plain text (no html)",
        (BodyViewMode::Html, _, BodySource::Fallback) => "message summary (no html)",
        (BodyViewMode::Html, _, BodySource::Snippet) => "snippet preview",
        (BodyViewMode::Text, true, _) => "reading view",
        (BodyViewMode::Text, false, BodySource::Plain) => "plain text",
        (BodyViewMode::Text, false, BodySource::Html) => "html as text",
        (BodyViewMode::Text, false, BodySource::Fallback) => "message summary",
        (BodyViewMode::Text, false, BodySource::Snippet) => "snippet preview",
    }
}

fn remove_from_list_effect(ids: &[MessageId]) -> MutationEffect {
    if ids.len() == 1 {
        MutationEffect::RemoveFromList(ids[0].clone())
    } else {
        MutationEffect::RemoveFromListMany(ids.to_vec())
    }
}

fn pluralize_messages(count: usize) -> &'static str {
    if count == 1 {
        "message"
    } else {
        "messages"
    }
}

fn bulk_message_detail(verb: &str, count: usize) -> String {
    format!(
        "You are about to {verb} these {count} {}.",
        pluralize_messages(count)
    )
}

fn subscription_summary_to_envelope(summary: &SubscriptionSummary) -> Envelope {
    Envelope {
        id: summary.latest_message_id.clone(),
        account_id: summary.account_id.clone(),
        provider_id: summary.latest_provider_id.clone(),
        thread_id: summary.latest_thread_id.clone(),
        message_id_header: None,
        in_reply_to: None,
        references: vec![],
        from: Address {
            name: summary.sender_name.clone(),
            email: summary.sender_email.clone(),
        },
        to: vec![],
        cc: vec![],
        bcc: vec![],
        subject: summary.latest_subject.clone(),
        date: summary.latest_date,
        flags: summary.latest_flags,
        snippet: summary.latest_snippet.clone(),
        has_attachments: summary.latest_has_attachments,
        size_bytes: summary.latest_size_bytes,
        unsubscribe: summary.unsubscribe.clone(),
        label_provider_ids: vec![],
    }
}

fn account_result_has_details(result: Option<&mxr_protocol::AccountOperationResult>) -> bool {
    let Some(result) = result else {
        return false;
    };

    result.save.is_some() || result.auth.is_some() || result.sync.is_some() || result.send.is_some()
}

fn account_result_modal_title(result: &mxr_protocol::AccountOperationResult) -> String {
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

fn account_result_modal_detail(result: &mxr_protocol::AccountOperationResult) -> String {
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

fn account_summary_to_config(
    account: &mxr_protocol::AccountSummaryData,
) -> Option<mxr_protocol::AccountConfigData> {
    Some(mxr_protocol::AccountConfigData {
        key: account.key.clone()?,
        name: account.name.clone(),
        email: account.email.clone(),
        enabled: account.enabled,
        sync: account.sync.clone(),
        send: account.send.clone(),
        is_default: account.is_default,
    })
}

fn account_form_from_config(account: mxr_protocol::AccountConfigData) -> AccountFormState {
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
            mxr_protocol::AccountSyncConfigData::Gmail {
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
            mxr_protocol::AccountSyncConfigData::Imap {
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
        Some(mxr_protocol::AccountSendConfigData::Smtp {
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
        Some(mxr_protocol::AccountSendConfigData::Gmail) => {
            if form.gmail_token_ref.is_empty() {
                form.gmail_token_ref = format!("mxr/{}-gmail", form.key);
            }
        }
        None => {}
    }

    form
}

fn account_form_field_value(form: &AccountFormState) -> Option<&str> {
    match (form.mode, form.active_field) {
        (_, 0) => None,
        (_, 1) => Some(form.key.as_str()),
        (_, 2) => Some(form.name.as_str()),
        (_, 3) => Some(form.email.as_str()),
        (AccountFormMode::Gmail, 4) => None,
        (AccountFormMode::Gmail, 5)
            if form.gmail_credential_source == mxr_protocol::GmailCredentialSourceData::Custom =>
        {
            Some(form.gmail_client_id.as_str())
        }
        (AccountFormMode::Gmail, 6)
            if form.gmail_credential_source == mxr_protocol::GmailCredentialSourceData::Custom =>
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

fn account_form_field_is_editable(form: &AccountFormState) -> bool {
    account_form_field_value(form).is_some()
}

fn with_account_form_field_mut<F>(form: &mut AccountFormState, mut update: F)
where
    F: FnMut(&mut String),
{
    let field = match (form.mode, form.active_field) {
        (_, 1) => &mut form.key,
        (_, 2) => &mut form.name,
        (_, 3) => &mut form.email,
        (AccountFormMode::Gmail, 5)
            if form.gmail_credential_source == mxr_protocol::GmailCredentialSourceData::Custom =>
        {
            &mut form.gmail_client_id
        }
        (AccountFormMode::Gmail, 6)
            if form.gmail_credential_source == mxr_protocol::GmailCredentialSourceData::Custom =>
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

fn insert_account_form_char(form: &mut AccountFormState, c: char) {
    let cursor = form.field_cursor;
    with_account_form_field_mut(form, |value| {
        let insert_at = char_to_byte_index(value, cursor);
        value.insert(insert_at, c);
    });
    form.field_cursor = form.field_cursor.saturating_add(1);
}

fn delete_account_form_char(form: &mut AccountFormState, backspace: bool) {
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
        .map(|(index, _)| index)
        .unwrap_or(value.len())
}

fn next_gmail_credential_source(
    current: mxr_protocol::GmailCredentialSourceData,
    forward: bool,
) -> mxr_protocol::GmailCredentialSourceData {
    match (current, forward) {
        (mxr_protocol::GmailCredentialSourceData::Bundled, true) => {
            mxr_protocol::GmailCredentialSourceData::Custom
        }
        (mxr_protocol::GmailCredentialSourceData::Custom, true) => {
            mxr_protocol::GmailCredentialSourceData::Bundled
        }
        (mxr_protocol::GmailCredentialSourceData::Bundled, false) => {
            mxr_protocol::GmailCredentialSourceData::Custom
        }
        (mxr_protocol::GmailCredentialSourceData::Custom, false) => {
            mxr_protocol::GmailCredentialSourceData::Bundled
        }
    }
}

pub fn snooze_presets() -> [SnoozePreset; 4] {
    [
        SnoozePreset::TomorrowMorning,
        SnoozePreset::Tonight,
        SnoozePreset::Weekend,
        SnoozePreset::NextMonday,
    ]
}

pub fn resolve_snooze_preset(
    preset: SnoozePreset,
    config: &mxr_config::SnoozeConfig,
) -> chrono::DateTime<chrono::Utc> {
    mxr_config::snooze::resolve_snooze_time(preset, config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::TestEnvelopeBuilder;
    use chrono::TimeZone;

    fn test_envelope(
        thread_id: mxr_core::ThreadId,
        subject: &str,
        date: chrono::DateTime<chrono::Utc>,
    ) -> Envelope {
        TestEnvelopeBuilder::new()
            .thread_id(thread_id)
            .subject(subject)
            .provider_id(subject)
            .date(date)
            .to(vec![])
            .message_id_header(None)
            .snippet("")
            .size_bytes(0)
            .build()
    }

    #[test]
    fn build_mail_list_rows_ignores_impossible_future_thread_dates() {
        let thread_id = mxr_core::ThreadId::new();
        let poisoned = test_envelope(
            thread_id.clone(),
            "Poisoned future",
            chrono::Utc
                .timestamp_opt(236_816_444_325, 0)
                .single()
                .unwrap(),
        );
        let recent = test_envelope(thread_id, "Real recent", chrono::Utc::now());

        let rows = App::build_mail_list_rows(&[poisoned, recent.clone()], MailListMode::Threads);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].representative.subject, recent.subject);
    }
}
