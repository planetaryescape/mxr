//! Sorting shows its work in the TUI (D119): `X` moves one email to a
//! mode, `K` sets the sender's mode, Now's Not-sure questions take one key,
//! "Always for this sender?" follows a move, and Enter on the arrivals line
//! lists the emails behind it. Every move is `MoveMessage`, so the daemon
//! stores the correction and `u` undoes it exactly (`UndoMove`).

use super::input::plain_or_shift;
use super::*;
use crate::app::state::{
    ArrivalsListFetch, ArrivalsListState, MoveMenu, NowRow, SenderAsk, UpdatesRow,
};
use mxr_protocol::{
    ArrivalBucketData, ArrivalItemData, ArrivalListData, ArrivalsData, ModeKindData,
    MoveOutcomeData,
};

/// The move menu's keys: the mode's `g` letter. Shift on m, u and r sets
/// the sender's mode instead.
pub(crate) const MOVE_CHOICES: [(char, ModeKindData); 5] = [
    ('m', ModeKindData::Messages),
    ('x', ModeKindData::Todo),
    ('u', ModeKindData::Updates),
    ('r', ModeKindData::Reading),
    ('e', ModeKindData::Archive),
];

/// A sender's mail can only go to Messages, Updates or Reading; To do and
/// Archive add one email there.
pub(crate) const fn sender_mode(mode: ModeKindData) -> bool {
    matches!(
        mode,
        ModeKindData::Messages | ModeKindData::Updates | ModeKindData::Reading
    )
}

/// The toast after a move: the daemon's copy, then its sender question.
pub(crate) fn move_status(outcome: &MoveOutcomeData, not_sure: bool) -> String {
    let copy = crate::ui::sanitize::one_line(&outcome.copy);
    if not_sure && !outcome.sender && sender_mode(outcome.to) {
        return format!("{copy}  Always for this sender? y/n");
    }
    match &outcome.ask_sender {
        Some(ask) => format!("{copy}  {}", crate::ui::sanitize::one_line(ask)),
        None => copy,
    }
}

/// Inbox's quiet mode word for a row: "Reading", "Spam".
pub(crate) fn chip_label(item: &ArrivalItemData) -> &'static str {
    match item.bucket.mode() {
        Some(mode) => mode.name(),
        None => match item.bucket {
            ArrivalBucketData::Spam => "Spam",
            ArrivalBucketData::ScreenedOut => "Screened out",
            _ => "Sorting",
        },
    }
}

impl App {
    pub(super) fn apply_move_action(&mut self, action: Action) {
        match action {
            Action::OpenMoveMenu => self.open_move_menu(false),
            Action::OpenSenderMoveMenu => self.open_sender_move_menu(),
            Action::MoveEmailTo { mode, sender } => self.move_from_menu(mode, sender),
            Action::NowAnswerNotSure(mode) => self.answer_not_sure(mode),
            Action::AnswerAlwaysForSender(yes) => self.answer_always_for_sender(yes),
            Action::OpenArrivals => self.open_arrivals_list(None),
            Action::ArrivalsNextBucket => {
                let next = self
                    .mailbox
                    .trust
                    .arrivals_list
                    .as_ref()
                    .map(ArrivalsListState::next_bucket);
                if let Some(next) = next {
                    self.open_arrivals_list(next);
                }
            }
            Action::CloseArrivals => self.mailbox.trust.arrivals_list = None,
            _ => {}
        }
    }

