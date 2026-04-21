mod actions;
mod draw;
mod input;
use crate::action::{Action, PatternKind, ScreenContext, UiContext};
use crate::async_result::SearchResultData;
use crate::client::Client;
use crate::input::InputHandler;
use crate::terminal_images::{HtmlImageEntry, HtmlImageKey, TerminalImageSupport};
use crate::theme::Theme;
use crate::ui;
use crate::ui::command_palette::CommandPalette;
use crate::ui::compose_picker::ComposePicker;
use crate::ui::label_picker::{LabelPicker, LabelPickerMode};
use crate::ui::search_bar::SearchBar;
use mxr_config::RenderConfig;
use mxr_core::id::{AccountId, AttachmentId, MessageId};
use mxr_core::types::*;
use mxr_core::MxrError;
use mxr_protocol::{MutationCommand, Request, Response, ResponseData};
use ratatui::crossterm::event::{KeyCode, KeyModifiers};
use ratatui::prelude::*;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use throbber_widgets_tui::ThrobberState;
use tui_textarea::TextArea;

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

/// Draft waiting for user confirmation after editor closes.
pub struct PendingSend {
    pub fm: mxr_compose::frontmatter::ComposeFrontmatter,
    pub body: String,
    pub draft_path: std::path::PathBuf,
    pub mode: PendingSendMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingBrowserOpen {
    pub message_id: MessageId,
    pub document: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingSendMode {
    SendOrSave,
    DraftOnlyNoRecipients,
    Unchanged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposeAction {
    New { to: String, subject: String },
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchPane {
    #[default]
    Results,
    Preview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchUiStatus {
    #[default]
    Idle,
    Debouncing,
    Searching,
    LoadingMore,
    Loaded,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailListMode {
    Threads,
    Messages,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailboxView {
    Messages,
    Subscriptions,
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
    Fallback,
    Snippet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BodyViewMode {
    #[default]
    Text,
    Html,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BodyViewMetadata {
    pub mode: BodyViewMode,
    pub provenance: Option<BodyPartSource>,
    pub reader_applied: bool,
    pub flowed: bool,
    pub inline_images: bool,
    pub remote_content_available: bool,
    pub remote_content_enabled: bool,
    pub original_lines: Option<usize>,
    pub cleaned_lines: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BodyViewState {
    Loading {
        preview: Option<String>,
    },
    Ready {
        raw: String,
        rendered: String,
        source: BodySource,
        metadata: BodyViewMetadata,
    },
    Empty {
        preview: Option<String>,
    },
    Error {
        message: String,
        preview: Option<String>,
    },
}

#[derive(Debug, Clone)]
pub struct MailListRow {
    pub thread_id: mxr_core::ThreadId,
    pub representative: Envelope,
    pub message_count: usize,
    pub unread_count: usize,
}

#[derive(Debug, Clone)]
pub struct SubscriptionEntry {
    pub summary: SubscriptionSummary,
    pub envelope: Envelope,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum SidebarItem {
    Account(mxr_protocol::AccountSummaryData),
    AllMail,
    Subscriptions,
    Label(Label),
    SavedSearch(mxr_core::SavedSearch),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SidebarSelectionKey {
    Account(String),
    AllMail,
    Subscriptions,
    Label(mxr_core::LabelId),
    SavedSearch(String),
}

#[derive(Debug, Clone, Default)]
pub struct SubscriptionsPageState {
    pub entries: Vec<SubscriptionEntry>,
}

#[derive(Debug, Clone)]
pub struct SearchPageState {
    pub query: String,
    pub editing: bool,
    pub results: Vec<Envelope>,
    pub scores: HashMap<MessageId, f32>,
    pub mode: SearchMode,
    pub sort: SortOrder,
    pub has_more: bool,
    pub loading_more: bool,
    pub total_count: Option<u32>,
    pub count_pending: bool,
    pub ui_status: SearchUiStatus,
    pub session_active: bool,
    pub load_to_end: bool,
    pub session_id: u64,
    pub active_pane: SearchPane,
    pub selected_index: usize,
    pub scroll_offset: usize,
    pub result_selected: bool,
    pub throbber: ThrobberState,
}

impl SearchPageState {
    pub fn has_session(&self) -> bool {
        self.session_active || !self.results.is_empty()
    }
}

impl Default for SearchPageState {
    fn default() -> Self {
        Self {
            query: String::new(),
            editing: false,
            results: Vec::new(),
            scores: HashMap::new(),
            mode: SearchMode::Lexical,
            sort: SortOrder::DateDesc,
            has_more: false,
            loading_more: false,
            total_count: None,
            count_pending: false,
            ui_status: SearchUiStatus::Idle,
            session_active: false,
            load_to_end: false,
            session_id: 0,
            active_pane: SearchPane::Results,
            selected_index: 0,
            scroll_offset: 0,
            result_selected: false,
            throbber: ThrobberState::default(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FeatureOnboardingState {
    pub visible: bool,
    pub step: usize,
    pub seen: bool,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiagnosticsPaneKind {
    #[default]
    Status,
    Data,
    Sync,
    Events,
    Logs,
}

impl DiagnosticsPaneKind {
    pub fn next(self) -> Self {
        match self {
            Self::Status => Self::Data,
            Self::Data => Self::Sync,
            Self::Sync => Self::Events,
            Self::Events => Self::Logs,
            Self::Logs => Self::Status,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Status => Self::Logs,
            Self::Data => Self::Status,
            Self::Sync => Self::Data,
            Self::Events => Self::Sync,
            Self::Logs => Self::Events,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DiagnosticsPageState {
    pub uptime_secs: Option<u64>,
    pub daemon_pid: Option<u32>,
    pub accounts: Vec<String>,
    pub total_messages: Option<u32>,
    pub sync_statuses: Vec<mxr_protocol::AccountSyncStatus>,
    pub doctor: Option<mxr_protocol::DoctorReport>,
    pub events: Vec<mxr_protocol::EventLogEntry>,
    pub logs: Vec<String>,
    pub status: Option<String>,
    pub refresh_pending: bool,
    pub pending_requests: u8,
    pub selected_pane: DiagnosticsPaneKind,
    pub fullscreen_pane: Option<DiagnosticsPaneKind>,
    pub status_scroll_offset: u16,
    pub data_scroll_offset: u16,
    pub sync_scroll_offset: u16,
    pub events_scroll_offset: u16,
    pub logs_scroll_offset: u16,
}

impl DiagnosticsPageState {
    pub fn active_pane(&self) -> DiagnosticsPaneKind {
        self.fullscreen_pane.unwrap_or(self.selected_pane)
    }

    pub fn toggle_fullscreen(&mut self) {
        self.fullscreen_pane = match self.fullscreen_pane {
            Some(pane) if pane == self.selected_pane => None,
            _ => Some(self.selected_pane),
        };
    }

    pub fn scroll_offset(&self, pane: DiagnosticsPaneKind) -> u16 {
        match pane {
            DiagnosticsPaneKind::Status => self.status_scroll_offset,
            DiagnosticsPaneKind::Data => self.data_scroll_offset,
            DiagnosticsPaneKind::Sync => self.sync_scroll_offset,
            DiagnosticsPaneKind::Events => self.events_scroll_offset,
            DiagnosticsPaneKind::Logs => self.logs_scroll_offset,
        }
    }

    pub fn scroll_offset_mut(&mut self, pane: DiagnosticsPaneKind) -> &mut u16 {
        match pane {
            DiagnosticsPaneKind::Status => &mut self.status_scroll_offset,
            DiagnosticsPaneKind::Data => &mut self.data_scroll_offset,
            DiagnosticsPaneKind::Sync => &mut self.sync_scroll_offset,
            DiagnosticsPaneKind::Events => &mut self.events_scroll_offset,
            DiagnosticsPaneKind::Logs => &mut self.logs_scroll_offset,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountFormMode {
    Gmail,
    ImapSmtp,
    SmtpOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AccountFormToggleField {
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

pub use mxr_config::snooze::{SnoozeOption as SnoozePreset, SNOOZE_PRESETS};

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
    pub optimistic_effect: Option<MutationEffect>,
    pub status_message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorModalState {
    pub title: String,
    pub detail: String,
    pub scroll_offset: usize,
}

impl ErrorModalState {
    pub fn new(title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            detail: detail.into(),
            scroll_offset: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentSummary {
    pub filename: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct PendingUnsubscribeConfirm {
    pub message_id: MessageId,
    pub account_id: AccountId,
    pub sender_email: String,
    pub method_label: String,
    pub archive_message_ids: Vec<MessageId>,
}

#[derive(Debug, Clone)]
pub struct PendingUnsubscribeAction {
    pub message_id: MessageId,
    pub archive_message_ids: Vec<MessageId>,
    pub sender_email: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SidebarGroup {
    SystemLabels,
    UserLabels,
    SavedSearches,
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

#[derive(Debug, Clone)]
struct PendingPreviewRead {
    message_id: MessageId,
    due_at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchTarget {
    Mailbox,
    SearchPage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingSearchRequest {
    pub query: String,
    pub mode: SearchMode,
    pub sort: SortOrder,
    pub limit: u32,
    pub offset: u32,
    pub target: SearchTarget,
    pub append: bool,
    pub session_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingSearchCountRequest {
    pub query: String,
    pub mode: SearchMode,
    pub session_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingSearchDebounce {
    pub query: String,
    pub mode: SearchMode,
    pub session_id: u64,
    pub due_at: Instant,
}

pub struct App {
    pub theme: Theme,
    pub envelopes: Vec<Envelope>,
    pub all_envelopes: Vec<Envelope>,
    pub mailbox_view: MailboxView,
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
    pub html_image_support: Option<TerminalImageSupport>,
    pub html_image_assets: HashMap<MessageId, HashMap<String, HtmlImageEntry>>,
    pub queued_body_fetches: Vec<MessageId>,
    pub queued_html_image_asset_fetches: Vec<MessageId>,
    pub queued_html_image_decodes: Vec<HtmlImageKey>,
    pub in_flight_body_requests: HashSet<MessageId>,
    pub in_flight_html_image_asset_requests: HashSet<MessageId>,
    pub pending_thread_fetch: Option<mxr_core::ThreadId>,
    pub in_flight_thread_fetch: Option<mxr_core::ThreadId>,
    pub thread_request_id: u64,
    pub pending_search: Option<PendingSearchRequest>,
    pub pending_search_count: Option<PendingSearchCountRequest>,
    pub pending_search_debounce: Option<PendingSearchDebounce>,
    pub mailbox_search_session_id: u64,
    pub search_active: bool,
    pub pending_rule_detail: Option<String>,
    pub rule_detail_request_id: u64,
    pub pending_rule_history: Option<String>,
    pub rule_history_request_id: u64,
    pub pending_rule_dry_run: Option<String>,
    pub pending_rule_delete: Option<String>,
    pub pending_rule_upsert: Option<serde_json::Value>,
    pub pending_rule_form_load: Option<String>,
    pub rule_form_request_id: u64,
    pub pending_rule_form_save: bool,
    pub pending_bug_report: bool,
    pub pending_browser_open: Option<PendingBrowserOpen>,
    pub pending_browser_open_after_load: Option<MessageId>,
    pub pending_config_edit: bool,
    pub pending_log_open: bool,
    pub pending_diagnostics_details: Option<DiagnosticsPaneKind>,
    pub pending_draft_cleanup: Vec<std::path::PathBuf>,
    pub pending_account_save: Option<mxr_protocol::AccountConfigData>,
    pub pending_account_test: Option<mxr_protocol::AccountConfigData>,
    pub pending_account_authorize: Option<(mxr_protocol::AccountConfigData, bool)>,
    pub pending_account_set_default: Option<String>,
    /// True when the set-default was triggered from sidebar account switching
    /// (vs the Accounts tab). Used to trigger full state reset on completion.
    pub pending_account_switch: bool,
    pub sidebar_selected: usize,
    pub sidebar_section: SidebarSection,
    pub help_modal_open: bool,
    pub help_scroll_offset: u16,
    pub help_query: String,
    pub help_selected: usize,
    pub saved_searches: Vec<mxr_core::SavedSearch>,
    pub subscriptions_page: SubscriptionsPageState,
    pub rules_page: RulesPageState,
    pub diagnostics_page: DiagnosticsPageState,
    pub accounts_page: AccountsPageState,
    pub onboarding: FeatureOnboardingState,
    pub pending_local_state_save: bool,
    pub active_label: Option<mxr_core::LabelId>,
    pub pending_label_fetch: Option<mxr_core::LabelId>,
    pub pending_active_label: Option<mxr_core::LabelId>,
    pub pending_labels_refresh: bool,
    pub pending_all_envelopes_refresh: bool,
    pub pending_subscriptions_refresh: bool,
    pub pending_status_refresh: bool,
    pub status_request_id: u64,
    pub diagnostics_request_id: u64,
    pub desired_system_mailbox: Option<String>,
    pub status_message: Option<String>,
    pending_preview_read: Option<PendingPreviewRead>,
    pub pending_mutation_count: usize,
    pub pending_mutation_status: Option<String>,
    pub pending_mutation_queue: Vec<(Request, MutationEffect)>,
    pub pending_compose: Option<ComposeAction>,
    pub pending_send_confirm: Option<PendingSend>,
    pub pending_bulk_confirm: Option<PendingBulkConfirm>,
    pub error_modal: Option<ErrorModalState>,
    pub pending_unsubscribe_confirm: Option<PendingUnsubscribeConfirm>,
    pub pending_unsubscribe_action: Option<PendingUnsubscribeAction>,
    pub reader_mode: bool,
    pub html_view: bool,
    pub render_html_command: Option<String>,
    pub show_reader_stats: bool,
    pub remote_content_enabled: bool,
    pub signature_expanded: bool,
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
    pub sidebar_accounts_expanded: bool,
    pub sidebar_system_expanded: bool,
    pub sidebar_user_expanded: bool,
    pub sidebar_saved_searches_expanded: bool,
    pending_label_action: Option<(LabelPickerMode, String)>,
    pub url_modal: Option<ui::url_modal::UrlModalState>,
    pub rule_condition_editor: TextArea<'static>,
    pub rule_action_editor: TextArea<'static>,
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
        self.reader_mode = config.render.reader_mode;
        self.render_html_command = config.render.html_command.clone();
        self.show_reader_stats = config.render.show_reader_stats;
        self.remote_content_enabled = config.render.html_remote_content;
        self.snooze_config = config.snooze.clone();
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
            envelopes: Vec::new(),
            all_envelopes: Vec::new(),
            mailbox_view: MailboxView::Messages,
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
            html_image_support: None,
            html_image_assets: HashMap::new(),
            queued_body_fetches: Vec::new(),
            queued_html_image_asset_fetches: Vec::new(),
            queued_html_image_decodes: Vec::new(),
            in_flight_body_requests: HashSet::new(),
            in_flight_html_image_asset_requests: HashSet::new(),
            pending_thread_fetch: None,
            in_flight_thread_fetch: None,
            thread_request_id: 0,
            pending_search: None,
            pending_search_count: None,
            pending_search_debounce: None,
            mailbox_search_session_id: 0,
            search_active: false,
            pending_rule_detail: None,
            rule_detail_request_id: 0,
            pending_rule_history: None,
            rule_history_request_id: 0,
            pending_rule_dry_run: None,
            pending_rule_delete: None,
            pending_rule_upsert: None,
            pending_rule_form_load: None,
            rule_form_request_id: 0,
            pending_rule_form_save: false,
            pending_bug_report: false,
            pending_browser_open: None,
            pending_browser_open_after_load: None,
            pending_config_edit: false,
            pending_log_open: false,
            pending_diagnostics_details: None,
            pending_draft_cleanup: Vec::new(),
            pending_account_save: None,
            pending_account_test: None,
            pending_account_authorize: None,
            pending_account_set_default: None,
            pending_account_switch: false,
            sidebar_selected: 0,
            sidebar_section: SidebarSection::Labels,
            help_modal_open: false,
            help_scroll_offset: 0,
            help_query: String::new(),
            help_selected: 0,
            saved_searches: Vec::new(),
            subscriptions_page: SubscriptionsPageState::default(),
            rules_page: RulesPageState::default(),
            diagnostics_page: DiagnosticsPageState::default(),
            accounts_page: AccountsPageState::default(),
            onboarding: FeatureOnboardingState::default(),
            pending_local_state_save: false,
            active_label: None,
            pending_label_fetch: None,
            pending_active_label: None,
            pending_labels_refresh: false,
            pending_all_envelopes_refresh: false,
            pending_subscriptions_refresh: false,
            pending_status_refresh: false,
            status_request_id: 0,
            diagnostics_request_id: 0,
            desired_system_mailbox: None,
            status_message: None,
            pending_preview_read: None,
            pending_mutation_count: 0,
            pending_mutation_status: None,
            pending_mutation_queue: Vec::new(),
            pending_compose: None,
            pending_send_confirm: None,
            pending_bulk_confirm: None,
            error_modal: None,
            pending_unsubscribe_confirm: None,
            pending_unsubscribe_action: None,
            reader_mode: render.reader_mode,
            html_view: false,
            render_html_command: render.html_command.clone(),
            show_reader_stats: render.show_reader_stats,
            remote_content_enabled: render.html_remote_content,
            signature_expanded: false,
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
            sidebar_accounts_expanded: true,
            sidebar_system_expanded: true,
            sidebar_user_expanded: true,
            sidebar_saved_searches_expanded: true,
            pending_label_action: None,
            url_modal: None,
            rule_condition_editor: TextArea::default(),
            rule_action_editor: TextArea::default(),
            input: InputHandler::new(),
        }
    }

    pub fn selected_envelope(&self) -> Option<&Envelope> {
        if self.mailbox_view == MailboxView::Subscriptions {
            return self
                .subscriptions_page
                .entries
                .get(self.selected_index)
                .map(|entry| &entry.envelope);
        }

        match self.mail_list_mode {
            MailListMode::Messages => self.envelopes.get(self.selected_index),
            MailListMode::Threads => self.selected_mail_row().and_then(|row| {
                self.envelopes
                    .iter()
                    .find(|env| env.id == row.representative.id)
            }),
        }
    }

    pub(crate) fn schedule_draft_cleanup(&mut self, path: std::path::PathBuf) {
        if !self.pending_draft_cleanup.contains(&path) {
            self.pending_draft_cleanup.push(path);
        }
    }

    pub(crate) fn take_pending_draft_cleanup(&mut self) -> Vec<std::path::PathBuf> {
        std::mem::take(&mut self.pending_draft_cleanup)
    }

    pub fn mail_list_rows(&self) -> Vec<MailListRow> {
        Self::build_mail_list_rows(&self.envelopes, self.mail_list_mode)
    }

    pub fn search_mail_list_rows(&self) -> Vec<MailListRow> {
        Self::build_mail_list_rows(&self.search_page.results, self.search_list_mode())
    }

    pub fn selected_mail_row(&self) -> Option<MailListRow> {
        if self.mailbox_view == MailboxView::Subscriptions {
            return None;
        }
        self.mail_list_rows().get(self.selected_index).cloned()
    }

    pub fn selected_subscription_entry(&self) -> Option<&SubscriptionEntry> {
        self.subscriptions_page.entries.get(self.selected_index)
    }

    pub fn focused_thread_envelope(&self) -> Option<&Envelope> {
        self.viewed_thread_messages.get(self.thread_selected_index)
    }

    pub fn sidebar_items(&self) -> Vec<SidebarItem> {
        let mut items = Vec::new();
        // Accounts section (only shown when multiple accounts exist)
        let sync_accounts: Vec<_> = self
            .accounts_page
            .accounts
            .iter()
            .filter(|a| a.sync_kind.is_some())
            .collect();
        if sync_accounts.len() > 1 && self.sidebar_accounts_expanded {
            items.extend(sync_accounts.into_iter().cloned().map(SidebarItem::Account));
        }
        let mut system_labels = Vec::new();
        let mut user_labels = Vec::new();
        for label in self.visible_labels() {
            if label.kind == LabelKind::System {
                system_labels.push(label.clone());
            } else {
                user_labels.push(label.clone());
            }
        }
        if self.sidebar_system_expanded {
            items.extend(system_labels.into_iter().map(SidebarItem::Label));
        }
        items.push(SidebarItem::AllMail);
        items.push(SidebarItem::Subscriptions);
        if self.sidebar_user_expanded {
            items.extend(user_labels.into_iter().map(SidebarItem::Label));
        }
        if self.sidebar_saved_searches_expanded {
            items.extend(
                self.saved_searches
                    .iter()
                    .cloned()
                    .map(SidebarItem::SavedSearch),
            );
        }
        items
    }

    pub fn sidebar_view(&self) -> crate::ui::sidebar::SidebarView<'_> {
        use crate::ui::sidebar::{AccountInfo, SidebarView};
        let accounts: Vec<AccountInfo> = self
            .accounts_page
            .accounts
            .iter()
            .filter(|a| a.sync_kind.is_some())
            .map(|a| AccountInfo {
                email: a.email.clone(),
                is_default: a.is_default,
            })
            .collect();
        SidebarView {
            labels: &self.labels,
            active_pane: &self.active_pane,
            saved_searches: &self.saved_searches,
            sidebar_selected: self.sidebar_selected,
            all_mail_active: !self.search_active
                && self.mailbox_view == MailboxView::Messages
                && self.active_label.is_none()
                && self.pending_active_label.is_none(),
            subscriptions_active: self.mailbox_view == MailboxView::Subscriptions,
            subscription_count: self.subscriptions_page.entries.len(),
            accounts,
            accounts_expanded: self.sidebar_accounts_expanded,
            system_expanded: self.sidebar_system_expanded,
            user_expanded: self.sidebar_user_expanded,
            saved_searches_expanded: self.sidebar_saved_searches_expanded,
            active_label: self
                .pending_active_label
                .as_ref()
                .or(self.active_label.as_ref()),
        }
    }

    pub fn selected_sidebar_item(&self) -> Option<SidebarItem> {
        self.sidebar_items().get(self.sidebar_selected).cloned()
    }

    pub(crate) fn selected_sidebar_key(&self) -> Option<SidebarSelectionKey> {
        self.selected_sidebar_item().map(|item| match item {
            SidebarItem::Account(account) => {
                SidebarSelectionKey::Account(account.key.clone().unwrap_or_default())
            }
            SidebarItem::AllMail => SidebarSelectionKey::AllMail,
            SidebarItem::Subscriptions => SidebarSelectionKey::Subscriptions,
            SidebarItem::Label(label) => SidebarSelectionKey::Label(label.id),
            SidebarItem::SavedSearch(search) => SidebarSelectionKey::SavedSearch(search.name),
        })
    }

    pub(crate) fn restore_sidebar_selection(&mut self, selection: Option<SidebarSelectionKey>) {
        let items = self.sidebar_items();
        match selection.and_then(|selection| {
            items.iter().position(|item| match (item, &selection) {
                (SidebarItem::Account(account), SidebarSelectionKey::Account(key)) => {
                    account.key.as_deref() == Some(key.as_str())
                }
                (SidebarItem::AllMail, SidebarSelectionKey::AllMail) => true,
                (SidebarItem::Subscriptions, SidebarSelectionKey::Subscriptions) => true,
                (SidebarItem::Label(label), SidebarSelectionKey::Label(label_id)) => {
                    label.id == *label_id
                }
                (SidebarItem::SavedSearch(search), SidebarSelectionKey::SavedSearch(name)) => {
                    search.name == *name
                }
                _ => false,
            })
        }) {
            Some(index) => self.sidebar_selected = index,
            None => {
                self.sidebar_selected = self.sidebar_selected.min(items.len().saturating_sub(1));
            }
        }
        self.sync_sidebar_section();
    }

    pub fn selected_search_envelope(&self) -> Option<&Envelope> {
        match self.search_list_mode() {
            MailListMode::Messages => self
                .search_page
                .results
                .get(self.search_page.selected_index),
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

    pub(crate) fn search_row_index_for_message(&self, message_id: &MessageId) -> Option<usize> {
        match self.search_list_mode() {
            MailListMode::Messages => self
                .search_page
                .results
                .iter()
                .position(|env| &env.id == message_id),
            MailListMode::Threads => self
                .search_page
                .results
                .iter()
                .find(|env| &env.id == message_id)
                .and_then(|env| {
                    self.search_mail_list_rows()
                        .iter()
                        .position(|row| row.thread_id == env.thread_id)
                }),
        }
    }

    pub(crate) fn apply_search_page_results(&mut self, append: bool, results: SearchResultData) {
        let SearchResultData {
            envelopes,
            scores,
            has_more,
        } = results;
        let selected_row_message_id = (!append && self.search_page.result_selected)
            .then(|| self.selected_search_envelope().map(|env| env.id.clone()))
            .flatten();

        if append {
            self.search_page.results.extend(envelopes);
            self.search_page.scores.extend(scores);
        } else {
            self.search_page.results = envelopes;
            self.search_page.scores = scores;
            self.search_page.selected_index = 0;
            self.search_page.scroll_offset = 0;

            if let Some(message_id) = selected_row_message_id {
                if let Some(index) = self.search_row_index_for_message(&message_id) {
                    self.search_page.selected_index = index;
                } else {
                    self.reset_search_preview_selection();
                }
            }
        }

        self.search_page.has_more = has_more;
        self.search_page.loading_more = false;
        self.search_page.ui_status = SearchUiStatus::Loaded;
        self.search_page.session_active =
            !self.search_page.query.is_empty() || !self.search_page.results.is_empty();

        if self.search_page.load_to_end {
            if self.search_page.has_more {
                self.load_more_search_results();
            } else {
                self.search_page.load_to_end = false;
                if self.search_row_count() > 0 {
                    self.search_page.selected_index = self.search_row_count() - 1;
                    self.sync_search_cursor_after_move();
                } else {
                    self.clear_message_view_state();
                }
            }
            return;
        }

        if self.screen == Screen::Search {
            if self.search_page.result_selected {
                self.sync_search_cursor_after_move();
            } else if self.search_row_count() > 0 {
                self.ensure_search_visible();
            } else {
                self.clear_message_view_state();
            }
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

    pub fn refresh_selected_rule_panel(&mut self) {
        let selected_rule_id = self
            .selected_rule()
            .and_then(|rule| rule["id"].as_str())
            .map(ToString::to_string);

        self.pending_rule_detail = None;
        self.pending_rule_history = None;
        self.pending_rule_dry_run = None;

        if let Some(rule_id) = selected_rule_id {
            match self.rules_page.panel {
                RulesPanel::History => self.pending_rule_history = Some(rule_id),
                RulesPanel::DryRun => self.pending_rule_dry_run = Some(rule_id),
                RulesPanel::Details | RulesPanel::Form => self.pending_rule_detail = Some(rule_id),
            }
        }
    }

    pub fn current_ui_context(&self) -> UiContext {
        match self.screen {
            Screen::Mailbox => match self.active_pane {
                ActivePane::Sidebar => UiContext::MailboxSidebar,
                ActivePane::MailList => UiContext::MailboxList,
                ActivePane::MessageView => UiContext::MailboxMessage,
            },
            Screen::Search => {
                if self.search_page.editing {
                    UiContext::SearchEditor
                } else {
                    match self.search_page.active_pane {
                        SearchPane::Results => UiContext::SearchResults,
                        SearchPane::Preview => UiContext::SearchPreview,
                    }
                }
            }
            Screen::Rules => {
                if self.rules_page.form.visible {
                    UiContext::RulesForm
                } else {
                    UiContext::RulesList
                }
            }
            Screen::Diagnostics => UiContext::Diagnostics,
            Screen::Accounts => {
                if self.accounts_page.form.visible {
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
        self.accounts_page.onboarding_required = true;
        self.accounts_page.onboarding_modal_open = true;
        self.onboarding.visible = false;
        self.active_label = None;
        self.pending_active_label = None;
        self.pending_label_fetch = None;
        self.desired_system_mailbox = None;
    }

    fn complete_account_setup_onboarding(&mut self) {
        self.accounts_page.onboarding_modal_open = false;
        self.screen = Screen::Accounts;
        self.accounts_page.refresh_pending = true;
        self.apply(Action::OpenAccountFormNew);
    }

    pub fn sync_rule_form_editors(&mut self) {
        self.rule_condition_editor = TextArea::from(if self.rules_page.form.condition.is_empty() {
            vec![String::new()]
        } else {
            self.rules_page
                .form
                .condition
                .lines()
                .map(ToString::to_string)
                .collect()
        });
        self.rule_action_editor = TextArea::from(if self.rules_page.form.action.is_empty() {
            vec![String::new()]
        } else {
            self.rules_page
                .form
                .action
                .lines()
                .map(ToString::to_string)
                .collect()
        });
    }

    pub fn sync_rule_form_strings_from_editors(&mut self) {
        self.rules_page.form.condition = self.rule_condition_editor.lines().join("\n");
        self.rules_page.form.action = self.rule_action_editor.lines().join("\n");
    }

    pub fn maybe_show_feature_onboarding(&mut self) {
        if self.onboarding.seen || self.accounts_page.accounts.is_empty() {
            return;
        }
        self.onboarding.visible = true;
        self.onboarding.step = 0;
    }

    pub fn dismiss_feature_onboarding(&mut self) {
        self.onboarding.visible = false;
        if !self.onboarding.seen {
            self.onboarding.seen = true;
            self.pending_local_state_save = true;
        }
    }

    pub fn advance_feature_onboarding(&mut self) {
        if self.onboarding.step >= 4 {
            self.dismiss_feature_onboarding();
        } else {
            self.onboarding.step += 1;
        }
    }

    fn selected_account_config(&self) -> Option<mxr_protocol::AccountConfigData> {
        self.selected_account().and_then(account_summary_to_config)
    }

    fn account_form_field_count(&self) -> usize {
        match self.accounts_page.form.mode {
            AccountFormMode::Gmail => {
                if self.accounts_page.form.gmail_credential_source
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

    fn account_form_data(&self, is_default: bool) -> mxr_protocol::AccountConfigData {
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
            sync,
            send,
            is_default,
        }
    }

    fn account_form_validation_failure(
        &self,
    ) -> Option<(mxr_protocol::AccountOperationResult, usize)> {
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

    fn fail_account_form_submission(
        &mut self,
        result: mxr_protocol::AccountOperationResult,
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
            .map(|value| value.chars().count())
            .unwrap_or(0);
    }

    fn account_result_modal_hint(label: &str, detail: &str) -> Option<&'static str> {
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

    fn next_account_form_mode(&self, forward: bool) -> AccountFormMode {
        match (self.accounts_page.form.mode, forward) {
            (AccountFormMode::Gmail, true) => AccountFormMode::ImapSmtp,
            (AccountFormMode::ImapSmtp, true) => AccountFormMode::SmtpOnly,
            (AccountFormMode::SmtpOnly, true) => AccountFormMode::Gmail,
            (AccountFormMode::Gmail, false) => AccountFormMode::SmtpOnly,
            (AccountFormMode::ImapSmtp, false) => AccountFormMode::Gmail,
            (AccountFormMode::SmtpOnly, false) => AccountFormMode::ImapSmtp,
        }
    }

    fn account_form_has_meaningful_input(&self) -> bool {
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

    fn open_new_account_form(&mut self) {
        self.accounts_page.form = AccountFormState {
            visible: true,
            is_new_account: true,
            ..AccountFormState::default()
        };
        self.accounts_page.resume_new_account_draft_prompt_open = false;
        self.refresh_account_form_derived_fields();
    }

    fn restore_new_account_form_draft(&mut self) {
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

    fn maybe_preserve_new_account_form_draft(&mut self) {
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

    fn apply_account_form_mode(&mut self, mode: AccountFormMode) {
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

    fn request_account_form_mode_change(&mut self, forward: bool) {
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

    fn refresh_account_form_derived_fields(&mut self) {
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

    fn current_account_form_toggle_field(&self) -> Option<AccountFormToggleField> {
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

    fn toggle_current_account_form_field(&mut self, forward: bool) -> bool {
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

    fn mail_row_count(&self) -> usize {
        if self.mailbox_view == MailboxView::Subscriptions {
            return self.subscriptions_page.entries.len();
        }
        self.mail_list_rows().len()
    }

    pub(crate) fn search_row_count(&self) -> usize {
        self.search_mail_list_rows().len()
    }

    fn search_list_mode(&self) -> MailListMode {
        self.mail_list_mode
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
                    if sane_mail_sort_timestamp(&envelope.date)
                        > sane_mail_sort_timestamp(&entry.representative.date)
                    {
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
            // In the results pane, prefer the selected search result so that
            // multi-select (ToggleSelect) targets the highlighted row rather
            // than a stale viewing_envelope left over from the mailbox.
            if self.search_page.active_pane == SearchPane::Results {
                return self
                    .selected_search_envelope()
                    .or_else(|| self.focused_thread_envelope())
                    .or(self.viewing_envelope.as_ref());
            }
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
        self.labels = client.list_labels().await?;
        self.all_envelopes = client.list_envelopes(5000, 0).await?;
        self.load_initial_mailbox(client).await?;
        self.saved_searches = client.list_saved_searches().await.unwrap_or_default();
        self.set_subscriptions(client.list_subscriptions(500).await.unwrap_or_default());
        if let Ok(Response::Ok {
            data:
                ResponseData::Status {
                    uptime_secs,
                    daemon_pid,
                    accounts,
                    total_messages,
                    sync_statuses,
                    ..
                },
        }) = client.raw_request(Request::GetStatus).await
        {
            self.apply_status_snapshot(
                uptime_secs,
                daemon_pid,
                accounts,
                total_messages,
                sync_statuses,
            );
        }
        // Queue body prefetch for first visible window
        self.queue_body_window();
        Ok(())
    }

    async fn load_initial_mailbox(&mut self, client: &mut Client) -> Result<(), MxrError> {
        let Some(inbox_id) = self
            .labels
            .iter()
            .find(|label| label.name == "INBOX")
            .map(|label| label.id.clone())
        else {
            self.envelopes = self.all_mail_envelopes();
            self.active_label = None;
            return Ok(());
        };

        match client
            .raw_request(Request::ListEnvelopes {
                label_id: Some(inbox_id.clone()),
                account_id: None,
                limit: 5000,
                offset: 0,
            })
            .await
        {
            Ok(Response::Ok {
                data: ResponseData::Envelopes { envelopes },
            }) => {
                self.envelopes = envelopes;
                self.active_label = Some(inbox_id);
                self.pending_active_label = None;
                self.pending_label_fetch = None;
                self.sidebar_selected = 0;
                Ok(())
            }
            Ok(Response::Error { message }) => {
                self.envelopes = self.all_mail_envelopes();
                self.active_label = None;
                self.status_message = Some(format!("Inbox load failed: {message}"));
                Ok(())
            }
            Ok(_) => {
                self.envelopes = self.all_mail_envelopes();
                self.active_label = None;
                self.status_message = Some("Inbox load failed: unexpected response".into());
                Ok(())
            }
            Err(error) => {
                self.envelopes = self.all_mail_envelopes();
                self.active_label = None;
                self.status_message = Some(format!("Inbox load failed: {error}"));
                Ok(())
            }
        }
    }

    pub fn apply_status_snapshot(
        &mut self,
        uptime_secs: u64,
        daemon_pid: Option<u32>,
        accounts: Vec<String>,
        total_messages: u32,
        sync_statuses: Vec<mxr_protocol::AccountSyncStatus>,
    ) {
        self.diagnostics_page.uptime_secs = Some(uptime_secs);
        self.diagnostics_page.daemon_pid = daemon_pid;
        self.diagnostics_page.accounts = accounts;
        self.diagnostics_page.total_messages = Some(total_messages);
        self.diagnostics_page.sync_statuses = sync_statuses;
        self.last_sync_status = Some(Self::summarize_sync_status(
            &self.diagnostics_page.sync_statuses,
        ));
    }

    pub fn input_pending(&self) -> bool {
        self.input.is_pending()
    }

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
            Some(SidebarItem::Account(account)) => {
                if let Some(key) = account.key {
                    if !account.is_default {
                        Some(Action::SwitchAccount(key))
                    } else {
                        None // already active
                    }
                } else {
                    None
                }
            }
            Some(SidebarItem::AllMail) => Some(Action::GoToAllMail),
            Some(SidebarItem::Subscriptions) => Some(Action::OpenSubscriptions),
            Some(SidebarItem::Label(label)) => Some(Action::SelectLabel(label.id)),
            Some(SidebarItem::SavedSearch(search)) => {
                Some(Action::SelectSavedSearch(search.query, search.search_mode))
            }
            None => None,
        }
    }

    fn bump_search_session_id(current: &mut u64) -> u64 {
        *current = current.saturating_add(1).max(1);
        *current
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "queued search state stays explicit at call sites"
    )]
    fn queue_search_request(
        &mut self,
        target: SearchTarget,
        append: bool,
        query: String,
        mode: SearchMode,
        sort: SortOrder,
        offset: u32,
        session_id: u64,
    ) {
        self.pending_search = Some(PendingSearchRequest {
            query,
            mode,
            sort,
            limit: SEARCH_PAGE_SIZE,
            offset,
            target,
            append,
            session_id,
        });
    }

    fn queue_search_count_request(&mut self, query: String, mode: SearchMode, session_id: u64) {
        self.pending_search_count = Some(PendingSearchCountRequest {
            query,
            mode,
            session_id,
        });
    }

    fn reset_search_page_workspace(&mut self) {
        Self::bump_search_session_id(&mut self.search_page.session_id);
        self.search_page.query.clear();
        self.search_page.results.clear();
        self.search_page.scores.clear();
        self.search_page.has_more = false;
        self.search_page.loading_more = false;
        self.search_page.total_count = None;
        self.search_page.count_pending = false;
        self.search_page.ui_status = SearchUiStatus::Idle;
        self.search_page.load_to_end = false;
        self.search_page.session_active = false;
        self.search_page.active_pane = SearchPane::Results;
        self.search_page.selected_index = 0;
        self.search_page.scroll_offset = 0;
        self.search_page.result_selected = false;
        self.search_page.throbber = ThrobberState::default();
        self.pending_search = None;
        self.pending_search_count = None;
        self.pending_search_debounce = None;
        self.clear_message_view_state();
    }

    fn begin_search_page_request(&mut self, status: SearchUiStatus) -> u64 {
        self.search_page.results.clear();
        self.search_page.scores.clear();
        self.search_page.has_more = false;
        self.search_page.loading_more = matches!(
            status,
            SearchUiStatus::Searching | SearchUiStatus::LoadingMore
        );
        self.search_page.total_count = None;
        self.search_page.count_pending = matches!(status, SearchUiStatus::Searching);
        self.search_page.ui_status = status;
        self.search_page.load_to_end = false;
        self.search_page.session_active = !self.search_page.query.trim().is_empty();
        self.search_page.active_pane = SearchPane::Results;
        self.search_page.selected_index = 0;
        self.search_page.scroll_offset = 0;
        self.search_page.result_selected = false;
        self.search_page.throbber = ThrobberState::default();
        self.pending_search = None;
        self.pending_search_count = None;
        self.clear_message_view_state();
        Self::bump_search_session_id(&mut self.search_page.session_id)
    }

    fn schedule_search_page_search(&mut self) {
        self.search_bar.query = self.search_page.query.clone();
        self.search_bar.mode = self.search_page.mode;
        self.search_page.sort = SortOrder::DateDesc;
        let query = self.search_page.query.trim().to_string();
        if query.is_empty() {
            self.reset_search_page_workspace();
            return;
        }

        let session_id = self.begin_search_page_request(SearchUiStatus::Debouncing);
        self.pending_search_debounce = Some(PendingSearchDebounce {
            query,
            mode: self.search_page.mode,
            session_id,
            due_at: Instant::now() + SEARCH_DEBOUNCE_DELAY,
        });
    }

    pub fn execute_search_page_search(&mut self) {
        self.search_bar.query = self.search_page.query.clone();
        self.search_bar.mode = self.search_page.mode;
        self.search_page.sort = SortOrder::DateDesc;
        let query = self.search_page.query.trim().to_string();
        self.pending_search_debounce = None;
        if query.is_empty() {
            self.reset_search_page_workspace();
            return;
        }

        let session_id = self.begin_search_page_request(SearchUiStatus::Searching);
        self.queue_search_request(
            SearchTarget::SearchPage,
            false,
            query.clone(),
            self.search_page.mode,
            self.search_page.sort.clone(),
            0,
            session_id,
        );
        self.queue_search_count_request(query, self.search_page.mode, session_id);
    }

    fn process_pending_search_debounce(&mut self) {
        let Some(pending) = self.pending_search_debounce.clone() else {
            return;
        };
        if pending.due_at > Instant::now() || pending.session_id != self.search_page.session_id {
            return;
        }

        self.pending_search_debounce = None;
        self.search_page.loading_more = true;
        self.search_page.count_pending = true;
        self.search_page.ui_status = SearchUiStatus::Searching;
        self.queue_search_request(
            SearchTarget::SearchPage,
            false,
            pending.query.clone(),
            pending.mode,
            self.search_page.sort.clone(),
            0,
            pending.session_id,
        );
        self.queue_search_count_request(pending.query, pending.mode, pending.session_id);
    }

    pub(crate) fn load_more_search_results(&mut self) {
        if self.search_page.loading_more
            || !self.search_page.has_more
            || self.search_page.query.is_empty()
        {
            return;
        }
        self.search_page.loading_more = true;
        self.search_page.ui_status = SearchUiStatus::LoadingMore;
        self.queue_search_request(
            SearchTarget::SearchPage,
            true,
            self.search_page.query.clone(),
            self.search_page.mode,
            self.search_page.sort.clone(),
            self.search_page.results.len() as u32,
            self.search_page.session_id,
        );
    }

    pub fn maybe_load_more_search_results(&mut self) {
        if self.screen != Screen::Search || self.search_page.active_pane != SearchPane::Results {
            return;
        }
        let row_count = self.search_row_count();
        if row_count == 0 || !self.search_page.has_more || self.search_page.loading_more {
            return;
        }
        if self.search_page.selected_index.saturating_add(3) >= row_count {
            self.load_more_search_results();
        }
    }

    fn sync_sidebar_section(&mut self) {
        self.sidebar_section = match self.selected_sidebar_item() {
            Some(SidebarItem::SavedSearch(_)) => SidebarSection::SavedSearches,
            _ => SidebarSection::Labels,
        };
    }

    fn current_sidebar_group(&self) -> SidebarGroup {
        match self.selected_sidebar_item() {
            Some(SidebarItem::SavedSearch(_)) => SidebarGroup::SavedSearches,
            Some(SidebarItem::Label(label)) if label.kind == LabelKind::System => {
                SidebarGroup::SystemLabels
            }
            Some(SidebarItem::Label(_)) => SidebarGroup::UserLabels,
            Some(SidebarItem::Account(_))
            | Some(SidebarItem::AllMail)
            | Some(SidebarItem::Subscriptions)
            | None => SidebarGroup::SystemLabels,
        }
    }

    fn collapse_current_sidebar_section(&mut self) {
        match self.current_sidebar_group() {
            SidebarGroup::SystemLabels => self.sidebar_system_expanded = false,
            SidebarGroup::UserLabels => self.sidebar_user_expanded = false,
            SidebarGroup::SavedSearches => self.sidebar_saved_searches_expanded = false,
        }
        self.sidebar_selected = self
            .sidebar_selected
            .min(self.sidebar_items().len().saturating_sub(1));
        self.sync_sidebar_section();
    }

    fn expand_current_sidebar_section(&mut self) {
        match self.current_sidebar_group() {
            SidebarGroup::SystemLabels => self.sidebar_system_expanded = true,
            SidebarGroup::UserLabels => self.sidebar_user_expanded = true,
            SidebarGroup::SavedSearches => self.sidebar_saved_searches_expanded = true,
        }
        self.sidebar_selected = self
            .sidebar_selected
            .min(self.sidebar_items().len().saturating_sub(1));
        self.sync_sidebar_section();
    }

    /// Live filter: instant client-side prefix matching on subject/from/snippet,
    /// plus async Tantivy search for full-text body matches.
    fn trigger_live_search(&mut self) {
        if self.screen == Screen::Search {
            self.schedule_search_page_search();
            return;
        }

        let query_source = self.search_bar.query.clone();
        self.search_bar.query = query_source.clone();
        self.search_page.mode = self.search_bar.mode;
        self.search_page.sort = SortOrder::DateDesc;
        let query = query_source.to_lowercase();
        if query.is_empty() {
            Self::bump_search_session_id(&mut self.mailbox_search_session_id);
            self.envelopes = self.all_mail_envelopes();
            self.search_active = false;
        } else {
            let query_words: Vec<&str> = query.split_whitespace().collect();
            // Instant client-side filter: every query word must prefix-match
            // some word in subject, from, or snippet
            let filtered: Vec<Envelope> = self
                .all_envelopes
                .iter()
                .filter(|e| !e.flags.contains(MessageFlags::TRASH))
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
            let mut filtered = filtered;
            filtered.sort_by(|left, right| {
                sane_mail_sort_timestamp(&right.date)
                    .cmp(&sane_mail_sort_timestamp(&left.date))
                    .then_with(|| right.id.as_str().cmp(&left.id.as_str()))
            });
            self.envelopes = filtered;
            self.search_active = true;
            let session_id = Self::bump_search_session_id(&mut self.mailbox_search_session_id);
            self.queue_search_request(
                SearchTarget::Mailbox,
                false,
                query_source,
                self.search_bar.mode,
                SortOrder::DateDesc,
                0,
                session_id,
            );
        }
        self.selected_index = 0;
        self.scroll_offset = 0;
    }

    pub fn search_is_pending(&self) -> bool {
        matches!(
            self.search_page.ui_status,
            SearchUiStatus::Debouncing | SearchUiStatus::Searching | SearchUiStatus::LoadingMore
        )
    }

    pub fn open_selected_search_result(&mut self) {
        if let Some(env) = self.selected_search_envelope().cloned() {
            self.search_page.result_selected = true;
            self.open_envelope(env);
            self.search_page.active_pane = SearchPane::Preview;
        } else {
            self.reset_search_preview_selection();
        }
    }

    pub fn maybe_open_search_preview(&mut self) {
        if self.search_page.result_selected {
            self.search_page.active_pane = SearchPane::Preview;
        } else {
            self.open_selected_search_result();
        }
    }

    pub fn reset_search_preview_selection(&mut self) {
        self.search_page.result_selected = false;
        self.search_page.active_pane = SearchPane::Results;
        self.clear_message_view_state();
    }

    /// Compute the mail list title based on active filter/search.
    pub fn mail_list_title(&self) -> String {
        if self.mailbox_view == MailboxView::Subscriptions {
            return format!("Subscriptions ({})", self.subscriptions_page.entries.len());
        }

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
            format!("All Mail {list_name} ({list_count})")
        }
    }

    fn all_mail_envelopes(&self) -> Vec<Envelope> {
        self.all_envelopes
            .iter()
            .filter(|envelope| !envelope.flags.contains(MessageFlags::TRASH))
            .cloned()
            .collect()
    }

    fn active_label_record(&self) -> Option<&Label> {
        let label_id = self
            .pending_active_label
            .as_ref()
            .or(self.active_label.as_ref())?;
        self.labels.iter().find(|label| &label.id == label_id)
    }

    fn global_starred_count(&self) -> usize {
        self.labels
            .iter()
            .find(|label| label.name.eq_ignore_ascii_case("STARRED"))
            .map(|label| label.total_count as usize)
            .unwrap_or_else(|| {
                self.all_envelopes
                    .iter()
                    .filter(|envelope| envelope.flags.contains(MessageFlags::STARRED))
                    .count()
            })
    }

    fn active_body_status(&self) -> Option<String> {
        let BodyViewState::Ready {
            source, metadata, ..
        } = &self.body_view_state
        else {
            return None;
        };

        let mut chips = vec![match metadata.mode {
            BodyViewMode::Text => "text".to_string(),
            BodyViewMode::Html => "html".to_string(),
        }];
        chips.push(
            match source {
                BodySource::Plain => "plain",
                BodySource::Html => "html-part",
                BodySource::Fallback => "fallback",
                BodySource::Snippet => "snippet",
            }
            .to_string(),
        );
        if let Some(provenance) = metadata.provenance {
            chips.push(
                match provenance {
                    BodyPartSource::Exact => "source:exact",
                    BodyPartSource::DerivedFromPlain => "source:plain-derived",
                    BodyPartSource::DerivedFromHtml => "source:html-derived",
                    BodyPartSource::BestEffortSummary => "source:best-effort",
                }
                .to_string(),
            );
        }
        if metadata.reader_applied {
            chips.push("reader".into());
        }
        if metadata.flowed {
            chips.push("flowed".into());
        }
        if metadata.inline_images {
            chips.push("inline-images".into());
        }
        if metadata.mode == BodyViewMode::Html && metadata.remote_content_available {
            chips.push(if metadata.remote_content_enabled {
                "remote:on".into()
            } else {
                "remote:off".into()
            });
        }
        if self.show_reader_stats {
            if let (Some(original), Some(cleaned)) =
                (metadata.original_lines, metadata.cleaned_lines)
            {
                chips.push(format!("reader:{cleaned}/{original}"));
            }
        }
        Some(chips.join(" "))
    }

    pub fn status_bar_state(&self) -> ui::status_bar::StatusBarState {
        let starred_count = self.global_starred_count();
        let body_status = self.active_body_status();

        if self.mailbox_view == MailboxView::Subscriptions {
            let unread_count = self
                .subscriptions_page
                .entries
                .iter()
                .filter(|entry| !entry.envelope.flags.contains(MessageFlags::READ))
                .count();
            return ui::status_bar::StatusBarState {
                mailbox_name: "SUBSCRIPTIONS".into(),
                total_count: self.subscriptions_page.entries.len(),
                unread_count,
                starred_count,
                body_status: body_status.clone(),
                sync_status: self.last_sync_status.clone(),
                status_message: self.status_message.clone(),
                pending_mutation_count: self.pending_mutation_count,
                pending_mutation_status: self.pending_mutation_status.clone(),
            };
        }

        if self.screen == Screen::Search || self.search_active {
            let results = if self.screen == Screen::Search {
                &self.search_page.results
            } else {
                &self.envelopes
            };
            let unread_count = results
                .iter()
                .filter(|envelope| !envelope.flags.contains(MessageFlags::READ))
                .count();
            return ui::status_bar::StatusBarState {
                mailbox_name: "SEARCH".into(),
                total_count: results.len(),
                unread_count,
                starred_count,
                body_status: body_status.clone(),
                sync_status: self.last_sync_status.clone(),
                status_message: self.status_message.clone(),
                pending_mutation_count: self.pending_mutation_count,
                pending_mutation_status: self.pending_mutation_status.clone(),
            };
        }

        if let Some(label) = self.active_label_record() {
            return ui::status_bar::StatusBarState {
                mailbox_name: label.name.clone(),
                total_count: label.total_count as usize,
                unread_count: label.unread_count as usize,
                starred_count,
                body_status: body_status.clone(),
                sync_status: self.last_sync_status.clone(),
                status_message: self.status_message.clone(),
                pending_mutation_count: self.pending_mutation_count,
                pending_mutation_status: self.pending_mutation_status.clone(),
            };
        }

        let unread_count = self
            .envelopes
            .iter()
            .filter(|envelope| !envelope.flags.contains(MessageFlags::READ))
            .count();
        ui::status_bar::StatusBarState {
            mailbox_name: "ALL MAIL".into(),
            total_count: self
                .diagnostics_page
                .total_messages
                .map(|count| count as usize)
                .unwrap_or_else(|| self.all_envelopes.len()),
            unread_count,
            starred_count,
            body_status,
            sync_status: self.last_sync_status.clone(),
            status_message: self.status_message.clone(),
            pending_mutation_count: self.pending_mutation_count,
            pending_mutation_status: self.pending_mutation_status.clone(),
        }
    }

    fn summarize_sync_status(sync_statuses: &[mxr_protocol::AccountSyncStatus]) -> String {
        if sync_statuses.is_empty() {
            return "not synced".into();
        }
        if sync_statuses.iter().any(|sync| sync.sync_in_progress) {
            return "syncing".into();
        }
        if sync_statuses
            .iter()
            .any(|sync| !sync.healthy || sync.last_error.is_some())
        {
            return "degraded".into();
        }
        sync_statuses
            .iter()
            .filter_map(|sync| sync.last_success_at.as_deref())
            .filter_map(Self::format_sync_age)
            .max_by_key(|(_, sort_key)| *sort_key)
            .map(|(display, _)| format!("synced {display}"))
            .unwrap_or_else(|| "not synced".into())
    }

    fn format_sync_age(timestamp: &str) -> Option<(String, i64)> {
        let parsed = chrono::DateTime::parse_from_rfc3339(timestamp).ok()?;
        let synced_at = parsed.with_timezone(&chrono::Utc);
        let elapsed = chrono::Utc::now().signed_duration_since(synced_at);
        let seconds = elapsed.num_seconds().max(0);
        let display = if seconds < 60 {
            "just now".to_string()
        } else if seconds < 3_600 {
            format!("{}m ago", seconds / 60)
        } else if seconds < 86_400 {
            format!("{}h ago", seconds / 3_600)
        } else {
            format!("{}d ago", seconds / 86_400)
        };
        Some((display, synced_at.timestamp()))
    }

    pub fn resolve_desired_system_mailbox(&mut self) {
        let Some(target) = self.desired_system_mailbox.as_deref() else {
            return;
        };
        if self.pending_active_label.is_some() || self.active_label.is_some() {
            return;
        }
        if let Some(label_id) = self
            .labels
            .iter()
            .find(|label| label.name.eq_ignore_ascii_case(target))
            .map(|label| label.id.clone())
        {
            self.apply(Action::SelectLabel(label_id));
        }
    }

    /// In ThreePane mode, auto-load the preview for the currently selected envelope.
    fn auto_preview(&mut self) {
        if self.mailbox_view == MailboxView::Subscriptions {
            if let Some(entry) = self.selected_subscription_entry().cloned() {
                if self.viewing_envelope.as_ref().map(|e| &e.id) != Some(&entry.envelope.id) {
                    self.open_envelope(entry.envelope);
                }
            } else {
                self.pending_preview_read = None;
                self.viewing_envelope = None;
                self.viewed_thread = None;
                self.viewed_thread_messages.clear();
                self.body_view_state = BodyViewState::Empty { preview: None };
            }
            return;
        }

        if self.layout_mode == LayoutMode::ThreePane {
            if let Some(row) = self.selected_mail_row() {
                if self.viewing_envelope.as_ref().map(|e| &e.id) != Some(&row.representative.id) {
                    self.open_envelope(row.representative);
                }
            }
        }
    }

    pub fn auto_preview_search(&mut self) {
        if !self.search_page.result_selected {
            if self.screen == Screen::Search {
                self.clear_message_view_state();
            }
            return;
        }
        if let Some(env) = self.selected_search_envelope().cloned() {
            if self
                .viewing_envelope
                .as_ref()
                .map(|current| current.id.clone())
                != Some(env.id.clone())
            {
                self.open_envelope(env);
            }
        } else if self.screen == Screen::Search {
            self.search_page.result_selected = false;
            self.clear_message_view_state();
        }
    }

    pub(crate) fn sync_search_cursor_after_move(&mut self) {
        let row_count = self.search_row_count();
        if row_count == 0 {
            self.search_page.selected_index = 0;
            self.search_page.scroll_offset = 0;
            self.search_page.result_selected = false;
            self.clear_message_view_state();
            return;
        }

        self.search_page.selected_index = self
            .search_page
            .selected_index
            .min(row_count.saturating_sub(1));
        self.ensure_search_visible();
        self.update_visual_selection();
        self.maybe_load_more_search_results();
        if self.search_page.result_selected {
            self.auto_preview_search();
        }
    }

    pub(crate) fn ensure_search_visible(&mut self) {
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
        let source_envelopes: Vec<Envelope> = if self.mailbox_view == MailboxView::Subscriptions {
            self.subscriptions_page
                .entries
                .iter()
                .map(|entry| entry.envelope.clone())
                .collect()
        } else {
            self.envelopes.clone()
        };
        let len = source_envelopes.len();
        if len == 0 {
            return;
        }
        let start = self.selected_index.saturating_sub(BUFFER / 2);
        let end = (self.selected_index + BUFFER / 2).min(len);
        let ids: Vec<MessageId> = source_envelopes[start..end]
            .iter()
            .map(|e| e.id.clone())
            .collect();
        for id in ids {
            self.queue_body_fetch(id);
        }
    }

    fn open_envelope(&mut self, env: Envelope) {
        self.close_attachment_panel();
        self.signature_expanded = false;
        self.viewed_thread = None;
        self.viewed_thread_messages = self.optimistic_thread_messages(&env);
        self.thread_selected_index = self.default_thread_selected_index();
        self.viewing_envelope = self.focused_thread_envelope().cloned();
        if let Some(viewing_envelope) = self.viewing_envelope.clone() {
            self.schedule_preview_read(&viewing_envelope);
        }
        for message in self.viewed_thread_messages.clone() {
            self.queue_body_fetch(message.id);
        }
        self.queue_thread_fetch(env.thread_id.clone());
        self.queue_html_assets_for_current_view();
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
        if let Some(viewing_envelope) = self.viewing_envelope.clone() {
            self.schedule_preview_read(&viewing_envelope);
        } else {
            self.pending_preview_read = None;
        }
        self.message_scroll_offset = 0;
        self.ensure_current_body_state();
    }

    fn schedule_preview_read(&mut self, envelope: &Envelope) {
        if envelope.flags.contains(MessageFlags::READ)
            || self.has_pending_set_read(&envelope.id, true)
        {
            self.pending_preview_read = None;
            return;
        }

        if self
            .pending_preview_read
            .as_ref()
            .is_some_and(|pending| pending.message_id == envelope.id)
        {
            return;
        }

        self.pending_preview_read = Some(PendingPreviewRead {
            message_id: envelope.id.clone(),
            due_at: Instant::now() + PREVIEW_MARK_READ_DELAY,
        });
    }

    fn has_pending_set_read(&self, message_id: &MessageId, read: bool) -> bool {
        self.pending_mutation_queue.iter().any(|(request, _)| {
            matches!(
                request,
                Request::Mutation(MutationCommand::SetRead { message_ids, read: queued_read })
                    if *queued_read == read
                        && message_ids.len() == 1
                        && message_ids[0] == *message_id
            )
        })
    }

    fn process_pending_preview_read(&mut self) {
        let Some(pending) = self.pending_preview_read.clone() else {
            return;
        };
        if Instant::now() < pending.due_at {
            return;
        }
        self.pending_preview_read = None;

        let Some(envelope) = self
            .viewing_envelope
            .clone()
            .filter(|envelope| envelope.id == pending.message_id)
        else {
            return;
        };

        if envelope.flags.contains(MessageFlags::READ)
            || self.has_pending_set_read(&envelope.id, true)
        {
            return;
        }

        let mut flags = envelope.flags;
        flags.insert(MessageFlags::READ);
        self.apply_local_flags(&envelope.id, flags);
        self.queue_mutation(
            Request::Mutation(MutationCommand::SetRead {
                message_ids: vec![envelope.id.clone()],
                read: true,
            }),
            MutationEffect::StatusOnly("Marked message as read".into()),
            "Marking message as read...".into(),
        );
    }

    pub fn next_background_timeout(&self, fallback: Duration) -> Duration {
        let mut timeout = fallback;
        if let Some(pending) = self.pending_preview_read.as_ref() {
            timeout = timeout.min(
                pending
                    .due_at
                    .checked_duration_since(Instant::now())
                    .unwrap_or(Duration::ZERO),
            );
        }
        if let Some(pending) = self.pending_search_debounce.as_ref() {
            timeout = timeout.min(
                pending
                    .due_at
                    .checked_duration_since(Instant::now())
                    .unwrap_or(Duration::ZERO),
            );
        }
        if self.search_is_pending() {
            timeout = timeout.min(SEARCH_SPINNER_TICK);
        }
        timeout
    }

    #[cfg(test)]
    pub fn expire_pending_preview_read_for_tests(&mut self) {
        if let Some(pending) = self.pending_preview_read.as_mut() {
            pending.due_at = Instant::now();
        }
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

    fn move_message_view_down(&mut self) {
        if self.viewed_thread_messages.len() > 1 {
            self.move_thread_focus_down();
        } else {
            self.message_scroll_offset = self.message_scroll_offset.saturating_add(1);
        }
    }

    fn move_message_view_up(&mut self) {
        if self.viewed_thread_messages.len() > 1 {
            self.move_thread_focus_up();
        } else {
            self.message_scroll_offset = self.message_scroll_offset.saturating_sub(1);
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

    fn reader_config(&self) -> mxr_reader::ReaderConfig {
        mxr_reader::ReaderConfig {
            html_command: self.render_html_command.clone(),
            ..Default::default()
        }
    }

    fn render_body(&self, raw: &str, source: BodySource) -> (String, Option<(usize, usize)>) {
        if !self.reader_mode || source == BodySource::Snippet {
            return (raw.to_string(), None);
        }

        let output = match source {
            BodySource::Plain => mxr_reader::clean(Some(raw), None, &self.reader_config()),
            BodySource::Html => mxr_reader::clean(None, Some(raw), &self.reader_config()),
            BodySource::Fallback => mxr_reader::clean(Some(raw), None, &self.reader_config()),
            BodySource::Snippet => unreachable!("snippet bodies bypass reader mode"),
        };

        (
            output.content,
            Some((output.original_lines, output.cleaned_lines)),
        )
    }

    fn body_inline_images(body: &MessageBody) -> bool {
        body.attachments.iter().any(Self::attachment_is_inlineish)
    }

    fn attachment_is_inlineish(attachment: &AttachmentMeta) -> bool {
        attachment.disposition == AttachmentDisposition::Inline
            || attachment.content_id.is_some()
            || attachment.content_location.is_some()
    }

    fn sorted_attachment_panel_attachments(body: &MessageBody) -> Vec<AttachmentMeta> {
        let mut attachments = body.attachments.clone();
        attachments.sort_by_key(Self::attachment_is_inlineish);
        attachments
    }

    fn html_has_remote_content(html: &str) -> bool {
        static REMOTE_IMAGE_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
        REMOTE_IMAGE_RE
            .get_or_init(|| {
                regex::Regex::new(r#"(?is)<img\b[^>]*\bsrc\s*=\s*["']https?://[^"']+["']"#)
                    .expect("valid remote image regex")
            })
            .is_match(html)
    }

    fn body_view_metadata(
        &self,
        body: &MessageBody,
        source: BodySource,
        mode: BodyViewMode,
        reader_applied: bool,
        stats: Option<(usize, usize)>,
    ) -> BodyViewMetadata {
        BodyViewMetadata {
            mode,
            provenance: match source {
                BodySource::Plain => body.metadata.text_plain_source,
                BodySource::Html => body.metadata.text_html_source,
                BodySource::Fallback | BodySource::Snippet => None,
            },
            reader_applied,
            flowed: matches!(
                body.metadata.text_plain_format,
                Some(TextPlainFormat::Flowed { .. })
            ),
            inline_images: Self::body_inline_images(body),
            remote_content_available: body
                .text_html
                .as_deref()
                .is_some_and(Self::html_has_remote_content),
            remote_content_enabled: self.remote_content_enabled,
            original_lines: stats.map(|(original, _)| original),
            cleaned_lines: stats.map(|(_, cleaned)| cleaned),
        }
    }

    fn resolve_body_view_state(&self, envelope: &Envelope) -> BodyViewState {
        let preview = Self::envelope_preview(envelope);

        if let Some(body) = self.body_cache.get(&envelope.id) {
            if self.html_view {
                if let Some(raw) = body.text_html.clone() {
                    let metadata = self.body_view_metadata(
                        body,
                        BodySource::Html,
                        BodyViewMode::Html,
                        false,
                        None,
                    );
                    return BodyViewState::Ready {
                        rendered: raw.clone(),
                        raw,
                        source: BodySource::Html,
                        metadata,
                    };
                }

                if let Some(raw) = body.text_plain.clone() {
                    let metadata = self.body_view_metadata(
                        body,
                        BodySource::Plain,
                        BodyViewMode::Html,
                        false,
                        None,
                    );
                    return BodyViewState::Ready {
                        rendered: raw.clone(),
                        raw,
                        source: BodySource::Plain,
                        metadata,
                    };
                }
            }

            if let Some(raw) = body.text_plain.clone() {
                let (rendered, stats) = self.render_body(&raw, BodySource::Plain);
                let metadata = self.body_view_metadata(
                    body,
                    BodySource::Plain,
                    BodyViewMode::Text,
                    self.reader_mode,
                    stats,
                );
                return BodyViewState::Ready {
                    raw,
                    rendered,
                    source: BodySource::Plain,
                    metadata,
                };
            }

            if let Some(raw) = body.text_html.clone() {
                let (rendered, stats) = self.render_body(&raw, BodySource::Html);
                let metadata = self.body_view_metadata(
                    body,
                    BodySource::Html,
                    BodyViewMode::Text,
                    self.reader_mode,
                    stats,
                );
                return BodyViewState::Ready {
                    raw,
                    rendered,
                    source: BodySource::Html,
                    metadata,
                };
            }

            if let Some(raw) = body.best_effort_readable_summary() {
                let (rendered, stats) = self.render_body(&raw, BodySource::Fallback);
                let metadata = self.body_view_metadata(
                    body,
                    BodySource::Fallback,
                    BodyViewMode::Text,
                    self.reader_mode,
                    stats,
                );
                return BodyViewState::Ready {
                    raw,
                    rendered,
                    source: BodySource::Fallback,
                    metadata,
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
        self.queue_html_assets_for_message(&message_id);

        if self.pending_browser_open_after_load.as_ref() == Some(&message_id) {
            self.pending_browser_open_after_load = None;
            if let Some(body) = self.body_cache.get(&message_id).cloned() {
                self.queue_browser_open_for_body(message_id.clone(), &body);
            }
        }

        if self.viewing_envelope.as_ref().map(|env| env.id.clone()) == Some(message_id) {
            self.ensure_current_body_state();
        }
    }

    pub fn resolve_body_fetch_error(&mut self, message_id: &MessageId, message: String) {
        self.in_flight_body_requests.remove(message_id);
        if self.pending_browser_open_after_load.as_ref() == Some(message_id) {
            self.pending_browser_open_after_load = None;
        }

        if let Some(env) = self
            .viewing_envelope
            .as_ref()
            .filter(|env| &env.id == message_id)
        {
            self.body_view_state = BodyViewState::Error {
                message,
                preview: Self::envelope_preview(env),
            };
        }
    }

    pub fn set_terminal_image_support(&mut self, support: TerminalImageSupport) {
        self.html_image_support = Some(support);
    }

    pub fn queue_html_assets_for_current_view(&mut self) {
        if !self.html_view {
            return;
        }

        let message_ids = self
            .viewed_thread_messages
            .iter()
            .map(|message| message.id.clone())
            .collect::<Vec<_>>();
        for message_id in message_ids {
            self.queue_html_assets_for_message(&message_id);
        }
    }

    pub fn queue_html_assets_for_message(&mut self, message_id: &MessageId) {
        if !self.html_view {
            return;
        }
        let Some(body) = self.body_cache.get(message_id) else {
            return;
        };
        if body.text_html.is_none() {
            return;
        }
        if self
            .in_flight_html_image_asset_requests
            .contains(message_id)
            || self
                .queued_html_image_asset_fetches
                .iter()
                .any(|queued| queued == message_id)
        {
            return;
        }
        self.queued_html_image_asset_fetches
            .push(message_id.clone());
    }

    pub fn invalidate_html_assets_for_current_view(&mut self) {
        let message_ids = self
            .viewed_thread_messages
            .iter()
            .map(|message| message.id.clone())
            .collect::<Vec<_>>();
        self.invalidate_html_assets_for_messages(&message_ids);
    }

    pub fn invalidate_html_assets_for_messages(&mut self, message_ids: &[MessageId]) {
        for message_id in message_ids {
            self.html_image_assets.remove(message_id);
            self.in_flight_html_image_asset_requests.remove(message_id);
            self.queued_html_image_asset_fetches
                .retain(|queued| queued != message_id);
            self.queued_html_image_decodes
                .retain(|queued| &queued.message_id != message_id);
        }
    }

    pub fn resolve_html_image_assets_success(
        &mut self,
        message_id: MessageId,
        assets: Vec<HtmlImageAsset>,
        allow_remote: bool,
    ) {
        self.in_flight_html_image_asset_requests.remove(&message_id);
        let mut entries = HashMap::new();
        for asset in assets {
            let source = asset.source.clone();
            let should_decode = asset.status == HtmlImageAssetStatus::Ready
                && asset.path.is_some()
                && !self
                    .queued_html_image_decodes
                    .iter()
                    .any(|queued| queued.message_id == message_id && queued.source == source);
            if should_decode {
                self.queued_html_image_decodes.push(HtmlImageKey {
                    message_id: message_id.clone(),
                    source: source.clone(),
                });
            }
            entries.insert(source, HtmlImageEntry::new(asset));
        }
        self.html_image_assets.insert(message_id.clone(), entries);

        if self.remote_content_enabled != allow_remote {
            return;
        }
        if self.viewing_envelope.as_ref().map(|env| env.id.clone()) == Some(message_id) {
            self.ensure_current_body_state();
        }
    }

    pub fn resolve_html_image_assets_error(&mut self, message_id: &MessageId, message: String) {
        self.in_flight_html_image_asset_requests.remove(message_id);
        let mut entries = HashMap::new();
        entries.insert(
            "__error__".into(),
            HtmlImageEntry {
                asset: HtmlImageAsset {
                    source: "__error__".into(),
                    kind: HtmlImageSourceKind::File,
                    status: HtmlImageAssetStatus::Failed,
                    mime_type: None,
                    path: None,
                    detail: Some(message),
                },
                render: crate::terminal_images::HtmlImageRenderState::Failed(
                    "asset resolution failed".into(),
                ),
            },
        );
        self.html_image_assets.insert(message_id.clone(), entries);
    }

    pub fn resolve_html_image_protocol(
        &mut self,
        key: &HtmlImageKey,
        protocol: ratatui_image::thread::ThreadProtocol,
    ) {
        if let Some(entry) = self
            .html_image_assets
            .get_mut(&key.message_id)
            .and_then(|assets| assets.get_mut(&key.source))
        {
            entry.render = crate::terminal_images::HtmlImageRenderState::Ready(Box::new(protocol));
        }
    }

    pub fn resolve_html_image_resize(
        &mut self,
        key: &HtmlImageKey,
        response: ratatui_image::thread::ResizeResponse,
    ) {
        if let Some(protocol) = self
            .html_image_assets
            .get_mut(&key.message_id)
            .and_then(|assets| assets.get_mut(&key.source))
            .and_then(HtmlImageEntry::ready_protocol_mut)
        {
            protocol.update_resized_protocol(response);
        }
    }

    pub fn resolve_html_image_failure(&mut self, key: &HtmlImageKey, message: String) {
        if let Some(entry) = self
            .html_image_assets
            .get_mut(&key.message_id)
            .and_then(|assets| assets.get_mut(&key.source))
        {
            entry.render = crate::terminal_images::HtmlImageRenderState::Failed(message);
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
            .map(Self::sorted_attachment_panel_attachments)
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

    pub fn open_url_modal(&mut self) {
        let body = self.current_viewing_body();
        let Some(body) = body else {
            self.status_message = Some("No message body loaded".into());
            return;
        };
        let text_plain = body.text_plain.as_deref();
        let text_html = body.text_html.as_deref();
        let urls = ui::url_modal::extract_urls(text_plain, text_html);
        if urls.is_empty() {
            self.status_message = Some("No links found".into());
            return;
        }
        self.url_modal = Some(ui::url_modal::UrlModalState::new(urls));
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

    fn attachment_summaries_for_envelope(&self, envelope: &Envelope) -> Vec<AttachmentSummary> {
        self.body_cache
            .get(&envelope.id)
            .map(|body| {
                body.attachments
                    .iter()
                    .map(|attachment| AttachmentSummary {
                        filename: attachment.filename.clone(),
                        size_bytes: attachment.size_bytes,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn thread_message_blocks(&self) -> Vec<ui::message_view::ThreadMessageBlock> {
        self.viewed_thread_messages
            .iter()
            .map(|message| ui::message_view::ThreadMessageBlock {
                envelope: message.clone(),
                body_state: self.resolve_body_view_state(message),
                labels: self.label_chips_for_envelope(message),
                attachments: self.attachment_summaries_for_envelope(message),
                selected: self.viewing_envelope.as_ref().map(|env| env.id.clone())
                    == Some(message.id.clone()),
                bulk_selected: self.selected_set.contains(&message.id),
                has_unsubscribe: !matches!(message.unsubscribe, UnsubscribeMethod::None),
                signature_expanded: self.signature_expanded,
            })
            .collect()
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
            if message_ids
                .iter()
                .any(|message_id| message_id == &envelope.id)
            {
                apply_provider_label_changes(
                    &mut envelope.label_provider_ids,
                    &add_provider_ids,
                    &remove_provider_ids,
                );
            }
        }
        if let Some(ref mut envelope) = self.viewing_envelope {
            if message_ids
                .iter()
                .any(|message_id| message_id == &envelope.id)
            {
                apply_provider_label_changes(
                    &mut envelope.label_provider_ids,
                    &add_provider_ids,
                    &remove_provider_ids,
                );
            }
        }
    }

    pub fn apply_local_flags(&mut self, message_id: &MessageId, flags: MessageFlags) {
        for envelope in self
            .envelopes
            .iter_mut()
            .chain(self.all_envelopes.iter_mut())
            .chain(self.search_page.results.iter_mut())
            .chain(self.viewed_thread_messages.iter_mut())
        {
            if &envelope.id == message_id {
                envelope.flags = flags;
            }
        }
        if let Some(envelope) = self.viewing_envelope.as_mut() {
            if &envelope.id == message_id {
                envelope.flags = flags;
            }
        }
    }

    pub fn apply_local_flags_many(&mut self, updates: &[(MessageId, MessageFlags)]) {
        for (message_id, flags) in updates {
            self.apply_local_flags(message_id, *flags);
        }
    }

    fn apply_local_mutation_effect(&mut self, effect: &MutationEffect) {
        match effect {
            MutationEffect::RemoveFromList(message_id) => {
                self.apply_removed_message_ids(std::slice::from_ref(message_id));
            }
            MutationEffect::RemoveFromListMany(message_ids) => {
                self.apply_removed_message_ids(message_ids);
            }
            MutationEffect::UpdateFlags { message_id, flags } => {
                self.apply_local_flags(message_id, *flags);
            }
            MutationEffect::UpdateFlagsMany { updates } => {
                self.apply_local_flags_many(updates);
            }
            MutationEffect::ModifyLabels {
                message_ids,
                add,
                remove,
                ..
            } => {
                self.apply_local_label_refs(message_ids, add, remove);
            }
            MutationEffect::RefreshList | MutationEffect::StatusOnly(_) => {}
        }
    }

    fn queue_mutation(&mut self, request: Request, effect: MutationEffect, status_message: String) {
        self.pending_mutation_queue.push((request, effect));
        self.pending_mutation_count += 1;
        self.pending_mutation_status = Some(status_message.clone());
        self.status_message = Some(status_message);
    }

    pub fn finish_pending_mutation(&mut self) {
        self.pending_mutation_count = self.pending_mutation_count.saturating_sub(1);
        if self.pending_mutation_count == 0 {
            self.pending_mutation_status = None;
        }
    }

    fn show_error_modal(&mut self, title: impl Into<String>, detail: impl Into<String>) {
        self.error_modal = Some(ErrorModalState::new(title, detail));
    }

    pub(crate) fn apply_account_operation_result(
        &mut self,
        result: mxr_protocol::AccountOperationResult,
    ) {
        self.accounts_page.operation_in_flight = false;
        self.accounts_page.throbber = Default::default();
        self.accounts_page.status = Some(result.summary.clone());
        self.accounts_page.last_result = Some(result.clone());
        self.accounts_page.form.last_result = Some(result.clone());
        self.accounts_page.form.gmail_authorized = result
            .auth
            .as_ref()
            .map(|step| step.ok)
            .unwrap_or(self.accounts_page.form.gmail_authorized);
        if result.save.as_ref().is_some_and(|step| step.ok) {
            self.accounts_page.new_account_draft = None;
            self.accounts_page.resume_new_account_draft_prompt_open = false;
            self.accounts_page.form.visible = false;
        }
        if !result.ok && account_result_has_details(Some(&result)) {
            self.open_account_result_details_modal(&result);
        }
        self.accounts_page.refresh_pending = true;
    }

    pub(crate) fn open_last_account_result_details_modal(&mut self) {
        if let Some(result) = self
            .accounts_page
            .form
            .last_result
            .clone()
            .or_else(|| self.accounts_page.last_result.clone())
        {
            self.open_account_result_details_modal(&result);
        }
    }

    fn open_account_result_details_modal(&mut self, result: &mxr_protocol::AccountOperationResult) {
        self.show_error_modal(
            account_result_modal_title(result),
            account_result_modal_detail(result),
        );
    }

    pub fn show_mutation_failure(&mut self, error: &MxrError) {
        self.show_error_modal(
            "Mutation Failed",
            format!(
                "Optimistic changes could not be applied.\nMailbox is refreshing to reconcile state.\n\n{error}"
            ),
        );
        self.status_message = Some(format!("Error: {error}"));
    }

    pub fn refresh_mailbox_after_mutation_failure(&mut self) {
        self.pending_labels_refresh = true;
        self.pending_all_envelopes_refresh = true;
        self.pending_status_refresh = true;
        self.pending_subscriptions_refresh = true;
        if let Some(label_id) = self
            .pending_active_label
            .clone()
            .or_else(|| self.active_label.clone())
        {
            self.pending_label_fetch = Some(label_id);
        }
    }

    pub(crate) fn clear_message_view_state(&mut self) {
        self.pending_preview_read = None;
        self.viewing_envelope = None;
        self.viewed_thread = None;
        self.viewed_thread_messages.clear();
        self.thread_selected_index = 0;
        self.pending_thread_fetch = None;
        self.in_flight_thread_fetch = None;
        self.message_scroll_offset = 0;
        self.body_view_state = BodyViewState::Empty { preview: None };
    }

    pub(crate) fn apply_removed_message_ids(&mut self, ids: &[MessageId]) {
        if ids.is_empty() {
            return;
        }

        let viewing_removed = self
            .viewing_envelope
            .as_ref()
            .is_some_and(|envelope| ids.iter().any(|id| id == &envelope.id));
        let reader_was_open =
            self.layout_mode == LayoutMode::ThreePane && self.viewing_envelope.is_some();

        self.envelopes
            .retain(|envelope| !ids.iter().any(|id| id == &envelope.id));
        self.all_envelopes
            .retain(|envelope| !ids.iter().any(|id| id == &envelope.id));
        self.search_page
            .results
            .retain(|envelope| !ids.iter().any(|id| id == &envelope.id));
        self.viewed_thread_messages
            .retain(|envelope| !ids.iter().any(|id| id == &envelope.id));
        self.selected_set
            .retain(|message_id| !ids.iter().any(|id| id == message_id));

        self.selected_index = self
            .selected_index
            .min(self.mail_row_count().saturating_sub(1));
        self.search_page.selected_index = self
            .search_page
            .selected_index
            .min(self.search_row_count().saturating_sub(1));

        if viewing_removed {
            self.clear_message_view_state();

            if reader_was_open {
                match self.screen {
                    Screen::Search if self.search_row_count() > 0 => {
                        self.ensure_search_visible();
                        self.auto_preview_search();
                    }
                    Screen::Mailbox if self.mail_row_count() > 0 => {
                        self.ensure_visible();
                        self.auto_preview();
                    }
                    _ => {}
                }
            }

            if self.viewing_envelope.is_none() && self.layout_mode == LayoutMode::ThreePane {
                self.layout_mode = LayoutMode::TwoPane;
                if self.active_pane == ActivePane::MessageView {
                    self.active_pane = ActivePane::MailList;
                }
            }
        } else {
            if self.screen == Screen::Mailbox && self.mail_row_count() > 0 {
                self.ensure_visible();
            } else if self.screen == Screen::Search && self.search_row_count() > 0 {
                self.ensure_search_visible();
            }
        }
    }

    fn message_flags(&self, message_id: &MessageId) -> Option<MessageFlags> {
        self.envelopes
            .iter()
            .chain(self.all_envelopes.iter())
            .chain(self.search_page.results.iter())
            .chain(self.viewed_thread_messages.iter())
            .find(|envelope| &envelope.id == message_id)
            .map(|envelope| envelope.flags)
            .or_else(|| {
                self.viewing_envelope
                    .as_ref()
                    .filter(|envelope| &envelope.id == message_id)
                    .map(|envelope| envelope.flags)
            })
    }

    fn flag_updates_for_ids<F>(
        &self,
        message_ids: &[MessageId],
        mut update: F,
    ) -> Vec<(MessageId, MessageFlags)>
    where
        F: FnMut(MessageFlags) -> MessageFlags,
    {
        message_ids
            .iter()
            .filter_map(|message_id| {
                self.message_flags(message_id)
                    .map(|flags| (message_id.clone(), update(flags)))
            })
            .collect()
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

        if self
            .viewing_envelope
            .as_ref()
            .map(|env| env.thread_id.clone())
            == Some(thread_id)
        {
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
            self.queue_html_assets_for_current_view();
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

    #[expect(
        clippy::too_many_arguments,
        reason = "bulk confirmation inputs stay explicit for safety"
    )]
    fn queue_or_confirm_bulk_action(
        &mut self,
        title: impl Into<String>,
        detail: impl Into<String>,
        request: Request,
        effect: MutationEffect,
        optimistic_effect: Option<MutationEffect>,
        status_message: String,
        count: usize,
    ) {
        if count > 1 {
            self.pending_bulk_confirm = Some(PendingBulkConfirm {
                title: title.into(),
                detail: detail.into(),
                request,
                effect,
                optimistic_effect,
                status_message,
            });
        } else {
            if let Some(effect) = optimistic_effect.as_ref() {
                self.apply_local_mutation_effect(effect);
            }
            self.queue_mutation(request, effect, status_message);
            self.clear_selection();
        }
    }

    /// Update visual selection range when moving in visual mode.
    fn update_visual_selection(&mut self) {
        if self.visual_mode {
            if let Some(anchor) = self.visual_anchor {
                let (cursor, source) = if self.screen == Screen::Search {
                    (self.search_page.selected_index, &self.search_page.results)
                } else {
                    (self.selected_index, &self.envelopes)
                };
                let start = anchor.min(cursor);
                let end = anchor.max(cursor);
                self.selected_set.clear();
                for env in source.iter().skip(start).take(end - start + 1) {
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

    pub fn set_subscriptions(&mut self, subscriptions: Vec<SubscriptionSummary>) {
        let selected_id = self
            .selected_subscription_entry()
            .map(|entry| entry.summary.latest_message_id.clone());
        self.subscriptions_page.entries = subscriptions
            .into_iter()
            .map(|summary| SubscriptionEntry {
                envelope: subscription_summary_to_envelope(&summary),
                summary,
            })
            .collect();

        if self.subscriptions_page.entries.is_empty() {
            if self.mailbox_view == MailboxView::Subscriptions {
                self.selected_index = 0;
                self.scroll_offset = 0;
                self.viewing_envelope = None;
                self.viewed_thread = None;
                self.viewed_thread_messages.clear();
                self.body_view_state = BodyViewState::Empty { preview: None };
            }
            return;
        }

        if let Some(selected_id) = selected_id {
            if let Some(position) = self
                .subscriptions_page
                .entries
                .iter()
                .position(|entry| entry.summary.latest_message_id == selected_id)
            {
                self.selected_index = position;
            } else {
                self.selected_index = self
                    .selected_index
                    .min(self.subscriptions_page.entries.len().saturating_sub(1));
            }
        } else {
            self.selected_index = self
                .selected_index
                .min(self.subscriptions_page.entries.len().saturating_sub(1));
        }

        if self.mailbox_view == MailboxView::Subscriptions {
            self.ensure_visible();
            self.auto_preview();
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
