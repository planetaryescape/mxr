//! Pattern select (`*a`, `*u`, ...), counted visual ranges (`V`, `3j`) and
//! the bulk confirm preview. Keys go through `handle_key` and then `apply`,
//! as the runner does.

use super::*;
use crate::action::PatternKind;
use crate::app::bulk_preview::request_message_ids;
use crate::app::SearchPane;
use crate::keybindings::{KeyPress, ViewContext};
use crate::theme::Theme;
use std::collections::HashSet;

fn press(app: &mut App, c: char) {
    let modifiers = if c.is_ascii_uppercase() {
        KeyModifiers::SHIFT
    } else {
        KeyModifiers::NONE
    };
    press_key(app, KeyCode::Char(c), modifiers);
}

fn press_key(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    if let Some(action) = app.handle_key(KeyEvent::new(code, modifiers)) {
        app.apply(action);
    }
}

fn mail_list_app(count: usize) -> App {
    let mut app = App::new();
    app.screen = Screen::Mailbox;
    app.mailbox.mailbox_view = MailboxView::Messages;
    app.mailbox.active_pane = ActivePane::MailList;
    app.mailbox.envelopes = make_test_envelopes(count);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app
}

fn ids_of(envelopes: &[Envelope], indices: &[usize]) -> HashSet<MessageId> {
    indices.iter().map(|&i| envelopes[i].id.clone()).collect()
}

fn selected_ids(app: &App) -> HashSet<MessageId> {
    app.mailbox.selected_set.clone()
}

/// Ids in the order the list shows them, the order a preview must use.
fn list_order(envelopes: &[Envelope], wanted: &HashSet<MessageId>) -> Vec<MessageId> {
    envelopes
        .iter()
        .filter(|env| wanted.contains(&env.id))
        .map(|env| env.id.clone())
        .collect()
}

fn pending_preview_ids(app: &App) -> Vec<MessageId> {
    app.modals
        .pending_bulk_confirm
        .as_ref()
        .expect("bulk confirm is open")
        .preview
        .rows
        .iter()
        .map(|row| row.message_id.clone())
        .collect()
}

// Flags from make_test_envelopes: even indices are read, odd are unread.

#[test]
fn pattern_keys_select_by_read_state_and_clear() {
    let mut app = mail_list_app(4);
    let envelopes = app.mailbox.envelopes.clone();

    press(&mut app, '*');
    press(&mut app, 'u');
    assert_eq!(selected_ids(&app), ids_of(&envelopes, &[1, 3]));
    assert_eq!(app.status_message.as_deref(), Some("2 selected"));

    press(&mut app, '*');
    press(&mut app, 'r');
    assert_eq!(selected_ids(&app), ids_of(&envelopes, &[0, 2]));

    press(&mut app, '*');
    press(&mut app, 'a');
    assert_eq!(selected_ids(&app).len(), 4);

    press(&mut app, '*');
    press(&mut app, 'n');
    assert!(selected_ids(&app).is_empty());
    assert!(!app.mailbox.visual_mode);
}

#[test]
fn pattern_star_selects_starred_only() {
    let mut app = mail_list_app(4);
    let envelopes = app.mailbox.envelopes.clone();
    app.mailbox.envelopes[2].flags.insert(MessageFlags::STARRED);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();

    press(&mut app, '*');
    press(&mut app, 's');
    assert_eq!(selected_ids(&app), ids_of(&envelopes, &[2]));
}

#[test]
fn pattern_thread_selects_the_whole_thread_in_threads_mode() {
    let mut app = mail_list_app(3);
    let envelopes = app.mailbox.envelopes.clone();
    app.mailbox.envelopes[1].thread_id = app.mailbox.envelopes[0].thread_id.clone();
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    app.mailbox.mail_list_mode = MailListMode::Threads;

    press(&mut app, '*');
    press(&mut app, 't');
    assert_eq!(selected_ids(&app), ids_of(&envelopes, &[0, 1]));
}

