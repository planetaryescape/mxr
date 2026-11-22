use super::*;
use crate::app::{ExportDestination, ExportStage, PendingExport};
use mxr_core::ExportFormat;

fn press(app: &mut App, code: KeyCode) {
    if let Some(action) = app.handle_key(KeyEvent::new(code, KeyModifiers::NONE)) {
        app.apply(action);
    }
}

fn export(app: &mut App) {
    app.apply(Action::ExportThread);
}

fn thread_app() -> App {
    let mut app = App::new();
    app.mailbox.envelopes = make_test_envelopes(1);
    app
}

#[test]
fn format_key_then_save_queues_the_chosen_format_at_the_prefilled_path() {
    let mut app = thread_app();
    let thread_id = app.mailbox.envelopes[0].thread_id.clone();
    export(&mut app);
    press(&mut app, KeyCode::Char('x'));
    assert_eq!(app.modals.export.stage, ExportStage::Destination);
    press(&mut app, KeyCode::Char('s'));
    assert_eq!(app.modals.export.stage, ExportStage::Path);
    assert_eq!(
        app.modals.export.input,
        format!("~/mxr/exports/mxr-thread-{thread_id}.mbox")
    );
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
fn esc_at_each_step_writes_nothing_and_queues_nothing() {
    let mut app = thread_app();
    export(&mut app);
    press(&mut app, KeyCode::Esc);
    assert!(!app.modals.export.visible);

    export(&mut app);
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Esc);
    assert!(!app.modals.export.visible);

    export(&mut app);
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Char('s'));
    press(&mut app, KeyCode::Esc);
    assert!(!app.modals.export.visible);

    assert!(app.mailbox.pending_export.is_none());
}
