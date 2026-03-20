use crate::action::{Action, PatternKind};
use crate::client::Client;
use crate::input::InputHandler;
use crate::ui;
use crate::ui::command_palette::CommandPalette;
use crate::ui::compose_picker::ComposePicker;
use crate::ui::label_picker::{LabelPicker, LabelPickerMode};
use crate::ui::search_bar::SearchBar;
use crossterm::event::{KeyCode, KeyModifiers};
use mxr_config::RenderConfig;
use mxr_core::id::{AttachmentId, MessageId};
use mxr_core::types::*;
use mxr_core::MxrError;
use mxr_protocol::{MutationCommand, Request};
use ratatui::prelude::*;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub enum MutationEffect {
    RemoveFromList(MessageId),
    RemoveFromListMany(Vec<MessageId>),
    UpdateFlags {
        message_id: MessageId,
        flags: MessageFlags,
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

/// Draft waiting for user confirmation after editor closes.
pub struct PendingSend {
    pub fm: mxr_compose::frontmatter::ComposeFrontmatter,
    pub body: String,
    pub draft_path: std::path::PathBuf,
    pub allow_send: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposeAction {
    New,
    NewWithTo(String),
    EditDraft(std::path::PathBuf),
    Reply { message_id: MessageId },
    ReplyAll { message_id: MessageId },
    Forward { message_id: MessageId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivePane {
    Sidebar,
    MailList,
    MessageView,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailListMode {
    Threads,
    Messages,
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
pub enum SidebarSection {
    Labels,
    SavedSearches,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutMode {
    TwoPane,
    ThreePane,
    FullScreen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodySource {
    Plain,
    Html,
    Snippet,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BodyViewState {
    Loading { preview: Option<String> },
    Ready {
        raw: String,
        rendered: String,
        source: BodySource,
    },
    Empty { preview: Option<String> },
    Error { message: String, preview: Option<String> },
}

#[derive(Debug, Clone)]
pub struct MailListRow {
    pub thread_id: mxr_core::ThreadId,
    pub representative: Envelope,
    pub message_count: usize,
    pub unread_count: usize,
}

#[derive(Debug, Clone)]
pub enum SidebarItem {
    Label(Label),
    SavedSearch(mxr_core::SavedSearch),
}

#[derive(Debug, Clone, Default)]
pub struct SearchPageState {
    pub query: String,
    pub editing: bool,
    pub results: Vec<Envelope>,
    pub selected_index: usize,
    pub scroll_offset: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RulesPanel {
    Details,
    History,
    DryRun,
    Form,
}

#[derive(Debug, Clone, Default)]
pub struct RuleFormState {
    pub visible: bool,
    pub existing_rule: Option<String>,
    pub name: String,
    pub condition: String,
    pub action: String,
    pub priority: String,
    pub enabled: bool,
    pub active_field: usize,
}

#[derive(Debug, Clone)]
pub struct RulesPageState {
    pub rules: Vec<serde_json::Value>,
    pub selected_index: usize,
    pub detail: Option<serde_json::Value>,
    pub history: Vec<serde_json::Value>,
    pub dry_run: Vec<serde_json::Value>,
    pub panel: RulesPanel,
    pub status: Option<String>,
    pub refresh_pending: bool,
    pub form: RuleFormState,
}

impl Default for RulesPageState {
    fn default() -> Self {
        Self {
            rules: Vec::new(),
            selected_index: 0,
            detail: None,
            history: Vec::new(),
            dry_run: Vec::new(),
            panel: RulesPanel::Details,
            status: None,
            refresh_pending: false,
            form: RuleFormState {
                enabled: true,
                priority: "100".to_string(),
                ..RuleFormState::default()
            },
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DiagnosticsPageState {
    pub uptime_secs: Option<u64>,
    pub accounts: Vec<String>,
    pub total_messages: Option<u32>,
    pub doctor: Option<mxr_protocol::DoctorReport>,
    pub events: Vec<mxr_protocol::EventLogEntry>,
    pub logs: Vec<String>,
    pub status: Option<String>,
    pub refresh_pending: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountFormMode {
    ImapSmtp,
    SmtpOnly,
}

#[derive(Debug, Clone)]
pub struct AccountFormState {
    pub visible: bool,
    pub mode: AccountFormMode,
    pub key: String,
    pub name: String,
    pub email: String,
    pub imap_host: String,
    pub imap_port: String,
    pub imap_username: String,
    pub imap_password_ref: String,
    pub imap_password: String,
    pub smtp_host: String,
    pub smtp_port: String,
    pub smtp_username: String,
    pub smtp_password_ref: String,
    pub smtp_password: String,
    pub active_field: usize,
}

impl Default for AccountFormState {
    fn default() -> Self {
        Self {
            visible: false,
            mode: AccountFormMode::ImapSmtp,
            key: String::new(),
            name: String::new(),
            email: String::new(),
            imap_host: String::new(),
            imap_port: "993".into(),
            imap_username: String::new(),
            imap_password_ref: String::new(),
            imap_password: String::new(),
            smtp_host: String::new(),
            smtp_port: "587".into(),
            smtp_username: String::new(),
            smtp_password_ref: String::new(),
            smtp_password: String::new(),
            active_field: 0,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AccountsPageState {
    pub accounts: Vec<mxr_protocol::AccountSummaryData>,
    pub selected_index: usize,
    pub status: Option<String>,
    pub refresh_pending: bool,
    pub form: AccountFormState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentOperation {
    Open,
    Download,
}

#[derive(Debug, Clone, Default)]
pub struct AttachmentPanelState {
    pub visible: bool,
    pub message_id: Option<MessageId>,
    pub attachments: Vec<AttachmentMeta>,
    pub selected_index: usize,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnoozePreset {
    TomorrowMorning,
    Tonight,
    Weekend,
    NextMonday,
}

#[derive(Debug, Clone, Default)]
pub struct SnoozePanelState {
    pub visible: bool,
    pub selected_index: usize,
}

#[derive(Debug, Clone)]
pub struct PendingAttachmentAction {
    pub message_id: MessageId,
    pub attachment_id: AttachmentId,
    pub operation: AttachmentOperation,
}

#[derive(Debug, Clone)]
pub struct PendingBulkConfirm {
    pub title: String,
    pub detail: String,
    pub request: Request,
    pub effect: MutationEffect,
    pub status_message: String,
}

impl BodyViewState {
    pub fn display_text(&self) -> Option<&str> {
        match self {
            Self::Ready { rendered, .. } => Some(rendered.as_str()),
            Self::Loading { preview } => preview.as_deref(),
            Self::Empty { preview } => preview.as_deref(),
            Self::Error { preview, .. } => preview.as_deref(),
        }
    }
}

pub struct App {
    pub envelopes: Vec<Envelope>,
    pub all_envelopes: Vec<Envelope>,
    pub labels: Vec<Label>,
    pub screen: Screen,
    pub mail_list_mode: MailListMode,
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub active_pane: ActivePane,
    pub should_quit: bool,
    pub layout_mode: LayoutMode,
    pub search_bar: SearchBar,
    pub search_page: SearchPageState,
    pub command_palette: CommandPalette,
    pub body_view_state: BodyViewState,
    pub viewing_envelope: Option<Envelope>,
    pub viewed_thread: Option<Thread>,
    pub viewed_thread_messages: Vec<Envelope>,
    pub thread_selected_index: usize,
    pub message_scroll_offset: u16,
    pub last_sync_status: Option<String>,
    pub visible_height: usize,
    pub body_cache: HashMap<MessageId, MessageBody>,
    pub queued_body_fetches: Vec<MessageId>,
    pub in_flight_body_requests: HashSet<MessageId>,
    pub pending_thread_fetch: Option<mxr_core::ThreadId>,
    pub in_flight_thread_fetch: Option<mxr_core::ThreadId>,
    pub pending_search: Option<String>,
    pub search_active: bool,
    pub pending_rule_detail: Option<String>,
    pub pending_rule_history: Option<String>,
    pub pending_rule_dry_run: Option<String>,
    pub pending_rule_delete: Option<String>,
    pub pending_rule_upsert: Option<serde_json::Value>,
    pub pending_rule_form_load: Option<String>,
    pub pending_rule_form_save: bool,
    pub pending_bug_report: bool,
    pub pending_account_save: Option<mxr_protocol::AccountConfigData>,
    pub pending_account_test: Option<mxr_protocol::AccountConfigData>,
    pub pending_account_set_default: Option<String>,
    pub sidebar_selected: usize,
    pub sidebar_section: SidebarSection,
    pub help_modal_open: bool,
    pub help_scroll_offset: u16,
    pub saved_searches: Vec<mxr_core::SavedSearch>,
    pub rules_page: RulesPageState,
    pub diagnostics_page: DiagnosticsPageState,
    pub accounts_page: AccountsPageState,
    pub active_label: Option<mxr_core::LabelId>,
    pub pending_label_fetch: Option<mxr_core::LabelId>,
    pub pending_active_label: Option<mxr_core::LabelId>,
    pub status_message: Option<String>,
    pub pending_mutation_queue: Vec<(Request, MutationEffect)>,
    pub pending_compose: Option<ComposeAction>,
    pub pending_send_confirm: Option<PendingSend>,
    pub pending_bulk_confirm: Option<PendingBulkConfirm>,
    pub reader_mode: bool,
    pub label_picker: LabelPicker,
    pub compose_picker: ComposePicker,
    pub attachment_panel: AttachmentPanelState,
    pub snooze_panel: SnoozePanelState,
    pub pending_attachment_action: Option<PendingAttachmentAction>,
    pub selected_set: HashSet<MessageId>,
    pub visual_mode: bool,
    pub visual_anchor: Option<usize>,
    pub pending_export_thread: Option<mxr_core::id::ThreadId>,
    pub snooze_config: mxr_config::SnoozeConfig,
    pending_label_action: Option<(LabelPickerMode, String)>,
    input: InputHandler,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        Self::from_render_and_snooze(&RenderConfig::default(), &mxr_config::SnoozeConfig::default())
    }

    pub fn from_config(config: &mxr_config::MxrConfig) -> Self {
        Self::from_render_and_snooze(&config.render, &config.snooze)
    }

    pub fn from_render_config(render: &RenderConfig) -> Self {
        Self::from_render_and_snooze(render, &mxr_config::SnoozeConfig::default())
    }

    fn from_render_and_snooze(
        render: &RenderConfig,
        snooze_config: &mxr_config::SnoozeConfig,
    ) -> Self {
        Self {
            envelopes: Vec::new(),
            all_envelopes: Vec::new(),
            labels: Vec::new(),
            screen: Screen::Mailbox,
            mail_list_mode: MailListMode::Threads,
            selected_index: 0,
            scroll_offset: 0,
            active_pane: ActivePane::MailList,
            should_quit: false,
            layout_mode: LayoutMode::TwoPane,
            search_bar: SearchBar::default(),
            search_page: SearchPageState::default(),
            command_palette: CommandPalette::default(),
            body_view_state: BodyViewState::Empty { preview: None },
            viewing_envelope: None,
            viewed_thread: None,
            viewed_thread_messages: Vec::new(),
            thread_selected_index: 0,
            message_scroll_offset: 0,
            last_sync_status: None,
            visible_height: 20,
            body_cache: HashMap::new(),
            queued_body_fetches: Vec::new(),
            in_flight_body_requests: HashSet::new(),
            pending_thread_fetch: None,
            in_flight_thread_fetch: None,
            pending_search: None,
            search_active: false,
            pending_rule_detail: None,
            pending_rule_history: None,
            pending_rule_dry_run: None,
            pending_rule_delete: None,
            pending_rule_upsert: None,
            pending_rule_form_load: None,
            pending_rule_form_save: false,
            pending_bug_report: false,
            pending_account_save: None,
            pending_account_test: None,
            pending_account_set_default: None,
            sidebar_selected: 0,
            sidebar_section: SidebarSection::Labels,
            help_modal_open: false,
            help_scroll_offset: 0,
            saved_searches: Vec::new(),
            rules_page: RulesPageState::default(),
            diagnostics_page: DiagnosticsPageState::default(),
            accounts_page: AccountsPageState::default(),
            active_label: None,
            pending_label_fetch: None,
            pending_active_label: None,
            status_message: None,
            pending_mutation_queue: Vec::new(),
            pending_compose: None,
            pending_send_confirm: None,
            pending_bulk_confirm: None,
            reader_mode: render.reader_mode,
            label_picker: LabelPicker::default(),
            compose_picker: ComposePicker::default(),
            attachment_panel: AttachmentPanelState::default(),
            snooze_panel: SnoozePanelState::default(),
            pending_attachment_action: None,
            selected_set: HashSet::new(),
            visual_mode: false,
            visual_anchor: None,
            pending_export_thread: None,
            snooze_config: snooze_config.clone(),
            pending_label_action: None,
            input: InputHandler::new(),
        }
    }

    pub fn selected_envelope(&self) -> Option<&Envelope> {
        match self.mail_list_mode {
            MailListMode::Messages => self.envelopes.get(self.selected_index),
            MailListMode::Threads => self
                .selected_mail_row()
                .and_then(|row| self.envelopes.iter().find(|env| env.id == row.representative.id)),
        }
    }

    pub fn mail_list_rows(&self) -> Vec<MailListRow> {
        Self::build_mail_list_rows(&self.envelopes, self.mail_list_mode)
    }

    pub fn search_mail_list_rows(&self) -> Vec<MailListRow> {
        Self::build_mail_list_rows(&self.search_page.results, self.mail_list_mode)
    }

    pub fn selected_mail_row(&self) -> Option<MailListRow> {
        self.mail_list_rows().get(self.selected_index).cloned()
    }

    pub fn focused_thread_envelope(&self) -> Option<&Envelope> {
        self.viewed_thread_messages.get(self.thread_selected_index)
    }

    pub fn sidebar_items(&self) -> Vec<SidebarItem> {
        let mut items = self
            .visible_labels()
            .into_iter()
            .cloned()
            .map(SidebarItem::Label)
            .collect::<Vec<_>>();
        items.extend(
            self.saved_searches
                .iter()
                .cloned()
                .map(SidebarItem::SavedSearch),
        );
        items
    }

    pub fn selected_sidebar_item(&self) -> Option<SidebarItem> {
        self.sidebar_items().get(self.sidebar_selected).cloned()
    }

    pub fn selected_search_envelope(&self) -> Option<&Envelope> {
        match self.mail_list_mode {
            MailListMode::Messages => self.search_page.results.get(self.search_page.selected_index),
            MailListMode::Threads => self
                .search_mail_list_rows()
                .get(self.search_page.selected_index)
                .and_then(|row| {
                    self.search_page
                        .results
                        .iter()
                        .find(|env| env.id == row.representative.id)
                }),
        }
    }

    pub fn selected_rule(&self) -> Option<&serde_json::Value> {
        self.rules_page.rules.get(self.rules_page.selected_index)
    }

    pub fn selected_account(&self) -> Option<&mxr_protocol::AccountSummaryData> {
        self.accounts_page
            .accounts
            .get(self.accounts_page.selected_index)
    }

    fn selected_account_config(&self) -> Option<mxr_protocol::AccountConfigData> {
        self.selected_account().and_then(account_summary_to_config)
    }

    fn account_form_field_count(&self) -> usize {
        match self.accounts_page.form.mode {
            AccountFormMode::ImapSmtp => 14,
            AccountFormMode::SmtpOnly => 9,
        }
    }

    fn account_form_data(&self, is_default: bool) -> mxr_protocol::AccountConfigData {
        let form = &self.accounts_page.form;
        let key = form.key.trim().to_string();
        let name = if form.name.trim().is_empty() {
            key.clone()
        } else {
            form.name.trim().to_string()
        };
        let email = form.email.trim().to_string();
        let imap_username = if form.imap_username.trim().is_empty() {
            email.clone()
        } else {
            form.imap_username.trim().to_string()
        };
        let smtp_username = if form.smtp_username.trim().is_empty() {
            email.clone()
        } else {
            form.smtp_username.trim().to_string()
        };
        let sync = match form.mode {
            AccountFormMode::ImapSmtp => Some(mxr_protocol::AccountSyncConfigData::Imap {
                host: form.imap_host.trim().to_string(),
                port: form.imap_port.parse().unwrap_or(993),
                username: imap_username,
                password_ref: form.imap_password_ref.trim().to_string(),
                password: if form.imap_password.is_empty() {
                    None
                } else {
                    Some(form.imap_password.clone())
                },
                use_tls: true,
            }),
            AccountFormMode::SmtpOnly => None,
        };
        let send = Some(mxr_protocol::AccountSendConfigData::Smtp {
            host: form.smtp_host.trim().to_string(),
            port: form.smtp_port.parse().unwrap_or(587),
            username: smtp_username,
            password_ref: form.smtp_password_ref.trim().to_string(),
            password: if form.smtp_password.is_empty() {
                None
            } else {
                Some(form.smtp_password.clone())
            },
            use_tls: true,
        });
        mxr_protocol::AccountConfigData {
            key,
            name,
            email,
            sync,
            send,
            is_default,
        }
    }

    fn mail_row_count(&self) -> usize {
        self.mail_list_rows().len()
    }

    fn search_row_count(&self) -> usize {
        self.search_mail_list_rows().len()
    }

    fn build_mail_list_rows(envelopes: &[Envelope], mode: MailListMode) -> Vec<MailListRow> {
        match mode {
            MailListMode::Messages => envelopes
                .iter()
                .map(|envelope| MailListRow {
                    thread_id: envelope.thread_id.clone(),
                    representative: envelope.clone(),
                    message_count: 1,
                    unread_count: usize::from(!envelope.flags.contains(MessageFlags::READ)),
                })
                .collect(),
            MailListMode::Threads => {
                let mut order: Vec<mxr_core::ThreadId> = Vec::new();
                let mut rows: HashMap<mxr_core::ThreadId, MailListRow> = HashMap::new();
                for envelope in envelopes {
                    let entry = rows.entry(envelope.thread_id.clone()).or_insert_with(|| {
                        order.push(envelope.thread_id.clone());
                        MailListRow {
                            thread_id: envelope.thread_id.clone(),
                            representative: envelope.clone(),
                            message_count: 0,
                            unread_count: 0,
                        }
                    });
                    entry.message_count += 1;
                    if !envelope.flags.contains(MessageFlags::READ) {
                        entry.unread_count += 1;
                    }
                    if envelope.date > entry.representative.date {
                        entry.representative = envelope.clone();
                    }
                }
                order
                    .into_iter()
                    .filter_map(|thread_id| rows.remove(&thread_id))
                    .collect()
            }
        }
    }

    /// Get the contextual envelope: the one being viewed, or the selected one.
    fn context_envelope(&self) -> Option<&Envelope> {
        if self.screen == Screen::Search {
            return self
                .focused_thread_envelope()
                .or(self.viewing_envelope.as_ref())
                .or_else(|| self.selected_search_envelope());
        }

        self.focused_thread_envelope()
            .or(self.viewing_envelope.as_ref())
            .or_else(|| self.selected_envelope())
    }

    pub async fn load(&mut self, client: &mut Client) -> Result<(), MxrError> {
        self.envelopes = client.list_envelopes(5000, 0).await?;
        self.all_envelopes = self.envelopes.clone();
        self.labels = client.list_labels().await?;
        self.saved_searches = client.list_saved_searches().await.unwrap_or_default();
        // Queue body prefetch for first visible window
        self.queue_body_window();
        Ok(())
    }

    pub fn input_pending(&self) -> bool {
        self.input.is_pending()
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        if self.help_modal_open {
            return match (key.code, key.modifiers) {
                (KeyCode::Esc | KeyCode::Enter, _)
                | (KeyCode::Char('?'), _)
                | (KeyCode::Char('q'), _) => Some(Action::Help),
                (KeyCode::Char('j') | KeyCode::Down, _) => {
                    self.help_scroll_offset = self.help_scroll_offset.saturating_add(1);
                    None
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    self.help_scroll_offset = self.help_scroll_offset.saturating_sub(1);
                    None
                }
                (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                    self.help_scroll_offset = self.help_scroll_offset.saturating_add(8);
                    None
                }
                (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                    self.help_scroll_offset = self.help_scroll_offset.saturating_sub(8);
                    None
                }
                _ => None,
            };
        }

        if self.command_palette.visible {
            match (key.code, key.modifiers) {
                (KeyCode::Enter, _) => return self.command_palette.confirm(),
                (KeyCode::Esc, _) => return Some(Action::CloseCommandPalette),
                (KeyCode::Backspace, _) => {
                    self.command_palette.on_backspace();
                    return None;
                }
                (KeyCode::Down, _) | (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
                    self.command_palette.select_next();
                    return None;
                }
                (KeyCode::Up, _) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                    self.command_palette.select_prev();
                    return None;
                }
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    self.command_palette.on_char(c);
                    return None;
                }
                _ => return None,
            }
        }

        // Route keys to search bar when active
        if self.search_bar.active {
            match (key.code, key.modifiers) {
                (KeyCode::Enter, _) => return Some(Action::SubmitSearch),
                (KeyCode::Esc, _) => return Some(Action::CloseSearch),
                (KeyCode::Backspace, _) => {
                    self.search_bar.on_backspace();
                    // Live filter as you type
                    self.trigger_live_search();
                    return None;
                }
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    self.search_bar.on_char(c);
                    // Live filter as you type
                    self.trigger_live_search();
                    return None;
                }
                _ => return None,
            }
        }

        // Route keys to send confirmation prompt
        if self.pending_send_confirm.is_some() {
            match (key.code, key.modifiers) {
                (KeyCode::Char('s'), KeyModifiers::NONE) => {
                    // Send
                    if let Some(pending) = self.pending_send_confirm.take() {
                        if !pending.allow_send {
                            self.pending_send_confirm = Some(pending);
                            return None;
                        }
                        let parse_addrs = |s: &str| -> Vec<mxr_core::Address> {
                            s.split(',')
                                .map(|a| a.trim())
                                .filter(|a| !a.is_empty())
                                .map(|a| mxr_core::Address {
                                    name: None,
                                    email: a.to_string(),
                                })
                                .collect()
                        };
                        let account_id = self
                            .envelopes
                            .first()
                            .or(self.all_envelopes.first())
                            .map(|e| e.account_id.clone())
                            .unwrap_or_default();
                        let now = chrono::Utc::now();
                        let draft = mxr_core::Draft {
                            id: mxr_core::id::DraftId::new(),
                            account_id,
                            in_reply_to: None,
                            to: parse_addrs(&pending.fm.to),
                            cc: parse_addrs(&pending.fm.cc),
                            bcc: parse_addrs(&pending.fm.bcc),
                            subject: pending.fm.subject,
                            body_markdown: pending.body,
                            attachments: pending
                                .fm
                                .attach
                                .iter()
                                .map(std::path::PathBuf::from)
                                .collect(),
                            created_at: now,
                            updated_at: now,
                        };
                        self.pending_mutation_queue.push((
                            Request::SendDraft { draft },
                            MutationEffect::StatusOnly("Sent!".into()),
                        ));
                        self.status_message = Some("Sending...".into());
                        let _ = std::fs::remove_file(&pending.draft_path);
                    }
                    return None;
                }
                (KeyCode::Char('d'), KeyModifiers::NONE) => {
                    // Save as draft to mail server
                    if let Some(pending) = self.pending_send_confirm.take() {
                        if !pending.allow_send {
                            self.pending_send_confirm = Some(pending);
                            return None;
                        }
                        let parse_addrs = |s: &str| -> Vec<mxr_core::Address> {
                            s.split(',')
                                .map(|a| a.trim())
                                .filter(|a| !a.is_empty())
                                .map(|a| mxr_core::Address {
                                    name: None,
                                    email: a.to_string(),
                                })
                                .collect()
                        };
                        let account_id = self
                            .envelopes
                            .first()
                            .or(self.all_envelopes.first())
                            .map(|e| e.account_id.clone())
                            .unwrap_or_default();
                        let now = chrono::Utc::now();
                        let draft = mxr_core::Draft {
                            id: mxr_core::id::DraftId::new(),
                            account_id,
                            in_reply_to: None,
                            to: parse_addrs(&pending.fm.to),
                            cc: parse_addrs(&pending.fm.cc),
                            bcc: parse_addrs(&pending.fm.bcc),
                            subject: pending.fm.subject,
                            body_markdown: pending.body,
                            attachments: pending
                                .fm
                                .attach
                                .iter()
                                .map(std::path::PathBuf::from)
                                .collect(),
                            created_at: now,
                            updated_at: now,
                        };
                        self.pending_mutation_queue.push((
                            Request::SaveDraftToServer { draft },
                            MutationEffect::StatusOnly("Draft saved to server".into()),
                        ));
                        self.status_message = Some("Saving draft...".into());
                        let _ = std::fs::remove_file(&pending.draft_path);
                    }
                    return None;
                }
                (KeyCode::Char('e'), KeyModifiers::NONE) => {
                    // Edit again — reopen editor
                    if let Some(pending) = self.pending_send_confirm.take() {
                        self.pending_compose = Some(ComposeAction::EditDraft(pending.draft_path));
                    }
                    return None;
                }
                (KeyCode::Esc, _) => {
                    // Discard
                    if let Some(pending) = self.pending_send_confirm.take() {
                        let _ = std::fs::remove_file(&pending.draft_path);
                        self.status_message = Some("Discarded".into());
                    }
                    return None;
                }
                _ => return None,
            }
        }

        if self.pending_bulk_confirm.is_some() {
            return match (key.code, key.modifiers) {
                (KeyCode::Enter, _)
                | (KeyCode::Char('y'), KeyModifiers::NONE)
                | (KeyCode::Char('Y'), KeyModifiers::SHIFT) => Some(Action::OpenSelected),
                (KeyCode::Esc, _) | (KeyCode::Char('n'), KeyModifiers::NONE) => {
                    self.pending_bulk_confirm = None;
                    self.status_message = Some("Bulk action cancelled".into());
                    None
                }
                _ => None,
            };
        }

        if self.snooze_panel.visible {
            match (key.code, key.modifiers) {
                (KeyCode::Enter, _) => return Some(Action::Snooze),
                (KeyCode::Esc, _) => {
                    self.snooze_panel.visible = false;
                    return None;
                }
                (KeyCode::Char('j') | KeyCode::Down, _) => {
                    self.snooze_panel.selected_index =
                        (self.snooze_panel.selected_index + 1) % snooze_presets().len();
                    return None;
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    self.snooze_panel.selected_index = self
                        .snooze_panel
                        .selected_index
                        .checked_sub(1)
                        .unwrap_or(snooze_presets().len() - 1);
                    return None;
                }
                _ => return None,
            }
        }

        // Route keys to compose picker when active
        if self.attachment_panel.visible {
            match (key.code, key.modifiers) {
                (KeyCode::Enter | KeyCode::Char('o'), _) => {
                    self.queue_attachment_action(AttachmentOperation::Open);
                    return None;
                }
                (KeyCode::Char('d'), _) => {
                    self.queue_attachment_action(AttachmentOperation::Download);
                    return None;
                }
                (KeyCode::Char('j') | KeyCode::Down, _) => {
                    if self.attachment_panel.selected_index + 1 < self.attachment_panel.attachments.len() {
                        self.attachment_panel.selected_index += 1;
                    }
                    return None;
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    self.attachment_panel.selected_index =
                        self.attachment_panel.selected_index.saturating_sub(1);
                    return None;
                }
                (KeyCode::Esc | KeyCode::Char('A'), _) => {
                    self.close_attachment_panel();
                    return None;
                }
                _ => return None,
            }
        }

        // Route keys to compose picker when active
        if self.compose_picker.visible {
            match (key.code, key.modifiers) {
                (KeyCode::Enter, _) => {
                    // Confirm all recipients and trigger compose
                    let to = self.compose_picker.confirm();
                    if to.is_empty() {
                        self.pending_compose = Some(ComposeAction::New);
                    } else {
                        self.pending_compose = Some(ComposeAction::NewWithTo(to));
                    }
                    return None;
                }
                (KeyCode::Tab, _) => {
                    // Tab adds selected contact to recipients
                    self.compose_picker.add_recipient();
                    return None;
                }
                (KeyCode::Esc, _) => {
                    self.compose_picker.close();
                    return None;
                }
                (KeyCode::Backspace, _) => {
                    self.compose_picker.on_backspace();
                    return None;
                }
                (KeyCode::Down, _) | (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
                    self.compose_picker.select_next();
                    return None;
                }
                (KeyCode::Up, _) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                    self.compose_picker.select_prev();
                    return None;
                }
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    self.compose_picker.on_char(c);
                    return None;
                }
                _ => return None,
            }
        }

        // Route keys to label picker when active
        if self.label_picker.visible {
            match (key.code, key.modifiers) {
                (KeyCode::Enter, _) => {
                    let mode = self.label_picker.mode;
                    if let Some(label_name) = self.label_picker.confirm() {
                        self.pending_label_action = Some((mode, label_name));
                        return match mode {
                            LabelPickerMode::Apply => Some(Action::ApplyLabel),
                            LabelPickerMode::Move => Some(Action::MoveToLabel),
                        };
                    }
                    return None;
                }
                (KeyCode::Esc, _) => {
                    self.label_picker.close();
                    return None;
                }
                (KeyCode::Backspace, _) => {
                    self.label_picker.on_backspace();
                    return None;
                }
                (KeyCode::Down, _) | (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
                    self.label_picker.select_next();
                    return None;
                }
                (KeyCode::Up, _) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                    self.label_picker.select_prev();
                    return None;
                }
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    self.label_picker.on_char(c);
                    return None;
                }
                _ => return None,
            }
        }

        if self.screen != Screen::Mailbox {
            return self.handle_screen_key(key);
        }

        // Route keys based on active pane
        match self.active_pane {
            ActivePane::MessageView => match (key.code, key.modifiers) {
                (KeyCode::Char('j') | KeyCode::Down, _) => {
                    self.move_thread_focus_down();
                    None
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    self.move_thread_focus_up();
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
                // h = move left to mail list
                (KeyCode::Char('h') | KeyCode::Left, KeyModifiers::NONE) => {
                    self.active_pane = ActivePane::MailList;
                    None
                }
                // o = open in browser (message already open in pane)
                (KeyCode::Char('o'), KeyModifiers::NONE) => Some(Action::OpenInBrowser),
                _ => self.input.handle_key(key),
            },
            ActivePane::Sidebar => match (key.code, key.modifiers) {
                (KeyCode::Char('j') | KeyCode::Down, _) => {
                    self.sidebar_move_down();
                    None
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    self.sidebar_move_up();
                    None
                }
                (KeyCode::Enter | KeyCode::Char('o'), _) => self.sidebar_select(),
                // l = select label and move to mail list
                (KeyCode::Char('l') | KeyCode::Right, KeyModifiers::NONE) => self.sidebar_select(),
                _ => self.input.handle_key(key),
            },
            ActivePane::MailList => match (key.code, key.modifiers) {
                // h = move left to sidebar
                (KeyCode::Char('h') | KeyCode::Left, KeyModifiers::NONE) => {
                    self.active_pane = ActivePane::Sidebar;
                    None
                }
                // Right arrow opens selected message
                (KeyCode::Right, KeyModifiers::NONE) => {
                    Some(Action::OpenSelected)
                }
                _ => self.input.handle_key(key),
            },
        }
    }

    fn handle_screen_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
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
                    None
                }
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    self.search_page.query.push(c);
                    None
                }
                _ => None,
            };
        }

        match (key.code, key.modifiers) {
            (KeyCode::Char('/'), _) => {
                self.search_page.editing = true;
                None
            }
            (KeyCode::Char('j') | KeyCode::Down, _) => {
                if self.search_page.selected_index + 1 < self.search_row_count() {
                    self.search_page.selected_index += 1;
                    self.ensure_search_visible();
                    self.auto_preview_search();
                }
                None
            }
            (KeyCode::Char('k') | KeyCode::Up, _) => {
                if self.search_page.selected_index > 0 {
                    self.search_page.selected_index -= 1;
                    self.ensure_search_visible();
                    self.auto_preview_search();
                }
                None
            }
            (KeyCode::Enter | KeyCode::Char('o'), _) => {
                if let Some(env) = self.selected_search_envelope().cloned() {
                    self.open_envelope(env);
                    self.screen = Screen::Mailbox;
                    self.layout_mode = LayoutMode::ThreePane;
                    self.active_pane = ActivePane::MessageView;
                }
                None
            }
            (KeyCode::Esc, _) => Some(Action::OpenMailboxScreen),
            _ => self.input.handle_key(key),
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
                }
                None
            }
            (KeyCode::Char('k') | KeyCode::Up, _) => {
                self.rules_page.selected_index = self.rules_page.selected_index.saturating_sub(1);
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
            _ => self.input.handle_key(key),
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
                self.rules_page.form.active_field = self.rules_page.form.active_field.saturating_sub(1);
                None
            }
            (KeyCode::Enter, _) => Some(Action::SaveRuleForm),
            (KeyCode::Char(' '), _) if self.rules_page.form.active_field == 4 => {
                self.rules_page.form.enabled = !self.rules_page.form.enabled;
                None
            }
            (KeyCode::Backspace, _) => {
                match self.rules_page.form.active_field {
                    0 => { self.rules_page.form.name.pop(); }
                    1 => { self.rules_page.form.condition.pop(); }
                    2 => { self.rules_page.form.action.pop(); }
                    3 => { self.rules_page.form.priority.pop(); }
                    _ => {}
                }
                None
            }
            (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                match self.rules_page.form.active_field {
                    0 => self.rules_page.form.name.push(c),
                    1 => self.rules_page.form.condition.push(c),
                    2 => self.rules_page.form.action.push(c),
                    3 => self.rules_page.form.priority.push(c),
                    _ => {}
                }
                None
            }
            _ => None,
        }
    }

    fn handle_diagnostics_screen_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        match (key.code, key.modifiers) {
            (KeyCode::Char('r'), _) => Some(Action::RefreshDiagnostics),
            (KeyCode::Char('b'), _) => Some(Action::GenerateBugReport),
            (KeyCode::Esc, _) => Some(Action::OpenMailboxScreen),
            _ => self.input.handle_key(key),
        }
    }

    fn handle_accounts_screen_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
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
            (KeyCode::Char('d'), _) => Some(Action::SetDefaultAccount),
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
            (KeyCode::Esc, _) => Some(Action::OpenMailboxScreen),
            _ => self.input.handle_key(key),
        }
    }

    fn handle_account_form_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => {
                self.accounts_page.form.visible = false;
                None
            }
            (KeyCode::Tab, _) => {
                self.accounts_page.form.active_field =
                    (self.accounts_page.form.active_field + 1) % self.account_form_field_count();
                None
            }
            (KeyCode::BackTab, _) => {
                self.accounts_page.form.active_field =
                    self.accounts_page.form.active_field.saturating_sub(1);
                None
            }
            (KeyCode::Enter, _) => Some(Action::SaveAccountForm),
            (KeyCode::Char('t'), _) => Some(Action::TestAccountForm),
            (KeyCode::Char(' '), _) if self.accounts_page.form.active_field == 0 => {
                self.accounts_page.form.mode = match self.accounts_page.form.mode {
                    AccountFormMode::ImapSmtp => AccountFormMode::SmtpOnly,
                    AccountFormMode::SmtpOnly => AccountFormMode::ImapSmtp,
                };
                None
            }
            (KeyCode::Backspace, _) => {
                mutate_account_form_field(&mut self.accounts_page.form, |value| {
                    value.pop();
                });
                None
            }
            (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                mutate_account_form_field(&mut self.accounts_page.form, |value| value.push(c));
                None
            }
            _ => None,
        }
    }

    pub fn tick(&mut self) {
        self.input.check_timeout();
    }

    pub fn apply(&mut self, action: Action) {
        // Clear status message on any action
        self.status_message = None;

        match action {
            Action::OpenMailboxScreen => {
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
                self.screen = Screen::Accounts;
            }
            Action::SaveAccountForm => {
                let is_default = self
                    .selected_account()
                    .is_some_and(|account| account.is_default)
                    || self.accounts_page.accounts.is_empty();
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
                self.pending_account_test = Some(account);
                self.accounts_page.status = Some("Testing account...".into());
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
                self.search_bar.activate();
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
                self.envelopes = self.all_envelopes.clone();
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
                }
            }
            Action::GoToStarred => {
                if let Some(label) = self.labels.iter().find(|l| l.name == "STARRED") {
                    self.apply(Action::SelectLabel(label.id.clone()));
                }
            }
            Action::GoToSent => {
                if let Some(label) = self.labels.iter().find(|l| l.name == "SENT") {
                    self.apply(Action::SelectLabel(label.id.clone()));
                }
            }
            Action::GoToDrafts => {
                if let Some(label) = self.labels.iter().find(|l| l.name == "DRAFT") {
                    self.apply(Action::SelectLabel(label.id.clone()));
                }
            }
            Action::GoToAllMail => {
                self.apply(Action::ClearFilter);
            }
            Action::GoToLabel => {
                self.apply(Action::ClearFilter);
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
                if let Some(row) = self.selected_mail_row() {
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
                        self.rules_page.status =
                            Some(if enabled { "Disabling rule...".into() } else { "Enabling rule...".into() });
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
                self.pending_label_fetch = Some(label_id);
                self.pending_active_label = self.pending_label_fetch.clone();
                self.active_pane = ActivePane::MailList;
                self.screen = Screen::Mailbox;
            }
            Action::SelectSavedSearch(query) => {
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
                self.active_label = None;
                self.pending_active_label = None;
                self.search_active = false;
                self.envelopes = self.all_envelopes.clone();
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
                    let status = if starred { "Starring..." } else { "Unstarring..." };
                    self.queue_or_confirm_bulk_action(
                        if starred { "Star messages" } else { "Unstar messages" },
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
                    let id = env.id.clone();
                    self.pending_mutation_queue.push((
                        Request::Unsubscribe { message_id: id },
                        MutationEffect::StatusOnly("Unsubscribed".into()),
                    ));
                    self.status_message = Some("Unsubscribing...".into());
                }
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
                                wake_at.with_timezone(&chrono::Local).format("%a %b %e %H:%M")
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

    /// Returns the ordered list of visible labels (system first, then user, no separator).
    pub fn ordered_visible_labels(&self) -> Vec<&Label> {
        let mut system: Vec<&Label> = self
            .labels
            .iter()
            .filter(|l| !crate::ui::sidebar::should_hide_label(&l.name))
            .filter(|l| l.kind == mxr_core::types::LabelKind::System)
            .filter(|l| {
                crate::ui::sidebar::is_primary_system_label(&l.name)
                    || l.total_count > 0
                    || l.unread_count > 0
            })
            .collect();
        system.sort_by_key(|l| crate::ui::sidebar::system_label_order(&l.name));

        let mut user: Vec<&Label> = self
            .labels
            .iter()
            .filter(|l| !crate::ui::sidebar::should_hide_label(&l.name))
            .filter(|l| l.kind != mxr_core::types::LabelKind::System)
            .collect();
        user.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        let mut result = system;
        result.extend(user);
        result
    }

    /// Number of visible (non-hidden) labels.
    pub fn visible_label_count(&self) -> usize {
        self.ordered_visible_labels().len()
    }

    /// Get the visible (filtered) labels.
    pub fn visible_labels(&self) -> Vec<&Label> {
        self.ordered_visible_labels()
    }

    fn sidebar_move_down(&mut self) {
        if self.sidebar_selected + 1 < self.sidebar_items().len() {
            self.sidebar_selected += 1;
        }
        self.sync_sidebar_section();
    }

    fn sidebar_move_up(&mut self) {
        self.sidebar_selected = self.sidebar_selected.saturating_sub(1);
        self.sync_sidebar_section();
    }

    fn sidebar_select(&mut self) -> Option<Action> {
        match self.selected_sidebar_item() {
            Some(SidebarItem::Label(label)) => Some(Action::SelectLabel(label.id)),
            Some(SidebarItem::SavedSearch(search)) => Some(Action::SelectSavedSearch(search.query)),
            None => None,
        }
    }

    fn sync_sidebar_section(&mut self) {
        self.sidebar_section = match self.selected_sidebar_item() {
            Some(SidebarItem::SavedSearch(_)) => SidebarSection::SavedSearches,
            _ => SidebarSection::Labels,
        };
    }

    /// Live filter: instant client-side prefix matching on subject/from/snippet,
    /// plus async Tantivy search for full-text body matches.
    fn trigger_live_search(&mut self) {
        let query = self.search_bar.query.to_lowercase();
        if query.is_empty() {
            self.envelopes = self.all_envelopes.clone();
            self.search_active = false;
        } else {
            let query_words: Vec<&str> = query.split_whitespace().collect();
            // Instant client-side filter: every query word must prefix-match
            // some word in subject, from, or snippet
            self.envelopes = self
                .all_envelopes
                .iter()
                .filter(|e| {
                    let haystack = format!(
                        "{} {} {} {}",
                        e.subject,
                        e.from.email,
                        e.from.name.as_deref().unwrap_or(""),
                        e.snippet
                    )
                    .to_lowercase();
                    query_words.iter().all(|qw| {
                        haystack.split_whitespace().any(|hw| hw.starts_with(qw))
                            || haystack.contains(qw)
                    })
                })
                .cloned()
                .collect();
            self.search_active = true;
            // Also fire async Tantivy search to catch body matches
            self.pending_search = Some(self.search_bar.query.clone());
        }
        self.selected_index = 0;
        self.scroll_offset = 0;
    }

    /// Compute the mail list title based on active filter/search.
    pub fn mail_list_title(&self) -> String {
        let list_name = match self.mail_list_mode {
            MailListMode::Threads => "Threads",
            MailListMode::Messages => "Messages",
        };
        let list_count = self.mail_row_count();
        if self.search_active {
            format!("Search: {} ({list_count})", self.search_bar.query)
        } else if let Some(label_id) = self
            .pending_active_label
            .as_ref()
            .or(self.active_label.as_ref())
        {
            if let Some(label) = self.labels.iter().find(|l| &l.id == label_id) {
                let name = crate::ui::sidebar::humanize_label(&label.name);
                format!("{name} {list_name} ({list_count})")
            } else {
                format!("{list_name} ({list_count})")
            }
        } else {
            format!("{list_name} ({list_count})")
        }
    }

    /// In ThreePane mode, auto-load the preview for the currently selected envelope.
    fn auto_preview(&mut self) {
        if self.layout_mode == LayoutMode::ThreePane {
            if let Some(row) = self.selected_mail_row() {
                if self.viewing_envelope.as_ref().map(|e| &e.id)
                    != Some(&row.representative.id)
                {
                    self.open_envelope(row.representative);
                }
            }
        }
    }

    pub fn auto_preview_search(&mut self) {
        if let Some(env) = self.selected_search_envelope().cloned() {
            if self.viewing_envelope.as_ref().map(|current| current.id.clone()) != Some(env.id.clone()) {
                self.open_envelope(env);
            }
        }
    }

    fn ensure_search_visible(&mut self) {
        let h = self.visible_height.max(1);
        if self.search_page.selected_index < self.search_page.scroll_offset {
            self.search_page.scroll_offset = self.search_page.selected_index;
        } else if self.search_page.selected_index >= self.search_page.scroll_offset + h {
            self.search_page.scroll_offset = self.search_page.selected_index + 1 - h;
        }
    }

    /// Queue body prefetch for messages around the current cursor position.
    /// Only fetches bodies not already in cache.
    pub fn queue_body_window(&mut self) {
        const BUFFER: usize = 50;
        let len = self.envelopes.len();
        if len == 0 {
            return;
        }
        let start = self.selected_index.saturating_sub(BUFFER / 2);
        let end = (self.selected_index + BUFFER / 2).min(len);
        let ids: Vec<MessageId> = self.envelopes[start..end]
            .iter()
            .map(|e| e.id.clone())
            .collect();
        for id in ids {
            self.queue_body_fetch(id);
        }
    }

    fn open_envelope(&mut self, env: Envelope) {
        self.close_attachment_panel();
        self.viewed_thread = None;
        self.viewed_thread_messages = self.optimistic_thread_messages(&env);
        self.thread_selected_index = self.default_thread_selected_index();
        self.viewing_envelope = self.focused_thread_envelope().cloned();
        for message in self.viewed_thread_messages.clone() {
            self.queue_body_fetch(message.id);
        }
        self.queue_thread_fetch(env.thread_id.clone());
        self.message_scroll_offset = 0;
        self.ensure_current_body_state();
    }

    fn optimistic_thread_messages(&self, env: &Envelope) -> Vec<Envelope> {
        let mut messages: Vec<Envelope> = self
            .all_envelopes
            .iter()
            .filter(|candidate| candidate.thread_id == env.thread_id)
            .cloned()
            .collect();
        if messages.is_empty() {
            messages.push(env.clone());
        }
        messages.sort_by_key(|message| message.date);
        messages
    }

    fn default_thread_selected_index(&self) -> usize {
        self.viewed_thread_messages
            .iter()
            .rposition(|message| !message.flags.contains(MessageFlags::READ))
            .or_else(|| self.viewed_thread_messages.len().checked_sub(1))
            .unwrap_or(0)
    }

    fn sync_focused_thread_envelope(&mut self) {
        self.close_attachment_panel();
        self.viewing_envelope = self.focused_thread_envelope().cloned();
        self.message_scroll_offset = 0;
        self.ensure_current_body_state();
    }

    fn move_thread_focus_down(&mut self) {
        if self.thread_selected_index + 1 < self.viewed_thread_messages.len() {
            self.thread_selected_index += 1;
            self.sync_focused_thread_envelope();
        }
    }

    fn move_thread_focus_up(&mut self) {
        if self.thread_selected_index > 0 {
            self.thread_selected_index -= 1;
            self.sync_focused_thread_envelope();
        }
    }

    fn ensure_current_body_state(&mut self) {
        if let Some(env) = self.viewing_envelope.clone() {
            if !self.body_cache.contains_key(&env.id) {
                self.queue_body_fetch(env.id.clone());
            }
            self.body_view_state = self.resolve_body_view_state(&env);
        } else {
            self.body_view_state = BodyViewState::Empty { preview: None };
        }
    }

    fn queue_body_fetch(&mut self, message_id: MessageId) {
        if self.body_cache.contains_key(&message_id)
            || self.in_flight_body_requests.contains(&message_id)
            || self.queued_body_fetches.contains(&message_id)
        {
            return;
        }

        self.in_flight_body_requests.insert(message_id.clone());
        self.queued_body_fetches.push(message_id);
    }

    fn queue_thread_fetch(&mut self, thread_id: mxr_core::ThreadId) {
        if self.pending_thread_fetch.as_ref() == Some(&thread_id)
            || self.in_flight_thread_fetch.as_ref() == Some(&thread_id)
        {
            return;
        }
        self.pending_thread_fetch = Some(thread_id);
    }

    fn envelope_preview(envelope: &Envelope) -> Option<String> {
        let snippet = envelope.snippet.trim();
        if snippet.is_empty() {
            None
        } else {
            Some(envelope.snippet.clone())
        }
    }

    fn render_body(raw: &str, source: BodySource, reader_mode: bool) -> String {
        if !reader_mode {
            return raw.to_string();
        }

        let config = mxr_reader::ReaderConfig::default();
        match source {
            BodySource::Plain => mxr_reader::clean(Some(raw), None, &config).content,
            BodySource::Html => mxr_reader::clean(None, Some(raw), &config).content,
            BodySource::Snippet => raw.to_string(),
        }
    }

    fn resolve_body_view_state(&self, envelope: &Envelope) -> BodyViewState {
        let preview = Self::envelope_preview(envelope);

        if let Some(body) = self.body_cache.get(&envelope.id) {
            if let Some(raw) = body.text_plain.clone() {
                let rendered = Self::render_body(&raw, BodySource::Plain, self.reader_mode);
                return BodyViewState::Ready {
                    raw,
                    rendered,
                    source: BodySource::Plain,
                };
            }

            if let Some(raw) = body.text_html.clone() {
                let rendered = Self::render_body(&raw, BodySource::Html, self.reader_mode);
                return BodyViewState::Ready {
                    raw,
                    rendered,
                    source: BodySource::Html,
                };
            }

            return BodyViewState::Empty { preview };
        }

        if self.in_flight_body_requests.contains(&envelope.id) {
            BodyViewState::Loading { preview }
        } else {
            BodyViewState::Empty { preview }
        }
    }

    pub fn resolve_body_success(&mut self, body: MessageBody) {
        let message_id = body.message_id.clone();
        self.in_flight_body_requests.remove(&message_id);
        self.body_cache.insert(message_id.clone(), body);

        if self.viewing_envelope.as_ref().map(|env| env.id.clone()) == Some(message_id) {
            self.ensure_current_body_state();
        }
    }

    pub fn resolve_body_fetch_error(&mut self, message_id: &MessageId, message: String) {
        self.in_flight_body_requests.remove(message_id);

        if let Some(env) = self.viewing_envelope.as_ref().filter(|env| &env.id == message_id) {
            self.body_view_state = BodyViewState::Error {
                message,
                preview: Self::envelope_preview(env),
            };
        }
    }

    pub fn current_viewing_body(&self) -> Option<&MessageBody> {
        self.viewing_envelope
            .as_ref()
            .and_then(|env| self.body_cache.get(&env.id))
    }

    pub fn selected_attachment(&self) -> Option<&AttachmentMeta> {
        self.attachment_panel
            .attachments
            .get(self.attachment_panel.selected_index)
    }

    pub fn open_attachment_panel(&mut self) {
        let Some(message_id) = self.viewing_envelope.as_ref().map(|env| env.id.clone()) else {
            self.status_message = Some("No message selected".into());
            return;
        };
        let Some(attachments) = self
            .current_viewing_body()
            .map(|body| body.attachments.clone())
        else {
            self.status_message = Some("No message body loaded".into());
            return;
        };
        if attachments.is_empty() {
            self.status_message = Some("No attachments".into());
            return;
        }

        self.attachment_panel.visible = true;
        self.attachment_panel.message_id = Some(message_id);
        self.attachment_panel.attachments = attachments;
        self.attachment_panel.selected_index = 0;
        self.attachment_panel.status = None;
    }

    pub fn close_attachment_panel(&mut self) {
        self.attachment_panel = AttachmentPanelState::default();
        self.pending_attachment_action = None;
    }

    pub fn queue_attachment_action(&mut self, operation: AttachmentOperation) {
        let Some(message_id) = self.attachment_panel.message_id.clone() else {
            return;
        };
        let Some(attachment) = self.selected_attachment().cloned() else {
            return;
        };

        self.attachment_panel.status = Some(match operation {
            AttachmentOperation::Open => format!("Opening {}...", attachment.filename),
            AttachmentOperation::Download => format!("Downloading {}...", attachment.filename),
        });
        self.pending_attachment_action = Some(PendingAttachmentAction {
            message_id,
            attachment_id: attachment.id,
            operation,
        });
    }

    pub fn resolve_attachment_file(&mut self, file: &mxr_protocol::AttachmentFile) {
        let path = std::path::PathBuf::from(&file.path);
        for attachment in &mut self.attachment_panel.attachments {
            if attachment.id == file.attachment_id {
                attachment.local_path = Some(path.clone());
            }
        }
        for body in self.body_cache.values_mut() {
            for attachment in &mut body.attachments {
                if attachment.id == file.attachment_id {
                    attachment.local_path = Some(path.clone());
                }
            }
        }
    }

    fn label_chips_for_envelope(&self, envelope: &Envelope) -> Vec<String> {
        envelope
            .label_provider_ids
            .iter()
            .filter_map(|provider_id| {
                self.labels
                    .iter()
                    .find(|label| &label.provider_id == provider_id)
                    .map(|label| crate::ui::sidebar::humanize_label(&label.name).to_string())
            })
            .collect()
    }

    fn attachment_chips_for_envelope(&self, envelope: &Envelope) -> Vec<String> {
        self.body_cache
            .get(&envelope.id)
            .map(|body| {
                body.attachments
                    .iter()
                    .map(|attachment| attachment.filename.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn apply_local_label_refs(
        &mut self,
        message_ids: &[MessageId],
        add: &[String],
        remove: &[String],
    ) {
        let add_provider_ids = self.resolve_label_provider_ids(add);
        let remove_provider_ids = self.resolve_label_provider_ids(remove);
        for envelope in self
            .envelopes
            .iter_mut()
            .chain(self.all_envelopes.iter_mut())
            .chain(self.search_page.results.iter_mut())
            .chain(self.viewed_thread_messages.iter_mut())
        {
            if message_ids.iter().any(|message_id| message_id == &envelope.id) {
                apply_provider_label_changes(
                    &mut envelope.label_provider_ids,
                    &add_provider_ids,
                    &remove_provider_ids,
                );
            }
        }
        if let Some(ref mut envelope) = self.viewing_envelope {
            if message_ids.iter().any(|message_id| message_id == &envelope.id) {
                apply_provider_label_changes(
                    &mut envelope.label_provider_ids,
                    &add_provider_ids,
                    &remove_provider_ids,
                );
            }
        }
    }

    fn resolve_label_provider_ids(&self, refs: &[String]) -> Vec<String> {
        refs.iter()
            .filter_map(|label_ref| {
                self.labels
                    .iter()
                    .find(|label| label.provider_id == *label_ref || label.name == *label_ref)
                    .map(|label| label.provider_id.clone())
                    .or_else(|| Some(label_ref.clone()))
            })
            .collect()
    }

    pub fn resolve_thread_success(&mut self, thread: Thread, mut messages: Vec<Envelope>) {
        let thread_id = thread.id.clone();
        self.in_flight_thread_fetch = None;
        messages.sort_by_key(|message| message.date);

        if self.viewing_envelope.as_ref().map(|env| env.thread_id.clone()) == Some(thread_id) {
            let focused_message_id = self.focused_thread_envelope().map(|env| env.id.clone());
            for message in &messages {
                self.queue_body_fetch(message.id.clone());
            }
            self.viewed_thread = Some(thread);
            self.viewed_thread_messages = messages;
            self.thread_selected_index = focused_message_id
                .and_then(|message_id| {
                    self.viewed_thread_messages
                        .iter()
                        .position(|message| message.id == message_id)
                })
                .unwrap_or_else(|| self.default_thread_selected_index());
            self.sync_focused_thread_envelope();
        }
    }

    pub fn resolve_thread_fetch_error(&mut self, thread_id: &mxr_core::ThreadId) {
        if self.in_flight_thread_fetch.as_ref() == Some(thread_id) {
            self.in_flight_thread_fetch = None;
        }
    }

    /// Get IDs to mutate: selected_set if non-empty, else context_envelope.
    fn mutation_target_ids(&self) -> Vec<MessageId> {
        if !self.selected_set.is_empty() {
            self.selected_set.iter().cloned().collect()
        } else if let Some(env) = self.context_envelope() {
            vec![env.id.clone()]
        } else {
            vec![]
        }
    }

    fn clear_selection(&mut self) {
        self.selected_set.clear();
        self.visual_mode = false;
        self.visual_anchor = None;
    }

    fn queue_or_confirm_bulk_action(
        &mut self,
        title: impl Into<String>,
        detail: impl Into<String>,
        request: Request,
        effect: MutationEffect,
        status_message: String,
        count: usize,
    ) {
        if count > 1 {
            self.pending_bulk_confirm = Some(PendingBulkConfirm {
                title: title.into(),
                detail: detail.into(),
                request,
                effect,
                status_message,
            });
        } else {
            self.pending_mutation_queue.push((request, effect));
            self.status_message = Some(status_message);
            self.clear_selection();
        }
    }

    /// Update visual selection range when moving in visual mode.
    fn update_visual_selection(&mut self) {
        if self.visual_mode {
            if let Some(anchor) = self.visual_anchor {
                let start = anchor.min(self.selected_index);
                let end = anchor.max(self.selected_index);
                self.selected_set.clear();
                for env in self.envelopes.iter().skip(start).take(end - start + 1) {
                    self.selected_set.insert(env.id.clone());
                }
            }
        }
    }

    /// Ensure selected_index is visible within the scroll viewport.
    fn ensure_visible(&mut self) {
        let h = self.visible_height.max(1);
        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        } else if self.selected_index >= self.scroll_offset + h {
            self.scroll_offset = self.selected_index + 1 - h;
        }
        // Prefetch bodies for messages near the cursor
        self.queue_body_window();
    }

    pub fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();

        // Layout: hint bar (1 line) | content | status bar (1 line)
        let outer_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // hint bar
                Constraint::Min(0),    // content
                Constraint::Length(1), // status bar
            ])
            .split(area);

        let hint_bar_area = outer_chunks[0];
        let content_area = outer_chunks[1];
        // Update visible height based on actual terminal size (subtract borders/header)
        self.visible_height = content_area.height.saturating_sub(2) as usize;
        let bottom_bar_area = outer_chunks[2];

        // Hint bar
        ui::hint_bar::draw(
            frame,
            hint_bar_area,
            ui::hint_bar::HintBarState {
                screen: self.screen,
                active_pane: &self.active_pane,
                search_active: self.search_bar.active,
                help_modal_open: self.help_modal_open,
                selected_count: self.selected_set.len(),
                bulk_confirm_open: self.pending_bulk_confirm.is_some(),
            },
        );

        match self.screen {
            Screen::Mailbox => match self.layout_mode {
            LayoutMode::TwoPane => {
                let chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(20), Constraint::Percentage(80)])
                    .split(content_area);

                ui::sidebar::draw(
                    frame,
                    chunks[0],
                    &ui::sidebar::SidebarView {
                        labels: &self.labels,
                        active_pane: &self.active_pane,
                        saved_searches: &self.saved_searches,
                        sidebar_selected: self.sidebar_selected,
                        active_label: self
                            .pending_active_label
                            .as_ref()
                            .or(self.active_label.as_ref()),
                    },
                );

                let mail_title = self.mail_list_title();
                ui::mail_list::draw_view(
                    frame,
                    chunks[1],
                    &ui::mail_list::MailListView {
                        rows: &self.mail_list_rows(),
                        selected_index: self.selected_index,
                        scroll_offset: self.scroll_offset,
                        active_pane: &self.active_pane,
                        title: &mail_title,
                        selected_set: &self.selected_set,
                        mode: self.mail_list_mode,
                    },
                );
            }
            LayoutMode::ThreePane => {
                let chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Percentage(15),
                        Constraint::Percentage(35),
                        Constraint::Percentage(50),
                    ])
                    .split(content_area);

                ui::sidebar::draw(
                    frame,
                    chunks[0],
                    &ui::sidebar::SidebarView {
                        labels: &self.labels,
                        active_pane: &self.active_pane,
                        saved_searches: &self.saved_searches,
                        sidebar_selected: self.sidebar_selected,
                        active_label: self
                            .pending_active_label
                            .as_ref()
                            .or(self.active_label.as_ref()),
                    },
                );

                let mail_title = self.mail_list_title();
                ui::mail_list::draw_view(
                    frame,
                    chunks[1],
                    &ui::mail_list::MailListView {
                        rows: &self.mail_list_rows(),
                        selected_index: self.selected_index,
                        scroll_offset: self.scroll_offset,
                        active_pane: &self.active_pane,
                        title: &mail_title,
                        selected_set: &self.selected_set,
                        mode: self.mail_list_mode,
                    },
                );
                ui::message_view::draw(
                    frame,
                    chunks[2],
                    &self
                        .viewed_thread_messages
                        .iter()
                        .map(|message| ui::message_view::ThreadMessageBlock {
                            envelope: message.clone(),
                            body_state: self.resolve_body_view_state(message),
                            labels: self.label_chips_for_envelope(message),
                            attachments: self.attachment_chips_for_envelope(message),
                            selected: self.viewing_envelope.as_ref().map(|env| env.id.clone())
                                == Some(message.id.clone()),
                        })
                        .collect::<Vec<_>>(),
                    self.message_scroll_offset,
                    &self.active_pane,
                );
            }
            LayoutMode::FullScreen => {
                ui::message_view::draw(
                    frame,
                    content_area,
                    &self
                        .viewed_thread_messages
                        .iter()
                        .map(|message| ui::message_view::ThreadMessageBlock {
                            envelope: message.clone(),
                            body_state: self.resolve_body_view_state(message),
                            labels: self.label_chips_for_envelope(message),
                            attachments: self.attachment_chips_for_envelope(message),
                            selected: self.viewing_envelope.as_ref().map(|env| env.id.clone())
                                == Some(message.id.clone()),
                        })
                        .collect::<Vec<_>>(),
                    self.message_scroll_offset,
                    &self.active_pane,
                );
            }
        },
            Screen::Search => {
                let rows = self.search_mail_list_rows();
                ui::search_page::draw(
                    frame,
                    content_area,
                    &self.search_page,
                    &rows,
                    self.mail_list_mode,
                    &self
                        .viewed_thread_messages
                        .iter()
                        .map(|message| ui::message_view::ThreadMessageBlock {
                            envelope: message.clone(),
                            body_state: self.resolve_body_view_state(message),
                            labels: self.label_chips_for_envelope(message),
                            attachments: self.attachment_chips_for_envelope(message),
                            selected: self.viewing_envelope.as_ref().map(|env| env.id.clone())
                                == Some(message.id.clone()),
                        })
                        .collect::<Vec<_>>(),
                    self.message_scroll_offset,
                );
            }
            Screen::Rules => {
                ui::rules_page::draw(frame, content_area, &self.rules_page);
            }
            Screen::Diagnostics => {
                ui::diagnostics_page::draw(frame, content_area, &self.diagnostics_page);
            }
            Screen::Accounts => {
                ui::accounts_page::draw(frame, content_area, &self.accounts_page);
            }
        }

        // Bottom bar: search bar takes priority over status bar
        if self.search_bar.active {
            ui::search_bar::draw(frame, bottom_bar_area, &self.search_bar);
        } else {
            ui::status_bar::draw(
                frame,
                bottom_bar_area,
                &self.envelopes,
                self.last_sync_status.as_deref(),
                self.status_message.as_deref(),
            );
        }

        // Command palette overlay
        ui::command_palette::draw(frame, area, &self.command_palette);

        // Label picker overlay
        ui::label_picker::draw(frame, area, &self.label_picker);

        // Compose picker overlay
        ui::compose_picker::draw(frame, area, &self.compose_picker);

        // Attachment overlay
        ui::attachment_modal::draw(frame, area, &self.attachment_panel);

        // Snooze overlay
        ui::snooze_modal::draw(frame, area, &self.snooze_panel, &self.snooze_config);

        // Send confirmation overlay
        ui::send_confirm_modal::draw(frame, area, self.pending_send_confirm.as_ref());

        // Bulk confirmation overlay
        ui::bulk_confirm_modal::draw(frame, area, self.pending_bulk_confirm.as_ref());

        // Help overlay
        ui::help_modal::draw(
            frame,
            area,
            ui::help_modal::HelpModalState {
                open: self.help_modal_open,
                screen: self.screen,
                active_pane: &self.active_pane,
                selected_count: self.selected_set.len(),
                scroll_offset: self.help_scroll_offset,
            },
        );
    }
}