#[test]
fn pattern_keys_select_from_search_results_on_the_search_screen() {
    let mut app = App::new();
    app.screen = Screen::Search;
    app.search.page.editing = false;
    app.search.page.active_pane = SearchPane::Results;
    app.search.page.results = make_test_envelopes(3);
    let results = app.search.page.results.clone();

    press(&mut app, '*');
    press(&mut app, 'a');
    assert_eq!(selected_ids(&app), ids_of(&results, &[0, 1, 2]));
}

#[test]
fn pattern_keys_reach_the_list_from_the_sidebar_and_the_reader_beside_it() {
    for pane in [ActivePane::Sidebar, ActivePane::MessageView] {
        let mut app = mail_list_app(4);
        let envelopes = app.mailbox.envelopes.clone();
        app.mailbox.active_pane = pane;
        press(&mut app, '*');
        press(&mut app, 'u');
        assert_eq!(selected_ids(&app), ids_of(&envelopes, &[1, 3]), "{pane:?}");
    }
}

#[test]
fn escape_in_the_sidebar_clears_the_list_beside_it() {
    let mut app = mail_list_app(4);
    app.mailbox.active_pane = ActivePane::Sidebar;
    press(&mut app, '*');
    press(&mut app, 'a');
    assert_eq!(selected_ids(&app).len(), 4);
    press_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(selected_ids(&app).is_empty());
}

#[test]
fn pattern_keys_are_not_keys_in_a_mode() {
    let mut app = mail_list_app(3);
    app.mailbox.mailbox_view = MailboxView::Todo;
    press(&mut app, '*');
    press(&mut app, 'a');
    assert!(selected_ids(&app).is_empty());
}

#[test]
fn pattern_select_ends_an_open_visual_range() {
    let mut app = mail_list_app(5);
    press(&mut app, 'V');
    press(&mut app, 'j');
    assert!(app.mailbox.visual_mode);

    press(&mut app, '*');
    press(&mut app, 'u');
    assert!(!app.mailbox.visual_mode);

    // With the range still open, this j would have rewritten the pattern.
    press(&mut app, 'j');
    assert_eq!(selected_ids(&app).len(), 2);
}

#[test]
fn visual_count_moves_the_counted_rows() {
    let mut app = mail_list_app(20);
    press(&mut app, 'V');
    press(&mut app, '1');
    press(&mut app, '0');
    assert_eq!(
        app.status_message.as_deref(),
        Some("-- VISUAL LINE -- 10"),
        "pending count shows in the status line"
    );
    press(&mut app, 'j');

    assert_eq!(app.mailbox.selected_index, 10);
    assert_eq!(selected_ids(&app).len(), 11);
    assert_eq!(app.mailbox.visual_count, None);
}

#[test]
fn visual_three_j_selects_four_rows() {
    let mut app = mail_list_app(10);
    press(&mut app, 'V');
    press(&mut app, '3');
    press(&mut app, 'j');
    assert_eq!(selected_ids(&app).len(), 4);
}

#[test]
fn visual_count_larger_than_the_list_stops_at_the_last_row() {
    let mut app = mail_list_app(5);
    press(&mut app, 'V');
    for c in ['9', '9', '9'] {
        press(&mut app, c);
    }
    press(&mut app, 'j');

    assert_eq!(app.mailbox.selected_index, 4);
    assert_eq!(selected_ids(&app).len(), 5);
}

#[test]
fn visual_count_is_dropped_by_escape_and_by_other_keys() {
    let mut app = mail_list_app(10);
    press(&mut app, 'V');
    press(&mut app, '5');
    press_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.mailbox.visual_mode, "Esc only drops the count");
    assert_eq!(app.mailbox.visual_count, None);
    assert_eq!(app.mailbox.selected_index, 0);

    // A count + G is not rejected: the G runs and the count is gone.
    press(&mut app, '5');
    press(&mut app, 'G');
    assert_eq!(app.mailbox.visual_count, None);
    assert_eq!(app.mailbox.selected_index, 9);
}

