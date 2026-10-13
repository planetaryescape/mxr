//! Marginalia: send a Reading item to the reading app, open it there, and
//! read the highlights made on it back into mxr.
//!
//! Only explicit requests send anything: a send names the item, and its
//! dry run contacts nothing. The site lives in config
//! (`reading.marginalia_url`), the personal token in the secrets file.
//! Highlights are read back for items already sent, on request and after
//! a sync at most every half hour; that reads the user's own highlights
//! and sends no mail.

use super::item::load_item;
use super::{item_key, parse_item_key, Issue};
use crate::handler::{HandlerError, HandlerResult};
use crate::state::AppState;
use chrono::{DateTime, Duration, Utc};
use mxr_core::id::{AccountId, MessageId};
use mxr_protocol::{
    MarginaliaConnectionData, MarginaliaSendData, MarginaliaStatusData, MarginaliaSyncData,
    MarginaliaSyncFailureData, ReadingMarginaliaData, ResponseData,
};
use mxr_reading::marginalia::{
    normalize_base_url, paragraphs_html, ArticleIn, MarginaliaClient, MarginaliaError,
};
use mxr_store::{MarginaliaHighlightIn, ReadingArticleRow, ReadingMarginaliaRow};

const SECRET_SERVICE: &str = "mxr/marginalia";
const SECRET_ACCOUNT: &str = "api-token";
/// After a sync, highlights are read back for items sent in this window,
/// each at most this often, a few at a time.
const BACKGROUND_WINDOW: Duration = Duration::days(60);
const BACKGROUND_EVERY: Duration = Duration::minutes(30);
const BACKGROUND_BATCH: u32 = 20;
/// Reading back every sent item, on request.
const SYNC_ALL_LIMIT: u32 = 500;

fn secrets() -> mxr_config::SecretStore {
    mxr_config::SecretStore::at_default_path()
}

fn secret_service() -> String {
    mxr_config::scoped_credential_service(SECRET_SERVICE)
}

fn saved_token() -> Option<String> {
    match secrets().get(&secret_service(), SECRET_ACCOUNT) {
        Ok(token) => token.filter(|token| !token.trim().is_empty()),
        Err(error) => {
            tracing::warn!(%error, "couldn't read the Marginalia token");
            None
        }
    }
}

fn save_token(token: &str) -> Result<(), HandlerError> {
    secrets()
        .set(&secret_service(), SECRET_ACCOUNT, token)
        .map_err(|error| HandlerError::from(format!("couldn't save the Marginalia token: {error}")))
}

/// The site and a client, when Marginalia is connected.
fn connected(state: &AppState) -> Option<(String, MarginaliaClient)> {
    let url = state.config_snapshot().reading.marginalia_url?;
    let token = saved_token()?;
    match MarginaliaClient::new(&url, &token) {
        Ok(client) => Some((url, client)),
        Err(error) => {
            tracing::warn!(%error, "the saved Marginalia connection isn't usable");
            None
        }
    }
}

fn require_connected(state: &AppState) -> Result<MarginaliaClient, HandlerError> {
    connected(state).map(|(_, client)| client).ok_or_else(|| {
        HandlerError::InvalidRequest(
            "Marginalia isn't connected: connect it with its address and a token from its Account page, Connected apps (mxr reading marginalia connect)"
                .into(),
        )
    })
}

fn failed(error: &MarginaliaError) -> HandlerError {
    match error {
        MarginaliaError::InvalidUrl(_) | MarginaliaError::Unauthorized => {
            HandlerError::InvalidRequest(error.to_string())
        }
        _ => HandlerError::Message(error.to_string()),
    }
}

fn marginalia_data(row: &ReadingMarginaliaRow) -> ReadingMarginaliaData {
    ReadingMarginaliaData {
        article_id: row.article_id.clone(),
        read_url: row.read_url.clone(),
        sent_at: row.sent_at,
        highlights_synced_at: row.highlights_synced_at,
    }
}

