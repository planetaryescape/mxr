use super::*;
use crate::app::UpdatesRow;
use crate::ui::updates_lens::tests::populated;
use mxr_protocol::UPDATES_GUIDE;

fn press(app: &mut App, code: KeyCode) {
    let modifiers = match code {
        KeyCode::Char(c) if c.is_ascii_uppercase() => KeyModifiers::SHIFT,
        _ => KeyModifiers::NONE,
    };
    if let Some(action) = app.handle_key(KeyEvent::new(code, modifiers)) {
        app.apply(action);
    }
}

fn queued(app: &App) -> Vec<Request> {
    app.pending_mutation_queue
        .iter()
        .map(|queued| queued.request.clone())
        .collect()
}

fn updates_app() -> App {
    let mut app = App::new();
    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
    press(&mut app, KeyCode::Char('u'));
    assert_eq!(app.mailbox.mailbox_view, MailboxView::Updates);
    assert!(
        app.mailbox.updates_page.pending_refresh && app.mailbox.pending_rail_refresh,
        "opening Updates fetches the digest, its guide and the rail"
    );
    let seen = chrono::Utc::now();
    app.set_updates_digest(
        populated(),
        Some(UPDATES_GUIDE.to_data(|_| Some(seen), &mxr_keymap::guide_key)),
    );
    app
}

fn select_source(app: &mut App, name: &str) {
    let index = app
        .mailbox
        .updates_page
        .rows()
        .iter()
        .position(|row| matches!(row, UpdatesRow::Line(line) if line.source_name == name))
        .expect("the source's line");
    app.mailbox.selected_index = index;
}

#[test]
fn shift_e_lets_go_of_the_digest_only_after_the_preview_and_commits_that_selection() {
    let mut app = updates_app();
    press(&mut app, KeyCode::Char('E'));
    assert!(
        queued(&app).is_empty(),
        "nothing changes before the preview"
    );
    assert!(
        std::mem::take(&mut app.mailbox.updates_page.pending_let_go_preview),
        "asks the daemon for a dry run"
    );
    let preview = super::now::let_go_preview(&[ThreadId::new(), ThreadId::new()]);
    app.show_updates_let_go_preview(preview.clone());
    let rendered = render_to_string(100, 30, |frame| app.draw(frame));
    assert!(rendered.contains("Let go of this digest"), "{rendered}");
    assert!(rendered.contains("Let go of 2 updates"), "{rendered}");
    press(&mut app, KeyCode::Enter);
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::LetGoDigest { selection_token: Some(token), cut: Some(cut), source_key: None, dry_run: false, .. }]
            if token == &preview.selection_token && *cut == preview.cut_at
    ));
    assert_eq!(
        app.mailbox.updates_page.row_count(),
        0,
        "the digest clears at once"
    );
}

#[test]
fn s_pins_only_the_latest_email_and_u_restores_its_prior_value() {
    let mut app = updates_app();
    select_source(&mut app, "Stripe (stripe.com)");
    let message_id = match app.selected_updates_row().unwrap() {
        UpdatesRow::Line(line) => line.latest_message_id.clone().expect("latest email"),
        _ => unreachable!(),
    };
    press(&mut app, KeyCode::Char('p'));
    assert!(queued(&app).is_empty(), "p moved to s and pins nothing");
    press(&mut app, KeyCode::Char('s'));
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::PinMessages { message_ids, pinned: true }]
            if message_ids == &vec![message_id.clone()]
    ));
    let effect = app.pending_mutation_queue[0].effect.clone();
    let crate::app::MutationEffect::UpdatesPin {
        message_id: effect_id,
        prior_pinned: false,
        status,
    } = effect
    else {
        panic!("pin carries its precise undo context");
    };
    assert_eq!(effect_id, message_id);
    let (effect, undo) = crate::runner::updates_pin_outcome(effect_id, false, status, 1);
    assert!(matches!(
        effect,
        crate::app::MutationEffect::RefreshUpdates(_)
    ));
    app.set_pending_undo(undo.expect("changed pin offers Undo"));
    press(&mut app, KeyCode::Char('u'));
    assert!(matches!(
        queued(&app).last(),
        Some(Request::PinMessages { message_ids, pinned: false })
            if message_ids == &vec![message_id.clone()]
    ));
    assert!(matches!(
        app.pending_mutation_queue
            .last()
            .map(|queued| &queued.effect),
        Some(crate::app::MutationEffect::RefreshUpdates(_))
    ));

    let existing = crate::app::PendingUndo {
        action: crate::app::UndoAction::Mutations(vec!["earlier".into()]),
        verb_past: "Archived".into(),
        count: 1,
        applied_at: std::time::Instant::now(),
    };
    let (effect, undo) =
        crate::runner::updates_pin_outcome(message_id, false, "Pinned latest email".into(), 0);
    assert!(matches!(
        effect,
        crate::app::MutationEffect::RefreshUpdates(_)
    ));
    assert!(undo.is_none());
    app.set_pending_undo(existing.clone());
    assert!(matches!(
        app.pending_undo.as_ref(),
        Some(undo) if undo.verb_past == "Archived"
            && matches!(&undo.action, crate::app::UndoAction::Mutations(ids) if ids.len() == 1 && ids[0] == "earlier")
    ));
}

#[test]
fn y_copies_the_lines_code_and_names_its_host() {
    let mut app = updates_app();
    let name = app
        .mailbox
        .updates_page
        .rows()
        .iter()
        .find_map(|row| match row {
            UpdatesRow::Line(line) => Some(line.source_name.clone()),
            _ => None,
        });
    let name = name.expect("a line");
    if let Some(digest) = app.mailbox.updates_page.digest.as_mut() {
        for line in digest
            .needs_a_look
            .iter_mut()
            .chain(digest.changed.iter_mut())
            .chain(digest.routine.iter_mut())
            .filter(|line| line.source_name == name)
        {
            line.code = Some("482913".into());
            line.sender_email = "otp@attacker.example".into();
        }
    }
    select_source(&mut app, &name);
    press(&mut app, KeyCode::Char('y'));
    assert_eq!(
        app.mailbox.updates_page.last_copied.as_deref(),
        Some("482913")
    );
    let status = app.status_message.clone().unwrap_or_default();
    assert!(status.contains("attacker.example"), "{status}");
}
