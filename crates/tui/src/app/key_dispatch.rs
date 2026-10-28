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
            ActivePane::MailList if matches!(self.mailbox.mailbox_view, MailboxView::Place(_)) => {
                Some("place")
            }
            ActivePane::MailList => Some("list"),
            ActivePane::MessageView => Some("reader"),
            ActivePane::Sidebar => Some("sidebar"),
        })
    }

    /// A key that moved away from the list, reader, sidebar or place view
    /// says where its action went and runs nothing (D171). The modes do this
    /// in `defined_key`, after their own states have had the key. Returns
    /// true when it did.
    pub(crate) fn announce_moved_key(&mut self, key: KeyEvent) -> bool {
        if self.mode_key_context().is_some() {
            return false;
        }
        let (Some(place), Some(chord)) = (self.key_place(), spelling(key)) else {
            return false;
        };
        // With a prefix waiting, only the whole sequence counts: `A` moved
        // away, but `g A` still opens Analytics.
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

    /// Show "<action> is now <key>" when `keys` moved away from `place`.
    /// A screen that still reads its own keys (Rules, the drafts list)
    /// calls this with its own place name.
    pub(crate) fn announce_moved(&mut self, place: &str, keys: &str) -> bool {
        // The person's keys: a remap onto the old key drops its notice, and
        // the notice names the key they gave the action.
        let keymap = self.keybindings.keymap();
        let Some((action, moved)) = keymap.moved_entry(place, keys) else {
            return false;
        };
        // A retired key whose action has none left names where to go
        // instead (`1` → `g i`), under the old action's name.
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

    /// What `key` does in `context`, completing a waiting sequence first.
    /// A second key that completes nothing cancels the sequence and runs
    /// nothing, as the built-in chords do.
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
            // A retired sequence (`z z`) still waits for its second key
            // while its move is announced.
            if keymap.tui_starts_sequence(context, &chord)
                || keymap.moved_starts_sequence(context, &chord)
                || self.keybindings.modes.starts_sequence(context, &chord)
            {
                self.input.wait_for_second(first);
                return DefinedKey::Waiting;
            }
        }
        // Only now, once the mode's own states and bindings had the key, so
        // a moved letter typed into the ask box still types.
        self.announce_moved(context, &chord);
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
