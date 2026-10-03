//! Reading and Paper trail in the TUI: open a place, pin exceptions, move a
//! sender to where it belongs, and sweep a bundle or the whole place after
//! the daemon's dry-run preview. The daemon owns every rule; this module
//! only turns keys into requests and keeps the lens honest meanwhile.

use super::*;
use mxr_protocol::{MailPlaceData, SenderKindData};

impl App {
    pub(super) fn apply_place_action(&mut self, action: Action) {
        match action {
            Action::OpenPlace(place) => self.open_place(place),
            Action::TogglePin => self.toggle_pin_on_selected_place_row(),
            Action::OpenSenderKindMenu => self.open_sender_kind_menu(),
            Action::SetSenderKind(kind) => self.set_sender_kind_from_menu(kind),
            Action::SweepBundle => self.preview_sweep(true),
            Action::SweepPlace => self.preview_sweep(false),
            Action::MorePlaceSenders => self.load_more_senders(),
            Action::MoreFromSender => self.load_more_from_sender(),
            _ => {}
        }
    }

    fn open_place(&mut self, place: MailPlaceData) {
        if self.mailbox.place_page.place != Some(place) {
            self.mailbox.place_page = PlacePageState::default();
        }
        self.mailbox.mailbox_view = MailboxView::Place(place);
        self.mailbox.active_label = None;
        self.mailbox.pending_active_label = None;
        self.mailbox.pending_label_fetch = None;
        self.mailbox.pending_preview_read = None;
        self.mailbox.desired_system_mailbox = None;
        self.search.active = false;
        self.screen = Screen::Mailbox;
        self.mailbox.active_pane = ActivePane::MailList;
        self.mailbox.selected_index = 0;
        self.mailbox.scroll_offset = 0;
        self.mailbox.pending_place_refresh = Some(place);
    }

    /// The place lens owns the keyboard (its list, not a reader beside it).
    pub(crate) fn place_list_focused(&self) -> bool {
        self.screen == Screen::Mailbox
            && matches!(self.mailbox.mailbox_view, MailboxView::Place(_))
            && self.mailbox.active_pane == ActivePane::MailList
    }

    /// The bundle and message under the cursor in the place lens.
    pub fn selected_place_row(
        &self,
    ) -> Option<(
        &mxr_protocol::PlaceBundleData,
        &mxr_protocol::PlaceMessageData,
    )> {
        self.mailbox
            .place_page
            .rows()
            .nth(self.mailbox.selected_index)
    }

    /// Fold a fetched page into the lens. Ignored if the user has moved on
    /// to another place meanwhile; the cursor is clamped to what is left.
    pub(crate) fn set_place(&mut self, fetch: &PlaceFetch, page: PlacePageState) {
        if page.place.map(MailboxView::Place) != Some(self.mailbox.mailbox_view) {
            return;
        }
        self.mailbox.place_page.apply(fetch, page);
        self.mailbox.selected_index = self
            .mailbox
            .selected_index
            .min(self.mailbox.place_page.row_count().saturating_sub(1));
    }

    /// Refetch the place on screen (after a pin, a correction, a sweep or
    /// any mutation) and the desk, whose counts the change may move.
    pub(crate) fn refresh_places(&mut self) {
        if let MailboxView::Place(place) = self.mailbox.mailbox_view {
            self.mailbox.pending_place_refresh = Some(place);
        }
        self.mailbox.pending_desk_refresh = true;
    }

    /// `e` on a place row: done in the mode the place backs (Updates for
    /// Paper trail, Reading for Reading). Returns false when no place row
    /// is under the cursor, so the verb runs as usual.
    pub(super) fn done_in_place_row(&mut self) -> bool {
        if !self.place_list_focused() {
            return false;
        }
        let MailboxView::Place(place) = self.mailbox.mailbox_view else {
            return false;
        };
        let Some((_, message)) = self.selected_place_row() else {
            return false;
        };
        let thread_id = message.thread_id.clone();
        let mode = match place {
            MailPlaceData::Reading => mxr_protocol::ModeKindData::Reading,
            MailPlaceData::PaperTrail => mxr_protocol::ModeKindData::Updates,
        };
        self.mailbox.place_page.remove_thread(&thread_id);
        self.mailbox.selected_index = self
            .mailbox
            .selected_index
            .min(self.mailbox.place_page.row_count().saturating_sub(1));
        self.queue_mode_done(mode, thread_id);
        true
    }

    /// Enter on a place row opens its message in the reader beside it.
    pub(super) fn open_selected_place_row(&mut self) {
        if let Some((_, message)) = self.selected_place_row() {
            self.mailbox.pending_invite_open = Some(message.message_id.clone());
            self.status_message = Some("Opening…".into());
        }
    }

    fn toggle_pin_on_selected_place_row(&mut self) {
        if !self.place_list_focused() {
            self.status_message = Some("Pin works in Reading and Paper trail".into());
            return;
        }
        let Some((_, message)) = self.selected_place_row() else {
            return;
        };
        let message_id = message.message_id.clone();
        let pinned = !message.pinned;
        self.mailbox.place_page.set_pinned(&message_id, pinned);
        self.queue_mutation(
            Request::PinMessages {
                message_ids: vec![message_id],
                pinned,
            },
            MutationEffect::RefreshPlaces(
                if pinned {
                    "Pinned: a sweep leaves it here"
                } else {
                    "Unpinned"
                }
                .into(),
            ),
            if pinned { "Pinning..." } else { "Unpinning..." }.into(),
        );
    }

