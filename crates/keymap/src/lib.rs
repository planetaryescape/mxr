//! Every keyboard shortcut in every mxr client, from one file (blueprint
//! chapter 35, D169).
//!
//! `keymap.toml` lists each action a key can trigger, the keys and scopes
//! the web app binds it to, and how the TUI binds it. The daemon serves it
//! through `GetKeymap`, `mxr keys` prints it, and the clients are tested
//! against it: the TUI's dispatcher probe (`docs/reference/tui-keymap.json`)
//! must equal [`Keymap::tui_keymap`], and the web registry must equal the
//! generated `apps/web/src/lib/actions/keymap.generated.json`.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

/// The shipped definition.
pub const DEFAULT_KEYMAP: &str = include_str!("../keymap.toml");

/// The mail contexts the TUI's keymap probe covers. A TUI binding with
/// `contexts = "all"` is live in every one of them.
pub const TUI_CONTEXTS: [&str; 13] = [
    "list", "reader", "sidebar", "place", "screener", "todo", "catchup", "expired", "now",
    "messages", "archive", "reading", "updates",
];

/// The web app's shortcut scopes (`ActionScope` in `apps/web`).
pub const WEB_SCOPES: [&str; 16] = [
    "global",
    "sidebar",
    "list",
    "reader",
    "screener",
    "focus",
    "place",
    "todo",
    "catchup",
    "expired",
    "now",
    "messages",
    "updates",
    "archive",
    "reading",
    "reading-reader",
];

/// The shipped definition, parsed once. The file is compiled in and
/// checked by this crate's tests, so a parse failure is a build defect.
pub fn defaults() -> &'static Keymap {
    static KEYMAP: OnceLock<Keymap> = OnceLock::new();
    KEYMAP.get_or_init(|| {
        Keymap::parse(DEFAULT_KEYMAP).expect("crates/keymap/keymap.toml must parse")
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Keymap {
    #[serde(rename = "action")]
    pub actions: Vec<KeyAction>,
    /// The verbs the status bar offers for each state (D130).
    #[serde(rename = "strip", default, skip_serializing_if = "Vec::is_empty")]
    pub strips: Vec<Strip>,
}

/// The status bar's verbs for one state, in order. A client drops a member
/// that doesn't apply to the lit item, and one it has no key for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Strip {
    pub id: String,
    pub actions: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Client {
    Tui,
    Web,
}

fn both_clients() -> Vec<Client> {
    vec![Client::Tui, Client::Web]
}

fn is_both(clients: &[Client]) -> bool {
    clients == both_clients()
}

/// One thing a key can do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyAction {
    /// Stable id shared by every client, e.g. `mail.archive`.
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub short_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub group: String,
    /// Web scopes the keys are live in; empty means `global`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scopes: Vec<String>,
    /// Web keys, primary first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keys: Vec<String>,
    /// Web keys bound on macOS only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mac_keys: Vec<String>,
    /// Web keys that still work but are never shown.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub retired_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub palette_only: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hide_in_palette: bool,
    /// Why the web key differs from the TUI's, shown beside it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default = "both_clients", skip_serializing_if = "is_both")]
    pub clients: Vec<Client>,
    /// How the TUI binds this action today.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tui: Vec<TuiBinding>,
}

impl KeyAction {
    pub fn is_for(&self, client: Client) -> bool {
        self.clients.contains(&client)
    }

    /// The web scopes the action's keys are live in.
    pub fn web_scopes(&self) -> Vec<&str> {
        if self.scopes.is_empty() {
            vec!["global"]
        } else {
            self.scopes.iter().map(String::as_str).collect()
        }
    }
}

/// A TUI binding: the action name its dispatcher returns for these keys, in
/// these contexts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TuiBinding {
    pub name: String,
    pub keys: Vec<String>,
    pub contexts: Contexts,
}

/// Where a TUI binding is live: every probed context, or a list of them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RawContexts", into = "RawContexts")]
pub enum Contexts {
    All,
    Only(Vec<String>),
}

impl Contexts {
    pub fn includes(&self, context: &str) -> bool {
        match self {
            Self::All => TUI_CONTEXTS.contains(&context),
            Self::Only(list) => list.iter().any(|c| c == context),
        }
    }

