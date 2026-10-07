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
    app.set_updates_digest(populated(), Some(UPDATES_GUIDE.to_data(Some(seen))));
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
fn e_lets_go_of_one_source_and_a_tracker_stays() {
    let mut app = updates_app();
    select_source(&mut app, "Strava");
    press(&mut app, KeyCode::Char('e'));
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::LetGoDigest { source_key: Some(key), dry_run: false, .. }] if key == "strava"
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
    select_source(&mut app, "Stripe");
    press(&mut app, KeyCode::Char('t'));
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::CreateTodo { title, dry_run: false, .. }]
            if title == "Check Stripe: Payout of R 4,210.00 failed: bank declined"
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
fn the_card_retires_on_esc_and_the_lens_reads_the_daemons_guide() {
    let mut app = updates_app();
    app.mailbox.updates_page.guide = Some(UPDATES_GUIDE.to_data(None));
    assert!(app.mailbox.updates_page.card_visible());
    press(&mut app, KeyCode::Esc);
    assert!(!app.mailbox.updates_page.card_visible());
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::SetModeGuideSeen { mode, seen: true }] if mode == "updates"
    ));
    assert_eq!(
        app.help_mode_guide().map(|guide| guide.mode.as_str()),
        Some("updates")
    );
}
