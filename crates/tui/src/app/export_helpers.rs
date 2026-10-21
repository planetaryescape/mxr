use super::*;
use crate::export_output::{default_export_path, FORMAT_CHOICES};
use mxr_core::ExportFormat;
use std::path::PathBuf;

impl App {
    /// `E` on a thread. Opens the picker; nothing is written until a
    /// destination is confirmed.
    pub fn open_export_modal(&mut self) {
        let Some(thread_id) = self.context_envelope().map(|env| env.thread_id.clone()) else {
            self.status_message = Some("No message selected".into());
            return;
        };
        self.modals.export.open(thread_id);
    }

    /// Arrow keys in the format stage. `j`/`k` are not used here because
    /// `j` picks JSON.
    pub fn export_modal_move(&mut self, down: bool) {
        let state = &mut self.modals.export;
        if down {
            if state.selected + 1 < FORMAT_CHOICES.len() {
                state.selected += 1;
            }
        } else {
            state.selected = state.selected.saturating_sub(1);
        }
    }

    pub fn export_modal_choose_format(&mut self, format: ExportFormat) {
        let state = &mut self.modals.export;
        state.format = Some(format);
        state.stage = ExportStage::Destination;
    }

    pub fn export_modal_choose_clipboard(&mut self) {
        self.queue_export(ExportDestination::Clipboard);
    }

    /// Moves to the path stage with the default path for the chosen format.
    pub fn export_modal_choose_save(&mut self) {
        let state = &mut self.modals.export;
        let (Some(thread_id), Some(format)) = (state.thread_id.as_ref(), state.format.as_ref())
        else {
            return;
        };
        state.input = default_export_path(thread_id, format);
        state.error = None;
        state.awaiting_overwrite_confirm = false;
        state.stage = ExportStage::Path;
    }

    pub fn export_modal_push_char(&mut self, c: char) {
        let state = &mut self.modals.export;
        state.input.push(c);
        state.error = None;
        state.awaiting_overwrite_confirm = false;
    }

    pub fn export_modal_backspace(&mut self) {
        let state = &mut self.modals.export;
        state.input.pop();
        state.error = None;
        state.awaiting_overwrite_confirm = false;
    }

    /// Validates the typed path the same way the attachment save does. A
    /// blank path, a directory, or an existing file keep the modal open with
    /// the reason. An existing file needs a second Enter.
    pub fn export_modal_confirm_path(&mut self) {
        let raw = self.modals.export.input.trim().to_string();
        if raw.is_empty() {
            self.modals.export.error = Some("Path cannot be empty".into());
            return;
        }
        let path = match shellexpand::full(&raw) {
            Ok(value) => PathBuf::from(value.into_owned()),
            Err(error) => {
                self.modals.export.error = Some(format!("Bad path: {error}"));
                return;
            }
        };
        if path.is_dir() {
            self.modals.export.error =
                Some("Destination is a directory — include a filename".into());
            return;
        }
        let overwrite_ok = self.modals.export.awaiting_overwrite_confirm;
        if path.exists() && !overwrite_ok {
            self.modals.export.awaiting_overwrite_confirm = true;
            self.modals.export.error = None;
            return;
        }
        self.queue_export(ExportDestination::Save { path, overwrite_ok });
    }

