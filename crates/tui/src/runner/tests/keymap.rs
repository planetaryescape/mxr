//! The TUI keymap as the dispatcher sees it. Every key is pressed into a
//! fresh `App` in each mail context and the action it returns is recorded
//! in `docs/reference/tui-keymap.json`. The web app's parity test reads
//! that file, so the two clients cannot drift apart silently.
//!
//! Regenerate after changing a TUI key: `UPDATE_KEYMAP=1` on this test,
//! then change the key in `crates/keymap/keymap.toml` to match;
//! `cargo test -p mxr-keymap` fails until the two agree.

use super::*;
use crate::keybindings::{action_from_name, default_keybindings, KeyBinding};
use mxr_protocol::MailPlaceData;
use std::collections::BTreeMap;

/// Mail contexts shared with the web app's shortcut scopes.
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
];

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

/// The cursor below the More header, so `{` has a header to go back to.
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
    // `z z` retired: `z` still waits so the pair can say so, and runs nothing.
    let mut defaults = app_in("list");
    assert_eq!(press_char(&mut defaults, 'z'), None);
    assert!(defaults.input_pending());
    assert_eq!(press_char(&mut defaults, 'z'), None);
    assert!(!defaults.input_pending());
}

fn rendered_help_lines(app: &mut App, query: &str) -> Vec<String> {
    app.modals.help_open = true;
    app.modals.help_query = query.into();
    let backend = ratatui::backend::TestBackend::new(140, 60);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal.draw(|frame| app.draw(frame)).unwrap();
    let popup = crate::ui::centered_rect(88, 88, ratatui::layout::Rect::new(0, 0, 140, 60));
    let buffer = terminal.backend().buffer();
    (popup.y..popup.bottom())
        .map(|y| {
            (popup.x..popup.right())
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_matches(|c: char| c == '│' || c.is_whitespace())
                .to_string()
        })
        .collect()
}

fn assert_search_help_matches_dispatch(source: &str, archive_key: char) {
    for (pane, message_count) in [
        (SearchPane::Results, 0),
        (SearchPane::Preview, 1),
        (SearchPane::Preview, 2),
    ] {
        let mut app = remapped_app("list", source);
        app.screen = Screen::Search;
        app.search.page.editing = false;
        app.search.page.active_pane = pane;
        app.mailbox.viewed_thread_messages = make_test_envelopes(message_count);
        assert_eq!(press_char(&mut app, '/'), None);
        assert!(app.search.page.editing, "slash edits the existing query");
        app.search.page.editing = false;
        let lines = rendered_help_lines(&mut app, "Edit the query");
        assert!(
            lines
                .iter()
                .any(|line| line.starts_with('/') && line.contains("Edit the query")),
            "missing query control in {pane:?}: {lines:?}"
        );
        let back = if pane == SearchPane::Results {
            "Back to the mailbox"
        } else {
            "Back to the results"
        };
        let lines = rendered_help_lines(&mut app, back);
        assert!(
            lines
                .iter()
                .any(|line| line.contains("Esc") && line.contains(back)),
            "missing back control in {pane:?}: {lines:?}"
        );
        let lines = rendered_help_lines(&mut app, "Search mail");
        assert!(
            !lines
                .iter()
                .any(|line| line.starts_with('/') && line.contains("Search mail")),
            "slash must not advertise a different action in Search: {lines:?}"
        );
        let lines = rendered_help_lines(&mut app, "Run the search");
        assert!(
            !lines
                .iter()
                .any(|line| line.starts_with("Enter") && line.contains("Run the search")),
            "Enter opens a message outside query editing: {lines:?}"
        );
        let lines = rendered_help_lines(&mut app, "Archive");
        assert!(
            lines.iter().any(|line| line.split_whitespace().next()
                == Some(archive_key.to_string().as_str())
                && line.contains("Archive")),
            "missing effective archive key in {pane:?}: {lines:?}"
        );
        if archive_key != 'e' {
            assert!(
                !lines
                    .iter()
                    .any(|line| line.split_whitespace().next() == Some("e")
                        && line.contains("Archive")),
                "retired e must not be advertised: {lines:?}"
            );
        }
        app.modals.help_open = false;
        assert_eq!(press_char(&mut app, archive_key), Some(Action::Archive));
        if archive_key != 'e' {
            assert_eq!(press_char(&mut app, 'e'), None);
        }
        if pane == SearchPane::Preview {
            assert_eq!(press_char(&mut app, 'h'), None);
            assert_eq!(app.search.page.active_pane, SearchPane::Results);
            app.search.page.active_pane = SearchPane::Preview;
            assert_eq!(
                app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
                None
            );
            assert_eq!(app.search.page.active_pane, SearchPane::Results);
        } else {
            assert_eq!(
                app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
                Some(Action::OpenMailboxScreen)
            );
        }
    }
}

