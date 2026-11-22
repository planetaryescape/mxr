use super::*;
use crate::app::AccountFormMode;
use mxr_protocol::MailPlaceData;
use std::collections::BTreeMap;

const CONTEXTS: &[&str] = &[
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
    "focus",
];

struct InlineKey {
    context: &'static str,
    key: &'static str,
    name: &'static str,
    setup: fn(&mut App),
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

fn under_more(app: &mut App) {
    app.mailbox.sidebar_selected = 9;
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
    inline("reader", "End", "jump_bottom", no_setup, scroll),
    inline("reader", "PageDown", "page_down", no_setup, scroll),
    inline("reader", "Space", "page_down", no_setup, scroll),
    inline("reader", "PageUp", "page_up", scrolled, scroll),
    inline("reader", "h", "focus_list", no_setup, pane),
    inline("reader", "ArrowLeft", "focus_list", no_setup, pane),
    inline("list", "h", "focus_sidebar", no_setup, pane),
    inline("list", "ArrowLeft", "focus_sidebar", no_setup, pane),
    inline("place", "h", "focus_sidebar", no_setup, pane),
    inline("place", "ArrowLeft", "focus_sidebar", no_setup, pane),
    inline("todo", "h", "focus_sidebar", no_setup, pane),
    inline("todo", "ArrowLeft", "focus_sidebar", no_setup, pane),
    inline("now", "h", "focus_sidebar", no_setup, pane),
    inline("now", "ArrowLeft", "focus_sidebar", no_setup, pane),
    inline("messages", "h", "focus_sidebar", no_setup, pane),
    inline("messages", "ArrowLeft", "focus_sidebar", no_setup, pane),
    inline("archive", "h", "focus_sidebar", no_setup, pane),
    inline("archive", "ArrowLeft", "focus_sidebar", no_setup, pane),
    inline("reading", "h", "focus_sidebar", no_setup, pane),
    inline("reading", "ArrowLeft", "focus_sidebar", no_setup, pane),
    inline("updates", "h", "focus_sidebar", no_setup, pane),
    inline("updates", "ArrowLeft", "focus_sidebar", no_setup, pane),
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
    inline(
        "sidebar",
        "g g",
        "first_item",
        on_second_item,
        sidebar_cursor,
    ),
    inline(
        "sidebar",
        "Home",
        "first_item",
        on_second_item,
        sidebar_cursor,
    ),
    inline("sidebar", "G", "last_item", no_setup, sidebar_cursor),
    inline("sidebar", "End", "last_item", no_setup, sidebar_cursor),
    inline("sidebar", "{", "prev_section", under_more, sidebar_cursor),
    inline("sidebar", "}", "next_section", no_setup, sidebar_cursor),
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
        "catchup" | "expired" => {
            app.mailbox.mailbox_view = MailboxView::Todo;
            app.mailbox.todo_page.panel = if context == "catchup" {
                crate::app::TodoPanel::Catchup
            } else {
                crate::app::TodoPanel::Expired
            };
        }
        "now" => app.mailbox.mailbox_view = MailboxView::Now,
        "messages" => app.mailbox.mailbox_view = MailboxView::People,
        "archive" => app.mailbox.mailbox_view = MailboxView::ArchiveMode,
        "reading" => app.mailbox.mailbox_view = MailboxView::Reading,
        "reading-reader" => {
            app.mailbox.mailbox_view = MailboxView::Reading;
            let edition = crate::ui::reading_lens::tests::edition();
            let item = edition.bands[0].items[0].clone();
            let source = edition.sources[0].clone();
            app.set_reading_edition(edition, None);
            app.set_reading_item(
                &item.item_key.clone(),
                mxr_protocol::ReadingItemDetailData {
                    source_data: source,
                    paragraphs: Vec::new(),
                    html: None,
                    article: None,
                    article_error: None,
                    highlights: Vec::new(),
                    marginalia: None,
                    minutes_left: 1,
                    pace_wpm: 230,
                    item,
                },
            );
            app.mailbox.reading_page.reader_focused = true;
        }
        "updates" => app.mailbox.mailbox_view = MailboxView::Updates,
        "focus" => app.apply(Action::OpenFocus),
        "screener" => app.modals.screener.visible = true,
        other => panic!("unknown context {other}"),
    }
    app
}

fn remapped_app(context: &str, toml: &str) -> App {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("keys.toml"), toml).unwrap();
    let mut app = app_in(context);
    app.load_keybindings_from(dir.path());
    app
}

