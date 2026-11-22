use crate::action::{Action, PatternKind};
use crate::keybindings::{format_keybinding, KeyBinding, KeyPress, KeybindingConfig, ViewContext};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::time::{Duration, Instant};

pub(crate) const MULTI_KEY_TIMEOUT: Duration = Duration::from_millis(4000);
pub(crate) const PREFIX_GUIDE_DELAY: Duration = Duration::from_millis(300);

fn plain_or_shift(modifiers: KeyModifiers) -> bool {
    modifiers.is_empty() || modifiers == KeyModifiers::SHIFT
}

fn pattern_select_chord(second: char) -> Option<Action> {
    let kind = match second {
        'a' => PatternKind::All,
        'n' => PatternKind::None,
        'r' => PatternKind::Read,
        'u' => PatternKind::Unread,
        's' => PatternKind::Starred,
        't' => PatternKind::Thread,
        _ => return None,
    };
    Some(Action::PatternSelect(kind))
}

fn g_view_chord(second: char) -> Option<Action> {
    match second {
        'q' => Some(Action::OpenReplyQueue),
        'o' => Some(Action::OpenOwedReplies),
        'v' => Some(Action::OpenCalendarInvites),
        'u' => Some(Action::OpenUpdates),
        'S' => Some(Action::OpenScreenerQueue),
        'F' => Some(Action::OpenFocus),
        'x' => Some(Action::OpenTodo),
        'm' => Some(Action::OpenMessages),
        'e' => Some(Action::OpenArchiveMode),
        '#' => Some(Action::GoToTrash),
        '!' => Some(Action::GoToSpam),
        'n' => Some(Action::OpenSnoozed),
        'w' => Some(Action::OpenMessages),
        'f' => Some(Action::OpenMailboxFilter),
        _ => None,
    }
}

fn g_tool_chord(second: char) -> Option<Action> {
    match second {
        'R' => Some(Action::OpenTab3),
        'C' => Some(Action::OpenTab4),
        'D' => Some(Action::OpenTab5),
        'P' => Some(Action::OpenTab7),
        'Y' => Some(Action::OpenActivityScreen),
        _ => None,
    }
}

#[derive(Debug)]
pub enum KeyState {
    Normal,
    WaitingForSecond { first: char, deadline: Instant },
}

pub struct InputHandler {
    state: KeyState,
    configured: Option<ConfiguredChord>,
    used_this_turn: bool,
    dead_end: Option<String>,
    #[cfg(test)]
    frozen_now: Option<Instant>,
}

