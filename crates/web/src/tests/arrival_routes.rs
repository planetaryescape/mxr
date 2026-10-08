//! Bridge tests for the arrivals line, moves and corrections: each route
//! forwards exactly the daemon request it names, and bad input never
//! reaches the daemon.

use super::*;
use mxr_protocol::{ArrivalBucketData, ModeKindData};
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
                data: ResponseData::Corrections {
                    corrections: Vec::new(),
                },
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
async fn arrival_routes_forward_their_requests() {
    let (_temp, addr, seen) = serve_recording().await;
    let client = reqwest::Client::new();
    let account = AccountId::new();
    let message = MessageId::new();

    for request in [
        client.get(format!("http://{addr}/api/v1/mail/arrivals")),
        client.get(format!(
            "http://{addr}/api/v1/mail/arrivals?account={account}&mark_seen=true"
        )),
        client.get(format!(
            "http://{addr}/api/v1/mail/arrivals/list?bucket=reading&since=2026-10-07T08:12:00Z&until=2026-10-07T10:00:00Z&limit=31"
        )),
        client
            .post(format!("http://{addr}/api/v1/mail/arrivals/modes"))
            .json(&serde_json::json!({ "message_ids": [message.to_string()] })),
        client
            .post(format!("http://{addr}/api/v1/mail/messages/{message}/move"))
            .json(&serde_json::json!({ "mode": "reading" })),
        client
            .post(format!("http://{addr}/api/v1/mail/messages/{message}/move"))
            .json(&serde_json::json!({
                "mode": "updates",
                "sender": true,
                "dry_run": true,
                "source": "not_sure",
            })),
        client.post(format!("http://{addr}/api/v1/mail/moves/42/undo")),
        client.get(format!(
            "http://{addr}/api/v1/mail/corrections?account={account}&limit=5"
        )),
    ] {
        let response = request.bearer_auth(TEST_AUTH_TOKEN).send().await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
    }

    let seen = seen.lock().unwrap();
    assert!(matches!(
        &seen[0],
        Request::GetArrivals {
            account_id: None,
            mark_seen: false,
        }
    ));
    assert!(matches!(
        &seen[1],
        Request::GetArrivals {
            account_id: Some(id),
            mark_seen: true,
        } if *id == account
    ));
    assert!(matches!(
        &seen[2],
        Request::ListArrivals {
            account_id: None,
            bucket: Some(ArrivalBucketData::Reading),
            since: Some(_),
            until: Some(_),
            limit: 31,
        }
    ));
    assert!(
        matches!(&seen[3], Request::GetArrivalModes { message_ids } if message_ids == std::slice::from_ref(&message))
    );
    assert!(matches!(
        &seen[4],
        Request::MoveMessage {
            mode: ModeKindData::Reading,
            sender: false,
            dry_run: false,
            source: None,
            ..
        }
    ));
    assert!(matches!(
        &seen[5],
        Request::MoveMessage {
            mode: ModeKindData::Updates,
            sender: true,
            dry_run: true,
            source: Some(source),
            ..
        } if source == "not_sure"
    ));
    assert!(matches!(
        &seen[6],
        Request::UndoMove { correction_id: 42 }
    ));
    assert!(matches!(
        &seen[7],
        Request::ListCorrections {
            account_id: Some(_),
            limit: 5,
        }
    ));
}

#[tokio::test]
async fn arrival_routes_reject_bad_input_before_the_daemon() {
    let (_temp, addr, seen) = serve_recording().await;
    let client = reqwest::Client::new();
    let message = MessageId::new();
    let too_many: Vec<String> = (0..201).map(|_| MessageId::new().to_string()).collect();
    for request in [
        client.get(format!("http://{addr}/api/v1/mail/arrivals/list?bucket=inbox")),
        client.get(format!(
            "http://{addr}/api/v1/mail/arrivals/list?since=2026-10-07T10:00:00Z&until=2026-10-07T08:00:00Z"
        )),
        client
            .post(format!("http://{addr}/api/v1/mail/arrivals/modes"))
            .json(&serde_json::json!({ "message_ids": [] })),
        client
            .post(format!("http://{addr}/api/v1/mail/arrivals/modes"))
            .json(&serde_json::json!({ "message_ids": too_many })),
        client
            .post(format!("http://{addr}/api/v1/mail/messages/{message}/move"))
            .json(&serde_json::json!({ "mode": "now" })),
        client
            .post(format!("http://{addr}/api/v1/mail/messages/not-an-id/move"))
            .json(&serde_json::json!({ "mode": "reading" })),
    ] {
        let response = request.bearer_auth(TEST_AUTH_TOKEN).send().await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    }
    assert!(seen.lock().unwrap().is_empty());
}
