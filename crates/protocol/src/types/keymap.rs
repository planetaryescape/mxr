//! Every keyboard shortcut in every client, as the daemon serves it
//! (`Request::GetKeymap`, blueprint chapter 35). The definition lives in
//! `crates/keymap/keymap.toml`; these are its wire shapes.

use serde::{Deserialize, Serialize};

/// Returned in `ResponseData::Keymap`: the definition with the person's
/// `keys.toml` applied.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct KeymapData {
    pub actions: Vec<KeyActionData>,
    /// False when the person turned off every shortcut made of single
    /// characters (`single_keys = false`, WCAG 2.1.4). Clients then ignore
    /// those keys outside text fields and overlays.
    #[serde(default = "single_keys_default")]
    pub single_keys: bool,
    /// Ids of the actions whose keys the person changed (`[actions]`).
    #[serde(default)]
    pub remapped: Vec<String>,
    /// What in `keys.toml` mxr is ignoring, as sentences.
    #[serde(default)]
    pub problems: Vec<String>,
}

fn single_keys_default() -> bool {
    true
}

/// A change to `keys.toml` (`Request::UpdateKeys`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum KeysChangeData {
    /// Give an action these keys, primary first, in every client; none
    /// leaves it to the palette.
    SetAction { id: String, keys: Vec<String> },
    /// Give an action back its default keys.
    ResetAction { id: String },
    /// Turn single-character shortcuts on or off.
    SingleKeys { on: bool },
}

/// Returned in `ResponseData::KeysUpdate`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct KeysUpdateData {
    /// True when `keys.toml` was written: not a dry run, and no problems.
    pub saved: bool,
    /// The `keys.toml` that was, or would be, written.
    pub file: String,
    /// Why the change can't be saved: a key another action has, a key a
    /// client can't receive. Empty when it can.
    pub problems: Vec<String>,
    /// The key map with the change applied (when it has no problems).
    pub keymap: KeymapData,
}

/// One thing a key can do, and the keys each client binds it to.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct KeyActionData {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub group: String,
    /// `tui`, `web`, or both.
    pub clients: Vec<String>,
    /// Web scopes the web keys are live in.
    #[serde(default)]
    pub scopes: Vec<String>,
    /// Web keys, primary first.
    #[serde(default)]
    pub keys: Vec<String>,
    /// Web keys bound on macOS only.
    #[serde(default)]
    pub mac_keys: Vec<String>,
    #[serde(default)]
    pub palette_only: bool,
    /// Why the clients' keys differ, when they do.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default)]
    pub tui: Vec<TuiKeysData>,
}

/// Keys the TUI binds to an action, and where.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct TuiKeysData {
    /// The TUI action its dispatcher returns.
    pub name: String,
    pub keys: Vec<String>,
    pub contexts: Vec<String>,
}
