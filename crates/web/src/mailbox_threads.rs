//! Thread-aware mailbox paging for `/mail/mailbox?view=threads`.
//!
//! Paging envelopes and grouping them by thread in the bridge splits one
//! thread across pages and makes `message_count` page-local. For the lenses
//! the daemon can filter server-side (Inbox, All Mail, Label) the bridge pages
//! whole threads with `Request::ListThreads` and hydrates each row from the
//! thread's envelopes. Saved-search and subscription lenses have no thread
//! listing in the daemon, so they keep the envelope path.

use super::chrome::{derived_counts, find_inbox_label, matches_system_label, BridgeChrome};
use super::chrome::{MessageRowView, THREAD_ROW_PARTICIPANT_LIMIT};
use super::envelope_list::{list_envelopes_by_message_ids, message_row_view};
use super::row_labels::{annotate_row_labels, LabelCatalog};
use super::*;
use mxr_core::{MessageFlags, Thread};

/// Envelope lookups are batched so a page of very long threads never builds
/// one oversized IPC frame.
const ENVELOPE_BATCH: usize = 500;

pub(crate) struct ThreadPage {
    pub(crate) lens_label: String,
    pub(crate) counts: serde_json::Value,
    pub(crate) rows: Vec<(DateTime<Utc>, MessageRowView)>,
    /// Envelope behind each row, for per-row annotations.
    pub(crate) row_envelopes: Vec<Envelope>,
    /// Threads the daemon returned; drives `has_more` in thread units.
    pub(crate) thread_count: usize,
}

/// The lens-specific inputs to a thread listing.
struct ThreadLens<'a> {
    label_filter: Option<LabelId>,
    /// Label whose members count as "in the lens". `None` means every message
    /// of the thread belongs (All Mail, or an account with no inbox label).
    member_label: Option<&'a Label>,
    count_label: Option<&'a Label>,
    lens_label: String,
}

fn thread_lens<'a>(
    chrome: &'a BridgeChrome,
    lens: &MailboxLensRequest,
) -> Result<Option<ThreadLens<'a>>, BridgeError> {
    Ok(Some(match lens.kind {
        MailboxLensKind::Inbox => {
            let inbox = find_inbox_label(&chrome.labels);
            ThreadLens {
                label_filter: chrome.inbox_label_id.clone(),
                member_label: inbox,
                count_label: inbox,
                lens_label: inbox.map_or_else(|| "Inbox".to_string(), |label| label.name.clone()),
            }
        }
        MailboxLensKind::AllMail => ThreadLens {
            label_filter: None,
            member_label: None,
            count_label: chrome
                .labels
                .iter()
                .find(|label| matches_system_label(label, "All Mail")),
            lens_label: "All Mail".to_string(),
        },
        MailboxLensKind::Label => {
            let label_id = lens
                .label_id
                .as_deref()
                .ok_or_else(|| BridgeError::BadRequest("label lens missing label_id".into()))
                .and_then(parse_label_id)?;
            let label = chrome
                .labels
                .iter()
                .find(|candidate| candidate.id == label_id);
            ThreadLens {
                label_filter: Some(label_id),
                member_label: label,
                count_label: label,
                lens_label: label.map_or_else(|| "Label".to_string(), |label| label.name.clone()),
            }
        }
        MailboxLensKind::SavedSearch | MailboxLensKind::Subscription => return Ok(None),
    }))
}

