use super::super::MutationEffect;
use crate::ui::label_picker::{LabelPicker, LabelPickerMode};
use mxr_core::id::{AccountId, MessageId};
use mxr_protocol::Request;
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
    /// Modal for editing the active analytics view's filter
    /// parameters in one form. Populated when the user presses `f`
    /// inside the Analytics screen; cleared on Esc/Enter.
    pub analytics_filter: Option<AnalyticsFilterModalState>,
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
