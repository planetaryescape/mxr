//! Archive in the TUI: `g e` opens the ledger and its guide, `/` asks,
//! the verbs become the daemon requests they name, and `T` on a
//! conversation previews filing it before anything is written.

use super::*;
use crate::ui::records_lens::tests::{apple, dell, lisbon_answer, octopus_list, page};
use mxr_protocol::{RecordChangeData, RecordEditData, RecordUndoData};

fn press(app: &mut App, code: KeyCode) {
    if let Some(action) = app.handle_key(KeyEvent::new(code, KeyModifiers::NONE)) {
        app.apply(action);
    }
}

fn press_shifted(app: &mut App, c: char) {
    if let Some(action) = app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::SHIFT)) {
        app.apply(action);
    }
}

fn typed(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c));
    }
}

fn queued(app: &App) -> Vec<Request> {
    app.pending_mutation_queue
        .iter()
        .map(|queued| queued.request.clone())
        .collect()
}

/// Archive open on Dell's order and an Apple receipt.
fn archive_app() -> App {
    let mut app = App::new();
    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
    press(&mut app, KeyCode::Char('e'));
    assert_eq!(app.mailbox.mailbox_view, MailboxView::ArchiveMode);
    assert!(
        app.mailbox.records_page.pending_refresh,
        "opening Archive fetches the ledger and its guide"
    );
    let loaded = page(vec![dell(), apple()], true);
    app.set_records_ledger(loaded.ledger.unwrap(), loaded.guide);
    app
}

#[test]
fn slash_and_a_query_ask_the_answer_box_and_y_copies_the_answer() {
    let mut app = archive_app();
    press(&mut app, KeyCode::Char('/'));
    assert!(app.mailbox.records_page.asking);
    typed(&mut app, "lisbon booking ref");
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        app.mailbox.records_page.pending_answer.as_deref(),
        Some("lisbon booking ref")
    );
    assert!(!app.mailbox.records_page.asking);
    app.set_records_answer(lisbon_answer());
    // With an answer on screen, the keys act on its record.
    press(&mut app, KeyCode::Char('y'));
    assert_eq!(
        app.mailbox.records_page.last_copied.as_deref(),
        Some("K7QX2M")
    );
    assert_eq!(app.status_message.as_deref(), Some("K7QX2M copied"));
    press(&mut app, KeyCode::Esc);
    assert!(app.mailbox.records_page.answer.is_none());
    // Back on the ledger: Y copies the row's amount as plain digits.
    press_shifted(&mut app, 'Y');
    assert_eq!(
        app.mailbox.records_page.last_copied.as_deref(),
        Some("1249.00")
    );
}

#[test]
fn a_shows_every_match_and_esc_puts_the_ledger_back() {
    let mut app = archive_app();
    press(&mut app, KeyCode::Char('/'));
    typed(&mut app, "lisbon booking ref");
    press(&mut app, KeyCode::Enter);
    app.mailbox.records_page.pending_answer = None;
    app.set_records_answer(lisbon_answer());
    press(&mut app, KeyCode::Char('a'));
    let page = &app.mailbox.records_page;
    assert_eq!(page.pending_answer.as_deref(), Some("lisbon booking ref"));
    assert!(page.answer_list, "the runtime asks for the list");

    // A slow answer to another query never lands under this one.
    app.set_records_answer(octopus_list());
    assert!(app.mailbox.records_page.listed_matches().is_none());

    // A list opens on its best match, and the keys act on the rows.
    let mut listed = octopus_list();
    listed.query = "lisbon booking ref".into();
    app.set_records_answer(listed);
    let page = &app.mailbox.records_page;
    assert_eq!(page.rows().len(), 2);
    assert_eq!(app.mailbox.selected_index, 1, "the February bill is best");
    assert_eq!(
        app.selected_record().map(|r| r.id.as_str()),
        Some("rec_octopus")
    );
    press(&mut app, KeyCode::Esc);
    let page = &app.mailbox.records_page;
    assert!(page.answer.is_none());
    assert!(!page.answer_list);
    assert_eq!(page.rows().len(), 2, "the ledger's Dell and Apple rows");
    assert_eq!(app.mailbox.selected_index, 0);

    // A new query decides for itself again.
    press(&mut app, KeyCode::Char('/'));
    typed(&mut app, "dell");
    press(&mut app, KeyCode::Enter);
    assert!(!app.mailbox.records_page.answer_list);
}

#[test]
fn x_takes_a_record_out_at_once_and_u_brings_it_back() {
    let mut app = archive_app();
    press_shifted(&mut app, 'X');
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::DismissRecord { record_ids, restore: false, dry_run: false }]
            if record_ids == &["rec_dell".to_string()]
    ));
    assert_eq!(app.mailbox.records_page.rows()[0].id, "rec_apple");

    // The daemon's answer offers u, which restores exactly those records.
    let undo = crate::runner::records_undo(&RecordChangeData {
        dry_run: false,
        action: "dismiss".into(),
        records: Vec::new(),
        message: mxr_protocol::archive_copy::NOT_A_RECORD.into(),
        undo: Some(RecordUndoData {
            kind: "restore".into(),
            record_ids: vec!["rec_dell".into()],
            fields: Vec::new(),
        }),
    })
    .expect("an undo");
    app.pending_mutation_queue.clear();
    app.pending_undo = Some(undo);
    press(&mut app, KeyCode::Char('u'));
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::DismissRecord { record_ids, restore: true, .. }]
            if record_ids == &["rec_dell".to_string()]
    ));
}