    /// Hands the export to the runner and closes the picker. The status line
    /// is set by the runner once the write or copy finishes, so it names the
    /// real destination.
    fn queue_export(&mut self, destination: ExportDestination) {
        let export = std::mem::take(&mut self.modals.export);
        if let (Some(thread_id), Some(format)) = (export.thread_id, export.format) {
            self.mailbox.pending_export = Some(PendingExport {
                thread_id,
                format,
                destination,
            });
            self.status_message = Some("Exporting thread...".into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::id::ThreadId;

    fn picker_app() -> (App, ThreadId) {
        let mut app = App::new();
        let thread_id = ThreadId::new();
        app.modals.export.open(thread_id.clone());
        (app, thread_id)
    }

    #[test]
    fn save_path_is_prefilled_per_format_and_expands_home() {
        let (mut app, thread_id) = picker_app();
        app.export_modal_choose_format(ExportFormat::LlmContext);
        app.export_modal_choose_save();
        assert_eq!(app.modals.export.stage, ExportStage::Path);
        assert_eq!(
            app.modals.export.input,
            format!("~/mxr/exports/mxr-thread-{thread_id}.txt")
        );
    }

    #[test]
    fn empty_path_stays_open_with_an_error() {
        let (mut app, _) = picker_app();
        app.export_modal_choose_format(ExportFormat::Json);
        app.export_modal_choose_save();
        while !app.modals.export.input.is_empty() {
            app.export_modal_backspace();
        }
        app.export_modal_confirm_path();
        assert!(app.modals.export.visible);
        assert_eq!(
            app.modals.export.error.as_deref(),
            Some("Path cannot be empty")
        );
        assert!(app.mailbox.pending_export.is_none());
    }

    #[test]
    fn directory_path_is_rejected_and_nothing_is_queued() {
        let (mut app, _) = picker_app();
        let dir = tempfile::tempdir().unwrap();
        app.export_modal_choose_format(ExportFormat::Markdown);
        app.export_modal_choose_save();
        app.modals.export.input = dir.path().display().to_string();
        app.export_modal_confirm_path();
        assert!(app.modals.export.visible);
        assert!(app
            .modals
            .export
            .error
            .as_deref()
            .unwrap()
            .contains("directory"));
        assert!(app.mailbox.pending_export.is_none());
    }

    #[test]
    fn existing_file_asks_twice_and_editing_resets_the_confirmation() {
        let (mut app, thread_id) = picker_app();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("thread.md");
        std::fs::write(&path, "old").unwrap();
        app.export_modal_choose_format(ExportFormat::Markdown);
        app.export_modal_choose_save();
        app.modals.export.input = path.display().to_string();

        app.export_modal_confirm_path();
        assert!(app.modals.export.awaiting_overwrite_confirm);
        assert!(app.mailbox.pending_export.is_none());

        app.export_modal_push_char('x');
        assert!(!app.modals.export.awaiting_overwrite_confirm);
        app.export_modal_backspace();

        app.export_modal_confirm_path();
        assert!(app.modals.export.awaiting_overwrite_confirm);
        app.export_modal_confirm_path();

        assert!(!app.modals.export.visible);
        assert_eq!(
            app.mailbox.pending_export,
            Some(PendingExport {
                thread_id,
                format: ExportFormat::Markdown,
                destination: ExportDestination::Save {
                    path,
                    overwrite_ok: true,
                },
            })
        );
    }

    #[test]
    fn new_file_is_queued_without_overwrite() {
        let (mut app, _) = picker_app();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/new.mbox");
        app.export_modal_choose_format(ExportFormat::Mbox);
        app.export_modal_choose_save();
        app.modals.export.input = path.display().to_string();
        app.export_modal_confirm_path();
        let pending = app.mailbox.pending_export.clone().unwrap();
        assert_eq!(pending.format, ExportFormat::Mbox);
        assert_eq!(
            pending.destination,
            ExportDestination::Save {
                path,
                overwrite_ok: false,
            }
        );
    }

    #[test]
    fn relative_path_is_queued_as_typed_and_not_treated_as_existing() {
        let (mut app, _) = picker_app();
        app.export_modal_choose_format(ExportFormat::Json);
        app.export_modal_choose_save();
        app.modals.export.input = "exports-test-relative/thread.json".into();
        app.export_modal_confirm_path();
        assert_eq!(
            app.mailbox
                .pending_export
                .map(|pending| pending.destination),
            Some(ExportDestination::Save {
                path: PathBuf::from("exports-test-relative/thread.json"),
                overwrite_ok: false,
            })
        );
    }

    #[test]
    fn prefilled_tilde_path_expands_to_the_home_exports_folder() {
        let (mut app, thread_id) = picker_app();
        app.export_modal_choose_format(ExportFormat::Json);
        app.export_modal_choose_save();
        app.export_modal_confirm_path();
        let Some(PendingExport {
            destination: ExportDestination::Save { path, .. },
            ..
        }) = app.mailbox.pending_export.clone()
        else {
            panic!("expected a save export to be queued");
        };
        let home = dirs::home_dir().expect("tests need a home directory");
        assert_eq!(
            path,
            home.join("mxr/exports")
                .join(format!("mxr-thread-{thread_id}.json"))
        );
    }

    #[test]
    fn clipboard_queues_and_closes_the_picker() {
        let (mut app, thread_id) = picker_app();
        app.export_modal_choose_format(ExportFormat::Markdown);
        app.export_modal_choose_clipboard();
        assert!(!app.modals.export.visible);
        assert_eq!(
            app.mailbox.pending_export,
            Some(PendingExport {
                thread_id,
                format: ExportFormat::Markdown,
                destination: ExportDestination::Clipboard,
            })
        );
    }

    #[test]
    fn reopening_after_edits_starts_clean() {
        let (mut app, _) = picker_app();
        app.export_modal_choose_format(ExportFormat::Json);
        app.export_modal_choose_save();
        app.export_modal_push_char('z');
        app.modals.export.close();
        app.modals.export.open(ThreadId::new());
        let state = &app.modals.export;
        assert!(state.visible);
        assert_eq!(state.stage, ExportStage::Format);
        assert_eq!(state.selected, 0);
        assert!(state.format.is_none());
        assert!(state.input.is_empty());
        assert!(state.error.is_none());
        assert!(!state.awaiting_overwrite_confirm);
    }

    #[test]
    fn close_without_choosing_queues_nothing() {
        let (mut app, _) = picker_app();
        app.export_modal_choose_format(ExportFormat::Markdown);
        app.export_modal_choose_save();
        app.modals.export.close();
        assert!(!app.modals.export.visible);
        assert!(app.mailbox.pending_export.is_none());
    }
}
