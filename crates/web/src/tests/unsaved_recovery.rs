use super::*;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn explicit_unsaved_recovery_retry_preserves_identity_and_never_overwrites_a_lost_create() {
    for lost_response in [false, true] {
        let temp = TempDir::new().unwrap();
        let socket = temp.path().join("mxr.sock");
        let account = AccountId::new();
        let store = FakeDraftStore::default();
        let saves = Arc::new(Mutex::new(0));
        let attempts = saves.clone();
        let saved = store.clone();
        let responder = draft_store_responder(store.clone(), sample_account(&account));
        let _ipc = spawn_fake_ipc_server(
            &socket,
            move |request| {
                if let Request::SaveDraft { draft } = &request {
                    let mut attempts = attempts.lock().unwrap();
                    *attempts += 1;
                    if *attempts == 1 {
                        if lost_response {
                            saved.insert(draft.clone());
                        }
                        return Some(Response::error_kinded(
                            "connection response lost".into(),
                            mxr_protocol::IpcErrorKind::Internal,
                        ));
                    }
                    if saved.snapshot().iter().any(|existing| {
                        existing.id == draft.id && existing.content != draft.content
                    }) {
                        return Some(Response::Error {
                            message: "Draft revision conflict".into(),
                            kind: mxr_protocol::IpcErrorKind::Store,
                            code: "draft_revision_conflict".into(),
                            retryable: false,
                            details: Some(
                                json!({"expected_revision": null, "current_revision": 1}),
                            ),
                        });
                    }
                }
                responder(request)
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
        let client = reqwest::Client::new();
        let start: serde_json::Value = client
            .post(format!("http://{addr}/api/v1/mail/compose/session"))
            .bearer_auth(TEST_AUTH_TOKEN)
            .json(&json!({"kind": "new"}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let source = compose_draft_path(&start);
        let id = start["session"]["draftId"].clone();
        let dir = compose_attachment_dir(Path::new(&source)).unwrap();
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let upload = dir.join("first.txt");
        tokio::fs::write(&upload, b"retry bytes").await.unwrap();
        let first = client.post(format!("http://{addr}/api/v1/mail/compose/session/update"))
            .bearer_auth(TEST_AUTH_TOKEN).json(&json!({"draft_path": source, "account_id": account,
                "to": "alice@example.com", "cc": "", "bcc": "", "subject": "Retry", "from": "me@example.com",
                "body": "first snapshot", "attach": [upload]})).send().await.unwrap();
        assert!(!first.status().is_success());
        let failure: serde_json::Value = first.json().await.unwrap();
        let recovery = failure["recovery_session"]["draft_path"].as_str().unwrap();
        assert_eq!(failure["recovery_session"]["draft_id"], id);
        assert!(failure["recovery_session"]["revision"].is_null());
        let attachments = failure["recovery_session"]["attachment_paths"].clone();
        let canonical_before = serde_json::to_value(store.snapshot()).unwrap();
        let retry = client.post(format!("http://{addr}/api/v1/mail/compose/session/update"))
            .bearer_auth(TEST_AUTH_TOKEN).json(&json!({"draft_path": recovery, "account_id": account,
                "retry_recovery": true, "expected_revision": null,
                "to": "alice@example.com", "cc": "", "bcc": "", "subject": "Retry", "from": "me@example.com",
                "body": "latest retry snapshot", "attach": attachments})).send().await.unwrap();
        assert_eq!(
            load_compose_session(Path::new(recovery)).await.unwrap()["bodyMarkdown"],
            "latest retry snapshot"
        );
        assert_eq!(*saves.lock().unwrap(), 2);
        assert_eq!(store.snapshot().len(), 1);
        assert_eq!(json!(store.snapshot()[0].id), id);
        if lost_response {
            assert_eq!(retry.status(), reqwest::StatusCode::CONFLICT);
            let rejected: serde_json::Value = retry.json().await.unwrap();
            assert!(rejected.get("recovery_session").is_none());
            assert_eq!(
                serde_json::to_value(store.snapshot()).unwrap(),
                canonical_before
            );
            let retained = recovery;
            assert_eq!(
                load_compose_session(Path::new(retained)).await.unwrap()["bodyMarkdown"],
                "latest retry snapshot"
            );
            let reopened =
                restore_session_path(&client, addr, &serde_json::from_value(id.clone()).unwrap())
                    .await;
            assert_eq!(
                load_compose_session(Path::new(&reopened)).await.unwrap()["bodyMarkdown"],
                "first snapshot"
            );
            remove_compose_file(Path::new(&reopened)).await.unwrap();
            remove_compose_file(Path::new(retained)).await.unwrap();
            remove_compose_attachment_dir(Path::new(retained))
                .await
                .unwrap();
        } else {
            assert_eq!(retry.status(), reqwest::StatusCode::OK);
            let accepted: serde_json::Value = retry.json().await.unwrap();
            let destination = accepted["session"]["draftPath"].as_str().unwrap();
            assert_ne!(destination, recovery);
            assert_eq!(accepted["session"]["draftId"], id);
            assert_eq!(accepted["session"]["revision"], 1);
            assert_eq!(accepted["session"]["preservedRecoveryPath"], recovery);
            assert!(compose_recovery_source(Path::new(destination))
                .await
                .unwrap()
                .is_none());
            let canonical_attachment = store.snapshot()[0].attachments[0].clone();
            assert_eq!(
                accepted["session"]["frontmatter"]["attach"][0],
                canonical_attachment.display().to_string()
            );
            assert_ne!(
                canonical_attachment.display().to_string(),
                attachments[0].as_str().unwrap()
            );
            remove_compose_file(Path::new(recovery)).await.unwrap();
            remove_compose_attachment_dir(Path::new(recovery))
                .await
                .unwrap();
            assert_eq!(
                tokio::fs::read(&canonical_attachment).await.unwrap(),
                b"retry bytes"
            );
            remove_compose_file(Path::new(destination)).await.unwrap();
            remove_compose_attachment_dir(Path::new(destination))
                .await
                .unwrap();
        }
        remove_compose_file(Path::new(recovery)).await.unwrap();
        remove_compose_attachment_dir(Path::new(recovery))
            .await
            .unwrap();
        remove_compose_file(Path::new(&source)).await.unwrap();
        remove_compose_attachment_dir(Path::new(&source))
            .await
            .unwrap();
    }
}
