//! Hints in the TUI (D118, amended 2026-10-07). There is no page-top card
//! and no tour: a hint is one sentence in the status line while the cursor
//! is on the element it explains, the first time that element is needed.
//! Esc, or acting on the element, dismisses it in every client
//! (`SetHintSeen`). The copy and the seen state come with each mode's guide.
//!
//! The rules match the web app's (`apps/web/src/features/hints`): one hint
//! at a time; none on arrival until a key is pressed on the page, unless the
//! element is the only thing there; and no chains, so once a hint leaves
//! nothing shows until the cursor moves.

use std::collections::{HashMap, HashSet};

use mxr_protocol::{HintData, ModeGuideData, Request};

use super::*;
use crate::app::state::NowRow;

/// Where the cursor is, as far as hints care: the page, the row, and the
/// part of the page that has the keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HintSpot {
    view: MailboxView,
    cursor: usize,
    person_open: bool,
    answer_open: bool,
}

#[derive(Debug, Default)]
pub struct HintState {
    /// Dismissed here before the daemon answered, so none flickers back.
    closed: HashSet<String>,
    /// The `SetHintSeen` sent for each dismissal, so a failed write can
    /// show that hint again at its next need.
    pending: HashMap<MutationId, String>,
    /// The page the last key was pressed on. The key that opens a page
    /// was pressed on the page before, so arriving never counts as a use.
    key_on: Option<MailboxView>,
    /// Where the cursor was when the last hint left.
    quiet_at: Option<HintSpot>,
}

/// The hints that can attach where the cursor is, in the order they win,
/// with whether the element is the only thing on the page.
struct Candidates<'a> {
    guide: Option<&'a ModeGuideData>,
    ids: Vec<&'static str>,
    alone: bool,
}

impl App {
    fn hint_spot(&self) -> HintSpot {
        let records = &self.mailbox.records_page;
        HintSpot {
            view: self.mailbox.mailbox_view,
            cursor: self.mailbox.selected_index,
            person_open: self.mailbox.messages_page.focus == MessagesFocus::Person,
            answer_open: records.card.is_none() && records.answer_record().is_some(),
        }
    }

