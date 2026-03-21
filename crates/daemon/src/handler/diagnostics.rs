use super::{
    build_account_sync_status, collect_doctor_report, collect_status_snapshot,
    handle_export_search, handle_export_thread, protocol_event_entry, recent_log_lines,
    should_fallback_to_tantivy, HandlerResult,
};
use crate::state::AppState;
use mxr_core::id::{AccountId, MessageId, ThreadId};
use mxr_core::types::ExportFormat;
use mxr_protocol::{ResponseData, SearchResultItem};
use mxr_search::{parse_query, QueryBuilder, SearchResult};
use std::sync::Arc;

pub(super) async fn list_events(
    state: &Arc<AppState>,
    limit: u32,
    level: Option<&str>,
    category: Option<&str>,
) -> HandlerResult {
    let entries = state
        .store
        .list_events(limit, level, category)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::EventLogEntries {
        entries: entries.into_iter().map(protocol_event_entry).collect(),
    })
}

pub(super) fn get_logs(limit: u32, level: Option<&str>) -> HandlerResult {
    let lines = recent_log_lines(limit as usize, level).map_err(|e| e.to_string())?;
    Ok(ResponseData::LogLines { lines })
}

pub(super) async fn doctor_report(state: &Arc<AppState>) -> HandlerResult {
    let report = collect_doctor_report(state).await?;
    Ok(ResponseData::DoctorReport { report })
}

pub(super) async fn bug_report(
    verbose: bool,
    full_logs: bool,
    since: Option<String>,
) -> HandlerResult {
    let content = crate::commands::bug_report::generate_report_markdown(
        &crate::commands::bug_report::BugReportOptions {
            edit: false,
            stdout: false,
            clipboard: false,
            github: false,
            output: None,
            verbose,
            full_logs,
            no_sanitize: false,
            since,
        },
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(ResponseData::BugReport { content })
}

pub(super) async fn search(state: &Arc<AppState>, query: &str, limit: u32) -> HandlerResult {
    let search = state.search.lock().await;
    let results = match parse_query(query) {
        Ok(ast) => {
            let builder = QueryBuilder::new(search.schema());
            let tantivy_query = builder.build(&ast);
            search.search_ast(tantivy_query, limit as usize)
        }
        Err(error) => {
            if should_fallback_to_tantivy(query, &error) {
                search.search(query, limit as usize)
            } else {
                Err(mxr_core::MxrError::Search(format!(
                    "Invalid search query: {error}"
                )))
            }
        }
    };
    let results = results.map_err(|e| e.to_string())?;
    Ok(ResponseData::SearchResults {
        results: search_result_items(results),
    })
}

pub(super) async fn count(state: &Arc<AppState>, query: &str) -> HandlerResult {
    let search = state.search.lock().await;
    let results = match parse_query(query) {
        Ok(ast) => {
            let builder = QueryBuilder::new(search.schema());
            let tantivy_query = builder.build(&ast);
            search.search_ast(tantivy_query, 10_000)
        }
        Err(error) => {
            if should_fallback_to_tantivy(query, &error) {
                search.search(query, 10_000)
            } else {
                Err(mxr_core::MxrError::Search(format!(
                    "Invalid search query: {error}"
                )))
            }
        }
    };
    Ok(ResponseData::Count {
        count: results.map_err(|e| e.to_string())?.len() as u32,
    })
}

pub(super) async fn get_headers(state: &Arc<AppState>, message_id: &MessageId) -> HandlerResult {
    match state
        .store
        .get_envelope(message_id)
        .await
        .map_err(|e| e.to_string())?
    {
        Some(envelope) => {
            let mut headers = Vec::new();
            headers.push((
                "From".to_string(),
                format!(
                    "{} <{}>",
                    envelope.from.name.as_deref().unwrap_or(""),
                    envelope.from.email
                ),
            ));
            headers.push(("Subject".to_string(), envelope.subject.clone()));
            headers.push(("Date".to_string(), envelope.date.to_rfc3339()));
            for addr in &envelope.to {
                headers.push((
                    "To".to_string(),
                    format!("{} <{}>", addr.name.as_deref().unwrap_or(""), addr.email),
                ));
            }
            for addr in &envelope.cc {
                headers.push((
                    "Cc".to_string(),
                    format!("{} <{}>", addr.name.as_deref().unwrap_or(""), addr.email),
                ));
            }
            if let Some(mid) = &envelope.message_id_header {
                headers.push(("Message-ID".to_string(), mid.clone()));
            }
            if let Some(irt) = &envelope.in_reply_to {
                headers.push(("In-Reply-To".to_string(), irt.clone()));
            }
            if let Ok(Some(body)) = state.store.get_body(message_id).await {
                if let Some(list_id) = body.metadata.list_id {
                    headers.push(("List-Id".to_string(), list_id));
                }
                for auth_result in body.metadata.auth_results {
                    headers.push(("Authentication-Results".to_string(), auth_result));
                }
                if !body.metadata.content_language.is_empty() {
                    headers.push((
                        "Content-Language".to_string(),
                        body.metadata.content_language.join(", "),
                    ));
                }
            }
            Ok(ResponseData::Headers { headers })
        }
        None => Err("Not found".to_string()),
    }
}

pub(super) async fn list_saved_searches(state: &Arc<AppState>) -> HandlerResult {
    let searches = state
        .store
        .list_saved_searches()
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::SavedSearches { searches })
}

