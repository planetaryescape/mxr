//! Now in the TUI: open the front page, act on a row in its own mode,
//! done here through `SetModeDone` with the daemon's handoff copy, let go
//! of the Updates card after a preview, and answer a new sender's one
//! question. The daemon owns every cap, count and line.

use super::input::plain_or_shift;
use super::*;
use crate::app::state::{NowDigestPreview, NowRow};
use mxr_core::id::ThreadId;
use mxr_protocol::{ModeDoneOutcomeData, ModeKindData, NowData};

/// The mode id Now's guide and its card are kept under.
pub(crate) const NOW_MODE: &str = mxr_protocol::NOW_GUIDE.mode;

/// What a row on Now refers to, copied out so the page can be changed.
enum NowTarget {
    Person {
        thread_id: ThreadId,
        message_id: MessageId,
    },
    Todo {
        todo_id: String,
        thread_id: Option<ThreadId>,
        message_id: Option<MessageId>,
    },
    Updates,
    Reading {
        thread_id: ThreadId,
        message_id: MessageId,
    },
}

impl From<NowRow<'_>> for NowTarget {
    fn from(row: NowRow<'_>) -> Self {
        match row {
            NowRow::Person(person) => Self::Person {
                thread_id: person.row.thread_id.clone(),
                message_id: person.row.message_id.clone(),
            },
            NowRow::Todo(todo) => Self::Todo {
                todo_id: todo.todo.id.clone(),
                thread_id: todo.todo.thread_id.clone(),
                message_id: todo.todo.source_message_id.clone(),
            },
            NowRow::Updates(_) => Self::Updates,
            NowRow::Reading(pick) => Self::Reading {
                thread_id: pick.thread_id.clone(),
                message_id: pick.message_id.clone(),
            },
        }
    }
}

/// The toast for a done-here answer: the daemon's handoff copy for one
/// thread, a count for several.
pub(crate) fn mode_done_copy(items: &[ModeDoneOutcomeData], mode: ModeKindData) -> String {
    let done: Vec<&ModeDoneOutcomeData> =
        items.iter().filter(|item| item.error.is_none()).collect();
    match done.as_slice() {
        [] => String::new(),
        [one] => one.copy.clone(),
        many => {
            let archived = many.iter().filter(|item| item.archived > 0).count();
            let base = format!("Done in {} with {} conversations.", mode.name(), many.len());
            if archived > 0 {
                format!("{base} {archived} archived in {}.", many[0].provider)
            } else {
                base
            }
        }
    }
}

impl App {
    pub(super) fn apply_now_action(&mut self, action: Action) {
        match action {
            Action::OpenNow => self.open_now(),
            Action::OpenMessages => self.open_messages(),
            Action::OpenArchiveMode => self.open_archive_records(),
            Action::NowOpen => self.now_open(false),
            Action::NowOpenEmail => self.now_open(true),
            Action::NowDone => self.now_done(),
            Action::NowLetGoDigest => self.now_let_go_digest(),
            Action::NowCloseCard => self.close_now_card(),
            Action::NowAnswerSender(choice) => self.answer_new_sender(choice),
            _ => {}
        }
    }

    pub(super) fn enter_mode_view(&mut self, view: MailboxView) {
        self.mailbox.mailbox_view = view;
        self.mailbox.active_label = None;
        self.mailbox.pending_active_label = None;
        self.mailbox.pending_label_fetch = None;
        self.mailbox.pending_preview_read = None;
        self.mailbox.desired_system_mailbox = None;
        self.search.active = false;
        self.screen = Screen::Mailbox;
        self.mailbox.active_pane = ActivePane::MailList;
        self.mailbox.layout_mode = LayoutMode::TwoPane;
        self.mailbox.selected_index = 0;
        self.mailbox.scroll_offset = 0;
    }

