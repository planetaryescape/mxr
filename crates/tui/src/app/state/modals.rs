use super::super::MutationEffect;
use crate::ui::label_picker::{LabelPicker, LabelPickerMode};
use mxr_core::id::{AccountId, MessageId};
use mxr_core::Envelope;
use mxr_protocol::{Request, ScreenerQueueEntryData, SenderProfileData, SnippetData};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Maximum number of user-visible errors retained in the ring buffer.
/// Beyond this, oldest entries are dropped — bounded memory under
/// error storms (e.g. flaky body parser hammering the reporter).
pub const USER_ERROR_LOG_CAPACITY: usize = 5;

/// Severity of a reported user-visible error. Warns surface in the
/// status bar (auto-clear after 5s); Errors escalate to a modal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserErrorSeverity {
    Warn,
    Error,
}

#[derive(Debug, Clone)]
pub struct UserError {
    pub severity: UserErrorSeverity,
    pub message: String,
    pub title: Option<String>,
    pub since: Instant,
}

/// How long a warn remains visible in the status bar before clearing.
pub const WARN_STATUS_TTL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Default)]
pub struct FeatureOnboardingState {
    pub visible: bool,
    pub step: usize,
    pub seen: bool,
}

pub use mxr_config::snooze::{SnoozeOption as SnoozePreset, SNOOZE_PRESETS};

#[derive(Debug, Clone, Default)]
pub struct SnoozePanelState {
    pub visible: bool,
    pub selected_index: usize,
    /// `Some(buffer)` while the user is typing a custom-time expression
    /// in the panel. `None` means the preset list is the active surface.
    pub custom_input: Option<String>,
    /// Most recent parser error for the custom input, surfaced in the
    /// modal so the user can correct without leaving the prompt.
    pub custom_error: Option<String>,
}

#[derive(Default)]
pub struct ModalsState {
    pub help_open: bool,
    pub help_scroll_offset: u16,
    pub help_query: String,
    pub help_selected: usize,
    pub onboarding: FeatureOnboardingState,
    pub label_picker: LabelPicker,
    pub snooze_panel: SnoozePanelState,
    pub snooze_config: mxr_config::SnoozeConfig,
    pub pending_label_action: Option<(LabelPickerMode, String)>,
    pub pending_bulk_confirm: Option<PendingBulkConfirm>,
    pub error: Option<ErrorModalState>,
    pub pending_unsubscribe_confirm: Option<PendingUnsubscribeConfirm>,
    pub pending_unsubscribe_action: Option<PendingUnsubscribeAction>,
    /// Ring buffer of user-visible warns and errors. Bounded so error
    /// storms don't leak memory; oldest entries are dropped.
    pub error_log: VecDeque<UserError>,
    /// Modal state for creating or editing a saved search. Visible
    /// when `Some`. When `existing_name` is set the save action
    /// does delete-then-create so the daemon's UNIQUE-name constraint
    /// doesn't reject the update.
    pub saved_search_form: Option<SavedSearchFormState>,
    /// Queue of saved-search IPC requests waiting for the dispatcher
    /// to send. Stored as a Vec so the edit path can enqueue
    /// `[Delete, Create]` atomically.
    pub pending_saved_search_dispatch: Vec<Request>,
    /// Set after a saved-search mutation completes so the next
    /// dispatcher tick refreshes `app.mailbox.saved_searches`.
    pub pending_saved_search_refresh: bool,
    /// `Some(name)` while a delete confirmation is open. Pressing
    /// `y`/`Enter` dispatches the delete; `n`/`Esc` cancels.
    pub pending_saved_search_delete_confirm: Option<String>,
    /// Queue of semantic-runtime IPC requests waiting to be dispatched.
    /// Populated by palette actions (Enable/Disable/Reindex/Install
    /// Profile); drained one-at-a-time by the lib.rs dispatcher.
    pub pending_semantic_dispatch: Vec<Request>,
    /// Queue of one-shot platform/AI requests whose result should be
    /// shown in a read-only modal (draft suggestions, commitments, voice).
    pub pending_platform_dispatch: Vec<PendingPlatformDispatch>,
    pub platform: PlatformModalState,
    /// Modal for editing the active analytics view's filter
    /// parameters in one form. Populated when the user presses `f`
    /// inside the Analytics screen; cleared on Esc/Enter.
    pub analytics_filter: Option<AnalyticsFilterModalState>,
    /// Browser modal listing the user's compose snippets. Read-only:
    /// add/edit/delete still flow through `mxr snippets` CLI; the modal
    /// surfaces the list so users discover what's available without
    /// leaving the TUI.
    pub snippets: SnippetsModalState,
    /// Browser modal showing per-sender relationship aggregates
    /// (volume, response cadence, open commitments). Surfaced via the
    /// command palette so users can drill into a sender without
    /// leaving the inbox.
    pub sender_profile: SenderProfileModalState,
    /// Triage queue modal listing senders awaiting a screener
    /// decision. Supports three-key disposition (allow / deny / feed /
    /// paper-trail) that fires `SetScreenerDecision`.
    pub screener: ScreenerModalState,
    /// Reply-later queue modal listing flagged messages so the user
    /// can walk them. Read-only: actually replying still uses the
    /// regular reply flow once the user opens the focused message.
    pub reply_queue: ReplyQueueModalState,
    /// Thread-summary modal showing the LLM-generated 2-3 sentence
    /// summary of the focused thread. Loading / error / disabled
    /// states all surface inline.
    pub summary: ThreadSummaryModalState,
    /// Slice 5.1/5.2 (C2.6): briefing modal for "returning to a
    /// dormant thread / recipient" context. Holds either a thread
    /// briefing or a recipient briefing.
    pub briefing: BriefingModalState,
}

