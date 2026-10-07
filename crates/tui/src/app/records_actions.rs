//! Archive in the TUI: open the ledger, ask the answer box, copy a
//! reference or an amount, open a record's PDF or email, step through
//! years and issuers, fix and confirm fields, dismiss, export, and `T`
//! on a conversation to file it. The daemon owns every rule, total and
//! message; this module turns keys into requests.

use super::input::plain_or_shift;
use super::*;
use crate::app::state::{PassMenu, RecordFixPrompt, RECORD_KIND_CHIPS};
use mxr_protocol::{RecordData, RecordEditData, RecordUndoData};

/// The mode id the guide and its card are kept under.
pub(crate) const ARCHIVE_MODE: &str = mxr_protocol::ARCHIVE_GUIDE.mode;

/// Copies to the system clipboard. Tests never touch the real one: what
/// was copied is kept on the page either way.
fn copy_to_clipboard(text: &str) {
    #[cfg(all(not(test), target_os = "macos"))]
    {
        use std::io::Write;
        if let Ok(mut child) = std::process::Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
        }
    }
    #[cfg(all(not(test), target_os = "linux"))]
    {
        use std::io::Write;
        if let Ok(mut child) = std::process::Command::new("xclip")
            .args(["-selection", "clipboard"])
            .stdin(std::process::Stdio::piped())
            .spawn()
        {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
        }
    }
    #[cfg(test)]
    let _ = text;
}

/// Plain digits for a spreadsheet or a form: "1249.00".
fn plain_amount(minor: i64) -> String {
    let sign = if minor < 0 { "-" } else { "" };
    let minor = minor.unsigned_abs();
    format!("{sign}{}.{:02}", minor / 100, minor % 100)
}

impl App {
    pub(super) fn apply_records_action(&mut self, action: Action) {
        match action {
            Action::RecordsAsk => {
                let page = &mut self.mailbox.records_page;
                page.asking = true;
                page.card = None;
            }
            Action::RecordsCopyReference => self.copy_record_field(false),
            Action::RecordsCopyAmount => self.copy_record_field(true),
            Action::RecordsOpenDocument => self.open_record_document(),
            Action::RecordsOpenCard => {
                if let Some(record) = self.focused_record() {
                    self.mailbox.records_page.pending_card = Some(record.id.clone());
                }
            }
            Action::RecordsOpenEmail => self.open_record_email(),
            Action::RecordsIssuerPage => self.records_issuer_page(),
            Action::RecordsPrevYear => self.step_records_year(-1),
            Action::RecordsNextYear => self.step_records_year(1),
            Action::RecordsFix => self.open_record_fix_prompt(),
            Action::RecordsMarkChecked => self.mark_record_checked(),
            Action::RecordsDismiss => self.dismiss_record(),
            Action::RecordsExport => {
                self.mailbox.records_page.pending_export = Some(false);
                self.status_message = Some("Checking what the export would hold…".into());
            }
            Action::RecordsNextKind => self.next_record_kind(),
            Action::RecordsMakeTodo => self.make_todo_from_record(),
            Action::RecordsBack => self.records_back(),
            Action::PassToMode => self.open_pass_menu(),
            _ => {}
        }
    }

    pub(super) fn open_archive_records(&mut self) {
        self.enter_mode_view(MailboxView::ArchiveMode);
        self.mailbox.pending_rail_refresh = true;
        let page = &mut self.mailbox.records_page;
        page.card = None;
        page.asking = false;
        page.export_preview = None;
        page.pending_refresh = true;
    }

    /// The Archive lens owns the keyboard (its ledger, not a reader).
    pub(crate) fn records_list_focused(&self) -> bool {
        self.screen == Screen::Mailbox
            && self.mailbox.mailbox_view == MailboxView::ArchiveMode
            && self.mailbox.active_pane == ActivePane::MailList
    }

    /// The ledger row under the cursor.
    pub fn selected_record(&self) -> Option<&RecordData> {
        self.mailbox
            .records_page
            .rows()
            .get(self.mailbox.selected_index)
    }

    /// What the keys act on: the open card, else the answer, else the row.
    pub(crate) fn focused_record(&self) -> Option<&RecordData> {
        let page = &self.mailbox.records_page;
        page.card
            .as_ref()
            .or_else(|| page.answer_record())
            .or_else(|| self.selected_record())
    }

