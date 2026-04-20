use crate::action::Action;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::time::{Duration, Instant};

const MULTI_KEY_TIMEOUT: Duration = Duration::from_millis(500);

fn plain_or_shift(modifiers: KeyModifiers) -> bool {
    modifiers.is_empty() || modifiers == KeyModifiers::SHIFT
}

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
            // Multi-key: g prefix
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
            (
                KeyState::WaitingForSecond { first: 'g', .. },
                KeyCode::Char('i'),
                KeyModifiers::NONE,
            ) => {
                self.state = KeyState::Normal;
                Some(Action::GoToInbox)
            }
            (
                KeyState::WaitingForSecond { first: 'g', .. },
                KeyCode::Char('s'),
                KeyModifiers::NONE,
            ) => {
                self.state = KeyState::Normal;
                Some(Action::GoToStarred)
            }
            (
                KeyState::WaitingForSecond { first: 'g', .. },
                KeyCode::Char('t'),
                KeyModifiers::NONE,
            ) => {
                self.state = KeyState::Normal;
                Some(Action::GoToSent)
            }
            (
                KeyState::WaitingForSecond { first: 'g', .. },
                KeyCode::Char('d'),
                KeyModifiers::NONE,
            ) => {
                self.state = KeyState::Normal;
                Some(Action::GoToDrafts)
            }
            (
                KeyState::WaitingForSecond { first: 'g', .. },
                KeyCode::Char('a'),
                KeyModifiers::NONE,
            ) => {
                self.state = KeyState::Normal;
                Some(Action::GoToAllMail)
            }
            (
                KeyState::WaitingForSecond { first: 'g', .. },
                KeyCode::Char('l'),
                KeyModifiers::NONE,
            ) => {
                self.state = KeyState::Normal;
                Some(Action::GoToLabel)
            }
            (
                KeyState::WaitingForSecond { first: 'g', .. },
                KeyCode::Char('c'),
                KeyModifiers::NONE,
            ) => {
                self.state = KeyState::Normal;
                Some(Action::EditConfig)
            }
            (
                KeyState::WaitingForSecond { first: 'g', .. },
                KeyCode::Char('L'),
                modifiers,
            ) if plain_or_shift(modifiers) => {
                self.state = KeyState::Normal;
                Some(Action::OpenLogs)
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
            (KeyState::Normal, KeyCode::Char('G'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::JumpBottom)
            }
            (KeyState::Normal, KeyCode::Char('d'), KeyModifiers::CONTROL) => Some(Action::PageDown),
            (KeyState::Normal, KeyCode::Char('u'), KeyModifiers::CONTROL) => Some(Action::PageUp),
            (KeyState::Normal, KeyCode::Char('H'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::ViewportTop)
            }
            (KeyState::Normal, KeyCode::Char('M'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::ViewportMiddle)
            }
            (KeyState::Normal, KeyCode::Char('L'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::ViewportBottom)
            }
            (KeyState::Normal, KeyCode::Tab, _) => Some(Action::SwitchPane),
            (KeyState::Normal, KeyCode::Enter, _)
            | (KeyState::Normal, KeyCode::Char('o'), KeyModifiers::NONE) => {
                Some(Action::OpenSelected)
            }
            (KeyState::Normal, KeyCode::Esc, _) => Some(Action::Back),
            (KeyState::Normal, KeyCode::Char('q'), _) => Some(Action::QuitView),
            (KeyState::Normal, KeyCode::Char('1'), KeyModifiers::NONE) => Some(Action::OpenTab1),
            (KeyState::Normal, KeyCode::Char('2'), KeyModifiers::NONE) => Some(Action::OpenTab2),
            (KeyState::Normal, KeyCode::Char('3'), KeyModifiers::NONE) => Some(Action::OpenTab3),
            (KeyState::Normal, KeyCode::Char('4'), KeyModifiers::NONE) => Some(Action::OpenTab4),
            (KeyState::Normal, KeyCode::Char('5'), KeyModifiers::NONE) => Some(Action::OpenTab5),

            // Search navigation
            (KeyState::Normal, KeyCode::Char('n'), KeyModifiers::NONE) => {
                Some(Action::NextSearchResult)
            }
            (KeyState::Normal, KeyCode::Char('N'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::PrevSearchResult)
            }

            // Command palette
            (KeyState::Normal, KeyCode::Char('p'), KeyModifiers::CONTROL) => {
                Some(Action::OpenCommandPalette)
            }

            // Shared selection / layout
            (KeyState::Normal, KeyCode::Char('x'), KeyModifiers::NONE) => {
                Some(Action::ToggleSelect)
            }
            (KeyState::Normal, KeyCode::Char('A'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::AttachmentList)
            }
            (KeyState::Normal, KeyCode::Char('V'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::VisualLineMode)
            }
            (KeyState::Normal, KeyCode::Char('E'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::ExportThread)
            }
            (KeyState::Normal, KeyCode::Char('F'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::ToggleFullscreen)
            }
            (KeyState::Normal, KeyCode::Char('?'), _) => Some(Action::Help),

            _ => None,
        }
    }
}
