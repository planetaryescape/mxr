use super::*;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn valid_selected_header_cannot_read_or_delete_another_owners_private_file() {
    let temp = TempDir::new().unwrap();
    let socket = temp.path().join("mxr.sock");
    let effects = Arc::new(Mutex::new(0));
    let observed = effects.clone();
    let _ipc = spawn_fake_ipc_server(
        &socket,
        move |request| {
            if !matches!(request, Request::GetStatus | Request::GetArchiveWorkspace) {
                *observed.lock().unwrap() += 1;
            }
            Some(Response::Ok {
                data: ResponseData::Ack,
            })
        },
        None,
    )
    .await;
    let addr = bind_and_serve(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        0,
        WebServerConfig::new(socket, TEST_AUTH_TOKEN.into()),
    )
    .await
    .unwrap();
    let path = temp.path().join("private.md");
    let private = b"Private unsent A text\n\n";
    tokio::fs::write(&path, private).await.unwrap();
    let mut original = test_workspace();
    original.workspace_id = "00000000-0000-7000-8000-000000000003".into();
    let sidecar = draft_session_metadata_path(&path);
    tokio::fs::write(
        &sidecar,
        serde_json::to_vec(
            &json!({ "compose_workspace": original, "compose_recovery_source": "/private/a.md" }),
        )
        .unwrap(),
    )
    .await
    .unwrap();
    let sidecar_before = tokio::fs::read(&sidecar).await.unwrap();
    for route in ["collaborators", "discard"] {
        let response = test_http_client().post(format!("http://{addr}/api/v1/mail/compose/session/{route}"))
            .bearer_auth(TEST_AUTH_TOKEN).json(&json!({ "draft_path": path, "account_id": AccountId::new(), "expected_revision": 1 })).send().await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::CONFLICT, "{route}");
        assert_eq!(tokio::fs::read(&path).await.unwrap(), private);
        assert_eq!(tokio::fs::read(&sidecar).await.unwrap(), sidecar_before);
    }
    assert_eq!(*effects.lock().unwrap(), 0);
}
