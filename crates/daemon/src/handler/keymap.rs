//! `GetKeymap`: every keyboard shortcut in every client, from the one
//! definition in `crates/keymap` (blueprint chapter 35).

use super::HandlerResult;
use mxr_keymap::{Client, KeyAction, Keymap};
use mxr_protocol::{KeyActionData, KeymapData, ResponseData, TuiKeysData};

pub(super) fn get() -> HandlerResult {
    Ok(ResponseData::Keymap {
        keymap: keymap_data(mxr_keymap::defaults()),
    })
}

fn keymap_data(keymap: &Keymap) -> KeymapData {
    KeymapData {
        actions: keymap.actions.iter().map(action_data).collect(),
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
    use mxr_keymap::Client;
    use mxr_protocol::MODE_GUIDES;

    /// A mode's own context and its sub-views, in both clients.
    fn places(mode: &str) -> &'static [&'static str] {
        match mode {
            "todo" => &["todo", "catchup", "expired"],
            "reading" => &["reading", "reading-reader"],
            "now" => &["now"],
            "messages" => &["messages"],
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