    /// The email a move acts on, and who sent it, wherever the cursor is.
    fn move_target(&self) -> Option<(MessageId, String)> {
        let name = |name: Option<&String>, email: &str| {
            name.filter(|name| !name.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| email.to_string())
        };
        if let Some(item) = self
            .mailbox
            .trust
            .arrivals_list
            .as_ref()
            .and_then(ArrivalsListState::selected_item)
        {
            return Some((
                item.message_id.clone(),
                name(item.sender_name.as_ref(), &item.sender_email),
            ));
        }
        if self.now_list_focused() {
            return match self.selected_now_row()? {
                NowRow::NotSure(question) => Some((
                    question.message_id.clone(),
                    name(question.sender_name.as_ref(), &question.sender_email),
                )),
                NowRow::Person(person) => Some((
                    person.row.message_id.clone(),
                    name(
                        person.row.counterparty_name.as_ref(),
                        &person.row.counterparty_email,
                    ),
                )),
                NowRow::Reading(pick) => Some((
                    pick.message_id.clone(),
                    name(pick.sender_name.as_ref(), &pick.sender_email),
                )),
                NowRow::Todo(todo) => todo.todo.source_message_id.clone().map(|id| {
                    (
                        id,
                        todo.todo
                            .counterparty
                            .clone()
                            .unwrap_or_else(|| "this email".into()),
                    )
                }),
                NowRow::Arrivals(_) | NowRow::Updates(_) => None,
            };
        }
        if self.updates_list_focused() {
            // A source line stands for its newest email.
            let UpdatesRow::Line(line) = self.selected_updates_row()? else {
                return None;
            };
            let message_id = line
                .latest_message_id
                .clone()
                .or_else(|| line.fact_message_id.clone())?;
            return Some((message_id, line.source_name.clone()));
        }
        if self.reading_lens_focused() {
            let item = self.selected_reading_row()?.issue();
            return Some((
                item.message_id.clone(),
                if item.source.trim().is_empty() {
                    item.sender_email.clone()
                } else {
                    item.source.clone()
                },
            ));
        }
        if self.messages_list_focused() {
            return self
                .selected_messages_topic()
                .map(|topic| (topic.reply_to, topic.who));
        }
        if self.place_list_focused() {
            return self.selected_place_row().map(|(bundle, message)| {
                (
                    message.message_id.clone(),
                    name(bundle.sender_name.as_ref(), &bundle.sender_email),
                )
            });
        }
        self.context_envelope().map(|envelope| {
            (
                envelope.id.clone(),
                name(envelope.from.name.as_ref(), &envelope.from.email),
            )
        })
    }

    fn open_move_menu(&mut self, sender_only: bool) {
        let Some((message_id, display)) = self.move_target() else {
            self.status_message = Some(
                if self.now_list_focused() {
                    "This row has no single email to move"
                } else {
                    "Select an email to move"
                }
                .into(),
            );
            return;
        };
        let not_sure =
            self.now_list_focused() && matches!(self.selected_now_row(), Some(NowRow::NotSure(_)));
        self.mailbox.trust.move_menu = Some(MoveMenu {
            message_id,
            display,
            sender_only,
            not_sure,
        });
    }

    /// `K`: answers a move's "Always for this sender?" while it shows,
    /// else opens the menu for the sender's mode.
    fn open_sender_move_menu(&mut self) {
        if let Some(ask) = self.mailbox.trust.sender_ask.take() {
            self.queue_move(ask.message_id, ask.mode, true, false);
            return;
        }
        self.open_move_menu(true);
    }

    fn move_from_menu(&mut self, mode: ModeKindData, sender: bool) {
        let Some(menu) = self.mailbox.trust.move_menu.take() else {
            return;
        };
        let sender = sender || menu.sender_only;
        if sender && !sender_mode(mode) {
            self.status_message =
                Some("A sender's mail can go to Messages, Updates or Reading".into());
            return;
        }
        if menu.not_sure {
            self.mailbox.now_page.remove_not_sure(&menu.message_id);
            self.clamp_now_selection();
            if !sender && sender_mode(mode) {
                self.ask_always_for_sender(menu.message_id.clone(), mode);
            }
        }
        self.queue_move(menu.message_id, mode, sender, menu.not_sure);
    }

    /// One key on a Not-sure question: where it goes, stored as the answer.
    fn answer_not_sure(&mut self, mode: ModeKindData) {
        let Some(NowRow::NotSure(question)) = self.selected_now_row() else {
            return;
        };
        let message_id = question.message_id.clone();
        self.mailbox.now_page.remove_not_sure(&message_id);
        self.clamp_now_selection();
        if sender_mode(mode) {
            self.ask_always_for_sender(message_id.clone(), mode);
        }
        self.queue_move(message_id, mode, false, true);
    }

    /// Asked once, in the status line, after a Not-sure answer.
    fn ask_always_for_sender(&mut self, message_id: MessageId, mode: ModeKindData) {
        self.mailbox.trust.sender_ask = Some(SenderAsk {
            message_id,
            mode,
            yes_no: true,
        });
    }

    fn answer_always_for_sender(&mut self, yes: bool) {
        let Some(ask) = self.mailbox.trust.sender_ask.take() else {
            return;
        };
        if yes {
            self.queue_move(ask.message_id, ask.mode, true, false);
        } else {
            self.status_message = Some("Just this one".into());
        }
    }

    fn queue_move(
        &mut self,
        message_id: MessageId,
        mode: ModeKindData,
        sender: bool,
        not_sure: bool,
    ) {
        let status = if sender {
            format!("Moving the sender to {}...", mode.name())
        } else {
            format!("Moving to {}...", mode.name())
        };
        // The runner replaces the effect's words with the daemon's copy.
        self.queue_mutation(
            Request::MoveMessage {
                message_id,
                mode,
                sender,
                dry_run: false,
                source: not_sure.then(|| "not_sure".to_string()),
            },
            MutationEffect::ModeDone(String::new()),
            status,
        );
    }