pub(super) async fn list_subscriptions(state: &Arc<AppState>, limit: u32) -> HandlerResult {
    let subscriptions = state
        .store
        .list_subscriptions(limit)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::Subscriptions { subscriptions })
}

pub(super) async fn create_saved_search(
    state: &Arc<AppState>,
    name: &str,
    query: &str,
) -> HandlerResult {
    let search = mxr_core::types::SavedSearch {
        id: mxr_core::SavedSearchId::new(),
        account_id: None,
        name: name.to_string(),
        query: query.to_string(),
        sort: mxr_core::types::SortOrder::DateDesc,
        icon: None,
        position: 0,
        created_at: chrono::Utc::now(),
    };
    state
        .store
        .insert_saved_search(&search)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::SavedSearchData { search })
}

pub(super) async fn delete_saved_search(state: &Arc<AppState>, name: &str) -> HandlerResult {
    match state
        .store
        .delete_saved_search_by_name(name)
        .await
        .map_err(|e| e.to_string())?
    {
        true => Ok(ResponseData::Ack),
        false => Err(format!("Saved search '{name}' not found")),
    }
}

pub(super) async fn run_saved_search(
    state: &Arc<AppState>,
    name: &str,
    limit: u32,
) -> HandlerResult {
    let saved = state
        .store
        .get_saved_search_by_name(name)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Saved search '{name}' not found"))?;
    let search = state.search.lock().await;
    let results = search
        .search(&saved.query, limit as usize)
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::SearchResults {
        results: search_result_items(results),
    })
}

pub(super) async fn get_status(state: &Arc<AppState>) -> HandlerResult {
    let (accounts, total_messages, sync_statuses) = collect_status_snapshot(state).await?;
    Ok(ResponseData::Status {
        uptime_secs: state.uptime_secs(),
        accounts,
        total_messages,
        daemon_pid: Some(std::process::id()),
        sync_statuses,
    })
}

pub(super) async fn sync_now(
    state: &Arc<AppState>,
    account_id: Option<&AccountId>,
) -> HandlerResult {
    let provider = state.get_provider(account_id).clone();
    state
        .sync_engine
        .sync_account(provider.as_ref())
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::Ack)
}

pub(super) async fn export_thread(
    state: &Arc<AppState>,
    thread_id: &ThreadId,
    format: &ExportFormat,
) -> HandlerResult {
    match handle_export_thread(state, thread_id, format).await {
        mxr_protocol::Response::Ok { data } => Ok(data),
        mxr_protocol::Response::Error { message } => Err(message),
    }
}

pub(super) async fn export_search(
    state: &Arc<AppState>,
    query: &str,
    format: &ExportFormat,
) -> HandlerResult {
    match handle_export_search(state, query, format).await {
        mxr_protocol::Response::Ok { data } => Ok(data),
        mxr_protocol::Response::Error { message } => Err(message),
    }
}

pub(super) async fn get_sync_status(
    state: &Arc<AppState>,
    account_id: &AccountId,
) -> HandlerResult {
    let sync = build_account_sync_status(state, account_id).await?;
    Ok(ResponseData::SyncStatus { sync })
}

fn search_result_items(results: Vec<SearchResult>) -> Vec<SearchResultItem> {
    results
        .into_iter()
        .filter_map(|result| {
            Some(SearchResultItem {
                message_id: mxr_core::MessageId::from_uuid(
                    uuid::Uuid::parse_str(&result.message_id).ok()?,
                ),
                account_id: mxr_core::AccountId::from_uuid(
                    uuid::Uuid::parse_str(&result.account_id).ok()?,
                ),
                thread_id: mxr_core::ThreadId::from_uuid(
                    uuid::Uuid::parse_str(&result.thread_id).ok()?,
                ),
                score: result.score,
            })
        })
        .collect()
}
