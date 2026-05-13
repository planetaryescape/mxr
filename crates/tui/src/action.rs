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
    OpenAnalyticsScreen,
    OpenAnalyticsView(crate::app::AnalyticsView),
    NextAnalyticsView,
    PrevAnalyticsView,
    RefreshAnalytics,
    // Analytics filter cycles (Slices 3, 4, 6, 7, 9)
    CycleStorageMode,
    CycleStorageGroupBy,
    ToggleStalePerspective,
    AdjustStaleOlderThanDays(i32),
    AdjustStaleWithinDays(i32),
    CycleContactsMode,
    RefreshContacts,
    ToggleResponseTimeDirection,
    ToggleSubscriptionsRank,
    CycleWrappedWindow,
    StepWrappedYear(i32),
    // Analytics drill-down + actionable rows (Slice 11, Slice 6)
    AnalyticsRowDrillDown,
    AnalyticsUnsubscribe,
    // Analytics filter modal (Slice 10)
    OpenAnalyticsFilterModal,
    CloseAnalyticsFilterModal,
    SubmitAnalyticsFilterModal,
    OpenTab1,
    OpenTab2,
    OpenTab3,
    OpenTab4,
    OpenTab5,
    OpenTab6,
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
    OpenOwedReplies,
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
    /// Jump to the Nth saved search (1-indexed) for keyboard-driven
    /// power users. `0` clears any active saved-search filter and
    /// returns to the default inbox view. Out-of-range indices are
    /// no-ops.
    OpenSavedSearchByIndex(usize),
    /// Mark the current message for reply-later. Local-only intent —
    /// never roundtrips to the provider. Cleared via the queue view or
    /// when the user replies.
    FlagReplyLater,
    /// Open the reply-later queue (a saved search for `is:reply-later`
    /// once the Tantivy operator lands; today opens via CLI).
    OpenReplyQueue,
    /// Close the reply-later queue modal (Esc).
    CloseReplyQueueModal,
    /// Move cursor to the next message in the reply queue.
    ReplyQueueModalNext,
    /// Move cursor to the previous message in the reply queue.
    ReplyQueueModalPrev,
    /// Start the normal reply compose flow for the selected queued message.
    ReplyQueueModalReply,
    /// Open the screener queue — senders waiting for a classification.
    OpenScreenerQueue,
    /// Close the screener triage modal (Esc).
    CloseScreenerModal,
    /// Move cursor to the next entry in the screener queue.
    ScreenerModalNext,
    /// Move cursor to the previous entry in the screener queue.
    ScreenerModalPrev,
    /// Set the focused sender's disposition to allow.
    ScreenerDisposeAllow,
    /// Set the focused sender's disposition to deny.
    ScreenerDisposeDeny,
    /// Set the focused sender's disposition to feed.
    ScreenerDisposeFeed,
    /// Set the focused sender's disposition to paper-trail.
    ScreenerDisposePaperTrail,
    /// Show the sender-view full-screen page for the currently focused
    /// message's `from` address.
    OpenSenderView,
    /// Close the sender-view modal (Esc).
    CloseSenderViewModal,
    /// Move cursor to the next recent sender message.
    SenderProfileNextMessage,
    /// Move cursor to the previous recent sender message.
    SenderProfilePrevMessage,
    /// Open the selected recent sender message.
    OpenSenderProfileMessage,
    /// Summarize the current thread via the configured LLM.
    SummarizeCurrentThread,
    /// Close the thread-summary modal (Esc).
    CloseSummaryModal,
    /// Slice 5.1 (C2.6): open the briefing modal for the focused
    /// thread. Fires `Request::GetThreadBriefing`.
    OpenThreadBriefing,
    /// Slice 5.2 (C2.6): open the briefing modal for the focused
    /// recipient. Fires `Request::GetRecipientBriefing`.
    OpenRecipientBriefing,
    /// Close the briefing modal (Esc).
    CloseBriefingModal,
    /// Open the snippet manager modal.
    OpenSnippets,
    /// Close the snippet manager modal (Esc).
    CloseSnippetsModal,
    /// Move cursor to the next snippet in the modal list.
    SnippetsModalNext,
    /// Move cursor to the previous snippet in the modal list.
    SnippetsModalPrev,
    ClearFilter,
    RefreshRules,
    ToggleRuleEnabled,
    DeleteRule,
    ShowRuleHistory,
    ShowRuleDryRun,
    OpenRuleFormNew,
    OpenRuleFormEdit,
    SaveRuleForm,
    OpenSavedSearchFormNew,
    OpenSavedSearchFormEdit,
    SaveSavedSearchForm,
    DeleteSavedSearch,
    EnableSemantic,
    DisableSemantic,
    ReindexSemantic,
    BackfillSemantic,
    InstallSemanticProfile(mxr_core::types::SemanticProfile),
    UseSemanticProfile(mxr_core::types::SemanticProfile),
    DraftAssistCurrentThread,
    DraftNewForSender,
    RefinePendingDraft,
    OpenVoiceProfile,
    RebuildUserVoice,
    OpenCommitments,
    RefreshDiagnostics,
    RefreshAccounts,
    OpenAccountFormNew,
    SaveAccountForm,
    TestAccountForm,
    ReauthorizeAccountForm,
    RepairAccount,
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
    /// Reverse the most recent destructive mutation if its undo window
    /// is still open. Bound to `u` in the mailbox; falls through to a
    /// status-bar warning when there's nothing to undo.
    UndoLastMutation,
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

    /// Cancel an in-flight Outlook device-code auth session.
    CancelOutlookAuth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScreenContext {
    Mailbox,
    Search,
    Rules,
    Diagnostics,
    Accounts,
    Analytics,
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
    Analytics,
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
            Self::Analytics => ScreenContext::Analytics,
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
            Self::Analytics => "Analytics",
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
        Analytics => matches!(
            action,
            OpenCommandPalette
                | CloseCommandPalette
                | OpenMailboxScreen
                | OpenSearchScreen
                | OpenRulesScreen
                | OpenDiagnosticsScreen
                | OpenAccountsScreen
                | OpenAnalyticsScreen
                | OpenAnalyticsView(_)
                | NextAnalyticsView
                | PrevAnalyticsView
                | RefreshAnalytics
                | CycleStorageMode
                | CycleStorageGroupBy
                | ToggleStalePerspective
                | AdjustStaleOlderThanDays(_)
                | AdjustStaleWithinDays(_)
                | CycleContactsMode
                | RefreshContacts
                | ToggleResponseTimeDirection
                | ToggleSubscriptionsRank
                | CycleWrappedWindow
                | StepWrappedYear(_)
                | AnalyticsRowDrillDown
                | AnalyticsUnsubscribe
                | OpenAnalyticsFilterModal
                | CloseAnalyticsFilterModal
                | SubmitAnalyticsFilterModal
                | OpenTab1
                | OpenTab2
                | OpenTab3
                | OpenTab4
                | OpenTab5
                | OpenTab6
                | SyncNow
                | EditConfig
                | OpenLogs
                | ShowOnboarding
                | OpenVoiceProfile
                | RebuildUserVoice
                | OpenCommitments
                | Help
                | QuitView
                | MoveDown
                | MoveUp
                | JumpTop
                | JumpBottom
        ),
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
                | OpenAnalyticsScreen
                | OpenTab1
                | OpenTab2
                | OpenTab3
                | OpenTab4
                | OpenTab5
                | OpenTab6
                | SyncNow
                | EditConfig
                | OpenLogs
                | ShowOnboarding
                | Help
                | QuitView
                | EnableSemantic
                | DisableSemantic
                | ReindexSemantic
                | BackfillSemantic
                | InstallSemanticProfile(_)
                | UseSemanticProfile(_)
                | OpenVoiceProfile
                | RebuildUserVoice
                | OpenCommitments
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
                | RepairAccount
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
                | OpenAnalyticsScreen
                | OpenTab1
                | OpenTab2
                | OpenTab3
                | OpenTab4
                | OpenTab5
                | OpenTab6
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
                | EnableSemantic
                | DisableSemantic
                | ReindexSemantic
                | BackfillSemantic
                | InstallSemanticProfile(_)
                | UseSemanticProfile(_)
                | OpenVoiceProfile
                | RebuildUserVoice
                | OpenCommitments
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
                | OpenAnalyticsScreen
                | OpenTab1
                | OpenTab2
                | OpenTab3
                | OpenTab4
                | OpenTab5
                | OpenTab6
                | EnableSemantic
                | DisableSemantic
                | ReindexSemantic
                | BackfillSemantic
                | InstallSemanticProfile(_)
                | UseSemanticProfile(_)
                | OpenVoiceProfile
                | RebuildUserVoice
                | OpenCommitments
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
                | OpenAnalyticsScreen
                | OpenTab1
                | OpenTab2
                | OpenTab3
                | OpenTab4
                | OpenTab5
                | OpenTab6
                | RefreshAccounts
                | OpenAccountFormNew
                | SaveAccountForm
                | TestAccountForm
                | ReauthorizeAccountForm
                | RepairAccount
                | SetDefaultAccount
                | EnableSemantic
                | DisableSemantic
                | ReindexSemantic
                | BackfillSemantic
                | InstallSemanticProfile(_)
                | UseSemanticProfile(_)
                | OpenVoiceProfile
                | RebuildUserVoice
                | OpenCommitments
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