    fn clamp_now_selection(&mut self) {
        if self.mailbox.mailbox_view == MailboxView::Now {
            self.mailbox.selected_index = self
                .mailbox
                .selected_index
                .min(self.mailbox.now_page.row_count().saturating_sub(1));
        }
    }

    /// A move landed: remember its sender question while it shows, and
    /// teach `X` and `K` on the first one.
    pub(crate) fn after_move(&mut self, outcome: &MoveOutcomeData, not_sure: bool) {
        if outcome.ask_sender.is_some() && !not_sure {
            self.mailbox.trust.sender_ask = Some(SenderAsk {
                message_id: outcome.message_id.clone(),
                mode: outcome.to,
                yes_no: false,
            });
        }
        if let Some(hint) = &outcome.hint {
            if !self.mailbox.trust.move_hint_shown {
                self.mailbox.trust.move_hint_shown = true;
                self.push_toast(Toast::info(crate::ui::sanitize::one_line(hint)));
            }
        }
        self.mailbox.trust.clear_chips();
        self.refetch_arrivals_list();
    }

    /// The runtime fetched the arrivals line.
    pub(crate) fn set_arrivals(&mut self, arrivals: ArrivalsData) {
        let before = self.mailbox.now_page.arrivals_rows().len();
        let first_question = !arrivals.not_sure.is_empty();
        let hint = arrivals.not_sure_hint.clone();
        self.mailbox.now_page.arrivals = Some(arrivals);
        let after = self.mailbox.now_page.arrivals_rows().len();
        // Keep the cursor on the row it was on as the line's rows come and go.
        if self.mailbox.mailbox_view == MailboxView::Now && self.mailbox.selected_index >= before {
            self.mailbox.selected_index =
                (self.mailbox.selected_index + after).saturating_sub(before);
        }
        self.clamp_now_selection();
        if first_question && !self.mailbox.trust.not_sure_hint_shown {
            if let Some(hint) = hint {
                self.mailbox.trust.not_sure_hint_shown = true;
                self.status_message = Some(crate::ui::sanitize::one_line(&hint));
            }
        }
    }

    /// The open arrivals list asks again, for the bucket it shows.
    fn refetch_arrivals_list(&mut self) {
        let bucket = self
            .mailbox
            .trust
            .arrivals_list
            .as_ref()
            .map(|list| list.bucket);
        if let Some(bucket) = bucket {
            self.open_arrivals_list(bucket);
        }
    }

    /// The emails behind the line, one bucket at a time.
    fn open_arrivals_list(&mut self, bucket: Option<ArrivalBucketData>) {
        let Some(arrivals) = self.mailbox.now_page.arrivals.as_ref() else {
            return;
        };
        let (since, until) = match &self.mailbox.trust.arrivals_list {
            Some(list) => (list.since, list.until),
            None => (arrivals.since, arrivals.until),
        };
        let buckets = arrivals
            .counts
            .iter()
            .chain(&arrivals.also)
            .map(|count| count.bucket)
            .collect();
        self.mailbox.trust.arrivals_list = Some(ArrivalsListState {
            since,
            until,
            buckets,
            bucket,
            list: None,
            selected: 0,
        });
        self.mailbox.trust.pending_list = Some(ArrivalsListFetch {
            since,
            until,
            bucket,
        });
    }

    /// A list came back; one for a bucket no longer shown is dropped.
    pub(crate) fn set_arrivals_list(&mut self, fetch: &ArrivalsListFetch, list: ArrivalListData) {
        let Some(state) = self.mailbox.trust.arrivals_list.as_mut() else {
            return;
        };
        if state.bucket != fetch.bucket {
            return;
        }
        state.selected = state.selected.min(list.items.len().saturating_sub(1));
        state.list = Some(list);
    }