    /// The runtime fetched the ledger and the guide.
    pub(crate) fn set_records_ledger(
        &mut self,
        ledger: mxr_protocol::RecordLedgerData,
        guide: Option<mxr_protocol::ModeGuideData>,
    ) {
        let page = &mut self.mailbox.records_page;
        page.ledger = Some(ledger);
        if guide.is_some() {
            page.guide = guide;
        }
        if self.mailbox.mailbox_view == MailboxView::ArchiveMode {
            self.mailbox.selected_index = self
                .mailbox
                .selected_index
                .min(self.mailbox.records_page.row_count().saturating_sub(1));
        }
    }

    pub(crate) fn set_records_answer(&mut self, answer: mxr_protocol::RecordAnswerData) {
        // A slow answer to a query the box has moved on from would show
        // one query's records under another's words.
        let asked = self.mailbox.records_page.query.trim();
        if !asked.is_empty() && answer.query != asked {
            return;
        }
        if answer.answer.is_some() {
            // Asking is the mode's main verb: it retires the card.
            self.retire_records_card();
        }
        let page = &mut self.mailbox.records_page;
        page.card = None;
        // A list starts on its best match; leaving one puts the cursor back
        // in the ledger's range.
        let top = answer.list.as_ref().map(|list| {
            list.records
                .iter()
                .position(|record| record.id == list.top_record_id)
                .unwrap_or(0)
        });
        let was_list = page.listed_matches().is_some();
        page.answer = Some(answer);
        if let Some(top) = top {
            self.mailbox.selected_index = top;
            self.mailbox.scroll_offset = 0;
        } else if was_list {
            self.mailbox.selected_index = 0;
            self.mailbox.scroll_offset = 0;
        }
    }

    /// `a` on an answer: every match as a list, so none is out of sight.
    fn show_all_matches(&mut self) {
        let page = &mut self.mailbox.records_page;
        let Some(answer) = &page.answer else {
            return;
        };
        if answer.list.is_some() || answer.matching < 2 {
            return;
        }
        page.answer_list = true;
        page.pending_answer = Some(answer.query.clone());
        self.status_message = Some(format!("Listing all {} matches…", answer.matching));
    }

    pub(crate) fn set_record_card(&mut self, record: RecordData) {
        self.mailbox.records_page.card = Some(record);
    }

    pub(crate) fn refresh_records(&mut self) {
        if self.mailbox.mailbox_view != MailboxView::ArchiveMode {
            return;
        }
        let page = &mut self.mailbox.records_page;
        page.pending_refresh = true;
        if let Some(card) = &page.card {
            page.pending_card = Some(card.id.clone());
        }
        if let Some(query) = page.answer.as_ref().map(|answer| answer.query.clone()) {
            page.pending_answer = Some(query);
        }
    }

    fn copy_record_field(&mut self, amount: bool) {
        let Some(record) = self.focused_record() else {
            return;
        };
        let (label, text) = if amount {
            match &record.amount {
                Some(found) => (found.display.clone(), plain_amount(found.minor)),
                None => {
                    self.status_message = Some("This record has no amount to copy".into());
                    return;
                }
            }
        } else {
            match &record.reference {
                Some(reference) => (reference.clone(), reference.clone()),
                None => {
                    self.status_message = Some("This record has no reference to copy".into());
                    return;
                }
            }
        };
        copy_to_clipboard(&text);
        // Say what was copied, so it can be read out to a call centre.
        self.status_message = Some(format!("{} copied", crate::ui::sanitize::one_line(&label)));
        self.mailbox.records_page.last_copied = Some(text);
    }

    fn open_record_document(&mut self) {
        let Some(record) = self.focused_record().cloned() else {
            return;
        };
        match &record.pdf {
            Some(pdf) => {
                let Ok(attachment_id) = pdf.attachment_id.parse() else {
                    self.status_message = Some("That document can't be opened".into());
                    return;
                };
                self.mailbox.pending_attachment_action = Some(PendingAttachmentAction {
                    message_id: pdf.message_id.clone(),
                    attachment_id,
                    operation: AttachmentOperation::Open,
                    destination: None,
                });
                self.status_message = Some(format!(
                    "Opening {}...",
                    crate::ui::sanitize::one_line(&pdf.filename)
                ));
            }
            // No PDF: the card is the record.
            None => self.mailbox.records_page.pending_card = Some(record.id),
        }
    }