#[test]
fn configured_search_help_preserves_default_query_and_back_controls() {
    assert_search_help_matches_dispatch("", 'e');
}

#[test]
fn configured_search_help_preserves_query_and_back_controls_with_mail_remaps() {
    assert_search_help_matches_dispatch("[mail_list]\n\";\" = \"archive\"\n[message_view]\n\";\" = \"archive\"\n[thread_view]\n\";\" = \"archive\"\n", ';');
}

#[test]
fn configured_search_help_does_not_restore_explicitly_overridden_controls() {
    let source = ["mail_list", "message_view", "thread_view"]
        .map(|section| {
            format!("[{section}]\n\"/\" = \"archive\"\n\"h\" = \"trash\"\n\"Esc\" = \"spam\"\n")
        })
        .join("");
    for pane in [SearchPane::Results, SearchPane::Preview] {
        let mut app = remapped_app("list", &source);
        app.screen = Screen::Search;
        app.search.page.editing = false;
        app.search.page.active_pane = pane;
        for (query, key) in [("Archive", "/"), ("Trash", "h"), ("spam", "Esc")] {
            let lines = rendered_help_lines(&mut app, query);
            assert!(lines.iter().any(|line| line.split_whitespace().next() == Some(key) && line.contains(query)), "missing configured control: {lines:?}");
        }
        for query in [
            "Edit the query",
            "Back to the results",
            "Back to the mailbox",
        ] {
            let lines = rendered_help_lines(&mut app, query);
            assert!(
                !lines.iter().any(|line| matches!(
                    line.split_whitespace().next(),
                    Some("/" | "h" | "Esc")
                ) && line.contains(query)),
                "overridden control must not be restored: {lines:?}"
            );
        }
        app.modals.help_open = false;
        assert_eq!(press_char(&mut app, '/'), Some(Action::Archive));
        assert!(!app.search.page.editing);
        assert_eq!(press_char(&mut app, 'h'), Some(Action::Trash));
        assert_eq!(
            app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            Some(Action::Spam)
        );
        assert_eq!(app.search.page.active_pane, pane);
    }
}

#[test]
fn configured_plain_a_remap_preserves_unrelated_ctrl_a() {
    let mut app = remapped_app("list", "[mail_list]\n\";\" = \"reply_all\"\n");
    assert_eq!(press_char(&mut app, 'a'), None);
    assert_eq!(press_char(&mut app, ';'), Some(Action::ReplyAll));
    assert_eq!(
        app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL)),
        None
    );
    assert_eq!(
        app.status_message.as_deref(),
        Some("Toggle assistant is now @"),
        "retiring plain a must not swallow Ctrl-a's move notice"
    );
}

#[test]
fn configured_plain_a_unbind_preserves_unrelated_ctrl_a() {
    let mut app = remapped_app("list", "[mail_list]\n\"a\" = \"\"\n");
    assert_eq!(press_char(&mut app, 'a'), None);
    assert_eq!(
        app.handle_key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL)),
        None
    );
    assert_eq!(
        app.status_message.as_deref(),
        Some("Toggle assistant is now @"),
        "unbinding plain a must not swallow Ctrl-a's move notice"
    );
}

