mod account_actions;
mod account_form_helpers;
mod account_identity;
mod account_marks;
mod actions;
mod analytics_actions;
mod archive_step;
mod assistant_actions;
mod attachment_helpers;
mod body_helpers;
pub(crate) mod bulk_preview;
mod compose_actions;
mod compose_assist;
mod compose_helpers;
mod diagnostics_actions;
mod draw;
mod export_helpers;
mod focus_actions;
mod hints;
mod input;
mod key_dispatch;
mod key_strip;
mod list_advance;
mod mailbox_actions;
mod mailbox_helpers;

pub(crate) const LOW_TIDE: &str = "Low tide. Nobody's waiting on you.";

pub(crate) const FRESHNESS_REQUEST: Request = Request::GetFreshness {
    account_id: None,
    limit: Some(0),
};
#[cfg(test)]
mod label_accounts_tests;
mod message_actions;
mod messages_actions;
mod modal_actions;
mod move_actions;
mod mutation_actions;
mod mutation_helpers;
pub mod mutation_snapshot;
mod not_now;
mod now_actions;
mod paste;
mod pending_optimistic;
mod personal_category_actions;
mod place_actions;
mod platform_actions;
mod promises;
mod reading_actions;
mod reading_highlights;
mod recorder;
mod records_actions;
mod reply_later_prompt;
mod row_gists;
mod rule_actions;
mod runtime_helpers;
mod saved_search_actions;
mod screen_actions;
mod screen_helpers;
mod search_actions;
mod search_helpers;
mod selection_helpers;
mod selection_scope;
mod semantic_actions;
mod sidebar_helpers;
mod snoozed_actions;
mod state;
mod todo_actions;
mod updates_actions;
mod visual_count;
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
use recorder::ActionRecorder;
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};
use throbber_widgets_tui::ThrobberState;
use tui_textarea::TextArea;

pub(in crate::app) use crate::ui::label_picker::LabelPickerMode;
use archive_step::archive_request_ids;
pub(crate) use focus_actions::focus_desk_request;
pub(crate) use mailbox_helpers::auto_summary_eligible;
pub(crate) use messages_actions::MESSAGES_MODE;
pub(crate) use move_actions::{chip_label, move_status, new_sender_key, sender_mode, MOVE_CHOICES};
pub use mutation_snapshot::{
    MutationId, MutationIdGenerator, MutationSnapshot, MutationSnapshotStore, QueuedMutation,
    TRANSIENT_MUTATION_MAX_RETRIES,
};
pub(crate) use now_actions::{mode_done_copy, NOW_MODE};
pub use pending_optimistic::PendingOptimisticState;
pub use promises::PromisePrompt;
#[cfg(test)]
pub(crate) use promises::{PROMISE_ANSWER_GUARD, PROMISE_PROMPT_TTL};
pub(crate) use reading_actions::READING_MODE;
pub(crate) use records_actions::ARCHIVE_MODE;
pub use row_gists::{RowGist, RowGistState, RowGists};
use state::PendingPreviewRead;
pub use state::*;
pub(crate) use todo_actions::TODO_MODE;
pub(crate) use updates_actions::{TUNE_CHOICES, UPDATES_MODE};

