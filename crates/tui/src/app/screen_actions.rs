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
        self.modals.reply_later_prompt = None;
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
            Action::ReopenLastSendDraft => {
                if self.compose.pending_send_confirm.is_some()
                    || self.compose.pending_compose.is_some()
                {
                    self.push_toast(Toast::info(
                        "Close the current compose review before reopening a retained draft",
                    ));
                    return;
                }
                if self
                    .compose
                    .failed_send
                    .as_ref()
                    .is_some_and(|pending| !self.compose_file_owned(&pending.draft_path))
                {
                    if let Some(pending) = self.compose.failed_send.take() {
                        self.compose.pending_compose = Some(super::ComposeAction::EditDraft {
                            path: pending.draft_path,
                            account_id: pending.account_id,
                        });
                        return;
                    }
                }
                let mut owned: Vec<_> = self
                    .compose
                    .pending_send_mutations
                    .values()
                    .map(|(pending, _)| pending.draft_path.clone())
                    .collect();
                owned.extend(self.compose.pending_schedule_files.iter().cloned());
                owned.extend(self.compose.pending_draft_cleanup.iter().cloned());
                owned.extend(self.compose.active_draft_cleanup.iter().cloned());
                match crate::send_recovery_state::last_compose_excluding(&owned) {
                    Ok(Some((_, path))) if self.compose_file_owned(&path) => {
                        self.push_toast(Toast::info("This compose file is still submitting; wait for its outcome before reopening"));
                    }
                    Ok(Some((account_id, path))) if path.is_file() => {
                        self.compose.pending_compose =
                            Some(super::ComposeAction::EditDraft { path, account_id });
                    }
                    Ok(_) => self.push_toast(Toast::info(if owned.is_empty() { "No retained compose file; check the send outcome" } else { "Retained compose is still submitting; wait for its outcome before reopening" })),
                    Err(error) => {
                        self.push_toast(Toast::error(format!(
                            "Cannot read send recovery: {error}"
                        )));
                    }
                }
            }
            Action::PreviewArchiveSelection => {
                let ids = self.mutation_target_ids();
                if ids.is_empty() {
                    self.push_toast(Toast::info("Select messages to preview Archive"));
                    return;
                }
                self.pending_archive_preview = Some(ids);
                self.modals
                    .platform
                    .open_loading("Archive preview", "Preparing a retained review…");
            }
            Action::CheckArchiveOperation(ref id) | Action::SubmitArchiveOperation(ref id) => {
                let retained =
                    mxr_client::ArchiveJournal::new(&mxr_config::data_dir()).and_then(|journal| {
                        let operation = journal.load(id)?;
                        if matches!(action, Action::SubmitArchiveOperation(_)) {
                            journal.retain(&operation)?;
                        }
                        Ok(operation)
                    });
                match retained {
                    Ok(operation) => {
                        let request = if matches!(action, Action::SubmitArchiveOperation(_)) {
                            mxr_protocol::Request::SubmitArchive {
                                identity: operation.identity,
                                review_id: operation.review_id,
                                mutation: mxr_protocol::MutationCommand::Archive {
                                    message_ids: operation
                                        .items
                                        .into_iter()
                                        .map(|item| item.message_id)
                                        .collect(),
                                },
                            }
                        } else {
                            mxr_protocol::Request::GetArchiveOperation {
                                identity: operation.identity,
                            }
                        };
                        self.queue_platform_request(
                            request,
                            "Archive recovery",
                            "Querying the retained Archive operation...",
                        );
                    }
                    Err(error) => self.push_toast(Toast::error(format!(
                        "Cannot read Archive recovery: {error}"
                    ))),
                }
            }
            Action::CheckLastSendOutcome => {
                if let Some((account_id, identity)) = self.compose.last_send_identity.clone() {
                    self.queue_platform_request(
                        mxr_protocol::Request::GetSendOutcome {
                            account_id,
                            operation_id: self
                                .compose
                                .last_send_query
                                .clone()
                                .unwrap_or(identity.operation_id),
                        },
                        "Send outcome",
                        "Checking send outcome...",
                    );
                } else {
                    self.push_toast(Toast::info("No send identity retained in this session"));
                }
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
                self.user_sync_pending = true;
                // Background: the daemon acks as soon as the sync starts, so a
                // backfill that runs longer than the client's 60s IPC timeout
                // no longer times out the request. Progress arrives as
                // `OperationProgress` events and the list refreshes on
                // `SyncCompleted`.
                self.queue_mutation(
                    Request::SyncNow {
                        account_id: None,
                        background: true,
                    },
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