    pub fn iter(&self) -> Vec<&str> {
        match self {
            Self::All => TUI_CONTEXTS.to_vec(),
            Self::Only(list) => list.iter().map(String::as_str).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum RawContexts {
    Word(String),
    List(Vec<String>),
}

impl TryFrom<RawContexts> for Contexts {
    type Error = String;

    fn try_from(raw: RawContexts) -> Result<Self, Self::Error> {
        match raw {
            RawContexts::Word(word) if word == "all" => Ok(Self::All),
            RawContexts::Word(word) => Err(format!(
                "contexts must be \"all\" or a list, not \"{word}\""
            )),
            RawContexts::List(list) => Ok(Self::Only(list)),
        }
    }
}

impl From<Contexts> for RawContexts {
    fn from(contexts: Contexts) -> Self {
        match contexts {
            Contexts::All => Self::Word("all".into()),
            Contexts::Only(list) => Self::List(list),
        }
    }
}

/// A key the web app binds, as its registry records it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct WebBinding {
    pub scope: String,
    pub chord: String,
    pub id: String,
    pub retired: bool,
    pub mac_only: bool,
}

impl Keymap {
    pub fn parse(text: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(text)
    }

    pub fn strip(&self, id: &str) -> Option<&Strip> {
        self.strips.iter().find(|strip| strip.id == id)
    }

    pub fn get(&self, id: &str) -> Option<&KeyAction> {
        self.actions.iter().find(|action| action.id == id)
    }

    /// The TUI's keys by context: context → key → TUI action name, the
    /// shape of `docs/reference/tui-keymap.json`.
    pub fn tui_keymap(&self) -> BTreeMap<String, BTreeMap<String, String>> {
        let mut map: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        for binding in self.actions.iter().flat_map(|action| &action.tui) {
            for context in binding.contexts.iter() {
                let keys = map.entry(context.to_string()).or_default();
                for key in &binding.keys {
                    keys.insert(key.clone(), binding.name.clone());
                }
            }
        }
        map
    }

    /// Every key the web app binds.
    pub fn web_bindings(&self) -> Vec<WebBinding> {
        let mut bindings = Vec::new();
        for action in self.actions.iter().filter(|a| a.is_for(Client::Web)) {
            for scope in action.web_scopes() {
                let mut push = |chord: &String, retired: bool, mac_only: bool| {
                    bindings.push(WebBinding {
                        scope: scope.to_string(),
                        chord: chord.clone(),
                        id: action.id.clone(),
                        retired,
                        mac_only,
                    });
                };
                action.keys.iter().for_each(|k| push(k, false, false));
                action.mac_keys.iter().for_each(|k| push(k, false, true));
                action
                    .retired_keys
                    .iter()
                    .for_each(|k| push(k, true, false));
            }
        }
        bindings.sort();
        bindings
    }

    /// Everything wrong with the definition, as sentences. Empty when it
    /// is sound.
    pub fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        let mut ids = BTreeSet::new();
        for action in &self.actions {
            let id = &action.id;
            if !ids.insert(id.as_str()) {
                problems.push(format!("{id}: defined twice"));
            }
            if action.label.trim().is_empty() {
                problems.push(format!("{id}: has no label"));
            }
            if action.clients.is_empty() {
                problems.push(format!("{id}: is bound in no client"));
            }
            for scope in &action.scopes {
                if !WEB_SCOPES.contains(&scope.as_str()) {
                    problems.push(format!("{id}: unknown web scope `{scope}`"));
                }
            }
            let web_keys = action
                .keys
                .iter()
                .chain(&action.mac_keys)
                .chain(&action.retired_keys);
            if !action.is_for(Client::Web) && web_keys.clone().next().is_some() {
                problems.push(format!("{id}: has web keys but is not a web action"));
            }
            if action.palette_only && web_keys.clone().next().is_some() {
                problems.push(format!("{id}: is palette-only but has keys"));
            }
            for key in web_keys.chain(action.tui.iter().flat_map(|b| &b.keys)) {
                if !is_well_formed(key) {
                    problems.push(format!("{id}: malformed key `{key}`"));
                }
            }
            if !action.is_for(Client::Tui) && !action.tui.is_empty() {
                problems.push(format!("{id}: has TUI bindings but is not a TUI action"));
            }
            if action.clients == [Client::Tui] && action.tui.is_empty() {
                problems.push(format!("{id}: is a TUI action with no TUI binding"));
            }
            for binding in &action.tui {
                if let Contexts::Only(list) = &binding.contexts {
                    for context in list {
                        if !TUI_CONTEXTS.contains(&context.as_str()) {
                            problems.push(format!("{id}: unknown TUI context `{context}`"));
                        }
                    }
                }
            }
        }
        let mut strip_ids = BTreeSet::new();
        for strip in &self.strips {
            if !strip_ids.insert(strip.id.as_str()) {
                problems.push(format!("strip {}: defined twice", strip.id));
            }
            for id in &strip.actions {
                if !ids.contains(id.as_str()) {
                    problems.push(format!("strip {}: no action `{id}`", strip.id));
                }
            }
        }
        problems.extend(clashes(
            "web scope",
            self.web_bindings()
                .into_iter()
                .map(|b| (b.scope, b.chord, b.id)),
        ));
        problems.extend(clashes(
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
        problems
    }
}

/// Two actions on one key in one place, or a key that is also the start of
/// a longer sequence there (the prefix could never complete).
fn clashes(place: &str, rows: impl Iterator<Item = (String, String, String)>) -> Vec<String> {
    let mut by_place: BTreeMap<String, BTreeMap<String, BTreeSet<String>>> = BTreeMap::new();
    for (where_, key, id) in rows {
        by_place
            .entry(where_)
            .or_default()
            .entry(key)
            .or_default()
            .insert(id);
    }
    let mut problems = Vec::new();
    for (where_, keys) in &by_place {
        for (key, ids) in keys {
            if ids.len() > 1 {
                let ids = ids.iter().cloned().collect::<Vec<_>>().join(", ");
                problems.push(format!("{place} `{where_}`: `{key}` is bound to {ids}"));
            }
            let prefix = format!("{key} ");
            if let Some((longer, _)) = keys.iter().find(|(other, _)| other.starts_with(&prefix)) {
                problems.push(format!(
                    "{place} `{where_}`: `{key}` is also the start of `{longer}`"
                ));
            }
        }
    }
    problems
}

/// A key is a chord, or chords separated by single spaces: `e`, `g i`,
/// `Mod+Shift+k`. A literal space is spelled `Space`.
fn is_well_formed(key: &str) -> bool {
    !key.is_empty()
        && key.split(' ').all(|chord| {
            !chord.is_empty() && (chord == "+" || chord.split('+').all(|part| !part.is_empty()))
        })
}

#[cfg(test)]
mod tests;
