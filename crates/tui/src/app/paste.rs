//! Pasted text (bracketed paste, blueprint 35 "Terminal and browser keys").
//! A paste types into the focused text field and is otherwise dropped, so
//! pasted text can never run as shortcuts.

use super::*;
use ratatui::crossterm::event::KeyEvent;

impl App {
    /// Type `text` into the focused text field, or drop it when none has
    /// the keys. Fields are single-line, so line breaks and tabs become
    /// spaces rather than Enter (which would submit) or Tab (next field).
    pub fn handle_paste(&mut self, text: &str) {
        let text = text.replace("\r\n", "\n");
        let typed = text.chars().filter_map(|c| match c {
            '\n' | '\r' | '\t' => Some(' '),
            c if c.is_control() => None,
            c => Some(c),
        });
        for c in typed {
            // Checked per character: a field that closed must not let the
            // rest of the paste reach the shortcuts behind it.
            if !self.text_input_focused() {
                break;
            }
            if let Some(action) =
                self.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
            {
                self.apply(action);
            }
        }
    }

    /// The owner of the next key is a text field. Mirrors the order in
    /// `handle_key_press`: the first owner that takes keys decides, so a
    /// menu over a field keeps the paste out of both.
    pub(crate) fn text_input_focused(&self) -> bool {
        let modals = &self.modals;
        if modals.error.is_some() {
            return false;
        }
        if modals.draft_options.visible {
            return modals.draft_options.active == DraftOptionsField::Instruction;
        }
        // The identity tag takes letters; the colour field does not.
        if self
            .accounts
            .page
            .identity_editor
            .as_ref()
            .is_some_and(|editor| editor.field == IdentityField::Tag)
        {
            return true;
        }
        if modals.platform.visible || self.accounts.page.onboarding_modal_open {
            return false;
        }
        if modals.help_open {
            return true;
        }
        if modals.personal_categories.visible {
            let categories = &modals.personal_categories;
            return categories.why.is_none()
                && ((categories.name_action.is_some() && !categories.name_preview_pending)
                    || (categories.name_action.is_none() && categories.filtering));
        }
        if modals.snippets.visible
            || modals.summary.visible
            || modals.briefing.visible
            || modals.whois.visible
            || modals.expert.visible
            || modals.sender_profile.visible
            || modals.reply_queue.visible
            || modals.drafts.visible
            || modals.screener.visible
            || modals.pending_saved_search_delete_confirm.is_some()
        {
            return false;
        }
        if let Some(form) = &modals.saved_search_form {
            return form.active_field != SavedSearchFormField::Mode;
        }
        if self.command_palette.palette.visible || self.search.bar.active {
            return true;
        }
        if self.compose.pending_send_confirm.is_some() {
            return self.compose.pending_remind_at_input.is_some()
                || self.compose.pending_send_at_input.is_some();
        }
        let trust = &self.mailbox.trust;
        if self.mailbox.sweep_confirm.is_some()
            || trust.move_menu.is_some()
            || trust.sender_ask.as_ref().is_some_and(|ask| ask.yes_no)
            || (trust.arrivals_list.is_some() && self.screen == Screen::Mailbox)
            || self.mailbox.sender_kind_menu.is_some()
            || modals.pending_bulk_confirm.is_some()
            || modals.pending_unsubscribe_confirm.is_some()
        {
            return false;
        }
        if modals.reply_later_prompt.is_some()
            || self.mailbox.todo_page.prompt.is_some()
            || self.mailbox.records_page.prompt.is_some()
        {
            return true;
        }
        if self.mailbox.records_page.pass_menu.is_some() {
            return false;
        }
        if modals.snooze_panel.visible {
            return modals.snooze_panel.custom_input.is_some();
        }
        if self.mailbox.url_modal.is_some() {
            return false;
        }
        if modals.export.visible {
            return modals.export.stage == ExportStage::Path;
        }
        if modals.save_attachment.visible {
            return true;
        }
        if self.mailbox.attachment_panel.visible {
            return false;
        }
        if self.compose.compose_picker.visible || modals.label_picker.visible {
            return true;
        }
        let assistant = &self.assistant;
        if assistant.open && assistant.focus != AssistantFocus::Away {
            if assistant.models.is_some() || assistant.list.is_some() {
                return false;
            }
            return match &assistant.prompt {
                Some(AssistantPrompt::RejectReason { .. } | AssistantPrompt::Other { .. }) => true,
                Some(AssistantPrompt::ConfirmDelete(..)) => false,
                None => assistant.focus == AssistantFocus::Input,
            };
        }
        if !self.promise_prompts.is_empty() {
            return false;
        }
        match self.screen {
            Screen::Search => self.search.page.editing,
            Screen::Rules => self.rules.page.form.visible && self.rules.page.form.active_field <= 3,
            Screen::Accounts => {
                let page = &self.accounts.page;
                // With single-key shortcuts off, a text field takes its
                // letters without Enter first (see `handle_account_form_key`).
                page.active_auth_session.is_none()
                    && !page.resume_new_account_draft_prompt_open
                    && page.form.visible
                    && page.form.pending_mode_switch.is_none()
                    && (page.form.editing_field
                        || (!self.keybindings.single_keys
                            && account_form_field_is_editable(&page.form)))
            }
            Screen::Analytics => modals.analytics_filter.as_ref().is_some_and(|modal| {
                modal
                    .fields
                    .get(modal.active_field)
                    .is_some_and(|field| field.options.is_empty())
            }),
            Screen::Diagnostics => self.diagnostics.page.search_input.is_some(),
            Screen::Deliveries => false,
            Screen::Mailbox => {
                if self.mailbox.active_pane != ActivePane::MailList {
                    return false;
                }
                match self.mailbox.mailbox_view {
                    MailboxView::ArchiveMode => self.mailbox.records_page.asking,
                    MailboxView::Reading => {
                        let page = &self.mailbox.reading_page;
                        page.confirm.is_none()
                            && page
                                .highlights
                                .as_ref()
                                .is_some_and(|panel| matches!(panel.step, HighlightStep::Note(_)))
                    }
                    _ => false,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_paste_types_into_the_focused_field_as_one_line() {
        let mut app = App::new();
        app.search.bar.activate();
        app.handle_paste("from:alice\r\nsubject:\tlunch\u{7}");
        assert_eq!(app.search.bar.query, "from:alice subject: lunch");
        assert!(app.search.bar.active, "a pasted line break must not submit");
    }

    #[test]
    fn a_paste_with_no_field_focused_runs_no_shortcut() {
        let mut app = App::new();
        app.screen = Screen::Mailbox;
        app.mailbox.active_pane = ActivePane::MailList;
        app.mailbox.envelopes = (0..3)
            .map(|i| {
                crate::test_fixtures::TestEnvelopeBuilder::new()
                    .provider_id(format!("paste-{i}"))
                    .build()
            })
            .collect();
        app.handle_paste("xjq/?");
        assert!(!app.should_quit);
        assert!(app.mailbox.selected_set.is_empty());
        assert!(!app.search.bar.active);
        assert!(!app.modals.help_open);
    }

    #[test]
    fn a_menu_over_a_field_keeps_the_paste_out_of_both() {
        let mut app = App::new();
        app.search.bar.activate();
        app.modals.error = Some(ErrorModalState::new("Oops", "detail"));
        app.handle_paste("q");
        assert!(app.modals.error.is_some(), "q closed the error modal");
        assert_eq!(app.search.bar.query, "");
    }
}
