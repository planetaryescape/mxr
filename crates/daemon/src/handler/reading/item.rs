//! One item in the reader, Later, engagement, highlights and per-source
//! choices.

use super::sources::Sources;
use super::{ensure_items, item_data, item_key, parse_item_key, Context, Issue};
use crate::handler::places::AccountKinds;
use crate::handler::{mail_kind, mode_guide, HandlerError, HandlerResult};
use crate::state::AppState;
use chrono::Utc;
use mxr_core::id::{AccountId, MessageId};
use mxr_protocol::{
    ModeKindData, ReadingArticleData, ReadingHighlightData, ReadingItemDetailData,
    ReadingLaterOutcomeData, ReadingParagraphData, ResponseData,
};
use mxr_reading::export::{to_markdown, HighlightExport};
use mxr_reading::{pace, Paragraph, ParagraphKind};
use mxr_store::{ReadingArticleRow, ReadingEngagementReport, ReadingHighlightRow, ReadingItemRow};
use std::collections::HashMap;

/// The longest passage a highlight keeps.
const MAX_QUOTE_CHARS: usize = 4000;
const MAX_NOTE_CHARS: usize = 2000;

pub(super) fn paragraph_data(paragraph: &Paragraph) -> ReadingParagraphData {
    ReadingParagraphData {
        kind: match paragraph.kind {
            ParagraphKind::Heading => "heading",
            ParagraphKind::Text => "text",
            ParagraphKind::ListItem => "list_item",
            ParagraphKind::Quote => "quote",
        }
        .to_string(),
        text: paragraph.text.clone(),
    }
}

/// A saved article as the reader shows it, or why it failed.
pub(super) fn article_data(
    row: &ReadingArticleRow,
    wpm: u32,
) -> Result<ReadingArticleData, String> {
    if row.status != "ok" {
        return Err(row
            .error
            .clone()
            .unwrap_or_else(|| "the article couldn't be fetched".to_string()));
    }
    let paragraphs: Vec<ReadingParagraphData> = row
        .paragraphs
        .as_deref()
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default();
    Ok(ReadingArticleData {
        title: row.title.clone().unwrap_or_default(),
        byline: row.byline.clone(),
        site_name: row.site_name.clone(),
        final_url: row.final_url.clone().unwrap_or_else(|| row.url.clone()),
        contacted: serde_json::from_str(&row.contacted).unwrap_or_default(),
        fetched_at: row.fetched_at,
        words: row.words,
        minutes: pace::minutes(row.words, wpm),
        paragraphs,
        html: row.html.clone().unwrap_or_default(),
    })
}

/// The issue behind an item key, with its classification when it is in
/// the inbox, and its cached items.
pub(super) async fn load_item(
    state: &AppState,
    key: &str,
) -> Result<(Issue, Vec<ReadingItemRow>, i64), HandlerError> {
    let (message_id, idx) = parse_item_key(key)?;
    let envelope = state
        .store
        .get_envelope(&message_id)
        .await?
        .ok_or_else(|| HandlerError::Message(format!("No message for Reading item {key}")))?;
    let mut issue = Issue::from_envelope(&envelope);
    if let Some(message) = state.store.place_message(&message_id).await? {
        let kinds = AccountKinds::load(
            state,
            &issue.account_id,
            std::slice::from_ref(&issue.sender),
        )
        .await?;
        issue.kind = Some(mail_kind::describe(&kinds.signals(&message)));
    }
    let rows = ensure_items(state, &[&issue])
        .await?
        .remove(&issue.id)
        .unwrap_or_default();
    if !rows.iter().any(|row| row.idx == idx) {
        return Err(HandlerError::Message(format!(
            "No Reading item {key}: that issue has {} item(s)",
            rows.len()
        )));
    }
    Ok((issue, rows, idx))
}

