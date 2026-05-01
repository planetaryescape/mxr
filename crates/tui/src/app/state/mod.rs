mod accounts;
mod command_palette;
mod compose;
mod diagnostics;
mod mailbox;
mod modals;
mod rules;
mod search;

pub(in crate::app) use accounts::AccountFormToggleField;
pub use accounts::{AccountFormMode, AccountFormState, AccountsPageState, AccountsState};
pub use command_palette::CommandPaletteState;
pub use compose::{ComposeAction, ComposeState, PendingSend, PendingSendMode};
pub use diagnostics::{DiagnosticsPageState, DiagnosticsPaneKind, DiagnosticsState};
pub(in crate::app) use mailbox::PendingPreviewRead;
pub(crate) use mailbox::SidebarSelectionKey;
pub use mailbox::{
    ActivePane, AttachmentOperation, AttachmentPanelState, AttachmentSummary, BodySource,
    BodyViewMetadata, BodyViewMode, BodyViewState, LayoutMode, MailListMode, MailListRow,
    MailboxState, MailboxView, PendingAttachmentAction, PendingBrowserOpen, SidebarItem,
    SidebarSection, SubscriptionEntry, SubscriptionsPageState,
};
pub use modals::{
    ErrorModalState, FeatureOnboardingState, ModalsState, PendingBulkConfirm,
    PendingUnsubscribeAction, PendingUnsubscribeConfirm, SnoozePanelState, SnoozePreset,
    SNOOZE_PRESETS,
};
pub use rules::{RuleFormState, RulesPageState, RulesPanel, RulesState};
pub use search::{
    PendingSearchCountRequest, PendingSearchDebounce, PendingSearchRequest, SearchPageState,
    SearchPane, SearchState, SearchTarget, SearchUiStatus,
};
