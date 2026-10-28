//! `GetKeymap` and `UpdateKeys`: every keyboard shortcut in every client,
//! from the one definition in `crates/keymap` with the person's `keys.toml`
//! applied (blueprint chapter 35, "The daemon serves the effective key map").
//! The file is read on every request, so an edit applies without a restart.

use super::{HandlerError, HandlerResult};
use mxr_keymap::{Client, KeyAction, Keymap, KeysEdit, KeysFile, ModeRemaps};
use mxr_protocol::{
    KeyActionData, KeymapData, KeysChangeData, KeysUpdateData, ResponseData, TuiKeysData,
};
use std::path::{Path, PathBuf};

pub(super) fn get() -> HandlerResult {
    get_at(&keys_toml_path())
}

pub(super) fn update(change: &KeysChangeData, dry_run: bool) -> HandlerResult {
    update_at(&keys_toml_path(), change, dry_run)
}

/// `keys.toml`, beside the config file.
pub(crate) fn keys_toml_path() -> PathBuf {
    mxr_config::config_file_path()
        .parent()
        .map_or_else(|| "keys.toml".into(), |dir| dir.join("keys.toml"))
}

fn get_at(path: &Path) -> HandlerResult {
    let (keymap, _) = effective(&read(path)?);
    Ok(ResponseData::Keymap { keymap })
}

/// Make `change` to the `keys.toml` at `path`, unless this is a dry run or
/// the change brings a problem the file didn't already have. The config
/// file lock is held across read, check and write, so a concurrent
/// `mxr keys set` can't lose this change or have this one lose its.
fn update_at(path: &Path, change: &KeysChangeData, dry_run: bool) -> HandlerResult {
    let _lock = mxr_config::lock_config_file_at(path).map_err(|error| {
        HandlerError::Message(format!("can't lock {}: {error}", path.display()))
    })?;
    let before = read(path)?;
    let edit = match change {
        KeysChangeData::SetAction { id, keys } => KeysEdit::SetAction {
            id: id.clone(),
            keys: keys.clone(),
        },
        KeysChangeData::ResetAction { id } => KeysEdit::ResetAction { id: id.clone() },
        KeysChangeData::SingleKeys { on } => KeysEdit::SingleKeys(*on),
    };
    let after = mxr_keymap::edit_keys_toml(&before, &edit).map_err(|error| {
        HandlerError::InvalidRequest(format!(
            "{error}; fix {} first (`mxr keys check` names the problem)",
            path.display()
        ))
    })?;
    let (_, already) = effective(&before);
    let (keymap, found) = effective(&after);
    let mut problems: Vec<String> = found
        .into_iter()
        .filter(|problem| !already.contains(problem))
        .collect();
    if let KeysChangeData::SetAction { id, .. } = change {
        if problems.is_empty() && !keymap.remapped.contains(id) {
            problems.push(format!("{id}: the change didn't apply"));
        }
    }
    let saved = !dry_run && problems.is_empty() && after != before;
    if saved {
        write(path, &after)?;
    }
    Ok(ResponseData::KeysUpdate {
        update: KeysUpdateData {
            saved,
            file: path.display().to_string(),
            problems,
            keymap,
        },
    })
}

fn read(path: &Path) -> Result<String, HandlerError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(HandlerError::Message(format!(
            "can't read {}: {error}",
            path.display()
        ))),
    }
}

