//! Focus & reply in the TUI (`g F`): everyone you owe a reply, one
//! conversation at a time, with the web app's queue, keys and requests.
//! The queue is the desk's You owe lane (`GetDesk`) then the reply-later
//! queue (`ListReplyQueue`); Done is the desk's Done (`ResolveDeskItems`),
//! Not now is the shared picker, and the reply is written in `$EDITOR`.
//! Sending the reply written for the conversation on screen moves on to
//! the next one; any other send, a failed one and a discarded one leave
//! the queue where it is.

use super::*;
use crate::app::state::{FocusItem, FocusPageState, FocusReply, FocusReturn, FocusSource};
use mxr_core::types::{DraftIntent, Envelope};
use mxr_core::ThreadId;
use mxr_protocol::{DeskLaneData, DeskLaneKind, DeskRowData, PeopleSplitData};
use std::path::Path;

/// The whole You owe lane, as the web's Focus & reply asks for it.
const FOCUS_LANE_LIMIT: u32 = 5000;

/// The desk request Focus & reply sends. Shared by the runtime and its
/// tests so they cannot drift.
pub(crate) fn focus_desk_request() -> Request {
    Request::GetDesk {
        account_id: None,
        lane_limit: FOCUS_LANE_LIMIT,
    }
}

/// What a Focus & reply key writes after the editor closes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AfterEditor {
    /// The send review, as for any reply.
    Review,
    /// The review, asking when to remind you if nobody replies.
    Remind,
    /// The writing model, to draft the reply in your voice.
    Draft,
}

/// The queue from both sources: You owe in the desk's order, then the
/// reply-later queue in its order, each conversation once.
pub(crate) fn build_focus_queue(owed: &[DeskRowData], reply_later: &[Envelope]) -> Vec<FocusItem> {
    let mut items: Vec<FocusItem> = Vec::new();
    let listed =
        |items: &[FocusItem], thread: &ThreadId| items.iter().any(|i| &i.thread_id == thread);
    for row in owed {
        if listed(&items, &row.thread_id) {
            continue;
        }
        let name = row
            .counterparty_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty());
        items.push(FocusItem {
            thread_id: row.thread_id.clone(),
            message_id: row.message_id.clone(),
            account_id: row.account_id.clone(),
            sender: name.unwrap_or(&row.counterparty_email).to_string(),
            subject: row.subject.clone(),
            reason: owed_reason(row),
            source: FocusSource::Owed,
        });
    }
    for message in reply_later {
        if listed(&items, &message.thread_id) {
            continue;
        }
        let name = message
            .from
            .name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty());
        items.push(FocusItem {
            thread_id: message.thread_id.clone(),
            message_id: message.id.clone(),
            account_id: message.account_id.clone(),
            sender: name.unwrap_or(&message.from.email).to_string(),
            subject: message.subject.clone(),
            reason: "In your reply-later queue".into(),
            source: FocusSource::ReplyLater,
        });
    }
    items
}

/// "Replied to your message · 2d · usually 4h", as the desk row reads.
fn owed_reason(row: &DeskRowData) -> String {
    let mut chars = row.reason.chars();
    let reason: String = chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default();
    format!("{reason} \u{b7} {}", crate::ui::desk_lens::age_cell(row))
}

impl App {
    pub(super) fn apply_focus_action(&mut self, action: Action) {
        match action {
            Action::OpenFocus => self.open_focus(None),
            Action::FocusSkip => self.focus_skip(),
            Action::FocusDone => self.focus_done(),
            Action::FocusReply => self.focus_reply(AfterEditor::Review),
            Action::FocusRemind => self.focus_reply(AfterEditor::Remind),
            Action::FocusDraft => self.focus_reply(AfterEditor::Draft),
            Action::FocusLeave => self.leave_focus(),
            _ => {}
        }
    }