    /// Ask for the chips of the mail list rows on screen. Cheap when
    /// nothing changed: the window is only re-read when it moves.
    pub(crate) fn request_visible_chips(&mut self) {
        if self.screen != Screen::Mailbox
            || self.search.active
            || self.mailbox.mailbox_view != MailboxView::Messages
        {
            return;
        }
        let window = (
            self.mailbox.envelopes.len(),
            self.mailbox.scroll_offset,
            self.mailbox.mail_list_mode == MailListMode::Threads,
        );
        if self.mailbox.trust.chip_window == Some(window) {
            return;
        }
        let rows = self.mail_list_rows();
        let ids: Vec<MessageId> = rows
            .iter()
            .skip(self.mailbox.scroll_offset)
            .take(super::state::CHIP_BATCH)
            .map(|row| row.representative.id.clone())
            .collect();
        self.mailbox.trust.want_chips(ids.iter());
        if self.mailbox.trust.pending_chips.is_some()
            || ids
                .iter()
                .all(|id| self.mailbox.trust.chips_requested.contains(id))
        {
            self.mailbox.trust.chip_window = Some(window);
        }
    }

    /// The full chip of the selected Inbox row, for the list's title.
    pub(crate) fn selected_mode_chip(&self) -> Option<&str> {
        if self.mailbox.mailbox_view != MailboxView::Messages {
            return None;
        }
        let row = self.selected_mail_row()?;
        self.mailbox
            .trust
            .chips
            .get(&row.representative.id)
            .map(|item| item.chip.as_str())
    }

    /// Where mail sits changed in some client: refetch what shows it.
    pub(crate) fn modes_changed(&mut self) {
        self.mailbox.trust.clear_chips();
        self.refresh_now();
        match self.mailbox.mailbox_view {
            MailboxView::People => self.refresh_messages(),
            MailboxView::Place(_) | MailboxView::Desk => self.refresh_places(),
            MailboxView::Todo => self.refresh_todo(),
            MailboxView::Updates => self.refresh_updates(),
            MailboxView::Reading => self.refresh_reading(),
            _ => {}
        }
        self.refetch_arrivals_list();
    }

    /// Keys while the move menu is open: one per mode, Shift for the
    /// sender, Esc closes. Everything else is swallowed.
    pub(super) fn move_menu_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        let menu = self.mailbox.trust.move_menu.as_ref()?;
        let sender_only = menu.sender_only;
        match key.code {
            KeyCode::Esc => {
                self.mailbox.trust.move_menu = None;
                None
            }
            KeyCode::Char(c) if plain_or_shift(key.modifiers) => {
                let shifted = c.is_ascii_uppercase();
                let lower = c.to_ascii_lowercase();
                let (_, mode) = MOVE_CHOICES.iter().find(|(letter, _)| *letter == lower)?;
                if (shifted || sender_only) && !sender_mode(*mode) {
                    return None;
                }
                Some(Action::MoveEmailTo {
                    mode: *mode,
                    sender: shifted,
                })
            }
            _ => None,
        }
    }

    /// Keys while the arrivals list is open over Now.
    pub(super) fn arrivals_list_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        let plain = key.modifiers == KeyModifiers::NONE;
        let shifted = plain_or_shift(key.modifiers);
        let list = self.mailbox.trust.arrivals_list.as_mut()?;
        let count = list.list.as_ref().map_or(0, |list| list.items.len());
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => Some(Action::CloseArrivals),
            KeyCode::Tab => Some(Action::ArrivalsNextBucket),
            KeyCode::Char('j') | KeyCode::Down => {
                list.selected = (list.selected + 1).min(count.saturating_sub(1));
                None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                list.selected = list.selected.saturating_sub(1);
                None
            }
            KeyCode::Char('X') if shifted => Some(Action::OpenMoveMenu),
            KeyCode::Char('K') if shifted => Some(Action::OpenSenderMoveMenu),
            KeyCode::Char('u') if plain => Some(Action::UndoLastMutation),
            KeyCode::Enter | KeyCode::Char('o') => {
                let message_id = list.selected_item()?.message_id.clone();
                self.mailbox.trust.arrivals_list = None;
                self.mailbox.pending_invite_open = Some(message_id);
                self.status_message = Some("Opening the email…".into());
                None
            }
            _ => None,
        }
    }

    /// y or n while "Always for this sender? y/n" waits. Any other key
    /// drops the question and does what it always does.
    pub(super) fn sender_ask_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        let asking = self
            .mailbox
            .trust
            .sender_ask
            .as_ref()
            .is_some_and(|ask| ask.yes_no);
        if !asking {
            return None;
        }
        match (key.code, key.modifiers) {
            (KeyCode::Char('y'), KeyModifiers::NONE) => Some(Action::AnswerAlwaysForSender(true)),
            (KeyCode::Char('n'), KeyModifiers::NONE) | (KeyCode::Esc, _) => {
                Some(Action::AnswerAlwaysForSender(false))
            }
            _ => {
                self.mailbox.trust.sender_ask = None;
                None
            }
        }
    }
}
