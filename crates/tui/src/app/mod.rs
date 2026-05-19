mod account_actions;
mod account_form_helpers;
mod actions;
mod analytics_actions;
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
pub mod mutation_snapshot;
mod pending_optimistic;
mod platform_actions;
mod recorder;
mod rule_actions;
mod runtime_helpers;
mod saved_search_actions;
mod screen_actions;
mod screen_helpers;
mod search_actions;
mod search_helpers;
mod selection_helpers;
mod semantic_actions;
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
use recorder::ActionRecorder;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use throbber_widgets_tui::ThrobberState;
use tui_textarea::TextArea;

pub(in crate::app) use crate::ui::label_picker::LabelPickerMode;
pub(crate) use mailbox_helpers::auto_summary_eligible;
pub use mutation_snapshot::{
    MutationId, MutationIdGenerator, MutationSnapshot, MutationSnapshotStore, QueuedMutation,
    TRANSIENT_MUTATION_MAX_RETRIES,
};
pub use pending_optimistic::PendingOptimisticState;
use state::PendingPreviewRead;
pub use state::*;

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
    RefreshList,
    StatusOnly(String),
    /// Successful SendDraft. Refreshes the active label so a Sent-view user
    /// sees the just-sent message immediately (no manual sync), and shows
    /// `status` in the status bar.
    SentSuccess {
        status: String,
        remind_at: Option<chrono::DateTime<chrono::Utc>>,
        sent_message_id: Option<MessageId>,
    },
}

/// Slice 5.1/5.2 (C2.6): which briefing the runtime should fetch.
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
}

/// Health of the IPC connection to the daemon. Drives the status bar and the
/// "daemon not responding" modal — replaces the old behaviour where a failed
/// initial connect silently exited the IPC worker and left the UI hung at
/// "connecting".
#[derive(Debug, Clone)]
pub enum ConnectionState {
    Connecting,
    Connected,
    Reconnecting {
        since: std::time::Instant,
        reason: String,
    },
}

/// Wall-clock window before a `Reconnecting` state escalates to an error
/// modal. Five seconds matches the existing `START_DAEMON_TIMEOUT` so
/// users see feedback right after the auto-restart attempt would have
/// succeeded if the daemon were healthy.
const CONNECTION_STALE_THRESHOLD: std::time::Duration = std::time::Duration::from_secs(5);

/// Client-side undo window. Matches the daemon's 60s expiry so the TUI
/// stops offering an undo affordance the daemon would refuse.
const UNDO_HINT_TTL: std::time::Duration = std::time::Duration::from_secs(60);

/// Captured handle for a recent undoable mutation. The TUI uses this to
/// show "Archived 5 — u to undo" in the status bar and to dispatch
/// `Request::UndoMutation` when the user presses `u`.
#[derive(Debug, Clone)]
pub struct PendingUndo {
    pub mutation_id: String,
    pub verb_past: String,
    pub count: u32,
    pub applied_at: std::time::Instant,
}

/// Hold-and-undo window for an iCal invite RSVP. Differs from `PendingUndo`
/// in that the daemon RPC has not fired yet — pressing `u` within the window
/// cancels it outright, so no email reaches the organizer. The status bar
/// rendering reads `status_label` directly while this is `Some`.
#[derive(Debug, Clone)]
pub struct PendingInviteSend {
    pub message_id: mxr_core::MessageId,
    pub action: mxr_protocol::CalendarInviteActionData,
    pub dispatch_at: std::time::Instant,
    pub status_label: String,
}