#[test]
fn configured_keys_dispatch_through_app_in_each_mail_context() {
    for (context, section, thread_size) in [
        ("list", "mail_list", 0),
        ("reader", "message_view", 1),
        ("reader", "thread_view", 2),
    ] {
        let mut app = remapped_app(context, &format!("[{section}]\n\";\" = \"trash\"\n"));
        app.mailbox.viewed_thread_messages = make_test_envelopes(thread_size);
        assert_eq!(
            app.handle_key(KeyEvent::new(KeyCode::Char(';'), KeyModifiers::NONE)),
            Some(Action::Trash),
            "{section}"
        );
        assert_eq!(
            app.handle_key(KeyEvent::new(KeyCode::Char('#'), KeyModifiers::SHIFT)),
            None,
            "retired destructive key in {section}"
        );
        assert_eq!(
            app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE)),
            Some(Action::Reply),
            "unconfigured default in {section}"
        );
    }
}

fn press_char(app: &mut App, c: char) -> Option<Action> {
    app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
}

// A regression here can overflow the stack before an assertion runs. Keep it
// in a bounded child test process, without changing the parent's environment.
#[tokio::test]
async fn configured_retired_prefixes_are_bounded() {
    const CHILD: &str = "MXR_TEST_RETIRED_PREFIX_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "runner::tests::keymap::configured_retired_prefixes_are_bounded",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .kill_on_drop(true);
        let output = tokio::time::timeout(std::time::Duration::from_secs(10), command.output())
            .await
            .expect("retired prefix dispatch must finish within ten seconds")
            .unwrap();
        assert!(
            output.status.success(),
            "retired prefix child failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }

    let mut app = remapped_app("list", "[mail_list]\n\";\" = \"center_current\"\n");
    assert_eq!(press_char(&mut app, ';'), Some(Action::CenterCurrent));
    assert_eq!(press_char(&mut app, ';'), Some(Action::CenterCurrent));
    assert_eq!(press_char(&mut app, 'j'), Some(Action::MoveDown));
    let mut app = remapped_app(
        "list",
        "[mail_list]\n\";\" = \"center_current\"\n\"wq\" = \"archive\"\n",
    );
    assert_eq!(press_char(&mut app, 'w'), None);
    assert_eq!(press_char(&mut app, 'z'), None);
    assert!(
        !app.input_pending(),
        "an invalid suffix can be a retired prefix"
    );
    assert_eq!(press_char(&mut app, 'w'), None);
    assert_eq!(press_char(&mut app, 'w'), None);
    assert!(
        !app.input_pending(),
        "a repeated prefix completes nothing, so it cancels the chord"
    );
    assert_eq!(app.status_message.as_deref(), Some("w w is not a shortcut"));
    assert_eq!(press_char(&mut app, 'w'), None);
    assert_eq!(press_char(&mut app, 'q'), Some(Action::Archive));

    let mut app = remapped_app(
        "list",
        "[mail_list]\n\";\" = \"invite_accept\"\n\",\" = \"invite_tentative\"\n\".\" = \"invite_decline\"\n",
    );
    for _ in 0..4 {
        assert_eq!(press_char(&mut app, 'i'), None);
        assert!(!app.input_pending());
    }
    assert_eq!(
        press_char(&mut app, 'a'),
        Some(Action::ReplyAll),
        "retired i must not revive invite_accept on the next key"
    );
    for (key, rsvp) in [
        (';', mxr_protocol::CalendarInviteActionData::Accept),
        (',', mxr_protocol::CalendarInviteActionData::Tentative),
        ('.', mxr_protocol::CalendarInviteActionData::Decline),
    ] {
        assert_eq!(press_char(&mut app, key), Some(Action::RespondInvite(rsvp)));
    }
    let mut partial = remapped_app("list", "[mail_list]\n\";\" = \"invite_accept\"\n");
    assert_eq!(press_char(&mut partial, 'i'), None);
    assert!(
        partial.input_pending(),
        "other invite chords are still live"
    );
    assert_eq!(
        press_char(&mut partial, 'm'),
        Some(Action::RespondInvite(
            mxr_protocol::CalendarInviteActionData::Tentative
        ))
    );
    let mut defaults = app_in("list");
    assert_eq!(
        press_char(&mut defaults, 'z'),
        Some(Action::UndoLastMutation)
    );
    assert!(!defaults.input_pending());
}

#[test]
fn configured_retired_destructive_keys_remain_blocked_with_modifiers() {
    for (old_key, name, action) in [('#', "trash", Action::Trash), ('!', "spam", Action::Spam)] {
        let mut baseline = app_in("list");
        for modifiers in [
            KeyModifiers::NONE,
            KeyModifiers::SHIFT,
            KeyModifiers::CONTROL,
            KeyModifiers::ALT,
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        ] {
            assert_eq!(
                baseline.handle_key(KeyEvent::new(KeyCode::Char(old_key), modifiers)),
                Some(action.clone()),
                "established destructive alias"
            );
            let mut app = remapped_app("list", &format!("[mail_list]\n\";\" = \"{name}\"\n"));
            assert_eq!(
                app.handle_key(KeyEvent::new(KeyCode::Char(old_key), modifiers)),
                None,
                "retired {old_key} with {modifiers:?}"
            );
            assert_eq!(press_char(&mut app, ';'), Some(action.clone()));
            let mut app = remapped_app("list", &format!("[mail_list]\n\"{old_key}\" = \"\"\n"));
            assert_eq!(
                app.handle_key(KeyEvent::new(KeyCode::Char(old_key), modifiers)),
                None,
                "disabled {old_key} with {modifiers:?}"
            );
        }
    }
}

#[test]
fn configured_keys_keep_text_entry_and_modal_owners_safe() {
    let mut app = remapped_app(
        "list",
        "[mail_list]\n\"wq\" = \"trash\"\n\";\" = \"archive\"\n",
    );
    assert_eq!(press_char(&mut app, 'w'), None);
    app.search.bar.active = true;
    assert_eq!(press_char(&mut app, ';'), None);
    assert_eq!(press_char(&mut app, 'q'), None);
    assert!(app.search.bar.query.ends_with(";q"));
    app.search.bar.active = false;
    assert_eq!(
        press_char(&mut app, 'q'),
        Some(Action::QuitView),
        "no stale chord after text entry"
    );

    assert_eq!(press_char(&mut app, 'w'), None);
    app.modals.help_open = true;
    assert_eq!(press_char(&mut app, ';'), None);
    assert_eq!(app.modals.help_query, ";");
    app.modals.help_open = false;
    assert_eq!(press_char(&mut app, 'q'), Some(Action::QuitView));

    app.modals.draft_options.visible = true;
    app.modals.draft_options.active = crate::app::DraftOptionsField::Instruction;
    assert_eq!(press_char(&mut app, ';'), None);
    assert!(app.modals.draft_options.instruction.ends_with(';'));
    app.modals.draft_options.visible = false;

    app.screen = Screen::Search;
    app.search.page.editing = true;
    assert_eq!(press_char(&mut app, ';'), None);
    assert!(app.search.page.query.ends_with(';'));
    app.screen = Screen::Rules;
    app.rules.page.form.visible = true;
    assert_eq!(press_char(&mut app, ';'), None);
    assert!(app.rules.page.form.name.ends_with(';'));
    app.rules.page.form.visible = false;
    assert_ne!(
        press_char(&mut app, ';'),
        Some(Action::Archive),
        "mail remaps do not leak into rules"
    );
}

pub(super) fn probe_keys() -> Vec<(KeyEvent, String)> {
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
         and run `cargo test -p mxr-keymap`"
    );
}

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