#[test]
fn digits_outside_visual_mode_are_not_counts() {
    let mut app = mail_list_app(3);
    assert_eq!(
        app.handle_key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE)),
        None
    );
    assert_eq!(app.mailbox.visual_count, None);

    press(&mut app, 'V');
    press(&mut app, 'V');
    assert!(!app.mailbox.visual_mode);
    assert_eq!(
        app.handle_key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE)),
        None,
    );
    assert_eq!(
        app.mailbox.visual_count, None,
        "after visual mode is left, 1 is no count"
    );
}

#[test]
fn selection_survives_plain_navigation() {
    let mut app = mail_list_app(4);
    press(&mut app, '*');
    press(&mut app, 'a');
    press_key(&mut app, KeyCode::Down, KeyModifiers::NONE);
    assert_eq!(selected_ids(&app).len(), 4);
}

#[test]
fn switching_label_drops_the_selection_and_any_visual_range() {
    let mut app = mail_list_app(4);
    press(&mut app, '*');
    press(&mut app, 'a');
    press(&mut app, 'V');
    press(&mut app, '3');

    app.apply(Action::SelectLabel(mxr_core::LabelId::new()));

    assert!(selected_ids(&app).is_empty());
    assert!(!app.mailbox.visual_mode);
    assert_eq!(app.mailbox.visual_count, None);
    assert_eq!(app.status_message.as_deref(), Some("Selection cleared"));
}

#[test]
fn opening_search_drops_the_selection() {
    let mut app = mail_list_app(4);
    press(&mut app, '*');
    press(&mut app, 'a');

    app.apply(Action::OpenSearchScreen);

    assert!(selected_ids(&app).is_empty());
}

#[test]
fn changing_the_mailbox_filter_drops_the_selection() {
    let mut app = mail_list_app(4);
    press(&mut app, '*');
    press(&mut app, 'a');

    // Submitting a mailbox filter narrows the list and bumps the filter session.
    app.apply(Action::OpenMailboxFilter);
    app.search.bar.query = "user".into();
    app.apply(Action::SubmitSearch);

    assert!(selected_ids(&app).is_empty());
}

#[test]
fn preview_strips_terminal_controls_from_mail_text() {
    let mut app = mail_list_app(2);
    app.mailbox.envelopes[0].subject = "Hi \x1b[2J\x07there".into();
    app.mailbox.envelopes[0].from.name = Some("Eve\x1b]0;x\x07".into());
    let request = Request::mutation(MutationCommand::Trash {
        message_ids: vec![app.mailbox.envelopes[0].id.clone()],
    });

    let row = &app.bulk_preview(&request).rows[0];

    assert_eq!(row.subject, "Hi [2Jthere");
    assert_eq!(row.sender, "Eve]0;x");
}

#[test]
fn archive_preview_lists_the_selected_messages_in_list_order() {
    let mut app = mail_list_app(12);
    let envelopes = app.mailbox.envelopes.clone();
    press(&mut app, '*');
    press(&mut app, 'u');
    let wanted = selected_ids(&app);

    app.apply(Action::Archive);

    assert!(
        app.pending_mutation_queue.is_empty(),
        "nothing queued before confirm"
    );
    let preview = app
        .modals
        .pending_bulk_confirm
        .as_ref()
        .expect("archive of several messages asks first")
        .preview
        .clone();
    assert_eq!(preview.not_in_view, 0);
    assert_eq!(pending_preview_ids(&app), list_order(&envelopes, &wanted));
    assert_eq!(preview.rows[0].sender, "User 1");
    assert_eq!(preview.rows[0].subject, "Subject 1");
}

#[test]
fn escape_on_the_preview_archives_nothing() {
    let mut app = mail_list_app(3);
    press(&mut app, '*');
    press(&mut app, 'a');
    app.apply(Action::Archive);

    press_key(&mut app, KeyCode::Esc, KeyModifiers::NONE);

    assert!(app.modals.pending_bulk_confirm.is_none());
    assert!(app.pending_mutation_queue.is_empty());
    assert_eq!(app.status_message.as_deref(), Some("Bulk action cancelled"));
}

