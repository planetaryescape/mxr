//! Every keyboard shortcut in every mxr client, from one file (blueprint
//! chapter 35, D169).
//!
//! `keymap.toml` lists each action a key can trigger, the keys and scopes
//! the web app binds it to, and how the TUI binds it. The daemon serves it
//! through `GetKeymap`, `mxr keys` prints it, and the clients are tested
//! against it: the TUI's dispatcher probe (`docs/reference/tui-keymap.json`)
//! must equal [`Keymap::tui_keymap`], and the web registry must equal the
//! generated `apps/web/src/lib/actions/keymap.generated.json`.

mod docs;
mod keys_file;
mod parity;
mod places;
mod remaps;

pub use keys_file::{
    action_keys_problems, edit_keys_toml, is_character_key, new_clashes, tui_key, KeysEdit,
    KeysFile, Remapped, ACTIONS_SECTION, SINGLE_KEYS_SETTING, TUI_REMAP_CONTEXTS,
};
pub use parity::{tui_cannot_deliver, TuiPassThrough};
pub use places::place_title;
pub use remaps::{ModeRemaps, MODE_SECTIONS};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

/// The shipped definition.
pub const DEFAULT_KEYMAP: &str = include_str!("../keymap.toml");

/// The mail contexts the TUI's keymap probe covers. A TUI binding with
/// `contexts = "all"` is live in every one of them.
pub const TUI_CONTEXTS: [&str; 14] = [
    "list",
    "reader",
    "sidebar",
    "place",
    "screener",
    "todo",
    "catchup",
    "expired",
    "now",
    "messages",
    "archive",
    "reading",
    "reading-reader",
    "updates",
];

/// TUI screens and overlays, and web rows, that read keys outside the
/// mail-context probe. A moved key there is announced by name from the
/// handler (`App::announce_moved`). The tool screens, Search and the
/// assistant pane list their keys here too, for help and the docs; a TUI
/// runner test holds them to what each handler does.
pub const SCREEN_CONTEXTS: [&str; 25] = [
    "accounts",
    "assistant",
    "invites",
    "not-sure",
    "rules",
    "saved-searches",
    "todo-microsoft",
    "compose-assist-proof",
    "accounts-form",
    "rules-form",
    "diagnostics",
    "analytics",
    "analytics-storage",
    "analytics-stale",
    "analytics-contacts",
    "analytics-response-time",
    "analytics-subscriptions",
    "analytics-wrapped",
    "deliveries",
    "search",
    "search-preview",
    "search-editor",
    "assistant-input",
    "assistant-approval",
    "assistant-question",
];

/// Overlays: pickers, confirms, previews, prompts and sheets (blueprint 35,
/// "Overlays share one grammar"). An open overlay owns every key, so its
/// keys are read by the overlay itself in both clients, never by a
/// dispatcher. The names are shared: a TUI binding names the overlay it is
/// live in, and a web action's `scopes` name the dialog that reads it. A
/// TUI runner test holds each TUI overlay's handler to its entries.
pub const OVERLAY_CONTEXTS: [&str; 54] = [
    // Both clients.
    "help",
    "palette",
    "label-picker",
    "links",
    "unsubscribe",
    "move-picker",
    "move-picker-sender",
    "new-sender-picker",
    "sender-kind-picker",
    "not-now",
    "correct-todo",
    "delete-confirm",
    "bulk-confirm",
    "sweep-confirm",
    "reply-later",
    "sender-ask",
    "categories",
    // The TUI's.
    "not-now-custom",
    "send-review",
    "send-time",
    "error",
    "sheet",
    "platform",
    "welcome",
    "snippets",
    "sender-profile",
    "reply-queue",
    "drafts",
    "compose-picker",
    "model-picker",
    "voice-source",
    "attachments",
    "save-attachment",
    "export",
    "export-destination",
    "export-path",
    "filter-bar",
    "analytics-filter",
    "saved-search-form",
    "messages-filter",
    "draft-options",
    "records-prompt",
    "pass-menu",
    "todo-prompt",
    "arrivals-list",
    "resume-draft",
    "account-mode-switch",
    "identity-editor",
    "auth-session",
    "promise-prompt",
    // The web's.
    "send-confirm",
    "let-go-preview",
    "compose-discard",
    "search-palette",
];