#[derive(Debug, Clone)]
pub struct PendingPlatformDispatch {
    pub prelude: Vec<Request>,
    pub request: Request,
    pub title: String,
    pub loading: String,
}

#[derive(Debug, Clone, Default)]
pub struct PlatformModalState {
    pub visible: bool,
    pub loading: bool,
    pub title: String,
    pub body: Option<String>,
    pub error: Option<String>,
}

impl PlatformModalState {
    pub fn open_loading(&mut self, title: impl Into<String>, loading: impl Into<String>) {
        self.visible = true;
        self.loading = true;
        self.title = title.into();
        self.body = Some(loading.into());
        self.error = None;
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.loading = false;
        self.body = None;
        self.error = None;
    }

    pub fn set_body(&mut self, body: String) {
        self.loading = false;
        self.body = Some(body);
        self.error = None;
    }

    pub fn set_error(&mut self, message: String) {
        self.loading = false;
        self.body = None;
        self.error = Some(message);
    }
}

/// Read-only state for the snippets browser modal. `visible=true`
/// while the modal is open; `loading=true` between dispatch and
/// response so the UI can show a spinner instead of "no snippets".
#[derive(Debug, Clone, Default)]
pub struct SnippetsModalState {
    pub visible: bool,
    pub loading: bool,
    pub snippets: Vec<SnippetData>,
    pub selected_index: usize,
    pub error: Option<String>,
}

impl SnippetsModalState {
    pub fn open_loading(&mut self) {
        self.visible = true;
        self.loading = true;
        self.snippets.clear();
        self.selected_index = 0;
        self.error = None;
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.loading = false;
        self.error = None;
    }

    pub fn set_snippets(&mut self, snippets: Vec<SnippetData>) {
        self.loading = false;
        self.error = None;
        self.snippets = snippets;
        self.selected_index = 0;
    }

    pub fn set_error(&mut self, message: String) {
        self.loading = false;
        self.error = Some(message);
    }

    pub fn select_next(&mut self) {
        if self.snippets.is_empty() {
            return;
        }
        self.selected_index = (self.selected_index + 1) % self.snippets.len();
    }

    pub fn select_prev(&mut self) {
        if self.snippets.is_empty() {
            return;
        }
        self.selected_index = self
            .selected_index
            .checked_sub(1)
            .unwrap_or(self.snippets.len() - 1);
    }