#[test]
fn confirm_queues_exactly_the_previewed_ids() {
    let mut app = mail_list_app(6);
    press(&mut app, '*');
    press(&mut app, 'u');
    app.apply(Action::Archive);
    let previewed = pending_preview_ids(&app);

    press_key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!(app.pending_mutation_queue.len(), 1);
    let queued = app.pending_mutation_queue[0].request.clone();
    let Request::Mutation {
        mutation: MutationCommand::Archive { message_ids },
        ..
    } = queued
    else {
        panic!("expected an archive request");
    };
    // The request carries the selection set in its own order; what must match
    // the preview is the set of messages.
    assert_eq!(message_ids.len(), previewed.len());
    assert_eq!(
        message_ids.into_iter().collect::<HashSet<_>>(),
        previewed.into_iter().collect::<HashSet<_>>()
    );
}

#[test]
fn preview_matches_the_request_for_every_bulk_command() {
    let app = mail_list_app(5);
    let envelopes = app.mailbox.envelopes.clone();
    let wanted = ids_of(&envelopes, &[0, 2, 4]);
    let ids: Vec<MessageId> = list_order(&envelopes, &wanted);
    let commands = vec![
        MutationCommand::Archive {
            message_ids: ids.clone(),
        },
        MutationCommand::ReadAndArchive {
            message_ids: ids.clone(),
        },
        MutationCommand::Trash {
            message_ids: ids.clone(),
        },
        MutationCommand::Spam {
            message_ids: ids.clone(),
        },
        MutationCommand::Star {
            message_ids: ids.clone(),
            starred: true,
        },
        MutationCommand::SetRead {
            message_ids: ids.clone(),
            read: true,
        },
        MutationCommand::ModifyLabels {
            message_ids: ids.clone(),
            add: vec!["STARRED".into()],
            remove: vec![],
        },
        MutationCommand::Move {
            message_ids: ids.clone(),
            target_label: "INBOX".into(),
        },
        MutationCommand::Route {
            message_ids: ids.clone(),
            to_label: "Later".into(),
            from_queue_label: "Queue".into(),
            archive: true,
            dry_run: false,
        },
    ];
    for command in commands {
        let request = Request::mutation(command);
        assert_eq!(request_message_ids(&request), ids);
        let preview = app.bulk_preview(&request);
        assert_eq!(
            preview
                .rows
                .iter()
                .map(|row| row.message_id.clone())
                .collect::<Vec<_>>(),
            ids,
            "{request:?}"
        );
        assert_eq!(preview.not_in_view, 0);
    }
}

#[test]
fn preview_reports_ids_the_list_does_not_show() {
    let app = mail_list_app(3);
    let envelopes = app.mailbox.envelopes.clone();
    let stranger = MessageId::new();
    let request = Request::mutation(MutationCommand::Trash {
        message_ids: vec![envelopes[1].id.clone(), stranger],
    });

    let preview = app.bulk_preview(&request);

    assert_eq!(preview.rows.len(), 1);
    assert_eq!(preview.rows[0].message_id, envelopes[1].id);
    assert_eq!(preview.not_in_view, 1);
}

#[test]
fn preview_in_threads_mode_and_on_the_search_screen_matches_the_request() {
    // Threads mode: a visual range over two rows expands to every member.
    let mut threaded = mail_list_app(3);
    let envelopes = threaded.mailbox.envelopes.clone();
    threaded.mailbox.envelopes[1].thread_id = threaded.mailbox.envelopes[0].thread_id.clone();
    threaded.mailbox.all_envelopes = threaded.mailbox.envelopes.clone();
    threaded.mailbox.mail_list_mode = MailListMode::Threads;
    press(&mut threaded, 'V');
    press(&mut threaded, 'j');
    let wanted = selected_ids(&threaded);
    assert_eq!(wanted, ids_of(&envelopes, &[0, 1, 2]));

    threaded.apply(Action::Archive);
    assert_eq!(pending_preview_ids(&threaded).len(), 3);
    let requested: HashSet<MessageId> = {
        let pending = threaded.modals.pending_bulk_confirm.as_ref().unwrap();
        request_message_ids(&pending.request).into_iter().collect()
    };
    assert_eq!(requested, wanted);

    // Search screen: the preview reads the results, not the mailbox.
    let mut search = App::new();
    search.screen = Screen::Search;
    search.search.page.editing = false;
    search.search.page.active_pane = SearchPane::Results;
    search.search.page.results = make_test_envelopes(3);
    search.mailbox.envelopes = make_test_envelopes(7);
    let results = search.search.page.results.clone();
    press(&mut search, '*');
    press(&mut search, 'a');
    search.apply(Action::Archive);
    assert_eq!(
        pending_preview_ids(&search),
        results.iter().map(|env| env.id.clone()).collect::<Vec<_>>()
    );
}

