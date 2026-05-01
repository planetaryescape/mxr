use super::*;

impl App {
    pub(super) fn apply_diagnostics_action(&mut self, action: Action) {
        match action {
            Action::RefreshDiagnostics => {
                self.diagnostics.page.refresh_pending = true;
            }
            Action::GenerateBugReport => {
                self.diagnostics.page.status = Some("Generating bug report...".into());
                self.diagnostics.pending_bug_report = true;
            }
            Action::EditConfig => {
                self.diagnostics.pending_config_edit = true;
                self.status_message = Some("Opening config in editor...".into());
            }
            Action::OpenLogs => {
                self.diagnostics.pending_log_open = true;
                self.status_message = Some("Opening log file in editor...".into());
            }
            Action::ShowOnboarding => {
                self.modals.onboarding.visible = true;
                self.modals.onboarding.step = 0;
            }
            Action::OpenDiagnosticsPaneDetails => {
                self.diagnostics.pending_details = Some(self.diagnostics.page.active_pane());
                self.status_message = Some("Opening diagnostics details...".into());
            }
            _ => unreachable!("action routed to wrong handler"),
        }
    }
}
