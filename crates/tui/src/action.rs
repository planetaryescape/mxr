#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
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
    CheckLastSendOutcome,
    PreviewLastSendRecovery,
    ResumePreviewedSend,
    PreviewArchiveSelection,
    CheckArchiveOperation(String),
    SubmitArchiveOperation(String),
    ReopenLastSendDraft,
    OpenAccountsScreen,
    OpenAnalyticsScreen,
    OpenAnalyticsView(crate::app::AnalyticsView),
    NextAnalyticsView,
    PrevAnalyticsView,
    RefreshAnalytics,
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
    AnalyticsRowDrillDown,
    AnalyticsUnsubscribe,
    OpenAnalyticsFilterModal,
    CloseAnalyticsFilterModal,
    SubmitAnalyticsFilterModal,
    OpenTab1,
    OpenTab2,
    OpenTab3,
    OpenTab4,
    OpenTab5,
    OpenTab6,
    OpenTab7,
    OpenGlobalSearch,
    OpenMailboxFilter,
    SubmitSearch,
    CloseSearch,
    CycleSearchMode,
    NextSearchResult,
    PrevSearchResult,
    GoToInbox,
    GoToStarred,
    GoToSent,
    GoToDrafts,
    GoToAllMail,
    GoToTrash,
    GoToSpam,
    OpenSnoozed,
    Wake,
    OpenSubscriptions,
    OpenOwedReplies,
    OpenDesk,
    OpenNow,
    OpenMessages,
    MessagesOpen,
    MessagesAck,
    MessagesCancelAck,
    MessagesDone,
    MessagesPin,
    MessagesNewTopic,
    MessagesPrevTopic,
    MessagesNextTopic,
    MessagesPersonPage,
    MessagesAsSent,
    MessagesBack,
    MessagesFilter,
    MessagesCycleFilter,
    OpenArchiveMode,
    NowOpen,
    NowDone,
    NowOpenEmail,
    NowLetGoDigest,
    OpenUpdates,
    UpdatesOpen,
    UpdatesOpenEmail,
    UpdatesLetGoAll,
    UpdatesLetGoSource,
    UpdatesNeedsMe,
    UpdatesTune,
    UpdatesSweep,
    UpdatesTuneChoice(usize),
    UpdatesOpenLink,
    UpdatesCopyCode,
    UpdatesPeek,
    UpdatesClose,
    OpenFocus,
    FocusSkip,
    FocusDone,
    FocusReply,
    FocusRemind,
    FocusDraft,
    FocusLeave,
    OpenMoveMenu,
    OpenSenderMoveMenu,
    MoveEmailTo {
        mode: mxr_protocol::ModeKindData,
        sender: bool,
    },
    AnswerNewSender(mxr_protocol::SenderKindData),
    AnswerAlwaysForSender(bool),
    OpenArrivals,
    ArrivalsNextBucket,
    CloseArrivals,
    OpenPlace(mxr_protocol::MailPlaceData),
    TogglePin,
    OpenSenderKindMenu,
    SetSenderKind(Option<mxr_protocol::SenderKindData>),
    SweepBundle,
    SweepPlace,
    MorePlaceSenders,
    MoreFromSender,
    OpenCalendarInvites,
    OpenTodo,
    TodoPrimary,
    TodoDone,
    TodoSchedule,
    TodoEdit,
    TodoDismiss,
    TodoMightNeedYou,
    TodoOpenEmail,
    TodoPeek,
    TodoReviewResolution,
    TodoSendToMicrosoft,
    TodoSyncMicrosoft,
    TodoConfirmResolution,
    TodoDismissResolution,
    TodoUndoResolution,
    DismissHint,
    TodoOpenExpired,
    TodoOpenCatchup,
    TodoShowRunway,
    TodoRestore,
    CatchupKeep,
    CatchupLetGo,
    CatchupLetGoAll,
    CreateTodoFromMessage,
    RecordsAsk,
    RecordsCopyReference,
    RecordsCopyAmount,
    RecordsOpenDocument,
    RecordsOpenCard,
    RecordsOpenEmail,
    RecordsIssuerPage,
    RecordsPrevYear,
    RecordsNextYear,
    RecordsFix,
    RecordsMarkChecked,
    RecordsDismiss,
    RecordsExport,
    RecordsNextKind,
    RecordsMakeTodo,
    RecordsBack,
    PassToMode,
    OpenReading,
    ReadingRead,
    ReadingArticle,
    ReadingLater,
    ReadingLetGo,
    ReadingLetGoAll,
    ReadingUnsubscribe,
    ReadingUnsubscribeOnly,
    ReadingOriginal,
    ReadingHighlight,
    ReadingHighlights,
    ReadingOpenEmail,
    ReadingBack,
    ReadingEdition,
    ReadingLaterShelf,
    ReadingMarginalia,
    GoToLabel,
    OpenCommandPalette,
    CloseCommandPalette,
    ToggleAssistant,
    NewAssistantConversation,
    OpenAssistantConversations,
    OpenAssistantModels,
    ToggleAssistantTrust,
    DeleteAssistantConversation,
    SyncNow,
    OpenMessageView,
    CloseMessageView,
    ToggleMailListMode,
    SelectLabel(mxr_core::LabelId),
    SelectSavedSearch(String, mxr_core::SearchMode),
    OpenSavedSearchByIndex(usize),
    FlagReplyLater,
    CancelAutoReminder,
    OpenReplyQueue,
    CloseReplyQueueModal,
    ReplyQueueModalNext,
    ReplyQueueModalPrev,
    ReplyQueueModalReply,
    ReplyQueueModalFocus,
    OpenScreenerQueue,
    CloseScreenerModal,
    OpenActivityScreen,
    CloseActivityModal,
    ActivityModalNext,
    ActivityModalPrev,
    ActivityTogglePause,
    ScreenerModalNext,
    ScreenerModalPrev,
    ScreenerDisposeAllow,
    ScreenerDisposeDeny,
    ScreenerDisposeFeed,
    ScreenerDisposePaperTrail,
    OpenSenderView,
    CloseSenderViewModal,
    SenderProfileNextMessage,
    SenderProfilePrevMessage,
    OpenSenderProfileMessage,
    SummarizeCurrentThread,
    CloseSummaryModal,
    OpenThreadBriefing,
    OpenRecipientBriefing,
    CloseBriefingModal,
    OpenWhoisOnFocusedSender,
    CloseWhoisModal,
    FindExpertOnFocusedMessage,
    CloseExpertModal,
    OpenSnippets,
    OpenPersonalCategories,
    AssignPersonalCategory,
    CloseSnippetsModal,
    SnippetsModalNext,
    SnippetsModalPrev,
    OpenStoredDrafts,
    CloseStoredDraftsModal,
    StoredDraftsModalNext,
    StoredDraftsModalPrev,
    StoredDraftsModalEdit,
    StoredDraftsModalPreviewDelete,
    StoredDraftsModalPreviewCancelSchedule,
    StoredDraftsModalPreviewPush,
    StoredDraftsModalCancelConfirmation,
    StoredDraftsModalConfirm,
    ClearFilter,
    RefreshRules,
    ToggleRuleEnabled,
    DeleteRule,
    ShowRuleHistory,
    ShowRuleDryRun,
    ApplyRuleTreatment,
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
    DraftWithOptions,
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
    OpenDiagnosticsPaneDetails,
    PreviewRecoveryActivation,
    ConfirmRecoveryActivation,
    PreviewRecoveredDraft,
    ConfirmRecoveredDraft,
    NextRecoveredDraft,
    PreviousRecoveredDraft,

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
    UndoLastMutation,
    ApplyLabel,
    MoveToLabel,
    RouteToLabel,
    Unsubscribe,
    ConfirmUnsubscribeOnly,
    ConfirmUnsubscribeAndArchiveSender,
    CancelUnsubscribe,
    Snooze,
    RespondInvite(mxr_protocol::CalendarInviteActionData),
    RespondInviteWithComment(mxr_protocol::CalendarInviteActionData),
    OpenInBrowser,

    ToggleReaderMode,
    ToggleHtmlView,
    ToggleRemoteContent,
    ToggleSignature,
    ToggleQuotes,
    ToggleMessage,
    ToggleAllMessages,
    ArchiveNext,
    ArchivePrev,

    ToggleSelect,
    VisualLineMode,
    PatternSelect(PatternKind),

    AttachmentList,

    OpenLinks,

    ToggleFullscreen,

    ExportThread,

    SwitchAccount(String),

    Help,
    #[cfg(debug_assertions)]
    DumpActionTrace,

    Noop,

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
    Deliveries,
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
    Deliveries,
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
            Self::Deliveries => ScreenContext::Deliveries,
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
            Self::Deliveries => "Deliveries",
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
        Deliveries => true,
        Analytics => matches!(
            action,
            OpenCommandPalette
                | CloseCommandPalette
                | ToggleAssistant
                | NewAssistantConversation
                | OpenAssistantConversations
                | OpenAssistantModels
                | ToggleAssistantTrust
                | DeleteAssistantConversation
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
                | MessagesFilter
                | MessagesCycleFilter
                | OpenAnalyticsFilterModal
                | CloseAnalyticsFilterModal
                | SubmitAnalyticsFilterModal
                | OpenTab1
                | OpenTab2
                | OpenTab3
                | OpenTab4
                | OpenTab5
                | OpenTab6
                | OpenTab7
                | SyncNow
                | EditConfig
                | OpenLogs
                | OpenVoiceProfile
                | RebuildUserVoice
                | OpenCommitments
                | OpenPersonalCategories
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
                | ToggleAssistant
                | NewAssistantConversation
                | OpenAssistantConversations
                | OpenAssistantModels
                | ToggleAssistantTrust
                | DeleteAssistantConversation
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
                | OpenTab7
                | SyncNow
                | EditConfig
                | OpenLogs
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
                | OpenPersonalCategories
        ),
        SearchResults | SearchPreview => !matches!(
            action,
            RefreshRules
                | ToggleRuleEnabled
                | DeleteRule
                | ShowRuleHistory
                | ShowRuleDryRun
                | ApplyRuleTreatment
                | OpenRuleFormNew
                | OpenRuleFormEdit
                | SaveRuleForm
                | RefreshDiagnostics
                | GenerateBugReport
                | OpenDiagnosticsPaneDetails
                | PreviewRecoveryActivation
                | ConfirmRecoveryActivation
                | PreviewRecoveredDraft
                | ConfirmRecoveredDraft
                | NextRecoveredDraft
                | PreviousRecoveredDraft
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
                | ToggleAssistant
                | NewAssistantConversation
                | OpenAssistantConversations
                | OpenAssistantModels
                | ToggleAssistantTrust
                | DeleteAssistantConversation
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
                | ApplyRuleTreatment
                | OpenRuleFormNew
                | OpenRuleFormEdit
                | SaveRuleForm
                | SyncNow
                | EditConfig
                | OpenLogs
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
                | OpenPersonalCategories
        ),
        Diagnostics => matches!(
            action,
            OpenCommandPalette
                | CloseCommandPalette
                | ToggleAssistant
                | NewAssistantConversation
                | OpenAssistantConversations
                | OpenAssistantModels
                | ToggleAssistantTrust
                | DeleteAssistantConversation
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
                | OpenPersonalCategories
                | RefreshDiagnostics
                | GenerateBugReport
                | OpenDiagnosticsPaneDetails
                | PreviewRecoveryActivation
                | ConfirmRecoveryActivation
                | PreviewRecoveredDraft
                | ConfirmRecoveredDraft
                | NextRecoveredDraft
                | PreviousRecoveredDraft
                | SyncNow
                | EditConfig
                | OpenLogs
                | Help
                | QuitView
        ),
        AccountsList | AccountsForm => matches!(
            action,
            OpenCommandPalette
                | CloseCommandPalette
                | ToggleAssistant
                | NewAssistantConversation
                | OpenAssistantConversations
                | OpenAssistantModels
                | ToggleAssistantTrust
                | DeleteAssistantConversation
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
                | OpenPersonalCategories
                | SyncNow
                | EditConfig
                | OpenLogs
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