    fn open_sender_kind_menu(&mut self) {
        if !self.place_list_focused() {
            self.status_message =
                Some("Move a sender from Reading or Paper trail (g r, g p)".into());
            return;
        }
        let Some((bundle, _)) = self.selected_place_row() else {
            return;
        };
        self.mailbox.sender_kind_menu = Some(SenderKindMenu {
            account_id: bundle.account_id.clone(),
            sender_email: bundle.sender_email.clone(),
            display: bundle
                .sender_name
                .clone()
                .unwrap_or_else(|| bundle.sender_email.clone()),
            current: bundle.kind.clone(),
        });
    }

    fn set_sender_kind_from_menu(&mut self, kind: Option<SenderKindData>) {
        let Some(menu) = self.mailbox.sender_kind_menu.take() else {
            return;
        };
        let destination = match kind {
            Some(SenderKindData::People) => "people",
            Some(SenderKindData::Reading) => "Reading",
            Some(SenderKindData::PaperTrail) => "Paper trail",
            Some(SenderKindData::ScreenedOut) => "screened out",
            None => "automatic sorting",
        };
        self.queue_mutation(
            Request::SetSenderKind {
                account_id: menu.account_id,
                sender_email: menu.sender_email,
                kind,
            },
            MutationEffect::SenderMoved(format!(
                "Moved {} to {destination}",
                crate::ui::sanitize::strip_control_chars(&menu.display)
            )),
            "Moving sender...".into(),
        );
    }

    /// Ask the daemon what a sweep would archive; the answer opens the
    /// confirmation. `bundle`: only the sender under the cursor.
    fn preview_sweep(&mut self, bundle: bool) {
        let MailboxView::Place(place) = self.mailbox.mailbox_view else {
            self.status_message = Some("Sweep works in Reading and Paper trail".into());
            return;
        };
        let target = if bundle {
            let Some((row_bundle, _)) = self.selected_place_row() else {
                return;
            };
            SweepTarget {
                place,
                account_id: Some(row_bundle.account_id.clone()),
                sender_email: Some(row_bundle.sender_email.clone()),
            }
        } else {
            SweepTarget {
                place,
                account_id: None,
                sender_email: None,
            }
        };
        self.mailbox.pending_sweep_preview = Some(target);
        self.status_message = Some("Checking what a sweep would archive…".into());
    }

    /// The runtime's dry-run answer: show it for a yes, or say there is
    /// nothing to sweep. Notes how much of it the lens has loaded, since a
    /// sweep reaches mail not yet shown.
    pub(crate) fn show_sweep_preview(&mut self, mut confirm: PendingSweepConfirm) {
        self.status_message = None;
        if confirm.preview.count == 0 {
            self.push_toast(Toast::success("Nothing to sweep: pinned mail stays"));
            return;
        }
        confirm.shown = self
            .mailbox
            .place_page
            .rows()
            .filter(|(bundle, message)| {
                !message.pinned
                    && confirm
                        .target
                        .account_id
                        .as_ref()
                        .is_none_or(|a| a == &bundle.account_id)
                    && confirm
                        .target
                        .sender_email
                        .as_ref()
                        .is_none_or(|s| s == &bundle.sender_email)
            })
            .count() as u32;
        confirm.sweep_focused = confirm.target.sender_email.is_some();
        self.mailbox.sweep_confirm = Some(confirm);
    }

    /// `>`: the next page of senders, when there are more.
    fn load_more_senders(&mut self) {
        let MailboxView::Place(place) = self.mailbox.mailbox_view else {
            return;
        };
        if !self.mailbox.place_page.has_more_senders() {
            self.status_message = Some("Every sender is shown".into());
            return;
        }
        self.mailbox.pending_place_more = Some(PlaceFetch::MoreSenders {
            place,
            offset: self.mailbox.place_page.bundles.len() as u32,
        });
    }

    /// `+`: the next page of the sender under the cursor.
    fn load_more_from_sender(&mut self) {
        let MailboxView::Place(place) = self.mailbox.mailbox_view else {
            return;
        };
        let Some((bundle, _)) = self.selected_place_row() else {
            return;
        };
        if bundle.messages.len() as u32 >= bundle.message_count {
            self.status_message = Some("All of this sender's mail is shown".into());
            return;
        }
        self.mailbox.pending_place_more = Some(PlaceFetch::MoreFromSender {
            place,
            account_id: bundle.account_id.clone(),
            sender_email: bundle.sender_email.clone(),
            message_offset: bundle.messages.len() as u32,
        });
    }

    /// Confirm the preview: run exactly the previewed selection, or cancel
    /// when Cancel has focus.
    pub(crate) fn confirm_sweep(&mut self) {
        if self
            .mailbox
            .sweep_confirm
            .as_ref()
            .is_some_and(|confirm| !confirm.sweep_focused)
        {
            self.mailbox.sweep_confirm = None;
            self.status_message = Some("Sweep cancelled".into());
            return;
        }
        if let Some(confirm) = self.mailbox.sweep_confirm.take() {
            self.status_message = Some(format!("Sweeping {}…", confirm.preview.count));
            self.mailbox.pending_sweep = Some(confirm);
        }
    }

    /// The sweep's archive job finished: offer undo over every chunk and
    /// refetch what it changed.
    pub(crate) fn finish_sweep(&mut self, archived: u32, undo_ids: Vec<String>) {
        self.status_message = None;
        if undo_ids.is_empty() {
            self.push_toast(Toast::success(format!("Swept {archived}")));
        } else {
            self.set_pending_undo(PendingUndo {
                action: UndoAction::Mutations(undo_ids),
                verb_past: "Swept".into(),
                count: archived,
                applied_at: std::time::Instant::now(),
            });
        }
        self.refresh_places();
        self.mailbox.pending_subscriptions_refresh = true;
    }
}
