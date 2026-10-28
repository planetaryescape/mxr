use super::*;
use crate::ui::sanitize::one_line;
use mxr_protocol::MutationCommand;
use std::collections::HashSet;

/// The message ids a request will mutate. The confirm preview reads these,
/// so the preview and the mutation cannot disagree.
pub(crate) fn request_message_ids(request: &Request) -> Vec<MessageId> {
    let Request::Mutation { mutation, .. } = request else {
        return Vec::new();
    };
    let ids = match mutation {
        MutationCommand::Archive { message_ids }
        | MutationCommand::ReadAndArchive { message_ids }
        | MutationCommand::Trash { message_ids }
        | MutationCommand::Spam { message_ids }
        | MutationCommand::Star { message_ids, .. }
        | MutationCommand::SetRead { message_ids, .. }
        | MutationCommand::ModifyLabels { message_ids, .. }
        | MutationCommand::Move { message_ids, .. }
        | MutationCommand::Route { message_ids, .. } => message_ids,
    };
    ids.clone()
}

impl App {
    /// Rows come out in the order the list shows them, not the order the
    /// selection set happens to iterate in.
    pub(crate) fn bulk_preview(&self, request: &Request) -> BulkPreview {
        let wanted: HashSet<MessageId> = request_message_ids(request).into_iter().collect();
        let source: &[Envelope] = if self.screen == Screen::Search {
            &self.search.page.results
        } else {
            self.mailbox_list_envelopes()
        };
        let mut seen: HashSet<&MessageId> = HashSet::new();
        let rows: Vec<BulkPreviewRow> = source
            .iter()
            .filter(|env| wanted.contains(&env.id) && seen.insert(&env.id))
            .map(|env| {
                // Sender and subject are mail-controlled text; strip terminal
                // controls the same way the mail list does.
                let subject = one_line(&env.subject);
                BulkPreviewRow {
                    message_id: env.id.clone(),
                    sender: one_line(
                        env.from
                            .name
                            .as_deref()
                            .filter(|name| !name.is_empty())
                            .unwrap_or(&env.from.email),
                    ),
                    subject: if subject.trim().is_empty() {
                        "(no subject)".to_string()
                    } else {
                        subject
                    },
                }
            })
            .collect();
        BulkPreview {
            not_in_view: wanted.len().saturating_sub(rows.len()),
            rows,
        }
    }
}
