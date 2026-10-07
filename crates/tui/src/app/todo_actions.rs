//! To do in the TUI: open the runway, do what a row's button says, tick
//! off, schedule, correct, dismiss, restore, and the one-time catch-up.
//! The daemon owns every rule and label; this module turns keys into
//! requests and keeps the lens honest until the daemon answers.

use super::input::plain_or_shift;
use super::*;
use crate::app::state::{TodoListFetch, TodoPanel, TodoPromptKind, TodoPromptState};
use mxr_protocol::{
    TodoCatchupDecisionData, TodoChangeData, TodoData, TodoEditData, TodoStateActionData,
};

/// The mode id `GetModeGuide` takes for To do.
pub(crate) const TODO_MODE: &str = mxr_protocol::TODO_GUIDE.mode;

impl App {
    pub(super) fn apply_todo_action(&mut self, action: Action) {
        match action {
            Action::OpenTodo => self.open_todo(),
            Action::TodoPrimary => self.todo_primary(),
            Action::TodoDone => self.set_selected_todo_state(TodoStateActionData::Done),
            Action::TodoDismiss => self.set_selected_todo_state(TodoStateActionData::Dismiss),
            Action::TodoRestore => self.set_selected_todo_state(TodoStateActionData::Undo),
            Action::TodoSchedule => self.open_todo_prompt(false),
            Action::TodoEdit => self.open_todo_prompt(true),
            Action::TodoOpenEmail => self.open_selected_todo_email(),
            Action::TodoOpenExpired => self.show_todo_panel(TodoPanel::Expired),
            Action::TodoOpenCatchup => self.show_todo_panel(TodoPanel::Catchup),
            Action::TodoShowRunway => self.show_todo_panel(TodoPanel::Runway),
            Action::CatchupKeep => self.decide_catchup_row(true),
            Action::CatchupLetGo => self.decide_catchup_row(false),
            Action::CatchupLetGoAll => self.let_go_of_all_catchup(),
            Action::CreateTodoFromMessage => self.open_create_todo_prompt(),
            _ => {}
        }
    }

    fn open_todo(&mut self) {
        self.mailbox.mailbox_view = MailboxView::Todo;
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
        let page = &mut self.mailbox.todo_page;
        page.panel = TodoPanel::Runway;
        page.catchup_preview = None;
        page.pending_refresh = true;
        page.pending_mark_seen = true;
        page.expired_on_open = 0;
    }

    /// The To do lens owns the keyboard (its list, not a reader beside it).
    pub(crate) fn todo_list_focused(&self) -> bool {
        self.screen == Screen::Mailbox
            && self.mailbox.mailbox_view == MailboxView::Todo
            && self.mailbox.active_pane == ActivePane::MailList
    }

    /// The guide `?` leads with: the mode's whose lens is on screen.
    pub(crate) fn help_mode_guide(&self) -> Option<&mxr_protocol::ModeGuideData> {
        if self.screen != Screen::Mailbox {
            return None;
        }
        match self.mailbox.mailbox_view {
            MailboxView::Todo => self.mailbox.todo_page.guide.as_ref(),
            MailboxView::Now => self.mailbox.now_page.guide.as_ref(),
            MailboxView::People => self.mailbox.messages_page.guide.as_ref(),
            MailboxView::ArchiveMode => self.mailbox.records_page.guide.as_ref(),
            MailboxView::Reading => self.mailbox.reading_page.guide.as_ref(),
            _ => None,
        }
    }

    pub fn selected_todo(&self) -> Option<&TodoData> {
        self.mailbox
            .todo_page
            .rows()
            .get(self.mailbox.selected_index)
            .copied()
    }

    /// The runtime fetched the runway and the guide.
    pub(crate) fn set_todo_runway(
        &mut self,
        runway: mxr_protocol::TodoRunwayData,
        guide: Option<mxr_protocol::ModeGuideData>,
    ) {
        let page = &mut self.mailbox.todo_page;
        page.expired_on_open = page.expired_on_open.max(runway.expired_since_last_looked);
        page.runway = Some(runway);
        if guide.is_some() {
            page.guide = guide;
        }
        self.clamp_todo_cursor();
    }

