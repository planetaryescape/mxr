use super::chrome::{MessageGroupView, MessageRowView};
use super::*;
use chrono::Datelike;
use mxr_core::{MessageFlags, SavedSearch};
use mxr_protocol::SearchResultItem;
use std::collections::{HashMap, HashSet};

pub(crate) async fn list_envelopes(
    socket_path: &Path,
    label_id: Option<LabelId>,
    limit: u32,
    offset: u32,
) -> Result<Vec<Envelope>, BridgeError> {
    match ipc_request(
        socket_path,
        Request::ListEnvelopes {
            label_id,
            account_id: None,
            limit,
            offset,
        },
    )
    .await?
    {
        ResponseData::Envelopes { envelopes } => Ok(envelopes),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

pub(crate) async fn list_envelopes_by_message_ids(
    socket_path: &Path,
    message_ids: &[MessageId],
) -> Result<Vec<Envelope>, BridgeError> {
    if message_ids.is_empty() {
        return Ok(Vec::new());
    }
    match ipc_request(
        socket_path,
        Request::ListEnvelopesByIds {
            message_ids: message_ids.to_vec(),
        },
    )
    .await?
    {
        ResponseData::Envelopes { envelopes } => Ok(reorder_envelopes(envelopes, message_ids)),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

pub(crate) async fn run_saved_search(
    socket_path: &Path,
    name: &str,
    limit: u32,
) -> Result<Vec<Envelope>, BridgeError> {
    match ipc_request(
        socket_path,
        Request::RunSavedSearch {
            name: name.to_string(),
            limit,
        },
    )
    .await?
    {
        ResponseData::SearchResults { results, .. } => {
            search_result_envelopes(socket_path, &results).await
        }
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

pub(crate) async fn search_envelopes(
    socket_path: &Path,
    query: &str,
    limit: u32,
) -> Result<Vec<Envelope>, BridgeError> {
    match ipc_request(
        socket_path,
        Request::Search {
            query: query.to_string(),
            limit,
            offset: 0,
            mode: Some(SearchMode::Lexical),
            sort: Some(SortOrder::DateDesc),
            explain: false,
        },
    )
    .await?
    {
        ResponseData::SearchResults { results, .. } => {
            search_result_envelopes(socket_path, &results).await
        }
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

pub(crate) async fn search_result_envelopes(
    socket_path: &Path,
    results: &[SearchResultItem],
) -> Result<Vec<Envelope>, BridgeError> {
    let message_ids = results
        .iter()
        .map(|result| result.message_id.clone())
        .collect::<Vec<_>>();
    if message_ids.is_empty() {
        return Ok(Vec::new());
    }
    match ipc_request(
        socket_path,
        Request::ListEnvelopesByIds {
            message_ids: message_ids.clone(),
        },
    )
    .await?
    {
        ResponseData::Envelopes { envelopes } => Ok(reorder_envelopes(envelopes, &message_ids)),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

pub(crate) fn group_envelopes(envelopes: Vec<Envelope>) -> Vec<MessageGroupView> {
    // Grouping is a web presentation choice, not daemon protocol.
    let mut groups = Vec::<MessageGroupView>::new();

    for envelope in envelopes {
        let (group_id, label) = date_bucket(envelope.date);
        let row = message_row_view(&envelope);
        if let Some(existing) = groups.iter_mut().find(|group| group.id == group_id) {
            existing.rows.push(row);
        } else {
            groups.push(MessageGroupView {
                id: group_id.to_string(),
                label: label.to_string(),
                rows: vec![row],
            });
        }
    }

    groups
}

pub(crate) fn date_bucket(date: DateTime<Utc>) -> (&'static str, &'static str) {
    let local = date.with_timezone(&Local);
    let today = Local::now().date_naive();
    let days_old = today.signed_duration_since(local.date_naive()).num_days();

    match days_old {
        0 => ("today", "Today"),
        1 => ("yesterday", "Yesterday"),
        2..=6 => ("last-7-days", "Last 7 Days"),
        _ if local.year() == today.year() => ("earlier", "Earlier"),
        _ => ("older", "Older"),
    }
}

pub(crate) fn message_row_view(envelope: &Envelope) -> MessageRowView {
    MessageRowView {
        id: envelope.id.to_string(),
        thread_id: envelope.thread_id.to_string(),
        provider_id: envelope.provider_id.clone(),
        sender: envelope
            .from
            .name
            .clone()
            .unwrap_or_else(|| envelope.from.email.clone()),
        sender_detail: Some(envelope.from.email.clone()),
        subject: envelope.subject.clone(),
        snippet: envelope.snippet.clone(),
        date_label: format_date_label(envelope.date),
        unread: !envelope.flags.contains(MessageFlags::READ),
        starred: envelope.flags.contains(MessageFlags::STARRED),
        has_attachments: envelope.has_attachments,
    }
}

pub(crate) fn format_date_label(date: DateTime<Utc>) -> String {
    let local = date.with_timezone(&Local);
    let today = Local::now().date_naive();
    if today == local.date_naive() {
        return local.format("%-I:%M%P").to_string();
    }
    local.format("%b %-d").to_string()
}

pub(crate) fn thread_reader_mode(bodies: &[MessageBody]) -> &'static str {
    let has_plain = bodies.iter().any(|body| body.text_plain.as_ref().is_some());
    let has_html = bodies.iter().any(|body| body.text_html.as_ref().is_some());
    if has_html && !has_plain {
        "html"
    } else {
        "reader"
    }
}

pub(crate) fn reorder_envelopes(envelopes: Vec<Envelope>, order: &[MessageId]) -> Vec<Envelope> {
    let mut by_id = HashMap::new();
    for envelope in envelopes {
        by_id.insert(envelope.id.clone(), envelope);
    }

    order.iter().filter_map(|id| by_id.remove(id)).collect()
}

pub(crate) fn dedupe_search_results_by_thread(
    results: Vec<SearchResultItem>,
) -> Vec<SearchResultItem> {
    let mut seen = HashSet::new();
    results
        .into_iter()
        .filter(|result| seen.insert(result.thread_id.clone()))
        .collect()
}

pub(crate) fn slugify(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

pub(crate) fn sorted_saved_searches(mut searches: Vec<SavedSearch>) -> Vec<SavedSearch> {
    searches.sort_by_key(|search| search.position);
    searches
}
