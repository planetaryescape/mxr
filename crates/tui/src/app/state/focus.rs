//! Focus & reply's queue: everyone you owe a reply, one conversation per
//! entry, in the web app's order and with its rules
//! (`apps/web/src/features/focus/focusQueue.ts`). The desk's You owe lane
//! comes first, then the reply-later queue, skipping conversations already
//! listed.
//!
//! The order is fixed when the page opens, so a refetch never reshuffles
//! what's next; new arrivals join the end; handled conversations never come
//! back unless their Done is undone; the one on screen stays put even if a
//! refetch drops it (you may be halfway through the reply).

use crate::app::MutationId;
use mxr_core::{AccountId, MessageId, ThreadId};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusSource {
    /// The desk's You owe lane.
    Owed,
    /// The reply-later queue.
    ReplyLater,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusItem {
    pub thread_id: ThreadId,
    /// The message a reply answers: the newest from them.
    pub message_id: MessageId,
    pub account_id: AccountId,
    pub sender: String,
    pub subject: String,
    /// Why it is here, in the daemon's words.
    pub reason: String,
    pub source: FocusSource,
}

/// The reply the page opened, so only its own send moves the queue on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusReply {
    pub thread_id: ThreadId,
    /// Its compose file, once the editor opens. A source message without a
    /// Message-ID header gives nothing else to match the send on.
    pub draft: Option<PathBuf>,
}

/// Where `Esc` goes back to: the view Focus & reply was opened from, as
/// it was. Opening Focus & reply leaves the list's label and rows alone, so
/// putting these back is enough.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusReturn {
    pub view: super::MailboxView,
    pub active_pane: super::ActivePane,
    pub layout_mode: super::LayoutMode,
    pub selected_index: usize,
    pub scroll_offset: usize,
}

#[derive(Debug, Default)]
pub struct FocusPageState {
    /// Every conversation seen this session, by thread.
    pub items: HashMap<ThreadId, FocusItem>,
    /// Still to do, in order; the first is on screen.
    pub queue: Vec<ThreadId>,
    /// Sent or put away, in the order handled.
    pub handled: Vec<ThreadId>,
    /// Skipped when nothing else was left: set aside until you come back.
    pub deferred: Vec<ThreadId>,
    /// The sources have answered at least once.
    pub loaded: bool,
    /// Opened from the desk: its You owe lane only, as the web's desk
    /// opens it.
    pub owed_only: bool,
    /// Owed conversations beyond what the lane returned.
    pub more: u32,
    /// People who wrote to you and wait in Messages, not here.
    pub others: u32,
    /// A source that failed. Nothing is ever called finished on it.
    pub error: Option<String>,
    pub pending_refresh: bool,
    /// The conversation to put on screen once it is in the queue (the reply
    /// queue's `F` names one).
    pub start_at: Option<ThreadId>,
    pub reply: Option<FocusReply>,
    /// The newest message's text for the one on screen, reader-cleaned
    /// once when it arrives (cleaning HTML can run an external command).
    pub body: Option<(MessageId, String)>,
    /// Done requests in flight, so a failure puts the conversation back.
    pub pending_done: HashMap<MutationId, ThreadId>,
    /// The last conversation put away with Done, while `u` can undo it.
    pub undoable_done: Option<ThreadId>,
    pub return_to: Option<FocusReturn>,
}

/// "2 of 5": the position of the one on screen, the session's size, and how
/// many are handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusProgress {
    pub position: usize,
    pub total: usize,
    pub done: usize,
}

impl FocusPageState {
    /// A fresh session: what an earlier visit handled can come back.
    pub fn start(
        start_at: Option<ThreadId>,
        owed_only: bool,
        return_to: Option<FocusReturn>,
    ) -> Self {
        Self {
            pending_refresh: true,
            start_at,
            owed_only,
            return_to,
            ..Self::default()
        }
    }

    pub fn current(&self) -> Option<&FocusItem> {
        self.queue.first().and_then(|id| self.items.get(id))
    }

    pub fn progress(&self) -> FocusProgress {
        let total = self.queue.len() + self.handled.len() + self.deferred.len();
        let done = self.handled.len();
        FocusProgress {
            position: (done + 1).min(total),
            total,
            done,
        }
    }