    fn open_record_email(&mut self) {
        let Some(record) = self.focused_record() else {
            return;
        };
        match record.message_id.clone() {
            Some(message_id) => {
                self.mailbox.records_page.pending_open = Some(message_id);
                self.status_message = Some("Opening the email…".into());
            }
            None => self.status_message = Some("This record has no email left".into()),
        }
    }

    /// Called once `GetEnvelope` answers for a record's email.
    pub(crate) fn open_record_envelope(&mut self, env: mxr_core::types::Envelope) {
        self.open_todo_envelope(env, None);
    }

    fn records_issuer_page(&mut self) {
        let Some(issuer) = self.focused_record().and_then(|r| r.issuer.clone()) else {
            self.status_message = Some("This record has no issuer".into());
            return;
        };
        let page = &mut self.mailbox.records_page;
        page.filter.issuer = Some(issuer);
        self.reload_records_ledger();
    }

    fn step_records_year(&mut self, step: i32) {
        let page = &self.mailbox.records_page;
        let years: Vec<i32> = page
            .ledger
            .as_ref()
            .map(|ledger| {
                ledger
                    .facets
                    .years
                    .iter()
                    .filter_map(|year| year.value.parse().ok())
                    .collect()
            })
            .unwrap_or_default();
        let Some(current) = page.current_year() else {
            return;
        };
        // Step to the next year that has records, so a gap never shows an
        // empty ledger.
        let next = if step < 0 {
            years.iter().filter(|year| **year < current).max().copied()
        } else {
            years.iter().filter(|year| **year > current).min().copied()
        };
        match next {
            Some(year) => {
                self.mailbox.records_page.filter.year = Some(year);
                self.reload_records_ledger();
            }
            None if step > 0 && page.filter.year.is_some() => {
                // Past the newest year: back to every year.
                self.mailbox.records_page.filter.year = None;
                self.reload_records_ledger();
            }
            None => {
                self.status_message = Some(if step < 0 {
                    "No older records".into()
                } else {
                    "No newer records".into()
                });
            }
        }
    }

    fn next_record_kind(&mut self) {
        let page = &mut self.mailbox.records_page;
        page.kind_chip = (page.kind_chip + 1) % RECORD_KIND_CHIPS.len();
        page.filter.kinds = RECORD_KIND_CHIPS[page.kind_chip].1.to_vec();
        self.reload_records_ledger();
    }

    fn reload_records_ledger(&mut self) {
        let page = &mut self.mailbox.records_page;
        page.pending_refresh = true;
        page.card = None;
        page.answer = None;
        page.answer_list = false;
        self.mailbox.selected_index = 0;
        self.mailbox.scroll_offset = 0;
    }

    /// Esc: the card, then the answer, then the issuer page and year, then
    /// the first-encounter card.
    fn records_back(&mut self) {
        let page = &mut self.mailbox.records_page;
        if page.card.take().is_some() {
            return;
        }
        if let Some(answer) = page.answer.take() {
            page.query.clear();
            page.answer_list = false;
            // The cursor was on the list's rows; the ledger starts again.
            if answer.list.is_some() {
                self.mailbox.selected_index = 0;
                self.mailbox.scroll_offset = 0;
            }
            return;
        }
        if page.filter.issuer.is_some() || page.filter.year.is_some() {
            page.filter.issuer = None;
            page.filter.year = None;
            self.reload_records_ledger();
            return;
        }
        if page.card_visible() {
            self.retire_records_card();
        }
    }

    /// Retire the first-encounter card here and in every other client.
    fn retire_records_card(&mut self) {
        let page = &mut self.mailbox.records_page;
        if !page.card_visible() {
            return;
        }
        page.card_closed = true;
        let id = self.queue_best_effort_mutation(
            Request::SetModeGuideSeen {
                mode: ARCHIVE_MODE.into(),
                seen: true,
            },
            MutationEffect::StatusOnly(String::new()),
            String::new(),
        );
        self.mailbox.records_page.card_close_mutation = Some(id);
    }