pub(in crate::handler) async fn get_item(state: &AppState, key: &str) -> HandlerResult {
    let now = Utc::now();
    let (issue, rows, idx) = load_item(state, key).await?;
    let ctx = Context::load(state, std::slice::from_ref(&issue.id), now).await?;
    let visits = super::load_visits(state, std::slice::from_ref(&issue.account_id)).await?;
    let sources = Sources::load(state, &[&issue], &visits, now).await?;
    let source = sources
        .get(&issue.account_id, &issue.sender)
        .ok_or_else(|| HandlerError::from("the item's source could not be read".to_string()))?;
    let expires_at = Some(mxr_reading::fade::expires_at(issue.date, &source.window));
    let item = item_data(&issue, &rows, idx, Some(source), expires_at, &ctx)
        .ok_or_else(|| HandlerError::Message(format!("No Reading item {key}")))?;

    let body = state.store.get_body(&issue.id).await?;
    let reader_issue = issue.clone();
    let issue_words = rows
        .iter()
        .find(|row| row.idx == 0)
        .map_or(0, |row| row.words);
    let (paragraphs, html) = tokio::task::spawn_blocking(move || {
        let extraction = mxr_reading::extract(&reader_issue.input(body.as_ref()));
        let reader_html = body
            .as_ref()
            .and_then(|b| b.text_html.as_deref())
            .and_then(|html| mxr_reading::article::issue_reader_html(html, issue_words));
        (extraction.paragraphs, reader_html)
    })
    .await
    .map_err(|error| HandlerError::from(format!("reading the issue failed: {error}")))?;

    let (article, article_error) = match state.store.reading_article(&issue.id, idx).await? {
        Some(row) => match article_data(&row, ctx.wpm) {
            Ok(article) => (Some(article), None),
            Err(error) => (None, Some(error)),
        },
        None => (None, None),
    };
    let highlights = state
        .store
        .reading_highlights_for_message(&issue.id)
        .await?
        .into_iter()
        .filter(|row| row.idx == idx)
        .map(|row| highlight_data(&row, &item.title, &item.source, item.url.clone()))
        .collect();
    let words = article.as_ref().map_or(item.words, |a| a.words);
    Ok(ResponseData::ReadingItem {
        item: ReadingItemDetailData {
            minutes_left: pace::minutes_left(words, item.progress, ctx.wpm),
            pace_wpm: ctx.wpm,
            source_data: source.data.clone(),
            item,
            paragraphs: paragraphs.iter().map(paragraph_data).collect(),
            html,
            article,
            article_error,
            highlights,
        },
    })
}

fn highlight_data(
    row: &ReadingHighlightRow,
    title: &str,
    source: &str,
    url: Option<String>,
) -> ReadingHighlightData {
    ReadingHighlightData {
        id: row.id.clone(),
        item_key: item_key(&row.message_id, row.idx),
        account_id: row.account_id.clone(),
        message_id: row.message_id.clone(),
        view: row.view.clone(),
        quote: row.quote.clone(),
        note: row.note.clone(),
        title: title.to_string(),
        source: source.to_string(),
        url,
        created_at: row.created_at,
    }
}

/// Items on Later in the accounts the request touched.
async fn later_count(state: &AppState, accounts: &[AccountId]) -> Result<u32, HandlerError> {
    Ok(u32::try_from(state.store.reading_later(accounts).await?.len()).unwrap_or(u32::MAX))
}