#[test]
fn a_mode_section_remaps_that_mode_and_retires_the_moved_default() {
    let toml = "[todo]\n\"x\" = \"todo.done\"\n\"Z\" = \"\"\n\"d d\" = \"todo.edit\"\n";
    let mut app = remapped_app("todo", toml);
    assert_eq!(press_char(&mut app, 'x'), Some(Action::TodoDone));
    assert_eq!(press_char(&mut app, 'e'), None, "e moved to x");
    assert_eq!(
        app.handle_key(KeyEvent::new(KeyCode::Char('Z'), KeyModifiers::SHIFT)),
        None,
        "an empty action unbinds the key"
    );
    assert_eq!(press_char(&mut app, 'd'), None);
    assert!(app.input_pending(), "d starts the person's own sequence");
    assert_eq!(press_char(&mut app, 'd'), Some(Action::TodoEdit));
    let mut app = remapped_app("now", toml);
    assert_eq!(
        press_char(&mut app, 'e'),
        Some(Action::NowDone),
        "other modes keep e"
    );
}

#[test]
fn a_moved_key_names_its_new_key_and_runs_nothing() {
    let todo = |until| {
        let mut app = app_in("todo");
        with_move(&mut app, "todo.catchup", "C", &["todo"], until);
        app
    };
    let mut app = todo("999.0.0");
    let shifted = |c| KeyEvent::new(KeyCode::Char(c), KeyModifiers::SHIFT);
    assert_eq!(app.handle_key(shifted('C')), None);
    let notice = app.status_message.clone().unwrap_or_default();
    assert!(notice.ends_with("is now 2"), "{notice}");
    assert_eq!(press_char(&mut app, '2'), Some(Action::TodoOpenCatchup));

    let mut app = todo("999.0.0");
    assert_eq!(press_char(&mut app, 'g'), None);
    assert_eq!(
        app.handle_key(shifted('C')),
        Some(Action::OpenTab4),
        "g C is a sequence, not the moved C"
    );

    let mut app = app_in("list");
    with_move(
        &mut app,
        "mail.snooze",
        "Z",
        &["list", "reader", "place"],
        "999.0.0",
    );
    assert_eq!(app.handle_key(shifted('Z')), None);
    let notice = app.status_message.clone().unwrap_or_default();
    assert!(notice.ends_with("is now b"), "{notice}");
    assert_eq!(press_char(&mut app, 'b'), Some(Action::Snooze));
}

