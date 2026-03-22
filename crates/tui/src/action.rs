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
    OpenSearch,
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
    OpenLogs,
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

    // Help
    Help,

    // No-op (for unrecognized keys)
    Noop,
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
