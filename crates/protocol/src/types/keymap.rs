//! Every keyboard shortcut in every client, as the daemon serves it
//! (`Request::GetKeymap`, blueprint chapter 35). The definition lives in
//! `crates/keymap/keymap.toml`; these are its wire shapes.

use serde::{Deserialize, Serialize};

/// Returned in `ResponseData::Keymap`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct KeymapData {
    pub actions: Vec<KeyActionData>,
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
