use super::*;

impl App {
    pub(super) fn browser_document(body: &MessageBody) -> Option<String> {
        body.text_html
            .clone()
            .or_else(|| {
                body.text_plain
                    .as_deref()
                    .map(render_plain_text_browser_document)
            })
            .or_else(|| {
                body.best_effort_readable_summary()
                    .map(|text| render_plain_text_browser_document(&text))
            })
    }

    pub(super) fn queue_browser_open_for_body(
        &mut self,
        message_id: MessageId,
        body: &MessageBody,
    ) {
        let Some(document) = Self::browser_document(body) else {
            self.status_message = Some("No readable body available".into());
            return;
        };

        self.mailbox.pending_browser_open = Some(PendingBrowserOpen {
            message_id,
            document,
        });
        self.status_message = Some("Opening in browser...".into());
    }

    pub(super) fn queue_current_message_browser_open(&mut self) {
        let Some(message_id) = self
            .mailbox
            .viewing_envelope
            .as_ref()
            .map(|env| env.id.clone())
        else {
            self.status_message = Some("No message selected".into());
            return;
        };

        let Some(body) = self.current_viewing_body() else {
            self.queue_body_fetch(message_id.clone());
            self.mailbox.pending_browser_open_after_load = Some(message_id);
            self.status_message = Some("Loading message body...".into());
            return;
        };

        let body = body.clone();
        self.queue_browser_open_for_body(message_id, &body);
    }

    pub fn tick(&mut self) {
        self.input.check_timeout();
        self.toasts.sweep_expired(std::time::Instant::now());
        if self.search_is_pending() {
            self.search.page.throbber.calc_next();
        }
        if self.mailbox.mailbox_loading_message.is_some() {
            self.mailbox.mailbox_loading_throbber.calc_next();
        }
        if self.accounts.page.operation_in_flight {
            self.accounts.page.throbber.calc_next();
        }
        if self.analytics.should_show_cold_load() {
            self.analytics.loading_throbber.calc_next();
        }
        if self.has_in_flight_work() {
            self.status_throbber.calc_next();
        }
        if self
            .diagnostics
            .page
            .sync_statuses
            .iter()
            .any(|status| status.sync_in_progress)
        {
            self.mailbox.sidebar_sync_throbber.calc_next();
        }
        self.process_pending_search_debounce();
        self.process_pending_preview_read();
    }