/// The web app's shortcut scopes (`ActionScope` in `apps/web`).
pub const WEB_SCOPES: [&str; 19] = [
    "global",
    "sidebar",
    "list",
    "reader",
    "screener",
    "focus",
    "todo",
    "catchup",
    "expired",
    "now",
    "messages",
    "messages-filters",
    "page",
    "updates",
    "archive",
    "reading",
    "reading-reader",
    "compose",
    "arrivals",
];

/// The key a mode guide shows for an action: the first web key the TUI also
/// binds, so the line is true in both clients; failing that, the TUI's or the
/// web's own key. `None` for an action with no key.
pub fn guide_key(id: &str) -> Option<String> {
    let action = defaults().get(id)?;
    let tui: Vec<&String> = action
        .tui
        .iter()
        .flat_map(|binding| &binding.keys)
        .collect();
    action
        .keys
        .iter()
        .find(|key| tui.contains(key))
        .or_else(|| tui.first().copied())
        .or_else(|| action.keys.first())
        .cloned()
}

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
    /// Where a TUI key reaches the pane beside the focused one, which the
    /// web's focus-bound keys don't (`Keymap::client_differences`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tui_pass_through: Vec<TuiPassThrough>,
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
    /// Keys this action had until a recent release. For a short window
    /// pressing one shows where the action went and runs nothing (D171).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub moved: Vec<MovedKey>,
    /// The Raycast key when it can't be Cmd plus the web letter: Raycast's
    /// own shortcut for the same action, or `""` for none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raycast_key: Option<String>,
    /// An overlay key the web gets from the browser or the dialog
    /// primitive (Esc closes a dialog, Enter presses the focused button,
    /// Tab moves between buttons), so no web code reads it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub web_native: bool,
}

impl KeyAction {
    pub fn is_for(&self, client: Client) -> bool {
        self.clients.contains(&client)
    }

    /// The key Raycast and other modifier-only clients use: Cmd plus the
    /// action's first single-letter web key, keeping its Shift, unless
    /// `raycast_key` says otherwise. Cmd+K is Raycast's action panel, so a
    /// derived key never takes it.
    pub fn raycast_key(&self) -> Option<String> {
        if let Some(key) = &self.raycast_key {
            return (!key.is_empty()).then(|| key.clone());
        }
        let letter = self
            .keys
            .iter()
            .filter_map(|key| {
                let mut chars = key.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) if c.is_ascii_alphabetic() => Some(c),
                    _ => None,
                }
            })
            .next()?;
        let key = if letter.is_ascii_uppercase() {
            format!("Cmd+Shift+{}", letter.to_ascii_lowercase())
        } else {
            format!("Cmd+{letter}")
        };
        (key != "Cmd+k").then_some(key)
    }

    /// True for an overlay's key: every place it is live in, in either
    /// client, is an overlay. The overlay reads it, so it is in neither
    /// client's dispatcher, palette or remaps.
    pub fn is_overlay(&self) -> bool {
        let web = if self.is_for(Client::Web) {
            self.web_scopes()
        } else {
            Vec::new()
        };
        let tui = self.tui.iter().flat_map(|binding| binding.contexts.iter());
        let mut places = web.into_iter().chain(tui).peekable();
        places.peek().is_some() && places.all(|place| OVERLAY_CONTEXTS.contains(&place))
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

/// An action's Raycast key, as `@raycast/api` takes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RaycastShortcut {
    pub modifiers: Vec<String>,
    pub key: String,
}

impl RaycastShortcut {
    /// `Cmd+Shift+e` → `{ modifiers: ["cmd", "shift"], key: "e" }`.
    pub fn parse(spelling: &str) -> Option<Self> {
        let mut parts: Vec<&str> = spelling.split('+').collect();
        let key = parts.pop()?.to_string();
        let modifiers = parts
            .into_iter()
            .map(|part| match part {
                "Cmd" => Some("cmd"),
                "Shift" => Some("shift"),
                "Ctrl" => Some("ctrl"),
                "Opt" => Some("opt"),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()?
            .into_iter()
            .map(String::from)
            .collect();
        (!key.is_empty()).then_some(Self { modifiers, key })
    }
}

/// A key an action no longer has, kept for a short window so pressing it
/// says where the action went instead of doing nothing or something else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MovedKey {
    pub key: String,
    /// Web scopes and TUI contexts (the names are shared) where it moved.
    pub contexts: Vec<String>,
    /// The first release that no longer shows the notice; the entry is
    /// deleted then (`problems` fails once mxr reaches this version).
    pub until: String,
    /// The action whose key the notice names, when the old action has no
    /// key of its own left (digit `1` points at `g i`). The notice keeps
    /// the old action's label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    /// The key the notice names when the new key belongs to a screen or
    /// overlay that still reads its own keys (Rules, the drafts list), so
    /// no action here carries it yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_key: Option<String>,
}

/// A TUI binding: the action name its dispatcher returns for these keys, in
/// these contexts. A palette-only action's binding has no keys: it names the
/// TUI action the palette runs, and the contexts it is listed in.
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