/// The Marginalia article an item became, for the reader.
pub(super) async fn item_marginalia(
    state: &AppState,
    message_id: &MessageId,
    idx: i64,
) -> Result<Option<ReadingMarginaliaData>, HandlerError> {
    Ok(state
        .store
        .reading_marginalia(message_id, idx)
        .await?
        .as_ref()
        .map(marginalia_data))
}

async fn sent_count(state: &AppState) -> Result<u32, HandlerError> {
    let rows = state
        .store
        .reading_marginalia_due(
            None,
            Utc::now() + Duration::days(1),
            DateTime::<Utc>::UNIX_EPOCH,
            u32::MAX,
        )
        .await?;
    Ok(u32::try_from(rows.len()).unwrap_or(u32::MAX))
}

pub(in crate::handler) async fn status(state: &AppState) -> HandlerResult {
    let url = state.config_snapshot().reading.marginalia_url;
    let connected = url.is_some() && saved_token().is_some();
    let host = url
        .as_deref()
        .and_then(|url| normalize_base_url(url).ok())
        .and_then(|url| url.host_str().map(str::to_string));
    Ok(ResponseData::MarginaliaStatus {
        status: MarginaliaStatusData {
            connected,
            url,
            host,
            sent: sent_count(state).await?,
        },
    })
}

/// Connect (the token is checked with Marginalia first) or disconnect.
pub(in crate::handler) async fn set_connection(
    state: &AppState,
    connection: Option<&MarginaliaConnectionData>,
) -> HandlerResult {
    match connection {
        Some(connection) => {
            let url = normalize_base_url(&connection.url).map_err(|e| failed(&e))?;
            let client =
                MarginaliaClient::new(url.as_str(), &connection.token).map_err(|e| failed(&e))?;
            client.verify().await.map_err(|e| failed(&e))?;
            save_token(connection.token.trim())?;
            let saved = url.to_string();
            state
                .mutate_config(|config| config.reading.marginalia_url = Some(saved))
                .await
                .map_err(HandlerError::from)?;
            tracing::info!(host = %client.host(), "Marginalia connected");
        }
        None => {
            secrets()
                .remove(&secret_service(), SECRET_ACCOUNT)
                .map_err(|error| {
                    HandlerError::from(format!("couldn't remove the Marginalia token: {error}"))
                })?;
            state
                .mutate_config(|config| config.reading.marginalia_url = None)
                .await
                .map_err(HandlerError::from)?;
            tracing::info!("Marginalia disconnected");
        }
    }
    status(state).await
}

/// The issue as HTML: its own, cleaned, when the cleaner kept most of it,
/// else the reader text.
async fn issue_html(state: &AppState, issue: &Issue, words: u32) -> Result<String, HandlerError> {
    let body = state.store.get_body(&issue.id).await?;
    let issue = issue.clone();
    tokio::task::spawn_blocking(move || {
        body.as_ref()
            .and_then(|b| b.text_html.as_deref())
            .and_then(|html| mxr_reading::article::issue_reader_html(html, words))
            .unwrap_or_else(|| {
                paragraphs_html(&mxr_reading::extract(&issue.input(body.as_ref())).paragraphs)
            })
    })
    .await
    .map_err(|error| HandlerError::from(format!("reading the issue failed: {error}")))
}

fn saved_article(row: Option<ReadingArticleRow>) -> Option<ReadingArticleRow> {
    row.filter(|row| row.status == "ok" && row.html.is_some())
}