    pub fn selected(&self) -> Option<&SnippetData> {
        self.snippets.get(self.selected_index)
    }
}

/// Read-only state for the sender-profile browser modal. Either holds
/// a fetched profile, an error, or a "loading" placeholder while the
/// IPC call is in-flight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SenderProfileTab {
    #[default]
    Overview,
    Relationship,
    Messages,
}

#[derive(Debug, Clone, Default)]
pub struct SenderProfileModalState {
    pub visible: bool,
    pub loading: bool,
    /// Email address whose profile is being shown. Kept on the state so
    /// the title bar can read it back even when the response is still
    /// pending.
    pub email: Option<String>,
    pub current_thread_id: Option<mxr_core::ThreadId>,
    pub profile: Option<SenderProfileData>,
    pub active_tab: SenderProfileTab,
    pub selected_recent_index: usize,
    pub error: Option<String>,
}

impl SenderProfileModalState {
    pub fn open_loading(&mut self, email: String, current_thread_id: Option<mxr_core::ThreadId>) {
        self.visible = true;
        self.loading = true;
        self.email = Some(email);
        self.current_thread_id = current_thread_id;
        self.profile = None;
        self.active_tab = SenderProfileTab::Overview;
        self.selected_recent_index = 0;
        self.error = None;
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.loading = false;
        self.email = None;
        self.current_thread_id = None;
        self.profile = None;
        self.active_tab = SenderProfileTab::Overview;
        self.selected_recent_index = 0;
        self.error = None;
    }

    pub fn set_profile(&mut self, profile: Option<SenderProfileData>) {
        self.loading = false;
        self.error = None;
        self.profile = profile;
        self.active_tab = SenderProfileTab::Overview;
        self.selected_recent_index = 0;
    }

    pub fn set_error(&mut self, message: String) {
        self.loading = false;
        self.error = Some(message);
    }

    pub fn select_tab(&mut self, tab: SenderProfileTab) {
        self.active_tab = tab;
    }

