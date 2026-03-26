use super::super::*;

pub(crate) async fn search(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<SearchQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, query.token.as_deref(), &state.config.auth_token)?;
    if query.q.trim().is_empty() {
        return Ok(Json(json!({
            "scope": query.scope.unwrap_or_else(|| "threads".to_string()),
            "sort": query.sort.unwrap_or_else(|| "recent".to_string()),
            "mode": query.mode.unwrap_or_default(),
            "total": 0,
            "has_more": false,
            "groups": [],
            "explain": serde_json::Value::Null,
        })));
    }

    let sort = match query.sort.as_deref() {
        Some("relevant") => SortOrder::Relevance,
        Some("oldest") => SortOrder::DateAsc,
        _ => SortOrder::DateDesc,
    };

    let thread_scope = query.scope.as_deref().unwrap_or("threads") == "threads";

    match ipc_request(
        &state.config.socket_path,
        Request::Search {
            query: query.q,
            limit: query.limit,
            offset: 0,
            mode: query.mode,
            sort: Some(sort),
            explain: query.explain,
        },
    )
    .await?
    {
        ResponseData::SearchResults {
            results,
            explain,
            has_more,
        } => {
            let effective_results = if thread_scope {
                dedupe_search_results_by_thread(results)
            } else {
                results
            };
            let message_ids = effective_results
                .iter()
                .map(|result| result.message_id.clone())
                .collect::<Vec<_>>();
            let envelopes = if message_ids.is_empty() {
                Vec::new()
            } else {
                match ipc_request(
                    &state.config.socket_path,
                    Request::ListEnvelopesByIds {
                        message_ids: message_ids.clone(),
                    },
                )
                .await?
                {
                    ResponseData::Envelopes { envelopes } => {
                        reorder_envelopes(envelopes, &message_ids)
                    }
                    _ => return Err(BridgeError::UnexpectedResponse),
                }
            };

            Ok(Json(json!({
                "scope": query.scope.unwrap_or_else(|| "threads".to_string()),
                "sort": query.sort.unwrap_or_else(|| "recent".to_string()),
                "mode": query.mode.unwrap_or_default(),
                "total": effective_results.len(),
                "has_more": has_more,
                "groups": group_envelopes(envelopes),
                "explain": explain,
            })))
        }
        _ => Err(BridgeError::UnexpectedResponse),
    }
}
