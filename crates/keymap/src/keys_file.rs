//! The parts of a person's `keys.toml` that every client reads (blueprint
//! chapter 35, D170): `single_keys`, the switch that turns off every
//! shortcut made of a single character (WCAG 2.1.4), and `[actions]`, which
//! gives an action its own keys wherever it has keys. The daemon applies
//! them to the definition for `GetKeymap`; the TUI folds `[actions]` into its
//! pane tables and mode remaps at start. The mode and pane sections stay the
//! TUI's own.

use crate::{clashes, is_well_formed, Contexts, Keymap, TuiBinding};
use std::collections::BTreeMap;

/// The `keys.toml` section that gives actions their own keys.
pub const ACTIONS_SECTION: &str = "actions";
/// The `keys.toml` setting that turns single-character shortcuts off.
pub const SINGLE_KEYS_SETTING: &str = "single_keys";

/// TUI contexts where `[actions]` changes keys: the mail list and reader,
/// whose keys come from the `[mail_list]`, `[message_view]` and
/// `[thread_view]` tables, and the modes. The sidebar, the screener and
/// overlays read their own keys and keep the defaults.
pub const TUI_REMAP_CONTEXTS: [&str; 11] = [
    "list", "reader", "place", "now", "messages", "todo", "catchup", "expired", "updates",
    "reading", "archive",
];

/// Chords the browser keeps for itself, so a web binding on one silently
/// does nothing for some people. Mirrors `apps/web/src/lib/keys/browserReserved.ts`.
const BROWSER_RESERVED: [(&str, &str); 26] = [
    ("Mod+w", "close tab"),
    ("Mod+Shift+w", "close window"),
    ("Mod+t", "new tab"),
    ("Mod+Shift+t", "reopen closed tab"),
    ("Mod+n", "new window"),
    ("Mod+Shift+n", "new private window"),
    ("Mod+q", "quit the browser"),
    ("Mod+l", "focus the address bar"),
    ("Mod+Tab", "switch tabs"),
    ("Mod+1", "jump to tab 1"),
    ("Mod+2", "jump to tab 2"),
    ("Mod+3", "jump to tab 3"),
    ("Mod+4", "jump to tab 4"),
    ("Mod+5", "jump to tab 5"),
    ("Mod+6", "jump to tab 6"),
    ("Mod+7", "jump to tab 7"),
    ("Mod+8", "jump to tab 8"),
    ("Mod+9", "jump to the last tab"),
    ("Mod+r", "reload"),
    ("Mod+Shift+r", "reload, bypassing the cache"),
    ("Mod+f", "find in page"),
    ("Mod+p", "print"),
    ("Mod+s", "save page"),
    ("Mod+o", "open file"),
    ("F5", "reload"),
    ("F12", "developer tools"),
];

/// Key names the TUI can receive without a modifier.
const NAMED_KEYS: [&str; 15] = [
    "Enter",
    "Escape",
    "Tab",
    "Backspace",
    "Delete",
    "ArrowUp",
    "ArrowDown",
    "ArrowLeft",
    "ArrowRight",
    "Home",
    "End",
    "PageUp",
    "PageDown",
    "Space",
    "+",
];

/// The definition with a person's `[actions]` applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remapped {
    pub keymap: Keymap,
    /// The entries that applied, by action id.
    pub applied: BTreeMap<String, Vec<String>>,
    /// Why the others didn't.
    pub problems: Vec<String>,
}

/// `single_keys` and `[actions]` from a person's `keys.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeysFile {
    /// False turns off every shortcut made of single characters.
    pub single_keys: bool,
    /// Action id → the person's keys for it, primary first. Empty means the
    /// action has no key.
    pub actions: BTreeMap<String, Vec<String>>,
}

impl Default for KeysFile {
    fn default() -> Self {
        Self {
            single_keys: true,
            actions: BTreeMap::new(),
        }
    }
}

impl KeysFile {
    /// Read `keys.toml` text. Malformed TOML gives the defaults and says so.
    pub fn parse(text: &str, keymap: &Keymap) -> (Self, Vec<String>) {
        match text.parse::<toml::Table>() {
            Ok(table) => Self::from_table(&table, keymap),
            Err(error) => (
                Self::default(),
                vec![format!("keys.toml is not valid TOML: {error}")],
            ),
        }
    }

