//! With the lexical index unavailable, a plain search still answers from
//! SQLite and says so; a query the fallback cannot honour is refused by name;
//! a healthy daemon's reply is untouched.

use super::*;
use crate::test_fixtures::TestEnvelopeBuilder;
use chrono::{Duration, Utc};
use mxr_core::types::{Envelope, MessageFlags, SearchMode, SortOrder};
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
    has_more: bool,
    next_offset: Option<u32>,
    degraded: Option<SearchDegraded>,
}

fn page(response: Response) -> Page {
    match response {
        Response::Ok {
            data:
                ResponseData::SearchResults {
                    results,
                    total,
                    has_more,
                    next_offset,
                    degraded,
                    ..
                },
        } => Page {
            ids: results
                .into_iter()
                .map(|item| item.message_id.as_str())
                .collect(),
            total,
            has_more,
            next_offset,
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

/// A message in the store only. Its search-table row comes from the store's
/// triggers; the lexical index never sees it, which is the point.
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
async fn subject_and_from_fields_work_without_the_index() {
    let state = stopped_state().await;
    let account = state.default_account_id();
    let by_subject = store_message(&state, &account, "a", "Roadmap review", "x@one.test", 1).await;
    let by_sender = store_message(&state, &account, "b", "Other", "roadmap@two.test", 2).await;

    let subject = page(send(&state, search_request("subject:roadmap", None, 10, 0)).await);
    let sender = page(send(&state, search_request("from:roadmap@two.test", None, 10, 0)).await);

    assert_eq!(subject.ids, vec![by_subject.id.as_str()]);
    assert_eq!(sender.ids, vec![by_sender.id.as_str()]);
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
async fn counts_and_grouping_have_no_limited_form_and_say_why() {
    let state = stopped_state().await;
    let account = state.default_account_id();
    store_message(&state, &account, "a", "Quarterly invoice", "x@one.test", 1).await;

    let (message, code) = error_of(
        send(
            &state,
            Request::Count {
                query: "invoice".into(),
                account_id: None,
                mode: Some(SearchMode::Lexical),
            },
        )
        .await,
    );

    assert_eq!(code, "search_index_unavailable");
    assert!(message.contains("needs the full index"), "{message}");
    assert!(!message.contains("worker stopped"), "{message}");
}

#[tokio::test]
async fn a_healthy_daemon_answers_from_the_index_with_no_degraded_field() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    let response = send(&state, search_request("deployment", None, 10, 0)).await;
    let json = serde_json::to_value(&response).unwrap();
    let found = page(response);

    assert!(!found.ids.is_empty());
    assert!(found.degraded.is_none());
    assert!(
        !json.to_string().contains("degraded"),
        "a healthy reply must not mention the field: {json}"
    );
}

#[tokio::test]
async fn an_index_that_finds_nothing_is_not_a_reason_to_fall_back() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let account = state.default_account_id();
    // In the store (and so in the search table) but not in the index: a
    // fallback would find it, the index correctly does not.
    store_message(&state, &account, "a", "zebra crossing", "x@one.test", 1).await;

    let found = page(send(&state, search_request("zebra", None, 10, 0)).await);

    assert!(found.ids.is_empty());
    assert!(found.degraded.is_none());
}

#[tokio::test]
async fn a_rebuild_in_progress_answers_from_sqlite_until_it_ends() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let account = state.default_account_id();
    let wanted = store_message(&state, &account, "a", "Quarterly invoice", "x@one.test", 1).await;

    let rebuild = state.search.begin_rebuild();
    let during = page(send(&state, search_request("invoice", None, 10, 0)).await);
    rebuild.finish();
    let after = page(send(&state, search_request("invoice", None, 10, 0)).await);

    assert_eq!(during.ids, vec![wanted.id.as_str()]);
    assert_eq!(
        during.degraded.map(|degraded| degraded.reason),
        Some(SearchDegradedReason::IndexRebuilding)
    );
    // Back on the index, which was never given this message.
    assert!(after.ids.is_empty());
    assert!(after.degraded.is_none());
}

#[tokio::test]
async fn a_failed_rebuild_keeps_search_limited_until_a_later_rebuild_finishes() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let account = state.default_account_id();
    let wanted = store_message(&state, &account, "a", "Quarterly invoice", "x@one.test", 1).await;
    store_message(&state, &account, "b", "Weekly digest", "x@one.test", 2).await;

    // Reindex has no failure injection point, so this mirrors its failure path:
    // clear the index, commit the first batch, then drop the guard unfinished,
    // which is what its `?` does when a later batch fails.
    let rebuild = state.search.begin_rebuild();
    state.search.clear().await.unwrap();
    let first_batch = state
        .store
        .list_all_envelopes_paginated(1, 0)
        .await
        .unwrap()
        .into_iter()
        .map(|envelope| mxr_search::SearchIndexEntry {
            envelope,
            body: None,
            reply_later: false,
        })
        .collect();
    state
        .search
        .apply_batch(mxr_search::SearchUpdateBatch {
            entries: first_batch,
            removed_message_ids: vec![],
        })
        .await
        .unwrap();
    drop(rebuild);

    assert!(!matches!(
        state.search.availability(),
        mxr_search::IndexAvailability::Ready { .. }
    ));
    let failed = page(send(&state, search_request("invoice", None, 10, 0)).await);
    assert_eq!(failed.ids, vec![wanted.id.as_str()]);
    assert_eq!(
        failed.degraded.map(|degraded| degraded.reason),
        Some(SearchDegradedReason::IndexIncomplete)
    );

    crate::reindex::reindex(&state.search, &state.store, |_| {})
        .await
        .unwrap();

    assert!(matches!(
        state.search.availability(),
        mxr_search::IndexAvailability::Ready { .. }
    ));
    let restored = page(send(&state, search_request("invoice", None, 10, 0)).await);
    assert!(restored.degraded.is_none());
}