    pub(crate) fn reopen_records_card_after_failure(&mut self, failed: crate::app::MutationId) {
        let page = &mut self.mailbox.records_page;
        if page.card_close_mutation == Some(failed) {
            page.card_close_mutation = None;
            page.card_closed = false;
        }
    }

    fn open_record_fix_prompt(&mut self) {
        let Some(record) = self.focused_record() else {
            return;
        };
        self.mailbox.records_page.prompt = Some(RecordFixPrompt {
            record_id: record.id.clone(),
            input: String::new(),
            previewed: None,
            preview: None,
            error: None,
        });
    }

    fn mark_record_checked(&mut self) {
        let Some(record) = self.focused_record().cloned() else {
            return;
        };
        if record.checked {
            self.status_message = Some("Already checked".into());
            return;
        }
        self.queue_mutation(
            Request::SetRecordField {
                record_id: record.id,
                edit: RecordEditData::ConfirmAll,
                apply_to_sender: false,
                dry_run: false,
            },
            MutationEffect::Records(String::new()),
            "Marking checked...".into(),
        );
    }

    fn dismiss_record(&mut self) {
        let Some(record) = self.focused_record().cloned() else {
            return;
        };
        // The row leaves at once; the daemon's answer offers `u`.
        let page = &mut self.mailbox.records_page;
        if let Some(ledger) = page.ledger.as_mut() {
            ledger.records.retain(|row| row.id != record.id);
        }
        page.card = None;
        if page.answer_record().is_some_and(|r| r.id == record.id) {
            page.answer = None;
        }
        self.mailbox.selected_index = self
            .mailbox
            .selected_index
            .min(self.mailbox.records_page.row_count().saturating_sub(1));
        self.queue_mutation(
            Request::DismissRecord {
                record_ids: vec![record.id],
                restore: false,
                dry_run: false,
            },
            MutationEffect::Records(String::new()),
            "Taking it out of Archive...".into(),
        );
    }

    fn make_todo_from_record(&mut self) {
        let Some(record) = self.focused_record() else {
            return;
        };
        let Some(message_id) = record.message_id.clone() else {
            self.status_message = Some("This record has no email to make a to-do from".into());
            return;
        };
        let what = record
            .title
            .clone()
            .or_else(|| record.issuer.clone())
            .unwrap_or_default();
        let lead = match record.kind {
            mxr_protocol::RecordKindData::Warranty | mxr_protocol::RecordKindData::Order => {
                "Claim warranty for"
            }
            _ => "Follow up on",
        };
        self.mailbox.todo_page.prompt = Some(crate::app::TodoPromptState::new(
            crate::app::TodoPromptKind::Create { message_id },
            format!("{lead} {}", crate::ui::sanitize::one_line(&what)),
        ));
    }

    /// `u` for an Archive change, as the daemon described its reversal.
    pub(super) fn undo_records(&mut self, undo: RecordUndoData, status: String) {
        match undo.kind.as_str() {
            "restore" | "dismiss" => {
                self.queue_mutation(
                    Request::DismissRecord {
                        record_ids: undo.record_ids,
                        restore: undo.kind == "restore",
                        dry_run: false,
                    },
                    MutationEffect::Records(String::new()),
                    status,
                );
            }
            "clear_fields" => {
                for record_id in &undo.record_ids {
                    for field in &undo.fields {
                        self.queue_mutation(
                            Request::SetRecordField {
                                record_id: record_id.clone(),
                                edit: RecordEditData::Clear {
                                    field: field.clone(),
                                },
                                apply_to_sender: false,
                                dry_run: false,
                            },
                            MutationEffect::Records(String::new()),
                            status.clone(),
                        );
                    }
                }
            }
            _ => self.status_message = Some("That change can't be undone here".into()),
        }
    }

    /// `T` on a conversation: the pass menu for the message in view.
    fn open_pass_menu(&mut self) {
        let Some(message_id) = self.context_envelope().map(|envelope| envelope.id.clone()) else {
            self.status_message = Some("Open or select an email to pass it to a mode".into());
            return;
        };
        self.mailbox.records_page.pass_menu = Some(PassMenu {
            message_id,
            preview: None,
        });
    }

