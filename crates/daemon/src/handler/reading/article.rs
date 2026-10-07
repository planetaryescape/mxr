//! `FetchArticle`: the article an item links to, fetched only when asked,
//! saved so Later reads offline.
//!
//! The fetch goes through `mxr_reading::fetch`, which refuses this machine,
//! private networks and redirects into them, and names every site it
//! contacted. The demo mailbox never touches the network: its links point
//! at made-up sites, so their articles come from the demo's own fixtures.

use super::item::{article_data, load_item, paragraph_data};
use super::{item_key, Context};
use crate::handler::{HandlerError, HandlerResult};
use crate::state::AppState;
use chrono::Utc;
use mxr_protocol::{ReadingFetchData, ResponseData};
use mxr_reading::article::extract_article;
use mxr_reading::fetch::{checked_url, fetch_page, FetchPolicy};
use mxr_store::ReadingArticleRow;

pub(in crate::handler) async fn fetch_article(
    state: &AppState,
    key: &str,
    refresh: bool,
) -> HandlerResult {
    let (issue, rows, idx) = load_item(state, key).await?;
    let row = rows
        .iter()
        .find(|row| row.idx == idx)
        .ok_or_else(|| HandlerError::Message(format!("No Reading item {key}")))?;
    let Some(url) = row.url.clone() else {
        return Err(HandlerError::InvalidRequest(
            "this item has no linked article: the issue is the whole piece".into(),
        ));
    };
    let domain = row.domain.clone().unwrap_or_default();
    let key = item_key(&issue.id, idx);
    let ctx = Context::load(state, std::slice::from_ref(&issue.id), Utc::now()).await?;
    if !refresh {
        if let Some(saved) = state.store.reading_article(&issue.id, idx).await? {
            if saved.status == "ok" {
                return Ok(ResponseData::ReadingArticle {
                    fetch: ReadingFetchData {
                        item_key: key,
                        domain,
                        cached: true,
                        article: article_data(&saved, ctx.wpm).ok(),
                        error: None,
                    },
                });
            }
        }
    }

    // The demo mailbox, in `mxr demo` or the web app's end-to-end daemon.
    let demo = mxr_config::is_demo_instance() || mxr_provider_fake::fixtures::demo_dataset_active();
    let fetched = if demo {
        mxr_provider_fake::fixtures::demo_article_html(&url)
            .map(|html| (url.clone(), Vec::new(), html.to_string()))
            .ok_or_else(|| format!("the demo has no copy of {domain}; nothing was fetched"))
    } else {
        // Refuse before anything is sent when the link itself is off limits.
        match checked_url(&url) {
            Ok(_) => fetch_page(&url, &FetchPolicy::default())
                .await
                .map(|page| (page.final_url.to_string(), page.contacted, page.html))
                .map_err(|error| error.to_string()),
            Err(error) => Err(error.to_string()),
        }
    };
    let now = Utc::now();
    let mut saved = ReadingArticleRow {
        message_id: issue.id.clone(),
        idx,
        url: url.clone(),
        final_url: None,
        status: "failed".to_string(),
        title: None,
        byline: None,
        site_name: None,
        html: None,
        paragraphs: None,
        words: 0,
        contacted: "[]".to_string(),
        error: None,
        fetched_at: now,
    };
    match fetched {
        Ok((final_url, contacted, html)) => {
            saved.final_url = Some(final_url.clone());
            saved.contacted = serde_json::to_string(&contacted)?;
            let extracted =
                tokio::task::spawn_blocking(move || extract_article(&html, Some(&final_url)))
                    .await
                    .map_err(|error| {
                        HandlerError::from(format!("reading the article failed: {error}"))
                    })?;
            match extracted {
                Ok(article) => {
                    saved.status = "ok".to_string();
                    saved.title = Some(article.title);
                    saved.byline = article.byline;
                    saved.site_name = article.site_name;
                    saved.paragraphs = Some(serde_json::to_string(
                        &article
                            .paragraphs
                            .iter()
                            .map(paragraph_data)
                            .collect::<Vec<_>>(),
                    )?);
                    saved.words = article.words;
                    saved.html = Some(article.html);
                }
                Err(error) => saved.error = Some(error.to_string()),
            }
        }
        Err(error) => saved.error = Some(error),
    }
    state.store.save_reading_article(&saved).await?;
    tracing::info!(
        item = %key,
        ok = saved.status == "ok",
        "reading article fetched on request"
    );
    let (article, error) = match article_data(&saved, ctx.wpm) {
        Ok(article) => (Some(article), None),
        Err(error) => (None, Some(error)),
    };
    Ok(ResponseData::ReadingArticle {
        fetch: ReadingFetchData {
            item_key: key,
            domain,
            cached: false,
            article,
            error,
        },
    })
}