/// Write through a sibling file and rename it into place, so a TUI starting
/// at that moment reads the old file or the new one, never half of it.
fn write(path: &Path, text: &str) -> Result<(), HandlerError> {
    let io = |error: std::io::Error| {
        HandlerError::Message(format!("can't write {}: {error}", path.display()))
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    let staged = path.with_extension("toml.new");
    std::fs::write(&staged, text).map_err(io)?;
    std::fs::rename(&staged, path).map_err(io)
}

/// The key map for `keys.toml` text, and everything in the file it ignores.
fn effective(text: &str) -> (KeymapData, Vec<String>) {
    let defaults = mxr_keymap::defaults();
    let (file, mut problems) = KeysFile::parse(text, defaults);
    if let Ok(table) = text.parse::<toml::Table>() {
        problems.extend(ModeRemaps::from_table(&table, defaults).1);
    }
    let remapped = defaults.remapped(&file.actions);
    problems.extend(
        remapped
            .problems
            .iter()
            .map(|problem| format!("[{}] {problem}", mxr_keymap::ACTIONS_SECTION)),
    );
    let mut keymap = keymap_data(&remapped.keymap);
    keymap.single_keys = file.single_keys;
    keymap.remapped = remapped.applied.keys().cloned().collect();
    keymap.problems = problems.clone();
    (keymap, problems)
}

fn keymap_data(keymap: &Keymap) -> KeymapData {
    KeymapData {
        actions: keymap.actions.iter().map(action_data).collect(),
        single_keys: true,
        remapped: Vec::new(),
        problems: Vec::new(),
    }
}

fn action_data(action: &KeyAction) -> KeyActionData {
    let is_web = action.is_for(Client::Web);
    KeyActionData {
        id: action.id.clone(),
        label: action.label.clone(),
        description: action.description.clone(),
        group: action.group.clone(),
        clients: action
            .clients
            .iter()
            .map(|client| match client {
                Client::Tui => "tui".to_string(),
                Client::Web => "web".to_string(),
            })
            .collect(),
        scopes: if is_web {
            action.web_scopes().into_iter().map(String::from).collect()
        } else {
            Vec::new()
        },
        keys: action.keys.clone(),
        mac_keys: action.mac_keys.clone(),
        palette_only: action.palette_only,
        note: action.note.clone(),
        tui: action
            .tui
            .iter()
            .map(|binding| TuiKeysData {
                name: binding.name.clone(),
                keys: binding.keys.clone(),
                contexts: binding
                    .contexts
                    .iter()
                    .into_iter()
                    .map(String::from)
                    .collect(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_protocol::MODE_GUIDES;

    fn keymap_at(path: &Path) -> KeymapData {
        match get_at(path).unwrap() {
            ResponseData::Keymap { keymap } => keymap,
            other => panic!("not a keymap: {other:?}"),
        }
    }

    fn change(path: &Path, change: KeysChangeData, dry_run: bool) -> KeysUpdateData {
        match update_at(path, &change, dry_run).unwrap() {
            ResponseData::KeysUpdate { update } => update,
            other => panic!("not a keys update: {other:?}"),
        }
    }

    fn web_keys(keymap: &KeymapData, id: &str) -> Vec<String> {
        keymap
            .actions
            .iter()
            .find(|action| action.id == id)
            .unwrap()
            .keys
            .clone()
    }

    #[test]
    fn the_key_map_serves_keys_toml_and_the_switch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keys.toml");
        let defaults = keymap_at(&path);
        assert!(defaults.single_keys);
        assert!(defaults.remapped.is_empty());
        std::fs::write(
            &path,
            "single_keys = false\n[actions]\n\"mail.archive\" = \"%\"\n\"mail.nope\" = \"x\"\n",
        )
        .unwrap();
        let keymap = keymap_at(&path);
        assert!(!keymap.single_keys);
        assert_eq!(keymap.remapped, vec!["mail.archive"]);
        assert_eq!(web_keys(&keymap, "mail.archive"), vec!["%"]);
        assert!(
            keymap.problems.iter().any(|p| p.contains("mail.nope")),
            "{:?}",
            keymap.problems
        );
    }

    #[test]
    fn a_dry_run_previews_and_a_clash_is_never_written() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keys.toml");
        std::fs::write(&path, "# mine\n").unwrap();
        let set = |keys: &[&str]| KeysChangeData::SetAction {
            id: "mail.archive".into(),
            keys: keys.iter().map(|k| (*k).to_string()).collect(),
        };
        let preview = change(&path, set(&["%"]), true);
        assert!(!preview.saved);
        assert!(preview.problems.is_empty(), "{:?}", preview.problems);
        assert_eq!(web_keys(&preview.keymap, "mail.archive"), vec!["%"]);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# mine\n");

        let star = web_keys(&keymap_at(&path), "mail.star")[0].clone();
        let clash = change(&path, set(&[star.as_str()]), false);
        assert!(!clash.saved);
        assert!(
            clash.problems.iter().any(|p| p.contains("mail.star")),
            "{:?}",
            clash.problems
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# mine\n");

        let saved = change(&path, set(&["%"]), false);
        assert!(saved.saved, "{:?}", saved.problems);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("# mine\n") && text.contains("\"mail.archive\" = \"%\""),
            "{text}"
        );

        let off = change(&path, KeysChangeData::SingleKeys { on: false }, false);
        assert!(off.saved && !off.keymap.single_keys);
        let reset = change(
            &path,
            KeysChangeData::ResetAction {
                id: "mail.archive".into(),
            },
            false,
        );
        assert!(reset.saved);
        assert_eq!(web_keys(&reset.keymap, "mail.archive"), vec!["e"]);
        assert!(!reset.keymap.single_keys);
    }

    #[test]
    fn a_malformed_keys_toml_is_not_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keys.toml");
        std::fs::write(&path, "[todo").unwrap();
        let error = update_at(&path, &KeysChangeData::SingleKeys { on: false }, false).unwrap_err();
        assert!(error.to_string().contains("not valid TOML"), "{error}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[todo");
    }

    /// A mode's own context and its sub-views, in both clients.
    fn places(mode: &str) -> &'static [&'static str] {
        match mode {
            "todo" => &["todo", "catchup", "expired"],
            "reading" => &["reading", "reading-reader"],
            "now" => &["now"],
            "messages" => &["messages", "messages-filters"],
            "updates" => &["updates"],
            "archive" => &["archive"],
            _ => &[],
        }
    }

    /// Every action a mode's guide or hints name has a key, and that key
    /// does that action in that mode in both clients, so `mxr modes
    /// explain`, the `?` sheet and the hints can't advertise a key that does
    /// something else.
    #[test]
    fn every_key_a_mode_guide_shows_does_that_action_there() {
        let keymap = mxr_keymap::defaults();
        let mut wrong = Vec::new();
        for guide in MODE_GUIDES {
            let named = guide
                .keys
                .iter()
                .copied()
                .chain(guide.hints.iter().map(|hint| hint.key));
            for (id, verb) in named {
                let (Some(action), Some(key)) = (keymap.get(id), mxr_keymap::guide_key(id)) else {
                    wrong.push(format!("{}: {id} ({verb}) has no key", guide.mode));
                    continue;
                };
                let places = places(guide.mode);
                let tui = action.tui.iter().any(|binding| {
                    binding.keys.contains(&key)
                        && places.iter().any(|place| binding.contexts.includes(place))
                });
                if !tui {
                    wrong.push(format!(
                        "{}: `{key}` doesn't {verb} ({id}) in the TUI",
                        guide.mode
                    ));
                }
                let web = action.is_for(Client::Web)
                    && action.keys.contains(&key)
                    && action
                        .web_scopes()
                        .iter()
                        .any(|scope| *scope == "global" || places.contains(scope));
                if !web {
                    wrong.push(format!(
                        "{}: `{key}` doesn't {verb} ({id}) in the web app",
                        guide.mode
                    ));
                }
            }
        }
        assert!(
            wrong.is_empty(),
            "mode guides name keys that don't work:\n{}",
            wrong.join("\n")
        );
    }
}