/// Pending `SetScreenerDecision` request queued by the screener
/// modal. Drained by the runtime each tick.
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
    pub theme: Theme,
    pub mailbox: MailboxState,
    pub search: SearchState,
    pub accounts: AccountsState,
    pub rules: RulesState,
    pub diagnostics: DiagnosticsState,
    pub analytics: AnalyticsState,
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
    pub pending_mutation_queue: Vec<QueuedMutation>,
    pub mutation_snapshots: MutationSnapshotStore,
    pub mutation_id_generator: MutationIdGenerator,
    /// Tracks message-id state of in-flight optimistic mutations so a
    /// stale envelope refresh from the daemon can't undo a still-pending
    /// optimistic change. See `pending_optimistic.rs` for the full bug
    /// description.
    pub pending_optimistic: PendingOptimisticState,
    pub connection_state: ConnectionState,
    /// Set by the input handler when the user presses "retry now" while the
    /// connection-error modal is open. The IPC worker drains and clears it.
    pub pending_connection_retry: bool,
    /// Set when the snippets browser modal opens; the runtime drains
    /// this flag and dispatches a `Request::ListSnippets`.
    pub pending_snippets_refresh: bool,
    /// Set when the reply-later queue modal opens; the runtime drains
    /// this flag and dispatches a `Request::ListReplyQueue`.
    pub pending_reply_queue_refresh: bool,
    /// Set when the activity modal opens (or the user requests a refresh);
    /// the runtime drains this and dispatches a `Request::ListActivity`.
    pub pending_activity_refresh: bool,
    /// Set when the user toggles pause/resume from the activity modal.
    pub pending_activity_pause_toggle: bool,
    /// Pending sender-view request — `Some((account_id, email))`
    /// triggers a `Request::GetSenderProfile`. Drained by the runtime
    /// after dispatch.
    pub pending_sender_profile_request: Option<(mxr_core::AccountId, String)>,
    /// Pending thread-summary request — `Some(thread_id)` triggers a
    /// `Request::SummarizeThread`. Drained by the runtime.
    pub pending_summary_request: Option<mxr_core::ThreadId>,
    /// Trailing-edge debounce for the lazy on-thread-open summary fire.
    /// Scrolling through the mail list opens a thread per row, which
    /// would otherwise fire an LLM request for every keypress. We park
    /// the request here with a deadline; only when the deadline elapses
    /// without being replaced does it move to `pending_summary_request`
    /// and actually go to the daemon. In-flight requests already fired
    /// are not cancelled — they complete and the daemon caches them.
    pub pending_summary_debounce: Option<(mxr_core::ThreadId, tokio::time::Instant)>,
    /// Slice 5.1/5.2 (C2.6): pending briefing fetch. Drained by the
    /// runtime, which fires either `Request::GetThreadBriefing` or
    /// `Request::GetRecipientBriefing` depending on the variant.
    pub pending_briefing_request: Option<BriefingRequest>,
    /// Slice 6.1 (C2.9): pending whois query. Drained by the runtime
    /// which fires `Request::ExplainEntity`.
    pub pending_whois_query: Option<String>,
    /// Slice 5.4 (C2.8 cont): pending expert-finder query. Drained
    /// by the runtime which fires `Request::FindExpert`.
    pub pending_expert_query: Option<String>,
    /// Pending screener queue refresh — set when the modal opens or
    /// after a disposition lands so the runtime re-fetches the queue.
    pub pending_screener_refresh: Option<mxr_core::AccountId>,
    /// Queue of screener-decision IPCs to dispatch. Populated by the
    /// disposition keypaths (a/d/f/p) and drained by the runtime.
    pub pending_screener_decisions: Vec<PendingScreenerDecision>,
    /// Recent undoable mutation, if any. Cleared by `tick_pending_undo`
    /// once the daemon-side window expires.
    pub pending_undo: Option<PendingUndo>,
    /// iCal invite RSVP queued for send after a 1s hold window. While set,
    /// pressing `u` cancels without ever dispatching to the daemon (no email
    /// goes out). `tick_pending_invite_send` promotes it into the mutation
    /// queue once `dispatch_at` arrives.
    pub pending_invite_send: Option<PendingInviteSend>,
    /// Default destination for the "save attachment as..." modal,
    /// mirrored from `config.general.download_dir`. Falls back to
    /// `dirs::download_dir()` when no config is available.
    pub download_dir: std::path::PathBuf,
    recorder: ActionRecorder,
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
        self.download_dir = config.general.download_dir.clone();
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
            analytics: AnalyticsState::default(),
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
            mutation_snapshots: MutationSnapshotStore::default(),
            mutation_id_generator: MutationIdGenerator::default(),
            pending_optimistic: PendingOptimisticState::default(),
            connection_state: ConnectionState::Connecting,
            pending_connection_retry: false,
            pending_snippets_refresh: false,
            pending_reply_queue_refresh: false,
            pending_activity_refresh: false,
            pending_activity_pause_toggle: false,
            pending_sender_profile_request: None,
            pending_summary_request: None,
            pending_summary_debounce: None,
            pending_briefing_request: None,
            pending_whois_query: None,
            pending_expert_query: None,
            pending_screener_refresh: None,
            pending_screener_decisions: Vec::new(),
            pending_undo: None,
            pending_invite_send: None,
            download_dir: dirs::download_dir().unwrap_or_else(|| {
                dirs::home_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                    .join("Downloads")
            }),
            recorder: ActionRecorder::new(1000),
            input: InputHandler::new(),
        }
    }

    /// Update the IPC connection health and propagate side effects:
    /// - On `Connected`, close any existing "daemon not responding" error
    ///   modal so the user isn't left with a stale dialog after recovery.
    /// - On `Reconnecting` or `Connecting`, leave the modal alone — the
    ///   tick-based opener decides when to surface it.
    pub fn set_connection_state(&mut self, state: ConnectionState) {
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
        self.connection_state = state;
    }

    /// Called from the main event loop on every tick. If the connection has
    /// been `Reconnecting` for more than `CONNECTION_STALE_THRESHOLD`,
    /// surface an error modal explaining the daemon is not responding.
    /// Idempotent — re-opening an already-open modal is fine.
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

    /// Mark a "retry now" request from the user. The IPC worker drains and
    /// clears this flag on the next iteration.
    pub fn request_connection_retry(&mut self) {
        self.pending_connection_retry = true;
    }

    /// Human-readable summary of the IPC connection state for the status bar.
    /// Returns `None` when `Connected` so the bar shows normal mailbox info.
    pub fn connection_state_label(&self) -> Option<String> {
        match &self.connection_state {
            ConnectionState::Connected => None,
            ConnectionState::Connecting => Some("Connecting to daemon...".into()),
            ConnectionState::Reconnecting { reason, .. } => {
                Some(format!("Reconnecting to daemon ({reason})..."))
            }
        }
    }

    /// Push a user-visible warning into the bounded error log. Surfaces in
    /// the status bar (auto-clears after 5s). Use for transient async
    /// failures that the user might want to know about but shouldn't
    /// block: HTML asset fetch, body parse, attachment fetch, etc.
    pub fn report_warn(&mut self, message: impl Into<String>) {
        self.push_user_error(UserError {
            severity: UserErrorSeverity::Warn,
            message: message.into(),
            title: None,
            since: std::time::Instant::now(),
        });
    }

    /// Push a user-visible error and escalate it to the error modal. Use
    /// for failures that demand user attention (mutation rollback, send
    /// failure, etc). The modal opens even if the status bar is occupied.
    pub fn report_error(&mut self, title: impl Into<String>, detail: impl Into<String>) {
        let title = title.into();
        let detail = detail.into();
        self.push_user_error(UserError {
            severity: UserErrorSeverity::Error,
            message: detail.clone(),
            title: Some(title.clone()),
            since: std::time::Instant::now(),
        });
        // Surface immediately. If a modal is already open, leave it — the
        // user will see the remaining errors via the log; we don't stack
        // modals on top of each other.
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

    /// Apply a `UserError` raised by a background task. Used by the main
    /// event loop when consuming `AsyncResult::ReportedError`. Errors
    /// also escalate to the modal; warns just go in the ring buffer.
    pub fn push_reported_error(&mut self, entry: UserError) {
        let escalate = matches!(entry.severity, UserErrorSeverity::Error);
        let title = entry.title.clone().unwrap_or_else(|| "Error".to_string());
        let message = entry.message.clone();
        self.push_user_error(entry);
        if escalate && self.modals.error.is_none() {
            self.modals.error = Some(crate::app::ErrorModalState::new(title, message));
        }
    }

    /// Latest unexpired warning suitable for the status bar. Returns
    /// `None` when the most recent warn is older than the TTL or when
    /// the log has no warns. Errors don't surface here — they're shown
    /// via the error modal instead.
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

    /// Record a recent undoable mutation so the status bar can prompt
    /// "Archived 15 — u to undo" and the input handler can resolve `u`
    /// to a specific `mutation_id`. Replaces any prior handle (the
    /// most recent one wins; the daemon stops offering older ones).
    pub fn set_pending_undo(&mut self, undo: PendingUndo) {
        self.pending_undo = Some(undo);
    }

    /// Drop the pending undo handle once it's expired client-side. The
    /// daemon would refuse the request anyway, so don't even try.
    pub fn tick_pending_undo(&mut self, now: std::time::Instant) {
        if let Some(undo) = &self.pending_undo {
            if now.saturating_duration_since(undo.applied_at) >= UNDO_HINT_TTL {
                self.pending_undo = None;
            }
        }
    }

    /// Promote a held iCal RSVP into the mutation queue once its 1s hold
    /// window expires. Until then the slot stays armed so pressing `u`
    /// cancels without ever talking to the daemon.
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

    /// Status-bar text for the active undo affordance, e.g.
    /// `"Archived 15 — u to undo"`. Returns `None` when no fresh
    /// pending undo exists; pairs with the override chain in
    /// `body_helpers::status_bar_state`.
    pub fn pending_undo_label(&self, now: std::time::Instant) -> Option<String> {
        let undo = self.pending_undo.as_ref()?;
        if now.saturating_duration_since(undo.applied_at) >= UNDO_HINT_TTL {
            return None;
        }
        Some(format!("{} {} — u to undo", undo.verb_past, undo.count))
    }

    /// Take the active undo handle, returning the `mutation_id` to
    /// dispatch in `Request::UndoMutation`. Clears the local handle
    /// regardless of the daemon's response — preserves the "u undoes
    /// the most recent" semantics even if the daemon errors.
    pub fn take_pending_undo(&mut self) -> Option<PendingUndo> {
        self.pending_undo.take()
    }

    /// Open a fresh saved-search form (no prefill).
    pub fn open_saved_search_form_new(&mut self) {
        self.modals.saved_search_form = Some(SavedSearchFormState::for_new());
    }

    /// Open the saved-search form prefilled for edit. Save will produce
    /// a Delete (for the old name) followed by a Create.
    pub fn open_saved_search_form_for_edit(
        &mut self,
        name: String,
        query: String,
        search_mode: mxr_core::types::SearchMode,
    ) {
        self.modals.saved_search_form =
            Some(SavedSearchFormState::for_edit(name, query, search_mode));
    }

    /// Close the saved-search form without dispatching anything.
    pub fn close_saved_search_form(&mut self) {
        self.modals.saved_search_form = None;
    }

    /// Validate the form and produce a single Create request for the
    /// new-saved-search path. Returns `None` if validation fails;
    /// the form stays open with `validation_error` populated so the
    /// caller can surface it. Use `take_saved_search_form_requests`
    /// for the edit path.
    pub fn take_saved_search_form_request(&mut self) -> Option<mxr_protocol::Request> {
        let form = self.modals.saved_search_form.as_mut()?;
        if form.existing_name.is_some() {
            // Edit path is delete+create — caller used the wrong helper.
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
            search_mode: form.search_mode,
        };
        self.modals.saved_search_form = None;
        Some(request)
    }

    /// Validate the form and produce delete+create requests for the
    /// edit path (or just create for the new path). Returns `None` if
    /// validation fails. Form is closed on success.
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

/// Phase 3.4: extended chip set that surfaces in-flight remote asset
/// loading. The async fetch path can take a few hundred ms when the
/// network is slow; without this chip the rendered HTML view sits
/// silently while the user wonders if anything is happening.
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
            // Test-only provider; do not surface in account-edit forms.
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
        (AccountFormMode::OutlookPersonal, 4) | (AccountFormMode::OutlookWork, 4) => {
            Some(form.outlook_client_id.as_str())
        }
        (AccountFormMode::OutlookPersonal, 5) | (AccountFormMode::OutlookWork, 5) => None, // token_ref is read-only
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
        (AccountFormMode::OutlookPersonal, 4) | (AccountFormMode::OutlookWork, 4) => {
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
    use chrono::{Duration, TimeZone};

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
    fn build_mail_list_rows_counts_other_thread_participants_excluding_representative_from() {
        let thread_id = mxr_core::ThreadId::new();
        let t0 = chrono::Utc::now();
        let alice_first = TestEnvelopeBuilder::new()
            .thread_id(thread_id.clone())
            .subject("first")
            .provider_id("m1")
            .date(t0)
            .with_from_address("Alice", "alice@example.com")
            .to(vec![Address {
                name: None,
                email: "bob@example.com".into(),
            }])
            .message_id_header(None)
            .snippet("")
            .size_bytes(0)
            .build();
        let bob_reply = TestEnvelopeBuilder::new()
            .thread_id(thread_id.clone())
            .subject("re")
            .provider_id("m2")
            .date(t0 + Duration::seconds(1))
            .with_from_address("Bob", "bob@example.com")
            .to(vec![Address {
                name: None,
                email: "alice@example.com".into(),
            }])
            .message_id_header(None)
            .snippet("")
            .size_bytes(0)
            .build();

        let rows = App::build_mail_list_rows(&[alice_first, bob_reply], MailListMode::Threads);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].message_count, 2);
        assert_eq!(rows[0].representative.from.email, "bob@example.com");
        assert_eq!(rows[0].other_participant_count, 1);
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