#[test]
fn bulk_preview_scroll_moves_with_j_and_k_and_stops_at_zero() {
    let mut app = mail_list_app(3);
    press(&mut app, '*');
    press(&mut app, 'a');
    app.apply(Action::Archive);

    press(&mut app, 'j');
    press(&mut app, 'j');
    assert_eq!(
        app.modals
            .pending_bulk_confirm
            .as_ref()
            .unwrap()
            .preview_scroll,
        2
    );

    press(&mut app, 'k');
    press(&mut app, 'k');
    press(&mut app, 'k');
    assert_eq!(
        app.modals
            .pending_bulk_confirm
            .as_ref()
            .unwrap()
            .preview_scroll,
        0
    );
    assert!(
        app.modals.pending_bulk_confirm.is_some(),
        "j/k do not confirm or cancel"
    );
}

#[test]
fn preview_modal_lists_rows_then_says_how_many_are_left() {
    let mut app = mail_list_app(12);
    press(&mut app, '*');
    press(&mut app, 'a');
    app.apply(Action::Archive);
    let pending = app
        .modals
        .pending_bulk_confirm
        .clone()
        .expect("confirm open");

    let rendered = render_to_string(90, 16, |frame| {
        crate::ui::bulk_confirm_modal::draw(frame, frame.area(), Some(&pending), &Theme::default());
    });

    assert!(rendered.contains("User 0 — Subject 0"), "{rendered}");
    assert!(rendered.contains("more (j/k to scroll)"), "{rendered}");
    assert!(rendered.contains("[Esc/n/q] cancel"), "{rendered}");
}

#[test]
fn default_star_chords_match_the_key_table() {
    // The table feeds keys.toml and the help text; the handler does the work.
    // They must name the same action, or a remap would not match its display.
    let table = crate::keybindings::default_keybindings();
    for letter in ['a', 'n', 'r', 'u', 's', 't'] {
        let sequence = [
            KeyPress::from_event(KeyEvent::new(KeyCode::Char('*'), KeyModifiers::NONE)),
            KeyPress::from_event(KeyEvent::new(KeyCode::Char(letter), KeyModifiers::NONE)),
        ];
        let name = crate::keybindings::resolve_action(&table, ViewContext::MailList, &sequence)
            .unwrap_or_else(|| panic!("*{letter} missing from the key table"));
        let mut input = crate::input::InputHandler::new();
        input.handle_key(KeyEvent::new(KeyCode::Char('*'), KeyModifiers::NONE));
        let typed = input.handle_key(KeyEvent::new(KeyCode::Char(letter), KeyModifiers::NONE));
        assert_eq!(
            typed,
            crate::keybindings::action_from_name(&name),
            "*{letter}"
        );
        assert!(matches!(typed, Some(Action::PatternSelect(_))), "*{letter}");
    }
}

#[test]
fn pattern_kinds_are_the_six_keys() {
    let kinds = [
        PatternKind::All,
        PatternKind::None,
        PatternKind::Read,
        PatternKind::Unread,
        PatternKind::Starred,
        PatternKind::Thread,
    ];
    for kind in kinds {
        let mut app = mail_list_app(2);
        app.apply(Action::PatternSelect(kind.clone()));
        assert!(app
            .status_message
            .as_deref()
            .unwrap_or("")
            .ends_with("selected"));
    }
}