    /// The sources answered: keep the order, add arrivals at the end, and
    /// drop what left the sources except the one on screen.
    pub fn sync(&mut self, items: Vec<FocusItem>) {
        let current = self.queue.first().cloned();
        let in_source: std::collections::HashSet<ThreadId> =
            items.iter().map(|item| item.thread_id.clone()).collect();
        self.queue
            .retain(|id| Some(id) == current.as_ref() || in_source.contains(id));
        for item in items {
            let known = self.queue.contains(&item.thread_id)
                || self.handled.contains(&item.thread_id)
                || self.deferred.contains(&item.thread_id);
            if !known {
                self.queue.push(item.thread_id.clone());
            }
            self.items.insert(item.thread_id.clone(), item);
        }
        self.loaded = true;
        if let Some(wanted) = self.start_at.take() {
            self.front(&wanted);
        }
    }

    /// The one on screen goes to the end. The last one left is set aside
    /// rather than shown again.
    pub fn skip(&mut self) {
        if self.queue.is_empty() {
            return;
        }
        let current = self.queue.remove(0);
        if self.queue.is_empty() {
            self.deferred.push(current);
        } else {
            self.queue.push(current);
        }
    }

    /// Bring the set-aside conversations back into the queue.
    pub fn revisit(&mut self) {
        self.queue.append(&mut self.deferred);
    }

    pub fn handled(&mut self, thread_id: &ThreadId) {
        if let Some(index) = self.queue.iter().position(|id| id == thread_id) {
            let id = self.queue.remove(index);
            self.handled.push(id);
        }
    }

    /// Back in front: its Done was undone or failed.
    pub fn restore(&mut self, thread_id: &ThreadId) {
        if let Some(index) = self.handled.iter().position(|id| id == thread_id) {
            let id = self.handled.remove(index);
            self.queue.insert(0, id);
        }
    }

    /// Put a queued conversation on screen.
    pub fn front(&mut self, thread_id: &ThreadId) {
        if let Some(index) = self.queue.iter().position(|id| id == thread_id) {
            let id = self.queue.remove(index);
            self.queue.insert(0, id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(n: u8) -> FocusItem {
        FocusItem {
            thread_id: ThreadId::new(),
            message_id: MessageId::new(),
            account_id: AccountId::new(),
            sender: format!("Person {n}"),
            subject: format!("Subject {n}"),
            reason: "Replied to your message".into(),
            source: FocusSource::Owed,
        }
    }

    fn loaded(items: &[FocusItem]) -> FocusPageState {
        let mut page = FocusPageState::start(None, false, None);
        page.sync(items.to_vec());
        page
    }

    #[test]
    fn skip_sends_it_to_the_end_and_the_last_one_aside() {
        let items = [item(1), item(2)];
        let mut page = loaded(&items);
        page.skip();
        assert_eq!(page.current(), Some(&items[1]));
        page.handled(&items[1].thread_id);
        page.skip();
        assert_eq!(page.current(), None);
        assert_eq!(page.deferred, vec![items[0].thread_id.clone()]);
        page.revisit();
        assert_eq!(page.current(), Some(&items[0]));
    }

    #[test]
    fn a_refetch_keeps_the_order_and_never_brings_back_what_was_handled() {
        let items = [item(1), item(2), item(3)];
        let mut page = loaded(&items);
        page.handled(&items[0].thread_id);
        let arrival = item(4);
        // The source still lists the handled one (it hasn't caught up) and
        // no longer lists the one on screen.
        page.sync(vec![items[0].clone(), items[2].clone(), arrival.clone()]);
        assert_eq!(
            page.queue,
            vec![
                items[1].thread_id.clone(),
                items[2].thread_id.clone(),
                arrival.thread_id.clone()
            ]
        );
        assert_eq!(
            page.progress(),
            FocusProgress {
                position: 2,
                total: 4,
                done: 1
            }
        );
    }

    #[test]
    fn restore_puts_it_back_in_front_and_start_at_picks_the_first() {
        let items = [item(1), item(2), item(3)];
        let mut page = FocusPageState::start(Some(items[2].thread_id.clone()), false, None);
        page.sync(items.to_vec());
        assert_eq!(page.current(), Some(&items[2]));
        page.handled(&items[2].thread_id);
        assert_eq!(page.current(), Some(&items[0]));
        page.restore(&items[2].thread_id);
        assert_eq!(page.current(), Some(&items[2]));
        assert!(page.handled.is_empty());
    }
}
