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

#[tokio::test]
async fn cors_exposes_the_owner_on_authenticated_success_and_conflict() {
    let temp = TempDir::new().unwrap();
    let socket = temp.path().join("mxr.sock");
    let _ipc = spawn_fake_ipc_server(&socket, |_| None, None).await;
    let addr = bind_and_serve(
        std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        0,
        WebServerConfig::new(socket, TEST_AUTH_TOKEN.into()),
    )
    .await
    .unwrap();
    let mut stale = test_workspace();
    stale.recovery_generation = "00000000-0000-7000-8000-000000000003".into();
    for (owner, expected) in [
        (test_workspace(), reqwest::StatusCode::OK),
        (stale, reqwest::StatusCode::CONFLICT),
    ] {
        let response = reqwest::Client::new()
            .get(format!(
                "http://{addr}/api/v1/mail/archive-operations/workspace"
            ))
            .bearer_auth(TEST_AUTH_TOKEN)
            .header("origin", "http://localhost:3000")
            .header("x-mxr-workspace", serde_json::to_string(&owner).unwrap())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
        assert_eq!(
            response.headers()["access-control-expose-headers"],
            "x-mxr-workspace"
        );
        assert_eq!(
            response.headers()["access-control-allow-origin"],
            "http://localhost:3000"
        );
        assert_eq!(
            response.headers()["x-mxr-workspace"],
            serde_json::to_string(&test_workspace()).unwrap()
        );
    }
}
