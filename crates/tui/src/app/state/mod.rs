mod accounts;
mod analytics;
mod command_palette;
mod compose;
mod diagnostics;
mod mailbox;
mod modals;
mod rules;
mod search;

pub(in crate::app) use accounts::AccountFormToggleField;
pub use accounts::{AccountFormMode, AccountFormState, AccountsPageState, AccountsState};
pub use analytics::{
    AnalyticsCacheKey, AnalyticsState, AnalyticsView, ContactsMode, StorageMode, WrappedWindow,
    ANALYTICS_CACHE_TTL,
};
pub use command_palette::CommandPaletteState;
pub use compose::{ComposeAction, ComposeState, PendingSend, PendingSendMode};
pub use diagnostics::{DiagnosticsPageState, DiagnosticsPaneKind, DiagnosticsState};
pub(in crate::app) use mailbox::PendingPreviewRead;
pub(crate) use mailbox::SidebarSelectionKey;
pub use mailbox::{
    ActivePane, AttachmentOperation, AttachmentPanelState, AttachmentSummary, BodySource,
    BodyViewMetadata, BodyViewMode, BodyViewState, LayoutMode, MailListMode, MailListRow,
    MailboxState, MailboxView, OwedRepliesPageState, PendingAttachmentAction, PendingBrowserOpen,
    SidebarItem, SidebarSection, SubscriptionEntry, SubscriptionsPageState,
};
pub use modals::{
    AnalyticsFilterField, AnalyticsFilterModalState, ErrorModalState, FeatureOnboardingState,
    ModalsState, PendingBulkConfirm, PendingPlatformDispatch, PendingUnsubscribeAction,
    PendingUnsubscribeConfirm, PlatformModalState, ReplyQueueModalState, SavedSearchFormField,
    SavedSearchFormState, ScreenerModalState, SenderProfileModalState, SenderProfileTab,
    SnippetsModalState, SnoozePanelState, SnoozePreset, ThreadSummaryModalState, UserError,
    UserErrorSeverity, SNOOZE_PRESETS, USER_ERROR_LOG_CAPACITY, WARN_STATUS_TTL,
};
pub use rules::{RuleFormState, RulesPageState, RulesPanel, RulesState};
pub use search::{
    PendingSearchCountRequest, PendingSearchDebounce, PendingSearchRequest, SearchPageState,
    SearchPane, SearchState, SearchTarget, SearchUiStatus,
};