    pub(crate) fn set_todo_expired(&mut self, todos: Vec<TodoData>) {
        self.mailbox.todo_page.expired = todos;
        self.clamp_todo_cursor();
    }

    pub(crate) fn set_todo_catchup(&mut self, catchup: mxr_protocol::TodoCatchupData) {
        self.mailbox.todo_page.catchup = Some(catchup);
        self.clamp_todo_cursor();
    }

    fn clamp_todo_cursor(&mut self) {
        if self.mailbox.mailbox_view != MailboxView::Todo {
            return;
        }
        self.mailbox.selected_index = self
            .mailbox
            .selected_index
            .min(self.mailbox.todo_page.row_count().saturating_sub(1));
    }

    /// Refetch whatever the lens shows after a change.
    pub(crate) fn refresh_todo(&mut self) {
        let page = &mut self.mailbox.todo_page;
        page.pending_refresh = true;
        page.pending_list = match page.panel {
            TodoPanel::Runway => None,
            TodoPanel::Expired => Some(TodoListFetch::Expired),
            TodoPanel::Catchup => Some(TodoListFetch::Catchup),
        };
    }

    fn show_todo_panel(&mut self, panel: TodoPanel) {
        let page = &mut self.mailbox.todo_page;
        page.panel = panel;
        page.catchup_preview = None;
        self.mailbox.selected_index = 0;
        self.refresh_todo();
    }

    /// Enter: open the link when the gate passed, otherwise the email. The
    /// footer printed the domain before the key was pressed.
    fn todo_primary(&mut self) {
        let Some(todo) = self.selected_todo().cloned() else {
            return;
        };
        // Never the link itself: the email opens, and the footer has
        // already said where its link goes.
        self.open_todo_email(&todo);
    }

    fn open_selected_todo_email(&mut self) {
        if let Some(todo) = self.selected_todo().cloned() {
            self.open_todo_email(&todo);
        }
    }

    fn open_todo_email(&mut self, todo: &TodoData) {
        // The action names the message its link is in; the row's source
        // message otherwise.
        let message_id = todo
            .action
            .as_ref()
            .and_then(|action| action.message_id.clone())
            .or_else(|| todo.source_message_id.clone());
        match message_id {
            Some(message_id) => {
                self.mailbox.todo_page.pending_open = Some(crate::app::TodoOpen {
                    message_id,
                    link: todo.action.as_ref().map(|action| action.url.clone()),
                });
                self.status_message = Some(
                    match todo.action.as_ref().and_then(|action| action.domain.as_deref()) {
                        Some(domain) => format!(
                            "Opening the email. Its link goes to {domain}; check it before you follow it"
                        ),
                        None => "Opening the email…".into(),
                    },
                );
            }
            None => self.status_message = Some("This to-do has no email to open".into()),
        }
    }

    /// Done, not a to-do, or restore, for the row under the cursor. The
    /// row leaves at once; the daemon's answer offers `u`.
    fn set_selected_todo_state(&mut self, action: TodoStateActionData) {
        let Some(todo) = self.selected_todo().cloned() else {
            return;
        };
        let (status, progress) = match action {
            TodoStateActionData::Done => (format!("Ticked off: {}", todo.title), "Ticking off..."),
            TodoStateActionData::Dismiss => (
                format!("Not a to-do: {}. It won't come back", todo.title),
                "Marking not a to-do...",
            ),
            TodoStateActionData::Undo => (
                format!("Back on the runway: {}", todo.title),
                "Restoring...",
            ),
        };
        self.remove_todo_rows(std::slice::from_ref(&todo.id));
        self.queue_mutation(
            Request::SetTodoState {
                todo_ids: vec![todo.id],
                action,
                dry_run: false,
            },
            MutationEffect::Todo(status),
            progress.into(),
        );
    }

