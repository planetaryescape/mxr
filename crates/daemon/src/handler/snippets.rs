//! Snippets: thin pass-through to the store with `Snippet` ↔
//! `SnippetData` translation at the IPC boundary.

use super::HandlerResult;
use crate::state::AppState;
use chrono::Utc;
use mxr_protocol::{ResponseData, SnippetData};
use mxr_store::Snippet;

fn to_data(snippet: Snippet) -> SnippetData {
    SnippetData {
        name: snippet.name,
        body: snippet.body,
        vars: snippet.vars,
        created_at: snippet.created_at,
        updated_at: snippet.updated_at,
    }
}

pub(super) async fn list_snippets(state: &AppState) -> HandlerResult {
    let snippets = state
        .store
        .list_snippets()
        .await
        ?;
    let data: Vec<SnippetData> = snippets.into_iter().map(to_data).collect();
    Ok(ResponseData::Snippets { snippets: data })
}

pub(super) async fn set_snippet(
    state: &AppState,
    name: String,
    body: String,
    vars: Vec<String>,
) -> HandlerResult {
    if name.trim().is_empty() {
        return Err(crate::handler::HandlerError::Message("snippet name cannot be empty".to_string()));
    }
    let now = Utc::now();
    // Preserve created_at on update; only updated_at advances.
    let created_at = match state
        .store
        .get_snippet(&name)
        .await
        ?
    {
        Some(existing) => existing.created_at,
        None => now,
    };
    let snippet = Snippet {
        name,
        body,
        vars,
        created_at,
        updated_at: now,
    };
    state
        .store
        .upsert_snippet(&snippet)
        .await
        ?;
    Ok(ResponseData::SnippetData {
        snippet: to_data(snippet),
    })
}

pub(super) async fn delete_snippet(state: &AppState, name: &str) -> HandlerResult {
    state
        .store
        .delete_snippet(name)
        .await
        ?;
    Ok(ResponseData::Ack)
}
