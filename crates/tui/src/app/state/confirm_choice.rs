//! A confirm that destroys something, or can't be undone, keeps two options
//! on screen and opens on Keep, as the web's dialogs open on Cancel
//! (blueprint 35, "Destroy": never on `Esc` or a bare `Enter` default).
//! `Enter` runs the lit option, `y` is the named action, `n`, `Esc` and `q`
//! keep, and `j`, `k` and the arrows move between the two.

use crossterm::event::KeyCode;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConfirmChoice {
    /// The named action is lit instead of Keep.
    pub on_action: bool,
}

/// What a key does in a two-option confirm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmKey {
    Confirm,
    Keep,
    /// The highlight moved; nothing ran.
    Moved,
    Ignored,
}

impl ConfirmChoice {
    /// A confirm that changes nothing it can't put back may open on its
    /// action, so `Enter` is yes.
    pub fn on_action() -> Self {
        Self { on_action: true }
    }

    pub fn key(&mut self, code: KeyCode) -> ConfirmKey {
        match code {
            KeyCode::Char('y') => ConfirmKey::Confirm,
            KeyCode::Char('n' | 'q') | KeyCode::Esc => ConfirmKey::Keep,
            KeyCode::Enter if self.on_action => ConfirmKey::Confirm,
            KeyCode::Enter => ConfirmKey::Keep,
            KeyCode::Char('j' | 'k' | 'h' | 'l')
            | KeyCode::Up
            | KeyCode::Down
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::Tab
            | KeyCode::BackTab => {
                self.on_action = !self.on_action;
                ConfirmKey::Moved
            }
            _ => ConfirmKey::Ignored,
        }
    }
}