const PREVIEW_MARK_READ_DELAY: Duration = Duration::from_secs(5);
pub const SEARCH_PAGE_SIZE: u32 = 200;
const SEARCH_DEBOUNCE_DELAY: Duration = Duration::from_millis(120);
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
    ReplyLater {
        message_id: MessageId,
        flag: bool,
        status: String,
    },
    RefreshUpdates(String),
    UpdatesPin {
        message_id: MessageId,
        prior_pinned: bool,
        status: String,
    },
    RefreshList,
    StatusOnly(String),
    NextTime(String),
    RefreshPlaces(String),
    SenderMoved(String),
    Todo(String),
    Records(String),
    ModeDone(String),
    Messages(String),
    FocusRestore {
        thread_id: mxr_core::ThreadId,
    },
    SentSuccess {
        status: String,
        remind_at: Option<chrono::DateTime<chrono::Utc>>,
        sent_message_id: Option<MessageId>,
        draft_path: Option<std::path::PathBuf>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BriefingRequest {
    Thread(mxr_core::ThreadId),
    Recipient { email: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Mailbox,
    Search,
    Rules,
    Diagnostics,
    Accounts,
    Analytics,
    Deliveries,
}

#[derive(Debug, Clone)]
pub enum ConnectionState {
    SelectionChanged,
    Connecting,
    Connected,
    Reconnecting {
        since: std::time::Instant,
        reason: String,
    },
}

const CONNECTION_STALE_THRESHOLD: std::time::Duration = std::time::Duration::from_secs(5);

const UNDO_HINT_TTL: std::time::Duration = std::time::Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UndoAction {
    /// Daemon mutations by id: usually one; a batch job (a sweep) leaves
    /// one per chunk, all undone together.
    Mutations(Vec<String>),
    SenderKind {
        account_id: mxr_core::AccountId,
        sender_email: String,
        previous: Option<mxr_protocol::SenderKindData>,
    },
    /// To-dos ticked off or dismissed: put them back on the runway
    /// (`SetTodoState` undo).
    Todos(Vec<String>),
    TodoCorrection {
        todo_id: String,
        correction_id: String,
    },
    TodoResolution {
        todo_id: String,
        suggestion_id: String,
    },
    /// Catch-up rows kept or let go: back in the batch, undecided.
    Catchup(Vec<String>),
    /// An Archive change, reversed as the daemon's `RecordUndoData` says:
    /// restore or dismiss records, or drop the fields you set.
    Records(mxr_protocol::RecordUndoData),
    UpdateSource {
        account_id: mxr_core::AccountId,
        source_key: String,
        prior: mxr_protocol::UpdateSourceSettingData,
    },
    /// A pin on an Updates line: restore exactly the previous value.
    UpdatesPin { message_id: MessageId, pinned: bool },
    /// A move of an email or a sender (`MoveMessage`): put back exactly
    /// as it was (`UndoMove`).
    Move(i64),
    /// A local category membership change; the daemon checks pair revisions.
    CategoryMembership(String),
}

#[derive(Debug, Clone)]
pub struct PendingUndo {
    pub action: UndoAction,
    pub verb_past: String,
    pub count: u32,
    pub applied_at: std::time::Instant,
}

impl PendingUndo {
    fn toast_label(&self) -> String {
        if matches!(&self.action, UndoAction::Todos(_)) {
            return match self.verb_past.as_str() {
                "Ticked off" => format!(
                    "Ticked off {} {}",
                    self.count,
                    if self.count == 1 { "to-do" } else { "to-dos" }
                ),
                "Marked not a to-do" => format!(
                    "Marked {} {} as not a to-do",
                    self.count,
                    if self.count == 1 { "item" } else { "items" }
                ),
                _ => format!("{} {}", self.verb_past, self.count),
            };
        }
        format!("{} {}", self.verb_past, self.count)
    }

    pub(crate) fn undoing_label(&self) -> String {
        if matches!(&self.action, UndoAction::Todos(_)) {
            return format!(
                "Undoing {} {}...",
                self.count,
                if self.count == 1 { "task" } else { "tasks" }
            );
        }
        format!(
            "Undoing {} {}...",
            self.verb_past.to_lowercase(),
            self.count
        )
    }
}

#[derive(Debug, Clone)]
pub struct PendingInviteSend {
    pub message_id: mxr_core::MessageId,
    pub action: mxr_protocol::CalendarInviteActionData,
    pub dispatch_at: std::time::Instant,
    pub status_label: String,
}

#[derive(Debug, Clone)]
pub struct PendingScreenerDecision {
    pub account_id: mxr_core::AccountId,
    pub sender_email: String,
    pub disposition: mxr_protocol::ScreenerDispositionData,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SidebarGroup {
    SystemLabels,
    UserLabels,
    SavedSearches,
}

pub struct App {
    paired_remote_read_only: bool,
    pub theme: Theme,
    pub mailbox: MailboxState,
    pub search: SearchState,
    pub accounts: AccountsState,
    pub rules: RulesState,
    pub diagnostics: DiagnosticsState,
    pub analytics: AnalyticsState,
    pub deliveries: DeliveriesState,
    pub assistant: AssistantState,
    pub modals: ModalsState,
    pub(crate) hints: hints::HintState,
    pub compose: ComposeState,
    pub screen: Screen,
    pub should_quit: bool,
    pub pending_suspend: bool,
    pub command_palette: CommandPaletteState,
    pub last_sync_status: Option<String>,
    pub freshness: Option<mxr_protocol::FreshnessData>,
    pub user_sync_pending: bool,
    pub user_sync_acknowledgements_pending: usize,
    pub user_sync_targets: HashSet<mxr_core::AccountId>,
    pub user_sync_batch_accounts: HashSet<mxr_core::AccountId>,
    pub user_sync_batch_failures: HashMap<mxr_core::AccountId, String>,
    pub user_sync_events_before_ack: Vec<mxr_protocol::DaemonEvent>,
    pub visible_height: usize,
    pub html_image_support: Option<TerminalImageSupport>,
    pub html_image_assets: HashMap<MessageId, HashMap<String, HtmlImageEntry>>,
    pub queued_html_image_asset_fetches: Vec<MessageId>,
    pub queued_html_image_decodes: Vec<HtmlImageKey>,
    pub in_flight_html_image_asset_requests: HashSet<MessageId>,
    pub pending_local_state_save: bool,
    pub status_message: Option<String>,
    pub status_throbber: ThrobberState,
    pub toasts: ToastQueue,
    pub pending_mutation_count: usize,
    pub pending_mutation_status: Option<String>,
    pub mutation_batch_total: usize,
    pub pending_archive_preview: Option<Vec<MessageId>>,
    pub pending_mutation_queue: Vec<QueuedMutation>,
    pub mutation_snapshots: MutationSnapshotStore,
    pub mutation_id_generator: MutationIdGenerator,
    pub pending_optimistic: PendingOptimisticState,
    pub connection_state: ConnectionState,
    pub pending_connection_retry: bool,
    pub pending_category_requests: Vec<(u64, Request)>,
    pub pending_snippets_refresh: bool,
    pub pending_reply_queue_refresh: bool,
    pub pending_drafts_refresh: bool,
    pub pending_draft_edit_request: Option<mxr_core::Draft>,
    pub pending_reply_draft_resume: Option<ReplyDraftResume>,
    pub pending_draft_operation: Option<StoredDraftOperation>,
    pub pending_deliveries_refresh: bool,
    pub pending_delivery_resolve: Option<mxr_core::DeliveryId>,
    pub pending_delivery_dismiss: Option<mxr_core::DeliveryId>,
    pub pending_delivery_open: Option<mxr_core::ThreadId>,
    pub pending_assistant_thread_open: Option<mxr_core::ThreadId>,
    pub pending_activity_refresh: bool,
    pub pending_activity_pause_toggle: bool,
    pub pending_sender_profile_request: Option<(mxr_core::AccountId, String)>,
    pub pending_summary_requests: VecDeque<mxr_core::ThreadId>,
    pub pending_summary_debounce: Option<(mxr_core::ThreadId, tokio::time::Instant)>,
    pub pending_thread_context: Option<mxr_core::ThreadId>,
    pub pending_gist_debounce: Option<(mxr_core::ThreadId, tokio::time::Instant)>,
    pub row_gists: RowGistState,
    pub pending_briefing_request: Option<BriefingRequest>,
    pub pending_whois_query: Option<String>,
    pub pending_expert_query: Option<String>,
    pub pending_screener_refresh: Option<mxr_core::AccountId>,
    pub pending_screener_decisions: Vec<PendingScreenerDecision>,
    pub pending_undo: Option<PendingUndo>,
    pub pending_invite_send: Option<PendingInviteSend>,
    pub pending_promise_check: Option<MessageId>,
    pub promise_prompts: VecDeque<PromisePrompt>,
    pub download_dir: std::path::PathBuf,
    recorder: ActionRecorder,
    input: InputHandler,
    pub(crate) keybindings: crate::keybindings::KeybindingConfig,
    input_context: Option<(UiContext, MailboxView)>,
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
        self.download_dir = config.general.download_dir.clone();
    }

    pub(crate) fn enable_paired_remote_read_only_session(&mut self) {
        self.paired_remote_read_only = true;
        self.accounts.page.onboarding_required = false;
        self.accounts.page.onboarding_modal_open = false;
        self.mailbox.pending_subscriptions_refresh = false;
        self.mailbox.pending_desk_refresh = false;
        self.diagnostics.pending_status_refresh = false;
    }

    pub(crate) fn is_paired_remote_read_only(&self) -> bool {
        self.paired_remote_read_only
    }

    pub fn from_render_config(render: &RenderConfig) -> Self {
        Self::from_render_and_snooze(render, &mxr_config::SnoozeConfig::default())
    }

    fn from_render_and_snooze(
        render: &RenderConfig,
        snooze_config: &mxr_config::SnoozeConfig,
    ) -> Self {
        Self {
            paired_remote_read_only: false,
            theme: Theme::default(),
            mailbox: MailboxState::from_render_config(render),
            search: SearchState::default(),
            accounts: AccountsState::default(),
            rules: RulesState::default(),
            diagnostics: DiagnosticsState::default(),
            analytics: AnalyticsState::default(),
            deliveries: DeliveriesState::default(),
            assistant: AssistantState::default(),
            modals: ModalsState {
                snooze_config: snooze_config.clone(),
                ..ModalsState::default()
            },
            hints: hints::HintState::default(),
            compose: ComposeState::default(),
            screen: Screen::Mailbox,
            should_quit: false,
            pending_suspend: false,
            command_palette: CommandPaletteState::default(),
            last_sync_status: None,
            freshness: None,
            user_sync_pending: false,
            user_sync_acknowledgements_pending: 0,
            user_sync_targets: HashSet::new(),
            user_sync_batch_accounts: HashSet::new(),
            user_sync_batch_failures: HashMap::new(),
            user_sync_events_before_ack: Vec::new(),
            visible_height: 20,
            html_image_support: None,
            html_image_assets: HashMap::new(),
            queued_html_image_asset_fetches: Vec::new(),
            queued_html_image_decodes: Vec::new(),
            in_flight_html_image_asset_requests: HashSet::new(),
            pending_local_state_save: false,
            status_message: None,
            status_throbber: ThrobberState::default(),
            toasts: ToastQueue::default(),
            pending_mutation_count: 0,
            pending_mutation_status: None,
            mutation_batch_total: 0,
            pending_archive_preview: None,
            pending_mutation_queue: Vec::new(),
            mutation_snapshots: MutationSnapshotStore::default(),
            mutation_id_generator: MutationIdGenerator::default(),
            pending_optimistic: PendingOptimisticState::default(),
            connection_state: ConnectionState::Connecting,
            pending_connection_retry: false,
            pending_category_requests: Vec::new(),
            pending_snippets_refresh: false,
            pending_reply_queue_refresh: false,
            pending_drafts_refresh: false,
            pending_draft_edit_request: None,
            pending_reply_draft_resume: None,
            pending_draft_operation: None,
            pending_deliveries_refresh: false,
            pending_delivery_resolve: None,
            pending_delivery_dismiss: None,
            pending_delivery_open: None,
            pending_assistant_thread_open: None,
            pending_activity_refresh: false,
            pending_activity_pause_toggle: false,
            pending_sender_profile_request: None,
            pending_summary_requests: VecDeque::new(),
            pending_summary_debounce: None,
            pending_thread_context: None,
            pending_gist_debounce: None,
            row_gists: RowGistState::default(),
            pending_briefing_request: None,
            pending_whois_query: None,
            pending_expert_query: None,
            pending_screener_refresh: None,
            pending_screener_decisions: Vec::new(),
            pending_undo: None,
            pending_invite_send: None,
            pending_promise_check: None,
            promise_prompts: VecDeque::new(),
            download_dir: dirs::download_dir().unwrap_or_else(|| {
                dirs::home_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                    .join("Downloads")
            }),
            recorder: ActionRecorder::new(1000),
            input: InputHandler::new(),
            keybindings: crate::keybindings::shown_keybindings().clone(),
            input_context: None,
        }
    }

    pub fn set_connection_state(&mut self, state: ConnectionState) {
        if matches!(self.connection_state, ConnectionState::SelectionChanged) {
            return;
        }
        if matches!(state, ConnectionState::SelectionChanged) {
            self.pending_connection_retry = false;
            self.modals.error = Some(crate::app::ErrorModalState::new(
                "Connection changed",
                mxr_client::selected_daemon::SELECTION_CHANGED_MESSAGE,
            ));
        }
        let state = match (&self.connection_state, state) {
            (
                ConnectionState::Reconnecting { since, .. },
                ConnectionState::Reconnecting { reason, .. },
            ) => ConnectionState::Reconnecting {
                since: *since,
                reason,
            },
            (_, state) => state,
        };
        if matches!(state, ConnectionState::Connected) {
            // Only close error modals that were opened by the connection
            // watchdog. Heuristic: title mentions "daemon".
            if self
                .modals
                .error
                .as_ref()
                .is_some_and(|err| err.title.to_lowercase().contains("daemon"))
            {
                self.modals.error = None;
            }
        }
        if matches!(state, ConnectionState::Connected)
            && !matches!(self.connection_state, ConnectionState::Connected)
        {
            crate::daemon_events::refresh_after_event_gap(self);
        }
        self.connection_state = state;
    }

    pub fn tick_connection_state(&mut self, now: std::time::Instant) {
        if let ConnectionState::Reconnecting { since, reason } = &self.connection_state {
            if now.saturating_duration_since(*since) >= CONNECTION_STALE_THRESHOLD {
                let already_open = self
                    .modals
                    .error
                    .as_ref()
                    .is_some_and(|err| err.title.to_lowercase().contains("daemon"));
                if !already_open {
                    self.modals.error = Some(crate::app::ErrorModalState::new(
                        "Daemon not responding",
                        format!(
                            "The mxr daemon is not responding ({reason}). Press `r` to retry, or `q` to quit."
                        ),
                    ));
                }
            }
        }
    }

    pub fn request_connection_retry(&mut self) {
        if matches!(self.connection_state, ConnectionState::SelectionChanged) {
            return;
        }
        self.pending_connection_retry = true;
    }

    pub fn connection_state_label(&self) -> Option<String> {
        match &self.connection_state {
            ConnectionState::Connected => None,
            ConnectionState::SelectionChanged => {
                Some("Connection changed; close and reopen mxr".into())
            }
            ConnectionState::Connecting => Some("Connecting to daemon...".into()),
            ConnectionState::Reconnecting { reason, .. } => {
                Some(format!("Reconnecting to daemon ({reason})..."))
            }
        }
    }

    pub fn report_warn(&mut self, message: impl Into<String>) {
        self.push_user_error(UserError {
            severity: UserErrorSeverity::Warn,
            message: message.into(),
            title: None,
            since: std::time::Instant::now(),
        });
    }

    pub fn report_error(&mut self, title: impl Into<String>, detail: impl Into<String>) {
        let title = title.into();
        let detail = detail.into();
        self.push_user_error(UserError {
            severity: UserErrorSeverity::Error,
            message: detail.clone(),
            title: Some(title.clone()),
            since: std::time::Instant::now(),
        });
        if self.modals.error.is_none() {
            self.modals.error = Some(crate::app::ErrorModalState::new(title, detail));
        }
    }

    fn push_user_error(&mut self, entry: UserError) {
        let log = &mut self.modals.error_log;
        if log.len() >= USER_ERROR_LOG_CAPACITY {
            log.pop_front();
        }
        log.push_back(entry);
    }

    pub fn push_reported_error(&mut self, entry: UserError) {
        let escalate = matches!(entry.severity, UserErrorSeverity::Error);
        let title = entry.title.clone().unwrap_or_else(|| "Error".to_string());
        let message = entry.message.clone();
        self.push_user_error(entry);
        if escalate && self.modals.error.is_none() {
            self.modals.error = Some(crate::app::ErrorModalState::new(title, message));
        }
    }

    pub fn current_user_warn(&self, now: std::time::Instant) -> Option<String> {
        let entry = self
            .modals
            .error_log
            .iter()
            .rev()
            .find(|e| matches!(e.severity, UserErrorSeverity::Warn))?;
        if now.saturating_duration_since(entry.since) >= WARN_STATUS_TTL {
            return None;
        }
        Some(entry.message.clone())
    }

    pub fn set_pending_undo(&mut self, undo: PendingUndo) {
        self.pending_undo = Some(undo);
    }

    pub fn tick_pending_undo(&mut self, now: std::time::Instant) {
        if let Some(undo) = &self.pending_undo {
            if now.saturating_duration_since(undo.applied_at) >= UNDO_HINT_TTL {
                self.pending_undo = None;
            }
        }
    }

    pub fn tick_pending_invite_send(&mut self, now: std::time::Instant) {
        let should_dispatch = self
            .pending_invite_send
            .as_ref()
            .is_some_and(|p| now >= p.dispatch_at);
        if !should_dispatch {
            return;
        }
        let pending = self.pending_invite_send.take().expect("checked above");
        self.queue_mutation(
            mxr_protocol::Request::RespondInvite {
                message_id: pending.message_id,
                action: pending.action,
                dry_run: false,
            },
            MutationEffect::StatusOnly(String::new()),
            String::new(),
        );
    }

    pub fn pending_undo_toast(&self, now: std::time::Instant) -> Option<Toast> {
        let undo = self.pending_undo.as_ref()?;
        let toast = Toast {
            text: undo.toast_label(),
            severity: ToastSeverity::Success,
            created_at: undo.applied_at,
            ttl: UNDO_HINT_TTL,
            action_hint: Some("u to undo".into()),
        };
        (!toast.expired(now)).then_some(toast)
    }

    pub fn push_toast(&mut self, toast: Toast) {
        self.toasts.push(toast);
    }

    pub fn pending_input_prefix(&self) -> Option<char> {
        self.input.pending_prefix()
    }

    pub fn has_in_flight_work(&self) -> bool {
        self.pending_mutation_count > 0
            || !self.pending_category_requests.is_empty()
            || self.modals.drafts.operation_in_flight
            || self.pending_draft_operation.is_some()
            || self.search_is_pending()
            || self.mailbox.pending_thread_fetch.is_some()
            || self.mailbox.in_flight_thread_fetch.is_some()
            || !self.mailbox.in_flight_body_requests.is_empty()
            || !self.mailbox.thread_summary_in_flight.is_empty()
            || self.accounts.page.operation_in_flight
    }

    pub fn take_pending_undo(&mut self) -> Option<PendingUndo> {
        self.pending_undo.take()
    }

    pub fn open_saved_search_form_new(&mut self) {
        self.modals.saved_search_form = Some(SavedSearchFormState::for_new());
    }

    pub fn open_saved_search_form_for_edit(
        &mut self,
        name: String,
        query: String,
        search_mode: mxr_core::types::SearchMode,
    ) {
        self.modals.saved_search_form =
            Some(SavedSearchFormState::for_edit(name, query, search_mode));
    }

    pub fn close_saved_search_form(&mut self) {
        self.modals.saved_search_form = None;
    }

    pub fn take_saved_search_form_request(&mut self) -> Option<mxr_protocol::Request> {
        let form = self.modals.saved_search_form.as_mut()?;
        if form.existing_name.is_some() {
            form.validation_error =
                Some("internal error: edit form requires take_saved_search_form_requests".into());
            return None;
        }
        if let Some(error) = saved_search_form_validation_error(form) {
            form.validation_error = Some(error);
            return None;
        }
        let request = mxr_protocol::Request::CreateSavedSearch {
            name: form.name.clone(),
            query: form.query.clone(),
            account_id: None,
            search_mode: form.search_mode,
        };
        self.modals.saved_search_form = None;
        Some(request)
    }

    pub fn take_saved_search_form_requests(&mut self) -> Option<Vec<mxr_protocol::Request>> {
        let form = self.modals.saved_search_form.as_mut()?;
        if let Some(error) = saved_search_form_validation_error(form) {
            form.validation_error = Some(error);
            return None;
        }
        let mut requests = Vec::with_capacity(2);
        if let Some(old_name) = form.existing_name.clone() {
            requests.push(mxr_protocol::Request::DeleteSavedSearch { name: old_name });
        }
        requests.push(mxr_protocol::Request::CreateSavedSearch {
            name: form.name.clone(),
            query: form.query.clone(),
            account_id: None,
            search_mode: form.search_mode,
        });
        self.modals.saved_search_form = None;
        Some(requests)
    }
}

fn saved_search_form_validation_error(form: &SavedSearchFormState) -> Option<String> {
    let trimmed_name = form.name.trim();
    if trimmed_name.is_empty() {
        return Some("Saved search name is required".into());
    }
    if form.query.trim().is_empty() {
        return Some("Saved search query is required".into());
    }
    None
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
    body_status_labels_with_loading(metadata, source, show_reader_stats, false)
}

pub(crate) fn body_status_labels_with_loading(
    metadata: &BodyViewMetadata,
    source: &BodySource,
    show_reader_stats: bool,
    assets_loading: bool,
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
            if assets_loading {
                "Loading external assets…".into()
            } else {
                "remote images shown".into()
            }
        } else {
            "External content blocked — press M to allow once".into()
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
        (BodyViewMode::Html, _, BodySource::Html) => "View: Original HTML",
        (BodyViewMode::Html, _, BodySource::Plain) => "View: Plain text (no HTML)",
        (BodyViewMode::Html, _, BodySource::Fallback) => "View: Message summary (no HTML)",
        (BodyViewMode::Html, _, BodySource::Snippet) => "View: Snippet preview",
        (BodyViewMode::Text, true, _) => "View: Reading",
        (BodyViewMode::Text, false, BodySource::Plain) => "View: Plain text",
        (BodyViewMode::Text, false, BodySource::Html) => "View: HTML as text",
        (BodyViewMode::Text, false, BodySource::Fallback) => "View: Message summary",
        (BodyViewMode::Text, false, BodySource::Snippet) => "View: Snippet preview",
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
        link_count: 0,
        body_word_count: 0,
        label_provider_ids: vec![],
        keywords: std::collections::BTreeSet::new(),
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
        color: None,
        tag: None,
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
                use_tls,
                max_connections,
                tls_cert_sha256,
                ..
            } => {
                form.mode = AccountFormMode::ImapSmtp;
                form.imap_use_tls = use_tls;
                form.imap_tls_cert_sha256 = tls_cert_sha256;
                form.imap_host = host;
                form.imap_port = port.to_string();
                form.imap_max_connections = max_connections.to_string();
                form.imap_username = username;
                form.imap_password_ref = password_ref;
                form.imap_auth_required = auth_required;
            }
            mxr_protocol::AccountSyncConfigData::OutlookPersonal {
                client_id,
                token_ref,
            } => {
                form.mode = AccountFormMode::OutlookPersonal;
                form.outlook_client_id = client_id.unwrap_or_default();
                form.outlook_token_ref = token_ref;
            }
            mxr_protocol::AccountSyncConfigData::OutlookWork {
                client_id,
                token_ref,
            } => {
                form.mode = AccountFormMode::OutlookWork;
                form.outlook_client_id = client_id.unwrap_or_default();
                form.outlook_token_ref = token_ref;
            }
            mxr_protocol::AccountSyncConfigData::Fake => {}
        }
    } else {
        form.mode = AccountFormMode::SmtpOnly;
    }

    match account.send {
        Some(
            mxr_protocol::AccountSendConfigData::OutlookPersonal {
                token_ref,
                client_id,
            }
            | mxr_protocol::AccountSendConfigData::OutlookWork {
                token_ref,
                client_id,
            },
        ) => {
            if form.outlook_token_ref.is_empty() {
                form.outlook_token_ref = token_ref;
            }
            if form.outlook_client_id.is_empty() {
                if let Some(cid) = client_id {
                    form.outlook_client_id = cid;
                }
            }
        }
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
        Some(mxr_protocol::AccountSendConfigData::Fake) | None => {}
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
        (AccountFormMode::ImapSmtp, 6) => Some(form.imap_max_connections.as_str()),
        (AccountFormMode::ImapSmtp, 7) => Some(form.imap_username.as_str()),
        (AccountFormMode::ImapSmtp, 8) => None,
        (AccountFormMode::ImapSmtp, 9) => Some(form.imap_password_ref.as_str()),
        (AccountFormMode::ImapSmtp, 10) => Some(form.imap_password.as_str()),
        (AccountFormMode::ImapSmtp, 11) => Some(form.smtp_host.as_str()),
        (AccountFormMode::ImapSmtp, 12) => Some(form.smtp_port.as_str()),
        (AccountFormMode::ImapSmtp, 13) => Some(form.smtp_username.as_str()),
        (AccountFormMode::ImapSmtp, 14) => None,
        (AccountFormMode::ImapSmtp, 15) => Some(form.smtp_password_ref.as_str()),
        (AccountFormMode::ImapSmtp, 16) => Some(form.smtp_password.as_str()),
        (AccountFormMode::SmtpOnly, 4) => Some(form.smtp_host.as_str()),
        (AccountFormMode::SmtpOnly, 5) => Some(form.smtp_port.as_str()),
        (AccountFormMode::SmtpOnly, 6) => Some(form.smtp_username.as_str()),
        (AccountFormMode::SmtpOnly, 7) => None,
        (AccountFormMode::SmtpOnly, 8) => Some(form.smtp_password_ref.as_str()),
        (AccountFormMode::SmtpOnly, 9) => Some(form.smtp_password.as_str()),
        (AccountFormMode::OutlookPersonal | AccountFormMode::OutlookWork, 4) => {
            Some(form.outlook_client_id.as_str())
        }
        (AccountFormMode::OutlookPersonal | AccountFormMode::OutlookWork, 5) => None,
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
        (AccountFormMode::ImapSmtp, 6) => &mut form.imap_max_connections,
        (AccountFormMode::ImapSmtp, 7) => &mut form.imap_username,
        (AccountFormMode::ImapSmtp, 9) => &mut form.imap_password_ref,
        (AccountFormMode::ImapSmtp, 10) => &mut form.imap_password,
        (AccountFormMode::ImapSmtp, 11) => &mut form.smtp_host,
        (AccountFormMode::ImapSmtp, 12) => &mut form.smtp_port,
        (AccountFormMode::ImapSmtp, 13) => &mut form.smtp_username,
        (AccountFormMode::ImapSmtp, 15) => &mut form.smtp_password_ref,
        (AccountFormMode::ImapSmtp, 16) => &mut form.smtp_password,
        (AccountFormMode::SmtpOnly, 4) => &mut form.smtp_host,
        (AccountFormMode::SmtpOnly, 5) => &mut form.smtp_port,
        (AccountFormMode::SmtpOnly, 6) => &mut form.smtp_username,
        (AccountFormMode::SmtpOnly, 8) => &mut form.smtp_password_ref,
        (AccountFormMode::SmtpOnly, 9) => &mut form.smtp_password,
        (AccountFormMode::OutlookPersonal | AccountFormMode::OutlookWork, 4) => {
            &mut form.outlook_client_id
        }
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
        .map_or(value.len(), |(index, _)| index)
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

pub fn available_snooze_presets(config: &mxr_config::SnoozeConfig) -> Vec<SnoozePreset> {
    snooze_presets()
        .into_iter()
        .filter(|preset| mxr_config::snooze::resolve_snooze_preset_time(*preset, config).is_some())
        .collect()
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

    fn test_account_summary(
        email: &str,
        enabled: bool,
        is_default: bool,
    ) -> mxr_protocol::AccountSummaryData {
        mxr_protocol::AccountSummaryData {
            identity: None,
            account_id: mxr_core::AccountId::new(),
            key: Some(email.to_string()),
            name: email.to_string(),
            email: email.to_string(),
            provider_kind: "imap".into(),
            sync_kind: Some("imap".into()),
            send_kind: Some("smtp".into()),
            enabled,
            is_default,
            source: mxr_protocol::AccountSourceData::Config,
            editable: mxr_protocol::AccountEditModeData::Full,
            sync: None,
            send: None,
            capabilities: Default::default(),
        }
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

    #[test]
    fn mailbox_sidebar_hides_disabled_sync_accounts_without_removing_account_records() {
        let mut app = App::new();
        app.mailbox.sidebar_accounts_expanded = true;
        let enabled_primary = test_account_summary("primary@example.com", true, true);
        let disabled = test_account_summary("disabled@example.com", false, false);
        let enabled_secondary = test_account_summary("secondary@example.com", true, false);
        app.accounts.page.accounts = vec![
            enabled_primary.clone(),
            disabled.clone(),
            enabled_secondary.clone(),
        ];

        let sidebar_accounts: Vec<_> = app
            .sidebar_items()
            .into_iter()
            .filter_map(|item| match item {
                SidebarItem::Account(account) => Some(account.email),
                _ => None,
            })
            .collect();

        assert_eq!(
            sidebar_accounts,
            vec![
                enabled_primary.email.clone(),
                enabled_secondary.email.clone()
            ]
        );
        assert_eq!(app.accounts.page.accounts.len(), 3);
        assert!(app
            .accounts
            .page
            .accounts
            .iter()
            .any(|account| account.email == disabled.email));

        let rendered_accounts: Vec<_> = app
            .sidebar_view()
            .accounts
            .iter()
            .map(|account| account.email.clone())
            .collect();
        assert_eq!(rendered_accounts, sidebar_accounts);
    }
}