    /// The switch and `[actions]` of a parsed `keys.toml`, and what in them
    /// can't be used. Unusable entries are left out.
    pub fn from_table(table: &toml::Table, keymap: &Keymap) -> (Self, Vec<String>) {
        let mut file = Self::default();
        let mut problems = Vec::new();
        match table.get(SINGLE_KEYS_SETTING) {
            None => {}
            Some(toml::Value::Boolean(on)) => file.single_keys = *on,
            Some(_) => problems.push(format!("`{SINGLE_KEYS_SETTING}` must be true or false")),
        }
        let Some(section) = table.get(ACTIONS_SECTION) else {
            return (file, problems);
        };
        let Some(entries) = section.as_table() else {
            problems.push(format!(
                "[{ACTIONS_SECTION}] must be a table of \"action id\" = \"key\" or [\"key\", …]"
            ));
            return (file, problems);
        };
        for (id, value) in entries {
            let keys: Vec<String> = match value {
                toml::Value::String(key) if key.is_empty() => Vec::new(),
                toml::Value::String(key) => vec![key.clone()],
                toml::Value::Array(items) => {
                    let keys: Option<Vec<String>> = items
                        .iter()
                        .map(|item| item.as_str().map(String::from))
                        .collect();
                    let Some(keys) = keys else {
                        problems.push(format!(
                            "[{ACTIONS_SECTION}] {id}: every key must be a string"
                        ));
                        continue;
                    };
                    keys
                }
                _ => {
                    problems.push(format!(
                        "[{ACTIONS_SECTION}] {id}: give a key, a list of keys, or [] for none"
                    ));
                    continue;
                }
            };
            match action_keys_problems(keymap, id, &keys).as_slice() {
                [] => {
                    file.actions.insert(id.clone(), keys);
                }
                found => problems.extend(
                    found
                        .iter()
                        .map(|problem| format!("[{ACTIONS_SECTION}] {problem}")),
                ),
            }
        }
        (file, problems)
    }
}

/// Everything wrong with giving action `id` these keys, before any clash
/// with another action is considered. Each sentence starts with the id.
pub fn action_keys_problems(keymap: &Keymap, id: &str, keys: &[String]) -> Vec<String> {
    let Some(action) = keymap.get(id) else {
        return vec![format!("{id}: no such action; `mxr keys` lists them")];
    };
    if action.palette_only {
        return vec![format!(
            "{id}: {} runs from the palette and has no key",
            action.label
        )];
    }
    // Tool screens and overlays still read their own keys, so a TUI-only
    // action that lives only there has nothing a remap could change.
    let remaps_in_tui = action.tui.iter().any(|binding| {
        !binding.keys.is_empty()
            && binding
                .contexts
                .iter()
                .iter()
                .any(|context| TUI_REMAP_CONTEXTS.contains(context))
    });
    if !action.is_for(crate::Client::Web) && !remaps_in_tui {
        return vec![format!(
            "{id}: {} is a key on a TUI screen or overlay, which keeps its own keys; keys.toml changes keys in the web app, the mail list, the reader and the modes",
            action.label
        )];
    }
    let mut problems = Vec::new();
    for (index, key) in keys.iter().enumerate() {
        if keys[..index].contains(key) {
            problems.push(format!("{id}: `{key}` is listed twice"));
            continue;
        }
        if !is_well_formed(key) {
            problems.push(format!(
                "{id}: `{key}` is not a key; spell it as `mxr keys` prints keys, e.g. `x`, `g x`, `Mod+Shift+k`"
            ));
            continue;
        }
        if action.is_for(crate::Client::Web) {
            if let Some((_, what)) = BROWSER_RESERVED.iter().find(|(chord, _)| chord == key) {
                problems.push(format!(
                    "{id}: the browser keeps `{key}` ({what}), so the web app never sees it"
                ));
            }
        }
        if remaps_in_tui {
            if let Some(why) = tui_key_problem(&tui_key(key)) {
                problems.push(format!("{id}: `{key}`: {why}"));
            }
        }
    }
    problems
}