#[tokio::test]
async fn pages_stay_in_one_order_and_count_from_one_source() {
    let state = stopped_state().await;
    let account = state.default_account_id();
    let mut newest_first = Vec::new();
    for day in 0..5 {
        let message = store_message(
            &state,
            &account,
            &format!("p{day}"),
            "weekly digest",
            "x@one.test",
            day,
        )
        .await;
        newest_first.push(message.id.as_str());
    }

    let first = page(send(&state, search_request("digest", None, 2, 0)).await);
    let second = page(send(&state, search_request("digest", None, 2, 2)).await);
    let last = page(send(&state, search_request("digest", None, 2, 4)).await);

    assert_eq!([first.total, second.total, last.total], [5, 5, 5]);
    assert_eq!(first.next_offset, Some(2));
    assert!(first.has_more && second.has_more && !last.has_more);
    assert_eq!(last.next_offset, None);
    let all = [first.ids, second.ids, last.ids].concat();
    assert_eq!(all, newest_first);
    assert!(first.degraded.is_some() && last.degraded.is_some());
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
async fn trash_and_spam_stay_out_unless_the_query_asks_for_everywhere() {
    let state = stopped_state().await;
    let account = state.default_account_id();
    let kept = store_message(&state, &account, "a", "report", "x@one.test", 1).await;
    let mut trashed = TestEnvelopeBuilder::new()
        .account_id(account.clone())
        .provider_id("fb-trash")
        .subject("report")
        .snippet("")
        .date(Utc::now() - Duration::days(2))
        .flags(MessageFlags::TRASH)
        .build();
    trashed.from.email = "t@two.test".into();
    state.store.upsert_envelope(&trashed).await.unwrap();

    let default = page(send(&state, search_request("report", None, 10, 0)).await);
    let anywhere = page(send(&state, search_request("report in:anywhere", None, 10, 0)).await);

    assert_eq!(default.ids, vec![kept.id.as_str()]);
    assert_eq!(anywhere.ids.len(), 2);
}

#[tokio::test]
async fn a_saved_search_runs_from_sqlite_too_and_says_so() {
    let state = stopped_state().await;
    let account = state.default_account_id();
    let wanted = store_message(&state, &account, "a", "Quarterly invoice", "x@one.test", 1).await;
    let created = send(
        &state,
        Request::CreateSavedSearch {
            name: "bills".into(),
            query: "invoice".into(),
            account_id: None,
            search_mode: SearchMode::Lexical,
        },
    )
    .await;
    assert!(matches!(created, Response::Ok { .. }), "{created:?}");

    let found = page(
        send(
            &state,
            Request::RunSavedSearch {
                name: "bills".into(),
                limit: 10,
                account_id: None,
            },
        )
        .await,
    );

    assert_eq!(found.ids, vec![wanted.id.as_str()]);
    assert!(found.degraded.is_some());
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

async fn store_detailed(
    state: &AppState,
    account: &mxr_core::AccountId,
    tag: &str,
    subject: &str,
    snippet: &str,
    sender: (&str, &str),
) -> Envelope {
    let envelope = TestEnvelopeBuilder::new()
        .account_id(account.clone())
        .provider_id(format!("fb-{tag}"))
        .sender_address(sender.0, sender.1)
        .subject(subject)
        .snippet(snippet)
        .date(Utc::now() - Duration::days(1))
        .build();
    state.store.upsert_envelope(&envelope).await.unwrap();
    envelope
}

#[tokio::test]
async fn from_matches_the_whole_address_as_the_index_does() {
    let state = stopped_state().await;
    let account = state.default_account_id();
    let exact = store_detailed(
        &state,
        &account,
        "a",
        "one",
        "",
        ("Someone", "Alice@Example.com"),
    )
    .await;
    store_detailed(
        &state,
        &account,
        "b",
        "two",
        "",
        ("Someone", "alice@example.com.evil"),
    )
    .await;
    store_detailed(
        &state,
        &account,
        "c",
        "three",
        "",
        ("alice@example.com", "z@other.test"),
    )
    .await;
    store_detailed(
        &state,
        &account,
        "d",
        "four",
        "",
        ("Alice", "malice@example.com"),
    )
    .await;

    let by_address = page(
        send(
            &state,
            search_request("from:alice@example.com", None, 10, 0),
        )
        .await,
    );
    let upper = page(
        send(
            &state,
            search_request("from:ALICE@EXAMPLE.COM", None, 10, 0),
        )
        .await,
    );
    let bare_name = page(send(&state, search_request("from:alice", None, 10, 0)).await);

    assert_eq!(by_address.ids, vec![exact.id.as_str()]);
    assert_eq!(upper.ids, vec![exact.id.as_str()]);
    assert!(
        bare_name.ids.is_empty(),
        "a bare name matches no address, as in the index"
    );
    assert!(bare_name.degraded.is_some());
}

#[tokio::test]
async fn a_compound_word_needs_every_token_not_the_phrase() {
    let state = stopped_state().await;
    let account = state.default_account_id();
    let apart = store_detailed(
        &state,
        &account,
        "a",
        "alpha middle beta",
        "",
        ("S", "a@one.test"),
    )
    .await;
    let across_fields = store_detailed(
        &state,
        &account,
        "b",
        "alpha",
        "and beta",
        ("S", "b@two.test"),
    )
    .await;
    store_detailed(
        &state,
        &account,
        "c",
        "alpha only",
        "",
        ("S", "c@three.test"),
    )
    .await;

    let found = page(send(&state, search_request("alpha-beta", None, 10, 0)).await);
    let exact = page(send(&state, search_request("+alpha-beta", None, 10, 0)).await);
    let phrase = page(send(&state, search_request("\"alpha beta\"", None, 10, 0)).await);

    let mut both = vec![apart.id.as_str(), across_fields.id.as_str()];
    both.sort();
    let mut got = found.ids.clone();
    got.sort();
    assert_eq!(got, both);
    assert_eq!(exact.ids.len(), 2);
    assert!(
        phrase.ids.is_empty(),
        "only a quoted phrase must be adjacent"
    );
}
