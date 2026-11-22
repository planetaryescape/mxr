use super::*;
use crate::app::bulk_preview::request_message_ids;
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
