use super::*;
use ratatui::crossterm;

pub(super) fn plain_or_shift(modifiers: KeyModifiers) -> bool {
    modifiers.is_empty() || modifiers == KeyModifiers::SHIFT
}

fn draft_from_pending(
    pending: &PendingSend,
    draft_id: mxr_core::id::DraftId,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<mxr_core::Draft, mxr_compose::frontmatter::ComposeError> {
    let parse_addrs = |s: &str| mxr_mail_parse::parse_address_list(s);
    let reply_headers =
        pending
            .fm
            .in_reply_to
            .as_ref()
            .map(|in_reply_to| mxr_core::types::ReplyHeaders {
                in_reply_to: in_reply_to.clone(),
                references: pending.fm.references.clone(),
                thread_id: pending.fm.thread_id.clone(),
            });
    let mut draft = mxr_core::Draft {
        revision: pending.draft_revision.or(Some(1)),
        id: draft_id,
        account_id: pending.account_id.clone(),
        from: mxr_compose::draft_codec::parse_from_field(&pending.fm.from)
            .ok()
            .flatten(),
        reply_headers,
        intent: pending.intent,
        to: parse_addrs(&pending.fm.to),
        cc: parse_addrs(&pending.fm.cc),
        bcc: parse_addrs(&pending.fm.bcc),
        subject: pending.fm.subject.clone(),
        content: mxr_core::DraftContent::markdown(pending.body.clone()),
        managed_attachments: None,
        attachments: pending
            .fm
            .attach
            .iter()
            .map(std::path::PathBuf::from)
            .collect(),
        inline_assets: Vec::new(),
        inline_calendar_reply: pending.invite_reply.clone(),
        created_at: now,
        updated_at: now,
    };
    mxr_compose::forward::apply(&mut draft, &pending.fm)?;
    Ok(draft)
}

impl App {
    pub(crate) fn keybinding_context(&self) -> crate::keybindings::ViewContext {
        use crate::keybindings::ViewContext;
        if matches!(
            self.current_ui_context(),
            UiContext::MailboxMessage | UiContext::SearchPreview
        ) {
            if self.mailbox.viewed_thread_messages.len() > 1 {
                ViewContext::ThreadView
            } else {
                ViewContext::MessageView
            }
        } else {
            ViewContext::MailList
        }
    }

    pub(crate) fn help_keybinding_context(&self) -> crate::keybindings::ViewContext {
        if self.help_modal_context() == self.current_ui_context() {
            self.keybinding_context()
        } else if matches!(
            self.help_modal_context(),
            UiContext::MailboxMessage | UiContext::SearchPreview
        ) {
            crate::keybindings::ViewContext::MessageView
        } else {
            crate::keybindings::ViewContext::MailList
        }
    }

    pub(crate) fn load_keybindings_from(&mut self, directory: &std::path::Path) {
        self.keybindings = crate::keybindings::load_keybindings(directory);
        self.input.reset();
    }

    fn configured_mail_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Option<Action>> {
        let context = self.keybinding_context();
        let resolved = self
            .input
            .handle_configured_key(&self.keybindings, context, key);
        let Some(name) = resolved else {
            // Some historic mail defaults are not listed in the display
            // registry (e.g. undo). Retire those fallback aliases as well.
            if !self.input.is_pending()
                && self
                    .mail_action_key(key)
                    .is_some_and(|action| self.keybindings.has_action_override(context, &action))
            {
                return Some(None);
            }
            return None;
        };
        let Some(name) = name else {
            return Some(None);
        };
        let action = crate::keybindings::action_from_name(&name)?;
        if !crate::action::action_allowed_in_context(&action, self.current_ui_context()) {
            return Some(None);
        }
        // Reader navigation is intentionally inline in the existing router.
        if matches!(
            context,
            crate::keybindings::ViewContext::MessageView
                | crate::keybindings::ViewContext::ThreadView
        ) {
            match name.as_str() {
                "scroll_down" | "move_down" => {
                    self.move_message_view_down();
                    return Some(None);
                }
                "scroll_up" | "move_up" => {
                    self.move_message_view_up();
                    return Some(None);
                }
                "next_message" => {
                    self.move_thread_focus_down();
                    return Some(None);
                }
                "prev_message" => {
                    self.move_thread_focus_up();
                    return Some(None);
                }
                "page_down" => {
                    self.mailbox.message_scroll_offset =
                        self.mailbox.message_scroll_offset.saturating_add(20);
                    return Some(None);
                }
                "page_up" => {
                    self.mailbox.message_scroll_offset =
                        self.mailbox.message_scroll_offset.saturating_sub(20);
                    return Some(None);
                }
                "jump_bottom" => {
                    self.mailbox.message_scroll_offset = u16::MAX;
                    return Some(None);
                }
                _ => {}
            }
        }
        Some(Some(action))
    }

    fn help_modal_state(&self) -> crate::ui::help_modal::HelpModalState<'_> {
        crate::ui::help_modal::HelpModalState {
            keybindings: Some(&self.keybindings),
            binding_context: Some(self.help_keybinding_context()),
            open: self.modals.help_open,
            ui_context: self.help_modal_context(),
            selected_count: self.mailbox.selected_set.len(),
            scroll_offset: self.modals.help_scroll_offset,
            query: &self.modals.help_query,
            selected: self.modals.help_selected,
            mode_guide: self.help_mode_guide(),
            key_places: self.help_key_places(),
            mode_common: self.mode_strip_hints(),
            _marker: std::marker::PhantomData,
        }
    }

    /// Context whose bindings the help modal shows: the user's Tab-cycled
    /// override when set, otherwise the focused view.
    pub(crate) fn help_modal_context(&self) -> UiContext {
        self.modals
            .help_context_filter
            .unwrap_or_else(|| self.current_ui_context())
    }

    /// Cycle the help modal's context filter (Tab while help is open).
    /// Starts from the focused view and wraps; landing back on the
    /// focused view clears the override so help tracks focus again.
    fn cycle_help_context(&mut self) {
        const CYCLE: [UiContext; 8] = [
            UiContext::MailboxList,
            UiContext::MailboxMessage,
            UiContext::SearchResults,
            UiContext::RulesList,
            UiContext::Diagnostics,
            UiContext::AccountsList,
            UiContext::Analytics,
            UiContext::Deliveries,
        ];
        let current = self.help_modal_context();
        let position = CYCLE.iter().position(|context| *context == current);
        let next = match position {
            Some(index) => CYCLE[(index + 1) % CYCLE.len()],
            // Focused view isn't in the cycle list (sidebar/form variants):
            // start from the top.
            None => CYCLE[0],
        };
        self.modals.help_context_filter = if next == self.current_ui_context() {
            None
        } else {
            Some(next)
        };
        self.modals.help_scroll_offset = 0;
        self.clamp_help_selected();
    }

    fn help_search_active(&self) -> bool {
        !self.modals.help_query.is_empty()
    }

    fn help_search_result_count(&self) -> usize {
        crate::ui::help_modal::search_result_count(&self.help_modal_state())
    }

    fn clamp_help_selected(&mut self) {
        let count = self.help_search_result_count();
        self.modals.help_selected = self.modals.help_selected.min(count.saturating_sub(1));
    }

    fn push_help_query(&mut self, c: char) {
        self.modals.help_query.push(c);
        self.modals.help_selected = 0;
    }

    fn pop_help_query(&mut self) {
        self.modals.help_query.pop();
        if self.modals.help_query.is_empty() {
            self.modals.help_selected = 0;
        } else {
            self.clamp_help_selected();
        }
    }

    fn help_select_next(&mut self) {
        let count = self.help_search_result_count();
        if count > 0 {
            self.modals.help_selected = (self.modals.help_selected + 1).min(count - 1);
        }
    }

    fn help_select_prev(&mut self) {
        self.modals.help_selected = self.modals.help_selected.saturating_sub(1);
    }

    fn help_page_down(&mut self) {
        let count = self.help_search_result_count();
        if count > 0 {
            self.modals.help_selected = (self.modals.help_selected + 8).min(count - 1);
        }
    }

    fn help_page_up(&mut self) {
        self.modals.help_selected = self.modals.help_selected.saturating_sub(8);
    }

    pub(super) fn contextual_input_action(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> Option<Action> {
        let action = self.input.handle_key(key)?;
        if matches!(
            self.current_ui_context(),
            UiContext::MailboxList
                | UiContext::MailboxMessage
                | UiContext::SearchResults
                | UiContext::SearchPreview
        ) && !matches!(action, Action::MoveDown | Action::MoveUp)
            && self
                .keybindings
                .has_action_override(self.keybinding_context(), &action)
        {
            return None;
        }
        crate::action::action_allowed_in_context(&action, self.current_ui_context())
            .then_some(action)
    }

    fn mail_action_key(&self, key: crossterm::event::KeyEvent) -> Option<Action> {
        match (key.code, key.modifiers) {
            (KeyCode::Char('c'), KeyModifiers::NONE) => Some(Action::Compose),
            (KeyCode::Char('r'), KeyModifiers::NONE) => Some(Action::Reply),
            (KeyCode::Char('a'), KeyModifiers::NONE) => Some(Action::ReplyAll),
            (KeyCode::Char('f'), KeyModifiers::NONE) => Some(Action::Forward),
            (KeyCode::Char('e'), KeyModifiers::NONE) => Some(Action::Archive),
            (KeyCode::Char('m'), KeyModifiers::NONE) => Some(Action::MarkReadAndArchive),
            (KeyCode::Char('#'), _) => Some(Action::Trash),
            // The web app's Delete and Backspace trash too (with undo).
            (KeyCode::Delete | KeyCode::Backspace, KeyModifiers::NONE) => Some(Action::Trash),
            (KeyCode::Char('!'), _) => Some(Action::Spam),
            (KeyCode::Char('u'), KeyModifiers::NONE) => Some(Action::UndoLastMutation),
            (KeyCode::Char('s'), KeyModifiers::NONE) => Some(Action::Star),
            (KeyCode::Char('I'), modifiers) if plain_or_shift(modifiers) => Some(Action::MarkRead),
            (KeyCode::Char('U'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::MarkUnread)
            }
            (KeyCode::Char('l'), KeyModifiers::NONE) => Some(Action::ApplyLabel),
            (KeyCode::Char('v'), KeyModifiers::NONE) => Some(Action::MoveToLabel),
            (KeyCode::Char('D'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::Unsubscribe)
            }
            (KeyCode::Char('O'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::OpenInBrowser)
            }
            (KeyCode::Char('R'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::ToggleReaderMode)
            }
            (KeyCode::Char('S'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::ToggleSignature)
            }
            (KeyCode::Char('A'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::AttachmentList)
            }
            (KeyCode::Char('L'), modifiers) if plain_or_shift(modifiers) => Some(Action::OpenLinks),
            (KeyCode::Char('t'), KeyModifiers::NONE) => Some(Action::CreateTodoFromMessage),
            // Slice 5.1 (C2.6): B opens the thread briefing modal.
            (KeyCode::Char('B'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::OpenThreadBriefing)
            }
            // Same keys as the web app. Reading and Paper trail take `p`
            // for pin before these mail keys are consulted.
            (KeyCode::Char('p'), KeyModifiers::NONE) => Some(Action::OpenSenderView),
            _ => None,
        }
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        // A key acts when it goes down. Windows consoles also report the
        // key coming up; acting on that too would run every key twice.
        if key.kind == crossterm::event::KeyEventKind::Release {
            return None;
        }
        self.note_key_for_hints();
        let context = (self.current_ui_context(), self.mailbox.mailbox_view);
        if self.input_context != Some(context) {
            self.input.reset();
        }
        self.input_context = Some(context);
        let was_waiting = self.input.is_pending();
        self.input.begin_turn();
        let mut action = self.handle_key_press(key);
        // `q` closes the innermost layer and quits only at the root
        // (blueprint 35, "Back and quit"). Below the root it goes back one
        // level, exactly as Esc does there.
        if action == Some(Action::QuitView) && key.code == KeyCode::Char('q') && self.below_root() {
            let esc = crossterm::event::KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
            action = self.handle_key_press(esc);
        }
        if let Some(keys) = self.input.take_dead_end() {
            self.status_message = Some(format!("{keys} is not a shortcut"));
        }
        self.input.end_turn();
        if !was_waiting {
            self.shorten_bare_prefix();
        }
        action
    }

    /// A layer under the root is open: a screen other than the mailbox, the
    /// reader, or a mode's sub-view. Until an account exists the Accounts
    /// screen is the root, so `q` can still leave first-run setup.
    fn below_root(&self) -> bool {
        if self.accounts.page.onboarding_required {
            return false;
        }
        if self.screen != Screen::Mailbox {
            return true;
        }
        let mailbox = &self.mailbox;
        if mailbox.active_pane == ActivePane::MessageView {
            return true;
        }
        match mailbox.mailbox_view {
            MailboxView::People => mailbox.messages_page.focus == MessagesFocus::Person,
            MailboxView::Todo => mailbox.todo_page.panel != TodoPanel::Runway,
            MailboxView::Reading => {
                let page = &mailbox.reading_page;
                page.later_shelf || (page.reader_focused && page.reader.is_some())
            }
            MailboxView::ArchiveMode => {
                mailbox.records_page.answer.is_some() || mailbox.records_page.card.is_some()
            }
            _ => false,
        }
    }

    fn handle_key_press(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        #[cfg(debug_assertions)]
        if key.code == KeyCode::Char('d')
            && key.modifiers.contains(KeyModifiers::CONTROL)
            && key.modifiers.contains(KeyModifiers::ALT)
        {
            return Some(Action::DumpActionTrace);
        }

        // Ctrl-z suspends the TUI, as in every terminal program (blueprint
        // 35). A global modifier chord, so it wins over text fields too.
        if cfg!(unix) && key.code == KeyCode::Char('z') && key.modifiers == KeyModifiers::CONTROL {
            self.pending_suspend = true;
            return None;
        }

        if self.modals.error.is_some() {
            return match (key.code, key.modifiers) {
                (KeyCode::Char('j') | KeyCode::Down, _) => {
                    if let Some(error) = self.modals.error.as_mut() {
                        error.scroll_offset = error.scroll_offset.saturating_add(1);
                    }
                    None
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    if let Some(error) = self.modals.error.as_mut() {
                        error.scroll_offset = error.scroll_offset.saturating_sub(1);
                    }
                    None
                }
                (KeyCode::Char('d'), KeyModifiers::CONTROL) | (KeyCode::PageDown, _) => {
                    if let Some(error) = self.modals.error.as_mut() {
                        error.scroll_offset = error.scroll_offset.saturating_add(8);
                    }
                    None
                }
                (KeyCode::Char('u'), KeyModifiers::CONTROL) | (KeyCode::PageUp, _) => {
                    if let Some(error) = self.modals.error.as_mut() {
                        error.scroll_offset = error.scroll_offset.saturating_sub(8);
                    }
                    None
                }
                (KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q' | 'x'), _) => {
                    self.modals.error = None;
                    None
                }
                _ => None,
            };
        }

        if self.modals.messages_filter.is_some() {
            self.messages_filter_picker_key(key);
            return None;
        }
        if self.modals.draft_options.visible {
            return self.handle_draft_options_key(key);
        }

        if self.modals.platform.visible {
            return match (key.code, key.modifiers) {
                (KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q' | 'x'), _) => {
                    if self.modals.platform.title == "Accepting writing proof" {
                        return None;
                    }
                    if self.modals.platform.title == "Writing proof" {
                        self.compose.assist_request_id =
                            self.compose.assist_request_id.wrapping_add(1);
                        self.compose.assist_review = None;
                    }
                    self.modals.platform.close();
                    None
                }
                _ => None,
            };
        }

        if self.accounts.page.onboarding_modal_open {
            return match (key.code, key.modifiers) {
                (KeyCode::Enter | KeyCode::Char(' '), _) => {
                    self.complete_account_setup_onboarding();
                    None
                }
                (KeyCode::Char('d'), _) => {
                    self.pick_demo_setup_path();
                    None
                }
                (KeyCode::Char('g'), _) => {
                    self.pick_form_setup_path("Gmail");
                    None
                }
                (KeyCode::Char('i'), _) => {
                    self.pick_form_setup_path("IMAP + SMTP");
                    None
                }
                (KeyCode::Char('q'), _) => Some(Action::QuitView),
                (KeyCode::Esc, _) => {
                    self.accounts.page.onboarding_modal_open = false;
                    None
                }
                _ => None,
            };
        }

        if self.modals.help_open {
            return match (key.code, key.modifiers) {
                (KeyCode::Esc | KeyCode::Enter | KeyCode::Char('?'), _) => Some(Action::Help),
                // q closes where nothing is typed; in a filter it types.
                (KeyCode::Char('q'), _) if self.modals.help_query.is_empty() => Some(Action::Help),
                (KeyCode::Tab, _) => {
                    self.cycle_help_context();
                    None
                }
                (KeyCode::Char('j') | KeyCode::Down, _) => {
                    if self.help_search_active() {
                        self.help_select_next();
                    } else {
                        self.modals.help_scroll_offset =
                            self.modals.help_scroll_offset.saturating_add(1);
                    }
                    None
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    if self.help_search_active() {
                        self.help_select_prev();
                    } else {
                        self.modals.help_scroll_offset =
                            self.modals.help_scroll_offset.saturating_sub(1);
                    }
                    None
                }
                (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                    if self.help_search_active() {
                        self.help_page_down();
                    } else {
                        self.modals.help_scroll_offset =
                            self.modals.help_scroll_offset.saturating_add(8);
                    }
                    None
                }
                (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                    if self.help_search_active() {
                        self.help_page_up();
                    } else {
                        self.modals.help_scroll_offset =
                            self.modals.help_scroll_offset.saturating_sub(8);
                    }
                    None
                }
                (KeyCode::Backspace, _) => {
                    self.pop_help_query();
                    None
                }
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    self.push_help_query(c);
                    None
                }
                _ => None,
            };
        }

        if self.modals.personal_categories.visible {
            self.handle_personal_category_key(key);
            return None;
        }

        if self.modals.snippets.visible {
            return match (key.code, key.modifiers) {
                (KeyCode::Esc | KeyCode::Char('q'), _) => Some(Action::CloseSnippetsModal),
                (KeyCode::Down | KeyCode::Char('j'), _) => Some(Action::SnippetsModalNext),
                (KeyCode::Up | KeyCode::Char('k'), _) => Some(Action::SnippetsModalPrev),
                _ => None,
            };
        }

        if self.modals.summary.visible {
            return match (key.code, key.modifiers) {
                (KeyCode::Esc | KeyCode::Char('q'), _) => Some(Action::CloseSummaryModal),
                _ => None,
            };
        }

        if self.modals.briefing.visible {
            return match (key.code, key.modifiers) {
                (KeyCode::Esc | KeyCode::Char('q'), _) => Some(Action::CloseBriefingModal),
                _ => None,
            };
        }

        if self.modals.whois.visible {
            return match (key.code, key.modifiers) {
                (KeyCode::Esc | KeyCode::Char('q'), _) => Some(Action::CloseWhoisModal),
                _ => None,
            };
        }

        if self.modals.expert.visible {
            return match (key.code, key.modifiers) {
                (KeyCode::Esc | KeyCode::Char('q'), _) => Some(Action::CloseExpertModal),
                _ => None,
            };
        }

        if self.modals.sender_profile.visible {
            return match (key.code, key.modifiers) {
                (KeyCode::Esc | KeyCode::Char('q'), _) => Some(Action::CloseSenderViewModal),
                (KeyCode::Char('1'), _) => {
                    self.modals
                        .sender_profile
                        .select_tab(SenderProfileTab::Overview);
                    None
                }
                (KeyCode::Char('2'), _) => {
                    self.modals
                        .sender_profile
                        .select_tab(SenderProfileTab::Relationship);
                    None
                }
                (KeyCode::Char('3'), _) => {
                    self.modals
                        .sender_profile
                        .select_tab(SenderProfileTab::Messages);
                    None
                }
                (KeyCode::Down | KeyCode::Char('j'), _) => Some(Action::SenderProfileNextMessage),
                (KeyCode::Up | KeyCode::Char('k'), _) => Some(Action::SenderProfilePrevMessage),
                (KeyCode::Enter | KeyCode::Char('o'), _) => Some(Action::OpenSenderProfileMessage),
                // Whois lives on the person page (blueprint 35).
                (KeyCode::Char('W'), modifiers) if plain_or_shift(modifiers) => {
                    Some(Action::OpenWhoisOnFocusedSender)
                }
                _ => None,
            };
        }

        if self.modals.reply_queue.visible {
            return match (key.code, key.modifiers) {
                (KeyCode::Esc | KeyCode::Char('q'), _) => Some(Action::CloseReplyQueueModal),
                (KeyCode::Down | KeyCode::Char('j'), _) => Some(Action::ReplyQueueModalNext),
                (KeyCode::Up | KeyCode::Char('k'), _) => Some(Action::ReplyQueueModalPrev),
                (KeyCode::Enter | KeyCode::Char('r'), _) => Some(Action::ReplyQueueModalReply),
                (KeyCode::Char('F'), modifiers) if plain_or_shift(modifiers) => {
                    Some(Action::ReplyQueueModalFocus)
                }
                _ => None,
            };
        }

        if self.modals.drafts.visible {
            if self.modals.drafts.operation_in_flight {
                return match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => Some(Action::CloseStoredDraftsModal),
                    _ => None,
                };
            }
            if self.modals.drafts.confirmation.is_some() {
                return match self.modals.drafts.choice.key(key.code) {
                    ConfirmKey::Confirm => Some(Action::StoredDraftsModalConfirm),
                    ConfirmKey::Keep => Some(Action::StoredDraftsModalCancelConfirmation),
                    ConfirmKey::Moved | ConfirmKey::Ignored => None,
                };
            }
            return match (key.code, key.modifiers) {
                (KeyCode::Esc | KeyCode::Char('q'), _) => Some(Action::CloseStoredDraftsModal),
                (KeyCode::Down | KeyCode::Char('j'), _) => Some(Action::StoredDraftsModalNext),
                (KeyCode::Up | KeyCode::Char('k'), _) => Some(Action::StoredDraftsModalPrev),
                (KeyCode::Enter, _) => Some(Action::StoredDraftsModalEdit),
                (KeyCode::Char('#'), _) => Some(Action::StoredDraftsModalPreviewDelete),
                // Moved keys (blueprint 35): `#` deletes and Enter edits.
                (KeyCode::Char(c @ ('d' | 'e')), KeyModifiers::NONE) => {
                    self.announce_moved("drafts", &c.to_string());
                    None
                }
                (KeyCode::Char('c'), _) => Some(Action::StoredDraftsModalPreviewCancelSchedule),
                (KeyCode::Char('p'), _) => Some(Action::StoredDraftsModalPreviewPush),
                _ => None,
            };
        }

        if self.modals.screener.visible {
            return match (key.code, key.modifiers) {
                (KeyCode::Esc | KeyCode::Char('q'), _) => Some(Action::CloseScreenerModal),
                (KeyCode::Down | KeyCode::Char('j'), _) => Some(Action::ScreenerModalNext),
                (KeyCode::Up | KeyCode::Char('k'), _) => Some(Action::ScreenerModalPrev),
                (KeyCode::Char('a'), _) => Some(Action::ScreenerDisposeAllow),
                (KeyCode::Char('d'), _) => Some(Action::ScreenerDisposeDeny),
                (KeyCode::Char('f'), _) => Some(Action::ScreenerDisposeFeed),
                (KeyCode::Char('p'), _) => Some(Action::ScreenerDisposePaperTrail),
                _ => None,
            };
        }

        // Saved-search delete confirm, which opens on Keep. Placed before
        // the form check because both can technically be open when a
        // delete is triggered from the sidebar with no form open — but the
        // typical lifecycle is: form closed, delete pending. Either way:
        // confirm wins.
        if self.modals.pending_saved_search_delete_confirm.is_some() {
            match self.modals.saved_search_delete_choice.key(key.code) {
                ConfirmKey::Confirm => {
                    self.confirm_pending_saved_search_delete();
                }
                ConfirmKey::Keep => self.cancel_pending_saved_search_delete(),
                ConfirmKey::Moved | ConfirmKey::Ignored => {}
            }
            return None;
        }

        // Saved-search form: routed before command palette so keys
        // typed into the modal don't trigger global shortcuts.
        if self.modals.saved_search_form.is_some() {
            return self.handle_saved_search_form_key(key);
        }

        if self.command_palette.palette.visible {
            match (key.code, key.modifiers) {
                (KeyCode::Enter, _) => {
                    let command = self.command_palette.palette.confirm_command()?;
                    // Recents changed; flag for async save by the
                    // local-IO worker on the next tick.
                    self.pending_local_state_save = true;
                    self.offer_next_time(&command);
                    return Some(command.action);
                }
                (KeyCode::Esc, _) => return Some(Action::CloseCommandPalette),
                (KeyCode::Backspace, _) => {
                    self.command_palette.palette.on_backspace();
                    return None;
                }
                (KeyCode::Down, _) | (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
                    self.command_palette.palette.select_next();
                    return None;
                }
                (KeyCode::Up, _) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                    self.command_palette.palette.select_prev();
                    return None;
                }
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    self.command_palette.palette.on_char(c);
                    return None;
                }
                _ => return None,
            }
        }

        // Route keys to search bar when active
        if self.search.bar.active {
            match (key.code, key.modifiers) {
                (KeyCode::Enter, _) => return Some(Action::SubmitSearch),
                (KeyCode::Tab, _) => return Some(Action::CycleSearchMode),
                (KeyCode::Esc, _) => return Some(Action::CloseSearch),
                (KeyCode::Backspace, _) => {
                    self.search.bar.on_backspace();
                    // Live filter as you type
                    self.trigger_live_search();
                    return None;
                }
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    self.search.bar.on_char(c);
                    // Live filter as you type
                    self.trigger_live_search();
                    return None;
                }
                _ => return None,
            }
        }

        if self.compose.model_picker.is_some() {
            return self.handle_compose_model_picker_key(key);
        }

        // Route keys to send confirmation prompt
        if self.compose.viewing_voice_source {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => self.compose.viewing_voice_source = false,
                KeyCode::Down | KeyCode::Char('j') => {
                    self.mailbox.message_scroll_offset =
                        self.mailbox.message_scroll_offset.saturating_add(1);
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.mailbox.message_scroll_offset =
                        self.mailbox.message_scroll_offset.saturating_sub(1);
                }
                KeyCode::PageDown => {
                    self.mailbox.message_scroll_offset =
                        self.mailbox.message_scroll_offset.saturating_add(10);
                }
                KeyCode::PageUp => {
                    self.mailbox.message_scroll_offset =
                        self.mailbox.message_scroll_offset.saturating_sub(10);
                }
                _ => {}
            }
            return None;
        }
        if self.compose.assist_review.is_some() {
            return self.handle_compose_assist_key(key);
        }

        if self.compose.pending_send_confirm.is_some() {
            if self.compose.pending_remind_at_input.is_some() {
                match (key.code, key.modifiers) {
                    (KeyCode::Enter, _) => {
                        let input = self
                            .compose
                            .pending_remind_at_input
                            .take()
                            .unwrap_or_default();
                        if let Some(pending) = self.compose.pending_send_confirm.take() {
                            let remind_at = match self.compose.pending_time.chosen() {
                                Ok(remind_at) => remind_at,
                                Err(error) => {
                                    self.status_message =
                                        Some(format!("Cannot parse reminder time: {error}"));
                                    self.compose.pending_send_confirm = Some(pending);
                                    self.compose.pending_remind_at_input = Some(input);
                                    return None;
                                }
                            };
                            self.dispatch_send_pending_with_reminder(
                                pending,
                                None,
                                Some(remind_at),
                            );
                        }
                        return None;
                    }
                    (KeyCode::Esc, _) => {
                        self.compose.pending_remind_at_input = None;
                        return None;
                    }
                    (KeyCode::Tab, _) => {
                        self.compose.pending_time.next_choice();
                        return None;
                    }
                    (KeyCode::Backspace, _) => {
                        if let Some(input) = &mut self.compose.pending_remind_at_input {
                            input.pop();
                            self.compose
                                .pending_time
                                .update(input, &self.modals.snooze_config);
                        }
                        return None;
                    }
                    (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                        if let Some(input) = &mut self.compose.pending_remind_at_input {
                            input.push(c);
                            self.compose
                                .pending_time
                                .update(input, &self.modals.snooze_config);
                        }
                        return None;
                    }
                    _ => return None,
                }
            }
            if self.compose.pending_send_at_input.is_some() {
                match (key.code, key.modifiers) {
                    (KeyCode::Enter, _) => {
                        let input = self
                            .compose
                            .pending_send_at_input
                            .take()
                            .unwrap_or_default();
                        if let Some(pending) = self.compose.pending_send_confirm.take() {
                            let now = chrono::Utc::now();
                            let send_at = match self.compose.pending_time.chosen() {
                                Ok(send_at) => send_at,
                                Err(error) => {
                                    self.status_message =
                                        Some(format!("Cannot parse send-at time: {error}"));
                                    self.compose.pending_send_confirm = Some(pending);
                                    self.compose.pending_send_at_input = Some(input);
                                    return None;
                                }
                            };
                            let draft_id = pending.draft_id();
                            let draft = match draft_from_pending(&pending, draft_id.clone(), now) {
                                Ok(draft) => draft,
                                Err(error) => {
                                    self.compose.pending_send_confirm = Some(pending);
                                    self.compose.pending_send_at_input = Some(input);
                                    self.push_toast(Toast::error(error.to_string()));
                                    return None;
                                }
                            };
                            self.queue_platform_request_after(
                                vec![Request::SaveDraft { draft }],
                                Request::ScheduleSend {
                                    managed_attachments_reviewed: true,
                                    draft_id,
                                    send_at,
                                    expected_revision: pending.draft_revision.or(Some(1)),
                                },
                                "Scheduled send",
                                "Saving draft and scheduling send...",
                            );
                            if let Some(dispatch) = self.modals.pending_platform_dispatch.last_mut()
                            {
                                self.compose
                                    .pending_schedule_files
                                    .insert(pending.draft_path.clone());
                                dispatch.compose = Some(Box::new(pending));
                            }
                        }
                        return None;
                    }
                    (KeyCode::Esc, _) => {
                        self.compose.pending_send_at_input = None;
                        return None;
                    }
                    (KeyCode::Tab, _) => {
                        self.compose.pending_time.next_choice();
                        return None;
                    }
                    (KeyCode::Backspace, _) => {
                        if let Some(input) = &mut self.compose.pending_send_at_input {
                            input.pop();
                            self.compose
                                .pending_time
                                .update(input, &self.modals.snooze_config);
                        }
                        return None;
                    }
                    (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                        if let Some(input) = &mut self.compose.pending_send_at_input {
                            input.push(c);
                            self.compose
                                .pending_time
                                .update(input, &self.modals.snooze_config);
                        }
                        return None;
                    }
                    _ => return None,
                }
            }
            match (key.code, key.modifiers) {
                // Enter runs the lit option (blueprint 35's overlay grammar).
                (KeyCode::Enter, _) => {
                    let send = self
                        .compose
                        .pending_send_confirm
                        .as_ref()
                        .is_some_and(PendingSend::opens_on_send);
                    let lit = if send { 's' } else { 'e' };
                    return self.handle_key_press(crossterm::event::KeyEvent::new(
                        KeyCode::Char(lit),
                        KeyModifiers::NONE,
                    ));
                }
                (KeyCode::Char('s'), KeyModifiers::NONE) => {
                    // Send. Suppressed when the safety verdict is
                    // Blocked — the user must use Ctrl-O (with the
                    // override token) or edit the draft to proceed.
                    if let Some(pending) = self.compose.pending_send_confirm.take() {
                        self.compose.pending_send_at_input = None;
                        self.compose.pending_remind_at_input = None;
                        if pending.mode != PendingSendMode::SendOrSave || pending.is_blocked() {
                            self.compose.pending_send_confirm = Some(pending);
                            return None;
                        }
                        self.dispatch_send_pending(pending, None);
                    }
                    return None;
                }
                (KeyCode::Char('a'), KeyModifiers::CONTROL) => {
                    // Slice 5.3 (C2.7 cont): add the top "maybe
                    // include" suggestion to the draft's Cc and
                    // remove it from the suggestions list.
                    if let Some(pending) = self.compose.pending_send_confirm.as_mut() {
                        if let Some(top) = pending.suggested_collaborators.first().cloned() {
                            let cc = pending.fm.cc.trim();
                            pending.fm.cc = if cc.is_empty() {
                                top.email.clone()
                            } else {
                                format!("{cc}, {}", top.email)
                            };
                            pending.suggested_collaborators.remove(0);
                            self.status_message = Some(format!("Added {} to Cc", top.email));
                        }
                    }
                    return None;
                }
                // `O` is the advertised override, matching the web app. Ctrl-O
                // stays as a silent alias for existing muscle memory.
                (KeyCode::Char('o'), KeyModifiers::CONTROL)
                | (KeyCode::Char('O'), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    // Override + send. Consumes the single-use token
                    // the daemon minted on the prior CheckDraftSafety.
                    // Only meaningful when the verdict is Blocked.
                    if let Some(pending) = self.compose.pending_send_confirm.take() {
                        if !pending.is_blocked() || pending.override_token.is_none() {
                            self.compose.pending_send_confirm = Some(pending);
                            return None;
                        }
                        let token = pending.override_token.clone();
                        self.compose.pending_send_at_input = None;
                        self.compose.pending_remind_at_input = None;
                        self.dispatch_send_pending(pending, token);
                    }
                    return None;
                }
                (KeyCode::Char('a'), KeyModifiers::NONE) => {
                    if let Some(pending) = self.compose.pending_send_confirm.as_ref() {
                        if pending.mode == PendingSendMode::SendOrSave {
                            self.compose.pending_send_at_input = Some(String::new());
                            self.compose.pending_time.clear();
                        }
                    }
                    return None;
                }
                (KeyCode::Char('n'), KeyModifiers::NONE) => {
                    if let Some(pending) = self.compose.pending_send_confirm.as_ref() {
                        if pending.mode == PendingSendMode::SendOrSave && !pending.is_blocked() {
                            self.compose.pending_remind_at_input = Some(String::new());
                            self.compose.pending_time.clear();
                        }
                    }
                    return None;
                }
                (KeyCode::Char('d'), KeyModifiers::NONE) => {
                    // Save as draft to mail server
                    if let Some(pending) = self.compose.pending_send_confirm.take() {
                        self.compose.pending_send_at_input = None;
                        self.compose.pending_remind_at_input = None;
                        if pending.mode == PendingSendMode::Unchanged {
                            self.compose.pending_send_confirm = Some(pending);
                            return None;
                        }
                        let retained = pending.clone();
                        let parse_addrs = |s: &str| mxr_mail_parse::parse_address_list(s);
                        let reply_headers = pending.fm.in_reply_to.as_ref().map(|in_reply_to| {
                            mxr_core::types::ReplyHeaders {
                                in_reply_to: in_reply_to.clone(),
                                references: pending.fm.references.clone(),
                                thread_id: pending.fm.thread_id.clone(),
                            }
                        });
                        let now = chrono::Utc::now();
                        let mut draft = mxr_core::Draft {
                            revision: pending.draft_revision.or(Some(1)),
                            id: pending.draft_id(),
                            account_id: pending.account_id.clone(),
                            from: mxr_compose::draft_codec::parse_from_field(&pending.fm.from)
                                .ok()
                                .flatten(),
                            reply_headers,
                            intent: pending.intent,
                            to: parse_addrs(&pending.fm.to),
                            cc: parse_addrs(&pending.fm.cc),
                            bcc: parse_addrs(&pending.fm.bcc),
                            subject: pending.fm.subject.clone(),
                            content: mxr_core::DraftContent::markdown(pending.body.clone()),
                            managed_attachments: None,
                            attachments: pending
                                .fm
                                .attach
                                .iter()
                                .map(std::path::PathBuf::from)
                                .collect(),
                            inline_assets: Vec::new(),
                            inline_calendar_reply: pending.invite_reply.clone(),
                            created_at: now,
                            updated_at: now,
                        };
                        if let Err(error) = mxr_compose::forward::apply(&mut draft, &pending.fm) {
                            self.compose.pending_send_confirm = Some(retained);
                            self.push_toast(Toast::error(error.to_string()));
                            return None;
                        }
                        let id = self.queue_mutation(
                            Request::SaveDraftToServer { draft },
                            MutationEffect::StatusOnly("Draft saved to server".into()),
                            "Saving draft...".into(),
                        );
                        self.compose
                            .pending_send_mutations
                            .insert(id, (retained, false));
                    }
                    return None;
                }
                // Edit again: reopen the editor on the same file. `Esc` goes
                // back to the draft the same way, because it must never
                // destroy (blueprint 35, "Back and quit"); the file holds the
                // only copy, so closing without reopening would strand it.
                (KeyCode::Char('e'), KeyModifiers::NONE) | (KeyCode::Esc, _) => {
                    if let Some(pending) = self.compose.pending_send_confirm.take() {
                        self.compose.pending_send_at_input = None;
                        self.compose.pending_remind_at_input = None;
                        self.compose.pending_compose = Some(ComposeAction::EditDraft {
                            path: pending.draft_path,
                            account_id: pending.account_id,
                        });
                    }
                    return None;
                }
                (KeyCode::Char('r'), KeyModifiers::NONE) => {
                    return Some(Action::RefinePendingDraft);
                }
                // Discard only on the option that names it, with the shared
                // destroy key (blueprint 36's "# discard draft").
                (KeyCode::Char('#'), _) => {
                    if let Some(pending) = self.compose.pending_send_confirm.take() {
                        self.compose.pending_send_at_input = None;
                        self.compose.pending_remind_at_input = None;
                        self.schedule_draft_cleanup(pending.draft_path);
                        self.status_message = Some("Discarded".into());
                    }
                    return None;
                }
                _ => return None,
            }
        }

        if let Some(confirm) = self.mailbox.sweep_confirm.as_mut() {
            return match (key.code, key.modifiers) {
                // y is Enter (blueprint 35): both run the highlighted option,
                // which a whole place's sweep starts on Cancel.
                (KeyCode::Enter, _) | (KeyCode::Char('y'), KeyModifiers::NONE) => {
                    self.confirm_sweep();
                    None
                }
                (KeyCode::Tab | KeyCode::BackTab, _) => {
                    confirm.sweep_focused = !confirm.sweep_focused;
                    None
                }
                (KeyCode::Esc, _) | (KeyCode::Char('n' | 'q'), KeyModifiers::NONE) => {
                    self.mailbox.sweep_confirm = None;
                    self.status_message = Some("Sweep cancelled".into());
                    None
                }
                _ => None,
            };
        }

        // Moves (D119): the menu `X` and `K` open, "Always for this
        // sender? y/n" after a Not-sure answer, and the arrivals list.
        if self.mailbox.trust.move_menu.is_some() {
            return self.move_menu_key(key);
        }
        if let Some(action) = self.sender_ask_key(key) {
            return Some(action);
        }
        if self.mailbox.trust.arrivals_list.is_some() && self.screen == Screen::Mailbox {
            return self.arrivals_list_key(key);
        }

        if let Some(menu) = self.mailbox.sender_kind_menu.as_mut() {
            let choices = crate::ui::place_lens::SENDER_KIND_CHOICES;
            return match (key.code, key.modifiers) {
                (KeyCode::Esc, _) | (KeyCode::Char('q'), KeyModifiers::NONE) => {
                    self.mailbox.sender_kind_menu = None;
                    None
                }
                // A picker moves with j, k and the arrows, and Enter runs
                // the lit choice (blueprint 35's overlay grammar).
                (KeyCode::Char('j') | KeyCode::Down, KeyModifiers::NONE) => {
                    menu.selected = (menu.selected + 1).min(choices.len() - 1);
                    None
                }
                (KeyCode::Char('k') | KeyCode::Up, KeyModifiers::NONE) => {
                    menu.selected = menu.selected.saturating_sub(1);
                    None
                }
                (KeyCode::Enter, _) => choices
                    .get(menu.selected)
                    .map(|(_, kind, _)| Action::SetSenderKind(*kind)),
                // A sender's menu: either case picks.
                (KeyCode::Char(c), modifiers) if plain_or_shift(modifiers) => {
                    let c = c.to_ascii_lowercase();
                    crate::ui::place_lens::SENDER_KIND_CHOICES
                        .iter()
                        .find(|(choice, _, _)| *choice == c)
                        .map(|(_, kind, _)| Action::SetSenderKind(*kind))
                }
                _ => None,
            };
        }

        if self.modals.pending_bulk_confirm.is_some() {
            return match (key.code, key.modifiers) {
                (KeyCode::Enter, _) => Some(Action::OpenSelected),
                (KeyCode::Char('y'), KeyModifiers::NONE) => Some(Action::OpenSelected),
                (KeyCode::Char('Y'), modifiers) if plain_or_shift(modifiers) => {
                    Some(Action::OpenSelected)
                }
                (KeyCode::Esc, _) | (KeyCode::Char('n' | 'q'), KeyModifiers::NONE) => {
                    self.modals.pending_bulk_confirm = None;
                    self.status_message = Some("Bulk action cancelled".into());
                    None
                }
                (KeyCode::Char('j') | KeyCode::Down, _) => {
                    if let Some(pending) = self.modals.pending_bulk_confirm.as_mut() {
                        pending.preview_scroll = pending.preview_scroll.saturating_add(1);
                    }
                    None
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    if let Some(pending) = self.modals.pending_bulk_confirm.as_mut() {
                        pending.preview_scroll = pending.preview_scroll.saturating_sub(1);
                    }
                    None
                }
                _ => None,
            };
        }

        if self.modals.pending_unsubscribe_confirm.is_some() {
            return match (key.code, key.modifiers) {
                (KeyCode::Enter, _) => Some(Action::ConfirmUnsubscribeOnly),
                (KeyCode::Char('u'), KeyModifiers::NONE) => Some(Action::ConfirmUnsubscribeOnly),
                (KeyCode::Char('U'), modifiers) if plain_or_shift(modifiers) => {
                    Some(Action::ConfirmUnsubscribeOnly)
                }
                // `e` is Archive's letter (blueprint 35's Pick rule).
                (KeyCode::Char('e'), KeyModifiers::NONE) => {
                    Some(Action::ConfirmUnsubscribeAndArchiveSender)
                }
                (KeyCode::Char('E'), modifiers) if plain_or_shift(modifiers) => {
                    Some(Action::ConfirmUnsubscribeAndArchiveSender)
                }
                (KeyCode::Char(c @ ('a' | 'A')), modifiers) if plain_or_shift(modifiers) => {
                    self.announce_moved("unsubscribe", &c.to_string());
                    None
                }
                // A yes or no question: n is Esc, and q closes too.
                (KeyCode::Esc | KeyCode::Char('n' | 'q'), _) => Some(Action::CancelUnsubscribe),
                _ => None,
            };
        }

        if self.modals.reply_later_prompt.is_some() {
            return self.reply_later_prompt_key(key.code, key.modifiers);
        }

        if self.mailbox.todo_page.prompt.is_some() {
            self.todo_prompt_key(key.code, key.modifiers);
            return None;
        }

        if self.mailbox.records_page.prompt.is_some() {
            self.records_prompt_key(key.code, key.modifiers);
            return None;
        }

        if self.mailbox.records_page.pass_menu.is_some() {
            self.pass_menu_key(key.code);
            return None;
        }

        if self.modals.snooze_panel.visible {
            // Custom-input mode: text-entry takes precedence over list navigation.
            if self.modals.snooze_panel.custom_input.is_some() {
                match (key.code, key.modifiers) {
                    (KeyCode::Enter, _) => return Some(Action::Snooze),
                    (KeyCode::Esc, _) => {
                        // Back to preset list — preserves the panel.
                        self.modals.snooze_panel.custom_input = None;
                        self.modals.snooze_panel.custom_error = None;
                        return None;
                    }
                    (KeyCode::Tab, _) => {
                        self.modals.snooze_panel.custom_time.next_choice();
                        return None;
                    }
                    (KeyCode::Backspace, _) => {
                        if let Some(buffer) = self.modals.snooze_panel.custom_input.as_mut() {
                            buffer.pop();
                            self.modals.snooze_panel.custom_error = None;
                            self.modals
                                .snooze_panel
                                .custom_time
                                .update(buffer, &self.modals.snooze_config);
                        }
                        return None;
                    }
                    (KeyCode::Char(c), modifiers)
                        if !modifiers.contains(KeyModifiers::CONTROL)
                            && !modifiers.contains(KeyModifiers::ALT) =>
                    {
                        if let Some(buffer) = self.modals.snooze_panel.custom_input.as_mut() {
                            buffer.push(c);
                            self.modals.snooze_panel.custom_error = None;
                            self.modals
                                .snooze_panel
                                .custom_time
                                .update(buffer, &self.modals.snooze_config);
                        }
                        return None;
                    }
                    _ => return None,
                }
            }
            let total = crate::ui::snooze_modal::rows_count(&self.modals.snooze_panel);
            match (key.code, key.modifiers) {
                (KeyCode::Enter, _) => return Some(Action::Snooze),
                (KeyCode::Esc | KeyCode::Char('q'), _) => {
                    self.close_not_now();
                    return None;
                }
                // A digit picks the Nth time at once (D166).
                (KeyCode::Char(digit @ '1'..='9'), KeyModifiers::NONE) => {
                    self.not_now_preset(digit as usize - '0' as usize);
                    return None;
                }
                // Ctrl-n and Ctrl-p move too, as in the web's Not now, whose
                // letters type a time.
                (KeyCode::Char('n'), KeyModifiers::CONTROL)
                | (KeyCode::Char('j') | KeyCode::Down, _) => {
                    self.modals.snooze_panel.selected_index =
                        (self.modals.snooze_panel.selected_index + 1) % total;
                    return None;
                }
                (KeyCode::Char('p'), KeyModifiers::CONTROL)
                | (KeyCode::Char('k') | KeyCode::Up, _) => {
                    self.modals.snooze_panel.selected_index = self
                        .modals
                        .snooze_panel
                        .selected_index
                        .checked_sub(1)
                        .unwrap_or(total - 1);
                    return None;
                }
                _ => return None,
            }
        }

        // Route keys to URL modal when active
        if let Some(ref mut url_state) = self.mailbox.url_modal {
            match (key.code, key.modifiers) {
                (KeyCode::Enter | KeyCode::Char('o'), _) => {
                    if let Some(url) = url_state
                        .selected_url()
                        .map(std::string::ToString::to_string)
                    {
                        ui::url_modal::open_url(&url);
                        self.status_message = Some(format!("Opening {url}"));
                    }
                    self.mailbox.url_modal = None;
                    return None;
                }
                (KeyCode::Char('y'), _) => {
                    if let Some(url) = url_state
                        .selected_url()
                        .map(std::string::ToString::to_string)
                    {
                        // Copy to clipboard via pbcopy (macOS) or xclip (Linux);
                        // tests press `y` without touching the real clipboard.
                        #[cfg(all(target_os = "macos", not(test)))]
                        {
                            use std::io::Write;
                            if let Ok(mut child) = std::process::Command::new("pbcopy")
                                .stdin(std::process::Stdio::piped())
                                .spawn()
                            {
                                if let Some(mut stdin) = child.stdin.take() {
                                    let _ = stdin.write_all(url.as_bytes());
                                }
                                let _ = child.wait();
                            }
                        }
                        #[cfg(all(target_os = "linux", not(test)))]
                        {
                            use std::io::Write;
                            if let Ok(mut child) = std::process::Command::new("xclip")
                                .args(["-selection", "clipboard"])
                                .stdin(std::process::Stdio::piped())
                                .spawn()
                            {
                                if let Some(mut stdin) = child.stdin.take() {
                                    let _ = stdin.write_all(url.as_bytes());
                                }
                                let _ = child.wait();
                            }
                        }
                        self.status_message = Some(format!("Copied: {url}"));
                    }
                    self.mailbox.url_modal = None;
                    return None;
                }
                (KeyCode::Char('j') | KeyCode::Down, _) => {
                    url_state.next();
                    return None;
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    url_state.prev();
                    return None;
                }
                (KeyCode::Esc | KeyCode::Char('q'), _) => {
                    self.mailbox.url_modal = None;
                    return None;
                }
                _ => return None,
            }
        }

        // The export picker takes every key while it is open. Keys are read
        // per stage, so `c`, `s` and the format letters only mean something
        // where they're offered.
        if self.modals.export.visible {
            match self.modals.export.stage {
                crate::app::ExportStage::Format => match (key.code, key.modifiers) {
                    (KeyCode::Esc, _) => self.modals.export.close(),
                    (KeyCode::Up, _) => self.export_modal_move(false),
                    (KeyCode::Down, _) => self.export_modal_move(true),
                    (KeyCode::Enter, _) => {
                        let format = crate::export_output::FORMAT_CHOICES
                            .get(self.modals.export.selected)
                            .cloned();
                        if let Some(format) = format {
                            self.export_modal_choose_format(format);
                        }
                    }
                    (KeyCode::Char(c), m) if m.is_empty() => {
                        let format = crate::export_output::FORMAT_CHOICES
                            .iter()
                            .find(|format| crate::export_output::format_key(format) == c)
                            .cloned();
                        if let Some(format) = format {
                            self.export_modal_choose_format(format);
                        }
                    }
                    _ => {}
                },
                crate::app::ExportStage::Destination => match (key.code, key.modifiers) {
                    (KeyCode::Esc, _) => self.modals.export.close(),
                    (KeyCode::Char('c'), m) if m.is_empty() => self.export_modal_choose_clipboard(),
                    (KeyCode::Char('s'), m) if m.is_empty() => self.export_modal_choose_save(),
                    _ => {}
                },
                crate::app::ExportStage::Path => match (key.code, key.modifiers) {
                    (KeyCode::Esc, _) => self.modals.export.close(),
                    (KeyCode::Enter, _) => self.export_modal_confirm_path(),
                    (KeyCode::Backspace, _) => self.export_modal_backspace(),
                    (KeyCode::Char(c), m) if !m.contains(KeyModifiers::CONTROL) => {
                        self.export_modal_push_char(c);
                    }
                    _ => {}
                },
            }
            return None;
        }

        // Route keys to the save-attachment modal when it's on top of
        // the attachment panel. This block must run before the
        // attachment-panel handler below since the modal layers on top.
        if self.modals.save_attachment.visible {
            match (key.code, key.modifiers) {
                (KeyCode::Esc, _) => {
                    self.modals.save_attachment.close();
                    return None;
                }
                (KeyCode::Enter, _) => {
                    self.save_attachment_confirm();
                    return None;
                }
                (KeyCode::Char('1'), m) if m.is_empty() => {
                    self.save_attachment_apply_preset(
                        crate::app::attachment_helpers::SavePathPreset::Downloads,
                    );
                    return None;
                }
                (KeyCode::Char('2'), m) if m.is_empty() => {
                    self.save_attachment_apply_preset(
                        crate::app::attachment_helpers::SavePathPreset::Desktop,
                    );
                    return None;
                }
                (KeyCode::Char('3'), m) if m.is_empty() => {
                    self.save_attachment_apply_preset(
                        crate::app::attachment_helpers::SavePathPreset::Cwd,
                    );
                    return None;
                }
                (KeyCode::Backspace, _) => {
                    self.modals.save_attachment.input.pop();
                    self.modals.save_attachment.error = None;
                    self.modals.save_attachment.awaiting_overwrite_confirm = false;
                    return None;
                }
                (KeyCode::Char(c), m)
                    if !m.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    self.modals.save_attachment.input.push(c);
                    self.modals.save_attachment.error = None;
                    self.modals.save_attachment.awaiting_overwrite_confirm = false;
                    return None;
                }
                _ => return None,
            }
        }

        // Route keys to compose picker when active
        if self.mailbox.attachment_panel.visible {
            match (key.code, key.modifiers) {
                (KeyCode::Enter | KeyCode::Char('o'), _) => {
                    self.queue_attachment_action(AttachmentOperation::Open);
                    return None;
                }
                (KeyCode::Char('d'), _) => {
                    self.open_save_attachment_modal();
                    return None;
                }
                (KeyCode::Char('j') | KeyCode::Down, _) => {
                    if self.mailbox.attachment_panel.selected_index + 1
                        < self.mailbox.attachment_panel.attachments.len()
                    {
                        self.mailbox.attachment_panel.selected_index += 1;
                    }
                    return None;
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    self.mailbox.attachment_panel.selected_index = self
                        .mailbox
                        .attachment_panel
                        .selected_index
                        .saturating_sub(1);
                    return None;
                }
                (KeyCode::Esc | KeyCode::Char('A' | 'q'), _) => {
                    self.close_attachment_panel();
                    return None;
                }
                _ => return None,
            }
        }

        // Route keys to compose picker when active
        if self.compose.compose_picker.visible {
            match (key.code, key.modifiers) {
                (KeyCode::Enter, _) => {
                    if self.compose.compose_picker.mode
                        == crate::ui::compose_picker::ComposePickerMode::To
                    {
                        self.compose.compose_picker.open_subject();
                    } else {
                        let (to, subject) = self.compose.compose_picker.confirm_subject();
                        self.compose.pending_compose = Some(ComposeAction::New { to, subject });
                    }
                    return None;
                }
                (KeyCode::Tab, _) => {
                    // Tab adds selected contact to recipients
                    self.compose.compose_picker.add_recipient();
                    return None;
                }
                (KeyCode::Esc, _) => {
                    self.compose.compose_picker.close();
                    return None;
                }
                (KeyCode::Backspace, _) => {
                    self.compose.compose_picker.on_backspace();
                    return None;
                }
                (KeyCode::Down, _) | (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
                    self.compose.compose_picker.select_next();
                    return None;
                }
                (KeyCode::Up, _) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                    self.compose.compose_picker.select_prev();
                    return None;
                }
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    self.compose.compose_picker.on_char(c);
                    return None;
                }
                _ => return None,
            }
        }

        // Route keys to label picker when active
        if self.modals.label_picker.visible {
            match (key.code, key.modifiers) {
                (KeyCode::Enter, _) => {
                    let mode = self.modals.label_picker.mode;
                    if let Some(label_name) = self.modals.label_picker.confirm() {
                        self.modals.pending_label_action = Some((mode, label_name));
                        return match mode {
                            LabelPickerMode::Apply => Some(Action::ApplyLabel),
                            LabelPickerMode::Move => Some(Action::MoveToLabel),
                            LabelPickerMode::Route => Some(Action::RouteToLabel),
                        };
                    }
                    return None;
                }
                (KeyCode::Esc, _) => {
                    self.modals.label_picker.close();
                    return None;
                }
                (KeyCode::Backspace, _) => {
                    self.modals.label_picker.on_backspace();
                    return None;
                }
                (KeyCode::Down, _) | (KeyCode::Char('n'), KeyModifiers::CONTROL) => {
                    self.modals.label_picker.select_next();
                    return None;
                }
                (KeyCode::Up, _) | (KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                    self.modals.label_picker.select_prev();
                    return None;
                }
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    self.modals.label_picker.on_char(c);
                    return None;
                }
                _ => return None,
            }
        }

        // The assistant pane, once no modal above it wants the key: every
        // key while the pane has focus.
        if let Some(action) = self.assistant_key(key) {
            return action;
        }

        // A promise prompt answers to y / n / Esc once no modal or prompt
        // above it wants the key; every other key works as usual.
        if !self.promise_prompts.is_empty() {
            let answer = match (key.code, key.modifiers) {
                (KeyCode::Char('y'), KeyModifiers::NONE) => Some(true),
                (KeyCode::Char('n'), KeyModifiers::NONE) | (KeyCode::Esc, _) => Some(false),
                _ => None,
            };
            if let Some(keep) = answer {
                // A held key must not answer the next prompt too.
                if key.kind != crossterm::event::KeyEventKind::Repeat {
                    self.answer_promise(keep, std::time::Instant::now());
                }
                return None;
            }
        }

        // Digits count rows in visual line mode, before they can mean tabs.
        if let Some(action) = self.visual_count_key(key) {
            return action;
        }

        // With single-character shortcuts off (WCAG 2.1.4), a character
        // typed on its own does nothing outside a text field. The overlays
        // above keep their letters: they act only while they are open.
        if !self.keybindings.single_keys
            && matches!(key.code, KeyCode::Char(c) if c != ' ')
            && plain_or_shift(key.modifiers)
            && !self.text_input_focused()
        {
            self.input.reset();
            self.status_message =
                Some("Single-key shortcuts are off; Ctrl-k opens the palette".into());
            return None;
        }

        // A key that moved away from the list, reader or sidebar says where
        // it went (the modes do this in `defined_key`).
        if self.announce_moved_key(key) {
            return None;
        }

        // Remaps are authoritative only in mail panes, after all modal/text
        // owners. Explicit keys also beat lens-specific single-key defaults.
        // A mode takes its remaps from its own keys.toml section, so
        // `[mail_list]` stays out of it.
        if self.mode_key_context().is_none()
            && matches!(
                self.current_ui_context(),
                UiContext::MailboxList
                    | UiContext::MailboxMessage
                    | UiContext::SearchResults
                    | UiContext::SearchPreview
            )
        {
            if let Some(action) = self.configured_mail_key(key) {
                return action;
            }
        }

        // The second key of a chord (`g r`, `g a`, `i a`) completes it,
        // before any single-key binding such as `r` reply can claim it.
        // A mode completes its own sequences from the key definition.
        if self.input.is_pending() && self.mode_key_context().is_none() {
            let action = self.contextual_input_action(key);
            return if self.screen == Screen::Mailbox
                && self.mailbox.active_pane == ActivePane::Sidebar
            {
                self.sidebar_ends(action)
            } else {
                action
            };
        }

        if self.screen != Screen::Mailbox {
            return self.handle_screen_key(key);
        }

        // Route keys based on active pane
        match self.mailbox.active_pane {
            ActivePane::MessageView => match (key.code, key.modifiers) {
                (KeyCode::Char('/'), KeyModifiers::NONE) => Some(Action::OpenGlobalSearch),
                (KeyCode::Char('f'), KeyModifiers::CONTROL) => Some(Action::OpenMailboxFilter),
                // The palette's Ctrl-k, before `k` scrolls (it ignores modifiers).
                (KeyCode::Char('k'), KeyModifiers::CONTROL) => Some(Action::OpenCommandPalette),
                (KeyCode::Char('j') | KeyCode::Down, _) => {
                    self.move_message_view_down();
                    None
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    self.move_message_view_up();
                    None
                }
                // J/K move focus between messages in a thread. This used to
                // live on j/k, which left a long body unscrollable whenever
                // the thread held more than one message.
                (KeyCode::Char('J'), modifiers) if plain_or_shift(modifiers) => {
                    self.move_thread_focus_down();
                    None
                }
                (KeyCode::Char('K'), modifiers) if plain_or_shift(modifiers) => {
                    self.move_thread_focus_up();
                    None
                }
                (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                    self.mailbox.message_scroll_offset =
                        self.mailbox.message_scroll_offset.saturating_add(20);
                    None
                }
                (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                    self.mailbox.message_scroll_offset =
                        self.mailbox.message_scroll_offset.saturating_sub(20);
                    None
                }
                (KeyCode::Char('G'), modifiers) if plain_or_shift(modifiers) => {
                    self.mailbox.message_scroll_offset = u16::MAX;
                    None
                }
                // The keyboard's motion keys and Space page the body, as in
                // the web reader; Home reaches `g g` below.
                (KeyCode::End, KeyModifiers::NONE) => {
                    self.mailbox.message_scroll_offset = u16::MAX;
                    None
                }
                (KeyCode::PageDown | KeyCode::Char(' '), KeyModifiers::NONE) => {
                    self.mailbox.message_scroll_offset =
                        self.mailbox.message_scroll_offset.saturating_add(20);
                    None
                }
                (KeyCode::PageUp, KeyModifiers::NONE) => {
                    self.mailbox.message_scroll_offset =
                        self.mailbox.message_scroll_offset.saturating_sub(20);
                    None
                }
                (KeyCode::Char('H'), modifiers) if plain_or_shift(modifiers) => {
                    Some(Action::ToggleHtmlView)
                }
                (KeyCode::Char('M'), modifiers) if plain_or_shift(modifiers) => {
                    Some(Action::ToggleRemoteContent)
                }
                // h = move left to mail list
                (KeyCode::Char('h') | KeyCode::Left, KeyModifiers::NONE) => {
                    if self.mailbox.layout_mode != LayoutMode::FullScreen {
                        self.mailbox.active_pane = ActivePane::MailList;
                    }
                    None
                }
                // L = open links picker
                (KeyCode::Char('L'), modifiers) if plain_or_shift(modifiers) => {
                    Some(Action::OpenLinks)
                }
                // X: move this email to another mode. K stays the
                // previous message here.
                (KeyCode::Char('X'), modifiers) if plain_or_shift(modifiers) => {
                    Some(Action::OpenMoveMenu)
                }
                // Enter only reopened the conversation already open here, and
                // the web reader leaves Enter to its buttons (blueprint 35's
                // same keys in both clients).
                (KeyCode::Enter, _) => None,
                _ if self.mail_action_key(key).is_some() => self.mail_action_key(key),
                _ => self.contextual_input_action(key),
            },
            ActivePane::Sidebar => match (key.code, key.modifiers) {
                (KeyCode::Char('/'), KeyModifiers::NONE) => Some(Action::OpenGlobalSearch),
                (KeyCode::Char('f'), KeyModifiers::CONTROL) => Some(Action::OpenMailboxFilter),
                // The palette's Ctrl-k, before `k` moves (it ignores modifiers).
                (KeyCode::Char('k'), KeyModifiers::CONTROL) => Some(Action::OpenCommandPalette),
                (KeyCode::Char('j') | KeyCode::Down, _) => {
                    self.sidebar_move_down();
                    None
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    self.sidebar_move_up();
                    None
                }
                // { and } jump between sections; Enter on a section's
                // header folds or unfolds it (blueprint 35).
                (KeyCode::Char('{'), _) => {
                    self.sidebar_jump_section(false);
                    None
                }
                (KeyCode::Char('}'), _) => {
                    self.sidebar_jump_section(true);
                    None
                }
                // c creates what this place holds: a saved search.
                (KeyCode::Char('c'), KeyModifiers::NONE) => Some(Action::OpenSavedSearchFormNew),
                // e/# only mean something on a Saved Searches row: a label
                // can't be edited or deleted from here. Elsewhere they fall
                // through.
                (KeyCode::Char('e'), KeyModifiers::NONE)
                    if self.current_sidebar_group() == SidebarGroup::SavedSearches =>
                {
                    Some(Action::OpenSavedSearchFormEdit)
                }
                (KeyCode::Char('#'), _)
                    if self.current_sidebar_group() == SidebarGroup::SavedSearches =>
                {
                    Some(Action::DeleteSavedSearch)
                }
                (KeyCode::Char('d'), KeyModifiers::NONE)
                    if self.current_sidebar_group() == SidebarGroup::SavedSearches =>
                {
                    self.announce_moved("saved-searches", "d");
                    None
                }
                (KeyCode::Enter | KeyCode::Char('o'), _) => self.sidebar_select(),
                // l = select label and move to mail list
                (KeyCode::Char('l') | KeyCode::Right, KeyModifiers::NONE) => self.sidebar_select(),
                (KeyCode::Char('u'), KeyModifiers::NONE) => Some(Action::UndoLastMutation),
                _ => {
                    let action = self.contextual_input_action(key);
                    self.sidebar_ends(action)
                }
            },
            ActivePane::MailList if self.mailbox.mailbox_view == MailboxView::CalendarInvites => {
                self.calendar_invites_key(key)
            }
            ActivePane::MailList if matches!(self.mailbox.mailbox_view, MailboxView::Place(_)) => {
                self.place_lens_key(key)
            }
            ActivePane::MailList if self.mailbox.mailbox_view == MailboxView::Todo => {
                self.todo_lens_key(key)
            }
            ActivePane::MailList if self.mailbox.mailbox_view == MailboxView::Now => {
                self.now_lens_key(key)
            }
            ActivePane::MailList if self.mailbox.mailbox_view == MailboxView::People => {
                self.messages_lens_key(key)
            }
            ActivePane::MailList if self.mailbox.mailbox_view == MailboxView::ArchiveMode => {
                self.records_lens_key(key)
            }
            ActivePane::MailList if self.mailbox.mailbox_view == MailboxView::Reading => {
                self.reading_lens_key(key)
            }
            ActivePane::MailList if self.mailbox.mailbox_view == MailboxView::Updates => {
                self.updates_lens_key(key)
            }
            ActivePane::MailList => match (key.code, key.modifiers) {
                (KeyCode::Char('/'), KeyModifiers::NONE) => Some(Action::OpenGlobalSearch),
                (KeyCode::Char('f'), KeyModifiers::CONTROL) => Some(Action::OpenMailboxFilter),
                // h = move left to sidebar
                (KeyCode::Char('h') | KeyCode::Left, KeyModifiers::NONE) => {
                    self.mailbox.active_pane = ActivePane::Sidebar;
                    None
                }
                // Right arrow opens selected message
                (KeyCode::Right, KeyModifiers::NONE) => Some(Action::OpenSelected),
                // X moves this email to another mode; K sets its sender's.
                (KeyCode::Char('X'), modifiers) if plain_or_shift(modifiers) => {
                    Some(Action::OpenMoveMenu)
                }
                // P: this sender, always (blueprint 35's Shift on p).
                (KeyCode::Char('P'), modifiers) if plain_or_shift(modifiers) => {
                    Some(Action::OpenSenderMoveMenu)
                }
                _ if self.mail_action_key(key).is_some() => self.mail_action_key(key),
                _ => self.contextual_input_action(key),
            },
        }
    }

    /// Key handling for Reading and Paper trail: `s` pins, `P` moves the
    /// sender, `E` sweeps the place (blueprint 35's verbs). Sweeping one
    /// sender's bundle and loading more senders are in the palette. Other
    /// keys fall through to the mail keys, which act on the message under
    /// the cursor.
    fn place_lens_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        match (key.code, key.modifiers) {
            (KeyCode::Char('/'), KeyModifiers::NONE) => Some(Action::OpenGlobalSearch),
            (KeyCode::Char('h') | KeyCode::Left, KeyModifiers::NONE) => {
                self.mailbox.active_pane = ActivePane::Sidebar;
                None
            }
            (KeyCode::Right, KeyModifiers::NONE) => Some(Action::OpenSelected),
            (KeyCode::Char('s'), KeyModifiers::NONE) => Some(Action::TogglePin),
            (KeyCode::Char('P'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::OpenSenderKindMenu)
            }
            (KeyCode::Char('X'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::OpenMoveMenu)
            }
            (KeyCode::Char('E'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::SweepPlace)
            }
            _ if self.mail_action_key(key).is_some() => self.mail_action_key(key),
            _ => self.contextual_input_action(key),
        }
    }

    /// Key handling for the calendar-invites lens (a list pane with inline
    /// RSVP). The `i` chords respond, as everywhere else (`i a`, `i m`,
    /// `i d`, and with a capital to add a comment); Enter opens the
    /// underlying message. The bare letters they replaced say so until
    /// 0.6.185. Unhandled keys fall through to the shared vim/chord input
    /// so j/k/gg/G still navigate.
    fn calendar_invites_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        match (key.code, key.modifiers) {
            (KeyCode::Char('/'), KeyModifiers::NONE) => Some(Action::OpenGlobalSearch),
            (KeyCode::Char('h') | KeyCode::Left, KeyModifiers::NONE) => {
                self.mailbox.active_pane = ActivePane::Sidebar;
                None
            }
            (KeyCode::Enter | KeyCode::Char('o') | KeyCode::Right, _) => {
                Some(Action::OpenMessageView)
            }
            // Cancel a just-issued RSVP while it's still in its send window.
            (KeyCode::Char('u'), KeyModifiers::NONE) => Some(Action::UndoLastMutation),
            (KeyCode::Char(c @ ('a' | 't' | 'm' | 'd' | 'A' | 'T' | 'M' | 'D')), m)
                if plain_or_shift(m) =>
            {
                self.announce_moved("invites", &c.to_string());
                None
            }
            _ => self.contextual_input_action(key),
        }
    }

    fn handle_screen_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        // The palette's Ctrl-k, before a screen's `k` (which ignores
        // modifiers) moves; a text field keeps it, and so does an open
        // overlay (blueprint 35: it owns every key).
        let typing = (self.screen == Screen::Rules && self.rules.page.form.visible)
            || (self.screen == Screen::Accounts && self.accounts.page.form.editing_field)
            || (self.screen == Screen::Search && self.search.page.editing);
        let page = &self.accounts.page;
        let overlay = (self.screen == Screen::Rules && self.rules.page.delete_confirm.is_some())
            || (self.screen == Screen::Analytics && self.modals.analytics_filter.is_some())
            || (self.screen == Screen::Accounts
                && (page.resume_new_account_draft_prompt_open
                    || page.identity_editor.is_some()
                    || page.active_auth_session.is_some()
                    || page.form.pending_mode_switch.is_some()));
        if key.code == KeyCode::Char('k')
            && key.modifiers == KeyModifiers::CONTROL
            && !typing
            && !overlay
        {
            return Some(Action::OpenCommandPalette);
        }
        match self.screen {
            Screen::Search => self.handle_search_screen_key(key),
            Screen::Rules => self.handle_rules_screen_key(key),
            Screen::Diagnostics => self.handle_diagnostics_screen_key(key),
            Screen::Accounts => self.handle_accounts_screen_key(key),
            Screen::Analytics => self.handle_analytics_screen_key(key),
            Screen::Deliveries => self.handle_deliveries_screen_key(key),
            Screen::Mailbox => None,
        }
    }

    fn handle_deliveries_screen_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        use crossterm::event::{KeyCode, KeyModifiers};
        match (key.code, key.modifiers) {
            // Scroll the open email in the split preview.
            (KeyCode::Char('d'), KeyModifiers::CONTROL) if self.deliveries.preview_active => {
                self.mailbox.message_scroll_offset =
                    self.mailbox.message_scroll_offset.saturating_add(10);
                None
            }
            (KeyCode::Char('u'), KeyModifiers::CONTROL) if self.deliveries.preview_active => {
                self.mailbox.message_scroll_offset =
                    self.mailbox.message_scroll_offset.saturating_sub(10);
                None
            }
            (KeyCode::Char('j') | KeyCode::Down, _) => {
                self.deliveries.select_next();
                None
            }
            (KeyCode::Char('k') | KeyCode::Up, _) => {
                self.deliveries.select_prev();
                None
            }
            // Refresh the list. `R`, since `r` resolves and `g` starts the
            // `g` + capital tool chords.
            (KeyCode::Char('R'), modifiers) if plain_or_shift(modifiers) => {
                self.deliveries.loading = true;
                self.pending_deliveries_refresh = true;
                None
            }
            (KeyCode::Char('D'), _) => {
                // Cycle Active -> Delivered -> All and refetch.
                self.deliveries.filter = self.deliveries.filter.next();
                self.deliveries.loading = true;
                self.pending_deliveries_refresh = true;
                None
            }
            (KeyCode::Char('r'), _) => {
                if let Some(row) = self.deliveries.selected_row() {
                    self.pending_delivery_resolve = Some(row.id.clone());
                    self.status_message = Some("Resolving delivery…".into());
                }
                None
            }
            (KeyCode::Char('d'), _) => {
                if let Some(row) = self.deliveries.selected_row() {
                    self.pending_delivery_dismiss = Some(row.id.clone());
                    self.status_message = Some("Dismissing delivery…".into());
                }
                None
            }
            (KeyCode::Char('o') | KeyCode::Enter | KeyCode::Right, _) => {
                if let Some(row) = self.deliveries.selected_row() {
                    if let Some(thread_id) = row.thread_id.clone() {
                        self.pending_delivery_open = Some(thread_id);
                        self.mailbox.message_scroll_offset = 0;
                        self.status_message = Some("Opening source email…".into());
                    } else {
                        self.status_message =
                            Some("No source thread recorded for this delivery".into());
                    }
                }
                None
            }
            // Esc closes the split preview before falling through.
            (KeyCode::Esc, _) if self.deliveries.preview_active => {
                self.close_delivery_preview();
                None
            }
            // Anything else (tab switches, quit, command palette, help, sync)
            // falls through to the shared input handler — otherwise the
            // Deliveries screen would trap the user with no way out.
            _ => self.contextual_input_action(key),
        }
    }

    fn handle_draft_options_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        // The instruction is a text field: letters type, they don't navigate.
        if self.modals.draft_options.active == crate::app::DraftOptionsField::Instruction {
            match key.code {
                KeyCode::Char(c)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    self.modals.draft_options.instruction.push(c);
                    return None;
                }
                KeyCode::Backspace => {
                    self.modals.draft_options.instruction.pop();
                    return None;
                }
                _ => {}
            }
        }
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => self.modals.draft_options.close(),
            (KeyCode::Enter, _) => self.submit_draft_options_modal(),
            (KeyCode::Tab | KeyCode::Up | KeyCode::Down | KeyCode::Char('j' | 'k'), _) => {
                self.modals.draft_options.next_field();
            }
            (KeyCode::Left | KeyCode::Char('h'), _) => self.modals.draft_options.cycle(-1),
            (KeyCode::Right | KeyCode::Char('l'), _) => self.modals.draft_options.cycle(1),
            _ => {}
        }
        None
    }

    fn handle_analytics_screen_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        // The analytics filter modal is a focused form: capture
        // text-edit keys here before they fall through to the screen
        // keymap.
        if self.modals.analytics_filter.is_some() {
            return self.handle_analytics_filter_modal_key(key);
        }

        // The Wrapped view replaces row navigation with tile
        // navigation (the tile grid is a 3x3 layout, not a list).
        if self.analytics.view == crate::app::AnalyticsView::Wrapped {
            return self.handle_wrapped_key(key);
        }

        let view = self.analytics.view;
        let storage_mode = self.analytics.storage_mode;
        match (key.code, key.modifiers) {
            (KeyCode::Char('j') | KeyCode::Down, _) => {
                let len = analytics_row_count(self);
                if len > 0 {
                    self.analytics.selected_index =
                        (self.analytics.selected_index + 1).min(len.saturating_sub(1));
                }
                None
            }
            (KeyCode::Char('k') | KeyCode::Up, _) => {
                self.analytics.selected_index = self.analytics.selected_index.saturating_sub(1);
                None
            }
            (KeyCode::Tab, _) => Some(Action::NextAnalyticsView),
            (KeyCode::BackTab, _) => Some(Action::PrevAnalyticsView),
            (KeyCode::Char('r'), _) => Some(Action::RefreshAnalytics),
            (KeyCode::Esc, _) => Some(Action::OpenMailboxScreen),
            (KeyCode::Char('f'), KeyModifiers::NONE) => Some(Action::OpenAnalyticsFilterModal),
            (KeyCode::Enter, _) => Some(Action::AnalyticsRowDrillDown),
            // Storage view bindings.
            (KeyCode::Char('m'), KeyModifiers::NONE)
                if view == crate::app::AnalyticsView::Storage =>
            {
                Some(Action::CycleStorageMode)
            }
            // `b` for group by: `g` starts the `g` + capital tool chords.
            (KeyCode::Char('b'), KeyModifiers::NONE)
                if view == crate::app::AnalyticsView::Storage
                    && storage_mode == crate::app::StorageMode::Breakdown =>
            {
                Some(Action::CycleStorageGroupBy)
            }
            // Stale view bindings.
            (KeyCode::Char('p'), KeyModifiers::NONE)
                if view == crate::app::AnalyticsView::StaleThreads =>
            {
                Some(Action::ToggleStalePerspective)
            }
            (KeyCode::Char('['), _) if view == crate::app::AnalyticsView::StaleThreads => {
                Some(Action::AdjustStaleOlderThanDays(-7))
            }
            (KeyCode::Char(']'), _) if view == crate::app::AnalyticsView::StaleThreads => {
                Some(Action::AdjustStaleOlderThanDays(7))
            }
            (KeyCode::Char('{'), _) if view == crate::app::AnalyticsView::StaleThreads => {
                Some(Action::AdjustStaleWithinDays(-30))
            }
            (KeyCode::Char('}'), _) if view == crate::app::AnalyticsView::StaleThreads => {
                Some(Action::AdjustStaleWithinDays(30))
            }
            // Contacts view bindings.
            (KeyCode::Char('m'), KeyModifiers::NONE)
                if view == crate::app::AnalyticsView::Contacts =>
            {
                Some(Action::CycleContactsMode)
            }
            (KeyCode::Char('R'), modifiers)
                if plain_or_shift(modifiers) && view == crate::app::AnalyticsView::Contacts =>
            {
                Some(Action::RefreshContacts)
            }
            // ResponseTime view bindings.
            (KeyCode::Char('d'), KeyModifiers::NONE)
                if view == crate::app::AnalyticsView::ResponseTime =>
            {
                Some(Action::ToggleResponseTimeDirection)
            }
            // Subscriptions view bindings.
            (KeyCode::Char('o'), KeyModifiers::NONE)
                if view == crate::app::AnalyticsView::Subscriptions =>
            {
                Some(Action::ToggleSubscriptionsRank)
            }
            (KeyCode::Char('u'), KeyModifiers::NONE)
                if view == crate::app::AnalyticsView::Subscriptions =>
            {
                Some(Action::AnalyticsUnsubscribe)
            }
            _ => self.contextual_input_action(key),
        }
    }

    fn handle_wrapped_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        // Tile order is row-major over a 2x3 grid (six tiles total).
        // Indices: 0=Volume 1=When 2=Contacts | 3=Reply 4=Storage 5=News
        match (key.code, key.modifiers) {
            (KeyCode::Char('h') | KeyCode::Left, _) => {
                let curr = self.analytics.wrapped_selected_tile;
                if curr > 0 && curr <= 5 {
                    self.analytics.wrapped_selected_tile = curr - 1;
                }
                None
            }
            (KeyCode::Char('l') | KeyCode::Right, _) => {
                let curr = self.analytics.wrapped_selected_tile;
                if curr < 5 {
                    self.analytics.wrapped_selected_tile = curr + 1;
                }
                None
            }
            (KeyCode::Char('j') | KeyCode::Down, _) => {
                let curr = self.analytics.wrapped_selected_tile;
                if curr <= 2 {
                    self.analytics.wrapped_selected_tile = curr + 3;
                }
                None
            }
            (KeyCode::Char('k') | KeyCode::Up, _) => {
                let curr = self.analytics.wrapped_selected_tile;
                if (3..=5).contains(&curr) {
                    self.analytics.wrapped_selected_tile = curr - 3;
                }
                None
            }
            (KeyCode::Tab, _) => Some(Action::NextAnalyticsView),
            (KeyCode::BackTab, _) => Some(Action::PrevAnalyticsView),
            (KeyCode::Char('r'), _) => Some(Action::RefreshAnalytics),
            (KeyCode::Esc, _) => Some(Action::OpenMailboxScreen),
            (KeyCode::Char('f'), KeyModifiers::NONE) => Some(Action::OpenAnalyticsFilterModal),
            (KeyCode::Enter, _) => Some(Action::AnalyticsRowDrillDown),
            (KeyCode::Char('y'), KeyModifiers::NONE) => Some(Action::StepWrappedYear(-1)),
            (KeyCode::Char('Y'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::StepWrappedYear(1))
            }
            (KeyCode::Char('t'), KeyModifiers::NONE) => Some(Action::CycleWrappedWindow),
            _ => self.contextual_input_action(key),
        }
    }

    fn handle_analytics_filter_modal_key(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> Option<Action> {
        let modal = self.modals.analytics_filter.as_mut()?;
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => Some(Action::CloseAnalyticsFilterModal),
            (KeyCode::Enter, _) => Some(Action::SubmitAnalyticsFilterModal),
            (KeyCode::Tab, _) => {
                modal.active_field = (modal.active_field + 1) % modal.fields.len().max(1);
                None
            }
            (KeyCode::BackTab, _) => {
                let n = modal.fields.len().max(1);
                modal.active_field = (modal.active_field + n - 1) % n;
                None
            }
            (KeyCode::Right, _) => {
                Self::cycle_analytics_filter_option(modal, 1);
                None
            }
            (KeyCode::Left, _) => {
                Self::cycle_analytics_filter_option(modal, -1);
                None
            }
            (KeyCode::Backspace, _) => {
                if let Some(f) = modal.fields.get_mut(modal.active_field) {
                    if f.options.is_empty() {
                        f.value.pop();
                    }
                }
                None
            }
            (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                if let Some(f) = modal.fields.get_mut(modal.active_field) {
                    if f.options.is_empty() {
                        f.value.push(c);
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn cycle_analytics_filter_option(
        modal: &mut crate::app::AnalyticsFilterModalState,
        delta: isize,
    ) {
        let Some(field) = modal.fields.get_mut(modal.active_field) else {
            return;
        };
        if field.options.is_empty() {
            return;
        }

        let len = field.options.len();
        let current = field
            .options
            .iter()
            .position(|option| option == &field.value)
            .unwrap_or(0);
        let next = if delta.is_negative() {
            (current + len - 1) % len
        } else {
            (current + 1) % len
        };
        field.value = field.options[next].clone();
        modal.validation_error = None;
    }

    fn handle_search_screen_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        if self.search.page.editing {
            return match (key.code, key.modifiers) {
                (KeyCode::Enter, _) => Some(Action::SubmitSearch),
                (KeyCode::Esc, _) => {
                    self.search.page.editing = false;
                    None
                }
                (KeyCode::Backspace, _) => {
                    self.search.page.query.pop();
                    self.trigger_live_search();
                    None
                }
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    self.search.page.query.push(c);
                    self.trigger_live_search();
                    None
                }
                _ => None,
            };
        }

        match self.search.page.active_pane {
            SearchPane::Results => match (key.code, key.modifiers) {
                (KeyCode::Char('/'), _) => {
                    self.search.page.editing = true;
                    None
                }
                (KeyCode::Char('j') | KeyCode::Down, _) => {
                    if self.search.page.selected_index + 1 < self.search_row_count() {
                        self.search.page.selected_index += 1;
                    }
                    self.sync_search_cursor_after_move();
                    None
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    if self.search.page.selected_index > 0 {
                        self.search.page.selected_index -= 1;
                    }
                    self.sync_search_cursor_after_move();
                    None
                }
                (KeyCode::Right, KeyModifiers::NONE) | (KeyCode::Enter | KeyCode::Char('o'), _) => {
                    Some(Action::OpenSelected)
                }
                _ if self.mail_action_key(key).is_some() => self.mail_action_key(key),
                (KeyCode::Esc, _) => Some(Action::OpenMailboxScreen),
                _ => self.contextual_input_action(key),
            },
            SearchPane::Preview => match (key.code, key.modifiers) {
                (KeyCode::Char('/'), _) => {
                    self.search.page.editing = true;
                    None
                }
                // `o` opened the original, as `O` does; it says so until 0.6.185.
                (KeyCode::Char('o'), KeyModifiers::NONE) => {
                    self.announce_moved("reader", "o");
                    None
                }
                (KeyCode::Char('h') | KeyCode::Left, KeyModifiers::NONE) => {
                    self.search.page.active_pane = SearchPane::Results;
                    None
                }
                (KeyCode::Char('j') | KeyCode::Down, _) => {
                    self.move_message_view_down();
                    None
                }
                (KeyCode::Char('k') | KeyCode::Up, _) => {
                    self.move_message_view_up();
                    None
                }
                (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                    self.mailbox.message_scroll_offset =
                        self.mailbox.message_scroll_offset.saturating_add(20);
                    None
                }
                (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                    self.mailbox.message_scroll_offset =
                        self.mailbox.message_scroll_offset.saturating_sub(20);
                    None
                }
                (KeyCode::Char('G'), modifiers) if plain_or_shift(modifiers) => {
                    self.mailbox.message_scroll_offset = u16::MAX;
                    None
                }
                (KeyCode::Char('H'), modifiers) if plain_or_shift(modifiers) => {
                    Some(Action::ToggleHtmlView)
                }
                (KeyCode::Char('M'), modifiers) if plain_or_shift(modifiers) => {
                    Some(Action::ToggleRemoteContent)
                }
                (KeyCode::Esc, _) => {
                    self.reset_search_preview_selection();
                    None
                }
                _ if self.mail_action_key(key).is_some() => self.mail_action_key(key),
                _ => self.contextual_input_action(key),
            },
        }
    }

    fn handle_rules_screen_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        if self.rules.page.form.visible {
            return self.handle_rule_form_key(key);
        }

        // Rule delete confirm, which opens on Keep: `y`, or `Enter` on
        // "Delete rule", deletes; n/Esc/q keep the rule.
        if let Some((rule_id, name)) = self.rules.page.delete_confirm.clone() {
            match self.rules.page.delete_choice.key(key.code) {
                ConfirmKey::Confirm => {
                    self.rules.page.delete_confirm = None;
                    self.rules.pending_delete = Some(rule_id);
                    self.rules.page.status = Some(format!("Deleting {name}..."));
                }
                ConfirmKey::Keep => {
                    self.rules.page.delete_confirm = None;
                    self.rules.page.status = Some(format!("Kept {name}"));
                }
                ConfirmKey::Moved | ConfirmKey::Ignored => {}
            }
            return None;
        }

        match (key.code, key.modifiers) {
            (KeyCode::PageDown, _) if self.rules.page.panel == RulesPanel::DryRun => {
                self.rules.page.dry_run_page = self
                    .rules
                    .page
                    .dry_run_page
                    .saturating_add(1)
                    .min(self.rules.page.dry_run_pages().saturating_sub(1));
                None
            }
            (KeyCode::PageUp, _) if self.rules.page.panel == RulesPanel::DryRun => {
                self.rules.page.dry_run_page = self.rules.page.dry_run_page.saturating_sub(1);
                None
            }
            (KeyCode::Char('j') | KeyCode::Down, _) => {
                if self.rules.page.selected_index + 1 < self.rules.page.rules.len() {
                    self.rules.page.selected_index += 1;
                    self.refresh_selected_rule_panel();
                }
                None
            }
            (KeyCode::Char('k') | KeyCode::Up, _) => {
                self.rules.page.selected_index = self.rules.page.selected_index.saturating_sub(1);
                self.refresh_selected_rule_panel();
                None
            }
            (KeyCode::Char('A'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::ApplyRuleTreatment)
            }
            (KeyCode::Enter | KeyCode::Char('o'), _) => Some(Action::RefreshRules),
            (KeyCode::Char('e'), _) => Some(Action::ToggleRuleEnabled),
            (KeyCode::Char('D'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::ShowRuleDryRun)
            }
            (KeyCode::Char('H'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::ShowRuleHistory)
            }
            (KeyCode::Char('#'), _) => Some(Action::DeleteRule),
            // c creates what this place holds (blueprint 35).
            (KeyCode::Char('c'), KeyModifiers::NONE) => Some(Action::OpenRuleFormNew),
            (KeyCode::Char('n'), KeyModifiers::NONE) => {
                self.announce_moved("rules", "n");
                None
            }
            (KeyCode::Char('E'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::OpenRuleFormEdit)
            }
            (KeyCode::Esc, _) => Some(Action::OpenMailboxScreen),
            _ => self.contextual_input_action(key),
        }
    }

    fn handle_rule_form_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => {
                self.rules.page.form.visible = false;
                self.rules.page.panel = RulesPanel::Details;
                None
            }
            (KeyCode::Tab, _) => {
                self.rules.page.form.active_field = (self.rules.page.form.active_field + 1) % 6;
                None
            }
            (KeyCode::BackTab, _) => {
                self.rules.page.form.active_field = if self.rules.page.form.active_field == 0 {
                    5
                } else {
                    self.rules.page.form.active_field - 1
                };
                None
            }
            (KeyCode::Char(' '), _) if self.rules.page.form.active_field == 4 => {
                self.rules.page.form.enabled = !self.rules.page.form.enabled;
                None
            }
            (KeyCode::Char(' '), _) if self.rules.page.form.active_field == 5 => {
                let mut choices: Vec<_> = self
                    .accounts
                    .page
                    .accounts
                    .iter()
                    .filter(|a| a.enabled)
                    .map(|a| Some(a.account_id.clone()))
                    .collect();
                if super::rule_actions::category_actions(&self.rules.page.form.action)
                    || self.rules.category_edit
                {
                    choices.push(None);
                }
                let index = choices
                    .iter()
                    .position(|id| id == &self.rules.page.form.account_id)
                    .map_or(0, |i| (i + 1) % choices.len().max(1));
                self.rules.page.form.account_id = choices.get(index).cloned().flatten();
                None
            }
            (KeyCode::Char('d'), KeyModifiers::CONTROL) => Some(Action::ShowRuleDryRun),
            (KeyCode::Char('s'), KeyModifiers::CONTROL) => Some(Action::SaveRuleForm),
            (_, _) if self.rules.page.form.active_field == 1 => {
                self.rules.condition_editor.input(key);
                self.sync_rule_form_strings_from_editors();
                None
            }
            (_, _) if self.rules.page.form.active_field == 2 => {
                self.rules.action_editor.input(key);
                self.sync_rule_form_strings_from_editors();
                None
            }
            (KeyCode::Backspace, _) => {
                match self.rules.page.form.active_field {
                    0 => {
                        self.rules.page.form.name.pop();
                    }
                    3 => {
                        self.rules.page.form.priority.pop();
                    }
                    _ => {}
                }
                None
            }
            (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                match self.rules.page.form.active_field {
                    0 => self.rules.page.form.name.push(c),
                    3 => self.rules.page.form.priority.push(c),
                    _ => {}
                }
                None
            }
            (KeyCode::Enter, _) => Some(Action::SaveRuleForm),
            _ => None,
        }
    }

    fn handle_saved_search_form_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        let form = self.modals.saved_search_form.as_mut()?;
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => {
                self.close_saved_search_form();
                None
            }
            (KeyCode::Tab, _) => {
                form.active_field = match form.active_field {
                    SavedSearchFormField::Name => SavedSearchFormField::Query,
                    SavedSearchFormField::Query => SavedSearchFormField::Mode,
                    SavedSearchFormField::Mode => SavedSearchFormField::Name,
                };
                form.validation_error = None;
                None
            }
            (KeyCode::BackTab, _) => {
                form.active_field = match form.active_field {
                    SavedSearchFormField::Name => SavedSearchFormField::Mode,
                    SavedSearchFormField::Query => SavedSearchFormField::Name,
                    SavedSearchFormField::Mode => SavedSearchFormField::Query,
                };
                form.validation_error = None;
                None
            }
            (KeyCode::Enter, _) | (KeyCode::Char('s'), KeyModifiers::CONTROL) => {
                Some(Action::SaveSavedSearchForm)
            }
            (KeyCode::Char(' '), _) if form.active_field == SavedSearchFormField::Mode => {
                form.search_mode = match form.search_mode {
                    mxr_core::types::SearchMode::Lexical => mxr_core::types::SearchMode::Hybrid,
                    mxr_core::types::SearchMode::Hybrid => mxr_core::types::SearchMode::Semantic,
                    mxr_core::types::SearchMode::Semantic => mxr_core::types::SearchMode::Lexical,
                };
                form.validation_error = None;
                None
            }
            (KeyCode::Backspace, _) => {
                match form.active_field {
                    SavedSearchFormField::Name => {
                        form.name.pop();
                    }
                    SavedSearchFormField::Query => {
                        form.query.pop();
                    }
                    SavedSearchFormField::Mode => {}
                }
                form.validation_error = None;
                None
            }
            (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                match form.active_field {
                    SavedSearchFormField::Name => form.name.push(c),
                    SavedSearchFormField::Query => form.query.push(c),
                    SavedSearchFormField::Mode => {}
                }
                form.validation_error = None;
                None
            }
            _ => None,
        }
    }

    fn handle_diagnostics_screen_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        // Live-search input mode: when active, all keys feed into the search
        // string for the focused pane. Esc cancels, Enter commits (closes the
        // input but keeps the string applied), Backspace deletes.
        if let Some(target_pane) = self.diagnostics.page.search_input {
            match (key.code, key.modifiers) {
                (KeyCode::Esc, _) => {
                    // Cancel: clear both the input and the applied search.
                    match target_pane {
                        crate::app::DiagnosticsPaneKind::Events => {
                            self.diagnostics.page.events_search.clear();
                        }
                        crate::app::DiagnosticsPaneKind::Logs => {
                            self.diagnostics.page.logs_search.clear();
                        }
                        crate::app::DiagnosticsPaneKind::Activity => {
                            self.diagnostics.page.activity_search.clear();
                        }
                        _ => {}
                    }
                    self.diagnostics.page.search_input = None;
                    return None;
                }
                (KeyCode::Enter, _) => {
                    self.diagnostics.page.search_input = None;
                    return None;
                }
                (KeyCode::Backspace, _) => {
                    let buf = match target_pane {
                        crate::app::DiagnosticsPaneKind::Events => {
                            &mut self.diagnostics.page.events_search
                        }
                        crate::app::DiagnosticsPaneKind::Logs => {
                            &mut self.diagnostics.page.logs_search
                        }
                        crate::app::DiagnosticsPaneKind::Activity => {
                            &mut self.diagnostics.page.activity_search
                        }
                        _ => return None,
                    };
                    buf.pop();
                    return None;
                }
                (KeyCode::Char(c), modifiers) if !modifiers.contains(KeyModifiers::CONTROL) => {
                    let buf = match target_pane {
                        crate::app::DiagnosticsPaneKind::Events => {
                            &mut self.diagnostics.page.events_search
                        }
                        crate::app::DiagnosticsPaneKind::Logs => {
                            &mut self.diagnostics.page.logs_search
                        }
                        crate::app::DiagnosticsPaneKind::Activity => {
                            &mut self.diagnostics.page.activity_search
                        }
                        _ => return None,
                    };
                    buf.push(c);
                    return None;
                }
                _ => return None,
            }
        }

        match (key.code, key.modifiers) {
            (KeyCode::Char('/'), _) => {
                // Start search-input mode on a filterable pane.
                let pane = self.diagnostics.page.active_pane();
                if matches!(
                    pane,
                    crate::app::DiagnosticsPaneKind::Events
                        | crate::app::DiagnosticsPaneKind::Logs
                        | crate::app::DiagnosticsPaneKind::Activity
                ) {
                    self.diagnostics.page.search_input = Some(pane);
                }
                None
            }
            (KeyCode::Tab | KeyCode::Right, _) => {
                self.diagnostics.page.selected_pane = self.diagnostics.page.selected_pane.next();
                None
            }
            (KeyCode::BackTab | KeyCode::Left, _) => {
                self.diagnostics.page.selected_pane = self.diagnostics.page.selected_pane.prev();
                None
            }
            (KeyCode::Char('j') | KeyCode::Down, _) => {
                self.diagnostics.page.selected_pane = self.diagnostics.page.selected_pane.next();
                None
            }
            (KeyCode::Char('k') | KeyCode::Up, _) => {
                self.diagnostics.page.selected_pane = self.diagnostics.page.selected_pane.prev();
                None
            }
            (KeyCode::Char('d'), KeyModifiers::CONTROL) | (KeyCode::PageDown, _) => {
                let pane = self.diagnostics.page.active_pane();
                *self.diagnostics.page.scroll_offset_mut(pane) =
                    self.diagnostics.page.scroll_offset(pane).saturating_add(8);
                None
            }
            (KeyCode::Char('u'), KeyModifiers::CONTROL) | (KeyCode::PageUp, _) => {
                let pane = self.diagnostics.page.active_pane();
                *self.diagnostics.page.scroll_offset_mut(pane) =
                    self.diagnostics.page.scroll_offset(pane).saturating_sub(8);
                None
            }
            (KeyCode::Enter | KeyCode::Char('o'), _) => {
                self.diagnostics.page.toggle_fullscreen();
                None
            }
            (KeyCode::Char('d'), _) => Some(Action::OpenDiagnosticsPaneDetails),
            (KeyCode::Char('r'), _) => Some(Action::RefreshDiagnostics),
            (KeyCode::Char('a'), _) => Some(Action::PreviewRecoveryActivation),
            (KeyCode::Char('A'), _) => Some(Action::ConfirmRecoveryActivation),
            (KeyCode::Char('p'), _) => Some(Action::PreviewRecoveredDraft),
            (KeyCode::Char('P'), _) => Some(Action::ConfirmRecoveredDraft),
            (KeyCode::Char(']'), _) => Some(Action::NextRecoveredDraft),
            (KeyCode::Char('['), _) => Some(Action::PreviousRecoveredDraft),
            (KeyCode::Char('b'), _) => Some(Action::GenerateBugReport),
            (KeyCode::Char('c'), _) => Some(Action::EditConfig),
            (KeyCode::Char('L'), modifiers) if plain_or_shift(modifiers) => Some(Action::OpenLogs),
            (KeyCode::Esc, _) => Some(Action::OpenMailboxScreen),
            _ => self.contextual_input_action(key),
        }
    }

    fn handle_accounts_screen_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        if self.accounts.page.active_auth_session.is_some() {
            if key.code == crossterm::event::KeyCode::Esc {
                return Some(Action::CancelOutlookAuth);
            }
            return None;
        }

        // "Resume draft" is lit and "Discard draft" is named (blueprint 35's
        // Destroy rule): only `y`, or Enter on "Discard draft", throws the
        // form away. `n`, Esc and `q` keep it for later.
        if self.accounts.page.resume_new_account_draft_prompt_open {
            let choice = &mut self.accounts.page.resume_draft_choice;
            match key.code {
                KeyCode::Char('c') => self.restore_new_account_form_draft(),
                KeyCode::Enter if !choice.on_action => self.restore_new_account_form_draft(),
                code => match choice.key(code) {
                    ConfirmKey::Confirm => {
                        self.accounts.page.new_account_draft = None;
                        self.open_new_account_form();
                    }
                    ConfirmKey::Keep => {
                        self.accounts.page.resume_new_account_draft_prompt_open = false;
                    }
                    ConfirmKey::Moved | ConfirmKey::Ignored => {}
                },
            }
            return None;
        }

        if self.accounts.page.identity_editor.is_some() {
            return self.handle_identity_editor_key(key);
        }

        if self.accounts.page.form.visible {
            return self.handle_account_form_key(key);
        }

        match (key.code, key.modifiers) {
            // i edits the selected account's colour and tag (blueprint 33).
            (KeyCode::Char('i'), KeyModifiers::NONE) => {
                self.open_identity_editor();
                None
            }
            (KeyCode::Char('j') | KeyCode::Down, _) => {
                if self.accounts.page.selected_index + 1 < self.accounts.page.accounts.len() {
                    self.accounts.page.selected_index += 1;
                }
                None
            }
            (KeyCode::Char('k') | KeyCode::Up, _) => {
                self.accounts.page.selected_index =
                    self.accounts.page.selected_index.saturating_sub(1);
                None
            }
            // c creates what this place holds (blueprint 35); Edit Config,
            // which had c here, is in the palette.
            (KeyCode::Char('c'), KeyModifiers::NONE) => Some(Action::OpenAccountFormNew),
            (KeyCode::Char('n'), KeyModifiers::NONE) => {
                self.announce_moved("accounts", "n");
                None
            }
            (KeyCode::Char('r'), _) => {
                // Phase 2.3: when the selected account is reporting an
                // unhealthy sync, `r` repairs it instead of refreshing
                // the list. Refresh stays available via `R` (Shift) so
                // the original behavior never disappears.
                let unhealthy = self
                    .selected_account()
                    .is_some_and(|account| self.account_unhealthy(account));
                if unhealthy {
                    Some(Action::RepairAccount)
                } else {
                    Some(Action::RefreshAccounts)
                }
            }
            (KeyCode::Char('R'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::RefreshAccounts)
            }
            (KeyCode::Char('t'), _) => Some(Action::TestAccountForm),
            (KeyCode::Char('O'), modifiers)
                if plain_or_shift(modifiers)
                    && super::account_result_has_details(
                        self.accounts.page.last_result.as_ref(),
                    ) =>
            {
                self.open_last_account_result_details_modal();
                None
            }
            (KeyCode::Char('d'), _) => Some(Action::SetDefaultAccount),
            (KeyCode::Enter | KeyCode::Char('o'), _) => {
                if let Some(account) = self.selected_account().cloned() {
                    if let Some(config) = account_summary_to_config(&account) {
                        self.accounts.page.form = account_form_from_config(config);
                        self.accounts.page.form.visible = true;
                    } else {
                        self.accounts.page.status = Some(
                            "Runtime-only account is inspectable but not editable here.".into(),
                        );
                    }
                }
                None
            }
            (KeyCode::Esc, _) if self.accounts.page.onboarding_required => None,
            (KeyCode::Esc, _) => Some(Action::OpenMailboxScreen),
            _ => self.contextual_input_action(key),
        }
    }

    fn handle_account_form_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        if self.accounts.page.form.pending_mode_switch.is_some() {
            return match (key.code, key.modifiers) {
                (KeyCode::Enter | KeyCode::Char('y'), _) => {
                    if let Some(mode) = self.accounts.page.form.pending_mode_switch {
                        self.apply_account_form_mode(mode);
                    }
                    None
                }
                (KeyCode::Esc | KeyCode::Char('n'), _) => {
                    self.accounts.page.form.pending_mode_switch = None;
                    None
                }
                _ => None,
            };
        }

        if self.accounts.page.form.editing_field {
            return match (key.code, key.modifiers) {
                (KeyCode::Esc | KeyCode::Enter, _) => {
                    self.accounts.page.form.editing_field = false;
                    None
                }
                (KeyCode::Tab, _) => {
                    self.accounts.page.form.editing_field = false;
                    self.accounts.page.form.active_field = (self.accounts.page.form.active_field
                        + 1)
                        % self.account_form_field_count();
                    self.accounts.page.form.field_cursor =
                        account_form_field_value(&self.accounts.page.form)
                            .map_or(0, |value| value.chars().count());
                    None
                }
                (KeyCode::BackTab, _) => {
                    self.accounts.page.form.editing_field = false;
                    self.accounts.page.form.active_field =
                        self.accounts.page.form.active_field.saturating_sub(1);
                    self.accounts.page.form.field_cursor =
                        account_form_field_value(&self.accounts.page.form)
                            .map_or(0, |value| value.chars().count());
                    None
                }
                (KeyCode::Left, _) => {
                    self.accounts.page.form.field_cursor =
                        self.accounts.page.form.field_cursor.saturating_sub(1);
                    None
                }
                (KeyCode::Right, _) => {
                    if let Some(value) = account_form_field_value(&self.accounts.page.form) {
                        self.accounts.page.form.field_cursor =
                            (self.accounts.page.form.field_cursor + 1).min(value.chars().count());
                    }
                    None
                }
                (KeyCode::Home, _) => {
                    self.accounts.page.form.field_cursor = 0;
                    None
                }
                (KeyCode::End, _) => {
                    self.accounts.page.form.field_cursor =
                        account_form_field_value(&self.accounts.page.form)
                            .map_or(0, |value| value.chars().count());
                    None
                }
                (KeyCode::Backspace, _) => {
                    delete_account_form_char(&mut self.accounts.page.form, true);
                    self.refresh_account_form_derived_fields();
                    None
                }
                (KeyCode::Delete, _) => {
                    delete_account_form_char(&mut self.accounts.page.form, false);
                    self.refresh_account_form_derived_fields();
                    None
                }
                (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                    insert_account_form_char(&mut self.accounts.page.form, c);
                    self.refresh_account_form_derived_fields();
                    None
                }
                _ => None,
            };
        }

        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => {
                self.maybe_preserve_new_account_form_draft();
                None
            }
            (KeyCode::Left | KeyCode::Char('h'), _) => {
                self.request_account_form_mode_change(false);
                None
            }
            (KeyCode::Right | KeyCode::Char('l'), _) => {
                self.request_account_form_mode_change(true);
                None
            }
            (KeyCode::Char('j') | KeyCode::Down, _) => {
                self.accounts.page.form.active_field =
                    (self.accounts.page.form.active_field + 1) % self.account_form_field_count();
                None
            }
            (KeyCode::Char('k') | KeyCode::Up, _) => {
                self.accounts.page.form.active_field = if self.accounts.page.form.active_field == 0
                {
                    self.account_form_field_count().saturating_sub(1)
                } else {
                    self.accounts.page.form.active_field - 1
                };
                None
            }
            (KeyCode::Tab, _) => {
                if self.accounts.page.form.active_field == 0 {
                    self.request_account_form_mode_change(true);
                } else {
                    self.accounts.page.form.active_field = (self.accounts.page.form.active_field
                        + 1)
                        % self.account_form_field_count();
                }
                None
            }
            (KeyCode::BackTab, _) => {
                if self.accounts.page.form.active_field == 0 {
                    self.request_account_form_mode_change(false);
                } else {
                    self.accounts.page.form.active_field =
                        self.accounts.page.form.active_field.saturating_sub(1);
                }
                None
            }
            (KeyCode::Enter | KeyCode::Char('i'), _) => {
                if account_form_field_is_editable(&self.accounts.page.form) {
                    self.accounts.page.form.editing_field = true;
                    self.accounts.page.form.field_cursor =
                        account_form_field_value(&self.accounts.page.form)
                            .map_or(0, |value| value.chars().count());
                } else {
                    if self.accounts.page.form.active_field == 0 {
                        self.request_account_form_mode_change(true);
                    } else {
                        let _ = self.toggle_current_account_form_field(true);
                    }
                }
                None
            }
            (KeyCode::Char('t'), _) => Some(Action::TestAccountForm),
            (KeyCode::Char('o'), _)
                if super::account_result_has_details(
                    self.accounts.page.form.last_result.as_ref(),
                ) =>
            {
                self.open_last_account_result_details_modal();
                None
            }
            (KeyCode::Char('r'), _)
                if matches!(self.accounts.page.form.mode, AccountFormMode::Gmail) =>
            {
                Some(Action::ReauthorizeAccountForm)
            }
            (KeyCode::Char('s'), _) => Some(Action::SaveAccountForm),
            (KeyCode::Char(' '), _) if self.accounts.page.form.active_field == 0 => {
                self.request_account_form_mode_change(true);
                None
            }
            (KeyCode::Char(' '), _) if self.toggle_current_account_form_field(true) => None,
            _ => None,
        }
    }
}

fn analytics_row_count(app: &App) -> usize {
    use crate::app::{AnalyticsView, ContactsMode, StorageMode};
    match app.analytics.view {
        AnalyticsView::Storage => match app.analytics.storage_mode {
            StorageMode::Breakdown => app.analytics.storage_rows.len(),
            StorageMode::LargestMessages => app.analytics.largest_message_rows.len(),
        },
        AnalyticsView::StaleThreads => app.analytics.stale_rows.len(),
        AnalyticsView::Contacts => match app.analytics.contacts_mode {
            ContactsMode::Asymmetry => app.analytics.asymmetry_rows.len(),
            ContactsMode::Decay => app.analytics.decay_rows.len(),
        },
        AnalyticsView::CadenceDrift => app.analytics.cadence_drift_rows.len(),
        AnalyticsView::ResponseTime => 1, // single summary row
        AnalyticsView::Subscriptions => app.analytics.subscriptions.len(),
        AnalyticsView::SearchAggregation => app.analytics.search_aggregation_rows.len(),
        AnalyticsView::Wrapped => 1, // tile grid uses tile selection, not row index
    }
}
