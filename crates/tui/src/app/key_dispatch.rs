//! Keys in the modes, resolved through the key definition
//! (`crates/keymap/keymap.toml`, blueprint chapter 35). A mode's handler
//! deals first with its own states (previews, questions on the lit row),
//! then asks the definition what the key does in its context, so the keys
//! help, the hint bar and `mxr keys` describe are the keys that work.

use super::input::plain_or_shift;
use super::*;
use crate::keymap_names::action_named;
use ratatui::crossterm::event::KeyEvent;

/// What a key does in a mode's context.
pub(crate) enum DefinedKey {
    Action(Action),
    /// A key the pane handles in place (`inline:` in the definition).
    Inline(&'static str),
    /// The first key of a sequence; the next key completes it.
    Waiting,
    Unbound,
}

/// The key's spelling in the definition, as the dispatcher probe names
/// keys. `None` for a key no binding can spell.
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
    /// The key definition's name for where keys are going: a mode, or the
    /// mailbox's list, reader or sidebar.
    pub(crate) fn key_place(&self) -> Option<&'static str> {
        if self.screen != Screen::Mailbox {
            return None;
        }
        self.mode_key_context().or(match self.mailbox.active_pane {
            ActivePane::MailList => Some("list"),
            ActivePane::MessageView => Some("reader"),
            ActivePane::Sidebar => Some("sidebar"),
        })
    }

    /// A key that moved away from here in a recent release says where its
    /// action went and runs nothing (D171). Returns true when it did.
    pub(crate) fn announce_moved_key(&mut self, key: KeyEvent) -> bool {
        let (Some(place), Some(chord)) = (self.key_place(), spelling(key)) else {
            return false;
        };
        let keymap = mxr_keymap::defaults();
        let sequence = self
            .input
            .pending_prefix()
            .map(|first| format!("{first} {chord}"));
        let Some(action) = sequence
            .as_deref()
            .and_then(|seq| keymap.moved_from(place, seq))
            .or_else(|| keymap.moved_from(place, &chord))
        else {
            return false;
        };
        let now = action
            .tui
            .iter()
            .find(|binding| binding.contexts.includes(place))
            .or_else(|| action.tui.first())
            .and_then(|binding| binding.keys.first())
            .or_else(|| action.keys.first());
        self.input.reset();
        self.status_message = Some(match now {
            Some(now) => format!("{} is now {now}", action.label),
            None => format!("{} has no key now; find it in the palette", action.label),
        });
        true
    }

    /// What `key` does in `context`, completing a waiting sequence first.
    /// A second key that completes nothing cancels the sequence and runs
    /// nothing, as the built-in chords do.
    pub(crate) fn defined_key(&mut self, context: &str, key: KeyEvent) -> DefinedKey {
        let Some(chord) = spelling(key) else {
            self.input.reset();
            return DefinedKey::Unbound;
        };
        if let Some(first) = self.input.take_waiting_first() {
            if let Some(hit) = self.bound_key(context, &format!("{first} {chord}")) {
                return hit;
            }
            self.input.cancel_prefix(first, key);
            return DefinedKey::Unbound;
        }
        if let Some(hit) = self.bound_key(context, &chord) {
            return hit;
        }
        let mut chars = chord.chars();
        if let (Some(first), None) = (chars.next(), chars.next()) {
            if mxr_keymap::defaults().tui_starts_sequence(context, &chord)
                || self.keybindings.modes.starts_sequence(context, &chord)
            {
                self.input.wait_for_second(first);
                return DefinedKey::Waiting;
            }
        }
        DefinedKey::Unbound
    }

    /// What `keys` is bound to in `context`: the person's `keys.toml` mode
    /// section first, then the definition, minus defaults they moved away.
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

    /// Run a mode's key: the action it returns, or its in-place effect.
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

/// The in-place keys the modes use.
const INLINE: &[&str] = &["inline:focus_sidebar"];
