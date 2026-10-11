//! Updates in the TUI: `g u` opens the briefing, `A` lets go of the digest
//! only after the daemon's preview and commits that selection, `e` lets go
//! of one source, `t` hands a line to To do, `K` tunes a source with undo,
//! and trackers can't be let go of or tuned.

use super::*;
use crate::app::UpdatesRow;
use crate::ui::updates_lens::tests::populated;
use mxr_protocol::{UpdateSourceSettingData, UPDATES_GUIDE};

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

/// Updates open on a populated digest, its card already seen.
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
    app.set_updates_digest(populated(), Some(UPDATES_GUIDE.to_data(|_| Some(seen))));
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
fn a_lets_go_of_the_digest_only_after_the_preview_and_commits_that_selection() {
    let mut app = updates_app();
    press(&mut app, KeyCode::Char('A'));
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
fn esc_on_the_preview_keeps_the_digest() {
    let mut app = updates_app();
    app.show_updates_let_go_preview(super::now::let_go_preview(&[ThreadId::new()]));
    press(&mut app, KeyCode::Esc);
    assert!(app.mailbox.updates_page.let_go_preview.is_none());
    assert!(queued(&app).is_empty());
}

#[test]
fn e_previews_then_lets_go_of_one_source_and_a_tracker_stays() {
    let mut app = updates_app();
    select_source(&mut app, "Strava");
    press(&mut app, KeyCode::Char('e'));
    assert!(
        queued(&app).is_empty(),
        "nothing changes before the preview"
    );
    assert!(std::mem::take(
        &mut app.mailbox.updates_page.pending_let_go_preview
    ));
    assert_eq!(
        app.mailbox
            .updates_page
            .let_go_source
            .as_ref()
            .map(|(_, key)| key.as_str()),
        Some("strava"),
        "the preview is for this source only"
    );
    let preview = super::now::let_go_preview(&[ThreadId::new()]);
    app.show_updates_let_go_preview(preview.clone());
    press(&mut app, KeyCode::Enter);
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::LetGoDigest { source_key: Some(key), selection_token: Some(token), dry_run: false, .. }]
            if key == "strava" && token == &preview.selection_token
    ));
    assert!(app
        .mailbox
        .updates_page
        .rows()
        .iter()
        .all(|row| !matches!(row, UpdatesRow::Line(line) if line.source_name == "Strava")));

    let mut app = updates_app();
    select_source(&mut app, "Bookshop");
    press(&mut app, KeyCode::Char('e'));
    assert!(queued(&app).is_empty(), "a parcel leaves on its own");
}

#[test]
fn t_makes_a_to_do_from_the_line_unless_it_is_already_in_to_do() {
    let mut app = updates_app();
    select_source(&mut app, "Stripe (stripe.com)");
    press(&mut app, KeyCode::Char('t'));
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::CreateTodo { title, dry_run: false, .. }]
            if title == "Check Stripe (stripe.com): Payout of R 4,210.00 failed: bank declined"
    ));

    let mut app = updates_app();
    select_source(&mut app, "Google");
    press(&mut app, KeyCode::Char('t'));
    assert!(queued(&app).is_empty());
    assert_eq!(app.status_message.as_deref(), Some("Already in To do"));
}

#[test]
fn k_tunes_a_source_and_u_sets_it_back() {
    let mut app = updates_app();
    select_source(&mut app, "Vercel");
    press(&mut app, KeyCode::Char('K'));
    assert!(app.mailbox.updates_page.tune.is_some());
    let rendered = render_to_string(100, 30, |frame| app.draw(frame));
    assert!(rendered.contains("Tune Vercel"), "{rendered}");
    press(&mut app, KeyCode::Char('3'));
    assert!(app.mailbox.updates_page.tune.is_none());
    let sent = queued(&app);
    let [Request::SetUpdateSource {
        source,
        setting: UpdateSourceSettingData::Muted,
        dry_run: false,
        account_id: Some(account_id),
    }] = sent.as_slice()
    else {
        panic!("{:?}", queued(&app));
    };
    assert_eq!(source, "vercel");

    // The daemon answered with the prior setting: `u` puts it back.
    app.pending_mutation_queue.clear();
    app.pending_undo = Some(crate::app::PendingUndo {
        action: crate::app::UndoAction::UpdateSource {
            account_id: account_id.clone(),
            source_key: "vercel".into(),
            prior: UpdateSourceSettingData::EveryDigest,
        },
        verb_past: "Tuned".into(),
        count: 1,
        applied_at: std::time::Instant::now(),
    });
    press(&mut app, KeyCode::Char('u'));
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::SetUpdateSource {
            setting: UpdateSourceSettingData::EveryDigest,
            ..
        }]
    ));
}

