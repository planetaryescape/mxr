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
    OpenTab7,
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
    OpenDesk,
    /// Open Now (`g h`): the front page, at most ten things in four fixed
    /// sections (`Request::GetNow`).
    OpenNow,
    /// Open Messages (`g m`): people you talk with, one row each, with the
    /// selected person's page beside them (`Request::ListMessages`).
    OpenMessages,
    /// Enter in Messages: open the person's page, or the Quiet band.
    MessagesOpen,
    /// `.` in Messages: Got it, previewed with a countdown, then sent.
    MessagesAck,
    /// `u` or Esc while Got it counts down: don't send it.
    MessagesCancelAck,
    /// `e` in Messages: done here for the selected topic (`SetModeDone`).
    MessagesDone,
    /// `s` in Messages: pin or unpin the person.
    MessagesPin,
    /// `c` in Messages: a new topic with the person.
    MessagesNewTopic,
    /// `[` in Messages: the previous topic with this person.
    MessagesPrevTopic,
    /// `]` in Messages: the next topic with this person.
    MessagesNextTopic,
    /// `p` in Messages: focus the person page.
    MessagesPersonPage,
    /// `o` or `v` in Messages: the selected topic's message as sent.
    MessagesAsSent,
    /// Esc on Messages' first-encounter card, or back to the list.
    MessagesBack,
    /// Open Archive (`g e`): an early version that says what backs it.
    OpenArchiveMode,
    /// Enter on Now: open the row in its own mode.
    NowOpen,
    /// `e` on Now: done here, in the row's own mode (`SetModeDone`).
    NowDone,
    /// `o` on Now: open the email itself.
    NowOpenEmail,
    /// `A` on Now: preview letting go of the Updates card, or confirm it.
    NowLetGoDigest,
    /// A digit on a new sender's row: answer its one question with that
    /// choice (`SetSenderKind`).
    NowAnswerSender(usize),
    /// Open Reading or Paper trail (`Request::ListPlace`).
    OpenPlace(mxr_protocol::MailPlaceData),
    /// Pin or unpin the message under the cursor in a place.
    TogglePin,
    /// Open the "move sender to…" menu for the sender under the cursor.
    OpenSenderKindMenu,
    /// Move that sender to a kind (`None`: back to automatic).
    SetSenderKind(Option<mxr_protocol::SenderKindData>),
    /// Preview sweeping the bundle under the cursor.
    SweepBundle,
    /// Preview sweeping the whole place on screen.
    SweepPlace,
    /// Load the next page of senders in a place.
    MorePlaceSenders,
    /// Load more of the sender under the cursor.
    MoreFromSender,
    /// Open the calendar-invites lens (sidebar item). Loads invites via
    /// `Request::ListInvites` into the dedicated lens view.
    OpenCalendarInvites,
    /// Open To do (`g x`): things email asked you to do, as a runway
    /// ordered by when to act (`Request::GetTodoRunway`).
    OpenTodo,
    /// Enter on a to-do: open its link when the pay-link gate passed,
    /// otherwise its email.
    TodoPrimary,
    /// `e`: tick the to-do off, with `u` to undo.
    TodoDone,
    /// `Z`: show it on a date you type, previewed as you type.
    TodoSchedule,
    /// `,`: correct a field as `field=value`.
    TodoEdit,
    /// `X`: not a to-do; kept as a correction.
    TodoDismiss,
    /// `o`: open the email the to-do came from.
    TodoOpenEmail,
    /// Esc while a hint shows in the status line: dismiss it in every
    /// client (`SetHintSeen`).
    DismissHint,
    /// The Expired list, one key from restore.
    TodoOpenExpired,
    /// The first run's one-time catch-up.
    TodoOpenCatchup,
    /// Back from the Expired list or the catch-up to the runway.
    TodoShowRunway,
    /// Put an expired row back on the runway.
    TodoRestore,
    /// Keep the catch-up row under the cursor.
    CatchupKeep,
    /// Let go of the catch-up row under the cursor.
    CatchupLetGo,
    /// Preview letting go of the whole catch-up, or confirm the preview.
    CatchupLetGoAll,
    /// `t` on a conversation: make a to-do from it.
    CreateTodoFromMessage,
    /// Archive `/`: type into the answer box.
    RecordsAsk,
    /// Archive `y`: copy the record's reference.
    RecordsCopyReference,
    /// Archive `Y`: copy the record's amount.
    RecordsCopyAmount,
    /// Archive Enter: open the record's PDF, else its card.
    RecordsOpenDocument,
    /// Archive Right: the record's whole card, every field and source.
    RecordsOpenCard,
    /// Archive `o` and `e`: open the email the record came from.
    RecordsOpenEmail,
    /// Archive `p`: every record from this issuer.
    RecordsIssuerPage,
    /// Archive `[`: the year before.
    RecordsPrevYear,
    /// Archive `]`: the year after.
    RecordsNextYear,
    /// Archive `,`: fix a field as `field=value`, previewed first.
    RecordsFix,
    /// Archive `v`: confirm every unchecked amount and date.
    RecordsMarkChecked,
    /// Archive `X`: not a record. The email is untouched.
    RecordsDismiss,
    /// Archive `E`: preview the CSV export, then write it.
    RecordsExport,
    /// Archive `g f`: the next kind chip.
    RecordsNextKind,
    /// Archive `t`: make a to-do from the record's email.
    RecordsMakeTodo,
    /// Archive Esc: close the card, the answer or the issuer page.
    RecordsBack,
    /// `T` on a conversation: pass it to a mode (Archive, this phase).
    PassToMode,
    /// Open Reading (`g r`): the edition, with a reader beside it
    /// (`Request::GetReadingEdition`).
    OpenReading,
    /// Enter: read the item under the cursor in the reader pane.
    ReadingRead,
    /// `L`: fetch the item's linked article, naming its site first.
    ReadingArticle,
    /// `b`: put the item on Later (a link's article is saved too).
    ReadingLater,
    /// `e`: let go of the item's issue in Reading.
    ReadingLetGo,
    /// `A`: preview letting go of everything shown, or confirm it.
    ReadingLetGoAll,
    /// `D`: preview unsubscribing from the item's source, or confirm it.
    ReadingUnsubscribe,
    /// `R`: the source's own text instead of the reader, remembered.
    ReadingOriginal,
    /// `h`: save the paragraph at the top of the reader as a highlight.
    ReadingHighlight,
    /// `o`: open the email itself.
    ReadingOpenEmail,
    /// Esc in the reader: back to the edition.
    ReadingBack,
    /// `B`: the Later shelf, or back to the edition from it.
    ReadingLaterShelf,
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
    /// Reply later: open the time prompt, or confirm it when open. A time
    /// takes the conversation away until then (`DeferThreads`); no time is
    /// the untimed local flag. Cleared via the queue view or when the user
    /// replies.
    FlagReplyLater,
    /// Cancel a pending follow-up reminder for the current sent message.
    CancelAutoReminder,
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
    /// Focus & reply: reply to each queued message in turn, from the
    /// selected one; the next reply opens after each send.
    ReplyQueueModalFocus,
    /// Open the screener queue — senders waiting for a classification.
    OpenScreenerQueue,
    /// Close the screener triage modal (Esc).
    CloseScreenerModal,
    /// Open the local activity log modal (Phase 5).
    /// Chord: `g y` (g-prefix for "go to"; `g a` is GoToAllMail) ·
    /// palette: View activity.
    OpenActivityScreen,
    /// Close the activity log modal (Esc).
    CloseActivityModal,
    /// Move cursor to the next entry in the activity modal.
    ActivityModalNext,
    /// Move cursor to the previous entry in the activity modal.
    ActivityModalPrev,
    /// Toggle paused state from the activity modal.
    ActivityTogglePause,
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
    /// Slice 6.1 (C2.9): open the whois modal for the focused
    /// sender's email. Fires `Request::ExplainEntity`.
    OpenWhoisOnFocusedSender,
    /// Close the whois modal (Esc).
    CloseWhoisModal,
    /// Slice 5.4 (C2.8 cont): "Find expert" command-palette action.
    /// Uses the focused message body as the query and fires
    /// `Request::FindExpert`.
    FindExpertOnFocusedMessage,
    /// Close the expert-finder modal (Esc).
    CloseExpertModal,
    /// Open the snippet manager modal.
    OpenSnippets,
    /// Close the snippet manager modal (Esc).
    CloseSnippetsModal,
    /// Move cursor to the next snippet in the modal list.
    SnippetsModalNext,
    /// Move cursor to the previous snippet in the modal list.
    SnippetsModalPrev,
    /// Open the stored-drafts browser modal (locally-saved drafts,
    /// across all accounts, edit-in-place via `$EDITOR`).
    OpenStoredDrafts,
    /// Close the stored-drafts modal (Esc).
    CloseStoredDraftsModal,
    /// Move cursor to the next draft in the modal list.
    StoredDraftsModalNext,
    /// Move cursor to the previous draft in the modal list.
    StoredDraftsModalPrev,
    /// Edit the selected stored draft in place via `$EDITOR`. The result
    /// round-trips through `Request::UpdateDraft`, preserving the
    /// draft's id instead of minting a new one.
    StoredDraftsModalEdit,
    /// Preview permanent deletion of the selected local draft.
    StoredDraftsModalPreviewDelete,
    StoredDraftsModalPreviewCancelSchedule,
    /// Preview a one-way copy of the selected local draft to its provider.
    StoredDraftsModalPreviewPush,
    /// Cancel the active stored-draft mutation preview.
    StoredDraftsModalCancelConfirmation,
    /// Commit the exact stored-draft operation shown in the preview.
    StoredDraftsModalConfirm,
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
    RouteToLabel,
    Unsubscribe,
    ConfirmUnsubscribeOnly,
    ConfirmUnsubscribeAndArchiveSender,
    CancelUnsubscribe,
    Snooze,
    RespondInvite(mxr_protocol::CalendarInviteActionData),
    /// Respond to a calendar invite with a free-text comment. Opens compose
    /// pre-seeded with the daemon-built REPLY ICS so the user can type a note
    /// before sending. Fires from the chord `i` + Shift-A/M/D.
    RespondInviteWithComment(mxr_protocol::CalendarInviteActionData),
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
        // The Deliveries screen handles its row keys directly; global
        // navigation/tab actions are all permitted.
        Deliveries => true,
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
                | OpenTab7
                | SyncNow
                | EditConfig
                | OpenLogs
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