#[test]
fn configured_modifier_specific_keys_preserve_plain_alt_and_control_defaults() {
    // These navigation aliases intentionally ignore modifiers in the existing
    // dispatcher. A different, explicit modified binding must not claim them.
    for (source, rebound, retired) in [
        (
            "[mail_list]\n\"Alt-j\" = \"reply_all\"\n",
            KeyModifiers::ALT,
            'a',
        ),
        (
            "[mail_list]\n\"Ctrl-j\" = \"reply_all\"\n",
            KeyModifiers::CONTROL,
            'a',
        ),
    ] {
        let mut app = remapped_app("list", source);
        assert_eq!(press_char(&mut app, retired), None);
        assert_eq!(
            app.handle_key(KeyEvent::new(KeyCode::Char('j'), rebound)),
            Some(Action::ReplyAll)
        );
        for modifiers in [KeyModifiers::NONE, KeyModifiers::ALT, KeyModifiers::CONTROL] {
            if modifiers != rebound {
                assert_eq!(
                    app.handle_key(KeyEvent::new(KeyCode::Char('j'), modifiers)),
                    Some(Action::MoveDown)
                );
            }
        }
    }
}

#[test]
fn configured_plain_mail_keys_preserve_other_control_shortcuts_and_retire_real_aliases() {
    for (source, key, expected) in [
        (
            "[mail_list]\n\";\" = \"forward\"\n",
            'f',
            Action::OpenMailboxFilter,
        ),
        (
            "[mail_list]\n\"f\" = \"\"\n",
            'f',
            Action::OpenMailboxFilter,
        ),
        (
            "[mail_list]\n\";\" = \"sender_profile\"\n",
            'p',
            Action::OpenCommandPalette,
        ),
        (
            "[mail_list]\n\"p\" = \"\"\n",
            'p',
            Action::OpenCommandPalette,
        ),
    ] {
        let mut baseline = app_in("list");
        let modified = KeyEvent::new(KeyCode::Char(key), KeyModifiers::CONTROL);
        assert_eq!(baseline.handle_key(modified), Some(expected.clone()));
        let mut app = remapped_app("list", source);
        assert_eq!(app.handle_key(modified), Some(expected));
        assert_eq!(
            press_char(&mut app, key),
            None,
            "the plain key is still retired"
        );
    }
    for source in [
        "[mail_list]\n\";\" = \"move_down\"\n",
        "[mail_list]\n\"j\" = \"\"\n",
    ] {
        for modifiers in [KeyModifiers::NONE, KeyModifiers::ALT, KeyModifiers::CONTROL] {
            let key = KeyEvent::new(KeyCode::Char('j'), modifiers);
            assert_eq!(app_in("list").handle_key(key), Some(Action::MoveDown));
            assert_eq!(
                remapped_app("list", source).handle_key(key),
                None,
                "the real move_down alias must remain retired"
            );
        }
    }
}

