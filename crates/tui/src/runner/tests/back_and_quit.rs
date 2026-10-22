//! `q` closes the innermost layer and quits only at the root (blueprint
//! 35, "Back and quit"). Below the root it does what Esc does there.

use super::*;
use crate::app::{MessagesFocus, TodoPanel};
use crate::ui::records_lens::tests::lisbon_answer;

fn mailbox_app() -> App {
    let mut app = App::new();
    app.screen = Screen::Mailbox;
    app.mailbox.mailbox_view = MailboxView::Messages;
    app.mailbox.active_pane = ActivePane::MailList;
    app
}

fn q(app: &mut App) -> Option<Action> {
    app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE))
}

#[test]
fn q_quits_only_at_the_mailbox_root() {
    let mut app = mailbox_app();
    assert_eq!(q(&mut app), Some(Action::QuitView));

    // First-run setup has nothing under it, so q still leaves.
    let mut app = mailbox_app();
    app.screen = Screen::Accounts;
    app.enter_account_setup_onboarding();
    assert_eq!(q(&mut app), Some(Action::QuitView));
    app.accounts.page.onboarding_modal_open = false;
    assert_eq!(q(&mut app), Some(Action::QuitView));
}

fn reader() -> App {
    let mut app = mailbox_app();
    app.mailbox.active_pane = ActivePane::MessageView;
    app
}

fn rules_screen() -> App {
    let mut app = mailbox_app();
    app.screen = Screen::Rules;
    app
}

fn todo_panel(panel: TodoPanel) -> App {
    let mut app = mailbox_app();
    app.mailbox.mailbox_view = MailboxView::Todo;
    app.mailbox.todo_page.panel = panel;
    app
}

fn person_view() -> App {
    let mut app = mailbox_app();
    app.mailbox.mailbox_view = MailboxView::People;
    app.mailbox.messages_page.focus = MessagesFocus::Person;
    app
}

fn later_shelf() -> App {
    let mut app = mailbox_app();
    app.mailbox.mailbox_view = MailboxView::Reading;
    app.mailbox.reading_page.later_shelf = true;
    app
}

fn archive_answer() -> App {
    let mut app = mailbox_app();
    app.mailbox.mailbox_view = MailboxView::ArchiveMode;
    app.set_records_answer(lisbon_answer());
    app
}

type Setup = fn() -> App;

#[test]
fn q_goes_back_one_level_below_the_root() {
    let layers: [(&str, Setup); 7] = [
        ("reader", reader),
        ("rules screen", rules_screen),
        ("catch-up", || todo_panel(TodoPanel::Catchup)),
        ("expired", || todo_panel(TodoPanel::Expired)),
        ("person view", person_view),
        ("later shelf", later_shelf),
        ("archive answer", archive_answer),
    ];
    for (name, layer) in layers {
        let esc = layer().handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        let action = q(&mut layer());
        assert_ne!(action, Some(Action::QuitView), "q quit from the {name}");
        assert!(action.is_some(), "q did nothing in the {name}");
        assert_eq!(action, esc, "q in the {name} is not its Esc");
    }
}

#[test]
fn q_in_the_reader_closes_it_and_keeps_the_app_open() {
    let mut app = reader();
    app.mailbox.layout_mode = LayoutMode::ThreePane;
    let action = q(&mut app).expect("q acts in the reader");
    app.apply(action);
    assert!(!app.should_quit);
    assert_ne!(app.mailbox.active_pane, ActivePane::MessageView);
}

#[test]
fn q_still_closes_a_modal() {
    let mut app = reader();
    app.modals.help_open = true;
    assert_eq!(q(&mut app), Some(Action::Help));
}