    /// Keys while the pass menu is open: `a` or Enter asks what filing
    /// would make; with that card on screen, Enter files it.
    pub(super) fn pass_menu_key(&mut self, code: KeyCode) {
        let Some(menu) = self.mailbox.records_page.pass_menu.as_mut() else {
            return;
        };
        match code {
            KeyCode::Esc => self.mailbox.records_page.pass_menu = None,
            KeyCode::Enter if menu.preview.is_some() => {
                let message_id = menu.message_id.clone();
                self.mailbox.records_page.pass_menu = None;
                self.queue_mutation(
                    Request::FileRecord {
                        message_id,
                        kind: None,
                        dry_run: false,
                    },
                    MutationEffect::Records(String::new()),
                    "Filing in Archive...".into(),
                );
            }
            KeyCode::Enter | KeyCode::Char('a') if menu.preview.is_none() => {
                self.mailbox.records_page.pending_file_preview = Some(menu.message_id.clone());
                self.status_message = Some("Reading the email…".into());
            }
            _ => {}
        }
    }

    pub(crate) fn show_file_preview(&mut self, preview: mxr_protocol::RecordChangeData) {
        self.status_message = None;
        if let Some(menu) = self.mailbox.records_page.pass_menu.as_mut() {
            menu.preview = Some(preview);
        }
    }