#[test]
fn e_previews_the_export_and_enter_writes_it() {
    let mut app = archive_app();
    press_shifted(&mut app, 'E');
    assert_eq!(app.mailbox.records_page.pending_export, Some(false));
    app.mailbox.records_page.pending_export = None;
    app.show_export(mxr_protocol::RecordExportData {
        dry_run: true,
        rows: 2,
        totals: Vec::new(),
        unchecked: 1,
        missing_pdfs: 1,
        by_kind: Vec::new(),
        summary: "2 records, \u{a3}1,251.99. 1 has an unchecked amount or date; 1 has no PDF."
            .into(),
        csv: None,
        attachments_dir: None,
        pdfs_copied: 0,
        pdf_errors: Vec::new(),
    });
    assert!(app.mailbox.records_page.export_preview.is_some());
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.mailbox.records_page.pending_export, Some(true));
    assert!(app.mailbox.records_page.export_preview.is_none());
}

#[test]
fn the_verbs_name_their_requests() {
    let mut app = archive_app();
    // Dell is checked already; Apple's amount isn't.
    press(&mut app, KeyCode::Char('v'));
    assert!(queued(&app).is_empty());
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Char('v'));
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::SetRecordField { record_id, edit: RecordEditData::ConfirmAll, dry_run: false, .. }]
            if record_id == "rec_apple"
    ));
    press(&mut app, KeyCode::Char('k'));
    press(&mut app, KeyCode::Enter);
    assert!(
        app.mailbox.pending_attachment_action.is_some(),
        "Enter opens the PDF"
    );
    press(&mut app, KeyCode::Char('o'));
    assert!(app.mailbox.records_page.pending_open.is_some());
    press(&mut app, KeyCode::Char('p'));
    assert_eq!(
        app.mailbox.records_page.filter.issuer.as_deref(),
        Some("Dell")
    );
    press(&mut app, KeyCode::Esc);
    assert!(app.mailbox.records_page.filter.issuer.is_none());
    press(&mut app, KeyCode::Char('['));
    assert_eq!(app.mailbox.records_page.filter.year, Some(2024));
    let _ = app.handle_key(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE));
    press(&mut app, KeyCode::Char('f'));
    assert_eq!(app.mailbox.records_page.kind_chip, 1);
    assert_eq!(
        app.mailbox.records_page.filter.kinds,
        vec![mxr_protocol::RecordKindData::Receipt]
    );
}

#[test]
fn comma_previews_a_fix_before_applying_exactly_it() {
    let mut app = archive_app();
    press(&mut app, KeyCode::Char(','));
    typed(&mut app, "amount=\u{a3}1,199.00");
    press(&mut app, KeyCode::Enter);
    let edit = RecordEditData::Set {
        field: "amount".into(),
        value: "\u{a3}1,199.00".into(),
    };
    assert_eq!(
        app.mailbox.records_page.pending_fix_preview,
        Some(("rec_dell".to_string(), edit.clone()))
    );
    assert!(
        queued(&app).is_empty(),
        "nothing is written before the preview"
    );
    app.show_fix_preview(RecordChangeData {
        dry_run: true,
        action: "set_field".into(),
        records: Vec::new(),
        message: "Would do: Fixed amount. It stays as you set it.".into(),
        undo: None,
    });
    press(&mut app, KeyCode::Enter);
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::SetRecordField { record_id, edit: applied, dry_run: false, .. }]
            if record_id == "rec_dell" && *applied == edit
    ));
    assert!(app.mailbox.records_page.prompt.is_none());
}

#[test]
fn capital_t_on_a_conversation_previews_filing_then_files_it() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(2);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    let message_id = app.mailbox.envelopes[0].id.clone();
    press_shifted(&mut app, 'T');
    let menu = app
        .mailbox
        .records_page
        .pass_menu
        .as_ref()
        .expect("the pass menu");
    assert_eq!(menu.message_id, message_id);
    press(&mut app, KeyCode::Char('a'));
    assert_eq!(
        app.mailbox.records_page.pending_file_preview.as_ref(),
        Some(&message_id)
    );
    assert!(queued(&app).is_empty());
    app.show_file_preview(RecordChangeData {
        dry_run: true,
        action: "file".into(),
        records: vec![dell()],
        message: "Would file in Archive: Dell, XPS 14 laptop.".into(),
        undo: None,
    });
    press(&mut app, KeyCode::Enter);
    assert!(matches!(
        queued(&app).as_slice(),
        [Request::FileRecord { message_id: filed, kind: None, dry_run: false }] if *filed == message_id
    ));
    assert!(app.mailbox.records_page.pass_menu.is_none());
}

#[test]
fn the_first_answer_retires_the_card_and_help_leads_with_archive() {
    let mut app = archive_app();
    let unseen = page(vec![dell()], false);
    app.set_records_ledger(unseen.ledger.unwrap(), unseen.guide);
    assert!(app.mailbox.records_page.card_visible());
    app.set_records_answer(lisbon_answer());
    assert!(!app.mailbox.records_page.card_visible());
    assert!(queued(&app).iter().any(|request| matches!(
        request,
        Request::SetModeGuideSeen { mode, seen: true } if mode == "archive"
    )));
    let guide = app.help_mode_guide().expect("Archive's guide");
    assert_eq!(guide.mode, "archive");
}