#[test]
fn configured_modified_mail_keys_do_not_steal_focused_text_owners() {
    let mut app = remapped_app(
        "list",
        "[mail_list]\n\"Ctrl-Shift-A\" = \"trash\"\n\";\" = \"reply_all\"\n",
    );
    let modified = KeyEvent::new(
        KeyCode::Char('a'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    app.search.bar.active = true;
    assert_eq!(app.handle_key(modified), None);
    assert!(app.search.bar.query.is_empty());
    app.search.bar.active = false;
    app.modals.help_open = true;
    assert_eq!(app.handle_key(modified), None);
    app.modals.help_open = false;
    app.assistant.open = true;
    app.assistant.focus = crate::app::AssistantFocus::Input;
    for c in ['a', ';', '#', '!'] {
        assert_eq!(press_char(&mut app, c), None);
    }
    assert_eq!(app.assistant.input, "a;#!");
    assert_ne!(app.handle_key(modified), Some(Action::Trash));
    app.assistant.focus = crate::app::AssistantFocus::Away;
    assert_eq!(app.handle_key(modified), Some(Action::Trash));
    assert_eq!(press_char(&mut app, 'A'), Some(Action::AttachmentList));
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
fn configured_chord_beats_mail_keys_and_retires_old_chord() {
    let mut app = remapped_app(
        "list",
        "[mail_list]\n\"ra\" = \"trash\"\n\";\" = \"go_inbox\"\n",
    );
    assert_eq!(press_char(&mut app, 'r'), None);
    assert!(app.input_pending());
    assert_eq!(press_char(&mut app, 'a'), Some(Action::Trash));
    assert!(!app.input_pending());
    assert_eq!(press_char(&mut app, '#'), None);
    assert_eq!(press_char(&mut app, 'g'), None);
    assert_eq!(
        press_char(&mut app, 'i'),
        None,
        "retired gi must not navigate"
    );
    assert_eq!(press_char(&mut app, ';'), Some(Action::GoToInbox));
    // A default chord with the same prefix still works, including the
    // dynamic saved-search chords not present in the display registry.
    assert_eq!(press_char(&mut app, 'g'), None);
    assert_eq!(press_char(&mut app, 's'), Some(Action::GoToStarred));
    assert_eq!(press_char(&mut app, 'g'), None);
    assert_eq!(
        press_char(&mut app, '2'),
        Some(Action::OpenSavedSearchByIndex(2))
    );
    let mut app = remapped_app("list", "[mail_list]\n\";\" = \"reply_all\"\n");
    assert_eq!(press_char(&mut app, 'g'), None);
    assert_eq!(press_char(&mut app, 'a'), Some(Action::GoToAllMail));
    assert_eq!(press_char(&mut app, 'i'), None);
    assert_eq!(
        press_char(&mut app, 'a'),
        Some(Action::RespondInvite(
            mxr_protocol::CalendarInviteActionData::Accept
        ))
    );
    assert_eq!(press_char(&mut app, 'g'), None);
    assert_eq!(
        press_char(&mut app, ';'),
        None,
        "a remapped key after a default prefix completes nothing and runs nothing"
    );
    assert_eq!(app.status_message.as_deref(), Some("g ; is not a shortcut"));
    assert_eq!(press_char(&mut app, ';'), Some(Action::ReplyAll));
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

#[test]
fn configured_keys_preserve_reader_navigation_semantics() {
    let mut app = remapped_app(
        "reader",
        "[thread_view]\n\";\" = \"scroll_down\"\n\"w\" = \"next_message\"\n\"z\" = \"page_down\"\n",
    );
    app.mailbox.viewed_thread_messages = make_test_envelopes(2);
    assert_eq!(press_char(&mut app, ';'), None);
    assert!(app.mailbox.message_scroll_offset > 0);
    let before = app.mailbox.message_scroll_offset;
    assert_eq!(press_char(&mut app, 'j'), None);
    assert_eq!(app.mailbox.message_scroll_offset, before);
    assert_eq!(
        app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL)),
        None
    );
    assert_eq!(
        app.mailbox.message_scroll_offset, before,
        "retired reader page key must not run its inline fallback"
    );
    assert_eq!(press_char(&mut app, 'z'), None);
    assert_eq!(app.mailbox.message_scroll_offset, before + 20);
    assert_eq!(press_char(&mut app, 'w'), None);
    assert_eq!(app.mailbox.thread_selected_index, 1);
    assert_eq!(press_char(&mut app, 'K'), None);
    assert_eq!(
        app.mailbox.thread_selected_index, 0,
        "unconfigured previous-message default survives scroll remap"
    );
}

#[test]
fn configured_uppercase_modifiers_unbinds_and_context_changes_are_safe() {
    let mut app = remapped_app("list", "[mail_list]\n\"Ctrl-Shift-A\" = \"trash\"\n\"e\" = \"\"\n\"wq\" = \"archive\"\n[message_view]\n\";\" = \"compose\"\n");
    for (code, modifiers) in [
        ('A', KeyModifiers::CONTROL | KeyModifiers::SHIFT),
        ('a', KeyModifiers::CONTROL | KeyModifiers::SHIFT),
        ('A', KeyModifiers::CONTROL),
    ] {
        assert_eq!(
            app.handle_key(KeyEvent::new(KeyCode::Char(code), modifiers)),
            Some(Action::Trash)
        );
    }
    assert_eq!(press_char(&mut app, '#'), None);
    assert_eq!(
        app.handle_key(KeyEvent::new(KeyCode::Char('#'), KeyModifiers::CONTROL)),
        None
    );
    assert_eq!(press_char(&mut app, 'e'), None);
    assert_eq!(
        press_char(&mut app, 'A'),
        Some(Action::AttachmentList),
        "Ctrl-Shift-A does not steal plain A"
    );
    assert_eq!(press_char(&mut app, 'w'), None);
    app.mailbox.active_pane = ActivePane::MessageView;
    // q in the reader closes it (it sits below the root), not `w q`.
    assert_eq!(press_char(&mut app, 'q'), Some(Action::Back));
    assert_eq!(
        press_char(&mut app, ';'),
        Some(Action::Compose),
        "the reader's own map applies after a context change"
    );
    app.mailbox.active_pane = ActivePane::MailList;
    assert_eq!(press_char(&mut app, 'w'), None);
    app.mailbox.mailbox_view = MailboxView::People;
    assert_eq!(
        press_char(&mut app, 'q'),
        Some(Action::QuitView),
        "a chord does not survive a mode change"
    );
}

#[test]
fn configured_help_render_and_palette_take_the_apps_effective_snapshot() {
    let mut app = remapped_app(
        "reader",
        "[message_view]\n\";\" = \"archive\"\n[mail_list]\n\"w\" = \"archive\"\n",
    );
    // Closing an already-open palette avoids reading archive recovery state
    // from the developer's data directory, while exercising App's wiring.
    app.command_palette.palette.visible = true;
    app.apply(Action::OpenCommandPalette);
    app.command_palette
        .palette
        .toggle_with_archive_operations(app.current_ui_context(), &[]);
    assert_eq!(
        app.command_palette
            .palette
            .visible_commands()
            .find(|command| command.action == Action::Archive)
            .unwrap()
            .shortcut,
        ";"
    );
    app.command_palette.palette.visible = false;
    app.modals.help_open = true;
    app.modals.help_query = "archive".into();
    let backend = ratatui::backend::TestBackend::new(120, 30);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal.draw(|frame| app.draw(frame)).unwrap();
    let text: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(ratatui::buffer::Cell::symbol)
        .collect();
    assert!(
        text.contains(';'),
        "rendered help must show the reader remap, not the default e or list remap w"
    );
}

#[test]
fn configured_actions_retire_unlisted_mail_aliases_without_stealing_navigation() {
    let mut app = remapped_app(
        "list",
        "[mail_list]\n\";\" = \"undo_last_mutation\"\n\"w\" = \"next_message\"\n",
    );
    assert_eq!(press_char(&mut app, ';'), Some(Action::UndoLastMutation));
    assert_eq!(
        press_char(&mut app, 'u'),
        None,
        "the hardcoded undo alias is retired too"
    );
    assert_eq!(
        press_char(&mut app, 'j'),
        Some(Action::MoveDown),
        "next_message is not the list's move_down binding"
    );
}

/// A key and its name in the web app's chord grammar (`lib/keys/chord.ts`).
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

/// A prefix followed by a key that completes nothing is cancelled and says
/// so (blueprint 35): a mistyped `g U` must not mark mail unread, in the
/// mailbox or in a mode.
#[test]
fn a_dead_end_prefix_runs_nothing_and_names_the_keys() {
    for context in ["list", "reader", "todo"] {
        let mut app = app_in(context);
        assert_eq!(press_char(&mut app, 'g'), None);
        let unread = app.handle_key(KeyEvent::new(KeyCode::Char('U'), KeyModifiers::SHIFT));
        assert_eq!(unread, None, "g U ran something in {context}");
        assert!(
            !app.input_pending(),
            "g U left the prefix waiting in {context}"
        );
        assert_eq!(
            app.status_message.as_deref(),
            Some("g U is not a shortcut"),
            "{context}"
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
fn mail_list_remaps_stay_out_of_the_modes() {
    let toml = "[mail_list]\n\";\" = \"archive\"\n";
    let mut app = remapped_app("list", toml);
    assert_eq!(press_char(&mut app, ';'), Some(Action::Archive));
    assert_eq!(press_char(&mut app, 'e'), None, "archive moved to ;");
    let mut app = remapped_app("now", toml);
    assert_eq!(
        press_char(&mut app, 'e'),
        Some(Action::NowDone),
        "retiring the list's e leaves Now's done alone"
    );
    assert_eq!(press_char(&mut app, ';'), None);
}

/// A key that moved in a recent release says where its action went and
/// runs nothing (blueprint 35, D171), without breaking a sequence that
/// starts with a prefix.
#[test]
fn a_moved_key_names_its_new_key_and_runs_nothing() {
    let mut app = app_in("now");
    let shifted = |c| KeyEvent::new(KeyCode::Char(c), KeyModifiers::SHIFT);
    assert_eq!(app.handle_key(shifted('A')), None);
    let notice = app.status_message.clone().unwrap_or_default();
    assert!(notice.ends_with("is now E"), "{notice}");
    assert_eq!(app.handle_key(shifted('E')), Some(Action::NowLetGoDigest));

    let mut app = app_in("now");
    assert_eq!(press_char(&mut app, 'g'), None);
    assert_eq!(
        app.handle_key(shifted('A')),
        Some(Action::OpenAnalyticsScreen),
        "g A is a sequence, not the moved A"
    );

    let mut app = app_in("list");
    assert_eq!(app.handle_key(shifted('K')), None);
    let notice = app.status_message.clone().unwrap_or_default();
    assert!(notice.ends_with("is now P"), "{notice}");
    assert_eq!(
        app.handle_key(shifted('P')),
        Some(Action::OpenSenderMoveMenu)
    );
}

/// The second wave of moves (blueprint 35's `g` table): tools take `g` and
/// a capital, duplicates retire, and each old key names the key to use, in
/// the mailbox panes and in the modes alike.
#[test]
fn retired_tab_digits_and_chords_name_their_new_keys() {
    let shifted = |c| KeyEvent::new(KeyCode::Char(c), KeyModifiers::SHIFT);
    for (context, keys, notice) in [
        ("list", vec![press_of('1')], "Mail is now g i"),
        ("list", vec![press_of('2')], "Search page is now /"),
        ("sidebar", vec![press_of('3')], "Rules is now g R"),
        ("reader", vec![press_of('4')], "Accounts is now g C"),
        ("todo", vec![press_of('5')], "Diagnostics is now g D"),
        ("now", vec![press_of('6')], "Analytics is now g A"),
        ("archive", vec![press_of('7')], "Deliveries is now g P"),
        (
            "list",
            vec![press_of('g'), press_of('c')],
            "Settings is now g C",
        ),
        (
            "updates",
            vec![press_of('g'), press_of('y')],
            "Activity log is now g Y",
        ),
        (
            "list",
            vec![press_of('g'), shifted('E')],
            "Go to Drafts is now g d",
        ),
        ("reader", vec![press_of('y')], "Summarize thread is now B"),
        (
            "list",
            vec![shifted('H')],
            "Top of screen has no key now; find it in the palette",
        ),
        (
            "todo",
            vec![press_of('z'), press_of('z')],
            "Center current has no key now; find it in the palette",
        ),
        (
            "messages",
            vec![KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL)],
            "Toggle assistant is now @",
        ),
    ] {
        let mut app = app_in(context);
        let mut action = None;
        for key in &keys {
            action = app.handle_key(*key);
        }
        assert_eq!(action, None, "{keys:?} in {context} ran something");
        assert!(!app.input_pending(), "{keys:?} in {context} left a prefix");
        assert_eq!(app.status_message.as_deref(), Some(notice), "{context}");
    }

    for (keys, expected) in [
        (vec![press_of('g'), shifted('R')], Action::OpenTab3),
        (vec![press_of('g'), shifted('C')], Action::OpenTab4),
        (vec![press_of('g'), shifted('D')], Action::OpenTab5),
        (vec![press_of('g'), shifted('P')], Action::OpenTab7),
        (
            vec![press_of('g'), shifted('Y')],
            Action::OpenActivityScreen,
        ),
        (vec![shifted(':')], Action::OpenCommandPalette),
        (
            vec![KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL)],
            Action::OpenCommandPalette,
        ),
        (vec![shifted('@')], Action::ToggleAssistant),
    ] {
        for context in ["list", "reader", "todo"] {
            assert_eq!(
                press(context, &keys),
                Ok(Some(expected.clone())),
                "{context} {keys:?}"
            );
        }
    }
}

fn press_of(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

/// Ctrl-k is the palette on the tool screens too, though their lists move
/// with `k` whatever the modifiers; a form's text field keeps it.
#[test]
fn ctrl_k_opens_the_palette_on_a_tool_screen_but_not_in_its_form() {
    let ctrl_k = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL);
    for screen in [
        Screen::Rules,
        Screen::Accounts,
        Screen::Diagnostics,
        Screen::Deliveries,
    ] {
        let mut app = App::new();
        app.screen = screen;
        assert_eq!(
            app.handle_key(ctrl_k),
            Some(Action::OpenCommandPalette),
            "{screen:?}"
        );
    }
    let mut app = App::new();
    app.screen = Screen::Rules;
    app.rules.page.form.visible = true;
    assert_ne!(app.handle_key(ctrl_k), Some(Action::OpenCommandPalette));
}

/// The third wave of moves (blueprint 35's verb table): every old key says
/// where its action went and runs nothing.
#[test]
fn third_wave_keys_name_their_new_keys() {
    let shifted = |c| KeyEvent::new(KeyCode::Char(c), KeyModifiers::SHIFT);
    let palette = "has no key now; find it in the palette";
    for (context, key, notice) in [
        (
            "reader",
            press_of('o'),
            "Open in browser is now O".to_string(),
        ),
        (
            "list",
            shifted('E'),
            format!("Export as Markdown {palette}"),
        ),
        (
            "reader",
            shifted('E'),
            format!("Export as Markdown {palette}"),
        ),
        (
            "sidebar",
            press_of('['),
            "Collapse section is now Enter".into(),
        ),
        (
            "sidebar",
            press_of(']'),
            "Expand section is now Enter".into(),
        ),
        ("sidebar", press_of('n'), "New saved search is now c".into()),
        ("place", press_of('p'), "Pin is now s".into()),
        ("place", shifted('K'), "Move sender to is now P".into()),
        ("place", shifted('S'), format!("Sweep sender {palette}")),
        ("place", shifted('A'), "Sweep all is now E".into()),
        (
            "place",
            press_of('>'),
            format!("More senders in this place {palette}"),
        ),
        (
            "place",
            press_of('+'),
            format!("More from this sender {palette}"),
        ),
        ("archive", shifted('E'), format!("Export {palette}")),
        ("archive", press_of('v'), "Checked is now ,".into()),
        ("todo", press_of('v'), "Details is now ,".into()),
        ("updates", shifted('S'), format!("Sweep sender {palette}")),
    ] {
        let mut app = app_in(context);
        assert_eq!(
            app.handle_key(key),
            None,
            "{key:?} in {context} ran something"
        );
        assert_eq!(
            app.status_message.as_deref(),
            Some(notice.as_str()),
            "{context}"
        );
    }

    for (context, keys, expected) in [
        ("place", vec![press_of('s')], Action::TogglePin),
        ("place", vec![shifted('P')], Action::OpenSenderKindMenu),
        ("place", vec![shifted('E')], Action::SweepPlace),
        (
            "sidebar",
            vec![press_of('c')],
            Action::OpenSavedSearchFormNew,
        ),
        ("reader", vec![shifted('O')], Action::OpenInBrowser),
    ] {
        assert_eq!(
            press(context, &keys),
            Ok(Some(expected.clone())),
            "{context}"
        );
    }
}

/// Enter on a sidebar section's header folds or unfolds it, and `{` / `}`
/// jump between headers (blueprint 35).
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

/// Screens and overlays that still read their own keys say where a moved
/// key went too.
#[test]
fn tool_screen_keys_that_moved_say_so() {
    for (screen, notice) in [
        (Screen::Rules, "New rule is now c"),
        (Screen::Accounts, "Add account is now c"),
    ] {
        let mut app = App::new();
        app.screen = screen;
        app.accounts.page.onboarding_required = false;
        assert_eq!(press_char(&mut app, 'n'), None, "{screen:?}");
        assert_eq!(app.status_message.as_deref(), Some(notice), "{screen:?}");
    }
    let mut app = App::new();
    app.screen = Screen::Rules;
    assert_eq!(press_char(&mut app, 'c'), Some(Action::OpenRuleFormNew));
    let mut app = App::new();
    app.screen = Screen::Accounts;
    assert_eq!(press_char(&mut app, 'c'), Some(Action::OpenAccountFormNew));

    let mut app = app_in("list");
    app.mailbox.mailbox_view = MailboxView::CalendarInvites;
    for (key, notice) in [
        ('a', "Accept invite is now i a"),
        ('t', "Maybe (invite) is now i m"),
        ('d', "Decline invite is now i d"),
    ] {
        assert_eq!(press_char(&mut app, key), None);
        assert_eq!(app.status_message.as_deref(), Some(notice));
    }
}

/// D171: an action with a key run from the palette names its key, once per
/// action and profile. The daemon keeps the seen state with the mode hints;
/// a second run in this session asks it nothing.
#[test]
fn the_palette_names_a_keyed_actions_key_next_time() {
    let mut app = app_in("todo");
    app.command_palette.palette.visible = true;
    app.apply(Action::OpenCommandPalette);
    app.command_palette
        .palette
        .toggle_with_archive_operations(app.current_ui_context(), &[]);
    for c in "catch-up".chars() {
        app.command_palette.palette.on_char(c);
    }
    let pressed = app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(pressed, Some(Action::TodoOpenCatchup));
    let asked: Vec<_> = app
        .pending_mutation_queue
        .iter()
        .filter_map(|queued| match (&queued.request, &queued.effect) {
            (Request::SetHintSeen { hint, seen: true }, MutationEffect::NextTime(text)) => {
                Some((hint.clone(), text.clone()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        asked,
        vec![(
            "next-time.todo.catchup".to_string(),
            "Catch-up. Next time: 2".to_string()
        )]
    );

    let catchup = app
        .command_palette
        .palette
        .commands
        .iter()
        .find(|command| command.id == Some("todo.catchup"))
        .cloned()
        .unwrap();
    let before = app.pending_mutation_queue.len();
    app.offer_next_time(&catchup);
    assert_eq!(app.pending_mutation_queue.len(), before, "asked once");

    // A palette-only action has no key to teach.
    let config = app
        .command_palette
        .palette
        .commands
        .iter()
        .find(|command| command.id == Some("tui.edit-config"))
        .cloned()
        .unwrap();
    app.offer_next_time(&config);
    assert_eq!(app.pending_mutation_queue.len(), before);

    app.apply_mutation_completion(
        MutationEffect::NextTime("Catch-up. Next time: 2".into()),
        false,
    );
    assert!(app
        .toasts
        .visible(std::time::Instant::now())
        .iter()
        .any(|toast| toast.text == "Catch-up. Next time: 2"));
}

#[test]
fn action_keys_from_keys_toml_apply_in_the_mail_panes_and_the_modes() {
    let toml = "[actions]\n\"mail.archive\" = \"%\"\n\"todo.done\" = \"%\"\n";
    for context in ["list", "reader"] {
        let mut app = remapped_app(context, toml);
        app.mailbox.viewed_thread_messages = make_test_envelopes(1);
        assert_eq!(
            press_char(&mut app, '%'),
            Some(Action::Archive),
            "{context}"
        );
        assert_eq!(press_char(&mut app, 'e'), None, "{context}: e is retired");
    }
    let mut app = remapped_app("todo", toml);
    assert_eq!(press_char(&mut app, '%'), Some(Action::TodoDone));
    assert_eq!(press_char(&mut app, 'e'), None);
    // An action left to the palette loses its keys.
    let mut app = remapped_app("list", "[actions]\n\"mail.star\" = []\n");
    assert_eq!(press_char(&mut app, 's'), None);
    assert_eq!(press_char(&mut app, 'e'), Some(Action::Archive));
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
fn a_moved_key_names_the_key_the_person_gave_its_action() {
    let keymap = mxr_keymap::defaults();
    let (place, key, target) = keymap
        .actions
        .iter()
        .filter(|action| {
            action
                .tui
                .iter()
                .any(|binding| binding.contexts.includes("todo") && !binding.keys.is_empty())
        })
        .find_map(|action| {
            let moved = action.moved.iter().find(|moved| {
                moved.to.is_none()
                    && moved.to_key.is_none()
                    && moved.contexts.iter().any(|c| c == "todo")
            })?;
            Some(("todo", moved.key.clone(), action.id.clone()))
        })
        .expect("a key moved within To do");
    let mut app = remapped_app(place, &format!("[actions]\n\"{target}\" = \"%\"\n"));
    press_char(&mut app, key.chars().next().unwrap());
    if let Some(second) = key.split(' ').nth(1).and_then(|k| k.chars().next()) {
        press_char(&mut app, second);
    }
    let status = app.status_message.clone().unwrap_or_default();
    assert!(status.ends_with("is now %"), "{key} in {place}: {status}");
}

#[test]
fn with_single_keys_off_a_tool_screen_ignores_its_letters() {
    let mut app = remapped_app("list", "single_keys = false\n");
    app.screen = Screen::Rules;
    assert_eq!(press_char(&mut app, 'c'), None);
    assert!(!app.rules.page.form.visible, "c no longer opens a new rule");
    assert_eq!(
        app.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL)),
        Some(Action::OpenCommandPalette)
    );
}
