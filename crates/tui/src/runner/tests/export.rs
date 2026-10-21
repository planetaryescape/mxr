//! Thread export in the TUI: `E` on a thread opens the picker, and each key
//! step either moves on or does nothing. Only the last confirmed step queues
//! a write or a copy.

use super::*;
use crate::app::{ExportDestination, ExportStage, PendingExport};
use mxr_core::ExportFormat;

fn press(app: &mut App, code: KeyCode) {
    if let Some(action) = app.handle_key(KeyEvent::new(code, KeyModifiers::NONE)) {
        app.apply(action);
    }
}

fn shift_e(app: &mut App) {
    if let Some(action) = app.handle_key(KeyEvent::new(KeyCode::Char('E'), KeyModifiers::SHIFT)) {
        app.apply(action);
    }
}

fn thread_app() -> App {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app
}

#[test]
fn e_on_a_thread_opens_the_picker_at_the_format_step() {
    let mut app = thread_app();
    shift_e(&mut app);
    assert!(app.modals.export.visible);
    assert_eq!(app.modals.export.stage, ExportStage::Format);
    assert!(app.mailbox.pending_export.is_none());
}

#[test]
fn e_with_no_thread_selected_says_so_and_opens_nothing() {
    let mut app = App::new();
    shift_e(&mut app);
    assert!(!app.modals.export.visible);
    assert_eq!(app.status_message.as_deref(), Some("No message selected"));
}

#[test]
fn format_key_then_save_queues_the_chosen_format_at_the_prefilled_path() {
    let mut app = thread_app();
    let thread_id = app.mailbox.envelopes[0].thread_id.clone();
    shift_e(&mut app);
    press(&mut app, KeyCode::Char('x'));
    assert_eq!(app.modals.export.stage, ExportStage::Destination);
    press(&mut app, KeyCode::Char('s'));
    assert_eq!(app.modals.export.stage, ExportStage::Path);
    assert_eq!(
        app.modals.export.input,
        format!("~/mxr/exports/mxr-thread-{thread_id}.mbox")
    );
    // Replace the prefilled path with a file in a scratch directory.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.mbox");
    app.modals.export.input = path.display().to_string();
    press(&mut app, KeyCode::Enter);
    assert!(!app.modals.export.visible);
    assert_eq!(
        app.mailbox.pending_export,
        Some(PendingExport {
            thread_id,
            format: ExportFormat::Mbox,
            destination: ExportDestination::Save {
                path,
                overwrite_ok: false,
            },
        })
    );
}

#[test]
fn every_format_key_is_offered_and_c_queues_a_clipboard_copy() {
    for (key, format) in [
        ('m', ExportFormat::Markdown),
        ('j', ExportFormat::Json),
        ('x', ExportFormat::Mbox),
        ('l', ExportFormat::LlmContext),
    ] {
        let mut app = thread_app();
        shift_e(&mut app);
        press(&mut app, KeyCode::Char(key));
        press(&mut app, KeyCode::Char('c'));
        assert!(!app.modals.export.visible);
        let pending = app.mailbox.pending_export.take().expect("copy queued");
        assert_eq!(pending.format, format);
        assert_eq!(pending.destination, ExportDestination::Clipboard);
    }
}

#[test]
fn arrows_and_enter_pick_a_format_too() {
    let mut app = thread_app();
    shift_e(&mut app);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.modals.export.format, Some(ExportFormat::Mbox));
    assert_eq!(app.modals.export.stage, ExportStage::Destination);
}

#[test]
fn esc_at_each_step_writes_nothing_and_queues_nothing() {
    let mut app = thread_app();
    shift_e(&mut app);
    press(&mut app, KeyCode::Esc);
    assert!(!app.modals.export.visible);

    shift_e(&mut app);
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Esc);
    assert!(!app.modals.export.visible);

    shift_e(&mut app);
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Char('s'));
    press(&mut app, KeyCode::Esc);
    assert!(!app.modals.export.visible);

    assert!(app.mailbox.pending_export.is_none());
}

#[test]
fn letters_typed_into_the_path_do_not_trigger_format_or_destination_keys() {
    let mut app = thread_app();
    shift_e(&mut app);
    press(&mut app, KeyCode::Char('m'));
    press(&mut app, KeyCode::Char('s'));
    app.modals.export.input.clear();
    press(&mut app, KeyCode::Char('c'));
    press(&mut app, KeyCode::Char('j'));
    assert_eq!(app.modals.export.input, "cj");
    assert!(app.modals.export.visible);
    assert!(app.mailbox.pending_export.is_none());
}