    /// The TUI binding a key or sequence (`e`, `g i`) triggers in `context`.
    pub fn tui_binding(&self, context: &str, keys: &str) -> Option<(&KeyAction, &TuiBinding)> {
        self.actions.iter().find_map(|action| {
            action
                .tui
                .iter()
                .find(|binding| {
                    binding.contexts.includes(context) && binding.keys.iter().any(|k| k == keys)
                })
                .map(|binding| (action, binding))
        })
    }

    /// True when `prefix` starts a longer TUI sequence in `context`, so the
    /// TUI waits for the next key.
    pub fn tui_starts_sequence(&self, context: &str, prefix: &str) -> bool {
        let start = format!("{prefix} ");
        self.actions
            .iter()
            .flat_map(|action| &action.tui)
            .any(|binding| {
                binding.contexts.includes(context)
                    && binding.keys.iter().any(|k| k.starts_with(&start))
            })
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

    /// The action whose old key `key` was in `context`, while its move is
    /// still being announced.
    pub fn moved_from(&self, context: &str, key: &str) -> Option<&KeyAction> {
        self.moved_entry(context, key).map(|(action, _)| action)
    }

    /// The action whose old key `key` was in `context`, with its move.
    pub fn moved_entry(&self, context: &str, key: &str) -> Option<(&KeyAction, &MovedKey)> {
        self.actions.iter().find_map(|action| {
            action
                .moved
                .iter()
                .find(|moved| moved.key == key && moved.contexts.iter().any(|c| c == context))
                .map(|moved| (action, moved))
        })
    }

    /// The action a moved key's notice names the key of: its `to`, or
    /// the action itself.
    pub fn moved_target<'a>(&'a self, action: &'a KeyAction, moved: &MovedKey) -> &'a KeyAction {
        moved
            .to
            .as_deref()
            .and_then(|id| self.get(id))
            .unwrap_or(action)
    }

    /// True when `prefix` starts a moved sequence in `context`, so the TUI
    /// still waits for the second key and can say where `z z` went.
    pub fn moved_starts_sequence(&self, context: &str, prefix: &str) -> bool {
        let start = format!("{prefix} ");
        self.actions
            .iter()
            .flat_map(|action| &action.moved)
            .any(|moved| {
                moved.key.starts_with(&start) && moved.contexts.iter().any(|c| c == context)
            })
    }

    /// Every web action's Raycast key, by action id.
    pub fn raycast_keys(&self) -> BTreeMap<String, RaycastShortcut> {
        self.actions
            .iter()
            .filter(|action| action.is_for(Client::Web) && !action.is_overlay())
            .filter_map(|action| {
                let shortcut = RaycastShortcut::parse(&action.raycast_key()?)?;
                Some((action.id.clone(), shortcut))
            })
            .collect()
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
            let overlay = action.is_overlay();
            for scope in &action.scopes {
                if OVERLAY_CONTEXTS.contains(&scope.as_str()) {
                    if !overlay {
                        problems.push(format!(
                            "{id}: `{scope}` is an overlay, which reads its own keys; an action is live in overlays only or in none"
                        ));
                    }
                } else if !WEB_SCOPES.contains(&scope.as_str()) {
                    problems.push(format!("{id}: unknown web scope `{scope}`"));
                }
            }
            if action.web_native && !(overlay && action.is_for(Client::Web)) {
                problems.push(format!(
                    "{id}: only an overlay's web keys can be the browser's own (`web_native`)"
                ));
            }
            let web_keys = action
                .keys
                .iter()
                .chain(&action.mac_keys)
                .chain(&action.retired_keys);
            if !action.is_for(Client::Web) && web_keys.clone().next().is_some() {
                problems.push(format!("{id}: has web keys but is not a web action"));
            }
            let tui_keys = action.tui.iter().any(|binding| !binding.keys.is_empty());
            if action.palette_only && (web_keys.clone().next().is_some() || tui_keys) {
                problems.push(format!("{id}: is palette-only but has keys"));
            }
            if !action.palette_only && action.tui.iter().any(|binding| binding.keys.is_empty()) {
                problems.push(format!(
                    "{id}: a TUI binding with no keys is for a palette-only action"
                ));
            }
            for key in web_keys.chain(action.tui.iter().flat_map(|b| &b.keys)) {
                if !is_well_formed(key) {
                    problems.push(format!("{id}: malformed key `{key}`"));
                }
            }
            if let Some(key) = action.raycast_key.as_deref().filter(|key| !key.is_empty()) {
                if RaycastShortcut::parse(key).is_none() {
                    problems.push(format!(
                        "{id}: raycast_key `{key}` is not Cmd/Shift/Ctrl/Opt plus a key"
                    ));
                }
            }
            if !action.is_for(Client::Tui) && !action.tui.is_empty() {
                problems.push(format!("{id}: has TUI bindings but is not a TUI action"));
            }
            // A TUI action whose key retired keeps its entry while the move
            // is announced (`z z`), with no binding left.
            if action.clients == [Client::Tui] && action.tui.is_empty() && action.moved.is_empty() {
                problems.push(format!("{id}: is a TUI action with no TUI binding"));
            }
            for binding in &action.tui {
                if let Contexts::Only(list) = &binding.contexts {
                    for context in list {
                        if !TUI_CONTEXTS.contains(&context.as_str())
                            && !SCREEN_CONTEXTS.contains(&context.as_str())
                            && !OVERLAY_CONTEXTS.contains(&context.as_str())
                        {
                            problems.push(format!("{id}: unknown TUI context `{context}`"));
                        }
                    }
                }
            }
        }
        for action in &self.actions {
            for moved in &action.moved {
                if let Some(to) = moved.to.as_deref() {
                    if !ids.contains(to) {
                        problems.push(format!(
                            "{}: the move of `{}` points to no action `{to}`",
                            action.id, moved.key
                        ));
                    }
                }
                if !older_than(env!("CARGO_PKG_VERSION"), &moved.until) {
                    problems.push(format!(
                        "{}: the move of `{}` was announced until {}; delete the entry",
                        action.id, moved.key, moved.until
                    ));
                }
                if let Some(key) = moved.to_key.as_deref().filter(|key| !is_well_formed(key)) {
                    problems.push(format!(
                        "{}: the move of `{}` names malformed key `{key}`",
                        action.id, moved.key
                    ));
                }
                // A misspelt place would never announce the move.
                for context in &moved.contexts {
                    let known = TUI_CONTEXTS.contains(&context.as_str())
                        || WEB_SCOPES.contains(&context.as_str())
                        || SCREEN_CONTEXTS.contains(&context.as_str())
                        || OVERLAY_CONTEXTS.contains(&context.as_str());
                    if !known {
                        problems.push(format!(
                            "{}: the move of `{}` names unknown context `{context}`",
                            action.id, moved.key
                        ));
                    }
                }
                // Only the clients the action is for: the TUI reader's `o`
                // moved while the web reader keeps `o` for its own verb.
                for context in &moved.contexts {
                    let web_bound = action.is_for(Client::Web)
                        && self.web_bindings().iter().any(|b| {
                            b.chord == moved.key && (b.scope == *context || b.scope == "global")
                        });
                    let tui_bound = action.is_for(Client::Tui)
                        && self.tui_binding(context, &moved.key).is_some();
                    if web_bound || tui_bound {
                        problems.push(format!(
                            "{}: `{}` moved away in {context} but is still bound there",
                            action.id, moved.key
                        ));
                    }
                }
            }
        }
        let mut moved_places = BTreeSet::new();
        for action in &self.actions {
            for moved in &action.moved {
                for context in &moved.contexts {
                    if !moved_places.insert((context.as_str(), moved.key.as_str())) {
                        problems.push(format!(
                            "{}: `{}` in {context} is announced as moved by two actions",
                            action.id, moved.key
                        ));
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

/// True when version `a` comes before `b` (`0.6.149` < `0.6.152`).
fn older_than(a: &str, b: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> { v.split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    parts(a) < parts(b)
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