    fn open_now(&mut self) {
        self.enter_mode_view(MailboxView::Now);
        let page = &mut self.mailbox.now_page;
        page.digest_preview = None;
        page.pending_refresh = true;
        self.mailbox.pending_rail_refresh = true;
    }

    /// The Now lens owns the keyboard (its list, not a reader beside it).
    pub(crate) fn now_list_focused(&self) -> bool {
        self.screen == Screen::Mailbox
            && self.mailbox.mailbox_view == MailboxView::Now
            && self.mailbox.active_pane == ActivePane::MailList
    }

    pub fn selected_now_row(&self) -> Option<NowRow<'_>> {
        self.mailbox
            .now_page
            .rows()
            .get(self.mailbox.selected_index)
            .copied()
    }

    /// The runtime fetched Now and its guide.
    pub(crate) fn set_now(&mut self, now: NowData, guide: Option<mxr_protocol::ModeGuideData>) {
        let page = &mut self.mailbox.now_page;
        page.now = Some(now);
        if guide.is_some() {
            page.guide = guide;
        }
        if self.mailbox.mailbox_view == MailboxView::Now {
            self.mailbox.selected_index = self
                .mailbox
                .selected_index
                .min(self.mailbox.now_page.row_count().saturating_sub(1));
        }
    }

    /// The Updates card's start in local time, for its section rule.
    pub(crate) fn now_updates_since(&self) -> Option<String> {
        self.mailbox
            .now_page
            .now
            .as_ref()
            .and_then(|now| now.updates.as_ref())
            .map(|card| {
                card.since
                    .with_timezone(&chrono::Local)
                    .format("%H:%M")
                    .to_string()
            })
    }

    /// Refetch Now and the rail after anything that can move them.
    pub(crate) fn refresh_now(&mut self) {
        self.mailbox.now_page.pending_refresh = true;
        self.mailbox.pending_rail_refresh = true;
    }

    /// Enter opens the row in its own mode; `o` opens the email itself.
    fn now_open(&mut self, email: bool) {
        let Some(target) = self.selected_now_row().map(NowTarget::from) else {
            return;
        };
        match target {
            NowTarget::Person { message_id, .. } | NowTarget::Reading { message_id, .. } => {
                self.mailbox.pending_invite_open = Some(message_id);
                self.status_message = Some("Opening conversation…".into());
            }
            NowTarget::Todo { message_id, .. } if email => match message_id {
                Some(message_id) => {
                    self.mailbox.todo_page.pending_open = Some(crate::app::TodoOpen {
                        message_id,
                        link: None,
                    });
                    self.status_message = Some("Opening the email…".into());
                }
                None => self.status_message = Some("This to-do has no email to open".into()),
            },
            NowTarget::Todo { .. } => self.apply(Action::OpenTodo),
            NowTarget::Updates => {
                self.apply(Action::OpenPlace(mxr_protocol::MailPlaceData::PaperTrail));
            }
        }
    }

    /// `e`: done here, in the row's own mode. The row leaves at once; the
    /// toast is the daemon's handoff copy and `u` undoes it.
    fn now_done(&mut self) {
        let Some(target) = self.selected_now_row().map(NowTarget::from) else {
            return;
        };
        // The main verb retires Now's card.
        self.retire_now_card();
        match target {
            NowTarget::Person { thread_id, .. } => {
                self.queue_mode_done(ModeKindData::Messages, thread_id);
            }
            NowTarget::Reading { thread_id, .. } => {
                self.queue_mode_done(ModeKindData::Reading, thread_id);
            }
            // Only this to-do: another one on the same email stays open.
            NowTarget::Todo {
                thread_id: Some(thread_id),
                todo_id,
                ..
            } => {
                self.queue_done(ModeKindData::Todo, thread_id, vec![todo_id]);
            }
            // A to-do you made with no email behind it: tick it off.
            NowTarget::Todo { todo_id, .. } => {
                // It leaves at once, so the next row takes the cursor.
                self.mailbox.now_page.remove_todo(&todo_id);
                self.mailbox.selected_index = self
                    .mailbox
                    .selected_index
                    .min(self.mailbox.now_page.row_count().saturating_sub(1));
                self.queue_mutation(
                    Request::SetTodoState {
                        todo_ids: vec![todo_id],
                        action: mxr_protocol::TodoStateActionData::Done,
                        dry_run: false,
                    },
                    MutationEffect::Todo("Ticked off".into()),
                    "Ticking off...".into(),
                );
                self.refresh_now();
            }
            NowTarget::Updates => self.now_let_go_digest(),
        }
    }

    /// Done in `mode` for one thread, taking it off Now and the desk at
    /// once.
    pub(crate) fn queue_mode_done(
        &mut self,
        mode: ModeKindData,
        thread_id: ThreadId,
    ) -> MutationId {
        self.queue_done(mode, thread_id, Vec::new())
    }

    /// `queue_mode_done`, naming the to-dos to tick off in To do.
    fn queue_done(
        &mut self,
        mode: ModeKindData,
        thread_id: ThreadId,
        todo_ids: Vec<String>,
    ) -> MutationId {
        self.mailbox.now_page.remove_thread(&thread_id);
        if self.mailbox.mailbox_view == MailboxView::Now {
            self.mailbox.selected_index = self
                .mailbox
                .selected_index
                .min(self.mailbox.now_page.row_count().saturating_sub(1));
        }
        self.remove_desk_thread(&thread_id);
        self.queue_mutation(
            Request::SetModeDone {
                thread_ids: vec![thread_id],
                mode,
                dry_run: false,
                todo_ids,
                sender: None,
            },
            MutationEffect::ModeDone(format!("Done in {}", mode.name())),
            "Done here...".into(),
        )
    }

    /// `A`: ask the daemon what letting go of the card's threads would do;
    /// with that preview on screen, Enter lets go of exactly those.
    fn now_let_go_digest(&mut self) {
        let page = &mut self.mailbox.now_page;
        if let Some(preview) = page.digest_preview.take() {
            if preview.thread_ids.is_empty() {
                return;
            }
            if let Some(now) = page.now.as_mut() {
                now.updates = None;
            }
            self.mailbox.selected_index = self
                .mailbox
                .selected_index
                .min(self.mailbox.now_page.row_count().saturating_sub(1));
            self.queue_mutation(
                Request::SetModeDone {
                    thread_ids: preview.thread_ids,
                    mode: ModeKindData::Updates,
                    dry_run: false,
                    todo_ids: Vec::new(),
                    sender: None,
                },
                MutationEffect::ModeDone("Let go of the digest".into()),
                "Letting go...".into(),
            );
            return;
        }
        let Some(card) = page.now.as_ref().and_then(|now| now.updates.as_ref()) else {
            self.status_message = Some("No updates on Now to let go of".into());
            return;
        };
        if card.thread_ids.is_empty() {
            return;
        }
        page.pending_digest_preview = Some(card.thread_ids.clone());
        self.status_message = Some("Checking what letting go would change…".into());
    }

    /// The daemon's dry run of letting go of the card.
    pub(crate) fn show_now_digest_preview(
        &mut self,
        thread_ids: Vec<ThreadId>,
        items: Vec<ModeDoneOutcomeData>,
    ) {
        self.status_message = None;
        let thread_ids: Vec<ThreadId> = items
            .iter()
            .filter(|item| item.error.is_none())
            .map(|item| item.thread_id.clone())
            .filter(|thread| thread_ids.contains(thread))
            .collect();
        if thread_ids.is_empty() {
            self.push_toast(Toast::success("Nothing left to let go of"));
            return;
        }
        self.mailbox.now_page.digest_preview = Some(NowDigestPreview { thread_ids, items });
    }

    fn close_now_card(&mut self) {
        if self.mailbox.now_page.card_visible() {
            self.retire_now_card();
        }
    }

    /// Retire Now's first-encounter card here and in every other client.
    fn retire_now_card(&mut self) {
        let page = &mut self.mailbox.now_page;
        if !page.card_visible() {
            return;
        }
        page.card_closed = true;
        let id = self.queue_best_effort_mutation(
            Request::SetModeGuideSeen {
                mode: NOW_MODE.into(),
                seen: true,
            },
            MutationEffect::StatusOnly(String::new()),
            String::new(),
        );
        self.mailbox.now_page.card_close_mutation = Some(id);
    }

    /// The daemon didn't store the closed card: show it again.
    pub(crate) fn reopen_now_card_after_failure(&mut self, failed: crate::app::MutationId) {
        let page = &mut self.mailbox.now_page;
        if page.card_close_mutation == Some(failed) {
            page.card_close_mutation = None;
            page.card_closed = false;
        }
    }

    /// A digit on a new sender's row answers its question with that choice.
    fn answer_new_sender(&mut self, choice: usize) {
        let Some(NowRow::Person(person)) = self.selected_now_row() else {
            return;
        };
        let Some(question) = person.new_sender.as_ref() else {
            return;
        };
        let Some(answer) = choice.checked_sub(1).and_then(|i| question.choices.get(i)) else {
            return;
        };
        let request = Request::SetSenderKind {
            account_id: question.account_id.clone(),
            sender_email: question.sender_email.clone(),
            kind: Some(answer.kind),
        };
        let status = format!(
            "{} goes to {} from now on",
            crate::ui::sanitize::one_line(&question.sender_email),
            answer.label
        );
        self.queue_mutation(
            request,
            MutationEffect::ModeDone(status),
            "Saving...".into(),
        );
    }

    /// Key handling for the Now lens: its verbs and navigation. Mail verbs
    /// need an email, so they open the row first.
    pub(super) fn now_lens_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        if self.mailbox.now_page.digest_preview.is_some() {
            return match key.code {
                KeyCode::Enter | KeyCode::Char('y') => Some(Action::NowLetGoDigest),
                KeyCode::Esc | KeyCode::Char('n') => {
                    self.mailbox.now_page.digest_preview = None;
                    self.status_message = Some("Kept the digest".into());
                    None
                }
                _ => None,
            };
        }
        let plain = key.modifiers == KeyModifiers::NONE;
        let shifted = plain_or_shift(key.modifiers);
        let asks = matches!(
            self.selected_now_row(),
            Some(NowRow::Person(person)) if person.new_sender.is_some()
        );
        let action = match key.code {
            KeyCode::Char('/') if plain => Some(Action::OpenGlobalSearch),
            KeyCode::Char('h') | KeyCode::Left if plain => {
                self.mailbox.active_pane = ActivePane::Sidebar;
                return None;
            }
            // Closes the card when it shows; nothing to go back from here.
            KeyCode::Esc => Some(Action::NowCloseCard),
            KeyCode::Enter => Some(Action::NowOpen),
            KeyCode::Char('e') if plain => Some(Action::NowDone),
            KeyCode::Char('o') if plain => Some(Action::NowOpenEmail),
            KeyCode::Char('r') if plain => Some(Action::Reply),
            KeyCode::Char('t') if plain => Some(Action::CreateTodoFromMessage),
            KeyCode::Char('A') if shifted => Some(Action::NowLetGoDigest),
            KeyCode::Char('u') if plain => Some(Action::UndoLastMutation),
            KeyCode::Char(digit @ '1'..='4') if plain && asks => {
                Some(Action::NowAnswerSender(digit as usize - '0' as usize))
            }
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
                    | Action::Archive
                    | Action::MarkReadAndArchive
                    | Action::Trash
                    | Action::Spam
                    | Action::Star
                    | Action::MarkRead
                    | Action::MarkUnread
                    | Action::RespondInvite(_)
                    | Action::RespondInviteWithComment(_)
            )
        })
    }
}
