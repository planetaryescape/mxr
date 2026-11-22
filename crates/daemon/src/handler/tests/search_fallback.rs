use super::*;
use crate::test_fixtures::TestEnvelopeBuilder;
use chrono::{Duration, Utc};
use mxr_core::types::{Envelope, SearchMode, SortOrder};
use mxr_protocol::{SearchDegraded, SearchDegradedReason};

async fn send(state: &Arc<AppState>, request: Request) -> Response {
    let msg = IpcMessage {
        expected_workspace: None,
        id: 90,
        source: ::mxr_protocol::ClientKind::default(),
        payload: IpcPayload::Request(request),
    };
    match handle_request(state, &msg).await.payload {
        IpcPayload::Response(response) => response,
        other => panic!("expected a response, got {other:?}"),
    }
}

fn search_request(
    query: &str,
    account_id: Option<mxr_core::AccountId>,
    limit: u32,
    offset: u32,
) -> Request {
    Request::Search {
        query: query.to_string(),
        limit,
        offset,
        account_id,
        mode: Some(SearchMode::Lexical),
        sort: Some(SortOrder::DateDesc),
        explain: false,
        limit_from_config: false,
    }
}

struct Page {
    ids: Vec<String>,
    total: u32,
    degraded: Option<SearchDegraded>,
}

fn page(response: Response) -> Page {
    match response {
        Response::Ok {
            data:
                ResponseData::SearchResults {
                    results,
                    total,
                    degraded,
                    ..
                },
        } => Page {
            ids: results
                .into_iter()
                .map(|item| item.message_id.as_str())
                .collect(),
            total,
            degraded,
        },
        other => panic!("expected search results, got {other:?}"),
    }
}

fn error_of(response: Response) -> (String, String) {
    match response {
        Response::Error { message, code, .. } => (message, code),
        other => panic!("expected an error, got {other:?}"),
    }
}

async fn store_message(
    state: &AppState,
    account: &mxr_core::AccountId,
    tag: &str,
    subject: &str,
    from: &str,
    days_ago: i64,
) -> Envelope {
    let envelope = TestEnvelopeBuilder::new()
        .account_id(account.clone())
        .provider_id(format!("fb-{tag}"))
        .sender_address("Sender", from)
        .subject(subject)
        .snippet("")
        .date(Utc::now() - Duration::days(days_ago))
        .build();
    state.store.upsert_envelope(&envelope).await.unwrap();
    envelope
}

async fn stopped_state() -> Arc<AppState> {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    state.abort_search_worker().await;
    state
}

#[tokio::test]
async fn a_plain_search_is_answered_from_sqlite_when_the_worker_has_stopped() {
    let state = stopped_state().await;
    let account = state.default_account_id();
    let wanted = store_message(&state, &account, "a", "Quarterly invoice", "x@one.test", 1).await;
    let _other = store_message(&state, &account, "b", "Lunch plans", "y@two.test", 2).await;

    let found = page(send(&state, search_request("invoice", None, 10, 0)).await);

    assert_eq!(found.ids, vec![wanted.id.as_str()]);
    assert_eq!(found.total, 1);
    let degraded = found.degraded.expect("a limited answer must say so");
    assert_eq!(degraded.reason, SearchDegradedReason::IndexStopped);
    assert_eq!(
        degraded.searched_fields,
        vec!["subject", "sender", "preview"]
    );
}

#[tokio::test]
async fn a_query_the_fallback_cannot_honour_is_refused_by_name_not_loosened() {
    let state = stopped_state().await;
    let account = state.default_account_id();
    store_message(&state, &account, "a", "Quarterly invoice", "x@one.test", 1).await;

    for (query, named) in [
        ("invoice is:unread", "is:unread"),
        ("invoice after:2024-01-01", "after:2024-01-01"),
        ("invoice label:work", "label:work"),
        ("invoice OR lunch", "OR"),
        ("invoice -lunch", "-lunch"),
        ("to:bob@one.test", "to:bob@one.test"),
        ("body:invoice", "body:invoice"),
    ] {
        let (message, code) = error_of(send(&state, search_request(query, None, 10, 0)).await);

        assert_eq!(code, "search_index_unavailable", "{query}");
        assert!(message.contains(named), "{query}: {message}");
        assert!(
            message.contains("Search is limited while the index is unavailable"),
            "{query}: {message}"
        );
        assert!(
            message.contains("mxr doctor --reindex"),
            "{query}: {message}"
        );
    }
}

#[tokio::test]
async fn account_scope_is_honoured_and_a_disabled_account_never_leaks() {
    let state = stopped_state().await;
    let first = state.default_account_id();
    let second = crate::test_fixtures::test_account();
    let disabled = crate::test_fixtures::test_account();
    state.store.insert_account(&second).await.unwrap();
    state.store.insert_account(&disabled).await.unwrap();
    let in_first = store_message(&state, &first, "a", "shared word", "x@one.test", 1).await;
    let in_second = store_message(&state, &second.id, "b", "shared word", "y@two.test", 2).await;
    let in_disabled =
        store_message(&state, &disabled.id, "c", "shared word", "z@three.test", 3).await;
    state
        .store
        .set_account_enabled(&disabled.id, false)
        .await
        .unwrap();

    let everywhere = page(send(&state, search_request("shared", None, 10, 0)).await);
    let only_second = page(
        send(
            &state,
            search_request("shared", Some(second.id.clone()), 10, 0),
        )
        .await,
    );
    let only_disabled = page(
        send(
            &state,
            search_request("shared", Some(disabled.id.clone()), 10, 0),
        )
        .await,
    );

    assert_eq!(
        everywhere.ids,
        vec![in_first.id.as_str(), in_second.id.as_str()]
    );
    assert_eq!(only_second.ids, vec![in_second.id.as_str()]);
    assert!(
        !only_disabled.ids.contains(&in_disabled.id.as_str()) && only_disabled.ids.is_empty(),
        "naming a disabled account must not reach its mail"
    );
}

#[tokio::test]
async fn triage_will_not_judge_the_narrower_result_set() {
    let state = stopped_state().await;
    let account = state.default_account_id();
    store_message(&state, &account, "a", "Quarterly invoice", "x@one.test", 1).await;

    let (message, code) = error_of(
        send(
            &state,
            Request::TriageSearch {
                query: "invoice".into(),
                limit: 10,
                offset: 0,
                account_id: None,
                mode: Some(SearchMode::Lexical),
                sort: Some(SortOrder::DateDesc),
            },
        )
        .await,
    );

    assert_eq!(code, "search_index_unavailable");
    assert!(message.contains("Triage needs the full index"), "{message}");
}
