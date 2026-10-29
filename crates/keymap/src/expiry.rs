//! Moved-key notices expire (blueprint 35, D171 as amended 2026-10-29).
//!
//! A `[[action.moved]]` entry announces its old key until mxr reaches the
//! entry's `until`. From that version on the entry is ignored: [`defaults`]
//! leaves it out, so the old key does nothing and shows nothing in either
//! client, and a release never waits on deleting it. A scheduled job deletes
//! ignored entries from `keymap.toml` now and then
//! (`scripts/prune_expired_key_moves`).
//!
//! [`defaults`]: crate::defaults

use crate::{Client, KeyAction, Keymap, MovedKey};

/// This build's version, the one [`crate::defaults`] expires moves at.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

impl Keymap {
    /// This definition as a build at `version` uses it: without the moves
    /// whose `until` that version has reached, and without a TUI action
    /// left with no binding once they go, which only carried them.
    pub fn live_at(&self, version: &str) -> Self {
        let mut keymap = self.clone();
        keymap.actions.retain_mut(|action| {
            let carried_moves = !action.moved.is_empty();
            action.moved.retain(|moved| is_live(moved, version));
            !(carried_moves && is_empty_tui_action(action))
        });
        keymap
    }

    /// The moves a build at `version` ignores, one sentence each, for CI's
    /// warning. Empty once the scheduled prune has deleted them.
    pub fn expired_moves(&self, version: &str) -> Vec<String> {
        self.actions
            .iter()
            .flat_map(|action| {
                action
                    .moved
                    .iter()
                    .filter(|moved| !is_live(moved, version))
                    .map(move |moved| {
                        format!(
                            "{}: the move of `{}` in {} expired at {}",
                            action.id,
                            moved.key,
                            moved.contexts.join(", "),
                            moved.until
                        )
                    })
            })
            .collect()
    }
}

/// True while a build at `version` still announces `moved`.
fn is_live(moved: &MovedKey, version: &str) -> bool {
    older_than(version, &moved.until)
}

fn is_empty_tui_action(action: &KeyAction) -> bool {
    action.clients == [Client::Tui] && action.tui.is_empty() && action.moved.is_empty()
}

/// `text`, a `keymap.toml`, without what a build at `version` ignores: the
/// `[[action.moved]]` entries it has reached the `until` of, and a TUI
/// action they leave with no binding. Everything else, comments and layout
/// included, is kept, so `text.parse()` of the result equals
/// `Keymap::parse(text)?.live_at(version)`.
pub fn prune_expired_moves(text: &str, version: &str) -> Result<String, toml_edit::TomlError> {
    let mut doc: toml_edit::DocumentMut = text.parse()?;
    let Some(actions) = doc
        .get_mut("action")
        .and_then(toml_edit::Item::as_array_of_tables_mut)
    else {
        return Ok(doc.to_string());
    };
    let mut emptied = Vec::new();
    for action in actions.iter_mut() {
        let Some(moved) = action
            .get_mut("moved")
            .and_then(toml_edit::Item::as_array_of_tables_mut)
        else {
            emptied.push(false);
            continue;
        };
        // An entry without a readable `until` doesn't parse as a keymap at
        // all, so it stays for the definition check to report.
        moved.retain(|entry| {
            entry
                .get("until")
                .and_then(toml_edit::Item::as_str)
                .is_none_or(|until| older_than(version, until))
        });
        let none_left = moved.is_empty();
        if none_left {
            action.remove("moved");
        }
        emptied.push(none_left && is_empty_tui_table(action));
    }
    let mut emptied = emptied.into_iter();
    actions.retain(|_| !emptied.next().unwrap_or(false));
    Ok(doc.to_string())
}

/// [`is_empty_tui_action`] for an `[[action]]` table.
fn is_empty_tui_table(action: &toml_edit::Table) -> bool {
    let tui_only = action
        .get("clients")
        .and_then(toml_edit::Item::as_array)
        .is_some_and(|clients| {
            clients.len() == 1 && clients.get(0).and_then(|c| c.as_str()) == Some("tui")
        });
    let bindings = action
        .get("tui")
        .and_then(toml_edit::Item::as_array_of_tables)
        .is_some_and(|tui| !tui.is_empty());
    tui_only && !bindings
}

/// True when version `a` comes before `b` (`0.6.149` < `0.6.152`).
pub(crate) fn older_than(a: &str, b: &str) -> bool {
    let parts = |v: &str| -> Vec<u64> { v.split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    parts(a) < parts(b)
}
