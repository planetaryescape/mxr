//! Bridge tests for To do: each route forwards exactly the daemon request
//! it names, with the protocol's defaults, and bad input never reaches the
//! daemon.

use super::*;
use mxr_protocol::{
    TodoCatchupDecisionData, TodoResolutionActionData, TodoStateActionData, TodoStateData,
};
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
async fn todo_routes_forward_their_requests() {
    let (_temp, addr, seen) = serve_recording().await;
    let client = test_http_client();
    let account = AccountId::new();
    let message = MessageId::new();

    for request in [
        client.get(format!("http://{addr}/api/v1/mail/todos?mark_seen=true")),
        client.get(format!(
            "http://{addr}/api/v1/mail/todos/in/expired?account={account}&limit=5"
        )),
        client.get(format!("http://{addr}/api/v1/mail/todos/3f2a91c4")),
        client
            .post(format!("http://{addr}/api/v1/mail/todos/state"))
            .json(
                &serde_json::json!({ "todo_ids": ["3f2a91c4"], "action": "done", "dry_run": true }),
            ),
        client
            .post(format!(
                "http://{addr}/api/v1/mail/todos/3f2a91c4/resolution"
            ))
            .json(&serde_json::json!({
                "suggestion_id": "offer-1",
                "action": "preview",
                "dry_run": true
            })),
        client
            .post(format!("http://{addr}/api/v1/mail/todos/3f2a91c4/schedule"))
            .json(&serde_json::json!({ "when": "mon 9am", "time_zone": "Europe/London" })),
        client
            .post(format!("http://{addr}/api/v1/mail/todos/3f2a91c4/edit"))
            .json(&serde_json::json!({ "edits": [{ "field": "due", "value": "fri" }] })),
        client
            .post(format!("http://{addr}/api/v1/mail/todos"))
            .json(
                &serde_json::json!({ "message_id": message.to_string(), "title": "Send the form" }),
            ),
        client.get(format!("http://{addr}/api/v1/mail/todos/catchup")),
        client
            .post(format!("http://{addr}/api/v1/mail/todos/catchup"))
            .json(
                &serde_json::json!({ "decision": { "decision": "let_go_all" }, "dry_run": true }),
            ),
    ] {
        let response = request.bearer_auth(TEST_AUTH_TOKEN).send().await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
    }

    let seen = seen.lock().unwrap();
    assert!(matches!(
        &seen[0],
        Request::GetTodoRunway {
            account_id: None,
            mark_seen: true
        }
    ));
    assert!(matches!(
        &seen[1],
        Request::ListTodos { account_id: Some(id), state: TodoStateData::Expired, limit: 5 } if *id == account
    ));
    assert!(matches!(&seen[2], Request::GetTodo { todo_id } if todo_id == "3f2a91c4"));
    assert!(matches!(
        &seen[3],
        Request::SetTodoState { action: TodoStateActionData::Done, dry_run: true, todo_ids } if todo_ids.len() == 1
    ));
    assert!(matches!(
        &seen[4],
        Request::ResolveTodoSuggestion {
            todo_id,
            suggestion_id,
            action: TodoResolutionActionData::Preview,
            dry_run: true,
        } if todo_id == "3f2a91c4" && suggestion_id == "offer-1"
    ));
    assert!(matches!(
        &seen[5],
        Request::ScheduleTodo { when: Some(when), time_zone: Some(_), dry_run: false, .. } if when == "mon 9am"
    ));
    assert!(matches!(&seen[6], Request::UpdateTodo { edits, .. } if edits[0].field == "due"));
    assert!(matches!(
        &seen[7],
        Request::CreateTodo { message_id, kind: None, due: None, dry_run: false, .. } if *message_id == message
    ));
    assert!(matches!(
        &seen[8],
        Request::GetTodoCatchup { account_id: None }
    ));
    assert!(matches!(
        &seen[9],
        Request::SetTodoCatchup {
            decision: TodoCatchupDecisionData::LetGoAll,
            dry_run: true,
            ..
        }
    ));
}

#[tokio::test]
async fn got_it_with_only_a_thread_id_is_a_preview_and_a_send_carries_its_token() {
    let (_temp, addr, seen) = serve_recording().await;
    let client = test_http_client();
    let thread = ThreadId::new();
    for body in [
        serde_json::json!({ "thread_id": thread.to_string() }),
        serde_json::json!({
            "thread_id": thread.to_string(),
            "dry_run": false,
            "expect_text": "Thanks Samir, got it.",
            "preview_token": "1.abc"
        }),
    ] {
        let response = client
            .post(format!("http://{addr}/api/v1/mail/people/ack"))
            .json(&body)
            .bearer_auth(TEST_AUTH_TOKEN)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::OK);
    }
    let seen = seen.lock().unwrap();
    assert!(matches!(
        &seen[0],
        Request::AckMessage {
            dry_run: true,
            expect_text: None,
            preview_token: None,
            ..
        }
    ));
    assert!(matches!(
        &seen[1],
        Request::AckMessage { dry_run: false, preview_token: Some(token), .. } if token == "1.abc"
    ));
}