/// Why the TUI can't take `key` (in its own spelling), if it can't. Its
/// dispatcher reads a character, `Ctrl` plus a character, a named key, or a
/// character followed by one more key.
fn tui_key_problem(key: &str) -> Option<&'static str> {
    let chords: Vec<&str> = key.split(' ').collect();
    if chords.len() > 2 {
        return Some("the TUI's sequences are two keys long");
    }
    if chords.len() == 2 && chords[0].chars().count() != 1 {
        return Some("a TUI sequence starts with a single character");
    }
    for chord in chords {
        if chord.chars().count() == 1 || NAMED_KEYS.contains(&chord) {
            continue;
        }
        let Some(rest) = chord.strip_prefix("Ctrl+") else {
            return Some("the TUI reads a character, Ctrl plus a character, or a named key");
        };
        if rest.starts_with("Shift+") {
            return Some("most terminals send Ctrl+Shift+ as plain Ctrl+");
        }
        if ["i", "m", "h", "["].contains(&rest) {
            return Some("terminals send this as Tab, Enter, Backspace or Escape");
        }
        if rest.chars().count() != 1 {
            return Some("the TUI reads a character, Ctrl plus a character, or a named key");
        }
    }
    None
}

/// A key as the TUI spells it: terminals have no Cmd, so `Mod` is `Ctrl`.
pub fn tui_key(key: &str) -> String {
    key.replace("Mod+", "Ctrl+")
}

/// True for a shortcut the single-key switch turns off: one that starts
/// with a character typed on its own (`e`, `E`, `#`, `g i`), without
/// `Mod`, `Ctrl`, `Alt` or `Meta`. Named keys (`Enter`, `Escape`, `Space`,
/// the arrows) and modifier chords stay on.
pub fn is_character_key(key: &str) -> bool {
    let first = key.split(' ').next().unwrap_or_default();
    first.chars().count() == 1 && first != " "
}

impl Keymap {
    /// The definition with the person's `[actions]` keys applied. Entries
    /// apply in id order; one that would share a key with another action,
    /// or block another's sequence, is left out and named, so the result
    /// never has a clash a client would reject.
    pub fn remapped(&self, actions: &BTreeMap<String, Vec<String>>) -> Remapped {
        let mut remapped = Remapped {
            keymap: self.clone(),
            applied: BTreeMap::new(),
            problems: Vec::new(),
        };
        for (id, keys) in actions {
            let found = action_keys_problems(&remapped.keymap, id, keys);
            if !found.is_empty() {
                remapped.problems.extend(found);
                continue;
            }
            let candidate = remapped.keymap.with_action_keys(id, keys);
            let clashes = new_clashes(&remapped.keymap, &candidate);
            if clashes.is_empty() {
                remapped.keymap = candidate;
                remapped.applied.insert(id.clone(), keys.clone());
            } else {
                remapped
                    .problems
                    .extend(clashes.into_iter().map(|clash| format!("{id}: {clash}")));
            }
        }
        remapped
    }

    /// This definition with `id`'s keys replaced by `keys` in every web
    /// scope and every TUI context in [`TUI_REMAP_CONTEXTS`].
    pub fn with_action_keys(&self, id: &str, keys: &[String]) -> Self {
        let mut keymap = self.clone();
        let Some(action) = keymap.actions.iter_mut().find(|action| action.id == id) else {
            return keymap;
        };
        if action.is_for(crate::Client::Web) {
            action.keys = keys.to_vec();
            action.mac_keys.clear();
            action.retired_keys.clear();
        }
        let tui_keys: Vec<String> = keys.iter().map(|key| tui_key(key)).collect();
        let mut bindings = Vec::new();
        for binding in std::mem::take(&mut action.tui) {
            if binding.keys.is_empty() {
                bindings.push(binding);
                continue;
            }
            let (remapped, kept): (Vec<&str>, Vec<&str>) = binding
                .contexts
                .iter()
                .into_iter()
                .partition(|context| TUI_REMAP_CONTEXTS.contains(context));
            if !remapped.is_empty() && !tui_keys.is_empty() {
                bindings.push(TuiBinding {
                    name: binding.name.clone(),
                    keys: tui_keys.clone(),
                    contexts: Contexts::Only(remapped.iter().map(ToString::to_string).collect()),
                });
            }
            if !kept.is_empty() {
                bindings.push(TuiBinding {
                    name: binding.name.clone(),
                    keys: binding.keys.clone(),
                    contexts: Contexts::Only(kept.iter().map(ToString::to_string).collect()),
                });
            }
        }
        action.tui = bindings;
        keymap.drop_shadowed_moves();
        keymap
    }

