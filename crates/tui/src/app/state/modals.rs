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
