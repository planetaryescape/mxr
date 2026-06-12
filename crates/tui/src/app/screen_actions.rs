use super::*;

impl App {
    /// Close transient overlay modals when switching top-level tabs so a
    /// picker/browser opened on one screen doesn't linger over another.
    /// Confirmation flows (error modal, bulk confirm, unsubscribe
    /// confirm, saved-search delete confirm) stay open — they guard
    /// decisions that are still pending; queued dispatch state is also
    /// left untouched.
    pub(super) fn close_transient_modals(&mut self) {
        self.modals.help_open = false;
        self.modals.help_query.clear();
        self.modals.help_selected = 0;
        self.modals.help_scroll_offset = 0;
        self.modals.help_context_filter = None;
        self.modals.label_picker.close();
        self.modals.snooze_panel.visible = false;
        self.modals.snooze_panel.custom_input = None;
        self.modals.snooze_panel.custom_error = None;
        self.modals.saved_search_form = None;
        self.modals.analytics_filter = None;
        self.modals.draft_options.close();
        self.modals.platform.close();
        self.modals.snippets.close();
        self.modals.sender_profile.close();
        self.modals.screener.close();
        self.modals.reply_queue.close();
        self.modals.activity.close();
        self.modals.summary.close();
        self.modals.briefing.close();
        self.modals.whois.close();
        self.modals.expert.close();
        self.modals.save_attachment.close();
        self.mailbox.url_modal = None;
        self.mailbox.attachment_panel.visible = false;
        self.command_palette.palette.visible = false;
    }

    pub(super) fn apply_screen_action(&mut self, action: Action) {
        match action {
            Action::OpenMailboxScreen => {
                self.close_transient_modals();
                if self.accounts.page.onboarding_required {
                    self.screen = Screen::Accounts;
                    self.accounts.page.onboarding_modal_open =
                        self.accounts.page.accounts.is_empty() && !self.accounts.page.form.visible;
                    return;
                }
                self.maybe_preserve_new_account_form_draft();
                self.screen = Screen::Mailbox;
                self.mailbox.active_pane = if self.mailbox.layout_mode == LayoutMode::ThreePane {
                    ActivePane::MailList
                } else {
                    self.mailbox.active_pane
                };
            }
            Action::OpenSearchScreen => {
                self.close_transient_modals();
                self.maybe_preserve_new_account_form_draft();
                self.mailbox.pending_preview_read = None;
                self.screen = Screen::Search;
                if self.search.page.has_session() {
                    self.search.page.editing = false;
                    if !self.search.page.result_selected {
                        self.clear_message_view_state();
                    }
                } else {
                    self.reset_search_page_workspace();
                    self.search.page.editing = true;
                    self.search.page.sort = SortOrder::DateDesc;
                }
            }
            Action::OpenGlobalSearch => {
                self.close_transient_modals();
                self.maybe_preserve_new_account_form_draft();
                self.mailbox.pending_preview_read = None;
                self.search.bar.deactivate();
                self.screen = Screen::Search;
                if !self.search.page.has_session() {
                    self.reset_search_page_workspace();
                }
                self.search.page.editing = true;
                self.search.page.active_pane = SearchPane::Results;
                if !self.search.page.result_selected {
                    self.clear_message_view_state();
                }
            }
            Action::OpenRulesScreen => {
                self.close_transient_modals();
                self.maybe_preserve_new_account_form_draft();
                self.mailbox.pending_preview_read = None;
                self.screen = Screen::Rules;
                self.rules.page.refresh_pending = true;
            }
            Action::OpenDiagnosticsScreen => {
                self.close_transient_modals();
                self.maybe_preserve_new_account_form_draft();
                self.mailbox.pending_preview_read = None;
                self.screen = Screen::Diagnostics;
                self.diagnostics.page.refresh_pending = true;
            }
            Action::OpenAccountsScreen => {
                self.close_transient_modals();
                self.mailbox.pending_preview_read = None;
                self.screen = Screen::Accounts;
                self.accounts.page.refresh_pending = true;
            }
            Action::OpenTab1 => {
                self.apply(Action::OpenMailboxScreen);
            }
            Action::OpenTab2 => {
                self.apply(Action::OpenSearchScreen);
            }
            Action::OpenTab3 => {
                self.apply(Action::OpenRulesScreen);
            }
            Action::OpenTab4 => {
                self.apply(Action::OpenAccountsScreen);
            }
            Action::OpenTab5 => {
                self.apply(Action::OpenDiagnosticsScreen);
            }
            Action::OpenTab6 => {
                self.apply(Action::OpenAnalyticsScreen);
            }
            Action::OpenTab7 => {
                self.close_transient_modals();
                self.screen = Screen::Deliveries;
                self.deliveries.loading = true;
                self.pending_deliveries_refresh = true;
            }
            // Command palette
            Action::SyncNow => {
                self.queue_mutation(
                    Request::SyncNow { account_id: None },
                    MutationEffect::RefreshList,
                    "Syncing...".into(),
                );
            }
            // Message view
            Action::Noop => {}
            // Intercepted in lib.rs before apply(); no-op if it somehow arrives here
            Action::CancelOutlookAuth => {}
            _ => unreachable!("action routed to wrong handler"),
        }
    }
}
