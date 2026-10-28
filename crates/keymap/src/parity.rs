//! The clients use the same keys (blueprint 35, rule 8). In each place both
//! clients have, a key must do the same thing in the web app and the TUI,
//! unless the definition itself says why not:
//!
//! - the action is for one client only (`clients`), with a `note` saying why;
//! - the key is one a terminal cannot deliver ([`tui_cannot_deliver`]), so
//!   only the web binds it;
//! - the TUI holds the key while a retired sequence that starts with it is
//!   announced (`z` while `z z` says where it went);
//! - a `[[tui_pass_through]]` entry lets the key reach a pane beside the
//!   focused one in the TUI, where it does what both clients do there.
//!
//! The overlays both clients have are compared the same way.
//!
//! The web's `Mod` is the TUI's `Ctrl`, so `Mod+k` and `Ctrl+k` are one key.

use crate::{Client, KeyAction, Keymap, TuiBinding};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// The places both clients have, with the web scopes live in each,
/// innermost first.
const SHARED_PLACES: [(&str, &[&str]); 11] = [
    ("list", &["list", "global"]),
    ("reader", &["reader", "global"]),
    ("sidebar", &["sidebar", "global"]),
    ("todo", &["todo", "global"]),
    ("now", &["now", "global"]),
    ("messages", &["messages", "messages-filters", "global"]),
    ("archive", &["archive", "global"]),
    ("updates", &["updates", "global"]),
    ("reading", &["reading", "global"]),
    ("focus", &["focus", "global"]),
    // The TUI's screener is a modal that takes every key.
    ("screener", &["screener"]),
];

/// The overlays both clients have, with the same grammar. An overlay owns
/// every key while it is open, so only its own keys are live there. Help,
/// the palette, the label picker, Categories and Reply later are filter
/// lists on the web, where letters type, and the web's bulk and sweep
/// previews are two-button dialogs while the TUI's are scrolling sheets, so
/// those are listed per client and not compared.
const SHARED_OVERLAYS: [&str; 13] = [
    "links",
    "unsubscribe",
    "move-picker",
    "move-picker-sender",
    "new-sender-picker",
    "sender-kind-picker",
    "correct-todo",
    "not-now",
    "delete-confirm",
    "sender-ask",
    "assistant-approval",
    "assistant-question",
    "assistant-history",
];

/// Keys TUI defaults never use, because a terminal sends something else
/// or nothing (blueprint 35, "Terminal and browser keys"): `Ctrl-i`,
/// `Ctrl-m`, `Ctrl-h` and `Ctrl-[` arrive as Tab, Enter, Backspace and Esc;
/// `Ctrl-s` and `Ctrl-q` are flow control; `Ctrl-b` and `Ctrl-a` are the
/// tmux and screen prefixes; `Ctrl-Enter` and `Ctrl-.` need the kitty
/// protocol; Shift+Space arrives as Space.
const TUI_UNDELIVERABLE: [&str; 11] = [
    "Ctrl+i",
    "Ctrl+m",
    "Ctrl+h",
    "Ctrl+[",
    "Ctrl+s",
    "Ctrl+q",
    "Ctrl+b",
    "Ctrl+a",
    "Ctrl+Enter",
    "Ctrl+.",
    "Shift+Space",
];

/// True when a terminal cannot deliver `key` (in the TUI's spelling):
/// the keys above, any `Ctrl+Shift` chord, and `Alt`, which macOS
/// terminals send only with "Option as Meta".
pub fn tui_cannot_deliver(key: &str) -> bool {
    key.split(' ').any(|chord| {
        TUI_UNDELIVERABLE.contains(&chord)
            || chord.contains("Ctrl+Shift+")
            || chord.split('+').any(|part| part == "Alt")
    })
}

/// The web's `Mod` is Cmd or Ctrl; a terminal has no Cmd, so the TUI's is
/// Ctrl.
fn tui_spelling(web_key: &str) -> String {
    web_key.replace("Mod+", "Ctrl+")
}

/// Keys the TUI lets reach a pane beside the focused one. The TUI draws the
/// sidebar, the mail list and the reader side by side, and its modes in the
/// list's pane; the web's keys follow focus. A TUI key in a `from` place is
/// accepted when it does what both clients do in `to`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TuiPassThrough {
    pub from: Vec<String>,
    pub to: String,
    pub why: String,
}

struct WebKey<'a> {
    action: &'a KeyAction,
    chord: String,
}

struct TuiKey<'a> {
    action: &'a KeyAction,
    binding: &'a TuiBinding,
}

