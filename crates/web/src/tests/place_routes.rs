//! Bridge tests for Reading and Paper trail: each route forwards exactly
//! the daemon request it names, with the protocol's defaults.

use super::*;
use mxr_protocol::{MailPlaceData, SenderKindData};
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
                data: ResponseData::MessagesPinned {
                    changed: 1,
                    pinned: true,
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
async fn place_routes_forward_their_requests() {
    let (_temp, addr, seen) = serve_recording().await;
    let client = reqwest::Client::new();
    let account = AccountId::new();
    let message = MessageId::new();

    for request in [
        client.get(format!("http://{addr}/api/v1/mail/places/reading")),
        client.get(format!(
            "http://{addr}/api/v1/mail/places/paper-trail?account={account}&sender=a@b.example&limit=5&offset=10&messages_per_bundle=2&message_offset=3"
        )),
        client
            .post(format!("http://{addr}/api/v1/mail/places/paper-trail/sweep"))
            .json(&serde_json::json!({
                "sender_email": "no-reply@shop.example",
                "dry_run": false,
                "preview_token": "tok-1",
            })),
        client.get(format!("http://{addr}/api/v1/mail/messages/{message}/kind")),
        client
            .post(format!("http://{addr}/api/v1/mail/messages/pin"))
            .json(&serde_json::json!({ "message_ids": [message.to_string()], "pinned": true })),
        client
            .post(format!("http://{addr}/api/v1/mail/senders/kind"))
            .json(&serde_json::json!({
                "account_id": account.to_string(),
                "sender_email": "editor@weekly.example",
                "kind": "paper_trail",
            })),
        client
            .post(format!("http://{addr}/api/v1/mail/senders/kind"))
            .json(&serde_json::json!({
                "account_id": account.to_string(),
                "sender_email": "editor@weekly.example",
                "kind": null,
            })),
    ] {
        let response = request.bearer_auth(TEST_AUTH_TOKEN).send().await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
    }

    let seen = seen.lock().unwrap();
    assert!(matches!(
        &seen[0],
        Request::ListPlace {
            place: MailPlaceData::Reading,
            account_id: None,
            sender_email: None,
            limit: 50,
            offset: 0,
            messages_per_bundle: 20,
            message_offset: 0,
        }
    ));
    assert!(matches!(
        &seen[1],
        Request::ListPlace {
            place: MailPlaceData::PaperTrail,
            account_id: Some(id),
            sender_email: Some(sender),
            limit: 5,
            offset: 10,
            messages_per_bundle: 2,
            message_offset: 3,
        } if *id == account && sender == "a@b.example"
    ));
    assert!(matches!(
        &seen[2],
        Request::SweepPlace {
            place: MailPlaceData::PaperTrail,
            account_id: None,
            sender_email: Some(sender),
            dry_run: false,
            preview_token: Some(token),
        } if sender == "no-reply@shop.example" && token == "tok-1"
    ));
    assert!(matches!(&seen[3], Request::GetMessageKind { message_id } if *message_id == message));
    assert!(
        matches!(&seen[4], Request::PinMessages { pinned: true, message_ids } if message_ids.len() == 1)
    );
    assert!(matches!(
        &seen[5],
        Request::SetSenderKind {
            kind: Some(SenderKindData::PaperTrail),
            ..
        }
    ));
    assert!(matches!(
        &seen[6],
        Request::SetSenderKind { kind: None, .. }
    ));
}

#[tokio::test]
async fn place_routes_reject_bad_input_before_the_daemon() {
    let (_temp, addr, seen) = serve_recording().await;
    let client = reqwest::Client::new();
    for request in [
        client.get(format!("http://{addr}/api/v1/mail/places/inbox")),
        client
            .post(format!("http://{addr}/api/v1/mail/messages/pin"))
            .json(&serde_json::json!({ "message_ids": [], "pinned": true })),
        client
            .post(format!("http://{addr}/api/v1/mail/senders/kind"))
            .json(&serde_json::json!({
                "account_id": AccountId::new().to_string(),
                "sender_email": " ",
            })),
    ] {
        let response = request.bearer_auth(TEST_AUTH_TOKEN).send().await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    }
    assert!(seen.lock().unwrap().is_empty());
}
