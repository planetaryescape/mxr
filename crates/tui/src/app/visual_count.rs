use super::*;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

/// Vim-style cap. A count larger than the list just reaches the last row.
const MAX_VISUAL_COUNT: usize = 9999;

// Counts only mean something in visual line mode: outside it, digits are the
// tab keys (1-5) and must keep that meaning.
impl App {
    /// `Some(None)` consumes the key. `None` leaves it to the normal router,
    /// which is also where a count is dropped by any key it does not use.
    pub(super) fn visual_count_key(&mut self, key: KeyEvent) -> Option<Option<Action>> {
        if !self.mailbox.visual_mode {
            self.mailbox.visual_count = None;
            return None;
        }
        if !matches!(
            self.current_ui_context(),
            UiContext::MailboxList | UiContext::SearchResults
        ) || self.input.is_pending()
        {
            return None;
        }

        let plain = key.modifiers.is_empty();
        let pending = self.mailbox.visual_count;
        match key.code {
            KeyCode::Char(digit @ '0'..='9') if plain && (digit != '0' || pending.is_some()) => {
                let digit = digit.to_digit(10).unwrap_or(0) as usize;
                let next = pending
                    .unwrap_or(0)
                    .saturating_mul(10)
                    .saturating_add(digit)
                    .min(MAX_VISUAL_COUNT);
                self.mailbox.visual_count = Some(next);
                self.status_message = Some(format!("-- VISUAL LINE -- {next}"));
                Some(None)
            }
            KeyCode::Esc if pending.is_some() => {
                self.mailbox.visual_count = None;
                self.status_message = Some("-- VISUAL LINE --".into());
                Some(None)
            }
            KeyCode::Char('j' | 'k') | KeyCode::Down | KeyCode::Up
                if plain && pending.is_some() =>
            {
                self.mailbox.visual_count = None;
                let down = matches!(key.code, KeyCode::Char('j') | KeyCode::Down);
                self.move_visual_cursor(pending.unwrap_or(1), down);
                Some(None)
            }
            _ => {
                self.mailbox.visual_count = None;
                None
            }
        }
    }

    /// Moves the cursor `count` rows, clamped to the list, and extends the
    /// visual range the same way a single j/k does.
    fn move_visual_cursor(&mut self, count: usize, down: bool) {
        if self.screen == Screen::Search {
            let last = self.search_row_count().saturating_sub(1);
            let cursor = self.search.page.selected_index;
            self.search.page.selected_index = if down {
                cursor.saturating_add(count).min(last)
            } else {
                cursor.saturating_sub(count)
            };
            self.sync_search_cursor_after_move();
        } else {
            let last = self.mail_row_count().saturating_sub(1);
            let cursor = self.mailbox.selected_index;
            self.mailbox.selected_index = if down {
                cursor.saturating_add(count).min(last)
            } else {
                cursor.saturating_sub(count)
            };
            self.ensure_visible();
            self.update_visual_selection();
            self.auto_preview();
        }
    }
}
