//! `H` in the reader: the item's highlights, to edit a note or remove one.
//! The daemon decides what may change (a highlight made in Marginalia
//! can't); this turns keys into `UpdateHighlight` and `RemoveHighlight`.
//! Removing asks first, naming the exact highlight Enter will remove (by
//! id, the same one the daemon's preview would name), and `u` undoes it.

use super::*;
use crate::app::state::{HighlightStep, HighlightsPanel};
use mxr_protocol::ReadingHighlightData;

/// The reason, shown instead of asking the daemon to refuse.
const MADE_ELSEWHERE: &str = "Made in Marginalia; change it there";

impl App {
    pub(super) fn reading_highlights_open(&mut self) {
        let page = &mut self.mailbox.reading_page;
        let Some(reader) = &page.reader else {
            self.status_message = Some("Open an item first: Enter reads it".into());
            return;
        };
        if reader.highlights.is_empty() {
            self.status_message = Some("No highlights yet: h saves the paragraph at the top".into());
            return;
        }
        page.highlights = Some(HighlightsPanel::default());
    }

    /// The highlight the panel has selected, clamped: a refetch after a
    /// removal may leave the index past the end.
    pub(crate) fn selected_highlight(&self) -> Option<&ReadingHighlightData> {
        let page = &self.mailbox.reading_page;
        let panel = page.highlights.as_ref()?;
        let highlights = &page.reader.as_ref()?.highlights;
        highlights.get(panel.selected.min(highlights.len().checked_sub(1)?))
    }

    /// Keys while the panel is open.
    pub(super) fn highlights_key(&mut self, key: crossterm::event::KeyEvent) {
        let Some(step) = self
            .mailbox
            .reading_page
            .highlights
            .as_ref()
            .map(|panel| panel.step.clone())
        else {
            return;
        };
        match step {
            HighlightStep::Browse => self.highlights_browse_key(key.code),
            HighlightStep::Note(note) => self.highlights_note_key(key, note),
            HighlightStep::Remove => self.highlights_remove_key(key.code),
        }
    }

    fn set_step(&mut self, step: HighlightStep) {
        if let Some(panel) = self.mailbox.reading_page.highlights.as_mut() {
            panel.step = step;
        }
    }

    fn highlights_browse_key(&mut self, code: KeyCode) {
        let count = self
            .mailbox
            .reading_page
            .reader
            .as_ref()
            .map_or(0, |reader| reader.highlights.len());
        let Some(panel) = self.mailbox.reading_page.highlights.as_mut() else {
            return;
        };
        match code {
            KeyCode::Esc | KeyCode::Char('q' | 'H') => {
                self.mailbox.reading_page.highlights = None;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                panel.selected = (panel.selected + 1).min(count.saturating_sub(1));
            }
            KeyCode::Char('k') | KeyCode::Up => {
                panel.selected = panel.selected.saturating_sub(1);
            }
            KeyCode::Char(c @ ('e' | 'x')) => {
                let Some(highlight) = self.selected_highlight() else {
                    return;
                };
                if highlight.origin != "mxr" {
                    self.status_message = Some(MADE_ELSEWHERE.into());
                    return;
                }
                let step = if c == 'e' {
                    HighlightStep::Note(highlight.note.clone().unwrap_or_default())
                } else {
                    HighlightStep::Remove
                };
                self.set_step(step);
            }
            KeyCode::Char('u') => {
                // The same undo as everywhere else; the panel stays open.
                self.apply(Action::UndoLastMutation);
            }
            _ => {}
        }
    }

    fn highlights_note_key(&mut self, key: crossterm::event::KeyEvent, mut note: String) {
        match key.code {
            KeyCode::Esc => self.set_step(HighlightStep::Browse),
            KeyCode::Enter => {
                let Some(id) = self.selected_highlight().map(|h| h.id.clone()) else {
                    return;
                };
                self.set_step(HighlightStep::Browse);
                self.queue_mutation(
                    Request::UpdateHighlight {
                        highlight_id: id,
                        note,
                        dry_run: false,
                    },
                    MutationEffect::ModeDone("Note saved".into()),
                    "Saving the note...".into(),
                );
            }
            KeyCode::Backspace => {
                note.pop();
                self.set_step(HighlightStep::Note(note));
            }
            KeyCode::Char(c)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                note.push(c);
                self.set_step(HighlightStep::Note(note));
            }
            _ => {}
        }
    }

    fn highlights_remove_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Enter | KeyCode::Char('y') => {
                let Some(id) = self.selected_highlight().map(|h| h.id.clone()) else {
                    return;
                };
                self.set_step(HighlightStep::Browse);
                self.queue_mutation(
                    Request::RemoveHighlight {
                        highlight_id: id,
                        dry_run: false,
                    },
                    MutationEffect::ModeDone("Highlight removed".into()),
                    "Removing the highlight...".into(),
                );
            }
            KeyCode::Esc | KeyCode::Char('n') => self.set_step(HighlightStep::Browse),
            _ => {}
        }
    }
}