struct ConfiguredChord {
    context: ViewContext,
    keys: Vec<KeyPress>,
    deadline: Instant,
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
            configured: None,
            used_this_turn: false,
            dead_end: None,
            #[cfg(test)]
            frozen_now: None,
        }
    }

    pub(crate) fn now(&self) -> Instant {
        #[cfg(test)]
        if let Some(now) = self.frozen_now {
            return now;
        }
        Instant::now()
    }

    pub(crate) fn take_dead_end(&mut self) -> Option<String> {
        self.dead_end.take()
    }

    pub(crate) fn cancel_prefix(&mut self, first: char, key: KeyEvent) {
        self.note_dead_end(&[
            KeyPress::from_event(KeyEvent::new(KeyCode::Char(first), KeyModifiers::NONE)),
            KeyPress::from_event(key),
        ]);
    }

    fn note_dead_end(&mut self, sequence: &[KeyPress]) {
        self.state = KeyState::Normal;
        self.configured = None;
        if sequence.last().is_some_and(|key| key.code == KeyCode::Esc) {
            return;
        }
        let spelled: Vec<String> = sequence
            .iter()
            .map(|key| {
                format_keybinding(&KeyBinding {
                    keys: vec![key.clone()],
                })
            })
            .collect();
        self.dead_end = Some(spelled.join(" "));
    }

    pub fn is_pending(&self) -> bool {
        self.configured
            .as_ref()
            .is_some_and(|chord| self.now() <= chord.deadline)
            || matches!(self.state, KeyState::WaitingForSecond { deadline, .. } if self.now() <= deadline)
    }

    pub fn pending_prefix(&self) -> Option<char> {
        if let Some(chord) = &self.configured {
            if self.now() <= chord.deadline {
                return chord.keys.first().and_then(|key| match key.code {
                    KeyCode::Char(c) => Some(c),
                    _ => None,
                });
            }
        }
        match self.state {
            KeyState::WaitingForSecond { first, .. } => Some(first),
            KeyState::Normal => None,
        }
    }

    pub(crate) fn prefix_waited(&self, now: Instant) -> Option<Duration> {
        let deadline = match (&self.configured, &self.state) {
            (Some(chord), _) => chord.deadline,
            (None, KeyState::WaitingForSecond { deadline, .. }) => *deadline,
            (None, KeyState::Normal) => return None,
        };
        (now <= deadline).then(|| MULTI_KEY_TIMEOUT.saturating_sub(deadline - now))
    }

    pub(crate) fn prefix_left(&self, now: Instant) -> Option<Duration> {
        self.prefix_waited(now)
            .map(|waited| MULTI_KEY_TIMEOUT.saturating_sub(waited))
    }

    #[cfg(test)]
    pub(crate) fn freeze_clock_for_tests(&mut self) {
        self.frozen_now = Some(Instant::now());
    }

    #[cfg(test)]
    pub(crate) fn advance_clock_for_tests(&mut self, by: Duration) {
        let now = self
            .frozen_now
            .expect("freeze the clock before advancing it");
        self.frozen_now = Some(now + by);
    }

    pub(crate) fn wait_for_second(&mut self, first: char) {
        self.used_this_turn = true;
        self.configured = None;
        self.state = KeyState::WaitingForSecond {
            first,
            deadline: self.now() + MULTI_KEY_TIMEOUT,
        };
    }

    pub(crate) fn take_waiting_first(&mut self) -> Option<char> {
        self.check_timeout();
        self.used_this_turn = true;
        match std::mem::replace(&mut self.state, KeyState::Normal) {
            KeyState::WaitingForSecond { first, .. } => Some(first),
            KeyState::Normal => None,
        }
    }

    pub fn reset(&mut self) {
        self.state = KeyState::Normal;
        self.configured = None;
    }

    pub(crate) fn begin_turn(&mut self) {
        self.used_this_turn = false;
    }

    pub(crate) fn end_turn(&mut self) {
        if !self.used_this_turn {
            self.reset();
        }
    }

    pub(crate) fn handle_configured_key(
        &mut self,
        config: &KeybindingConfig,
        context: ViewContext,
        key: KeyEvent,
    ) -> Option<Option<String>> {
        self.check_timeout();
        if self
            .configured
            .as_ref()
            .is_some_and(|chord| chord.context != context)
        {
            self.reset();
        }
        let press = KeyPress::from_event(key);
        let overrides_alias =
            config.overrides_legacy_alias(context, &press, || Self::new().handle_key(key));
        if self.configured.is_none()
            && !config.overrides_prefix(context, &press)
            && !overrides_alias
        {
            return None;
        }
        if self.configured.is_none() {
            if let KeyState::WaitingForSecond { first, .. } = self.state {
                let mut probe = Self::new();
                probe.handle_key(KeyEvent::new(KeyCode::Char(first), KeyModifiers::NONE));
                if probe.handle_key(key).is_some() {
                    return None;
                }
                self.used_this_turn = true;
                self.cancel_prefix(first, key);
                return Some(None);
            }
        }
        let pending = self.configured.take();
        if pending.is_none() && !config.overrides_prefix(context, &press) {
            if overrides_alias {
                self.used_this_turn = true;
                return Some(None);
            }
            return None;
        }
        self.used_this_turn = true;
        let had_pending_chord = pending.is_some();
        let mut sequence = pending.map(|chord| chord.keys).unwrap_or_default();
        sequence.push(press);
        if let Some(name) = crate::keybindings::resolve_action(config, context, &sequence) {
            self.state = KeyState::Normal;
            return Some(Some(name));
        }
        if config.bindings(context).keys().any(|binding| {
            binding.keys.starts_with(&sequence) && binding.keys.len() > sequence.len()
        }) {
            self.state = KeyState::Normal;
            self.configured = Some(ConfiguredChord {
                context,
                keys: sequence,
                deadline: self.now() + MULTI_KEY_TIMEOUT,
            });
            return Some(None);
        }
        if config.retired_sequence(context, &sequence) {
            self.state = KeyState::Normal;
            return Some(None);
        }
        if !had_pending_chord {
            self.state = KeyState::Normal;
            return Some(None);
        }
        self.state = KeyState::Normal;
        for previous in sequence.iter().take(sequence.len().saturating_sub(1)) {
            self.handle_key(KeyEvent::new(previous.code, previous.modifiers));
        }
        if matches!(self.state, KeyState::WaitingForSecond { .. }) {
            return None;
        }
        self.note_dead_end(&sequence);
        Some(None)
    }

    pub fn check_timeout(&mut self) -> Option<Action> {
        if self
            .configured
            .as_ref()
            .is_some_and(|chord| self.now() > chord.deadline)
        {
            self.reset();
        }
        if let KeyState::WaitingForSecond { deadline, .. } = &self.state {
            if self.now() > *deadline {
                self.state = KeyState::Normal;
                return None;
            }
        }
        None
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        self.used_this_turn = true;
        self.check_timeout();

        match (&self.state, key.code, key.modifiers) {
            (KeyState::Normal, KeyCode::Char('g'), KeyModifiers::NONE) => {
                self.state = KeyState::WaitingForSecond {
                    first: 'g',
                    deadline: self.now() + MULTI_KEY_TIMEOUT,
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
            (KeyState::WaitingForSecond { first: 'g', .. }, KeyCode::Char(c), modifiers)
                if plain_or_shift(modifiers) && g_tool_chord(c).is_some() =>
            {
                self.state = KeyState::Normal;
                g_tool_chord(c)
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
            (
                KeyState::WaitingForSecond { first: 'g', .. },
                KeyCode::Char('r'),
                KeyModifiers::NONE,
            ) => {
                self.state = KeyState::Normal;
                Some(Action::OpenReading)
            }
            (
                KeyState::WaitingForSecond { first: 'g', .. },
                KeyCode::Char('h'),
                KeyModifiers::NONE,
            ) => {
                self.state = KeyState::Normal;
                Some(Action::OpenNow)
            }
            (KeyState::WaitingForSecond { first: 'g', .. }, KeyCode::Char(c), modifiers)
                if plain_or_shift(modifiers) && g_view_chord(c).is_some() =>
            {
                self.state = KeyState::Normal;
                g_view_chord(c)
            }
            (
                KeyState::WaitingForSecond { first: 'g', .. },
                KeyCode::Char(c),
                KeyModifiers::NONE,
            ) if c.is_ascii_digit() => {
                self.state = KeyState::Normal;
                let index = c.to_digit(10).unwrap_or(0) as usize;
                Some(Action::OpenSavedSearchByIndex(index))
            }

            (KeyState::Normal, KeyCode::Char('i'), KeyModifiers::NONE) => {
                self.state = KeyState::WaitingForSecond {
                    first: 'i',
                    deadline: self.now() + MULTI_KEY_TIMEOUT,
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

            (KeyState::Normal, KeyCode::Char('*'), KeyModifiers::NONE) => {
                self.state = KeyState::WaitingForSecond {
                    first: '*',
                    deadline: self.now() + MULTI_KEY_TIMEOUT,
                };
                None
            }
            (
                KeyState::WaitingForSecond { first: '*', .. },
                KeyCode::Char(c),
                KeyModifiers::NONE,
            ) if pattern_select_chord(c).is_some() => {
                self.state = KeyState::Normal;
                pattern_select_chord(c)
            }

            (KeyState::WaitingForSecond { first, .. }, _, _) => {
                let first = *first;
                self.cancel_prefix(first, key);
                None
            }

            (KeyState::Normal, KeyCode::Char(':'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::OpenCommandPalette)
            }
            (KeyState::Normal, KeyCode::Char('k' | 'p'), KeyModifiers::CONTROL) => {
                Some(Action::OpenCommandPalette)
            }
            (KeyState::Normal, KeyCode::Char('@'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::ToggleAssistant)
            }
            (KeyState::Normal, KeyCode::Char('j') | KeyCode::Down, _) => Some(Action::MoveDown),
            (KeyState::Normal, KeyCode::Char('k') | KeyCode::Up, _) => Some(Action::MoveUp),
            (KeyState::Normal, KeyCode::Char('G'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::JumpBottom)
            }
            (KeyState::Normal, KeyCode::Char('d'), KeyModifiers::CONTROL) => Some(Action::PageDown),
            (KeyState::Normal, KeyCode::Char('u'), KeyModifiers::CONTROL) => Some(Action::PageUp),
            (KeyState::Normal, KeyCode::Home, KeyModifiers::NONE) => Some(Action::JumpTop),
            (KeyState::Normal, KeyCode::End, KeyModifiers::NONE) => Some(Action::JumpBottom),
            (KeyState::Normal, KeyCode::PageDown, KeyModifiers::NONE) => Some(Action::PageDown),
            (KeyState::Normal, KeyCode::PageUp, KeyModifiers::NONE) => Some(Action::PageUp),
            (KeyState::Normal, KeyCode::Tab, _) => Some(Action::SwitchPane),
            (KeyState::Normal, KeyCode::Enter, _)
            | (KeyState::Normal, KeyCode::Char('o'), KeyModifiers::NONE) => {
                Some(Action::OpenSelected)
            }
            (KeyState::Normal, KeyCode::Esc, _) => Some(Action::Back),
            (KeyState::Normal, KeyCode::Char('q'), _) => Some(Action::QuitView),
            (KeyState::Normal, KeyCode::Char('b'), KeyModifiers::NONE) => Some(Action::Snooze),
            (KeyState::Normal, KeyCode::Char('n'), KeyModifiers::NONE) => {
                Some(Action::NextSearchResult)
            }
            (KeyState::Normal, KeyCode::Char('N'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::PrevSearchResult)
            }

            (KeyState::Normal, KeyCode::Char('x'), KeyModifiers::NONE) => {
                Some(Action::ToggleSelect)
            }
            (KeyState::Normal, KeyCode::Char('A'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::AttachmentList)
            }
            (KeyState::Normal, KeyCode::Char('V'), modifiers) if plain_or_shift(modifiers) => {
                Some(Action::VisualLineMode)
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

    #[test]
    fn configured_chord_timeout_and_invalid_suffix_recover_without_sleep() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("keys.toml"),
            "[mail_list]\n\"wqr\" = \"trash\"\n\";\" = \"archive\"\n",
        )
        .unwrap();
        let config = crate::keybindings::load_keybindings(dir.path());
        let mut input = InputHandler::new();
        let context = ViewContext::MailList;
        assert_eq!(
            input.handle_configured_key(&config, context, key(KeyCode::Char('w'))),
            Some(None)
        );
        assert_eq!(
            input.handle_configured_key(&config, context, key(KeyCode::Char('q'))),
            Some(None)
        );
        assert_eq!(
            input.handle_configured_key(&config, context, key(KeyCode::Char('r'))),
            Some(Some("trash".into()))
        );
        input.handle_configured_key(&config, context, key(KeyCode::Char('w')));
        input.configured.as_mut().unwrap().deadline = Instant::now() - Duration::from_millis(1);
        assert_eq!(
            input.handle_configured_key(&config, context, key(KeyCode::Char(';'))),
            Some(Some("archive".into()))
        );
        input.handle_configured_key(&config, context, key(KeyCode::Char('w')));
        assert_eq!(
            input.handle_configured_key(&config, context, key(KeyCode::Char(';'))),
            Some(None),
            "a suffix that completes nothing cancels the chord and runs nothing"
        );
        assert_eq!(input.take_dead_end().as_deref(), Some("w ;"));
        assert!(!input.is_pending());
        input.handle_configured_key(&config, context, key(KeyCode::Char('w')));
        input.configured.as_mut().unwrap().deadline = Instant::now() - Duration::from_millis(1);
        assert_eq!(
            input.handle_configured_key(&config, context, key(KeyCode::Char('w'))),
            Some(None)
        );
        assert_eq!(
            input.handle_configured_key(&config, context, key(KeyCode::Char('q'))),
            Some(None)
        );
        assert_eq!(
            input.handle_configured_key(&config, context, key(KeyCode::Char('r'))),
            Some(Some("trash".into())),
            "a new configured chord can start after timeout"
        );
        assert!(!input.is_pending());
    }

    #[test]
    fn a_second_key_that_completes_nothing_cancels_the_prefix() {
        let mut input = InputHandler::new();
        assert_eq!(input.handle_key(key(KeyCode::Char('g'))), None);
        assert_eq!(input.handle_key(key(KeyCode::Char('j'))), None);
        assert_eq!(input.take_dead_end().as_deref(), Some("g j"));
        assert!(!input.is_pending());
        assert_eq!(
            input.handle_key(key(KeyCode::Char('j'))),
            Some(Action::MoveDown)
        );

        assert_eq!(input.handle_key(key(KeyCode::Char('*'))), None);
        assert_eq!(input.handle_key(key(KeyCode::Esc)), None);
        assert_eq!(input.take_dead_end(), None);
        assert!(!input.is_pending());
    }

    #[test]
    fn chord_g_then_a_view_letter_opens_the_view_the_web_opens() {
        for (second, modifiers, expected) in [
            ('q', KeyModifiers::NONE, Action::OpenReplyQueue),
            ('o', KeyModifiers::NONE, Action::OpenOwedReplies),
            ('v', KeyModifiers::NONE, Action::OpenCalendarInvites),
            ('u', KeyModifiers::NONE, Action::OpenUpdates),
            ('m', KeyModifiers::NONE, Action::OpenMessages),
            ('e', KeyModifiers::NONE, Action::OpenArchiveMode),
            ('S', KeyModifiers::SHIFT, Action::OpenScreenerQueue),
            ('n', KeyModifiers::NONE, Action::OpenSnoozed),
        ] {
            let mut input = InputHandler::new();
            assert_eq!(input.handle_key(key(KeyCode::Char('g'))), None);
            assert_eq!(
                input.handle_key(key_with(KeyCode::Char(second), modifiers)),
                Some(expected),
                "g {second}"
            );
        }
    }

    #[test]
    fn colon_and_ctrl_k_open_the_palette_and_at_the_assistant() {
        for (code, modifiers, expected) in [
            (
                KeyCode::Char(':'),
                KeyModifiers::SHIFT,
                Action::OpenCommandPalette,
            ),
            (
                KeyCode::Char('k'),
                KeyModifiers::CONTROL,
                Action::OpenCommandPalette,
            ),
            (
                KeyCode::Char('p'),
                KeyModifiers::CONTROL,
                Action::OpenCommandPalette,
            ),
            (
                KeyCode::Char('@'),
                KeyModifiers::SHIFT,
                Action::ToggleAssistant,
            ),
        ] {
            let mut input = InputHandler::new();
            assert_eq!(input.handle_key(key_with(code, modifiers)), Some(expected));
        }
    }

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
}
