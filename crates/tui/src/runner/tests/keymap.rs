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
const CONTEXTS: &[&str] = &["list", "reader", "sidebar", "place", "screener", "todo"];

/// A key a pane handles in place (scrolling, moving focus) without
/// returning an action, so the dispatcher probe cannot see it.
struct InlineKey {
    context: &'static str,
    key: &'static str,
    name: &'static str,
    /// State the key needs to have a visible effect (a thread to move
    /// through, a scrolled body to scroll back).
    setup: fn(&mut App),
    /// The state the key changes.
    observe: fn(&App) -> String,
}

fn no_setup(_: &mut App) {}

fn scrolled(app: &mut App) {
    app.mailbox.message_scroll_offset = 40;
}

fn two_message_thread(app: &mut App) {
    app.mailbox.viewed_thread_messages = make_test_envelopes(2);
}

fn on_second_message(app: &mut App) {
    two_message_thread(app);
    app.mailbox.thread_selected_index = 1;
}

fn on_second_item(app: &mut App) {
    app.mailbox.sidebar_selected = 1;
}

fn labels_collapsed(app: &mut App) {
    app.mailbox.sidebar_system_expanded = false;
}

fn scroll(app: &App) -> String {
    app.mailbox.message_scroll_offset.to_string()
}

fn thread_focus(app: &App) -> String {
    app.mailbox.thread_selected_index.to_string()
}

fn pane(app: &App) -> String {
    format!("{:?}", app.mailbox.active_pane)
}

fn sidebar_cursor(app: &App) -> String {
    app.mailbox.sidebar_selected.to_string()
}

fn sections(app: &App) -> String {
    format!(
        "{} {} {}",
        app.mailbox.sidebar_system_expanded,
        app.mailbox.sidebar_user_expanded,
        app.mailbox.sidebar_saved_searches_expanded
    )
}

/// Every inline key, each probed through the state it changes
/// (`inline_keys_change_what_they_say`), so removing a handler fails.
const INLINE: &[InlineKey] = &[
    inline("reader", "j", "scroll_down", no_setup, scroll),
    inline("reader", "ArrowDown", "scroll_down", no_setup, scroll),
    inline("reader", "k", "scroll_up", scrolled, scroll),
    inline("reader", "ArrowUp", "scroll_up", scrolled, scroll),
    inline(
        "reader",
        "J",
        "next_message",
        two_message_thread,
        thread_focus,
    ),
    inline(
        "reader",
        "K",
        "prev_message",
        on_second_message,
        thread_focus,
    ),
    inline("reader", "Ctrl+d", "page_down", no_setup, scroll),
    inline("reader", "Ctrl+u", "page_up", scrolled, scroll),
    inline("reader", "G", "jump_bottom", no_setup, scroll),
    inline("reader", "h", "focus_list", no_setup, pane),
    inline("reader", "ArrowLeft", "focus_list", no_setup, pane),
    inline("list", "h", "focus_sidebar", no_setup, pane),
    inline("list", "ArrowLeft", "focus_sidebar", no_setup, pane),
    inline("place", "h", "focus_sidebar", no_setup, pane),
    inline("place", "ArrowLeft", "focus_sidebar", no_setup, pane),
    inline("todo", "h", "focus_sidebar", no_setup, pane),
    inline("todo", "ArrowLeft", "focus_sidebar", no_setup, pane),
    inline("sidebar", "j", "next_item", no_setup, sidebar_cursor),
    inline(
        "sidebar",
        "ArrowDown",
        "next_item",
        no_setup,
        sidebar_cursor,
    ),
    inline("sidebar", "k", "prev_item", on_second_item, sidebar_cursor),
    inline(
        "sidebar",
        "ArrowUp",
        "prev_item",
        on_second_item,
        sidebar_cursor,
    ),
    inline("sidebar", "[", "collapse_section", no_setup, sections),
    inline("sidebar", "]", "expand_section", labels_collapsed, sections),
];

const fn inline(
    context: &'static str,
    key: &'static str,
    name: &'static str,
    setup: fn(&mut App),
    observe: fn(&App) -> String,
) -> InlineKey {
    InlineKey {
        context,
        key,
        name,
        setup,
        observe,
    }
}

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
        "todo" => app.mailbox.mailbox_view = MailboxView::Todo,
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
            for inline in INLINE.iter().filter(|inline| inline.context == *context) {
                map.insert(inline.key.to_string(), format!("inline:{}", inline.name));
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
fn inline_keys_change_what_they_say() {
    for inline in INLINE {
        let mut app = app_in(inline.context);
        (inline.setup)(&mut app);
        let before = (inline.observe)(&app);
        let mut action = None;
        for key in events_for(inline.key) {
            action = app.handle_key(key);
        }
        let (context, key, name) = (inline.context, inline.key, inline.name);
        assert_eq!(
            action, None,
            "{key} in {context} ({name}) returns an action"
        );
        assert_ne!(
            (inline.observe)(&app),
            before,
            "{key} in {context} is declared as {name} but changes nothing"
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
            let inline = INLINE.iter().any(|inline| {
                inline.context == context
                    && inline.name == name
                    && sequence_of(binding) == events_for(inline.key)
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
