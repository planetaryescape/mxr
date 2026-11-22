use mxr_core::id::ThreadId;
use mxr_core::ExportFormat;
use std::path::PathBuf;

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
    pub thread_id: ThreadId,
    pub format: ExportFormat,
    pub destination: ExportDestination,
}

#[derive(Debug, Clone, Default)]
pub struct ExportModalState {
    pub visible: bool,
    pub thread_id: Option<ThreadId>,
    pub stage: ExportStage,
    pub selected: usize,
    pub format: Option<ExportFormat>,
    pub input: String,
    pub error: Option<String>,
    pub awaiting_overwrite_confirm: bool,
}

impl ExportModalState {
    pub fn open(&mut self, thread_id: ThreadId) {
        *self = Self {
            visible: true,
            thread_id: Some(thread_id),
            ..Self::default()
        };
    }

    pub fn close(&mut self) {
        *self = Self::default();
    }
}
