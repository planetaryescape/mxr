//! `b`: reply later at a time, from the list, the reader or the desk. The
//! prompt previews the typed time with the shared parser, and Enter sends
//! the previewed instant (`DeferThreads`); the daemon decides by who wrote
//! last whether that is reply later or "bring it back if nobody replies".
//! Enter with nothing typed is the untimed reply later.

use super::*;
use crate::app::state::ReplyLaterPromptState;
use mxr_core::id::ThreadId;
use mxr_protocol::DeskLaneKind;

impl App {
    /// The conversation `b` acts on: the desk row under the cursor, or the
    /// message in context elsewhere (a desk row's reader included).
    pub(super) fn reply_later_target(&self) -> Option<(ThreadId, MessageId, bool)> {
        let desk_row = (self.screen == Screen::Mailbox
            && self.mailbox.mailbox_view == MailboxView::Desk)
            .then(|| self.selected_desk_row())
            .flatten();
        if self.messages_list_focused() {
            let topic = self.selected_messages_topic()?;
            return Some((topic.thread_id, topic.reply_to, false));
        }
        if self.desk_list_focused() {
            let row = desk_row?;
            return Some((
                row.thread_id.clone(),
                row.message_id.clone(),
                row.lane == DeskLaneKind::Waiting,
            ));
        }
        let envelope = self.context_envelope()?;
        let waiting = desk_row.is_some_and(|row| {
            row.thread_id == envelope.thread_id && row.lane == DeskLaneKind::Waiting
        });
        Some((envelope.thread_id.clone(), envelope.id.clone(), waiting))
    }

    /// Keys while the prompt is open. Returns the action Enter confirms.
    pub(super) fn reply_later_prompt_key(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> Option<Action> {
        let config = &self.modals.snooze_config;
        let prompt = self.modals.reply_later_prompt.as_mut()?;
        match code {
            KeyCode::Enter => return Some(Action::FlagReplyLater),
            KeyCode::Esc => {
                self.modals.reply_later_prompt = None;
                return None;
            }
            KeyCode::Tab => prompt.time.next_choice(),
            KeyCode::Backspace => {
                prompt.input.pop();
                prompt.error = None;
                prompt.time.update(&prompt.input, config);
            }
            KeyCode::Char(c)
                if !modifiers.contains(KeyModifiers::CONTROL)
                    && !modifiers.contains(KeyModifiers::ALT) =>
            {
                prompt.input.push(c);
                prompt.error = None;
                prompt.time.update(&prompt.input, config);
            }
            _ => {}
        }
        None
    }

    /// Enter: the untimed flag when nothing was typed, otherwise the
    /// previewed time, or the parser's reason in the prompt.
    pub(super) fn confirm_reply_later_prompt(&mut self) {
        let Some(prompt) = self.modals.reply_later_prompt.as_mut() else {
            return;
        };
        if prompt.input.trim().is_empty() {
            if prompt.waiting {
                prompt.error = Some("Type when to bring it back, like \"in 3d\"".into());
                return;
            }
            let message_id = prompt.message_id.clone();
            self.modals.reply_later_prompt = None;
            self.flag_reply_later_now(message_id);
            return;
        }
        let until = match prompt.time.chosen() {
            Ok(until) => until,
            Err(message) => {
                prompt.error = Some(message);
                return;
            }
        };
        let Some(ReplyLaterPromptState { thread_id, .. }) = self.modals.reply_later_prompt.take()
        else {
            return;
        };
        // Off the desk at once; the answer says what was set, and a failure
        // refetches the desk.
        self.remove_desk_thread(&thread_id);
        self.queue_mutation(
            Request::DeferThreads {
                thread_ids: vec![thread_id],
                until,
                dry_run: false,
            },
            MutationEffect::RefreshPlaces("Time set. Press u to undo".into()),
            "Setting the time...".into(),
        );
    }

    /// The untimed reply later: into the reply queue now.
    fn flag_reply_later_now(&mut self, message_id: MessageId) {
        let effect = MutationEffect::ReplyLater {
            message_id: message_id.clone(),
            flag: true,
            status: "Marked for reply later".into(),
        };
        let snapshot = self.snapshot_for_effect(&effect);
        self.apply_local_mutation_effect(&effect);
        let mutation_id = self.queue_mutation(
            Request::SetReplyLater {
                message_id,
                flag: true,
            },
            effect,
            "Marking for reply later...".into(),
        );
        self.mutation_snapshots.insert(mutation_id, snapshot);
    }
}
