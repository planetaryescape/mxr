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
        if self.search_is_pending() {
            self.search.page.throbber.calc_next();
        }
        if self.mailbox.mailbox_loading_message.is_some() {
            self.mailbox.mailbox_loading_throbber.calc_next();
        }
        if self.accounts.page.operation_in_flight {
            self.accounts.page.throbber.calc_next();
        }
        self.process_pending_search_debounce();
        self.process_pending_preview_read();
    }

    pub fn apply(&mut self, action: Action) {
        // Clear status message on any action
        self.status_message = None;

        match action {
            Action::RefreshAccounts
            | Action::OpenAccountFormNew
            | Action::SaveAccountForm
            | Action::TestAccountForm
            | Action::ReauthorizeAccountForm
            | Action::SetDefaultAccount
            | Action::SwitchAccount(_) => self.apply_account_action(action),
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
            | Action::SyncNow
            | Action::Noop => self.apply_screen_action(action),
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
            | Action::GoToLabel
            | Action::SelectLabel(_)
            | Action::SelectSavedSearch(_, _)
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
            Action::RefreshDiagnostics
            | Action::GenerateBugReport
            | Action::EditConfig
            | Action::OpenLogs
            | Action::ShowOnboarding
            | Action::OpenDiagnosticsPaneDetails => self.apply_diagnostics_action(action),
            Action::Compose | Action::Reply | Action::ReplyAll | Action::Forward => {
                self.apply_compose_action(action)
            }
            Action::Archive
            | Action::MarkReadAndArchive
            | Action::Trash
            | Action::Spam
            | Action::Star
            | Action::MarkRead
            | Action::MarkUnread
            | Action::ApplyLabel
            | Action::MoveToLabel
            | Action::Unsubscribe
            | Action::ConfirmUnsubscribeOnly
            | Action::ConfirmUnsubscribeAndArchiveSender
            | Action::CancelUnsubscribe
            | Action::Snooze
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
                self.apply_modal_action(action)
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