pub(in crate::handler) async fn send(state: &AppState, key: &str, dry_run: bool) -> HandlerResult {
    let client = require_connected(state)?;
    let host = client.host();
    let (issue, rows, idx) = load_item(state, key).await?;
    let row = rows
        .iter()
        .find(|row| row.idx == idx)
        .ok_or_else(|| HandlerError::Message(format!("No Reading item {key}")))?;
    let key = item_key(&issue.id, idx);

    if let Some(sent) = state.store.reading_marginalia(&issue.id, idx).await? {
        return Ok(ResponseData::MarginaliaSend {
            send: MarginaliaSendData {
                item_key: key,
                dry_run,
                host,
                title: row.title.clone(),
                view: sent.view.clone(),
                bytes: 0,
                fetch_first: None,
                already_sent: true,
                marginalia: Some(marginalia_data(&sent)),
                copy: "Already in Marginalia.".into(),
            },
        });
    }

    let mut article = saved_article(state.store.reading_article(&issue.id, idx).await?);
    // A digest link is its article: fetch it first when it isn't saved.
    let fetch_first =
        (article.is_none() && idx > 0).then(|| row.domain.clone().unwrap_or_default());
    if dry_run {
        let (view, bytes) = match (&article, &fetch_first) {
            (Some(saved), _) => ("article", saved.html.as_ref().map_or(0, String::len)),
            (None, Some(_)) => ("article", 0),
            (None, None) => ("issue", issue_html(state, &issue, row.words).await?.len()),
        };
        return Ok(ResponseData::MarginaliaSend {
            send: MarginaliaSendData {
                item_key: key,
                dry_run: true,
                copy: format!("Would send to {host}."),
                host,
                title: row.title.clone(),
                view: view.into(),
                bytes: u64::try_from(bytes).unwrap_or(u64::MAX),
                fetch_first,
                already_sent: false,
                marginalia: None,
            },
        });
    }
    if fetch_first.is_some() {
        super::fetch_article(state, &key, false).await?;
        article = saved_article(state.store.reading_article(&issue.id, idx).await?);
        if article.is_none() {
            return Err(HandlerError::Message(
                "the article couldn't be fetched, so nothing was sent to Marginalia; open it in a browser instead"
                    .into(),
            ));
        }
    }

    let (view, payload) = match article {
        Some(saved) => (
            "article",
            ArticleIn {
                external_id: format!("mxr:{key}"),
                title: saved
                    .title
                    .clone()
                    .filter(|title| !title.trim().is_empty())
                    .unwrap_or_else(|| row.title.clone()),
                html: saved.html.clone().unwrap_or_default(),
                url: Some(saved.final_url.clone().unwrap_or_else(|| saved.url.clone())),
                author: saved.byline.clone().or_else(|| saved.site_name.clone()),
                published_at: None,
            },
        ),
        None => (
            "issue",
            ArticleIn {
                external_id: format!("mxr:{key}"),
                title: row.title.clone(),
                html: issue_html(state, &issue, row.words).await?,
                url: None,
                author: Some(issue.source_name.clone()),
                published_at: Some(issue.date.timestamp_millis()),
            },
        ),
    };
    let bytes = u64::try_from(payload.html.len()).unwrap_or(u64::MAX);
    let sent = client.send(&payload).await.map_err(|e| failed(&e))?;
    let saved = ReadingMarginaliaRow {
        message_id: issue.id.clone(),
        idx,
        account_id: issue.account_id.clone(),
        article_id: sent.article_id,
        read_url: sent.read_url,
        view: view.into(),
        sent_at: Utc::now(),
        highlights_synced_at: None,
    };
    state.store.upsert_reading_marginalia(&saved).await?;
    tracing::info!(item = %key, %host, created = sent.created, "reading item sent to Marginalia");
    Ok(ResponseData::MarginaliaSend {
        send: MarginaliaSendData {
            item_key: key,
            dry_run: false,
            host,
            title: payload.title,
            view: view.into(),
            bytes,
            fetch_first,
            already_sent: false,
            marginalia: Some(marginalia_data(&saved)),
            copy: if sent.created {
                "Sent to Marginalia."
            } else {
                "Already in Marginalia."
            }
            .into(),
        },
    })
}