impl Keymap {
    /// What the web binds in `place`, keyed by the TUI's spelling.
    fn web_keys_in(&self, scopes: &[&str]) -> BTreeMap<String, WebKey<'_>> {
        let mut keys = BTreeMap::new();
        for scope in scopes.iter().rev() {
            for action in self.actions.iter().filter(|a| a.is_for(Client::Web)) {
                if !action.web_scopes().contains(scope) {
                    continue;
                }
                for chord in action.keys.iter().chain(&action.mac_keys) {
                    keys.insert(
                        tui_spelling(chord),
                        WebKey {
                            action,
                            chord: chord.clone(),
                        },
                    );
                }
            }
        }
        keys
    }

    fn tui_keys_in(&self, place: &str) -> BTreeMap<String, TuiKey<'_>> {
        let mut keys = BTreeMap::new();
        for action in &self.actions {
            for binding in action.tui.iter().filter(|b| b.contexts.includes(place)) {
                for key in &binding.keys {
                    keys.insert(key.clone(), TuiKey { action, binding });
                }
            }
        }
        keys
    }

    fn scopes_of(place: &str) -> &'static [&'static str] {
        SHARED_PLACES
            .iter()
            .find(|(name, _)| *name == place)
            .map_or(&[], |(_, scopes)| scopes)
    }

    /// True when the TUI waits after `key` in `place` only so a retired
    /// sequence starting with it can say where it went.
    fn held_for_a_move(&self, place: &str, key: &str) -> bool {
        let start = format!("{key} ");
        self.actions
            .iter()
            .filter(|action| !action.is_for(Client::Web))
            .flat_map(|action| &action.moved)
            .any(|moved| moved.key.starts_with(&start) && moved.contexts.iter().any(|c| c == place))
    }

    /// The pass-through that lets `id`'s TUI `key` work in `place`.
    fn passes_through(&self, place: &str, key: &str, id: &str) -> Option<usize> {
        self.tui_pass_through.iter().position(|pass| {
            pass.from.iter().any(|from| from == place)
                && self
                    .tui_keys_in(&pass.to)
                    .get(key)
                    .is_some_and(|tui| tui.action.id == id)
                && self
                    .web_keys_in(Self::scopes_of(&pass.to))
                    .get(key)
                    .is_some_and(|web| web.action.id == id)
        })
    }

    /// Every place where the web app and the TUI differ and the definition
    /// doesn't say why, as sentences.
    pub fn client_differences(&self) -> Vec<String> {
        let mut problems = Vec::new();
        let mut used_passes = BTreeSet::new();
        let mut unexplained_one_client = BTreeSet::new();
        let overlays = SHARED_OVERLAYS
            .iter()
            .map(|overlay| (*overlay, std::slice::from_ref(overlay)));
        for (place, scopes) in SHARED_PLACES.into_iter().chain(overlays) {
            let web = self.web_keys_in(scopes);
            let tui = self.tui_keys_in(place);
            let keys: BTreeSet<&String> = web.keys().chain(tui.keys()).collect();
            for key in keys {
                let (w, t) = (web.get(key), tui.get(key));
                let explained = match (w, t) {
                    (Some(w), Some(t)) if w.action.id == t.action.id => true,
                    // The TUI specialises a key the web only inherits from
                    // everywhere, for a feature only the TUI has here.
                    (Some(w), Some(t)) => {
                        !w.action.web_scopes().contains(&place) && !t.action.is_for(Client::Web)
                    }
                    (Some(w), None) => {
                        tui_cannot_deliver(key)
                            || self.held_for_a_move(place, key)
                            || !w.action.is_for(Client::Tui)
                    }
                    (None, Some(t)) => {
                        !t.action.is_for(Client::Web)
                            || self
                                .passes_through(place, key, &t.action.id)
                                .map(|index| used_passes.insert(index))
                                .is_some()
                    }
                    (None, None) => true,
                };
                for one in [w.map(|w| w.action), t.map(|t| t.action)]
                    .into_iter()
                    .flatten()
                {
                    if one.clients.len() == 1 && one.note.is_none() {
                        unexplained_one_client.insert(one.id.as_str());
                    }
                }
                if !explained {
                    let side = |id: Option<&str>| id.unwrap_or("nothing").to_string();
                    let spelled = w.map_or(key.as_str(), |w| w.chord.as_str());
                    problems.push(format!(
                        "{place} `{spelled}`: the web runs {}, the TUI {} ({}), and the \
                         definition doesn't say why",
                        side(w.map(|w| w.action.id.as_str())),
                        side(t.map(|t| t.action.id.as_str())),
                        t.map_or("unbound", |t| t.binding.name.as_str()),
                    ));
                }
            }
        }
        for id in unexplained_one_client {
            problems.push(format!(
                "{id}: is in one client only, and has no `note` saying why"
            ));
        }
        for (index, pass) in self.tui_pass_through.iter().enumerate() {
            if !used_passes.contains(&index) {
                problems.push(format!(
                    "the TUI pass-through from {} to {} explains no key; delete it",
                    pass.from.join(", "),
                    pass.to
                ));
            }
        }
        problems
    }
}