    /// Keys while the `,` prompt is open: typing, Enter to preview, Enter
    /// again to apply what was previewed, Esc to cancel.
    pub(super) fn records_prompt_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        let Some(prompt) = self.mailbox.records_page.prompt.as_mut() else {
            return;
        };
        match code {
            KeyCode::Esc => self.mailbox.records_page.prompt = None,
            KeyCode::Enter => {
                let Some((field, value)) = prompt
                    .input
                    .split_once('=')
                    .filter(|(field, _)| !field.trim().is_empty())
                else {
                    prompt.error = Some(
                        "Type field=value: amount, date, reference, issuer, title, kind, return_by or warranty_until"
                            .into(),
                    );
                    return;
                };
                let edit = RecordEditData::Set {
                    field: field.trim().to_string(),
                    value: value.trim().to_string(),
                };
                if prompt.preview.is_some() && prompt.previewed.as_ref() == Some(&edit) {
                    let record_id = prompt.record_id.clone();
                    self.mailbox.records_page.prompt = None;
                    self.queue_mutation(
                        Request::SetRecordField {
                            record_id,
                            edit,
                            apply_to_sender: false,
                            dry_run: false,
                        },
                        MutationEffect::Records(String::new()),
                        "Saving...".into(),
                    );
                } else {
                    prompt.previewed = Some(edit.clone());
                    prompt.preview = None;
                    prompt.error = None;
                    self.mailbox.records_page.pending_fix_preview =
                        Some((prompt.record_id.clone(), edit));
                }
            }
            KeyCode::Backspace => {
                prompt.input.pop();
                prompt.preview = None;
                prompt.error = None;
            }
            KeyCode::Char(c)
                if !modifiers.contains(KeyModifiers::CONTROL)
                    && !modifiers.contains(KeyModifiers::ALT) =>
            {
                prompt.input.push(c);
                prompt.preview = None;
                prompt.error = None;
            }
            _ => {}
        }
    }

    pub(crate) fn show_fix_preview(&mut self, preview: mxr_protocol::RecordChangeData) {
        if let Some(prompt) = self.mailbox.records_page.prompt.as_mut() {
            prompt.preview = Some(preview);
        }
    }

    pub(crate) fn show_fix_error(&mut self, error: String) {
        if let Some(prompt) = self.mailbox.records_page.prompt.as_mut() {
            prompt.previewed = None;
            prompt.error = Some(error);
        }
    }

    /// The export's dry run, or what the export wrote.
    pub(crate) fn show_export(&mut self, export: mxr_protocol::RecordExportData) {
        self.status_message = None;
        if export.dry_run {
            self.mailbox.records_page.export_preview = Some(export);
            return;
        }
        let Some(csv) = export.csv.as_deref() else {
            self.status_message = Some("The export came back empty".into());
            return;
        };
        let dir = dirs::download_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(std::env::temp_dir);
        let path = dir.join(format!(
            "mxr-records-{}.csv",
            chrono::Local::now().format("%Y-%m-%d")
        ));
        match std::fs::write(&path, csv) {
            Ok(()) => self.push_toast(Toast::success(format!(
                "Exported {} to {}",
                crate::ui::sanitize::one_line(&export.summary),
                path.display()
            ))),
            Err(error) => {
                self.status_message = Some(format!("Couldn't write {}: {error}", path.display()));
            }
        }
    }

    /// Keys while the ask box takes input.
    fn records_ask_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        let page = &mut self.mailbox.records_page;
        match key.code {
            KeyCode::Esc => page.asking = false,
            KeyCode::Enter => {
                page.asking = false;
                let query = page.query.trim().to_string();
                // A new query decides for itself between answer and list.
                page.answer_list = false;
                if query.is_empty() {
                    page.answer = None;
                } else {
                    page.pending_answer = Some(query);
                    self.status_message = Some("Asking Archive…".into());
                }
            }
            KeyCode::Backspace => {
                page.query.pop();
            }
            KeyCode::Char(c)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                page.query.push(c);
            }
            _ => {}
        }
        None
    }

    /// Key handling for the Archive lens. Only the mode's verbs and
    /// navigation reach it: the ledger holds records, not emails.
    pub(super) fn records_lens_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        if self.mailbox.records_page.asking {
            return self.records_ask_key(key);
        }
        if self.mailbox.records_page.export_preview.is_some() {
            return match key.code {
                KeyCode::Enter | KeyCode::Char('y') => {
                    let page = &mut self.mailbox.records_page;
                    page.export_preview = None;
                    page.pending_export = Some(true);
                    self.status_message = Some("Exporting…".into());
                    None
                }
                KeyCode::Esc | KeyCode::Char('n') => {
                    self.mailbox.records_page.export_preview = None;
                    None
                }
                _ => None,
            };
        }
        let plain = key.modifiers == KeyModifiers::NONE;
        let shifted = plain_or_shift(key.modifiers);
        let action = match key.code {
            KeyCode::Char('/') if plain => Some(Action::RecordsAsk),
            KeyCode::Char('a') if plain && self.mailbox.records_page.answer.is_some() => {
                self.show_all_matches();
                return None;
            }
            KeyCode::Char('h') | KeyCode::Left if plain => {
                self.mailbox.active_pane = ActivePane::Sidebar;
                return None;
            }
            KeyCode::Esc => Some(Action::RecordsBack),
            KeyCode::Enter => Some(Action::RecordsOpenDocument),
            KeyCode::Right if plain => Some(Action::RecordsOpenCard),
            KeyCode::Char('y') if plain => Some(Action::RecordsCopyReference),
            KeyCode::Char('Y') if shifted => Some(Action::RecordsCopyAmount),
            KeyCode::Char('o' | 'e') if plain => Some(Action::RecordsOpenEmail),
            KeyCode::Char('p') if plain => Some(Action::RecordsIssuerPage),
            KeyCode::Char('[') => Some(Action::RecordsPrevYear),
            KeyCode::Char(']') => Some(Action::RecordsNextYear),
            KeyCode::Char(',') => Some(Action::RecordsFix),
            KeyCode::Char('v') if plain => Some(Action::RecordsMarkChecked),
            KeyCode::Char('X') if shifted => Some(Action::RecordsDismiss),
            KeyCode::Char('E') if shifted => Some(Action::RecordsExport),
            KeyCode::Char('t') if plain => Some(Action::RecordsMakeTodo),
            KeyCode::Char('u') if plain => Some(Action::UndoLastMutation),
            _ => None,
        };
        if action.is_some() {
            return action;
        }
        self.contextual_input_action(key).filter(|action| {
            !matches!(
                action,
                Action::OpenSelected
                    | Action::ToggleSelect
                    | Action::VisualLineMode
                    | Action::AttachmentList
                    | Action::ExportThread
                    | Action::ToggleFullscreen
                    | Action::FlagReplyLater
                    | Action::NextSearchResult
                    | Action::PrevSearchResult
                    | Action::SwitchPane
                    | Action::Back
                    | Action::RespondInvite(_)
                    | Action::RespondInviteWithComment(_)
            )
        })
    }
}