/// Reads back each row's highlights, stopping early when Marginalia
/// refuses the token or asks to slow down.
async fn read_back(
    state: &AppState,
    client: &MarginaliaClient,
    rows: &[ReadingMarginaliaRow],
) -> Result<MarginaliaSyncData, HandlerError> {
    let mut out = MarginaliaSyncData::default();
    let mut changed: Vec<MessageId> = Vec::new();
    for row in rows {
        let key = item_key(&row.message_id, row.idx);
        match client.highlights(&row.article_id).await {
            Ok(highlights) => {
                let now = Utc::now();
                let marks: Vec<MarginaliaHighlightIn> = highlights
                    .into_iter()
                    .map(|h| MarginaliaHighlightIn {
                        origin_id: h.id,
                        quote: h.text,
                        created_at: DateTime::from_timestamp_millis(h.created_at).unwrap_or(now),
                    })
                    .collect();
                let mirror = state
                    .store
                    .mirror_marginalia_highlights(row, &marks, now)
                    .await?;
                out.items += 1;
                out.added += mirror.added;
                out.removed += mirror.removed;
                if mirror.added + mirror.removed > 0 && !changed.contains(&row.message_id) {
                    changed.push(row.message_id.clone());
                }
            }
            Err(MarginaliaError::NotFound) => {
                state
                    .store
                    .delete_reading_marginalia(&row.message_id, row.idx)
                    .await?;
                out.forgotten += 1;
            }
            Err(error) => {
                let stop = matches!(
                    error,
                    MarginaliaError::Unauthorized | MarginaliaError::RateLimited
                );
                out.failed.push(MarginaliaSyncFailureData {
                    item_key: key,
                    error: error.to_string(),
                });
                if stop {
                    break;
                }
            }
        }
    }
    // Search finds highlights through their message's chunks.
    if !changed.is_empty() {
        if let Err(error) = state.semantic.enqueue_ingest_messages(&changed).await {
            tracing::warn!(%error, "couldn't queue Marginalia highlights for search");
        }
    }
    Ok(out)
}

pub(in crate::handler) async fn sync(state: &AppState, key: Option<&str>) -> HandlerResult {
    let client = require_connected(state)?;
    let rows = match key {
        Some(key) => {
            let (message_id, idx) = parse_item_key(key)?;
            let sent = state
                .store
                .reading_marginalia(&message_id, idx)
                .await?
                .ok_or_else(|| {
                    HandlerError::InvalidRequest(format!(
                        "{key} hasn't been sent to Marginalia, so it has no highlights there"
                    ))
                })?;
            vec![sent]
        }
        None => {
            state
                .store
                .reading_marginalia_due(
                    None,
                    Utc::now() + Duration::days(1),
                    DateTime::<Utc>::UNIX_EPOCH,
                    SYNC_ALL_LIMIT,
                )
                .await?
        }
    };
    let sync = read_back(state, &client, &rows).await?;
    Ok(ResponseData::MarginaliaSync { sync })
}

/// After a sync: read back highlights on this account's recently sent
/// items, each at most every half hour. Quiet when not connected.
pub(super) async fn after_sync(state: &AppState, account_id: &AccountId) {
    let Some((_, client)) = connected(state) else {
        return;
    };
    let now = Utc::now();
    let result = async {
        let rows = state
            .store
            .reading_marginalia_due(
                Some(account_id),
                now - BACKGROUND_EVERY,
                now - BACKGROUND_WINDOW,
                BACKGROUND_BATCH,
            )
            .await?;
        if rows.is_empty() {
            return Ok(None);
        }
        read_back(state, &client, &rows).await.map(Some)
    }
    .await;
    match result {
        Ok(Some(sync)) => tracing::debug!(
            account = %account_id,
            items = sync.items,
            added = sync.added,
            removed = sync.removed,
            failed = sync.failed.len(),
            "Marginalia highlights read back"
        ),
        Ok(None) => {}
        Err(error) => {
            tracing::warn!(account = %account_id, %error, "Marginalia highlight read-back failed");
        }
    }
}
