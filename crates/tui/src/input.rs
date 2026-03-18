use crate::action::Action;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::time::{Duration, Instant};

const MULTI_KEY_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Debug)]
pub enum KeyState {
    Normal,
    WaitingForSecond { first: char, deadline: Instant },
}

pub struct InputHandler {
    state: KeyState,
}

impl Default for InputHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl InputHandler {
    pub fn new() -> Self {
        Self {
            state: KeyState::Normal,
        }
    }

    pub fn is_pending(&self) -> bool {
        matches!(self.state, KeyState::WaitingForSecond { .. })
    }

    pub fn check_timeout(&mut self) -> Option<Action> {
        if let KeyState::WaitingForSecond { deadline, .. } = &self.state {
            if Instant::now() > *deadline {
                self.state = KeyState::Normal;
                return None;
            }
        }
        None
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        self.check_timeout();

        match (&self.state, key.code, key.modifiers) {
            // Multi-key: gg
            (KeyState::Normal, KeyCode::Char('g'), KeyModifiers::NONE) => {
                self.state = KeyState::WaitingForSecond {
                    first: 'g',
                    deadline: Instant::now() + MULTI_KEY_TIMEOUT,
                };
                None
            }
            (
                KeyState::WaitingForSecond { first: 'g', .. },
                KeyCode::Char('g'),
                KeyModifiers::NONE,
            ) => {
                self.state = KeyState::Normal;
                Some(Action::JumpTop)
            }

            // Multi-key: zz
            (KeyState::Normal, KeyCode::Char('z'), KeyModifiers::NONE) => {
                self.state = KeyState::WaitingForSecond {
                    first: 'z',
                    deadline: Instant::now() + MULTI_KEY_TIMEOUT,
                };
                None
            }
            (
                KeyState::WaitingForSecond { first: 'z', .. },
                KeyCode::Char('z'),
                KeyModifiers::NONE,
            ) => {
                self.state = KeyState::Normal;
                Some(Action::CenterCurrent)
            }

            (KeyState::WaitingForSecond { .. }, _, _) => {
                self.state = KeyState::Normal;
                self.handle_key(key)
            }

            // Single keys
            (KeyState::Normal, KeyCode::Char('j') | KeyCode::Down, _) => Some(Action::MoveDown),
            (KeyState::Normal, KeyCode::Char('k') | KeyCode::Up, _) => Some(Action::MoveUp),
            (KeyState::Normal, KeyCode::Char('G'), KeyModifiers::SHIFT) => Some(Action::JumpBottom),
            (KeyState::Normal, KeyCode::Char('d'), KeyModifiers::CONTROL) => Some(Action::PageDown),
            (KeyState::Normal, KeyCode::Char('u'), KeyModifiers::CONTROL) => Some(Action::PageUp),
            (KeyState::Normal, KeyCode::Char('H'), KeyModifiers::SHIFT) => {
                Some(Action::ViewportTop)
            }
            (KeyState::Normal, KeyCode::Char('M'), KeyModifiers::SHIFT) => {
                Some(Action::ViewportMiddle)
            }
            (KeyState::Normal, KeyCode::Char('L'), KeyModifiers::SHIFT) => {
                Some(Action::ViewportBottom)
            }
            (KeyState::Normal, KeyCode::Tab, _) => Some(Action::SwitchPane),
            (KeyState::Normal, KeyCode::Enter, _)
            | (KeyState::Normal, KeyCode::Char('o'), KeyModifiers::NONE) => {
                Some(Action::OpenSelected)
            }
            (KeyState::Normal, KeyCode::Esc, _) => Some(Action::Back),
            (KeyState::Normal, KeyCode::Char('q'), _) => Some(Action::QuitView),

            _ => None,
        }
    }
}
