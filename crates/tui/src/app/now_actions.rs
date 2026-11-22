
use super::*;
use crate::app::state::NowRow;
use mxr_core::id::ThreadId;
use mxr_protocol::{ModeDoneOutcomeData, ModeKindData, NowData};

pub(crate) const NOW_MODE: &str = mxr_protocol::NOW_GUIDE.mode;

enum NowTarget {
    Arrivals,
    NotSure {
        thread_id: ThreadId,
        message_id: MessageId,
        mode: ModeKindData,
    },
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
            NowRow::Arrivals(_) => Self::Arrivals,
            NowRow::NotSure(question) => Self::NotSure {
                thread_id: question.thread_id.clone(),
                message_id: question.message_id.clone(),
                mode: question.mode,
            },
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
        page.pending_mark_seen = true;
        self.mailbox.pending_rail_refresh = true;
    }

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

    pub(crate) fn refresh_now(&mut self) {
        self.mailbox.now_page.pending_refresh = true;
        self.mailbox.pending_rail_refresh = true;
    }

    fn now_open(&mut self, email: bool) {
        let Some(target) = self.selected_now_row().map(NowTarget::from) else {
            return;
        };
        match target {
            NowTarget::Arrivals => self.apply(Action::OpenArrivals),
            NowTarget::Person { thread_id, .. } if !email => self.open_messages_thread(thread_id),
            NowTarget::Person { message_id, .. }
            | NowTarget::Reading { message_id, .. }
            | NowTarget::NotSure { message_id, .. } => {
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

    fn now_done(&mut self) {
        let Some(target) = self.selected_now_row().map(NowTarget::from) else {
            return;
        };
        match target {
            NowTarget::Arrivals => {}
            NowTarget::NotSure {
                mode: ModeKindData::Archive,
                ..
            } => {
                self.status_message = Some("Archive has no done: records stay filed.".into());
            }
            NowTarget::NotSure {
                thread_id, mode, ..
            } => {
                self.queue_mode_done(mode, thread_id);
            }
            NowTarget::Person { thread_id, .. } => {
                self.queue_mode_done(ModeKindData::Messages, thread_id);
                self.explain_done_here_once();
            }
            NowTarget::Reading { thread_id, .. } => {
                self.queue_mode_done(ModeKindData::Reading, thread_id);
            }
            NowTarget::Todo {
                thread_id: Some(thread_id),
                todo_id,
                ..
            } => {
                self.queue_done(ModeKindData::Todo, thread_id, vec![todo_id]);
            }
            NowTarget::Todo { todo_id, .. } => {
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

    pub(crate) fn queue_mode_done(
        &mut self,
        mode: ModeKindData,
        thread_id: ThreadId,
    ) -> MutationId {
        self.queue_done(mode, thread_id, Vec::new())
    }

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

    pub(crate) fn show_now_digest_preview(&mut self, preview: mxr_protocol::UpdatesLetGoData) {
        self.status_message = None;
        if preview.message_ids.is_empty() {
            self.push_toast(Toast::success("Nothing left to let go of"));
            return;
        }
        self.mailbox.now_page.digest_preview = Some(preview);
    }

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
        if self.input.is_pending() {
            let defined = self.defined_key("now", key);
            return self.run_defined_key(defined);
        }
        let defined = self.defined_key("now", key);
        self.run_defined_key(defined)
    }
}
