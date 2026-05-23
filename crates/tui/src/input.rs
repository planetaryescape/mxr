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
            (KeyState::WaitingForSecond { first: 'g', .. }, KeyCode::Char('L'), modifiers)
                if plain_or_shift(modifiers) =>
            {
                self.state = KeyState::Normal;
                Some(Action::OpenLogs)
            }
            (KeyState::WaitingForSecond { first: 'g', .. }, KeyCode::Char('A'), modifiers)
                if plain_or_shift(modifiers) =>
            {
                self.state = KeyState::Normal;
                Some(Action::OpenAnalyticsScreen)
            }
            // g <0-9>: jump to a saved-search tab by index. `g 0` returns
            // to the default inbox view; `g N` (1..=9) targets the Nth
            // saved search. Out-of-range indices are no-ops.
            (
                KeyState::WaitingForSecond { first: 'g', .. },
                KeyCode::Char(c),
                KeyModifiers::NONE,
            ) if c.is_ascii_digit() => {
                self.state = KeyState::Normal;
                let index = c.to_digit(10).unwrap_or(0) as usize;
                Some(Action::OpenSavedSearchByIndex(index))
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

            // Multi-key: i-prefix for iCal invite responses.
            //
            // Plain-suffix arms (`ia`/`im`/`id`) fire the auto-confirm + 1s
            // undo flow; SHIFT-suffix arms (`iA`/`iM`/`iD`) route to the
            // compose-with-comment path. The card hint text in
            // `message_view.rs` references these chords — keep in sync if
            // either side moves.
            (KeyState::Normal, KeyCode::Char('i'), KeyModifiers::NONE) => {
                self.state = KeyState::WaitingForSecond {
                    first: 'i',
                    deadline: Instant::now() + MULTI_KEY_TIMEOUT,
                };
                None
            }
            (
                KeyState::WaitingForSecond { first: 'i', .. },
                KeyCode::Char('a'),
                KeyModifiers::NONE,
            ) => {
                self.state = KeyState::Normal;
                Some(Action::RespondInvite(
                    mxr_protocol::CalendarInviteActionData::Accept,
                ))
            }
            (
                KeyState::WaitingForSecond { first: 'i', .. },
                KeyCode::Char('m'),
                KeyModifiers::NONE,
            ) => {
                self.state = KeyState::Normal;
                Some(Action::RespondInvite(
                    mxr_protocol::CalendarInviteActionData::Tentative,
                ))
            }
            (
                KeyState::WaitingForSecond { first: 'i', .. },
                KeyCode::Char('d'),
                KeyModifiers::NONE,
            ) => {
                self.state = KeyState::Normal;
                Some(Action::RespondInvite(
                    mxr_protocol::CalendarInviteActionData::Decline,
                ))
            }
            (KeyState::WaitingForSecond { first: 'i', .. }, KeyCode::Char('A'), modifiers)
                if plain_or_shift(modifiers) =>
            {
                self.state = KeyState::Normal;
                Some(Action::RespondInviteWithComment(
                    mxr_protocol::CalendarInviteActionData::Accept,
                ))
            }
            (KeyState::WaitingForSecond { first: 'i', .. }, KeyCode::Char('M'), modifiers)
                if plain_or_shift(modifiers) =>
            {
                self.state = KeyState::Normal;
                Some(Action::RespondInviteWithComment(
                    mxr_protocol::CalendarInviteActionData::Tentative,
                ))
            }
            (KeyState::WaitingForSecond { first: 'i', .. }, KeyCode::Char('D'), modifiers)
                if plain_or_shift(modifiers) =>
            {
                self.state = KeyState::Normal;
                Some(Action::RespondInviteWithComment(
                    mxr_protocol::CalendarInviteActionData::Decline,
                ))
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
            // Reply-later quick-mark: `b` for "bookmark for reply later".
            // Local-only intent — never roundtrips to the provider.
            (KeyState::Normal, KeyCode::Char('b'), KeyModifiers::NONE) => {
                Some(Action::FlagReplyLater)
            }
            (KeyState::Normal, KeyCode::Char('1'), KeyModifiers::NONE) => Some(Action::OpenTab1),
            (KeyState::Normal, KeyCode::Char('2'), KeyModifiers::NONE) => Some(Action::OpenTab2),
            (KeyState::Normal, KeyCode::Char('3'), KeyModifiers::NONE) => Some(Action::OpenTab3),
            (KeyState::Normal, KeyCode::Char('4'), KeyModifiers::NONE) => Some(Action::OpenTab4),
            (KeyState::Normal, KeyCode::Char('5'), KeyModifiers::NONE) => Some(Action::OpenTab5),
            (KeyState::Normal, KeyCode::Char('6'), KeyModifiers::NONE) => Some(Action::OpenTab6),
            (KeyState::Normal, KeyCode::Char('7'), KeyModifiers::NONE) => Some(Action::OpenTab7),

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
            #[cfg(debug_assertions)]
            (KeyState::Normal, KeyCode::Char('d'), modifiers)
                if modifiers.contains(KeyModifiers::CONTROL)
                    && modifiers.contains(KeyModifiers::ALT) =>
            {
                Some(Action::DumpActionTrace)
            }

            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn key_with(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    /// Slice 1 / B1.4: a single press of '6' from the normal state
    /// dispatches `OpenTab6` so users can reach the analytics screen
    /// from any other top-level view via the same numeric pattern as
    /// tabs 1-5. Catches "we added the tab to the bar but forgot to
    /// route the keystroke" regressions.
    #[test]
    fn pressing_six_returns_open_tab_6() {
        let mut input = InputHandler::new();
        let action = input.handle_key(key(KeyCode::Char('6')));
        assert_eq!(action, Some(Action::OpenTab6));
    }

    /// Slice 1 / B1.5: the chord `g` then `A` (capital, mirroring the
    /// existing `g L` -> OpenLogs convention) opens the analytics
    /// screen directly. Lowercase `g a` is already bound to
    /// GoToAllMail, so capital A is the analytics mnemonic.
    #[test]
    fn chord_g_then_capital_a_opens_analytics() {
        let mut input = InputHandler::new();
        // First key consumes silently while waiting for the second.
        let first = input.handle_key(key(KeyCode::Char('g')));
        assert_eq!(first, None, "g alone must wait, not act");
        let second = input.handle_key(key_with(KeyCode::Char('A'), KeyModifiers::SHIFT));
        assert_eq!(second, Some(Action::OpenAnalyticsScreen));
    }

    /// Slice 1 / B1.5 (continued): lowercase `g a` must keep its
    /// existing meaning (GoToAllMail). Guards against accidentally
    /// stealing a Gmail navigation chord when adding the analytics
    /// shortcut.
    #[test]
    fn chord_g_then_lowercase_a_still_goes_to_all_mail() {
        let mut input = InputHandler::new();
        let _ = input.handle_key(key(KeyCode::Char('g')));
        let action = input.handle_key(key(KeyCode::Char('a')));
        assert_eq!(action, Some(Action::GoToAllMail));
    }

    /// `g <digit>` jumps to the corresponding saved-search tab. `g 1`
    /// targets the first saved search, `g 9` the ninth. The action is
    /// 1-indexed; `g 0` is reserved for "return to default inbox".
    #[test]
    fn chord_g_then_digit_one_through_nine_jumps_to_saved_search() {
        for digit in 1..=9 {
            let mut input = InputHandler::new();
            let _ = input.handle_key(key(KeyCode::Char('g')));
            let action = input.handle_key(key(KeyCode::Char(char::from_digit(digit, 10).unwrap())));
            assert_eq!(
                action,
                Some(Action::OpenSavedSearchByIndex(digit as usize)),
                "g {digit} should target saved-search index {digit}"
            );
        }
    }

    /// `g 0` returns to the default inbox view, clearing any active
    /// saved-search filter. Mirrors Apple Mail / Gmail "g 0 = inbox" muscle
    /// memory while staying inside the `g`-prefix family.
    #[test]
    fn chord_g_then_zero_returns_to_default_inbox() {
        let mut input = InputHandler::new();
        let _ = input.handle_key(key(KeyCode::Char('g')));
        let action = input.handle_key(key(KeyCode::Char('0')));
        assert_eq!(action, Some(Action::OpenSavedSearchByIndex(0)));
    }

    /// Bare digits keep their existing meaning (screen tabs). The
    /// chord-prefix is required for saved-search navigation. Guards
    /// against stealing the existing `1`-`6` screen-tab muscle memory.
    #[test]
    fn bare_digit_still_opens_screen_tab_not_saved_search() {
        let mut input = InputHandler::new();
        let action = input.handle_key(key(KeyCode::Char('1')));
        assert_eq!(action, Some(Action::OpenTab1));
    }

    /// `i` followed by `a`/`m`/`d` fires the auto-confirm invite RSVP path.
    /// The chord-prefix wiring is the load-bearing fix for the original bug
    /// where `ia` fell through to ReplyAll. See `calendar-email` docs.
    #[test]
    fn chord_i_then_a_responds_invite_accept() {
        let mut input = InputHandler::new();
        let first = input.handle_key(key(KeyCode::Char('i')));
        assert_eq!(first, None, "i alone must wait");
        let action = input.handle_key(key(KeyCode::Char('a')));
        assert_eq!(
            action,
            Some(Action::RespondInvite(
                mxr_protocol::CalendarInviteActionData::Accept
            ))
        );
    }

    #[test]
    fn chord_i_then_m_responds_invite_tentative() {
        let mut input = InputHandler::new();
        let _ = input.handle_key(key(KeyCode::Char('i')));
        let action = input.handle_key(key(KeyCode::Char('m')));
        assert_eq!(
            action,
            Some(Action::RespondInvite(
                mxr_protocol::CalendarInviteActionData::Tentative
            ))
        );
    }

    #[test]
    fn chord_i_then_d_responds_invite_decline() {
        let mut input = InputHandler::new();
        let _ = input.handle_key(key(KeyCode::Char('i')));
        let action = input.handle_key(key(KeyCode::Char('d')));
        assert_eq!(
            action,
            Some(Action::RespondInvite(
                mxr_protocol::CalendarInviteActionData::Decline
            ))
        );
    }

    /// Shift-suffix routes to the compose-with-comment path. Crossterm
    /// distinguishes `Char('a'), NONE` from `Char('A'), SHIFT` so the same
    /// `i` prefix can dispatch two different actions cleanly.
    #[test]
    fn chord_i_then_shift_a_responds_invite_with_comment_accept() {
        let mut input = InputHandler::new();
        let _ = input.handle_key(key(KeyCode::Char('i')));
        let action = input.handle_key(key_with(KeyCode::Char('A'), KeyModifiers::SHIFT));
        assert_eq!(
            action,
            Some(Action::RespondInviteWithComment(
                mxr_protocol::CalendarInviteActionData::Accept
            ))
        );
    }
}