fn apply_provider_label_changes(
    label_provider_ids: &mut Vec<String>,
    add_provider_ids: &[String],
    remove_provider_ids: &[String],
) {
    label_provider_ids.retain(|provider_id| !remove_provider_ids.iter().any(|remove| remove == provider_id));
    for provider_id in add_provider_ids {
        if !label_provider_ids.iter().any(|existing| existing == provider_id) {
            label_provider_ids.push(provider_id.clone());
        }
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

fn account_summary_to_config(
    account: &mxr_protocol::AccountSummaryData,
) -> Option<mxr_protocol::AccountConfigData> {
    Some(mxr_protocol::AccountConfigData {
        key: account.key.clone()?,
        name: account.name.clone(),
        email: account.email.clone(),
        sync: account.sync.clone(),
        send: account.send.clone(),
        is_default: account.is_default,
    })
}

fn account_form_from_config(account: mxr_protocol::AccountConfigData) -> AccountFormState {
    let mut form = AccountFormState {
        visible: true,
        key: account.key,
        name: account.name,
        email: account.email,
        ..AccountFormState::default()
    };

    if let Some(sync) = account.sync {
        match sync {
            mxr_protocol::AccountSyncConfigData::Imap {
                host,
                port,
                username,
                password_ref,
                ..
            } => {
                form.mode = AccountFormMode::ImapSmtp;
                form.imap_host = host;
                form.imap_port = port.to_string();
                form.imap_username = username;
                form.imap_password_ref = password_ref;
            }
            mxr_protocol::AccountSyncConfigData::Gmail { .. } => {}
        }
    } else {
        form.mode = AccountFormMode::SmtpOnly;
    }

    if let Some(mxr_protocol::AccountSendConfigData::Smtp {
        host,
        port,
        username,
        password_ref,
        ..
    }) = account.send
    {
        form.smtp_host = host;
        form.smtp_port = port.to_string();
        form.smtp_username = username;
        form.smtp_password_ref = password_ref;
    }

    form
}

fn mutate_account_form_field<F>(form: &mut AccountFormState, mut update: F)
where
    F: FnMut(&mut String),
{
    let field = match (form.mode, form.active_field) {
        (_, 0) => return,
        (_, 1) => &mut form.key,
        (_, 2) => &mut form.name,
        (_, 3) => &mut form.email,
        (AccountFormMode::ImapSmtp, 4) => &mut form.imap_host,
        (AccountFormMode::ImapSmtp, 5) => &mut form.imap_port,
        (AccountFormMode::ImapSmtp, 6) => &mut form.imap_username,
        (AccountFormMode::ImapSmtp, 7) => &mut form.imap_password_ref,
        (AccountFormMode::ImapSmtp, 8) => &mut form.imap_password,
        (AccountFormMode::ImapSmtp, 9) => &mut form.smtp_host,
        (AccountFormMode::ImapSmtp, 10) => &mut form.smtp_port,
        (AccountFormMode::ImapSmtp, 11) => &mut form.smtp_username,
        (AccountFormMode::ImapSmtp, 12) => &mut form.smtp_password_ref,
        (AccountFormMode::ImapSmtp, 13) => &mut form.smtp_password,
        (AccountFormMode::SmtpOnly, 4) => &mut form.smtp_host,
        (AccountFormMode::SmtpOnly, 5) => &mut form.smtp_port,
        (AccountFormMode::SmtpOnly, 6) => &mut form.smtp_username,
        (AccountFormMode::SmtpOnly, 7) => &mut form.smtp_password_ref,
        (AccountFormMode::SmtpOnly, 8) => &mut form.smtp_password,
        _ => &mut form.smtp_password,
    };
    update(field);
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
    use chrono::{Datelike, Duration, Local, NaiveTime, Weekday};

    let now = Local::now();
    match preset {
        SnoozePreset::TomorrowMorning => {
            let tomorrow = now.date_naive() + Duration::days(1);
            let time = NaiveTime::from_hms_opt(config.morning_hour as u32, 0, 0).unwrap();
            tomorrow
                .and_time(time)
                .and_local_timezone(now.timezone())
                .single()
                .unwrap()
                .with_timezone(&chrono::Utc)
        }
        SnoozePreset::Tonight => {
            let today = now.date_naive();
            let time = NaiveTime::from_hms_opt(config.evening_hour as u32, 0, 0).unwrap();
            let tonight = today
                .and_time(time)
                .and_local_timezone(now.timezone())
                .single()
                .unwrap()
                .with_timezone(&chrono::Utc);
            if tonight <= chrono::Utc::now() {
                tonight + Duration::days(1)
            } else {
                tonight
            }
        }
        SnoozePreset::Weekend => {
            let target_day = match config.weekend_day.as_str() {
                "sunday" => Weekday::Sun,
                _ => Weekday::Sat,
            };
            let days_until = (target_day.num_days_from_monday() as i64
                - now.weekday().num_days_from_monday() as i64
                + 7)
                % 7;
            let days = if days_until == 0 { 7 } else { days_until };
            let weekend = now.date_naive() + Duration::days(days);
            let time = NaiveTime::from_hms_opt(config.weekend_hour as u32, 0, 0).unwrap();
            weekend
                .and_time(time)
                .and_local_timezone(now.timezone())
                .single()
                .unwrap()
                .with_timezone(&chrono::Utc)
        }
        SnoozePreset::NextMonday => {
            let days_until_monday = (Weekday::Mon.num_days_from_monday() as i64
                - now.weekday().num_days_from_monday() as i64
                + 7)
                % 7;
            let days = if days_until_monday == 0 {
                7
            } else {
                days_until_monday
            };
            let monday = now.date_naive() + Duration::days(days);
            let time = NaiveTime::from_hms_opt(config.morning_hour as u32, 0, 0).unwrap();
            monday
                .and_time(time)
                .and_local_timezone(now.timezone())
                .single()
                .unwrap()
                .with_timezone(&chrono::Utc)
        }
    }
}
