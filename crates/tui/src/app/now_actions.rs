//! Now in the TUI: open the front page, act on a row in its own mode,
//! done here through `SetModeDone` with the daemon's handoff copy, let go
//! of the Updates card after a preview, and answer a new sender's one
//! question. The daemon owns every cap, count and line.

use super::*;
use crate::app::state::NowRow;
use mxr_protocol::{ModeDoneOutcomeData, ModeKindData, NowData};

/// The mode id `GetModeGuide` takes for Now.
pub(crate) const NOW_MODE: &str = mxr_protocol::NOW_GUIDE.mode;

/// What a row on Now refers to, copied out so the page can be changed.
enum NowTarget {
    /// The arrivals line: Enter lists the emails behind it.
    Arrivals,
    /// A Not-sure question: Enter opens the email; a mode key answers it.
    NotSure {
        message_id: MessageId,
    },
    Person {
        thread: ThreadKey,
        message_id: MessageId,
    },
    Todo {
        todo_id: String,
        thread: Option<ThreadKey>,
        message_id: Option<MessageId>,
    },
    Updates,
    Reading {
        thread: ThreadKey,
        message_id: MessageId,
    },
}

impl From<NowRow<'_>> for NowTarget {
    fn from(row: NowRow<'_>) -> Self {
        match row {
            NowRow::Arrivals(_) => Self::Arrivals,
            NowRow::NotSure(question) => Self::NotSure {
                message_id: question.message_id.clone(),
            },
            NowRow::Person(person) => Self::Person {
                thread: ThreadKey::new(person.row.account_id.clone(), person.row.thread_id.clone()),
                message_id: person.row.message_id.clone(),
            },
            NowRow::Todo(todo) => Self::Todo {
                todo_id: todo.todo.id.clone(),
                thread: todo
                    .todo
                    .thread_id
                    .clone()
                    .map(|thread_id| ThreadKey::new(todo.todo.account_id.clone(), thread_id)),
                message_id: todo.todo.source_message_id.clone(),
            },
            NowRow::Updates(_) => Self::Updates,
            NowRow::Reading(pick) => Self::Reading {
                thread: ThreadKey::new(pick.account_id.clone(), pick.thread_id.clone()),
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
        // Opening Now is a visit: the arrivals line counts from the last.
        page.pending_mark_seen = true;
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

    /// The Updates card's cut in local time, for its section rule.
    pub(crate) fn now_updates_since(&self) -> Option<String> {
        self.mailbox
            .now_page
            .now
            .as_ref()
            .and_then(|now| now.updates.as_ref())
            .map(|card| {
                if card.cut_label.is_empty() {
                    card.since
                        .with_timezone(&chrono::Local)
                        .format("%H:%M")
                        .to_string()
                } else {
                    card.cut_label.clone()
                }
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
            NowTarget::Arrivals => self.apply(Action::OpenArrivals),
            NowTarget::Person { message_id, .. }
            | NowTarget::Reading { message_id, .. }
            | NowTarget::NotSure { message_id } => {
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
            NowTarget::Updates => self.apply(Action::OpenUpdates),
        }
    }

    /// `e`: done here, in the row's own mode. The row leaves at once; the
    /// toast is the daemon's handoff copy and `u` undoes it.
    fn now_done(&mut self) {
        let Some(target) = self.selected_now_row().map(NowTarget::from) else {
            return;
        };
        match target {
            // Nothing to be done with the line, and a question is
            // answered with a mode's key.
            NowTarget::Arrivals => {}
            NowTarget::NotSure { .. } => {
                self.status_message = Some(
                    "Answer with m Messages, x To do, u Updates, r Reading or e Archive".into(),
                );
            }
            NowTarget::Person { thread, .. } => {
                self.queue_mode_done(ModeKindData::Messages, thread);
                // After queueing, which sets its own progress line.
                self.explain_done_here_once();
            }
            NowTarget::Reading { thread, .. } => {
                self.queue_mode_done(ModeKindData::Reading, thread);
            }
            // Only this to-do: another one on the same email stays open.
            NowTarget::Todo {
                thread: Some(thread),
                todo_id,
                ..
            } => {
                self.queue_done(ModeKindData::Todo, thread, vec![todo_id]);
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
    pub(crate) fn queue_mode_done(&mut self, mode: ModeKindData, thread: ThreadKey) -> MutationId {
        self.queue_done(mode, thread, Vec::new())
    }

    /// `queue_mode_done`, naming the to-dos to tick off in To do.
    fn queue_done(
        &mut self,
        mode: ModeKindData,
        thread: ThreadKey,
        todo_ids: Vec<String>,
    ) -> MutationId {
        self.mailbox.now_page.remove_thread(&thread);
        if self.mailbox.mailbox_view == MailboxView::Now {
            self.mailbox.selected_index = self
                .mailbox
                .selected_index
                .min(self.mailbox.now_page.row_count().saturating_sub(1));
        }
        self.remove_desk_thread(&thread);
        self.queue_mutation(
            Request::SetModeDone {
                thread_ids: vec![thread.thread_id.clone()],
                thread_accounts: thread_accounts(std::slice::from_ref(&thread)),
                mode,
                dry_run: false,
                todo_ids,
                sender: None,
            },
            MutationEffect::ModeDone(format!("Done in {}", mode.name())),
            "Done here...".into(),
        )
    }

    /// `A`: ask the daemon what letting go of the card's digest would do;
    /// with that preview on screen, Enter lets go of exactly that
    /// selection, and refuses if the cut changed in between.
    fn now_let_go_digest(&mut self) {
        let page = &mut self.mailbox.now_page;
        if let Some(preview) = page.digest_preview.take() {
            if preview.message_ids.is_empty() {
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
                Request::LetGoDigest {
                    account_id: None,
                    cut: Some(preview.cut_at),
                    source_key: None,
                    selection_token: Some(preview.selection_token),
                    dry_run: false,
                },
                MutationEffect::ModeDone(String::new()),
                "Letting go...".into(),
            );
            return;
        }
        if page
            .now
            .as_ref()
            .and_then(|now| now.updates.as_ref())
            .is_none()
        {
            self.status_message = Some("No updates on Now to let go of".into());
            return;
        }
        page.pending_digest_preview = true;
        self.status_message = Some("Checking what letting go would change…".into());
    }

    /// The daemon's dry run of letting go of the card's digest.
    pub(crate) fn show_now_digest_preview(&mut self, preview: mxr_protocol::UpdatesLetGoData) {
        self.status_message = None;
        if preview.message_ids.is_empty() {
            self.push_toast(Toast::success("Nothing left to let go of"));
            return;
        }
        self.mailbox.now_page.digest_preview = Some(preview);
    }

    /// Key handling for the Now lens: its verbs and navigation. Mail verbs
    /// need an email, so they open the row first.
    pub(super) fn now_lens_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        if self.mailbox.now_page.digest_preview.is_some() {
            return match key.code {
                KeyCode::Enter | KeyCode::Char('y') => Some(Action::NowLetGoDigest),
                KeyCode::Esc | KeyCode::Char('n' | 'q') => {
                    self.mailbox.now_page.digest_preview = None;
                    self.status_message = Some("Kept the digest".into());
                    None
                }
                _ => None,
            };
        }
        // A question's one-key answer must not steal the suffix of a global
        // navigation chord (g m opens Messages; it does not move this mail).
        if self.input.is_pending() {
            let defined = self.defined_key("now", key);
            return self.run_defined_key(defined);
        }
        // A Not-sure question answers through `X` now (blueprint 35); its
        // old one-key answers say so instead of doing done or reply here.
        // `u` stays undo, as everywhere.
        if key.modifiers == KeyModifiers::NONE
            && matches!(self.selected_now_row(), Some(NowRow::NotSure(_)))
        {
            if let KeyCode::Char(c @ ('m' | 'x' | 'r' | 'e')) = key.code {
                self.announce_moved("not-sure", &c.to_string());
                return None;
            }
        }
        let defined = self.defined_key("now", key);
        self.run_defined_key(defined)
    }
}
