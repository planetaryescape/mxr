use super::chrome::{MessageGroupView, MessageLabelView, MessageRowView};
use super::*;
use chrono::{Datelike, Local};
use mxr_core::{Label, LabelKind, MessageFlags, SavedSearch};
use mxr_protocol::SearchResultItem;
use std::collections::{HashMap, HashSet};

pub(crate) async fn list_envelopes(
    socket_path: &Path,
    label_id: Option<LabelId>,
    account_id: Option<&AccountId>,
    limit: u32,
    offset: u32,
) -> Result<Vec<Envelope>, BridgeError> {
    match ipc_request(
        socket_path,
        Request::ListEnvelopes {
            label_id,
            account_id: account_id.cloned(),
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

pub(crate) async fn list_bodies_by_message_ids(
    socket_path: &Path,
    message_ids: &[MessageId],
) -> Result<Vec<MessageBody>, BridgeError> {
    if message_ids.is_empty() {
        return Ok(Vec::new());
    }
    match ipc_request(
        socket_path,
        Request::ListBodies {
            message_ids: message_ids.to_vec(),
        },
    )
    .await?
    {
        ResponseData::Bodies { bodies, .. } => Ok(reorder_bodies(bodies, message_ids)),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

pub(crate) async fn run_saved_search(
    socket_path: &Path,
    name: &str,
    account_id: Option<&AccountId>,
    limit: u32,
) -> Result<Vec<Envelope>, BridgeError> {
    match ipc_request(
        socket_path,
        Request::RunSavedSearch {
            name: name.to_string(),
            limit,
            account_id: account_id.cloned(),
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
    account_id: Option<&AccountId>,
    limit: u32,
    offset: u32,
) -> Result<Vec<Envelope>, BridgeError> {
    match ipc_request(
        socket_path,
        Request::Search {
            query: query.to_string(),
            limit,
            offset,
            account_id: account_id.cloned(),
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

pub(crate) fn group_envelopes(
    envelopes: Vec<Envelope>,
    catalog: &super::row_labels::LabelCatalog,
) -> Vec<MessageGroupView> {
    let mut rows = envelopes
        .iter()
        .map(|envelope| (envelope.date, message_row_view(envelope)))
        .collect::<Vec<_>>();
    super::row_labels::annotate_row_labels(&mut rows, &envelopes, catalog);
    group_row_views(rows)
}

pub(crate) fn group_row_views(rows: Vec<(DateTime<Utc>, MessageRowView)>) -> Vec<MessageGroupView> {
    // Grouping is a web presentation choice, not daemon protocol.
    let mut groups = Vec::<MessageGroupView>::new();

    for (date, row) in rows {
        let (group_id, label) = date_bucket(date);
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

pub(crate) fn mailbox_message_rows(
    envelopes: Vec<Envelope>,
) -> Vec<(DateTime<Utc>, MessageRowView)> {
    envelopes
        .into_iter()
        .map(|envelope| {
            let date = envelope.date;
            let mut row = message_row_view(&envelope);
            row.kind = "message";
            (date, row)
        })
        .collect()
}

/// Envelope-page fallback for lenses without a daemon thread listing
/// (saved searches, subscriptions). Counts, `message_ids` and participants
/// are page-local here.
pub(crate) fn mailbox_thread_rows(
    envelopes: Vec<Envelope>,
) -> Vec<(DateTime<Utc>, MessageRowView)> {
    let mut message_counts = HashMap::new();
    let mut thread_has_attachments = HashMap::new();
    let mut thread_message_ids = HashMap::<ThreadId, Vec<String>>::new();
    let mut thread_participants = HashMap::<ThreadId, Vec<mxr_core::Address>>::new();
    for envelope in &envelopes {
        *message_counts
            .entry(envelope.thread_id.clone())
            .or_insert(0_u32) += 1;
        *thread_has_attachments
            .entry(envelope.thread_id.clone())
            .or_insert(false) |= envelope.has_attachments;
        thread_message_ids
            .entry(envelope.thread_id.clone())
            .or_default()
            .push(envelope.id.to_string());
        let participants = thread_participants
            .entry(envelope.thread_id.clone())
            .or_default();
        if participants.len() < super::chrome::THREAD_ROW_PARTICIPANT_LIMIT
            && !participants
                .iter()
                .any(|known| known.email.eq_ignore_ascii_case(&envelope.from.email))
        {
            participants.push(envelope.from.clone());
        }
    }

    let mut seen = HashSet::new();
    envelopes
        .into_iter()
        .filter_map(|envelope| {
            if !seen.insert(envelope.thread_id.clone()) {
                return None;
            }
            let date = envelope.date;
            let mut row = message_row_view(&envelope);
            row.kind = "thread";
            row.message_count = message_counts.get(&envelope.thread_id).copied();
            row.message_ids = thread_message_ids.remove(&envelope.thread_id);
            row.participants = thread_participants.remove(&envelope.thread_id);
            row.has_attachments = thread_has_attachments
                .get(&envelope.thread_id)
                .copied()
                .unwrap_or(envelope.has_attachments);
            Some((date, row))
        })
        .collect()
}

pub(crate) fn attachment_search_rows(
    envelopes: &[Envelope],
    bodies: &[MessageBody],
) -> Vec<(DateTime<Utc>, MessageRowView)> {
    let envelopes_by_id = envelopes
        .iter()
        .map(|envelope| (envelope.id.clone(), envelope))
        .collect::<HashMap<_, _>>();

    let mut rows = Vec::new();
    for body in bodies {
        let Some(envelope) = envelopes_by_id.get(&body.message_id) else {
            continue;
        };
        for attachment in &body.attachments {
            let mut row = message_row_view(envelope);
            row.kind = "attachment";
            row.has_attachments = true;
            row.attachment_id = Some(attachment.id.to_string());
            row.attachment_filename = Some(attachment.filename.clone());
            row.attachment_size_bytes = Some(attachment.size_bytes);
            row.snippet = format!("{} · {} bytes", attachment.mime_type, attachment.size_bytes);
            rows.push((envelope.date, row));
        }
    }
    rows
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
        kind: "message",
        account_id: envelope.account_id.to_string(),
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
        date: envelope.date,
        date_label: format_date_label(envelope.date),
        date_full: format_date_full(envelope.date),
        date_relative: format_relative_label(envelope.date),
        to: envelope.to.clone(),
        cc: envelope.cc.clone(),
        bcc: envelope.bcc.clone(),
        labels: Vec::new(),
        unread: !envelope.flags.contains(MessageFlags::READ),
        starred: envelope.flags.contains(MessageFlags::STARRED),
        has_attachments: envelope.has_attachments,
        message_count: None,
        message_ids: None,
        participants: None,
        attachment_id: None,
        attachment_filename: None,
        attachment_size_bytes: None,
        open_commitment_count: None,
        triage_verdict: None,
        triage_reason: None,
        triage_line: None,
    }
}

pub(crate) fn message_row_view_with_labels(
    envelope: &Envelope,
    labels: &[Label],
) -> MessageRowView {
    let mut row = message_row_view(envelope);
    row.labels = message_labels(envelope, labels);
    row
}

pub(crate) fn format_date_label(date: DateTime<Utc>) -> String {
    let local = date.with_timezone(&Local);
    let today = Local::now().date_naive();
    if today == local.date_naive() {
        return local.format("%-I:%M%P").to_string();
    }
    if local.year() == today.year() {
        local.format("%b %-d %-I:%M%P").to_string()
    } else {
        local.format("%b %-d, %Y %-I:%M%P").to_string()
    }
}

pub(crate) fn format_date_full(date: DateTime<Utc>) -> String {
    date.with_timezone(&Local)
        .format("%b %-d, %Y, %-I:%M %p")
        .to_string()
}

pub(crate) fn format_relative_label(date: DateTime<Utc>) -> String {
    let seconds = Utc::now().signed_duration_since(date).num_seconds().max(0);
    if seconds < 60 {
        return "just now".to_string();
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return plural(minutes, "minute");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return plural(hours, "hour");
    }
    let days = hours / 24;
    if days < 7 {
        return plural(days, "day");
    }
    format_date_label(date)
}

fn plural(value: i64, unit: &str) -> String {
    if value == 1 {
        format!("1 {unit} ago")
    } else {
        format!("{value} {unit}s ago")
    }
}

pub(crate) fn message_labels(envelope: &Envelope, labels: &[Label]) -> Vec<MessageLabelView> {
    if envelope.label_provider_ids.is_empty() {
        return Vec::new();
    }
    let provider_ids = envelope
        .label_provider_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    labels
        .iter()
        .filter(|label| {
            provider_ids.contains(label.provider_id.as_str())
                || provider_ids.contains(label.name.as_str())
        })
        .map(|label| MessageLabelView {
            id: label.id.to_string(),
            name: label.name.clone(),
            kind: match label.kind {
                LabelKind::System => "system",
                LabelKind::Folder => "folder",
                LabelKind::User => "user",
            },
            color: label.color.clone(),
        })
        .collect()
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

pub(crate) fn reorder_bodies(bodies: Vec<MessageBody>, order: &[MessageId]) -> Vec<MessageBody> {
    let mut by_id = HashMap::new();
    for body in bodies {
        by_id.insert(body.message_id.clone(), body);
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

/// URL slug for a label or saved search. Letters and digits in any script
/// survive: an ASCII-only slug turned "中文" into "" and made labels that
/// differ only in non-ASCII characters share one URL.
pub(crate) fn slugify(value: &str) -> String {
    let mut slug = String::with_capacity(value.len());
    for ch in value.chars().flat_map(char::to_lowercase) {
        if ch.is_alphanumeric() {
            slug.push(ch);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_matches('-').to_string()
}

pub(crate) fn sorted_saved_searches(mut searches: Vec<SavedSearch>) -> Vec<SavedSearch> {
    searches.sort_by_key(|search| search.position);
    searches
}