    /// Take rows off the lens before the daemon answers, so the next row
    /// takes the cursor.
    pub(crate) fn remove_todo_rows(&mut self, ids: &[String]) {
        let page = &mut self.mailbox.todo_page;
        let keep = |todo: &TodoData| !ids.contains(&todo.id);
        if let Some(runway) = page.runway.as_mut() {
            runway.now.retain(keep);
            for week in &mut runway.coming_up {
                week.todos.retain(keep);
            }
            runway.coming_up.retain(|week| !week.todos.is_empty());
            runway.later.retain(keep);
            runway.whenever.retain(keep);
        }
        page.expired.retain(keep);
        if let Some(catchup) = page.catchup.as_mut() {
            catchup.todos.retain(keep);
        }
        self.clamp_todo_cursor();
    }

    /// Called once `GetEnvelope` answers for a row's email: show that
    /// message, not the newest in its thread, with the row's link marked.
    pub(crate) fn open_todo_envelope(
        &mut self,
        env: mxr_core::types::Envelope,
        link: Option<String>,
    ) {
        let id = env.id.clone();
        self.open_invite_envelope(env);
        if let Some(index) = self
            .mailbox
            .viewed_thread_messages
            .iter()
            .position(|message| message.id == id)
        {
            self.mailbox.thread_selected_index = index;
            self.sync_focused_thread_envelope();
        }
        self.mailbox.todo_link = link.map(|link| (id, link));
    }

    fn open_todo_prompt(&mut self, edit: bool) {
        let Some(todo) = self.selected_todo() else {
            return;
        };
        let todo_id = todo.id.clone();
        let kind = if edit {
            TodoPromptKind::Edit { todo_id }
        } else {
            TodoPromptKind::Schedule { todo_id }
        };
        self.mailbox.todo_page.prompt = Some(TodoPromptState::new(kind, String::new()));
    }

    /// `t` on a conversation: a prompt for what to do, starting from
    /// "Reply to <sender>".
    fn open_create_todo_prompt(&mut self) {
        let now_person = self
            .now_list_focused()
            .then(|| match self.selected_now_row() {
                Some(crate::app::NowRow::Person(person)) => Some((
                    person.row.message_id.clone(),
                    person
                        .row
                        .counterparty_name
                        .clone()
                        .unwrap_or_else(|| person.row.counterparty_email.clone()),
                )),
                _ => None,
            })
            .flatten();
        let messages_topic = self
            .messages_list_focused()
            .then(|| self.selected_messages_topic())
            .flatten()
            .map(|topic| (topic.reply_to, topic.who));
        let target = if now_person.is_some() {
            now_person
        } else if messages_topic.is_some() {
            messages_topic
        } else if self.reading_lens_focused() {
            self.selected_reading_row().map(|row| {
                let issue = row.issue();
                (issue.message_id.clone(), issue.source.clone())
            })
        } else if self.place_list_focused() {
            self.selected_place_row().map(|(bundle, message)| {
                (
                    message.message_id.clone(),
                    bundle
                        .sender_name
                        .clone()
                        .unwrap_or_else(|| bundle.sender_email.clone()),
                )
            })
        } else {
            self.context_envelope().map(|envelope| {
                (
                    envelope.id.clone(),
                    envelope
                        .from
                        .name
                        .clone()
                        .unwrap_or_else(|| envelope.from.email.clone()),
                )
            })
        };
        let Some((message_id, sender)) = target else {
            self.status_message = Some("Open or select a conversation to make a to-do".into());
            return;
        };
        let sender = crate::ui::sanitize::one_line(&sender);
        self.mailbox.todo_page.prompt = Some(TodoPromptState::new(
            TodoPromptKind::Create { message_id },
            format!("Reply to {sender}"),
        ));
    }

