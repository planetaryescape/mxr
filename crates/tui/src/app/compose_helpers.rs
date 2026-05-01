use super::*;

impl App {
    pub(crate) fn schedule_draft_cleanup(&mut self, path: std::path::PathBuf) {
        if !self.compose.pending_draft_cleanup.contains(&path) {
            self.compose.pending_draft_cleanup.push(path);
        }
    }

    pub(crate) fn take_pending_draft_cleanup(&mut self) -> Vec<std::path::PathBuf> {
        std::mem::take(&mut self.compose.pending_draft_cleanup)
    }
}
