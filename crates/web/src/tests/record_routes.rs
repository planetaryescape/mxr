//! Bridge tests for Archive: each route forwards exactly the daemon request
//! it names, with the protocol's defaults, and bad input never reaches the
//! daemon.

use super::*;
use mxr_protocol::{RecordEditData, RecordKindData};
use std::sync::{Arc, Mutex};

async fn serve_recording() -> (TempDir, std::net::SocketAddr, Arc<Mutex<Vec<Request>>>) {
    let temp = TempDir::new().unwrap();
    let socket_path = temp.path().join("mxr.sock");
    let seen = Arc::new(Mutex::new(Vec::<Request>::new()));
    let seen_for_ipc = seen.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket_path,
        move |request| {
            seen_for_ipc.lock().unwrap().push(request);
            Some(Response::Ok {
                data: ResponseData::Todos { todos: vec![] },
            })
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

#[tokio::test]
async fn record_routes_forward_their_requests() {
    let (_temp, addr, seen) = serve_recording().await;
    let client = reqwest::Client::new();
    let account = AccountId::new();
    let message = MessageId::new();

    for request in [
        client.get(format!(
            "http://{addr}/api/v1/mail/records?kind=order,booking&issuer=Dell&year=2025&has_pdf=true&limit=50&offset=50"
        )),
        client.get(format!(
            "http://{addr}/api/v1/mail/records/answer?q=lisbon%20booking%20ref&account={account}"
        )),
        client.get(format!("http://{addr}/api/v1/mail/records/rec_3f2a")),
        client
            .post(format!("http://{addr}/api/v1/mail/records/rec_3f2a/field"))
            .json(&serde_json::json!({ "edit": { "op": "set", "field": "amount", "value": "£12.50" }, "dry_run": true })),
        client
            .post(format!("http://{addr}/api/v1/mail/records/dismiss"))
            .json(&serde_json::json!({ "record_ids": ["rec_3f2a"] })),
        client
            .post(format!("http://{addr}/api/v1/mail/records/file"))
            .json(&serde_json::json!({ "message_id": message.to_string(), "kind": "contract", "dry_run": true })),
        client
            .post(format!("http://{addr}/api/v1/mail/records/sender"))
            .json(&serde_json::json!({ "message_id": message.to_string(), "verdict": "always" })),
        client
            .post(format!("http://{addr}/api/v1/mail/records/export"))
            .json(&serde_json::json!({ "filter": { "year": 2025 }, "dry_run": true })),
        client.get(format!(
            "http://{addr}/api/v1/mail/records/subscriptions?account={account}"
        )),
    ] {
        let response = request.bearer_auth(TEST_AUTH_TOKEN).send().await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
    }

    let seen = seen.lock().unwrap();
    assert!(matches!(
        &seen[0],
        Request::ListRecords { account_id: None, filter, limit: 50, offset: 50 }
            if filter.kinds == vec![RecordKindData::Order, RecordKindData::Booking]
                && filter.issuer.as_deref() == Some("Dell")
                && filter.year == Some(2025)
                && filter.has_pdf == Some(true)
    ));
    assert!(matches!(
        &seen[1],
        Request::AnswerFromRecords { query, account_id: Some(id), fallback: true, limit: 4, list: false, offset: 0, list_limit: 200 }
            if query == "lisbon booking ref" && *id == account
    ));
    assert!(matches!(&seen[2], Request::GetRecord { record_id } if record_id == "rec_3f2a"));
    assert!(matches!(
        &seen[3],
        Request::SetRecordField { edit: RecordEditData::Set { field, .. }, dry_run: true, apply_to_sender: false, .. }
            if field == "amount"
    ));
    assert!(matches!(
        &seen[4],
        Request::DismissRecord { restore: false, dry_run: false, record_ids } if record_ids.len() == 1
    ));
    assert!(matches!(
        &seen[5],
        Request::FileRecord { message_id, kind: Some(RecordKindData::Contract), dry_run: true }
            if *message_id == message
    ));
    assert!(matches!(
        &seen[6],
        Request::SetRecordSender { verdict: Some(verdict), dry_run: false, .. } if verdict == "always"
    ));
    assert!(matches!(
        &seen[7],
        Request::ExportRecords { filter, dry_run: true, attachments_dir: None, .. } if filter.year == Some(2025)
    ));
    assert!(matches!(
        &seen[8],
        Request::ListRecordSubscriptions { account_id: Some(id) } if *id == account
    ));
}

#[tokio::test]
async fn record_routes_reject_bad_input_before_the_daemon() {
    let (_temp, addr, seen) = serve_recording().await;
    let client = reqwest::Client::new();
    for request in [
        client.get(format!("http://{addr}/api/v1/mail/records?kind=parcel")),
        client.get(format!("http://{addr}/api/v1/mail/records/answer?q=%20")),
        client.get(format!(
            "http://{addr}/api/v1/mail/records/subscriptions?account=not-an-id"
        )),
        client
            .post(format!("http://{addr}/api/v1/mail/records/dismiss"))
            .json(&serde_json::json!({ "record_ids": [] })),
        client
            .post(format!("http://{addr}/api/v1/mail/records/file"))
            .json(&serde_json::json!({ "message_id": "not-an-id" })),
        client
            .post(format!("http://{addr}/api/v1/mail/records/sender"))
            .json(&serde_json::json!({ "message_id": MessageId::new().to_string(), "verdict": "sometimes" })),
    ] {
        let response = request.bearer_auth(TEST_AUTH_TOKEN).send().await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    }
    assert!(seen.lock().unwrap().is_empty());
}
