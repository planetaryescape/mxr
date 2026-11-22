use super::*;
use crate::app::{PendingUndo, UndoAction};
use mxr_protocol::{DeskDoneItemData, DeskLaneData, DeskLaneKind, DeskRowData, PeopleSplitData};

fn press(app: &mut App, code: KeyCode) {
    let modifiers = match code {
        KeyCode::Char(c) if c.is_ascii_uppercase() => KeyModifiers::SHIFT,
        _ => KeyModifiers::NONE,
    };
    if let Some(action) = app.handle_key(KeyEvent::new(code, modifiers)) {
        app.apply(action);
    }
}

fn g_capital_f(app: &mut App) {
    press(app, KeyCode::Char('g'));
    press(app, KeyCode::Char('F'));
}

fn queued(app: &App) -> Vec<Request> {
    app.pending_mutation_queue
        .iter()
        .map(|queued| queued.request.clone())
        .collect()
}

fn owed_row(name: &str) -> DeskRowData {
    DeskRowData {
        lane: DeskLaneKind::Owed,
        account_id: AccountId::new(),
        thread_id: ThreadId::new(),
        message_id: MessageId::new(),
        message_ids: vec![],
        counterparty_email: format!("{}@example.com", name.to_lowercase()),
        counterparty_name: Some(name.to_string()),
        subject: format!("From {name}"),
        reason: "replied to your message".into(),
        since: chrono::Utc::now(),
        age_seconds: 2 * 86_400,
        usual_seconds: Some(4 * 3_600),
        usual_samples: 3,
        overdue: true,
        unread: true,
        starred: false,
        commitment_id: None,
        back_at: None,
    }
}

fn focus_app() -> (App, Vec<DeskRowData>, Vec<Envelope>) {
    let mut app = App::new();
    app.apply(Action::OpenNow);
    g_capital_f(&mut app);
    assert_eq!(app.mailbox.mailbox_view, MailboxView::Focus);
    assert!(
        app.mailbox.focus_page.pending_refresh,
        "opening it fetches both sources"
    );
    let owed = vec![owed_row("Nora"), owed_row("Sam")];
    let mut later = make_test_envelopes(2);
    later[1].thread_id = owed[1].thread_id.clone();
    app.set_focus_sources(
        Ok((
            DeskLaneData {
                rows: owed.clone(),
                total: 2,
            },
            PeopleSplitData {
                write_with: 2,
                others: 1,
            },
        )),
        Ok(later.clone()),
    );
    (app, owed, later)
}

fn current_thread(app: &App) -> Option<ThreadId> {
    app.mailbox
        .focus_page
        .current()
        .map(|item| item.thread_id.clone())
}

#[test]
fn g_capital_f_opens_the_webs_queue_in_its_order() {
    assert!(matches!(
        crate::app::focus_desk_request(),
        Request::GetDesk {
            account_id: None,
            lane_limit: 5000
        }
    ));
    let (app, owed, later) = focus_app();
    let page = &app.mailbox.focus_page;
    assert_eq!(
        page.queue,
        vec![
            owed[0].thread_id.clone(),
            owed[1].thread_id.clone(),
            later[0].thread_id.clone()
        ],
        "You owe first, then reply later, each conversation once"
    );
    let first = page.current().unwrap();
    assert_eq!(first.sender, "Nora");
    assert!(first
        .reason
        .starts_with("Replied to your message \u{b7} 2d"));
    assert_eq!(page.others, 1);
    assert!(app
        .mailbox
        .queued_body_fetches
        .contains(&owed[0].message_id));
    assert_eq!(app.mode_key_context(), Some("focus"));
}

#[test]
fn j_skips_to_the_end_and_e_puts_it_away_with_undo() {
    let (mut app, owed, later) = focus_app();
    press(&mut app, KeyCode::Char('j'));
    assert_eq!(current_thread(&app), Some(owed[1].thread_id.clone()));
    assert_eq!(
        app.mailbox.focus_page.queue.last(),
        Some(&owed[0].thread_id)
    );

    press(&mut app, KeyCode::Char('e'));
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::ResolveDeskItems { items, dry_run: false }]
            if items == &[DeskDoneItemData {
                thread_id: owed[1].thread_id.clone(),
                lane: Some(DeskLaneKind::Owed),
                commitment_id: None,
            }]
    ));
    assert_eq!(current_thread(&app), Some(later[0].thread_id.clone()));
    assert_eq!(app.mailbox.focus_page.progress().done, 1);

    app.pending_mutation_queue.clear();
    app.pending_undo = Some(PendingUndo {
        action: UndoAction::Mutations(vec!["done-1".into()]),
        verb_past: "Done".into(),
        count: 1,
        applied_at: std::time::Instant::now(),
    });
    press(&mut app, KeyCode::Char('u'));
    let undo = app.pending_mutation_queue.pop().expect("undo queued");
    assert!(matches!(
        &undo.request,
        Request::UndoMutation { mutation_id } if mutation_id == "done-1"
    ));
    app.apply_mutation_completion(undo.effect, true);
    assert_eq!(current_thread(&app), Some(owed[1].thread_id.clone()));
    assert_eq!(app.mailbox.focus_page.progress().done, 0);
}

#[test]
fn b_puts_off_the_conversation_on_screen() {
    let (mut app, owed, _later) = focus_app();
    press(&mut app, KeyCode::Char('b'));
    assert_eq!(
        app.modals.snooze_panel.target,
        Some(crate::app::NotNowTarget::Mail {
            message_id: owed[0].message_id.clone()
        })
    );
    press(&mut app, KeyCode::Enter);
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::SetReplyLater { message_id, flag: true }] if *message_id == owed[0].message_id
    ));
    assert_eq!(current_thread(&app), Some(owed[1].thread_id.clone()));
}