pub(in crate::handler) async fn set_later(
    state: &AppState,
    keys: &[String],
    later: bool,
    dry_run: bool,
) -> HandlerResult {
    if keys.is_empty() {
        return Err(HandlerError::InvalidRequest(
            "name at least one item".into(),
        ));
    }
    let now = Utc::now();
    let mut outcomes = Vec::new();
    let mut accounts: Vec<AccountId> = Vec::new();
    for key in keys {
        let loaded = load_item(state, key).await;
        let (issue, rows, idx) = match loaded {
            Ok(found) => found,
            Err(error) => {
                outcomes.push(ReadingLaterOutcomeData {
                    item_key: key.clone(),
                    title: None,
                    on_later: false,
                    changed: false,
                    error: Some(error.to_string()),
                });
                continue;
            }
        };
        if !accounts.contains(&issue.account_id) {
            accounts.push(issue.account_id.clone());
        }
        let title = rows
            .iter()
            .find(|row| row.idx == idx)
            .map(|row| row.title.clone());
        let was = state
            .store
            .reading_states_for_messages(std::slice::from_ref(&issue.id))
            .await?
            .into_iter()
            .find(|row| row.idx == idx)
            .and_then(|row| row.later_at)
            .is_some();
        let changed = was != later;
        if !dry_run {
            state
                .store
                .set_reading_later(&issue.account_id, &issue.id, idx, later, now)
                .await?;
        }
        outcomes.push(ReadingLaterOutcomeData {
            item_key: item_key(&issue.id, idx),
            title,
            on_later: later,
            changed,
            error: None,
        });
    }
    let count = later_count(state, &accounts).await?;
    let copy = match (later, count) {
        (true, 1) => "Saved to Later. 1 thing saved.".to_string(),
        (true, n) => format!("Saved to Later. {n} things saved."),
        (false, 1) => "Taken off Later. 1 thing still saved.".to_string(),
        (false, n) => format!("Taken off Later. {n} things still saved."),
    };
    Ok(ResponseData::ReadingLater {
        items: outcomes,
        dry_run,
        later_count: count,
        copy,
    })
}

pub(in crate::handler) async fn record_engagement(
    state: &AppState,
    key: &str,
    report: ReadingEngagementReport,
) -> HandlerResult {
    let (message_id, idx) = parse_item_key(key)?;
    let envelope = state
        .store
        .get_envelope(&message_id)
        .await?
        .ok_or_else(|| HandlerError::Message(format!("No message for Reading item {key}")))?;
    if report.opened {
        // Reading something is the mode's main verb: the card has done its job.
        mode_guide::retire(state, ModeKindData::Reading.id()).await?;
    }
    // One privacy switch: MXR_ACTIVITY=off (or a pause) stops engagement too.
    if !state.activity.is_enabled() || state.activity.pause_status().0 {
        return Ok(ResponseData::ReadingEngagement {
            item_key: item_key(&message_id, idx),
            recorded: false,
            finished: false,
        });
    }
    let finished = state
        .store
        .record_reading_engagement(&envelope.account_id, &message_id, idx, report, Utc::now())
        .await?;
    Ok(ResponseData::ReadingEngagement {
        item_key: item_key(&message_id, idx),
        recorded: true,
        finished,
    })
}

pub(in crate::handler) async fn save_highlight(
    state: &AppState,
    key: &str,
    quote: &str,
    note: Option<&str>,
    view: Option<&str>,
) -> HandlerResult {
    let quote = quote.trim();
    if quote.is_empty() {
        return Err(HandlerError::InvalidRequest(
            "select the text to highlight first".into(),
        ));
    }
    if quote.chars().count() > MAX_QUOTE_CHARS {
        return Err(HandlerError::InvalidRequest(format!(
            "a highlight keeps at most {MAX_QUOTE_CHARS} characters"
        )));
    }
    let note = note.map(str::trim).filter(|n| !n.is_empty());
    if note.is_some_and(|n| n.chars().count() > MAX_NOTE_CHARS) {
        return Err(HandlerError::InvalidRequest(format!(
            "a note keeps at most {MAX_NOTE_CHARS} characters"
        )));
    }
    let view = match view.unwrap_or("issue") {
        "issue" => "issue",
        "article" => "article",
        other => {
            return Err(HandlerError::InvalidRequest(format!(
                "view is issue or article, not \"{other}\""
            )))
        }
    };
    let (issue, rows, idx) = load_item(state, key).await?;
    let row = ReadingHighlightRow {
        id: uuid::Uuid::now_v7().to_string(),
        account_id: issue.account_id.clone(),
        message_id: issue.id.clone(),
        idx,
        view: view.to_string(),
        quote: quote.to_string(),
        note: note.map(str::to_string),
        created_at: Utc::now(),
    };
    state.store.insert_reading_highlight(&row).await?;
    // Search finds highlights through the message's semantic chunks.
    if let Err(error) = state
        .semantic
        .enqueue_ingest_messages(std::slice::from_ref(&issue.id))
        .await
    {
        tracing::warn!(%error, "couldn't queue the highlight for search");
    }
    let item = rows.iter().find(|r| r.idx == idx);
    Ok(ResponseData::ReadingHighlight {
        highlight: highlight_data(
            &row,
            item.map_or(&issue.subject, |r| &r.title),
            &issue.source_name,
            item.and_then(|r| r.url.clone()),
        ),
    })
}