    /// Keys on Focus & reply, from the key definition.
    pub(super) fn focus_page_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        let defined = self.defined_key("focus", key);
        let action = self.run_defined_key(defined);
        // A held `j` would spin through the queue faster than anyone reads it.
        if action == Some(Action::FocusSkip) && key.kind == crossterm::event::KeyEventKind::Repeat {
            return None;
        }
        action
    }

    /// Focus & reply owns the keyboard.
    pub(crate) fn focus_page_focused(&self) -> bool {
        self.screen == Screen::Mailbox
            && self.mailbox.mailbox_view == MailboxView::Focus
            && self.mailbox.active_pane == ActivePane::MailList
    }

    /// Open Focus & reply, with `start_at` on screen first when it is in
    /// the queue. Every visit is a new session, as on the web: what an
    /// earlier one skipped or set aside is back.
    pub(crate) fn open_focus(&mut self, start_at: Option<ThreadId>) {
        if self.focus_page_focused() && start_at.is_none() {
            return;
        }
        let return_to = (self.screen == Screen::Mailbox
            && self.mailbox.mailbox_view != MailboxView::Focus)
            .then_some(FocusReturn {
                view: self.mailbox.mailbox_view,
                active_pane: self.mailbox.active_pane,
                layout_mode: self.mailbox.layout_mode,
                selected_index: self.mailbox.selected_index,
                scroll_offset: self.mailbox.scroll_offset,
            })
            .or_else(|| self.mailbox.focus_page.return_to.take());
        // From the desk it works through the desk's You owe lane alone.
        let owed_only = return_to
            .as_ref()
            .is_some_and(|back| back.view == MailboxView::Desk);
        self.mailbox.focus_page = FocusPageState::start(start_at, owed_only, return_to);
        self.screen = Screen::Mailbox;
        self.mailbox.mailbox_view = MailboxView::Focus;
        self.mailbox.active_pane = ActivePane::MailList;
        self.mailbox.layout_mode = LayoutMode::TwoPane;
        self.status_message = None;
    }

    /// `F` in the reply queue: Focus & reply, from the selected message.
    pub(super) fn focus_from_reply_queue(&mut self) {
        let queue = &self.modals.reply_queue;
        let start = queue
            .messages
            .get(queue.selected_index)
            .map(|message| message.thread_id.clone());
        self.modals.reply_queue.close();
        self.open_focus(start);
    }

    /// The runtime fetched both sources. A failed one is reported and the
    /// other still fills the queue; neither can prove nobody is waiting.
    pub(crate) fn set_focus_sources(
        &mut self,
        owed: Result<(DeskLaneData, PeopleSplitData), MxrError>,
        reply_later: Result<Vec<Envelope>, MxrError>,
    ) {
        let mut errors = Vec::new();
        let page = &mut self.mailbox.focus_page;
        let rows = match owed {
            Ok((lane, people)) => {
                page.more = lane.total.saturating_sub(lane.rows.len() as u32);
                page.others = people.others;
                lane.rows
            }
            Err(error) => {
                errors.push(error.to_string());
                Vec::new()
            }
        };
        let later = match reply_later {
            Ok(messages) if !page.owed_only => messages,
            Ok(_) => Vec::new(),
            Err(error) => {
                errors.push(error.to_string());
                Vec::new()
            }
        };
        page.error = (!errors.is_empty()).then(|| errors.join("; "));
        page.sync(build_focus_queue(&rows, &later));
        self.focus_load_current();
    }

    /// The conversation on screen shows its newest message from them.
    fn focus_load_current(&mut self) {
        let Some(message_id) = self
            .mailbox
            .focus_page
            .current()
            .map(|item| item.message_id.clone())
        else {
            return;
        };
        if self.mailbox.body_cache.contains_key(&message_id) {
            self.focus_body_arrived(&message_id);
        } else {
            self.queue_body_fetch(message_id);
        }
    }

    /// A body arrived: when it is the one on screen, clean it for reading.
    pub(super) fn focus_body_arrived(&mut self, message_id: &MessageId) {
        let wanted = self
            .mailbox
            .focus_page
            .current()
            .is_some_and(|item| &item.message_id == message_id);
        let fresh = self
            .mailbox
            .focus_page
            .body
            .as_ref()
            .is_none_or(|(id, _)| id != message_id);
        if !wanted || !fresh {
            return;
        }
        let Some(body) = self.mailbox.body_cache.get(message_id) else {
            return;
        };
        let text = if body.text_plain.is_some() || body.text_html.is_some() {
            mxr_reader::clean(
                body.text_plain.as_deref(),
                body.text_html.as_deref(),
                &self.reader_config(),
            )
            .content
        } else {
            body.best_effort_readable_summary().unwrap_or_default()
        };
        self.mailbox.focus_page.body = Some((message_id.clone(), text));
    }

    pub(super) fn focus_body_failed(&mut self, message_id: &MessageId, error: &str) {
        let wanted = self
            .mailbox
            .focus_page
            .current()
            .is_some_and(|item| &item.message_id == message_id);
        if wanted {
            self.mailbox.focus_page.body = Some((
                message_id.clone(),
                format!("Couldn't load this email: {error}"),
            ));
        }
    }

    pub(crate) fn focus_view(&self) -> crate::ui::focus_lens::FocusView<'_> {
        let page = &self.mailbox.focus_page;
        let body = page
            .current()
            .zip(page.body.as_ref())
            .filter(|(item, (id, _))| &item.message_id == id)
            .map(|(_, (_, text))| text.clone());
        crate::ui::focus_lens::FocusView {
            page,
            focused: self.mailbox.active_pane == ActivePane::MailList,
            body,
        }
    }

    /// A reply is about to open: a skip now would show one conversation
    /// while the editor opens on another.
    fn focus_reply_opening(&self) -> bool {
        self.compose.pending_compose.is_some() || self.compose.deferred_compose.is_some()
    }

    fn focus_skip(&mut self) {
        if self.focus_reply_opening() {
            return;
        }
        let page = &mut self.mailbox.focus_page;
        // At the end, `j` brings back the ones set aside.
        if page.queue.is_empty() {
            page.revisit();
        } else {
            page.skip();
        }
        self.focus_load_current();
    }

    /// Done, no reply needed: the desk's Done (archived, marked read and
    /// off the desk until someone writes again). It leaves at once; `u`
    /// undoes it, and a Done that fails puts it back in front.
    fn focus_done(&mut self) {
        if self.focus_reply_opening() {
            return;
        }
        let Some(item) = self.mailbox.focus_page.current().cloned() else {
            return;
        };
        let request = Request::ResolveDeskItems {
            items: vec![mxr_protocol::DeskDoneItemData {
                thread_id: item.thread_id.clone(),
                lane: (item.source == FocusSource::Owed).then_some(DeskLaneKind::Owed),
                commitment_id: None,
            }],
            dry_run: false,
        };
        self.mailbox.focus_page.handled(&item.thread_id);
        let id = self.queue_mutation(
            request,
            MutationEffect::RefreshPlaces("Done. Press u to undo".into()),
            "Putting it away...".into(),
        );
        let page = &mut self.mailbox.focus_page;
        page.pending_done.insert(id, item.thread_id.clone());
        page.undoable_done = Some(item.thread_id);
        self.focus_load_current();
    }

    /// A Done that failed: back in front, so `e` retries it.
    pub(crate) fn focus_mutation_failed(&mut self, id: MutationId) {
        let page = &mut self.mailbox.focus_page;
        if let Some(thread_id) = page.pending_done.remove(&id) {
            if page.undoable_done.as_ref() == Some(&thread_id) {
                page.undoable_done = None;
            }
            page.restore(&thread_id);
            self.focus_load_current();
        }
    }

    /// `u` on Focus & reply right after a Done: once the daemon has put it
    /// back, the conversation is back in front.
    pub(super) fn focus_undo_effect(&mut self, verb_past: &str) -> Option<MutationEffect> {
        if self.mailbox.mailbox_view != MailboxView::Focus || verb_past != "Done" {
            return None;
        }
        let thread_id = self.mailbox.focus_page.undoable_done.take()?;
        Some(MutationEffect::FocusRestore { thread_id })
    }

    pub(super) fn focus_restore(&mut self, thread_id: &ThreadId) {
        self.mailbox.focus_page.restore(thread_id);
        self.focus_load_current();
    }

    /// Not now on the conversation on screen went through: it is handled.
    pub(super) fn focus_put_off(&mut self, message_id: &MessageId) {
        if self.mailbox.mailbox_view != MailboxView::Focus {
            return;
        }
        let Some(thread_id) = self
            .mailbox
            .focus_page
            .current()
            .filter(|item| &item.message_id == message_id)
            .map(|item| item.thread_id.clone())
        else {
            return;
        };
        let page = &mut self.mailbox.focus_page;
        page.handled(&thread_id);
        page.undoable_done = None;
        self.focus_load_current();
    }

    /// What `b` acts on in Focus & reply: the newest message from them.
    pub(crate) fn focus_not_now_message(&self) -> Option<MessageId> {
        self.focus_page_focused()
            .then(|| self.mailbox.focus_page.current())
            .flatten()
            .map(|item| item.message_id.clone())
    }

    /// Write the reply to the conversation on screen in `$EDITOR`. Only the
    /// send written in this compose file moves the queue on.
    fn focus_reply(&mut self, after: AfterEditor) {
        if self.focus_reply_opening() {
            return;
        }
        let Some(item) = self.mailbox.focus_page.current().cloned() else {
            return;
        };
        self.mailbox.focus_page.reply = Some(FocusReply {
            thread_id: item.thread_id.clone(),
            draft: None,
        });
        match after {
            AfterEditor::Review => {}
            AfterEditor::Remind => self.compose.remind_after_editor = true,
            AfterEditor::Draft => self.compose.assist_after_editor = true,
        }
        self.dispatch_or_defer_reply(item.message_id, item.account_id, false);
        self.status_message = Some(format!("Replying to {}", item.sender));
    }

    /// The reply Focus & reply asked for is ready: remember its compose
    /// file. Only the first reply to open after the key counts.
    pub(crate) fn note_focus_compose(&mut self, intent: DraftIntent, draft_path: &Path) {
        if let Some(reply) = self.mailbox.focus_page.reply.as_mut() {
            if reply.draft.is_none() && intent == DraftIntent::Reply {
                reply.draft = Some(draft_path.to_path_buf());
            }
        }
    }

    /// After a successful send: the next conversation is up if what went
    /// out was written in the reply's compose file.
    pub(crate) fn advance_focus_after_send(&mut self, draft_path: Option<&Path>) {
        let page = &mut self.mailbox.focus_page;
        let Some(reply) = page.reply.as_ref() else {
            return;
        };
        if draft_path.is_none() || reply.draft.as_deref() != draft_path {
            return;
        }
        let thread_id = reply.thread_id.clone();
        page.reply = None;
        page.undoable_done = None;
        page.handled(&thread_id);
        self.focus_load_current();
    }

    /// Back to where Focus & reply was opened from, as it was.
    fn leave_focus(&mut self) {
        let page = std::mem::take(&mut self.mailbox.focus_page);
        let Some(back) = page.return_to else {
            self.apply(Action::OpenNow);
            return;
        };
        self.mailbox.mailbox_view = back.view;
        self.mailbox.active_pane = back.active_pane;
        self.mailbox.layout_mode = back.layout_mode;
        self.mailbox.selected_index = back.selected_index;
        self.mailbox.scroll_offset = back.scroll_offset;
        // What was handled here may have left the view behind it.
        match back.view {
            MailboxView::Now => self.refresh_now(),
            MailboxView::People => self.refresh_messages(),
            MailboxView::Updates => self.refresh_updates(),
            MailboxView::Todo => self.refresh_todo(),
            MailboxView::Reading => self.refresh_reading(),
            MailboxView::ArchiveMode => self.refresh_records(),
            MailboxView::Desk | MailboxView::Place(_) => self.refresh_places(),
            MailboxView::Messages => {
                if let Some(label_id) = self.mailbox.active_label.clone() {
                    self.mailbox.pending_label_fetch = Some(label_id);
                }
            }
            _ => {}
        }
    }
}