    pub fn recent_messages(&self) -> Vec<&mxr_protocol::SenderEmailReferenceData> {
        self.profile
            .as_ref()
            .map(|profile| {
                profile
                    .recent_messages
                    .iter()
                    .filter(|message| {
                        self.current_thread_id
                            .as_ref()
                            .map_or(true, |thread_id| &message.thread_id != thread_id)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn select_next_recent_message(&mut self) {
        let len = self.recent_messages().len();
        if len == 0 {
            self.selected_recent_index = 0;
            return;
        }
        self.selected_recent_index = (self.selected_recent_index + 1).min(len - 1);
    }

    pub fn select_prev_recent_message(&mut self) {
        self.selected_recent_index = self.selected_recent_index.saturating_sub(1);
    }

    pub fn selected_recent_message(&self) -> Option<mxr_protocol::SenderEmailReferenceData> {
        self.recent_messages()
            .get(self.selected_recent_index)
            .map(|message| (*message).clone())
    }
}

/// State for the screener triage modal. Holds the queue of senders
/// without a decision yet; key dispositions remove the entry from the
/// list optimistically and fire the IPC.
#[derive(Debug, Clone, Default)]
pub struct ScreenerModalState {
    pub visible: bool,
    pub loading: bool,
    pub account_id: Option<mxr_core::AccountId>,
    pub entries: Vec<ScreenerQueueEntryData>,
    pub selected_index: usize,
    pub error: Option<String>,
}

impl ScreenerModalState {
    pub fn open_loading(&mut self, account_id: mxr_core::AccountId) {
        self.visible = true;
        self.loading = true;
        self.account_id = Some(account_id);
        self.entries.clear();
        self.selected_index = 0;
        self.error = None;
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.loading = false;
        self.account_id = None;
        self.entries.clear();
        self.error = None;
    }

    pub fn set_entries(&mut self, entries: Vec<ScreenerQueueEntryData>) {
        self.loading = false;
        self.error = None;
        self.entries = entries;
        self.selected_index = 0;
    }

    pub fn set_error(&mut self, message: String) {
        self.loading = false;
        self.error = Some(message);
    }

    pub fn select_next(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.selected_index = (self.selected_index + 1) % self.entries.len();
    }

    pub fn select_prev(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.selected_index = self
            .selected_index
            .checked_sub(1)
            .unwrap_or(self.entries.len() - 1);
    }

    pub fn selected(&self) -> Option<&ScreenerQueueEntryData> {
        self.entries.get(self.selected_index)
    }

    /// Remove the currently-selected entry (after a disposition has
    /// been applied) and clamp the cursor so it stays within range.
    pub fn remove_selected(&mut self) -> Option<ScreenerQueueEntryData> {
        if self.entries.is_empty() {
            return None;
        }
        let removed = self.entries.remove(self.selected_index);
        if self.selected_index >= self.entries.len() && !self.entries.is_empty() {
            self.selected_index = self.entries.len() - 1;
        } else if self.entries.is_empty() {
            self.selected_index = 0;
        }
        Some(removed)
    }
}

/// State for the reply-later queue modal. Read-only listing of
/// flagged messages; replying still flows through the regular
/// compose pipeline once the user opens the focused message.
#[derive(Debug, Clone, Default)]
pub struct ReplyQueueModalState {
    pub visible: bool,
    pub loading: bool,
    pub messages: Vec<Envelope>,
    pub selected_index: usize,
    pub error: Option<String>,
}

impl ReplyQueueModalState {
    pub fn open_loading(&mut self) {
        self.visible = true;
        self.loading = true;
        self.messages.clear();
        self.selected_index = 0;
        self.error = None;
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.loading = false;
        self.error = None;
    }

    pub fn set_messages(&mut self, messages: Vec<Envelope>) {
        self.loading = false;
        self.error = None;
        self.messages = messages;
        self.selected_index = 0;
    }

    pub fn set_error(&mut self, message: String) {
        self.loading = false;
        self.error = Some(message);
    }

    pub fn select_next(&mut self) {
        if self.messages.is_empty() {
            return;
        }
        self.selected_index = (self.selected_index + 1) % self.messages.len();
    }

    pub fn select_prev(&mut self) {
        if self.messages.is_empty() {
            return;
        }
        self.selected_index = self
            .selected_index
            .checked_sub(1)
            .unwrap_or(self.messages.len() - 1);
    }

    pub fn selected(&self) -> Option<&Envelope> {
        self.messages.get(self.selected_index)
    }
}

/// State for the thread-summary modal. Holds the LLM result while the
/// modal is visible; `error` carries the daemon-side message verbatim
/// (e.g. `LlmDisabled`, `ThreadTooShort`).
#[derive(Debug, Clone, Default)]
pub struct ThreadSummaryModalState {
    pub visible: bool,
    pub loading: bool,
    /// Thread identifier currently being summarized. Used to drop late
    /// responses for a previously-focused thread.
    pub thread_id: Option<mxr_core::ThreadId>,
    pub summary: Option<String>,
    pub model: Option<String>,
    pub error: Option<String>,
}

impl ThreadSummaryModalState {
    pub fn open_loading(&mut self, thread_id: mxr_core::ThreadId) {
        self.visible = true;
        self.loading = true;
        self.thread_id = Some(thread_id);
        self.summary = None;
        self.model = None;
        self.error = None;
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.loading = false;
        self.thread_id = None;
        self.summary = None;
        self.model = None;
        self.error = None;
    }

    pub fn set_summary(&mut self, text: String, model: String) {
        self.loading = false;
        self.summary = Some(text);
        self.model = Some(model);
        self.error = None;
    }

    pub fn set_error(&mut self, message: String) {
        self.loading = false;
        self.summary = None;
        self.model = None;
        self.error = Some(message);
    }
}

/// Slice 5.1 / 5.2 wiring (C2.6): briefing modal state. Holds
/// either a thread briefing or a recipient briefing; the kind is
/// encoded by which Subject was set on opening.
#[derive(Debug, Clone, Default)]
pub struct BriefingModalState {
    pub visible: bool,
    pub loading: bool,
    pub subject: Option<BriefingModalSubject>,
    pub body_markdown: Option<String>,
    pub citations: Vec<mxr_protocol::CitationRefData>,
    pub generated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub from_cache: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BriefingModalSubject {
    Thread(mxr_core::ThreadId),
    Recipient(String),
}

impl BriefingModalState {
    pub fn open_thread_loading(&mut self, thread_id: mxr_core::ThreadId) {
        self.visible = true;
        self.loading = true;
        self.subject = Some(BriefingModalSubject::Thread(thread_id));
        self.body_markdown = None;
        self.citations.clear();
        self.generated_at = None;
        self.from_cache = false;
        self.error = None;
    }

    pub fn open_recipient_loading(&mut self, email: String) {
        self.visible = true;
        self.loading = true;
        self.subject = Some(BriefingModalSubject::Recipient(email));
        self.body_markdown = None;
        self.citations.clear();
        self.generated_at = None;
        self.from_cache = false;
        self.error = None;
    }

    pub fn set_briefing(
        &mut self,
        body: String,
        citations: Vec<mxr_protocol::CitationRefData>,
        generated_at: chrono::DateTime<chrono::Utc>,
        from_cache: bool,
    ) {
        self.loading = false;
        self.body_markdown = Some(body);
        self.citations = citations;
        self.generated_at = Some(generated_at);
        self.from_cache = from_cache;
        self.error = None;
    }

    pub fn set_error(&mut self, message: String) {
        self.loading = false;
        self.error = Some(message);
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.loading = false;
        self.subject = None;
        self.body_markdown = None;
        self.citations.clear();
        self.generated_at = None;
        self.from_cache = false;
        self.error = None;
    }
}

/// Form state for the analytics filter modal. Holds string-form
/// fields per active view so the user can edit numeric inputs by
/// typing; on submit, the analytics action handler parses them back
/// into the typed `AnalyticsState` fields.
#[derive(Debug, Clone)]
pub struct AnalyticsFilterModalState {
    pub view: crate::app::AnalyticsView,
    pub active_field: usize,
    pub fields: Vec<AnalyticsFilterField>,
    pub validation_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AnalyticsFilterField {
    pub label: String,
    pub value: String,
    pub options: Vec<String>,
}

/// Modal form for creating (or editing via delete+create) a saved
/// search. Mirrors the shape of `RuleFormState` but kept distinct so
/// the rule-form's tui-textarea editors aren't carried into a much
/// simpler two-line form.
#[derive(Debug, Clone)]
pub struct SavedSearchFormState {
    /// `Some(old_name)` when editing — save first deletes the old row.
    pub existing_name: Option<String>,
    pub name: String,
    pub query: String,
    /// `lexical` / `semantic` / `hybrid` etc., serialised as the daemon
    /// expects. Stored as the protocol enum so we don't need to
    /// reparse on submit.
    pub search_mode: mxr_core::types::SearchMode,
    pub active_field: SavedSearchFormField,
    /// Surfaced to the user when validation rejects a submit (empty
    /// name etc.). Cleared on the next successful interaction.
    pub validation_error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SavedSearchFormField {
    #[default]
    Name,
    Query,
    Mode,
}

impl Default for SavedSearchFormState {
    fn default() -> Self {
        Self {
            existing_name: None,
            name: String::new(),
            query: String::new(),
            search_mode: mxr_core::types::SearchMode::Lexical,
            active_field: SavedSearchFormField::Name,
            validation_error: None,
        }
    }
}

impl SavedSearchFormState {
    pub fn for_new() -> Self {
        Self::default()
    }

    pub fn for_edit(name: String, query: String, search_mode: mxr_core::types::SearchMode) -> Self {
        Self {
            existing_name: Some(name.clone()),
            name,
            query,
            search_mode,
            active_field: SavedSearchFormField::Name,
            validation_error: None,
        }
    }
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
