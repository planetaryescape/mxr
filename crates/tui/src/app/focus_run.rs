//! Focus & reply in the TUI: from the reply-later queue, reply to each
//! message in turn. Sending the reply to the current message opens the
//! next one's reply; any other send leaves the run alone, and a discarded
//! reply just pauses it (start again from the queue).

use super::App;
use crate::app::Toast;
use mxr_core::types::{DraftIntent, Envelope};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct FocusRun {
    /// Queued messages still to reply to, in queue order.
    pub remaining: VecDeque<Envelope>,
    /// The message whose reply is open (or about to be).
    pub current: Envelope,
    /// The compose file of the current reply, once its editor opens.
    pub reply_draft: Option<PathBuf>,
    pub total: usize,
}

impl FocusRun {
    /// 1-based position of the current message, for "Focus 2 of 5".
    pub fn position(&self) -> usize {
        self.total - self.remaining.len()
    }
}

impl App {
    /// Start replying to every message in the reply-queue modal, from the
    /// selected one on, wrapping to those above it.
    pub(super) fn start_focus_run(&mut self) {
        let messages = &self.modals.reply_queue.messages;
        if messages.is_empty() {
            self.status_message = Some("Reply queue is empty".into());
            return;
        }
        let start = self
            .modals
            .reply_queue
            .selected_index
            .min(messages.len() - 1);
        let mut remaining: VecDeque<Envelope> = messages[start..]
            .iter()
            .chain(&messages[..start])
            .cloned()
            .collect();
        let total = remaining.len();
        let current = remaining.pop_front().expect("checked non-empty");
        self.modals.reply_queue.close();
        self.focus_run = Some(FocusRun {
            remaining,
            current,
            reply_draft: None,
            total,
        });
        self.open_focus_reply();
    }

    fn open_focus_reply(&mut self) {
        let Some(run) = self.focus_run.as_ref() else {
            return;
        };
        let (id, account_id) = (run.current.id.clone(), run.current.account_id.clone());
        let status = format!(
            "Focus {} of {}: replying to {}",
            run.position(),
            run.total,
            sender_label(&run.current)
        );
        self.dispatch_or_defer_reply(id, account_id, false);
        self.status_message = Some(status);
    }

    /// The reply the run asked for is ready: remember its compose file.
    /// Only the first reply to open after the run moved on counts.
    pub(crate) fn note_focus_compose(&mut self, intent: DraftIntent, draft_path: &Path) {
        if let Some(run) = self.focus_run.as_mut() {
            if run.reply_draft.is_none() && intent == DraftIntent::Reply {
                run.reply_draft = Some(draft_path.to_path_buf());
            }
        }
    }

    /// After a successful send: open the next reply if what went out was
    /// written in the current reply's compose file, or finish. Any other
    /// send, and a failed one (which never gets here), leaves the run where
    /// it is.
    pub(crate) fn advance_focus_run_after_send(&mut self, draft_path: Option<&Path>) {
        let Some(run) = self.focus_run.as_mut() else {
            return;
        };
        if draft_path.is_none() || run.reply_draft.as_deref() != draft_path {
            return;
        }
        match run.remaining.pop_front() {
            Some(next) => {
                run.current = next;
                run.reply_draft = None;
                self.open_focus_reply();
            }
            None => {
                let total = run.total;
                self.focus_run = None;
                self.push_toast(Toast::success(format!(
                    "Low tide. That's everyone in the reply queue ({total} replied)."
                )));
            }
        }
    }
}

fn sender_label(envelope: &Envelope) -> String {
    envelope
        .from
        .name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(&envelope.from.email)
        .to_string()
}