    fn hint_candidates(&self) -> Candidates<'_> {
        let at = self.mailbox.selected_index;
        let none = Candidates {
            guide: None,
            ids: Vec::new(),
            alone: false,
        };
        match self.mailbox.mailbox_view {
            MailboxView::Now => {
                let page = &self.mailbox.now_page;
                let rows = page.rows();
                let first_why = rows
                    .iter()
                    .position(|row| !matches!(row, NowRow::Updates(_)));
                let ids = match rows.get(at) {
                    Some(NowRow::Updates(_)) => vec!["updates.let_go"],
                    Some(_) if first_why == Some(at) => vec!["now.from_mode"],
                    _ => Vec::new(),
                };
                Candidates {
                    guide: page.guide.as_ref(),
                    ids,
                    alone: rows.len() == 1,
                }
            }
            MailboxView::Todo => {
                let page = &self.mailbox.todo_page;
                if page.panel != TodoPanel::Runway {
                    return none;
                }
                let rows = page.runway_rows();
                let first_bar = rows.iter().position(|todo| todo.runway.is_some());
                let mut ids = Vec::new();
                if first_bar == Some(at) {
                    ids.push("todo.runway");
                }
                // The catch-up line sits above the first row.
                if at == 0 && page.runway.as_ref().is_some_and(|r| r.catchup_count > 0) {
                    ids.push("todo.catchup");
                }
                Candidates {
                    guide: page.guide.as_ref(),
                    ids,
                    alone: rows.len() == 1,
                }
            }
            MailboxView::People => {
                let page = &self.mailbox.messages_page;
                let row = self.selected_messages_row();
                let ids = match page.focus {
                    MessagesFocus::Person
                        if row
                            .and_then(|row| page.page_for_row(row))
                            .is_some_and(|person| person.topics.len() > 1) =>
                    {
                        vec!["messages.topics"]
                    }
                    MessagesFocus::List if row.is_some_and(|row| row.your_turn) => {
                        vec!["messages.got_it"]
                    }
                    _ => Vec::new(),
                };
                Candidates {
                    guide: page.guide.as_ref(),
                    ids,
                    alone: false,
                }
            }
            MailboxView::ArchiveMode => {
                let page = &self.mailbox.records_page;
                let ids = if page.card.is_some() {
                    Vec::new()
                } else if page.answer_record().is_some() {
                    vec!["archive.answer"]
                } else if at == 0 && !page.rows().is_empty() {
                    vec!["archive.record"]
                } else {
                    Vec::new()
                };
                Candidates {
                    guide: page.guide.as_ref(),
                    ids,
                    alone: page.row_count() == 1,
                }
            }
            _ => none,
        }
    }

    /// The hint for the element under the cursor, if it is that element's
    /// first need. Pure: drawing calls it every frame.
    pub fn active_hint(&self) -> Option<&HintData> {
        if self.screen != Screen::Mailbox || self.mailbox.active_pane == ActivePane::Sidebar {
            return None;
        }
        let candidates = self.hint_candidates();
        let used_here = self.hints.key_on == Some(self.mailbox.mailbox_view);
        if !(used_here || candidates.alone) || self.hints.quiet_at == Some(self.hint_spot()) {
            return None;
        }
        let guide = candidates.guide?;
        candidates.ids.iter().find_map(|id| {
            guide
                .hints
                .iter()
                .find(|hint| hint.id == *id && !hint.seen && !self.hints.closed.contains(*id))
        })
    }

    /// Every key counts as a use of the page it was pressed on.
    pub(crate) fn note_key_for_hints(&mut self) {
        self.hints.key_on = (self.screen == Screen::Mailbox).then_some(self.mailbox.mailbox_view);
    }

    /// The quiet after a hint leaves lasts until the cursor moves.
    pub(crate) fn settle_hint_quiet(&mut self) {
        if self
            .hints
            .quiet_at
            .is_some_and(|spot| spot != self.hint_spot())
        {
            self.hints.quiet_at = None;
        }
    }

    /// Esc while a hint shows.
    pub(crate) fn dismiss_active_hint(&mut self) {
        if let Some(id) = self.active_hint().map(|hint| hint.id.clone()) {
            self.dismiss_hint(id);
        }
    }

    /// Acting on the element a hint explains dismisses it: a hint about
    /// something already done is noise.
    pub(crate) fn dismiss_hint_acted_on(&mut self, action: &Action) {
        let Some(id) = self.active_hint().map(|hint| hint.id.clone()) else {
            return;
        };
        let acts = match id.as_str() {
            "now.from_mode" => matches!(
                action,
                Action::NowOpen | Action::NowDone | Action::NowOpenEmail | Action::Reply
            ),
            "updates.let_go" => matches!(
                action,
                Action::NowLetGoDigest | Action::NowDone | Action::NowOpen
            ),
            "todo.runway" => matches!(action, Action::TodoPrimary),
            "todo.catchup" => matches!(action, Action::TodoOpenCatchup),
            "messages.topics" => matches!(
                action,
                Action::MessagesNextTopic | Action::MessagesPrevTopic
            ),
            "messages.got_it" => matches!(action, Action::MessagesAck),
            "archive.record" => matches!(action, Action::RecordsOpenEmail),
            "archive.answer" => matches!(
                action,
                Action::RecordsCopyReference | Action::RecordsOpenDocument
            ),
            _ => false,
        };
        if acts {
            self.dismiss_hint(id);
        }
    }

    fn dismiss_hint(&mut self, id: String) {
        self.hints.quiet_at = Some(self.hint_spot());
        self.hints.closed.insert(id.clone());
        let mutation = self.queue_best_effort_mutation(
            Request::SetHintSeen {
                hint: id.clone(),
                seen: true,
            },
            MutationEffect::StatusOnly(String::new()),
            String::new(),
        );
        self.hints.pending.insert(mutation, id);
    }

    /// The daemon didn't store a dismissal: the hint shows again at its
    /// next need, the safer miss.
    pub(crate) fn reopen_hint_after_failure(&mut self, failed: MutationId) {
        if let Some(id) = self.hints.pending.remove(&failed) {
            self.hints.closed.remove(&id);
        }
    }

    /// The first done here on a conversation: say in the status line that
    /// it only clears this mode. Once, in every client.
    pub(crate) fn explain_done_here_once(&mut self) {
        const DONE_HERE: &str = "done_here";
        let hint = [
            self.mailbox.messages_page.guide.as_ref(),
            self.mailbox.now_page.guide.as_ref(),
        ]
        .into_iter()
        .flatten()
        .flat_map(|guide| &guide.hints)
        .find(|hint| hint.id == DONE_HERE);
        let Some(hint) = hint else {
            return;
        };
        if hint.seen || self.hints.closed.contains(DONE_HERE) {
            return;
        }
        self.status_message = Some(hint.text.clone());
        self.dismiss_hint(DONE_HERE.to_string());
    }

    /// The status line while a hint shows: the hint, then how to dismiss it.
    pub(crate) fn hint_status_line(&self) -> Option<String> {
        self.active_hint()
            .map(|hint| format!("Hint: {} (Esc dismisses)", hint.text))
    }
}