#[test]
fn sidebar_headers_hold_the_cursor_and_fold_their_section() {
    let mut app = app_in("sidebar");
    assert_eq!(press_char(&mut app, '}'), None);
    assert!(matches!(
        app.selected_sidebar_item(),
        Some(crate::app::SidebarItem::Header(
            crate::app::SidebarHeader::More
        ))
    ));
    let header = app.mailbox.sidebar_selected;
    assert!(app.mailbox.sidebar_system_expanded);
    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(app.handle_key(enter), None);
    assert!(!app.mailbox.sidebar_system_expanded, "Enter folds More");
    assert_eq!(app.mailbox.sidebar_selected, header, "the cursor stays");
    assert_eq!(app.handle_key(enter), None);
    assert!(app.mailbox.sidebar_system_expanded, "Enter unfolds it");
    assert_eq!(press_char(&mut app, '{'), None);
    assert_eq!(
        app.mailbox.sidebar_selected, 0,
        "{{ from the first header goes to the top"
    );
}

#[test]
fn with_single_keys_off_e_does_nothing_and_the_palette_still_archives() {
    for context in ["list", "reader", "todo", "now"] {
        let mut app = remapped_app(context, "single_keys = false\n");
        app.mailbox.viewed_thread_messages = make_test_envelopes(1);
        assert_eq!(press_char(&mut app, 'e'), None, "{context}");
        assert_eq!(press_char(&mut app, 'g'), None, "{context}");
        assert!(!app.input_pending(), "{context}: no prefix starts");
        assert_eq!(
            app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL)),
            Some(Action::OpenCommandPalette),
            "{context}: modifier chords still work"
        );
    }
    let mut app = remapped_app("list", "single_keys = false\n");
    app.mailbox.envelopes = make_test_envelopes(2);
    assert_eq!(
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
        Some(Action::MoveDown),
        "named keys still work"
    );
    app.command_palette.palette.visible = true;
    app.apply(Action::OpenCommandPalette);
    app.command_palette
        .palette
        .toggle_with_archive_operations(app.current_ui_context(), &[]);
    for c in "archive".chars() {
        assert_eq!(
            press_char(&mut app, c),
            None,
            "letters type into the palette"
        );
    }
    assert_eq!(app.command_palette.palette.input, "archive");
    let position = app
        .command_palette
        .palette
        .visible_commands()
        .position(|command| command.action == Action::Archive)
        .expect("the palette offers Archive");
    app.command_palette.palette.selected = position;
    assert_eq!(
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        Some(Action::Archive)
    );
}

#[test]
fn with_single_keys_off_the_account_form_types_letters_into_its_text_fields() {
    let mut app = remapped_app("list", "single_keys = false\n");
    app.screen = Screen::Accounts;
    app.accounts.page.form.visible = true;
    app.accounts.page.form.mode = AccountFormMode::ImapSmtp;
    app.accounts.page.form.active_field = 2;
    for c in "shark".chars() {
        assert_eq!(press_char(&mut app, c), None, "{c} is text, not a command");
    }
    assert_eq!(app.accounts.page.form.name, "shark");
    assert!(app.accounts.page.form.editing_field);
    assert!(app.status_message.is_none(), "no single-key notice");
}
