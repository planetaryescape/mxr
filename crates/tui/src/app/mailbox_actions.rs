use super::*;

impl App {
    pub(super) fn apply_mailbox_action(&mut self, action: Action) {
        match action {
            Action::MoveDown => {
                if self.screen == Screen::Search {
                    if self.search.page.selected_index + 1 < self.search_row_count() {
                        self.search.page.selected_index += 1;
                    }
                    self.sync_search_cursor_after_move();
                    return;
                }
                if self.mailbox.selected_index + 1 < self.mail_row_count() {
                    self.mailbox.selected_index += 1;
                }
                self.ensure_visible();
                self.update_visual_selection();
                self.auto_preview();
            }
            Action::MoveUp => {
                if self.screen == Screen::Search {
                    if self.search.page.selected_index > 0 {
                        self.search.page.selected_index -= 1;
                    }
                    self.sync_search_cursor_after_move();
                    return;
                }
                if self.mailbox.selected_index > 0 {
                    self.mailbox.selected_index -= 1;
                }
                self.ensure_visible();
                self.update_visual_selection();
                self.auto_preview();
            }
            Action::JumpTop => {
                if self.screen == Screen::Search {
                    self.search.page.selected_index = 0;
                    self.sync_search_cursor_after_move();
                    return;
                }
                self.mailbox.selected_index = 0;
                self.mailbox.scroll_offset = 0;
                self.auto_preview();
            }
            Action::JumpBottom => {
                if self.screen == Screen::Search {
                    if self.search.page.has_more {
                        self.search.page.load_to_end = true;
                        self.load_more_search_results();
                    } else if self.search_row_count() > 0 {
                        self.search.page.selected_index = self.search_row_count() - 1;
                        self.sync_search_cursor_after_move();
                    }
                    return;
                }
                if self.mail_row_count() > 0 {
                    self.mailbox.selected_index = self.mail_row_count() - 1;
                }
                self.ensure_visible();
                self.auto_preview();
            }
            Action::PageDown => {
                if self.screen == Screen::Search {
                    let page = self.visible_height.max(1);
                    self.search.page.selected_index = (self.search.page.selected_index + page)
                        .min(self.search_row_count().saturating_sub(1));
                    self.sync_search_cursor_after_move();
                    return;
                }
                let page = self.visible_height.max(1);
                self.mailbox.selected_index = (self.mailbox.selected_index + page)
                    .min(self.mail_row_count().saturating_sub(1));
                self.ensure_visible();
                self.auto_preview();
            }
            Action::PageUp => {
                if self.screen == Screen::Search {
                    let page = self.visible_height.max(1);
                    self.search.page.selected_index =
                        self.search.page.selected_index.saturating_sub(page);
                    self.sync_search_cursor_after_move();
                    return;
                }
                let page = self.visible_height.max(1);
                self.mailbox.selected_index = self.mailbox.selected_index.saturating_sub(page);
                self.ensure_visible();
                self.auto_preview();
            }
            Action::ViewportTop => {
                self.mailbox.selected_index = self.mailbox.scroll_offset;
                self.auto_preview();
            }
            Action::ViewportMiddle => {
                let visible_height = 20;
                self.mailbox.selected_index = (self.mailbox.scroll_offset + visible_height / 2)
                    .min(self.mail_row_count().saturating_sub(1));
                self.auto_preview();
            }
            Action::ViewportBottom => {
                let visible_height = 20;
                self.mailbox.selected_index = (self.mailbox.scroll_offset + visible_height)
                    .min(self.mail_row_count().saturating_sub(1));
                self.auto_preview();
            }
            Action::CenterCurrent => {
                let visible_height = 20;
                self.mailbox.scroll_offset = self
                    .mailbox
                    .selected_index
                    .saturating_sub(visible_height / 2);
            }
            Action::SwitchPane => {
                if self.screen == Screen::Search {
                    self.search.page.active_pane = match self.search.page.active_pane {
                        SearchPane::Results => {
                            self.maybe_open_search_preview();
                            self.search.page.active_pane
                        }
                        SearchPane::Preview => SearchPane::Results,
                    };
                    return;
                }
                self.mailbox.active_pane =
                    match (self.mailbox.layout_mode, self.mailbox.active_pane) {
                        // ThreePane: Sidebar → MailList → MessageView → Sidebar
                        (LayoutMode::ThreePane, ActivePane::Sidebar) => ActivePane::MailList,
                        (LayoutMode::ThreePane, ActivePane::MailList) => ActivePane::MessageView,
                        (LayoutMode::ThreePane, ActivePane::MessageView) => ActivePane::Sidebar,
                        // FullScreen: Sidebar → MessageView → Sidebar
                        (LayoutMode::FullScreen, ActivePane::Sidebar) => ActivePane::MessageView,
                        (LayoutMode::FullScreen, ActivePane::MessageView) => ActivePane::Sidebar,
                        // TwoPane: Sidebar → MailList → Sidebar
                        (_, ActivePane::Sidebar) => ActivePane::MailList,
                        (_, ActivePane::MailList) => ActivePane::Sidebar,
                        (_, ActivePane::MessageView) => ActivePane::Sidebar,
                    };
            }
            Action::OpenSelected => {
                if let Some(pending) = self.modals.pending_bulk_confirm.take() {
                    let snapshot = pending
                        .optimistic_effect
                        .as_ref()
                        .map(|effect| self.snapshot_for_effect(effect));
                    if let Some(effect) = pending.optimistic_effect.as_ref() {
                        self.apply_local_mutation_effect(effect);
                    }
                    let id = self.queue_mutation(
                        pending.request,
                        pending.effect,
                        pending.status_message,
                    );
                    if let Some(snapshot) = snapshot {
                        self.mutation_snapshots.insert(id, snapshot);
                    }
                    self.clear_selection();
                    return;
                }
                if self.screen == Screen::Search {
                    self.open_selected_search_result();
                    return;
                }
                if self.mailbox.mailbox_view == MailboxView::Subscriptions {
                    if let Some(entry) = self.selected_subscription_entry().cloned() {
                        self.open_envelope(entry.envelope);
                        self.mailbox.layout_mode = LayoutMode::ThreePane;
                        self.mailbox.active_pane = ActivePane::MessageView;
                    }
                    return;
                }
                if let Some(row) = self.selected_mail_row() {
                    self.open_envelope(row.representative);
                    self.mailbox.layout_mode = LayoutMode::ThreePane;
                    self.mailbox.active_pane = ActivePane::MessageView;
                }
            }
            Action::Back => match self.mailbox.active_pane {
                _ if self.screen != Screen::Mailbox => {
                    self.screen = Screen::Mailbox;
                }
                ActivePane::MessageView => {
                    self.apply(Action::CloseMessageView);
                }
                ActivePane::MailList => {
                    if !self.mailbox.selected_set.is_empty() {
                        self.apply(Action::ClearSelection);
                    } else if self.search.active {
                        self.apply(Action::CloseSearch);
                    } else if self.mailbox.active_label.is_some() {
                        self.apply(Action::ClearFilter);
                    } else if self.mailbox.layout_mode == LayoutMode::ThreePane {
                        self.apply(Action::CloseMessageView);
                    }
                }
                ActivePane::Sidebar => {}
            },
            Action::QuitView => {
                self.should_quit = true;
            }
            Action::ClearSelection => {
                self.clear_selection();
                self.status_message = Some("Selection cleared".into());
            }
            // Search
            Action::GoToInbox => {
                if let Some(label) = self.mailbox.labels.iter().find(|l| l.name == "INBOX") {
                    self.apply(Action::SelectLabel(label.id.clone()));
                } else {
                    self.mailbox.desired_system_mailbox = Some("INBOX".into());
                }
            }
            Action::GoToStarred => {
                if let Some(label) = self.mailbox.labels.iter().find(|l| l.name == "STARRED") {
                    self.apply(Action::SelectLabel(label.id.clone()));
                } else {
                    self.mailbox.desired_system_mailbox = Some("STARRED".into());
                }
            }
            Action::GoToSent => {
                if let Some(label) = self.mailbox.labels.iter().find(|l| l.name == "SENT") {
                    self.apply(Action::SelectLabel(label.id.clone()));
                } else {
                    self.mailbox.desired_system_mailbox = Some("SENT".into());
                }
            }
            Action::GoToDrafts => {
                if let Some(label) = self.mailbox.labels.iter().find(|l| l.name == "DRAFT") {
                    self.apply(Action::SelectLabel(label.id.clone()));
                } else {
                    self.mailbox.desired_system_mailbox = Some("DRAFT".into());
                }
            }
            Action::GoToAllMail => {
                self.mailbox.mailbox_view = MailboxView::Messages;
                self.apply(Action::ClearFilter);
            }
            Action::OpenSubscriptions => {
                self.mailbox.mailbox_view = MailboxView::Subscriptions;
                self.mailbox.active_label = None;
                self.mailbox.pending_active_label = None;
                self.mailbox.pending_label_fetch = None;
                self.mailbox.pending_preview_read = None;
                self.mailbox.desired_system_mailbox = None;
                self.search.active = false;
                self.screen = Screen::Mailbox;
                self.mailbox.active_pane = ActivePane::MailList;
                self.mailbox.selected_index = self.mailbox.selected_index.min(
                    self.mailbox
                        .subscriptions_page
                        .entries
                        .len()
                        .saturating_sub(1),
                );
                self.mailbox.scroll_offset = 0;
                if self.mailbox.subscriptions_page.entries.is_empty() {
                    self.mailbox.pending_subscriptions_refresh = true;
                }
                self.auto_preview();
            }
            Action::OpenOwedReplies => {
                self.mailbox.mailbox_view = MailboxView::Owed;
                self.mailbox.active_label = None;
                self.mailbox.pending_active_label = None;
                self.mailbox.pending_label_fetch = None;
                self.mailbox.pending_preview_read = None;
                self.mailbox.desired_system_mailbox = None;
                self.search.active = false;
                self.screen = Screen::Mailbox;
                self.mailbox.active_pane = ActivePane::MailList;
                self.mailbox.selected_index = self.mailbox.selected_index.min(
                    self.mailbox.owed_page.entries.len().saturating_sub(1),
                );
                self.mailbox.scroll_offset = 0;
                self.mailbox.pending_owed_refresh = true;
            }
            Action::GoToLabel => {
                self.mailbox.mailbox_view = MailboxView::Messages;
                self.apply(Action::ClearFilter);
            }
            Action::OpenMessageView => {
                if self.screen == Screen::Search {
                    self.open_selected_search_result();
                    return;
                }
                if self.mailbox.mailbox_view == MailboxView::Subscriptions {
                    if let Some(entry) = self.selected_subscription_entry().cloned() {
                        self.open_envelope(entry.envelope);
                        self.mailbox.layout_mode = LayoutMode::ThreePane;
                    }
                } else if let Some(row) = self.selected_mail_row() {
                    self.open_envelope(row.representative);
                    self.mailbox.layout_mode = LayoutMode::ThreePane;
                }
            }
            Action::CloseMessageView => {
                if self.screen == Screen::Search {
                    self.reset_search_preview_selection();
                    return;
                }
                self.close_attachment_panel();
                self.mailbox.layout_mode = LayoutMode::TwoPane;
                self.mailbox.active_pane = ActivePane::MailList;
                self.mailbox.pending_preview_read = None;
                self.mailbox.viewing_envelope = None;
                self.mailbox.viewed_thread = None;
                self.mailbox.viewed_thread_messages.clear();
                self.mailbox.thread_selected_index = 0;
                self.mailbox.pending_thread_fetch = None;
                self.mailbox.in_flight_thread_fetch = None;
                self.mailbox.message_scroll_offset = 0;
                self.mailbox.body_view_state = BodyViewState::Empty { preview: None };
            }
            Action::ToggleMailListMode => {
                if self.mailbox.mailbox_view == MailboxView::Subscriptions {
                    return;
                }
                let search_row_message_id = (self.screen == Screen::Search)
                    .then(|| self.selected_search_envelope().map(|env| env.id.clone()))
                    .flatten();
                self.mailbox.mail_list_mode = match self.mailbox.mail_list_mode {
                    MailListMode::Threads => MailListMode::Messages,
                    MailListMode::Messages => MailListMode::Threads,
                };
                if self.screen == Screen::Search {
                    self.search.page.selected_index = search_row_message_id
                        .as_ref()
                        .and_then(|message_id| self.search_row_index_for_message(message_id))
                        .unwrap_or(0)
                        .min(self.search_row_count().saturating_sub(1));
                    if self.search.page.result_selected {
                        self.sync_search_cursor_after_move();
                    } else if self.search_row_count() > 0 {
                        self.ensure_search_visible();
                    }
                } else {
                    self.mailbox.selected_index = self
                        .mailbox
                        .selected_index
                        .min(self.mail_row_count().saturating_sub(1));
                }
            }
            Action::SelectLabel(label_id) => {
                self.mailbox.mailbox_view = MailboxView::Messages;
                self.mailbox.pending_label_fetch = Some(label_id);
                self.mailbox.pending_active_label = self.mailbox.pending_label_fetch.clone();
                self.mailbox.desired_system_mailbox = None;
                self.mailbox.active_pane = ActivePane::MailList;
                self.screen = Screen::Mailbox;
            }
            Action::OpenSavedSearchByIndex(index) => {
                if index == 0 {
                    self.apply(Action::ClearFilter);
                    return;
                }
                // 1-indexed lookup; out-of-range is a safe no-op.
                let Some(search) = self.mailbox.saved_searches.get(index - 1).cloned() else {
                    return;
                };
                self.apply(Action::SelectSavedSearch(search.query, search.search_mode));
            }
            Action::FlagReplyLater => {
                let Some(env) = self.context_envelope() else {
                    self.status_message = Some("No message selected".into());
                    return;
                };
                let id = env.id.clone();
                let effect = MutationEffect::ReplyLater {
                    message_id: id.clone(),
                    flag: true,
                    status: "Marked for reply later".into(),
                };
                let snapshot = self.snapshot_for_effect(&effect);
                self.apply_local_mutation_effect(&effect);
                let mutation_id = self.queue_mutation(
                    Request::SetReplyLater {
                        message_id: id,
                        flag: true,
                    },
                    effect,
                    "Marking for reply later...".into(),
                );
                self.mutation_snapshots.insert(mutation_id, snapshot);
            }
            Action::OpenReplyQueue => {
                self.modals.reply_queue.open_loading();
                self.pending_reply_queue_refresh = true;
                self.status_message = Some("Loading reply-later queue...".into());
            }
            Action::CloseReplyQueueModal => {
                self.modals.reply_queue.close();
            }
            Action::ReplyQueueModalNext => {
                self.modals.reply_queue.select_next();
            }
            Action::ReplyQueueModalPrev => {
                self.modals.reply_queue.select_prev();
            }
            Action::ReplyQueueModalReply => {
                let Some(env) = self.modals.reply_queue.selected().cloned() else {
                    self.status_message = Some("Reply queue is empty".into());
                    return;
                };
                self.modals.reply_queue.close();
                self.compose.pending_compose = Some(ComposeAction::Reply {
                    message_id: env.id,
                    account_id: env.account_id,
                });
                self.status_message = Some("Opening reply from reply queue...".into());
            }
            Action::OpenScreenerQueue => {
                // Reach for an account_id from any visible envelope so
                // we don't need to plumb a "default account" surface
                // here. The CLI is the right tool for cross-account
                // sweeps; the modal scopes to the inbox the user is
                // already looking at.
                let Some(account_id) = self
                    .mailbox
                    .envelopes
                    .first()
                    .map(|env| env.account_id.clone())
                    .or_else(|| self.context_envelope().map(|env| env.account_id.clone()))
                else {
                    self.status_message =
                        Some("Screener: open an inbox first so we know which account".into());
                    return;
                };
                self.modals.screener.open_loading(account_id.clone());
                self.pending_screener_refresh = Some(account_id);
            }
            Action::CloseScreenerModal => {
                self.modals.screener.close();
            }
            Action::ScreenerModalNext => {
                self.modals.screener.select_next();
            }
            Action::ScreenerModalPrev => {
                self.modals.screener.select_prev();
            }
            Action::ScreenerDisposeAllow => {
                self.dispatch_screener_disposition(mxr_protocol::ScreenerDispositionData::Allow);
            }
            Action::ScreenerDisposeDeny => {
                self.dispatch_screener_disposition(mxr_protocol::ScreenerDispositionData::Deny);
            }
            Action::ScreenerDisposeFeed => {
                self.dispatch_screener_disposition(mxr_protocol::ScreenerDispositionData::Feed);
            }
            Action::ScreenerDisposePaperTrail => {
                self.dispatch_screener_disposition(
                    mxr_protocol::ScreenerDispositionData::PaperTrail,
                );
            }
            Action::OpenSenderView => {
                let Some(env) = self.context_envelope() else {
                    self.status_message = Some("No message selected".into());
                    return;
                };
                let email = env.from.email.clone();
                let account_id = env.account_id.clone();
                self.modals
                    .sender_profile
                    .open_loading(email.clone(), Some(env.thread_id.clone()));
                self.pending_sender_profile_request = Some((account_id, email));
            }
            Action::CloseSenderViewModal => {
                self.modals.sender_profile.close();
            }
            Action::SenderProfileNextMessage => {
                self.modals.sender_profile.select_next_recent_message();
            }
            Action::SenderProfilePrevMessage => {
                self.modals.sender_profile.select_prev_recent_message();
            }
            Action::OpenSenderProfileMessage => {
                let Some(message) = self.modals.sender_profile.selected_recent_message() else {
                    self.status_message = Some("No other sender emails to open".into());
                    return;
                };
                let Some(account_id) = self
                    .modals
                    .sender_profile
                    .profile
                    .as_ref()
                    .map(|profile| profile.account_id.clone())
                else {
                    self.status_message = Some("Sender profile not loaded".into());
                    return;
                };

                let env = self
                    .mailbox
                    .envelopes
                    .iter()
                    .chain(self.search.page.results.iter())
                    .find(|env| env.thread_id == message.thread_id)
                    .cloned()
                    .unwrap_or_else(|| Envelope {
                        id: message.message_id.clone(),
                        account_id,
                        provider_id: String::new(),
                        thread_id: message.thread_id.clone(),
                        message_id_header: None,
                        in_reply_to: None,
                        references: Vec::new(),
                        from: Address {
                            name: message.from_name.clone(),
                            email: message.from_email.clone(),
                        },
                        to: Vec::new(),
                        cc: Vec::new(),
                        bcc: Vec::new(),
                        subject: message.subject.clone(),
                        date: message.date,
                        flags: MessageFlags::empty(),
                        snippet: message.snippet.clone(),
                        has_attachments: message.has_attachments,
                        size_bytes: 0,
                        unsubscribe: UnsubscribeMethod::None,
                        label_provider_ids: Vec::new(),
                    });

                if let Some(index) = self
                    .mail_list_rows()
                    .iter()
                    .position(|row| row.thread_id == env.thread_id)
                {
                    self.mailbox.selected_index = index;
                    self.ensure_visible();
                    self.update_visual_selection();
                }
                self.modals.sender_profile.close();
                self.open_envelope(env);
                self.status_message = Some("Opened sender email".into());
            }
            Action::SummarizeCurrentThread => {
                let Some(env) = self.context_envelope() else {
                    self.status_message = Some("No message selected".into());
                    return;
                };
                let thread_id = env.thread_id.clone();
                self.modals.summary.open_loading(thread_id.clone());
                self.pending_summary_request = Some(thread_id);
                self.status_message = Some("Summarizing thread...".into());
            }
            Action::CloseSummaryModal => {
                self.modals.summary.close();
            }
            Action::OpenSnippets => {
                self.modals.snippets.open_loading();
                self.pending_snippets_refresh = true;
                self.status_message = Some("Loading snippets...".into());
            }
            Action::CloseSnippetsModal => {
                self.modals.snippets.close();
            }
            Action::SnippetsModalNext => {
                self.modals.snippets.select_next();
            }
            Action::SnippetsModalPrev => {
                self.modals.snippets.select_prev();
            }
            Action::SelectSavedSearch(query, mode) => {
                self.mailbox.mailbox_view = MailboxView::Messages;
                if self.screen == Screen::Search {
                    self.search.page.query = query.clone();
                    self.search.page.editing = false;
                    self.search.page.mode = mode;
                    self.search.page.sort = SortOrder::DateDesc;
                    self.search.page.active_pane = SearchPane::Results;
                    self.search.bar.query = query.clone();
                    self.search.bar.mode = mode;
                    self.trigger_live_search();
                } else {
                    self.search.active = true;
                    self.mailbox.active_pane = ActivePane::MailList;
                    self.search.bar.query = query.clone();
                    self.search.bar.mode = mode;
                    self.trigger_live_search();
                }
            }
            Action::ClearFilter => {
                self.mailbox.mailbox_view = MailboxView::Messages;
                self.mailbox.active_label = None;
                self.mailbox.pending_active_label = None;
                self.mailbox.pending_preview_read = None;
                self.mailbox.desired_system_mailbox = None;
                self.search.active = false;
                self.mailbox.envelopes = self.all_mail_envelopes();
                self.mailbox.selected_index = 0;
                self.mailbox.scroll_offset = 0;
            }

            // Phase 2: Email actions (Gmail-native A005)
            _ => unreachable!("action routed to wrong handler"),
        }
    }
}
