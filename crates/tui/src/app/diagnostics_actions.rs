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
            Action::OpenDiagnosticsPaneDetails => {
                self.diagnostics.pending_details = Some(self.diagnostics.page.active_pane());
                self.status_message = Some("Opening diagnostics details...".into());
            }
            Action::NextRecoveredDraft => {
                let count = self
                    .diagnostics
                    .page
                    .recovery
                    .as_ref()
                    .map_or(0, |r| r.held_drafts.len());
                if count > 0 {
                    self.diagnostics.page.selected_recovery_draft =
                        (self.diagnostics.page.selected_recovery_draft + 1) % count;
                }
            }
            Action::PreviousRecoveredDraft => {
                let count = self
                    .diagnostics
                    .page
                    .recovery
                    .as_ref()
                    .map_or(0, |r| r.held_drafts.len());
                if count > 0 {
                    self.diagnostics.page.selected_recovery_draft = self
                        .diagnostics
                        .page
                        .selected_recovery_draft
                        .checked_sub(1)
                        .unwrap_or(count - 1)
                        % count;
                }
            }
            Action::PreviewRecoveryActivation => {
                if let Some(recovery) = self
                    .diagnostics
                    .page
                    .recovery
                    .as_ref()
                    .filter(|r| r.restore_pending)
                {
                    let owner = mxr_protocol::ArchiveWorkspaceData {
                        workspace_id: recovery.workspace_id.clone(),
                        recovery_generation: recovery.recovery_generation.clone(),
                    };
                    self.diagnostics.pending_recovery_request = Some((
                        mxr_protocol::Request::ActivateRecovery {
                            expected_workspace: owner.clone(),
                            source_owner_stopped: true,
                            dry_run: true,
                        },
                        owner,
                    ));
                    self.status_message = Some(
                        "Previewing recovery activation. No old intentions will replay.".into(),
                    );
                } else {
                    self.status_message =
                        Some("No paused restored profile is available to activate.".into());
                }
            }
            Action::ConfirmRecoveryActivation => {
                let Some(recovery) = self
                    .diagnostics
                    .page
                    .recovery
                    .as_ref()
                    .filter(|r| r.restore_pending)
                else {
                    self.status_message = Some(
                        "Recovery is no longer paused; refresh status before continuing.".into(),
                    );
                    return;
                };
                let current = mxr_protocol::ArchiveWorkspaceData {
                    workspace_id: recovery.workspace_id.clone(),
                    recovery_generation: recovery.recovery_generation.clone(),
                };
                let Some(super::RecoveryPreview::Activation { owner }) =
                    self.diagnostics.page.recovery_preview.as_ref()
                else {
                    self.status_message = Some(
                        "Preview activation first, then confirm that the source owner is stopped."
                            .into(),
                    );
                    return;
                };
                if owner != &current {
                    self.diagnostics.page.recovery_preview = None;
                    self.status_message = Some(
                        "Selected workspace changed. Preview activation again before confirming."
                            .into(),
                    );
                    return;
                }
                self.diagnostics.pending_recovery_request = Some((
                    mxr_protocol::Request::ActivateRecovery {
                        expected_workspace: owner.clone(),
                        source_owner_stopped: true,
                        dry_run: false,
                    },
                    owner.clone(),
                ));
                self.status_message =
                    Some("Confirming source owner stopped and activating new work.".into());
            }
            Action::PreviewRecoveredDraft => {
                let Some(recovery) = self
                    .diagnostics
                    .page
                    .recovery
                    .as_ref()
                    .filter(|r| !r.restore_pending && r.source_owner_stopped)
                else {
                    self.status_message =
                        Some("Activate recovery before preparing a new draft.".into());
                    return;
                };
                let Some(draft) = recovery
                    .held_drafts
                    .get(self.diagnostics.page.selected_recovery_draft)
                else {
                    self.status_message = Some("No held original draft is selected.".into());
                    return;
                };
                if draft.prepared_draft_id.is_some() {
                    self.status_message =
                        Some("This original already has a prepared new draft.".into());
                    return;
                }
                let owner = mxr_protocol::ArchiveWorkspaceData {
                    workspace_id: recovery.workspace_id.clone(),
                    recovery_generation: recovery.recovery_generation.clone(),
                };
                self.diagnostics.pending_recovery_request = Some((
                    mxr_protocol::Request::PrepareRecoveredDraft {
                        draft_id: draft.draft_id.clone(),
                        expected_revision: draft.revision,
                        expected_workspace: owner.clone(),
                        dry_run: true,
                    },
                    owner,
                ));
                self.status_message =
                    Some("Previewing a separate local draft; the original remains held.".into());
            }
            Action::ConfirmRecoveredDraft => {
                let Some(recovery) = self
                    .diagnostics
                    .page
                    .recovery
                    .as_ref()
                    .filter(|r| !r.restore_pending && r.source_owner_stopped)
                else {
                    self.status_message =
                        Some("Refresh recovery status before confirming a prepared draft.".into());
                    return;
                };
                let current = mxr_protocol::ArchiveWorkspaceData {
                    workspace_id: recovery.workspace_id.clone(),
                    recovery_generation: recovery.recovery_generation.clone(),
                };
                let Some(super::RecoveryPreview::Draft {
                    owner,
                    draft_id,
                    revision,
                    ..
                }) = self.diagnostics.page.recovery_preview.as_ref()
                else {
                    self.status_message =
                        Some("Preview the selected draft before confirming.".into());
                    return;
                };
                let still_current = owner == &current
                    && recovery.held_drafts.iter().any(|d| {
                        &d.draft_id == draft_id
                            && d.revision == *revision
                            && d.prepared_draft_id.is_none()
                    });
                if !still_current {
                    self.diagnostics.page.recovery_preview = None;
                    self.status_message = Some("Selected owner or original revision changed. Preview the current draft again.".into());
                    return;
                }
                self.diagnostics.pending_recovery_request = Some((
                    mxr_protocol::Request::PrepareRecoveredDraft {
                        draft_id: draft_id.clone(),
                        expected_revision: *revision,
                        expected_workspace: owner.clone(),
                        dry_run: false,
                    },
                    owner.clone(),
                ));
                self.status_message = Some(
                    "Creating the separate local draft. It still requires normal send review."
                        .into(),
                );
            }
            _ => unreachable!("action routed to wrong handler"),
        }
    }
}
