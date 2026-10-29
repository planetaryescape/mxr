//! `]` and `[` in the reader: archive the open conversation, then open the
//! next or previous one, as the web reader does. The archive is the list's
//! own (same request, bulk confirm and undo); the step waits for the daemon
//! to finish it, so a refused or failed archive leaves the reader in place.

use super::*;

/// The messages an archive request acts on.
pub(super) fn archive_request_ids(request: &Request) -> Option<&[MessageId]> {
    match request {
        Request::Mutation {
            mutation: MutationCommand::Archive { message_ids },
            ..
        } => Some(message_ids),
        _ => None,
    }
}

fn same_messages(left: &[MessageId], right: &[MessageId]) -> bool {
    left.len() == right.len() && left.iter().all(|id| right.contains(id))
}

impl App {
    pub(super) fn archive_and_step(&mut self, forward: bool) {
        let Some(thread) = self
            .mailbox
            .viewing_envelope
            .as_ref()
            .map(ThreadKey::of_envelope)
        else {
            return;
        };
        let message_ids = self.mutation_target_ids();
        if message_ids.is_empty() {
            return;
        }
        // Taken now: the archive may take this row out of the list, which
        // moves every index after it.
        let neighbour = self.reader_rows().and_then(|(rows, index)| {
            let target = if forward {
                index.checked_add(1)
            } else {
                index.checked_sub(1)
            }?;
            rows.get(target).map(|row| row.representative.id.clone())
        });
        let queued_before = self.pending_mutation_queue.len();
        self.mailbox.archive_step = None;
        self.apply_mutation_action(Action::Archive);

        let mut step = ArchiveStep {
            message_ids,
            thread,
            forward,
            neighbour,
            mutation: None,
        };
        let queued = (self.pending_mutation_queue.len() > queued_before)
            .then(|| self.pending_mutation_queue.last())
            .flatten()
            .filter(|queued| {
                archive_request_ids(&queued.request)
                    .is_some_and(|ids| same_messages(ids, &step.message_ids))
            })
            .map(|queued| queued.id);
        let confirming = self
            .modals
            .pending_bulk_confirm
            .as_ref()
            .is_some_and(|pending| {
                archive_request_ids(&pending.request)
                    .is_some_and(|ids| same_messages(ids, &step.message_ids))
            });
        if queued.is_some() || confirming {
            step.mutation = queued;
            self.mailbox.archive_step = Some(step);
        }
    }

    /// The bulk confirm queued the archive a `]` or `[` is waiting on.
    pub(super) fn adopt_archive_step(&mut self, id: MutationId, archived: &[MessageId]) {
        if let Some(step) = self.mailbox.archive_step.as_mut() {
            if step.mutation.is_none() && same_messages(&step.message_ids, archived) {
                step.mutation = Some(id);
            }
        }
    }

    /// The step waiting on mutation `id`, if any, and whether the reader
    /// still showed its conversation. Called before the completion is
    /// applied, which may open another conversation in the reader itself.
    pub(crate) fn take_archive_step(&mut self, id: MutationId) -> Option<(ArchiveStep, bool)> {
        if self.mailbox.archive_step.as_ref()?.mutation != Some(id) {
            return None;
        }
        let step = self.mailbox.archive_step.take()?;
        let reader_on_it = self
            .mailbox
            .viewing_envelope
            .as_ref()
            .is_some_and(|env| step.thread.matches_envelope(env));
        Some((step, reader_on_it))
    }

    /// Open the conversation the step was going to, once its archive is
    /// done. Someone who moved on while it ran keeps where they went.
    pub(crate) fn finish_archive_step(&mut self, step: ArchiveStep, reader_on_it: bool) {
        if !reader_on_it {
            return;
        }
        let Some((rows, _)) = self.reader_rows() else {
            // A mode's reader has no mail list here; n and N step it.
            self.apply(if step.forward {
                Action::NextSearchResult
            } else {
                Action::PrevSearchResult
            });
            return;
        };
        let Some(neighbour) = step.neighbour else {
            self.status_message = Some(if step.forward {
                "Archived; that was the last conversation".into()
            } else {
                "Archived; that was the first conversation".into()
            });
            return;
        };
        let Some(index) = rows
            .iter()
            .position(|row| row.representative.id == neighbour)
        else {
            return;
        };
        if self.screen == Screen::Search {
            self.search.page.selected_index = index;
            self.sync_search_cursor_after_move();
        } else {
            self.mailbox.selected_index = index;
            self.ensure_visible();
            self.auto_preview();
        }
    }

    /// The list the reader was opened from and the row it shows, when the
    /// reader sits beside a mail list.
    fn reader_rows(&self) -> Option<(Vec<MailListRow>, usize)> {
        match self.screen {
            Screen::Search => Some((
                self.search_mail_list_rows(),
                self.search.page.selected_index,
            )),
            Screen::Mailbox if self.selected_mail_row().is_some() => {
                Some((self.mail_list_rows(), self.mailbox.selected_index))
            }
            _ => None,
        }
    }
}