#[test]
fn p_pins_only_the_latest_email_and_u_restores_its_prior_value() {
    let mut app = updates_app();
    select_source(&mut app, "Stripe (stripe.com)");
    let message_id = match app.selected_updates_row().unwrap() {
        UpdatesRow::Line(line) => line.latest_message_id.clone().expect("latest email"),
        _ => unreachable!(),
    };
    press(&mut app, KeyCode::Char('p'));
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

    // A no-op still refreshes the digest, but leaves an older Undo intact.
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
fn p_unpins_and_undo_restores_the_pinned_value() {
    let mut app = updates_app();
    let digest = app.mailbox.updates_page.digest.as_mut().unwrap();
    let line = digest
        .needs_a_look
        .iter_mut()
        .find(|line| line.source_name == "Stripe (stripe.com)")
        .unwrap();
    line.latest_message_pinned = true;
    let message_id = line.latest_message_id.clone().unwrap();
    select_source(&mut app, "Stripe (stripe.com)");
    press(&mut app, KeyCode::Char('p'));
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::PinMessages { message_ids, pinned: false }]
            if message_ids == &vec![message_id.clone()]
    ));
    let (effect, undo) = crate::runner::updates_pin_outcome(
        message_id.clone(),
        true,
        "Unpinned latest email".into(),
        1,
    );
    assert!(matches!(
        effect,
        crate::app::MutationEffect::RefreshUpdates(_)
    ));
    app.set_pending_undo(undo.unwrap());
    press(&mut app, KeyCode::Char('u'));
    assert!(matches!(
        queued(&app).last(),
        Some(Request::PinMessages { message_ids, pinned: true })
            if message_ids == &vec![message_id]
    ));
}

#[test]
fn p_does_not_pin_a_tracker_without_a_latest_message() {
    let mut app = updates_app();
    select_source(&mut app, "Bookshop");
    press(&mut app, KeyCode::Char('p'));
    assert!(queued(&app).is_empty());
    assert_eq!(
        app.status_message.as_deref(),
        Some("This update has no email to pin")
    );
}

#[test]
fn uppercase_s_previews_an_account_sender_sweep_and_cancel_or_apply_uses_that_preview() {
    let mut app = updates_app();
    select_source(&mut app, "Stripe (stripe.com)");
    let (account_id, sender_email) = match app.selected_updates_row().unwrap() {
        UpdatesRow::Line(line) => (line.account_id.clone(), line.sender_email.clone()),
        _ => unreachable!(),
    };
    press(&mut app, KeyCode::Char('S'));
    let target = app
        .mailbox
        .pending_sweep_preview
        .take()
        .expect("S requests a preview");
    assert_eq!(target.account_id, Some(account_id));
    assert_eq!(target.sender_email.as_deref(), Some(sender_email.as_str()));
    assert!(matches!(
        crate::runner::sweep_request(&target, true, None),
        Request::SweepPlace {
            account_id: Some(ref account),
            sender_email: Some(ref sender),
            dry_run: true,
            preview_token: None,
            ..
        } if account == &target.account_id.clone().unwrap() && sender == &sender_email
    ));

    let confirm = || crate::app::PendingSweepConfirm {
        target: target.clone(),
        preview: mxr_protocol::SweepPreviewData {
            place: mxr_protocol::MailPlaceData::PaperTrail,
            sender_email: Some(sender_email.clone()),
            count: 8,
            pinned_excluded: 2,
            senders: vec![],
            sample_subjects: vec!["A result".into()],
            preview_token: Some("updates-selection".into()),
        },
        shown: 99,
        sweep_focused: false,
        from_updates: true,
    };

    app.show_sweep_preview(confirm());
    assert_eq!(app.mailbox.sweep_confirm.as_ref().unwrap().shown, 0);
    press(&mut app, KeyCode::Esc);
    assert!(app.mailbox.sweep_confirm.is_none());
    assert!(app.mailbox.pending_sweep.is_none(), "cancel does not apply");

    app.show_sweep_preview(confirm());
    press(&mut app, KeyCode::Enter);
    let apply = app
        .mailbox
        .pending_sweep
        .as_ref()
        .expect("Enter applies preview");
    assert_eq!(
        apply.preview.preview_token.as_deref(),
        Some("updates-selection")
    );
    assert!(matches!(
        crate::runner::sweep_request(
            &apply.target,
            false,
            apply.preview.preview_token.clone()
        ),
        Request::SweepPlace {
            account_id: Some(ref account),
            sender_email: Some(ref sender),
            dry_run: false,
            preview_token: Some(ref token),
            ..
        } if account == &target.account_id.clone().unwrap()
            && sender == &sender_email
            && token == "updates-selection"
    ));
}

#[test]
fn enter_unfolds_the_quieter_sources_and_o_opens_the_email() {
    let mut app = updates_app();
    let fold = app.mailbox.updates_page.row_count() - 1;
    app.mailbox.selected_index = fold;
    press(&mut app, KeyCode::Enter);
    assert!(app.mailbox.updates_page.routine_open);

    select_source(&mut app, "Strava");
    let message = match app.selected_updates_row() {
        Some(UpdatesRow::Line(line)) => line.latest_message_id.clone(),
        _ => None,
    };
    press(&mut app, KeyCode::Char('o'));
    assert_eq!(app.mailbox.pending_invite_open, message);
}

#[test]
fn a_hint_shows_on_the_first_line_after_a_key_and_esc_dismisses_it_everywhere() {
    let mut app = updates_app();
    app.mailbox.updates_page.guide = Some(UPDATES_GUIDE.to_data(|_| None));
    app.mailbox.selected_index = 0;
    assert!(app.active_hint().is_none(), "nothing on arrival");
    app.note_key_for_hints();
    let id = app
        .active_hint()
        .expect("a hint on the first line")
        .id
        .clone();
    assert!(
        ["updates.suggestion", "updates.source"].contains(&id.as_str()),
        "{id}"
    );
    press(&mut app, KeyCode::Esc);
    assert!(app.active_hint().is_none());
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::SetHintSeen { hint, seen: true }] if *hint == id
    ));
    assert_eq!(
        app.help_mode_guide().map(|guide| guide.mode.as_str()),
        Some("updates")
    );
}