    /// Keys while the prompt is open: typing, Tab for another reading of
    /// a time, Enter to send, Esc to cancel.
    pub(super) fn todo_prompt_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        let config = &self.modals.snooze_config;
        let Some(prompt) = self.mailbox.todo_page.prompt.as_mut() else {
            return;
        };
        let is_schedule = matches!(prompt.kind, TodoPromptKind::Schedule { .. });
        match code {
            KeyCode::Enter => self.confirm_todo_prompt(),
            KeyCode::Esc => self.mailbox.todo_page.prompt = None,
            KeyCode::Tab if is_schedule => prompt.time.next_choice(),
            KeyCode::Backspace => {
                prompt.input.pop();
                prompt.error = None;
                if is_schedule {
                    prompt.time.update(&prompt.input, config);
                }
            }
            KeyCode::Char(c)
                if !modifiers.contains(KeyModifiers::CONTROL)
                    && !modifiers.contains(KeyModifiers::ALT) =>
            {
                prompt.input.push(c);
                prompt.error = None;
                if is_schedule {
                    prompt.time.update(&prompt.input, config);
                }
            }
            _ => {}
        }
    }

    fn confirm_todo_prompt(&mut self) {
        let Some(prompt) = self.mailbox.todo_page.prompt.as_mut() else {
            return;
        };
        let input = prompt.input.trim().to_string();
        let request = match &prompt.kind {
            TodoPromptKind::Schedule { todo_id } => match prompt.time.chosen() {
                // The previewed instant, so what was shown is what is stored.
                Ok(at) => (
                    Request::ScheduleTodo {
                        todo_id: todo_id.clone(),
                        when: Some(at.to_rfc3339()),
                        time_zone: None,
                        dry_run: false,
                    },
                    "Scheduled. It shows up then".to_string(),
                ),
                Err(message) => {
                    prompt.error = Some(message);
                    return;
                }
            },
            TodoPromptKind::Edit { todo_id } => match input.split_once('=') {
                Some((field, value)) if !field.trim().is_empty() => (
                    Request::UpdateTodo {
                        todo_id: todo_id.clone(),
                        edits: vec![TodoEditData {
                            field: field.trim().to_string(),
                            value: value.trim().to_string(),
                        }],
                        time_zone: None,
                        dry_run: false,
                    },
                    format!("Changed {}. The row is yours now", field.trim()),
                ),
                _ => {
                    prompt.error =
                        Some("Type field=value: title, due, amount, counterparty or kind".into());
                    return;
                }
            },
            TodoPromptKind::Create { message_id } => {
                if input.is_empty() {
                    prompt.error = Some("Say what to do, starting with the verb".into());
                    return;
                }
                (
                    Request::CreateTodo {
                        message_id: message_id.clone(),
                        title: input,
                        kind: None,
                        due: None,
                        time_zone: None,
                        dry_run: false,
                    },
                    "Added to To do (g x)".to_string(),
                )
            }
        };
        self.mailbox.todo_page.prompt = None;
        let (request, status) = request;
        self.queue_mutation(request, MutationEffect::Todo(status), "Saving...".into());
    }

    fn decide_catchup_row(&mut self, keep: bool) {
        let Some(todo) = self.selected_todo().cloned() else {
            return;
        };
        let todo_ids = vec![todo.id.clone()];
        let (decision, status) = if keep {
            (
                TodoCatchupDecisionData::Keep { todo_ids },
                format!("Kept: {}. It's on the runway", todo.title),
            )
        } else {
            (
                TodoCatchupDecisionData::LetGo { todo_ids },
                format!("Let go of {}. It's in the Expired list", todo.title),
            )
        };
        self.remove_todo_rows(std::slice::from_ref(&todo.id));
        self.queue_mutation(
            Request::SetTodoCatchup {
                account_id: None,
                decision,
                dry_run: false,
            },
            MutationEffect::Todo(status),
            "Saving...".into(),
        );
    }

    /// `A` asks the daemon what letting go of all would change; with that
    /// preview on screen, Enter lets go of exactly the previewed rows.
    fn let_go_of_all_catchup(&mut self) {
        let page = &mut self.mailbox.todo_page;
        if page.panel != TodoPanel::Catchup {
            return;
        }
        let Some(preview) = page.catchup_preview.take() else {
            page.pending_catchup_preview = true;
            self.status_message = Some("Checking what letting go would change…".into());
            return;
        };
        let todo_ids: Vec<String> = preview.changed.iter().map(|todo| todo.id.clone()).collect();
        if todo_ids.is_empty() {
            return;
        }
        self.remove_todo_rows(&todo_ids);
        let count = todo_ids.len();
        self.queue_mutation(
            Request::SetTodoCatchup {
                account_id: None,
                decision: TodoCatchupDecisionData::LetGo { todo_ids },
                dry_run: false,
            },
            MutationEffect::Todo(format!("Let go of {count}. They're in the Expired list")),
            "Letting go...".into(),
        );
    }

    /// The daemon's dry run of "let go of all".
    pub(crate) fn show_catchup_preview(&mut self, preview: TodoChangeData) {
        self.status_message = None;
        if preview.changed.is_empty() {
            self.push_toast(Toast::success("Nothing left to let go of"));
            return;
        }
        self.mailbox.todo_page.catchup_preview = Some(preview);
    }

    /// Key handling for the To do lens. Only the mode's verbs and
    /// navigation reach it: mail verbs need an email, and this list holds
    /// things to do.
    pub(super) fn todo_lens_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        let page = &self.mailbox.todo_page;
        if page.catchup_preview.is_some() {
            return match key.code {
                KeyCode::Enter | KeyCode::Char('y') => Some(Action::CatchupLetGoAll),
                KeyCode::Esc | KeyCode::Char('n') => {
                    self.mailbox.todo_page.catchup_preview = None;
                    self.status_message = Some("Kept them all for now".into());
                    None
                }
                _ => None,
            };
        }
        let panel = page.panel;
        let hint = self.active_hint().is_some();
        let plain = key.modifiers == KeyModifiers::NONE;
        let shifted = plain_or_shift(key.modifiers);
        let action = match (panel, key.code) {
            (_, KeyCode::Char('/')) if plain => Some(Action::OpenGlobalSearch),
            (_, KeyCode::Char('h') | KeyCode::Left) if plain => {
                self.mailbox.active_pane = ActivePane::Sidebar;
                return None;
            }
            (_, KeyCode::Esc) if hint => Some(Action::DismissHint),
            (TodoPanel::Expired | TodoPanel::Catchup, KeyCode::Esc) => Some(Action::TodoShowRunway),
            (_, KeyCode::Char('o')) if plain => Some(Action::TodoOpenEmail),
            (TodoPanel::Runway, KeyCode::Enter) => Some(Action::TodoPrimary),
            (TodoPanel::Runway, KeyCode::Char('e')) if plain => Some(Action::TodoDone),
            (TodoPanel::Runway, KeyCode::Char('Z')) if shifted => Some(Action::TodoSchedule),
            (TodoPanel::Runway, KeyCode::Char(',')) => Some(Action::TodoEdit),
            (TodoPanel::Runway, KeyCode::Char('X')) if shifted => Some(Action::TodoDismiss),
            (TodoPanel::Runway | TodoPanel::Catchup, KeyCode::Char('u')) if plain => {
                Some(Action::UndoLastMutation)
            }
            (TodoPanel::Runway, KeyCode::Char('E')) if shifted => Some(Action::TodoOpenExpired),
            (TodoPanel::Runway, KeyCode::Char('C')) if shifted => Some(Action::TodoOpenCatchup),
            (TodoPanel::Expired, KeyCode::Enter) => Some(Action::TodoRestore),
            (TodoPanel::Expired, KeyCode::Char('u')) if plain => Some(Action::TodoRestore),
            (TodoPanel::Catchup, KeyCode::Enter) => Some(Action::CatchupKeep),
            (TodoPanel::Catchup, KeyCode::Char('e')) if plain => Some(Action::CatchupLetGo),
            (TodoPanel::Catchup, KeyCode::Char('A')) if shifted => Some(Action::CatchupLetGoAll),
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
