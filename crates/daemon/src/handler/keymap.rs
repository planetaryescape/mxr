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