    pub fn apply(&mut self, action: Action) {
        self.recorder.record(&action, &self.screen);
        // Clear status message on any action
        self.status_message = None;
        // Before the action moves the cursor or the page.
        self.dismiss_hint_acted_on(&action);

        match action {
            Action::DismissHint => self.dismiss_active_hint(),
            #[cfg(debug_assertions)]
            Action::DumpActionTrace => self.dump_action_trace(),
            Action::RefreshAccounts
            | Action::OpenAccountFormNew
            | Action::SaveAccountForm
            | Action::TestAccountForm
            | Action::ReauthorizeAccountForm
            | Action::RepairAccount
            | Action::SetDefaultAccount
            | Action::SwitchAccount(_) => self.apply_account_action(action),
            Action::OpenPlace(_)
            | Action::TogglePin
            | Action::OpenSenderKindMenu
            | Action::SetSenderKind(_)
            | Action::SweepBundle
            | Action::SweepPlace
            | Action::MorePlaceSenders
            | Action::MoreFromSender => self.apply_place_action(action),
            Action::OpenTodo
            | Action::TodoPrimary
            | Action::TodoDone
            | Action::TodoSchedule
            | Action::TodoEdit
            | Action::TodoDismiss
            | Action::TodoOpenEmail
            | Action::TodoOpenExpired
            | Action::TodoOpenCatchup
            | Action::TodoShowRunway
            | Action::TodoRestore
            | Action::CatchupKeep
            | Action::CatchupLetGo
            | Action::CatchupLetGoAll
            | Action::CreateTodoFromMessage => self.apply_todo_action(action),
            Action::RecordsAsk
            | Action::RecordsCopyReference
            | Action::RecordsCopyAmount
            | Action::RecordsOpenDocument
            | Action::RecordsOpenCard
            | Action::RecordsOpenEmail
            | Action::RecordsIssuerPage
            | Action::RecordsPrevYear
            | Action::RecordsNextYear
            | Action::RecordsFix
            | Action::RecordsMarkChecked
            | Action::RecordsDismiss
            | Action::RecordsExport
            | Action::RecordsNextKind
            | Action::RecordsMakeTodo
            | Action::RecordsBack
            | Action::PassToMode => self.apply_records_action(action),
            Action::OpenReading
            | Action::ReadingRead
            | Action::ReadingArticle
            | Action::ReadingLater
            | Action::ReadingLetGo
            | Action::ReadingLetGoAll
            | Action::ReadingUnsubscribe
            | Action::ReadingOriginal
            | Action::ReadingHighlight
            | Action::ReadingOpenEmail
            | Action::ReadingBack
            | Action::ReadingLaterShelf
            | Action::ReadingCloseCard => self.apply_reading_action(action),
            Action::OpenNow
            | Action::OpenMessages
            | Action::OpenArchiveMode
            | Action::NowOpen
            | Action::NowDone
            | Action::NowOpenEmail
            | Action::NowLetGoDigest
            | Action::NowAnswerSender(_) => self.apply_now_action(action),
            Action::MessagesOpen
            | Action::MessagesAck
            | Action::MessagesCancelAck
            | Action::MessagesDone
            | Action::MessagesPin
            | Action::MessagesNewTopic
            | Action::MessagesPrevTopic
            | Action::MessagesNextTopic
            | Action::MessagesPersonPage
            | Action::MessagesAsSent
            | Action::MessagesBack => self.apply_messages_action(action),
            Action::OpenMailboxScreen
            | Action::OpenSearchScreen
            | Action::OpenGlobalSearch
            | Action::OpenRulesScreen
            | Action::OpenDiagnosticsScreen
            | Action::OpenAccountsScreen
            | Action::OpenTab1
            | Action::OpenTab2
            | Action::OpenTab3
            | Action::OpenTab4
            | Action::OpenTab5
            | Action::OpenTab6
            | Action::OpenTab7
            | Action::SyncNow
            | Action::Noop
            | Action::CancelOutlookAuth => self.apply_screen_action(action),
            Action::OpenAnalyticsScreen
            | Action::OpenAnalyticsView(_)
            | Action::NextAnalyticsView
            | Action::PrevAnalyticsView
            | Action::RefreshAnalytics
            | Action::CycleStorageMode
            | Action::CycleStorageGroupBy
            | Action::ToggleStalePerspective
            | Action::AdjustStaleOlderThanDays(_)
            | Action::AdjustStaleWithinDays(_)
            | Action::CycleContactsMode
            | Action::RefreshContacts
            | Action::ToggleResponseTimeDirection
            | Action::ToggleSubscriptionsRank
            | Action::CycleWrappedWindow
            | Action::StepWrappedYear(_)
            | Action::AnalyticsRowDrillDown
            | Action::AnalyticsUnsubscribe
            | Action::OpenAnalyticsFilterModal
            | Action::CloseAnalyticsFilterModal
            | Action::SubmitAnalyticsFilterModal => self.apply_analytics_action(action),
            Action::MoveDown
            | Action::MoveUp
            | Action::JumpTop
            | Action::JumpBottom
            | Action::PageDown
            | Action::PageUp
            | Action::ViewportTop
            | Action::ViewportMiddle
            | Action::ViewportBottom
            | Action::CenterCurrent
            | Action::SwitchPane
            | Action::OpenSelected
            | Action::Back
            | Action::QuitView
            | Action::ClearSelection
            | Action::GoToInbox
            | Action::GoToStarred
            | Action::GoToSent
            | Action::GoToDrafts
            | Action::GoToAllMail
            | Action::OpenSubscriptions
            | Action::OpenOwedReplies
            | Action::OpenDesk
            | Action::OpenCalendarInvites
            | Action::GoToLabel
            | Action::SelectLabel(_)
            | Action::SelectSavedSearch(_, _)
            | Action::OpenSavedSearchByIndex(_)
            | Action::FlagReplyLater
            | Action::CancelAutoReminder
            | Action::OpenReplyQueue
            | Action::CloseReplyQueueModal
            | Action::ReplyQueueModalNext
            | Action::ReplyQueueModalPrev
            | Action::ReplyQueueModalReply
            | Action::ReplyQueueModalFocus
            | Action::OpenActivityScreen
            | Action::CloseActivityModal
            | Action::ActivityModalNext
            | Action::ActivityModalPrev
            | Action::ActivityTogglePause
            | Action::OpenScreenerQueue
            | Action::CloseScreenerModal
            | Action::ScreenerModalNext
            | Action::ScreenerModalPrev
            | Action::ScreenerDisposeAllow
            | Action::ScreenerDisposeDeny
            | Action::ScreenerDisposeFeed
            | Action::ScreenerDisposePaperTrail
            | Action::OpenSenderView
            | Action::CloseSenderViewModal
            | Action::SenderProfileNextMessage
            | Action::SenderProfilePrevMessage
            | Action::OpenSenderProfileMessage
            | Action::SummarizeCurrentThread
            | Action::CloseSummaryModal
            | Action::OpenThreadBriefing
            | Action::OpenRecipientBriefing
            | Action::CloseBriefingModal
            | Action::OpenWhoisOnFocusedSender
            | Action::CloseWhoisModal
            | Action::FindExpertOnFocusedMessage
            | Action::CloseExpertModal
            | Action::OpenSnippets
            | Action::CloseSnippetsModal
            | Action::SnippetsModalNext
            | Action::SnippetsModalPrev
            | Action::OpenStoredDrafts
            | Action::CloseStoredDraftsModal
            | Action::StoredDraftsModalNext
            | Action::StoredDraftsModalPrev
            | Action::StoredDraftsModalEdit
            | Action::StoredDraftsModalPreviewDelete
            | Action::StoredDraftsModalPreviewCancelSchedule
            | Action::StoredDraftsModalPreviewPush
            | Action::StoredDraftsModalCancelConfirmation
            | Action::StoredDraftsModalConfirm
            | Action::ClearFilter
            | Action::OpenMessageView
            | Action::CloseMessageView
            | Action::ToggleMailListMode => self.apply_mailbox_action(action),
            Action::OpenMailboxFilter
            | Action::SubmitSearch
            | Action::CycleSearchMode
            | Action::CloseSearch
            | Action::NextSearchResult
            | Action::PrevSearchResult => self.apply_search_action(action),
            Action::RefreshRules
            | Action::ToggleRuleEnabled
            | Action::DeleteRule
            | Action::ShowRuleHistory
            | Action::ShowRuleDryRun
            | Action::OpenRuleFormNew
            | Action::OpenRuleFormEdit
            | Action::SaveRuleForm => self.apply_rule_action(action),
            Action::OpenSavedSearchFormNew
            | Action::OpenSavedSearchFormEdit
            | Action::SaveSavedSearchForm
            | Action::DeleteSavedSearch => self.apply_saved_search_action(action),
            Action::EnableSemantic
            | Action::DisableSemantic
            | Action::ReindexSemantic
            | Action::BackfillSemantic
            | Action::InstallSemanticProfile(_)
            | Action::UseSemanticProfile(_) => self.apply_semantic_action(action),
            Action::DraftAssistCurrentThread
            | Action::DraftWithOptions
            | Action::DraftNewForSender
            | Action::RefinePendingDraft
            | Action::OpenVoiceProfile
            | Action::RebuildUserVoice
            | Action::OpenCommitments => self.apply_platform_action(action),
            Action::RefreshDiagnostics
            | Action::GenerateBugReport
            | Action::EditConfig
            | Action::OpenLogs
            | Action::OpenDiagnosticsPaneDetails => self.apply_diagnostics_action(action),
            Action::Compose | Action::Reply | Action::ReplyAll | Action::Forward => {
                self.apply_compose_action(action);
            }
            Action::Archive
            | Action::MarkReadAndArchive
            | Action::Trash
            | Action::Spam
            | Action::Star
            | Action::MarkRead
            | Action::MarkUnread
            | Action::UndoLastMutation
            | Action::ApplyLabel
            | Action::MoveToLabel
            | Action::RouteToLabel
            | Action::Unsubscribe
            | Action::ConfirmUnsubscribeOnly
            | Action::ConfirmUnsubscribeAndArchiveSender
            | Action::CancelUnsubscribe
            | Action::Snooze
            | Action::RespondInvite(_)
            | Action::RespondInviteWithComment(_)
            | Action::ToggleSelect
            | Action::VisualLineMode
            | Action::PatternSelect(_) => self.apply_mutation_action(action),
            Action::OpenInBrowser
            | Action::ToggleReaderMode
            | Action::ToggleHtmlView
            | Action::ToggleRemoteContent
            | Action::ToggleSignature
            | Action::AttachmentList
            | Action::OpenLinks
            | Action::ToggleFullscreen
            | Action::ExportThread => self.apply_message_action(action),
            Action::OpenCommandPalette | Action::CloseCommandPalette | Action::Help => {
                self.apply_modal_action(action);
            }
        }
        // The person page follows the cursor in Messages, and a Got it
        // never outlives the conversation it answers.
        self.sync_messages_page();
        self.guard_messages_ack();
        self.settle_hint_quiet();
    }

    #[cfg(debug_assertions)]
    fn dump_action_trace(&mut self) {
        let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let path = mxr_config::data_dir().join(format!("action-trace-{timestamp}.jsonl"));
        match self.recorder.flush_to(&path) {
            Ok(()) => {
                self.status_message = Some(format!("Action trace written to {}", path.display()));
            }
            Err(error) => {
                self.status_message = Some(format!("Action trace write failed: {error}"));
            }
        }
    }
}

fn render_plain_text_browser_document(text: &str) -> String {
    let escaped = htmlescape::encode_minimal(text);
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>mxr message</title><style>body{{margin:2rem;font:16px/1.5 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;background:#fafafa;color:#111;}}pre{{white-space:pre-wrap;word-break:break-word;}}</style></head><body><pre>{escaped}</pre></body></html>"
    )
}
