//! The TUI keymap as the dispatcher sees it. Every key is pressed into a
//! fresh `App` in each mail context and the action it returns is recorded
//! in `docs/reference/tui-keymap.json`. The web app's parity test reads
//! that file, so the two clients cannot drift apart silently.
//!
//! Regenerate after changing a TUI key: `UPDATE_KEYMAP=1` on this test.

use super::*;
use crate::keybindings::{action_from_name, default_keybindings, KeyBinding};
use mxr_protocol::MailPlaceData;
use std::collections::BTreeMap;

/// Mail contexts shared with the web app's shortcut scopes.
const CONTEXTS: &[&str] = &["list", "reader", "sidebar", "place", "screener"];

/// Keys a pane handles in place (scrolling, moving focus) without
/// returning an action, so probing cannot see them. Declared here so the
/// inventory is complete; `inline_keys_really_are_inline` checks each one
/// is swallowed rather than unbound.
const INLINE: &[(&str, &str, &str)] = &[
    ("reader", "j", "scroll_down"),
    ("reader", "ArrowDown", "scroll_down"),
    ("reader", "k", "scroll_up"),
    ("reader", "ArrowUp", "scroll_up"),
    ("reader", "J", "next_message"),
    ("reader", "K", "prev_message"),
    ("reader", "Ctrl+d", "page_down"),
    ("reader", "Ctrl+u", "page_up"),
    ("reader", "G", "jump_bottom"),
    ("reader", "h", "focus_list"),
    ("reader", "ArrowLeft", "focus_list"),
    ("list", "h", "focus_sidebar"),
    ("list", "ArrowLeft", "focus_sidebar"),
    ("place", "h", "focus_sidebar"),
    ("place", "ArrowLeft", "focus_sidebar"),
    ("sidebar", "j", "next_item"),
    ("sidebar", "ArrowDown", "next_item"),
    ("sidebar", "k", "prev_item"),
    ("sidebar", "ArrowUp", "prev_item"),
    ("sidebar", "[", "collapse_section"),
    ("sidebar", "]", "expand_section"),
];

fn app_in(context: &str) -> App {
    let mut app = App::new();
    app.screen = Screen::Mailbox;
    app.mailbox.mailbox_view = MailboxView::Messages;
    app.mailbox.active_pane = ActivePane::MailList;
    match context {
        "list" => {}
        "reader" => app.mailbox.active_pane = ActivePane::MessageView,
        "sidebar" => app.mailbox.active_pane = ActivePane::Sidebar,
        "place" => app.mailbox.mailbox_view = MailboxView::Place(MailPlaceData::Reading),
        "screener" => app.modals.screener.visible = true,
        other => panic!("unknown context {other}"),
    }
    app
}

/// A key and its name in the web app's chord grammar (`lib/keys/chord.ts`).
fn probe_keys() -> Vec<(KeyEvent, String)> {
    let mut keys = Vec::new();
    for c in '!'..='~' {
        let modifiers = if c.is_ascii_uppercase() {
            KeyModifiers::SHIFT
        } else {
            KeyModifiers::NONE
        };
        keys.push((KeyEvent::new(KeyCode::Char(c), modifiers), c.to_string()));
    }
    keys.push((
        KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE),
        "Space".into(),
    ));
    for c in 'a'..='z' {
        keys.push((
            KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL),
            format!("Ctrl+{c}"),
        ));
    }
    for (code, name) in [
        (KeyCode::Enter, "Enter"),
        (KeyCode::Esc, "Escape"),
        (KeyCode::Tab, "Tab"),
        (KeyCode::Backspace, "Backspace"),
        (KeyCode::Delete, "Delete"),
        (KeyCode::Up, "ArrowUp"),
        (KeyCode::Down, "ArrowDown"),
        (KeyCode::Left, "ArrowLeft"),
        (KeyCode::Right, "ArrowRight"),
        (KeyCode::Home, "Home"),
        (KeyCode::End, "End"),
        (KeyCode::PageUp, "PageUp"),
        (KeyCode::PageDown, "PageDown"),
    ] {
        keys.push((KeyEvent::new(code, KeyModifiers::NONE), name.into()));
    }
    keys
}

/// Press `sequence` into a fresh app in `context`. Returns the action, or
/// `Err(())` when the last key left a chord waiting for more.
fn press(context: &str, sequence: &[KeyEvent]) -> Result<Option<Action>, ()> {
    let mut app = app_in(context);
    let mut action = None;
    for key in sequence {
        action = app.handle_key(*key);
    }
    if action.is_none() && app.input_pending() {
        return Err(());
    }
    Ok(action)
}

fn action_name(action: &Action) -> String {
    format!("{action:?}")
}