pub(in crate::handler) async fn export_highlights(
    state: &AppState,
    accounts: &[AccountId],
) -> HandlerResult {
    let mut rows = state.store.reading_highlights(None).await?;
    rows.retain(|row| accounts.contains(&row.account_id));
    let mut ids: Vec<MessageId> = rows.iter().map(|row| row.message_id.clone()).collect();
    ids.sort_by_key(MessageId::as_str);
    ids.dedup();
    let issues: HashMap<MessageId, Issue> = state
        .store
        .list_envelopes_by_ids(&ids)
        .await?
        .iter()
        .map(|envelope| (envelope.id.clone(), Issue::from_envelope(envelope)))
        .collect();
    let refs: Vec<&Issue> = issues.values().collect();
    let items = ensure_items(state, &refs).await?;
    let highlights: Vec<ReadingHighlightData> = rows
        .iter()
        .filter_map(|row| {
            let issue = issues.get(&row.message_id)?;
            let item = items
                .get(&row.message_id)
                .and_then(|rows| rows.iter().find(|r| r.idx == row.idx));
            Some(highlight_data(
                row,
                item.map_or(&issue.subject, |r| &r.title),
                &issue.source_name,
                item.and_then(|r| r.url.clone()),
            ))
        })
        .collect();
    let markdown = to_markdown(
        &highlights
            .iter()
            .map(|h| HighlightExport {
                quote: h.quote.clone(),
                note: h.note.clone(),
                title: h.title.clone(),
                source: h.source.clone(),
                url: h.url.clone(),
                created_at: h.created_at,
            })
            .collect::<Vec<_>>(),
    );
    Ok(ResponseData::ReadingHighlights {
        highlights,
        markdown,
    })
}

pub(in crate::handler) async fn set_source(
    state: &AppState,
    account_id: &AccountId,
    sender_email: &str,
    original_layout: Option<bool>,
    dismiss_unsubscribe_offer: bool,
) -> HandlerResult {
    let sender = sender_email.trim().to_ascii_lowercase();
    if sender.is_empty() {
        return Err(HandlerError::InvalidRequest("name the sender".into()));
    }
    let latest = state
        .store
        .reading_source_issues(
            account_id,
            std::slice::from_ref(&sender_email.trim().to_string()),
            1,
        )
        .await?;
    let latest = match latest.into_iter().next() {
        Some(found) => found,
        None => state
            .store
            .reading_source_issues(account_id, std::slice::from_ref(&sender), 1)
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| HandlerError::Message(format!("No mail from {sender}")))?,
    };
    let now = Utc::now();
    state
        .store
        .set_reading_source_prefs(
            account_id,
            &sender,
            original_layout,
            dismiss_unsubscribe_offer,
            now,
        )
        .await?;
    let envelope = state
        .store
        .get_envelope(&latest.message_id)
        .await?
        .ok_or_else(|| HandlerError::Message(format!("No mail from {sender}")))?;
    let issue = Issue::from_envelope(&envelope);
    let visits = super::load_visits(state, std::slice::from_ref(&issue.account_id)).await?;
    let sources = Sources::load(state, &[&issue], &visits, now).await?;
    let source = sources
        .get(account_id, &sender)
        .ok_or_else(|| HandlerError::Message(format!("No mail from {sender}")))?;
    Ok(ResponseData::ReadingSource {
        source: source.data.clone(),
    })
}
