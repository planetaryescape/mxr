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
fn spelling(key: KeyEvent) -> Option<String> {
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
    /// The mode context whose keys come from the key definition, when the
    /// list is showing one. Modes move here one at a time.
    pub(crate) fn defined_key_context(&self) -> Option<&'static str> {
        self.mode_key_context()
            .filter(|context| matches!(*context, "todo" | "catchup" | "expired" | "now" | "messages"))
    }

    /// What `key` does in `context`, completing a waiting sequence first.
    /// A second key that completes nothing is read on its own, as the
    /// built-in chords read it.
    pub(crate) fn defined_key(&mut self, context: &str, key: KeyEvent) -> DefinedKey {
        let keymap = mxr_keymap::defaults();
        let Some(chord) = spelling(key) else {
            self.input.reset();
            return DefinedKey::Unbound;
        };
        if let Some(first) = self.input.take_waiting_first() {
            if let Some((_, binding)) = keymap.tui_binding(context, &format!("{first} {chord}")) {
                return resolved(&binding.name);
            }
        }
        if let Some((_, binding)) = keymap.tui_binding(context, &chord) {
            return resolved(&binding.name);
        }
        let mut chars = chord.chars();
        if let (Some(first), None) = (chars.next(), chars.next()) {
            if keymap.tui_starts_sequence(context, &chord) {
                self.input.wait_for_second(first);
                return DefinedKey::Waiting;
            }
        }
        DefinedKey::Unbound
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