/// Every key and chord in `context` that returns an action. Leaves out
/// what is only an echo of another key: a pending chord that gives up and
/// re-reads its second key (`g V` is just `V`), and handlers that ignore
/// modifiers (`Ctrl+j` is just `j`).
fn dispatched(context: &str) -> BTreeMap<String, String> {
    let keys = probe_keys();
    let singles: Vec<Result<Option<Action>, ()>> = keys
        .iter()
        .map(|(key, _)| press(context, &[*key]))
        .collect();
    let alone = |key: KeyEvent| {
        keys.iter()
            .position(|(candidate, _)| *candidate == key)
            .and_then(|index| singles[index].clone().ok().flatten())
    };
    let mut map = BTreeMap::new();
    for ((first, first_name), single) in keys.iter().zip(&singles) {
        match single {
            Ok(Some(action)) => {
                let echo = first.modifiers == KeyModifiers::CONTROL
                    && alone(KeyEvent::new(first.code, KeyModifiers::NONE)).as_ref()
                        == Some(action);
                if !echo {
                    map.insert(first_name.clone(), action_name(action));
                }
            }
            Ok(None) => {}
            Err(()) => {
                // Chords end in a printable key; named and Ctrl keys only
                // cancel the chord.
                let printable = keys.iter().filter(|(key, _)| {
                    matches!(key.code, KeyCode::Char(c) if c != ' ')
                        && key.modifiers != KeyModifiers::CONTROL
                });
                for (second, second_name) in printable {
                    let Ok(Some(action)) = press(context, &[*first, *second]) else {
                        continue;
                    };
                    let echo = alone(*second).as_ref() == Some(&action)
                        || InputHandler::new().handle_key(*second).as_ref() == Some(&action);
                    if !echo {
                        map.insert(format!("{first_name} {second_name}"), action_name(&action));
                    }
                }
            }
        }
    }
    if context == "sidebar" {
        // Enter opens whichever item is selected, so its action depends on
        // the sidebar's contents. Name the key, not today's first item.
        let open =
            alone(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)).map(|a| action_name(&a));
        for (key, action) in &mut map {
            if !key.contains(' ') && Some(&*action) == open.as_ref() {
                *action = "SidebarSelect".into();
            }
        }
    }
    map
}

fn inventory() -> BTreeMap<String, BTreeMap<String, String>> {
    CONTEXTS
        .iter()
        .map(|context| {
            let mut map = dispatched(context);
            for (inline_context, key, name) in INLINE {
                if inline_context == context {
                    map.insert((*key).to_string(), format!("inline:{name}"));
                }
            }
            ((*context).to_string(), map)
        })
        .collect()
}

fn keymap_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/reference/tui-keymap.json")
}

#[test]
fn tui_keymap_json_matches_the_dispatcher() {
    let generated = format!(
        "{}\n",
        serde_json::to_string_pretty(&inventory()).expect("keymap serializes")
    );
    let path = keymap_path();
    if std::env::var("UPDATE_KEYMAP").as_deref() == Ok("1") {
        std::fs::write(&path, &generated).expect("write tui-keymap.json");
        return;
    }
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        current == generated,
        "docs/reference/tui-keymap.json is stale; rerun this test with UPDATE_KEYMAP=1 \
         and check the web parity test (apps/web keymapParity.test.ts)"
    );
}

/// The key events for a key name from the inventory, such as `g g`.
fn events_for(name: &str) -> Vec<KeyEvent> {
    let keys = probe_keys();
    name.split(' ')
        .map(|token| {
            keys.iter()
                .find(|(_, candidate)| candidate == token)
                .map_or_else(|| panic!("{token} is not a probe key"), |(event, _)| *event)
        })
        .collect()
}

#[test]
fn inline_keys_really_are_inline() {
    for (context, key, name) in INLINE {
        assert_eq!(
            press(context, &events_for(key)),
            Ok(None),
            "{key} in {context} is declared inline ({name}) but returns an action or waits"
        );
    }
}

fn sequence_of(binding: &KeyBinding) -> Vec<KeyEvent> {
    binding
        .keys
        .iter()
        .map(|press| KeyEvent::new(press.code, press.modifiers))
        .collect()
}

/// Help, the hint bar and the palette show the default binding tables;
/// every key they show must do what they say.
#[test]
fn shown_bindings_do_what_help_says() {
    let config = default_keybindings();
    let mut wrong = Vec::new();
    for (context, table) in [
        ("list", &config.mail_list),
        ("reader", &config.message_view),
        ("reader", &config.thread_view),
    ] {
        for (binding, name) in table {
            if name == "dump_action_trace" {
                continue;
            }
            let shown = crate::keybindings::format_keybinding(binding);
            let inline = INLINE.iter().any(|(inline_context, key, inline_name)| {
                *inline_context == context
                    && *inline_name == name
                    && sequence_of(binding) == events_for(key)
            });
            if inline {
                continue;
            }
            let expected = action_from_name(name);
            let actual = press(context, &sequence_of(binding)).ok().flatten();
            if expected.is_none() || actual != expected {
                wrong.push(format!(
                    "{context}: `{shown}` is shown as {name} but does {actual:?}"
                ));
            }
        }
    }
    wrong.sort();
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
