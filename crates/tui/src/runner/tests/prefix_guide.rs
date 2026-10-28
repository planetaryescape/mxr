//! The prefix guide (blueprint 35, K4): a pause after a prefix lists the
//! keys that can follow, from the key definition, and typing faster than
//! that never shows it.

use super::*;
use crate::input::PREFIX_GUIDE_DELAY;

fn screen(app: &mut App) -> String {
    let backend = ratatui::backend::TestBackend::new(140, 40);
    let mut terminal = ratatui::Terminal::new(backend).unwrap();
    terminal.draw(|frame| app.draw(frame)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn press(app: &mut App, c: char) -> Option<Action> {
    let modifiers = if c.is_ascii_uppercase() {
        KeyModifiers::SHIFT
    } else {
        KeyModifiers::NONE
    };
    app.handle_key(KeyEvent::new(KeyCode::Char(c), modifiers))
}

fn mailbox(view: MailboxView) -> App {
    let mut app = App::new();
    app.screen = Screen::Mailbox;
    app.mailbox.mailbox_view = view;
    app.mailbox.active_pane = ActivePane::MailList;
    app
}

/// The guide's row for `key`, if it lists one: the key, then its label.
fn lists(shown: &str, key: &str, label: &str) -> bool {
    shown
        .lines()
        .any(|line| line.contains(&format!("{key}  {label}")))
}

#[test]
fn holding_g_for_300ms_lists_the_places() {
    for view in [MailboxView::Messages, MailboxView::Now, MailboxView::Todo] {
        let mut app = mailbox(view);
        assert_eq!(press(&mut app, 'g'), None);
        assert!(
            !screen(&mut app).contains("g then…"),
            "{view:?}: the guide waits"
        );

        app.age_prefix_for_tests(PREFIX_GUIDE_DELAY);
        let shown = screen(&mut app);
        assert!(shown.contains("g then…"), "{view:?}:\n{shown}");
        for (key, label) in [
            ("i", "Go to Inbox"),
            ("h", "Go to Now"),
            ("x", "To do"),
            ("R", "Rules"),
        ] {
            assert!(
                lists(&shown, key, label),
                "{view:?} lacks g {key}:\n{shown}"
            );
        }
        // Still waiting: the guide teaches, it doesn't cancel.
        assert_eq!(press(&mut app, 'i'), Some(Action::GoToInbox), "{view:?}");
        assert!(!screen(&mut app).contains("g then…"), "{view:?}");
    }
}

#[test]
fn the_selection_and_invitation_prefixes_list_theirs() {
    let mut app = mailbox(MailboxView::Messages);
    press(&mut app, '*');
    app.age_prefix_for_tests(PREFIX_GUIDE_DELAY);
    let shown = screen(&mut app);
    assert!(shown.contains("* then…"), "{shown}");
    assert!(lists(&shown, "u", "Select unread"), "{shown}");
}

#[test]
fn a_fast_g_i_shows_nothing() {
    let mut app = mailbox(MailboxView::Messages);
    press(&mut app, 'g');
    app.age_prefix_for_tests(PREFIX_GUIDE_DELAY - std::time::Duration::from_millis(50));
    assert!(!screen(&mut app).contains("g then…"));
    assert_eq!(press(&mut app, 'i'), Some(Action::GoToInbox));
    assert!(!screen(&mut app).contains("g then…"));
}

#[test]
fn esc_cancels_the_prefix_and_its_guide() {
    let mut app = mailbox(MailboxView::Messages);
    press(&mut app, 'g');
    app.age_prefix_for_tests(PREFIX_GUIDE_DELAY);
    assert!(screen(&mut app).contains("g then…"));
    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(!app.input_pending());
    assert!(!screen(&mut app).contains("g then…"));
    assert_eq!(app.status_message, None, "Esc cancels quietly");
}

#[test]
fn the_guide_goes_when_the_prefix_times_out() {
    let mut app = mailbox(MailboxView::Messages);
    press(&mut app, 'g');
    app.age_prefix_for_tests(crate::input::MULTI_KEY_TIMEOUT);
    app.age_prefix_for_tests(std::time::Duration::from_millis(1));
    app.tick();
    assert!(!app.input_pending());
    assert!(!screen(&mut app).contains("g then…"));
}

#[test]
fn the_guide_shows_the_persons_keys() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("keys.toml"),
        "[actions]\n\"nav.starred\" = \"g y\"\n",
    )
    .unwrap();
    let mut app = mailbox(MailboxView::Messages);
    app.load_keybindings_from(dir.path());
    press(&mut app, 'g');
    app.age_prefix_for_tests(PREFIX_GUIDE_DELAY);
    let shown = screen(&mut app);
    assert!(lists(&shown, "y", "Go to Starred"), "{shown}");
    assert!(!lists(&shown, "s", "Go to Starred"), "{shown}");
}

#[test]
fn with_single_keys_off_no_prefix_waits() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("keys.toml"), "single_keys = false\n").unwrap();
    let mut app = mailbox(MailboxView::Messages);
    app.load_keybindings_from(dir.path());
    press(&mut app, 'g');
    app.age_prefix_for_tests(PREFIX_GUIDE_DELAY);
    assert!(!screen(&mut app).contains("g then…"));
}

#[test]
fn z_undoes_now_that_z_z_retired() {
    let mut app = mailbox(MailboxView::Messages);
    assert_eq!(press(&mut app, 'z'), Some(Action::UndoLastMutation));
    assert!(!app.input_pending());
}

#[test]
fn g_still_waits_four_seconds() {
    let mut app = mailbox(MailboxView::Messages);
    press(&mut app, 'g');
    app.age_prefix_for_tests(std::time::Duration::from_millis(3990));
    app.tick();
    assert!(app.input_pending(), "g waits while its guide can be read");
    assert_eq!(press(&mut app, 'i'), Some(Action::GoToInbox));
    press(&mut app, 'g');
    app.age_prefix_for_tests(crate::input::MULTI_KEY_TIMEOUT + std::time::Duration::from_millis(1));
    app.tick();
    assert!(!app.input_pending());
}
