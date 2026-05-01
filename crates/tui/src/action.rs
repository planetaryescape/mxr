#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    // Navigation (vim-native)
    MoveDown,
    MoveUp,
    JumpTop,
    JumpBottom,
    PageDown,
    PageUp,
    ViewportTop,
    ViewportMiddle,
    ViewportBottom,
    CenterCurrent,
    SwitchPane,
    OpenSelected,
    Back,
    QuitView,
    ClearSelection,
    OpenMailboxScreen,
    OpenSearchScreen,
    OpenRulesScreen,
    OpenDiagnosticsScreen,
    OpenAccountsScreen,
    OpenTab1,
    OpenTab2,
    OpenTab3,
    OpenTab4,
    OpenTab5,
    // Search
    OpenGlobalSearch,
    OpenMailboxFilter,
    SubmitSearch,
    CloseSearch,
    CycleSearchMode,
    NextSearchResult,
    PrevSearchResult,
    // Gmail go-to navigation (A005)
    GoToInbox,
    GoToStarred,
    GoToSent,
    GoToDrafts,
    GoToAllMail,
    OpenSubscriptions,
    GoToLabel,
    // Command palette
    OpenCommandPalette,
    CloseCommandPalette,
    // Sync
    SyncNow,
    // Message view
    OpenMessageView,
    CloseMessageView,
    ToggleMailListMode,
    // Label / saved search selection
    SelectLabel(mxr_core::LabelId),
    SelectSavedSearch(String, mxr_core::SearchMode),
    ClearFilter,
    RefreshRules,
    ToggleRuleEnabled,
    DeleteRule,
    ShowRuleHistory,
    ShowRuleDryRun,
    OpenRuleFormNew,
    OpenRuleFormEdit,
    SaveRuleForm,
    RefreshDiagnostics,
    RefreshAccounts,
    OpenAccountFormNew,
    SaveAccountForm,
    TestAccountForm,
    ReauthorizeAccountForm,
    SetDefaultAccount,
    GenerateBugReport,
    EditConfig,
    OpenLogs,
    ShowOnboarding,
    OpenDiagnosticsPaneDetails,

    // --- Phase 2: Email actions (Gmail-native A005) ---
    Compose,
    Reply,
    ReplyAll,
    Forward,
    Archive,
    MarkReadAndArchive,
    Trash,
    Spam,
    Star,
    MarkRead,
    MarkUnread,
    ApplyLabel,
    MoveToLabel,
    Unsubscribe,
    ConfirmUnsubscribeOnly,
    ConfirmUnsubscribeAndArchiveSender,
    CancelUnsubscribe,
    Snooze,
    OpenInBrowser,

    // --- Phase 2: Reader mode ---
    ToggleReaderMode,
    ToggleHtmlView,
    ToggleRemoteContent,
    ToggleSignature,

    // --- Phase 2: Batch operations (A007) ---
    ToggleSelect,
    VisualLineMode,
    PatternSelect(PatternKind),

    // --- Phase 2: Attachments ---
    AttachmentList,

    // --- Phase 2: Links ---
    OpenLinks,

    // --- Phase 2: Layout ---
    ToggleFullscreen,

    // --- Phase 2: Export ---
    ExportThread,

    // --- Account switching ---
    SwitchAccount(String),

    // Help
    Help,
    // Debug-only diagnostics
    #[cfg(debug_assertions)]
    DumpActionTrace,

    // No-op (for unrecognized keys)
    Noop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScreenContext {
    Mailbox,
    Search,
    Rules,
    Diagnostics,
    Accounts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UiContext {
    MailboxSidebar,
    MailboxList,
    MailboxMessage,
    SearchEditor,
    SearchResults,
    SearchPreview,
    RulesList,
    RulesForm,
    Diagnostics,
    AccountsList,
    AccountsForm,
}

impl UiContext {
    pub const fn screen(self) -> ScreenContext {
        match self {
            Self::MailboxSidebar | Self::MailboxList | Self::MailboxMessage => {
                ScreenContext::Mailbox
            }
            Self::SearchEditor | Self::SearchResults | Self::SearchPreview => ScreenContext::Search,
            Self::RulesList | Self::RulesForm => ScreenContext::Rules,
            Self::Diagnostics => ScreenContext::Diagnostics,
            Self::AccountsList | Self::AccountsForm => ScreenContext::Accounts,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::MailboxSidebar => "Mailbox / Sidebar",
            Self::MailboxList => "Mailbox / List",
            Self::MailboxMessage => "Mailbox / Message",
            Self::SearchEditor => "Search / Query",
            Self::SearchResults => "Search / Results",
            Self::SearchPreview => "Search / Preview",
            Self::RulesList => "Rules",
            Self::RulesForm => "Rules / Form",
            Self::Diagnostics => "Diagnostics",
            Self::AccountsList => "Accounts",
            Self::AccountsForm => "Accounts / Form",
        }
    }
}

pub fn action_allowed_in_context(action: &Action, context: UiContext) -> bool {
    use Action::*;
    use UiContext::*;

    #[cfg(debug_assertions)]
    if matches!(action, DumpActionTrace) {
        return true;
    }

    match context {
        MailboxSidebar | MailboxList | MailboxMessage => true,
        SearchEditor => matches!(
            action,
            OpenGlobalSearch
                | SubmitSearch
                | CloseSearch
                | CycleSearchMode
                | OpenCommandPalette
                | CloseCommandPalette
                | OpenMailboxScreen
                | OpenSearchScreen
                | OpenRulesScreen
                | OpenDiagnosticsScreen
                | OpenAccountsScreen
                | OpenTab1
                | OpenTab2
                | OpenTab3
                | OpenTab4
                | OpenTab5
                | SyncNow
                | EditConfig
                | OpenLogs
                | ShowOnboarding
                | Help
                | QuitView
        ),
        SearchResults | SearchPreview => !matches!(
            action,
            RefreshRules
                | ToggleRuleEnabled
                | DeleteRule
                | ShowRuleHistory
                | ShowRuleDryRun
                | OpenRuleFormNew
                | OpenRuleFormEdit
                | SaveRuleForm
                | RefreshDiagnostics
                | GenerateBugReport
                | OpenDiagnosticsPaneDetails
                | RefreshAccounts
                | OpenAccountFormNew
                | SaveAccountForm
                | TestAccountForm
                | ReauthorizeAccountForm
                | SetDefaultAccount
        ),
        RulesList | RulesForm => matches!(
            action,
            OpenCommandPalette
                | CloseCommandPalette
                | OpenMailboxScreen
                | OpenSearchScreen
                | OpenRulesScreen
                | OpenDiagnosticsScreen
                | OpenAccountsScreen
                | OpenTab1
                | OpenTab2
                | OpenTab3
                | OpenTab4
                | OpenTab5
                | RefreshRules
                | ToggleRuleEnabled
                | DeleteRule
                | ShowRuleHistory
                | ShowRuleDryRun
                | OpenRuleFormNew
                | OpenRuleFormEdit
                | SaveRuleForm
                | SyncNow
                | EditConfig
                | OpenLogs
                | ShowOnboarding
                | Help
                | QuitView
        ),
        Diagnostics => matches!(
            action,
            OpenCommandPalette
                | CloseCommandPalette
                | OpenMailboxScreen
                | OpenSearchScreen
                | OpenRulesScreen
                | OpenDiagnosticsScreen
                | OpenAccountsScreen
                | OpenTab1
                | OpenTab2
                | OpenTab3
                | OpenTab4
                | OpenTab5
                | RefreshDiagnostics
                | GenerateBugReport
                | OpenDiagnosticsPaneDetails
                | SyncNow
                | EditConfig
                | OpenLogs
                | ShowOnboarding
                | Help
                | QuitView
        ),
        AccountsList | AccountsForm => matches!(
            action,
            OpenCommandPalette
                | CloseCommandPalette
                | OpenMailboxScreen
                | OpenSearchScreen
                | OpenRulesScreen
                | OpenDiagnosticsScreen
                | OpenAccountsScreen
                | OpenTab1
                | OpenTab2
                | OpenTab3
                | OpenTab4
                | OpenTab5
                | RefreshAccounts
                | OpenAccountFormNew
                | SaveAccountForm
                | TestAccountForm
                | ReauthorizeAccountForm
                | SetDefaultAccount
                | SyncNow
                | EditConfig
                | OpenLogs
                | ShowOnboarding
                | Help
                | QuitView
        ),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatternKind {
    All,
    None,
    Read,
    Unread,
    Starred,
    Thread,
}
