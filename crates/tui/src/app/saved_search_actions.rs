use super::*;

impl App {
    pub(super) fn apply_saved_search_action(&mut self, action: Action) {
        match action {
            Action::OpenSavedSearchFormNew => {
                self.open_saved_search_form_new();
            }
            Action::OpenSavedSearchFormEdit => {
                let Some(SidebarItem::SavedSearch(search)) = self.selected_sidebar_item() else {
                    self.report_warn("Select a saved search to edit (j/k in the sidebar).");
                    return;
                };
                self.open_saved_search_form_for_edit(search.name, search.query, search.search_mode);
            }
            Action::SaveSavedSearchForm => {
                let Some(requests) = self.take_saved_search_form_requests() else {
                    // Form left open with `validation_error` set. Surface it
                    // through the warn channel so users notice the rejected
                    // submit even when the modal is wide.
                    if let Some(error) = self
                        .modals
                        .saved_search_form
                        .as_ref()
                        .and_then(|f| f.validation_error.clone())
                    {
                        self.report_warn(error);
                    }
                    return;
                };
                self.queue_saved_search_dispatch(requests);
            }
            Action::DeleteSavedSearch => {
                let Some(SidebarItem::SavedSearch(search)) = self.selected_sidebar_item() else {
                    self.report_warn("Select a saved search to delete (j/k in the sidebar).");
                    return;
                };
                self.modals.pending_saved_search_delete_confirm = Some(search.name);
            }
            _ => unreachable!("action routed to wrong handler"),
        }
    }

    /// Queue saved-search requests for the dispatcher loop in `lib.rs` to
    /// drain on its next iteration. Pairs with
    /// `take_pending_saved_search_dispatch` and
    /// `pending_saved_search_refresh` so a successful save triggers a
    /// refresh of the sidebar list.
    pub(crate) fn queue_saved_search_dispatch(&mut self, mut requests: Vec<mxr_protocol::Request>) {
        self.modals
            .pending_saved_search_dispatch
            .append(&mut requests);
    }

    /// Drain any queued saved-search requests for dispatch. Returns an
    /// empty Vec when the queue is empty so the caller can `is_empty`
    /// check without an extra option dance.
    pub(crate) fn take_pending_saved_search_dispatch(&mut self) -> Vec<mxr_protocol::Request> {
        std::mem::take(&mut self.modals.pending_saved_search_dispatch)
    }

    /// Confirm the pending delete and enqueue a `DeleteSavedSearch` for
    /// the dispatcher. Returns the name that was deleted, or `None` if
    /// no confirm was pending.
    pub fn confirm_pending_saved_search_delete(&mut self) -> Option<String> {
        let name = self.modals.pending_saved_search_delete_confirm.take()?;
        self.queue_saved_search_dispatch(vec![mxr_protocol::Request::DeleteSavedSearch {
            name: name.clone(),
        }]);
        Some(name)
    }

    /// Cancel the pending delete without dispatching anything.
    pub fn cancel_pending_saved_search_delete(&mut self) {
        self.modals.pending_saved_search_delete_confirm = None;
    }
}
