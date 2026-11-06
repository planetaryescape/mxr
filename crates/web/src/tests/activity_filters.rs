//! Activity list/count filters: every query form a caller sends reaches the
//! daemon as one `ActivityFilter`, the two routes apply the same filter, and a
//! value the bridge can't honour is a 400 rather than an unfiltered page.
//!
//! Regression: axum's default `Query` 400'd on `?source=cli` and silently
//! dropped `?source[0]=cli` as an unknown field, so the Activity page showed
//! every row no matter which chips the user picked.

use super::*;
use mxr_protocol::{ActivityFilter, ActivityTier, ClientKind};
use std::sync::{Arc, Mutex};

async fn serve_recording() -> (TempDir, std::net::SocketAddr, Arc<Mutex<Vec<Request>>>) {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let seen = Arc::new(Mutex::new(Vec::<Request>::new()));
    let seen_for_ipc = seen.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| {
            let response = match &request {
                Request::ListActivity { .. } => ResponseData::ActivityEntries {
                    entries: Vec::new(),
                    next_cursor: None,
                },
                Request::CountActivity { .. } => ResponseData::ActivityCount { count: 0 },
                _ => return None,
            };
            seen_for_ipc.lock().unwrap().push(request);
            Some(Response::Ok { data: response })
        },
        None,
    )
    .await;
    let addr = bind_and_serve(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        0,
        WebServerConfig::new(socket_path, TEST_AUTH_TOKEN.into()),
    )
    .await
    .unwrap();
    (temp, addr, seen)
}

fn only_filter(seen: &Arc<Mutex<Vec<Request>>>) -> ActivityFilter {
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1, "one request must reach the daemon: {seen:?}");
    match &seen[0] {
        Request::ListActivity { filter, .. } | Request::CountActivity { filter, .. } => {
            filter.clone()
        }
        other => panic!("expected an activity query, got {other:?}"),
    }
}

fn sources(filter: &ActivityFilter) -> Vec<ClientKind> {
    filter.sources.clone()
}

fn get(client: &reqwest::Client, url: String) -> reqwest::RequestBuilder {
    client.get(url).bearer_auth(TEST_AUTH_TOKEN)
}

#[tokio::test]
async fn repeated_keys_filter_the_list_and_the_count_identically() {
    let (_temp, addr, seen) = serve_recording().await;
    let client = test_http_client();
    let query =
        "source=cli&source=web&tier=important&tier=standard&action=mail.send,search.run&limit=25";

    let list = get(
        &client,
        format!("http://{addr}/api/v1/admin/activity?{query}"),
    )
    .send()
    .await
    .unwrap();
    assert_eq!(list.status(), reqwest::StatusCode::OK);
    let list_filter = only_filter(&seen);
    assert_eq!(
        sources(&list_filter),
        vec![ClientKind::Cli, ClientKind::Web],
        "both repeated sources reach the daemon"
    );
    assert_eq!(
        list_filter.tiers,
        vec![ActivityTier::Important, ActivityTier::Standard]
    );
    assert_eq!(list_filter.actions, vec!["mail.send", "search.run"]);

    seen.lock().unwrap().clear();
    let count = get(
        &client,
        format!("http://{addr}/api/v1/admin/activity/count?{query}"),
    )
    .send()
    .await
    .unwrap();
    assert_eq!(count.status(), reqwest::StatusCode::OK);
    let count_filter = only_filter(&seen);
    assert_eq!(
        count_filter.sources, list_filter.sources,
        "the count applies the list's sources"
    );
    assert_eq!(count_filter.tiers, list_filter.tiers);
    assert_eq!(count_filter.actions, list_filter.actions);
    assert_eq!(count_filter.account_id, list_filter.account_id);
    assert_eq!(count_filter.include_redacted, list_filter.include_redacted);
}

#[tokio::test]
async fn comma_and_bracket_forms_apply_the_same_filter() {
    let (_temp, addr, seen) = serve_recording().await;
    let client = test_http_client();

    for query in [
        "source=cli,web",
        "source[0]=cli&source[1]=web",
        "source[]=cli&source[]=web",
    ] {
        seen.lock().unwrap().clear();
        let response = get(
            &client,
            format!("http://{addr}/api/v1/admin/activity?{query}"),
        )
        .send()
        .await
        .unwrap();
        assert_eq!(
            response.status(),
            reqwest::StatusCode::OK,
            "query `{query}`"
        );
        assert_eq!(
            sources(&only_filter(&seen)),
            vec![ClientKind::Cli, ClientKind::Web],
            "query `{query}` must keep both sources"
        );
    }
}

#[tokio::test]
async fn every_activity_source_chip_reaches_the_daemon() {
    let (_temp, addr, seen) = serve_recording().await;
    let client = test_http_client();
    let chips = [
        "human",
        "tui",
        "cli",
        "script",
        "web",
        "daemon",
        "agent",
        "mcp",
        "assistant",
    ];

    for chip in chips {
        seen.lock().unwrap().clear();
        let response = get(
            &client,
            format!("http://{addr}/api/v1/admin/activity?source={chip}"),
        )
        .send()
        .await
        .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK, "chip `{chip}`");
        let filter = only_filter(&seen);
        assert_eq!(
            filter.sources.len(),
            1,
            "chip `{chip}` must not be dropped: {filter:?}"
        );
        assert_eq!(filter.sources[0].as_str(), chip);
    }
}

#[tokio::test]
async fn an_unknown_source_or_tier_is_refused_rather_than_dropped() {
    let (_temp, addr, seen) = serve_recording().await;
    let client = test_http_client();

    for query in [
        "source[0]=carrier-pigeon",
        "source=cli,carrier-pigeon",
        "tier=loud",
    ] {
        let response = get(
            &client,
            format!("http://{addr}/api/v1/admin/activity?{query}"),
        )
        .send()
        .await
        .unwrap();
        assert_eq!(
            response.status(),
            reqwest::StatusCode::BAD_REQUEST,
            "query `{query}` must be refused, not run unfiltered"
        );
    }
    assert!(
        seen.lock().unwrap().is_empty(),
        "a refused filter never reaches the daemon"
    );
}

#[tokio::test]
async fn a_wrong_type_scalar_is_a_bad_request() {
    let (_temp, addr, seen) = serve_recording().await;
    let client = test_http_client();

    for query in ["limit=lots", "since=yesterday", "include_redacted=maybe"] {
        let response = get(
            &client,
            format!("http://{addr}/api/v1/admin/activity?{query}"),
        )
        .send()
        .await
        .unwrap();
        assert_eq!(
            response.status(),
            reqwest::StatusCode::BAD_REQUEST,
            "query `{query}`"
        );
    }
    assert!(seen.lock().unwrap().is_empty());
}
