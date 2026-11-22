
use super::input::plain_or_shift;
use super::*;
use crate::keymap_names::action_named;
use ratatui::crossterm::event::KeyEvent;

pub(crate) enum DefinedKey {
    Action(Action),
    Inline(&'static str),
    Waiting,
    Unbound,
}

pub(crate) fn spelling(key: KeyEvent) -> Option<String> {
    let named = |name: &str| (key.modifiers == KeyModifiers::NONE).then(|| name.to_string());
    match key.code {
        KeyCode::Char(' ') => named("Space"),
        KeyCode::Char(c) if key.modifiers == KeyModifiers::CONTROL => Some(format!("Ctrl+{c}")),
        KeyCode::Char(c) if plain_or_shift(key.modifiers) => Some(c.to_string()),
        KeyCode::Enter => named("Enter"),
        KeyCode::Esc => named("Escape"),
        KeyCode::Tab => named("Tab"),
        KeyCode::Backspace => named("Backspace"),
        KeyCode::Delete => named("Delete"),
        KeyCode::Up => named("ArrowUp"),
        KeyCode::Down => named("ArrowDown"),
        KeyCode::Left => named("ArrowLeft"),
        KeyCode::Right => named("ArrowRight"),
        KeyCode::Home => named("Home"),
        KeyCode::End => named("End"),
        KeyCode::PageUp => named("PageUp"),
        KeyCode::PageDown => named("PageDown"),
        _ => None,
    }
}

impl App {
    pub(crate) fn key_place(&self) -> Option<&'static str> {
        if self.screen != Screen::Mailbox {
            return None;
        }
        self.mode_key_context().or(match self.mailbox.active_pane {
            ActivePane::MailList if matches!(self.mailbox.mailbox_view, MailboxView::Place(_)) => {
                Some("place")
            }
            ActivePane::MailList => Some("list"),
            ActivePane::MessageView => Some("reader"),
            ActivePane::Sidebar => Some("sidebar"),
        })
    }

    pub(crate) fn announce_moved_key(&mut self, key: KeyEvent) -> bool {
        if self.mode_key_context().is_some() {
            return false;
        }
        let (Some(place), Some(chord)) = (self.key_place(), spelling(key)) else {
            return false;
        };
        let keys = match self.input.pending_prefix() {
            Some(first) => format!("{first} {chord}"),
            None => chord,
        };
        let announced = self.announce_moved(place, &keys);
        if announced {
            self.input.reset();
        }
        announced
    }

    pub(crate) fn announce_moved(&mut self, place: &str, keys: &str) -> bool {
        let keymap = self.keybindings.keymap();
        let Some((action, moved)) = keymap.moved_entry(place, keys) else {
            return false;
        };
        let target = keymap.moved_target(action, moved);
        let remapped = self.keybindings.modes.key_for(place, &target.id);
        let now = moved
            .to_key
            .as_deref()
            .or(remapped)
            .or_else(|| {
                target
                    .tui
                    .iter()
                    .find(|binding| binding.contexts.includes(place))
                    .or_else(|| action.tui.first())
                    .and_then(|binding| binding.keys.first())
                    .or_else(|| target.keys.first())
                    .map(String::as_str)
            })
            .map(String::from);
        let name = action
            .short_label
            .as_deref()
            .unwrap_or(&action.label)
            .trim_end_matches('…');
        self.status_message = Some(match now {
            Some(now) => format!("{name} is now {now}"),
            None => format!("{name} has no key now; find it in the palette"),
        });
        true
    }

    pub(crate) fn defined_key(&mut self, context: &str, key: KeyEvent) -> DefinedKey {
        let Some(chord) = spelling(key) else {
            self.input.reset();
            return DefinedKey::Unbound;
        };
        if let Some(first) = self.input.take_waiting_first() {
            let sequence = format!("{first} {chord}");
            if let Some(hit) = self.bound_key(context, &sequence) {
                return hit;
            }
            if !self.announce_moved(context, &sequence) {
                self.input.cancel_prefix(first, key);
            }
            return DefinedKey::Unbound;
        }
        if let Some(hit) = self.bound_key(context, &chord) {
            return hit;
        }
        let mut chars = chord.chars();
        if let (Some(first), None) = (chars.next(), chars.next()) {
            let keymap = mxr_keymap::defaults();
            if keymap.tui_starts_sequence(context, &chord)
                || self
                    .keybindings
                    .keymap()
                    .moved_starts_sequence(context, &chord)
                || self.keybindings.modes.starts_sequence(context, &chord)
            {
                self.input.wait_for_second(first);
                return DefinedKey::Waiting;
            }
        }
        self.announce_moved(context, &chord);
        DefinedKey::Unbound
    }

    fn bound_key(&self, context: &str, keys: &str) -> Option<DefinedKey> {
        let keymap = mxr_keymap::defaults();
        let remaps = &self.keybindings.modes;
        match remaps.lookup(context, keys) {
            Some(Some(id)) => {
                let action = keymap.get(id)?;
                let binding = action
                    .tui
                    .iter()
                    .find(|binding| binding.contexts.includes(context))
                    .or_else(|| action.tui.first())?;
                return Some(resolved(&binding.name));
            }
            Some(None) => return Some(DefinedKey::Unbound),
            None => {}
        }
        let (action, binding) = keymap.tui_binding(context, keys)?;
        if remaps.retires(context, &action.id) {
            return Some(DefinedKey::Unbound);
        }
        Some(resolved(&binding.name))
    }

    pub(crate) fn run_defined_key(&mut self, key: DefinedKey) -> Option<Action> {
        match key {
            DefinedKey::Action(action) => Some(action),
            DefinedKey::Inline("inline:focus_sidebar") => {
                self.mailbox.active_pane = ActivePane::Sidebar;
                None
            }
            DefinedKey::Inline(_) | DefinedKey::Waiting | DefinedKey::Unbound => None,
        }
    }
}

fn resolved(name: &str) -> DefinedKey {
    if let Some(inline) = INLINE.iter().find(|inline| **inline == name) {
        return DefinedKey::Inline(inline);
    }
    action_named(name).map_or(DefinedKey::Unbound, DefinedKey::Action)
}

const INLINE: &[&str] = &["inline:focus_sidebar"];
