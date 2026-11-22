use crossterm::event::KeyCode;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConfirmChoice {
    pub on_action: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmKey {
    Confirm,
    Keep,
    Moved,
    Ignored,
}

impl ConfirmChoice {
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