    /// A moved key's notice where a remap now binds that key, or a key it
    /// starts or extends, would hide the binding, so it goes.
    fn drop_shadowed_moves(&mut self) {
        let web = self.web_bindings();
        let tui = self.tui_keymap();
        let shadows = |bound: &str, moved: &str| {
            bound == moved
                || bound.starts_with(&format!("{moved} "))
                || moved.starts_with(&format!("{bound} "))
        };
        for action in &mut self.actions {
            for moved in &mut action.moved {
                moved.contexts.retain(|context| {
                    let web_bound = web.iter().any(|binding| {
                        (binding.scope == *context || binding.scope == "global")
                            && shadows(&binding.chord, &moved.key)
                    });
                    let tui_bound = tui
                        .get(context)
                        .is_some_and(|keys| keys.keys().any(|key| shadows(key, &moved.key)));
                    !web_bound && !tui_bound
                });
            }
            action.moved.retain(|moved| !moved.contexts.is_empty());
        }
    }

    /// Every key clash in this definition, web and TUI.
    fn clash_list(&self) -> Vec<String> {
        let mut found = clashes(
            "web scope",
            self.web_bindings()
                .into_iter()
                .map(|binding| (binding.scope, binding.chord, binding.id)),
        );
        found.extend(clashes(
            "TUI context",
            self.actions.iter().flat_map(|action| {
                action.tui.iter().flat_map(move |binding| {
                    binding
                        .contexts
                        .iter()
                        .into_iter()
                        .flat_map(move |context| {
                            binding.keys.iter().map(move |key| {
                                (context.to_string(), key.clone(), action.id.clone())
                            })
                        })
                })
            }),
        ));
        found
    }
}

/// The clashes `after` has that `before` didn't.
pub fn new_clashes(before: &Keymap, after: &Keymap) -> Vec<String> {
    let old = before.clash_list();
    after
        .clash_list()
        .into_iter()
        .filter(|clash| !old.contains(clash))
        .collect()
}

/// A change to `keys.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeysEdit {
    /// Give an action these keys (none when empty) in every client.
    SetAction { id: String, keys: Vec<String> },
    /// Give an action back its default keys.
    ResetAction { id: String },
    /// Turn single-character shortcuts on or off.
    SingleKeys(bool),
}

/// `text` with `edit` made, keeping everything else in the file as written,
/// comments included.
pub fn edit_keys_toml(text: &str, edit: &KeysEdit) -> Result<String, String> {
    let mut doc: toml_edit::DocumentMut = text
        .parse()
        .map_err(|error| format!("keys.toml is not valid TOML: {error}"))?;
    match edit {
        KeysEdit::SingleKeys(true) => {
            // On is the default, so the setting goes rather than staying as noise.
            doc.remove(SINGLE_KEYS_SETTING);
        }
        KeysEdit::SingleKeys(false) => {
            doc[SINGLE_KEYS_SETTING] = toml_edit::value(false);
        }
        KeysEdit::ResetAction { id } => {
            if let Some(table) = doc
                .get_mut(ACTIONS_SECTION)
                .and_then(toml_edit::Item::as_table_like_mut)
            {
                table.remove(id);
                if table.is_empty() {
                    doc.remove(ACTIONS_SECTION);
                }
            }
        }
        KeysEdit::SetAction { id, keys } => {
            if doc.get(ACTIONS_SECTION).is_none() {
                doc[ACTIONS_SECTION] = toml_edit::table();
            }
            let table = doc[ACTIONS_SECTION]
                .as_table_like_mut()
                .ok_or_else(|| format!("[{ACTIONS_SECTION}] in keys.toml is not a table"))?;
            let value = match keys.as_slice() {
                [key] => toml_edit::Value::from(key.as_str()),
                keys => toml_edit::Value::Array(keys.iter().map(String::as_str).collect()),
            };
            table.insert(id, toml_edit::Item::Value(value));
        }
    }
    Ok(doc.to_string())
}
