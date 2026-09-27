//! Label chips for mailbox and search rows.
//!
//! Envelopes carry provider label ids only. Rows need the resolved labels so
//! the web app can render user-label chips and show which labels a selection
//! already has. Labels are fetched once per account per request (seeded from
//! the list the mailbox chrome already loaded), never once per row.

use super::chrome::{MessageLabelView, MessageRowView};
use super::envelope_list::message_labels;
use super::*;

/// Labels of every account that appears in a page, keyed by account.
/// Provider label ids (`INBOX`, ...) repeat across accounts, so matching is
/// always done against the envelope's own account's labels.
#[derive(Debug, Default)]
pub(crate) struct LabelCatalog {
    by_account: HashMap<AccountId, Vec<Label>>,
}

impl LabelCatalog {
    /// `known` is a label list the handler already holds (the chrome's
    /// `ListLabels`); only accounts it does not cover cost an extra
    /// `ListLabels` round-trip. Labels are decoration, so a failed lookup
    /// leaves that account's rows without chips instead of failing the page.
    pub(crate) async fn load(socket_path: &Path, envelopes: &[Envelope], known: &[Label]) -> Self {
        let mut by_account = HashMap::<AccountId, Vec<Label>>::new();
        for label in known {
            by_account
                .entry(label.account_id.clone())
                .or_default()
                .push(label.clone());
        }
        let missing = envelopes
            .iter()
            .map(|envelope| envelope.account_id.clone())
            .filter(|account_id| !by_account.contains_key(account_id))
            .collect::<HashSet<_>>();
        for account_id in missing {
            match ipc_request(
                socket_path,
                Request::ListLabels {
                    account_id: Some(account_id.clone()),
                },
            )
            .await
            {
                Ok(ResponseData::Labels { labels }) => {
                    by_account.insert(account_id, labels);
                }
                Ok(_) => tracing::debug!(%account_id, "unexpected ListLabels response"),
                Err(error) => tracing::debug!(%account_id, %error, "ListLabels failed"),
            }
        }
        Self { by_account }
    }

    fn labels_for(&self, envelope: &Envelope) -> &[Label] {
        self.by_account
            .get(&envelope.account_id)
            .map_or(&[], Vec::as_slice)
    }
}

/// Fill `labels` on each row. Thread rows get the union of labels across
/// the thread's messages in `envelopes`; every other row (message,
/// attachment) gets the labels of the message it shows.
pub(crate) fn annotate_row_labels(
    rows: &mut [(DateTime<Utc>, MessageRowView)],
    envelopes: &[Envelope],
    catalog: &LabelCatalog,
) {
    let by_id = envelopes
        .iter()
        .map(|envelope| (envelope.id.to_string(), envelope))
        .collect::<HashMap<_, _>>();
    let mut by_thread = HashMap::<String, Vec<&Envelope>>::new();
    for envelope in envelopes {
        by_thread
            .entry(envelope.thread_id.to_string())
            .or_default()
            .push(envelope);
    }

    for (_, row) in rows {
        row.labels = if row.kind == "thread" {
            by_thread
                .get(&row.thread_id)
                .map(|members| union_labels(members, catalog))
                .unwrap_or_default()
        } else {
            by_id
                .get(&row.id)
                .map(|envelope| message_labels(envelope, catalog.labels_for(envelope)))
                .unwrap_or_default()
        };
    }
}

/// Union of a thread's labels in a stable order: system labels first, then
/// by name. Envelopes arrive in no guaranteed order, and first-seen order
/// made a row's label chips reshuffle between refreshes.
fn union_labels(envelopes: &[&Envelope], catalog: &LabelCatalog) -> Vec<MessageLabelView> {
    let mut seen = HashSet::new();
    let mut labels: Vec<MessageLabelView> = envelopes
        .iter()
        .flat_map(|envelope| message_labels(envelope, catalog.labels_for(envelope)))
        .filter(|label| seen.insert(label.id.clone()))
        .collect();
    labels.sort_by(|a, b| {
        let rank = |label: &MessageLabelView| match label.kind {
            "system" => 0,
            "folder" => 1,
            _ => 2,
        };
        rank(a)
            .cmp(&rank(b))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    labels
}
