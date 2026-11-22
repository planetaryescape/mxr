use super::*;
use crate::ui::records_lens::tests::{apple, dell, lisbon_answer, page};
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
    press(&mut app, KeyCode::Char('y'));
    assert_eq!(
        app.mailbox.records_page.last_copied.as_deref(),
        Some("K7QX2M")
    );
    assert_eq!(app.status_message.as_deref(), Some("K7QX2M copied"));
    press(&mut app, KeyCode::Esc);
    assert!(app.mailbox.records_page.answer.is_none());
    press_shifted(&mut app, 'Y');
    assert_eq!(
        app.mailbox.records_page.last_copied.as_deref(),
        Some("1249.00")
    );
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
fn the_export_previews_and_enter_writes_it() {
    let mut app = archive_app();
    app.apply(Action::RecordsExport);
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
fn pass_to_a_mode_previews_filing_then_files_it() {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(2);
    app.mailbox.all_envelopes = app.mailbox.envelopes.clone();
    let message_id = app.mailbox.envelopes[0].id.clone();
    app.apply(Action::PassToMode);
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