/// Returns `None` for lenses that have no daemon-side thread listing; the
/// caller falls back to grouping an envelope page.
pub(crate) async fn load_thread_page(
    socket_path: &Path,
    chrome: &BridgeChrome,
    lens: &MailboxLensRequest,
    account_id: Option<&AccountId>,
    limit: u32,
    offset: u32,
) -> Result<Option<ThreadPage>, BridgeError> {
    let Some(thread_lens) = thread_lens(chrome, lens)? else {
        return Ok(None);
    };
    let threads = match ipc_request(
        socket_path,
        Request::ListThreads {
            account_id: account_id.cloned(),
            label_id: thread_lens.label_filter.clone(),
            limit,
            offset,
            sort: Some(SortOrder::DateDesc),
        },
    )
    .await?
    {
        ResponseData::Threads { threads } => threads,
        _ => return Err(BridgeError::UnexpectedResponse),
    };

    let message_ids = threads
        .iter()
        .flat_map(|thread| thread.message_ids.iter().cloned())
        .collect::<Vec<_>>();
    let mut envelopes_by_id = HashMap::with_capacity(message_ids.len());
    for batch in message_ids.chunks(ENVELOPE_BATCH) {
        for envelope in list_envelopes_by_message_ids(socket_path, batch).await? {
            envelopes_by_id.insert(envelope.id.clone(), envelope);
        }
    }

    let mut rows = Vec::with_capacity(threads.len());
    let mut row_envelopes = Vec::with_capacity(threads.len());
    for thread in &threads {
        let envelopes = thread
            .message_ids
            .iter()
            .filter_map(|id| envelopes_by_id.get(id))
            .collect::<Vec<_>>();
        match thread_row(thread, &envelopes, thread_lens.member_label) {
            Some((date, row, latest)) => {
                rows.push((date, row));
                row_envelopes.push(latest.clone());
            }
            None => tracing::debug!(
                thread_id = %thread.id,
                "thread listed without any stored envelope; skipping row"
            ),
        }
    }

    let page_envelopes = envelopes_by_id.values().cloned().collect::<Vec<_>>();
    let catalog = LabelCatalog::load(socket_path, &page_envelopes, &chrome.labels).await;
    annotate_row_labels(&mut rows, &page_envelopes, &catalog);

    let counts = thread_lens.count_label.map_or_else(
        || derived_counts(&page_envelopes),
        |label| {
            json!({
                "unread": label.unread_count,
                "total": label.total_count,
            })
        },
    );

    Ok(Some(ThreadPage {
        lens_label: thread_lens.lens_label,
        counts,
        rows,
        row_envelopes,
        thread_count: threads.len(),
    }))
}

fn envelope_has_label(envelope: &Envelope, label: &Label) -> bool {
    envelope
        .label_provider_ids
        .iter()
        .any(|id| id == &label.provider_id || id == &label.name)
}

/// Build one thread row. `message_ids` lists the thread's messages that sit
/// in the lens (for Inbox: the ones still carrying the inbox label), so a
/// thread-level action touches exactly what the row represents. When no
/// envelope carries the lens label (label ids out of step with the store)
/// the whole thread is used rather than an empty row. The row's id, sender,
/// subject, snippet and date come from the newest in-lens message, which is
/// also what the daemon sorted the page by.
fn thread_row<'a>(
    thread: &Thread,
    envelopes: &[&'a Envelope],
    member_label: Option<&Label>,
) -> Option<(DateTime<Utc>, MessageRowView, &'a Envelope)> {
    let in_lens = member_label
        .map(|label| {
            envelopes
                .iter()
                .copied()
                .filter(|envelope| envelope_has_label(envelope, label))
                .collect::<Vec<_>>()
        })
        .filter(|members| !members.is_empty())
        .unwrap_or_else(|| envelopes.to_vec());
    // `max_by_key` keeps the last of equal dates, matching the store's
    // `date ASC, id ASC` member order.
    let latest = *in_lens.iter().max_by_key(|envelope| envelope.date)?;

    let mut row = message_row_view(latest);
    row.kind = "thread";
    row.message_count = Some(thread.message_count);
    row.message_ids = Some(
        in_lens
            .iter()
            .map(|envelope| envelope.id.to_string())
            .collect(),
    );
    row.participants = Some(
        thread
            .participants
            .iter()
            .take(THREAD_ROW_PARTICIPANT_LIMIT)
            .cloned()
            .collect(),
    );
    row.unread = thread.unread_count > 0;
    let starred_message_ids = envelopes
        .iter()
        .filter(|envelope| envelope.flags.contains(MessageFlags::STARRED))
        .map(|envelope| envelope.id.to_string())
        .collect::<Vec<_>>();
    row.starred = !starred_message_ids.is_empty();
    row.starred_message_ids = Some(starred_message_ids);
    row.has_attachments = envelopes.iter().any(|envelope| envelope.has_attachments);
    Some((latest.date, row, latest))
}
