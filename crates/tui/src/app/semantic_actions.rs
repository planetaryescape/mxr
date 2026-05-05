use super::*;

impl App {
    pub(super) fn apply_semantic_action(&mut self, action: Action) {
        match action {
            Action::EnableSemantic => {
                self.queue_semantic_request(mxr_protocol::Request::EnableSemantic {
                    enabled: true,
                });
                self.status_message = Some("Enabling semantic search...".into());
            }
            Action::DisableSemantic => {
                self.queue_semantic_request(mxr_protocol::Request::EnableSemantic {
                    enabled: false,
                });
                self.status_message = Some("Disabling semantic search...".into());
            }
            Action::ReindexSemantic => {
                self.queue_semantic_request(mxr_protocol::Request::ReindexSemantic);
                self.status_message = Some("Reindexing semantic search...".into());
            }
            Action::InstallSemanticProfile(profile) => {
                self.queue_semantic_request(mxr_protocol::Request::InstallSemanticProfile {
                    profile,
                });
                self.status_message = Some(format!(
                    "Installing semantic profile {}...",
                    profile.as_str()
                ));
            }
            _ => unreachable!("action routed to wrong handler"),
        }
    }

    /// Queue a semantic IPC request for the dispatcher to drain on its
    /// next iteration. One queue per request keeps the existing
    /// "drain in lib.rs" pattern consistent with saved-searches.
    pub(crate) fn queue_semantic_request(&mut self, request: mxr_protocol::Request) {
        self.modals.pending_semantic_dispatch.push(request);
    }

    pub(crate) fn take_pending_semantic_dispatch(&mut self) -> Vec<mxr_protocol::Request> {
        std::mem::take(&mut self.modals.pending_semantic_dispatch)
    }
}
