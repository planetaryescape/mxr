use mxr_core::ExportFormat;
use std::path::PathBuf;

/// The picker's steps: format, then destination, then (for save) the path.
/// Esc closes it from any step, so nothing is written or copied until the
/// last step confirms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExportStage {
    #[default]
    Format,
    Destination,
    Path,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportDestination {
    Clipboard,
    /// `overwrite_ok` is set only after the user pressed Enter on the
    /// "file exists" prompt, so the write must not replace anything else.
    Save {
        path: PathBuf,
        overwrite_ok: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingExport {
    pub thread: crate::app::ThreadKey,
    pub format: ExportFormat,
    pub destination: ExportDestination,
}

#[derive(Debug, Clone, Default)]
pub struct ExportModalState {
    pub visible: bool,
    pub thread: Option<crate::app::ThreadKey>,
    pub stage: ExportStage,
    /// Highlighted row in the format stage, an index into `FORMAT_CHOICES`.
    pub selected: usize,
    /// Set when the user leaves the format stage. The path stage relies on it
    /// being fixed, so the prefilled path always matches the chosen format.
    pub format: Option<ExportFormat>,
    pub input: String,
    pub error: Option<String>,
    pub awaiting_overwrite_confirm: bool,
}

impl ExportModalState {
    pub fn open(&mut self, thread: impl Into<crate::app::ThreadKey>) {
        *self = Self {
            visible: true,
            thread: Some(thread.into()),
            ..Self::default()
        };
    }

    pub fn close(&mut self) {
        *self = Self::default();
    }
}
