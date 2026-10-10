use super::*;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn explicit_saved_recovery_retry_uses_observed_cas_and_owns_its_attachment_payload() {
    for intervening_writer in [false, true] {
        let temp = TempDir::new().unwrap();
        let socket = temp.path().join("mxr.sock");
        let account = AccountId::new();
        let id = DraftId::new();
        let store = FakeDraftStore::with(vec![markdown_draft(
            &id,
            &account,
            "Retry",
            "canonical text",
            Utc::now(),
        )]);
        let updates = Arc::new(Mutex::new(0));
        let seen = updates.clone();
        let effects = Arc::new(Mutex::new(0));
        let seen_effects = effects.clone();
        let responder = draft_store_responder(store.clone(), sample_account(&account));
        let _ipc = spawn_fake_ipc_server(
            &socket,
            move |request| {
                if let Request::UpdateDraft { .. } = &request {
                    let mut attempts = seen.lock().unwrap();
                    *attempts += 1;
                    if *attempts == 1 {
                        return Some(Response::error_kinded(
                            "temporary daemon failure",
                            mxr_protocol::IpcErrorKind::Internal,
                        ));
                    }
                }
                if matches!(
                    &request,
                    Request::SendDraft { .. }
                        | Request::SaveDraftToServer { .. }
                        | Request::ScheduleSend { .. }
                ) {
                    *seen_effects.lock().unwrap() += 1;
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
        let source = restore_session_path(&client, addr, &id).await;
        let dir = compose_attachment_dir(Path::new(&source)).unwrap();
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let upload = dir.join("retry.txt");
        tokio::fs::write(&upload, b"independent retry bytes")
            .await
            .unwrap();
        let first = client.post(format!("http://{addr}/api/v1/mail/compose/session/update"))
            .bearer_auth(TEST_AUTH_TOKEN).json(&json!({"draft_path": source, "account_id": account,
                "expected_revision": 1, "to": "alice@example.com", "cc": "", "bcc": "", "subject": "Retry", "from": "me@example.com",
                "body": "first private text", "attach": [upload]})).send().await.unwrap();
        assert!(!first.status().is_success());
        let failure: serde_json::Value = first.json().await.unwrap();
        let recovery = failure["recovery_session"]["draft_path"].as_str().unwrap();
        assert_eq!(failure["recovery_session"]["revision"], 1);
        let attachments = failure["recovery_session"]["attachment_paths"].clone();
        if intervening_writer {
            let mut winner = store.snapshot()[0].clone();
            winner.content = DraftContent::markdown("intervening writer text");
            store.update(winner).unwrap();
        }
        let canonical_before = serde_json::to_value(store.snapshot()).unwrap();
        let retry = client.post(format!("http://{addr}/api/v1/mail/compose/session/update"))
            .bearer_auth(TEST_AUTH_TOKEN).json(&json!({"draft_path": recovery, "account_id": account,
                "retry_recovery": true, "expected_revision": 1,
                "to": "alice@example.com", "cc": "", "bcc": "", "subject": "Retry", "from": "me@example.com",
                "body": "latest private text", "attach": attachments})).send().await.unwrap();
        let retained = load_compose_session(Path::new(recovery)).await.unwrap();
        assert_eq!(retained["bodyMarkdown"], "latest private text");
        assert_eq!(retained["revision"], 1);
        let retained_attachment = retained["frontmatter"]["attach"][0].as_str().unwrap();
        assert_eq!(
            tokio::fs::read(retained_attachment).await.unwrap(),
            b"independent retry bytes"
        );
        assert_eq!(*effects.lock().unwrap(), 0);
        assert_eq!(store.snapshot().len(), 1);
        if intervening_writer {
            assert_eq!(retry.status(), reqwest::StatusCode::CONFLICT);
            let rejected: serde_json::Value = retry.json().await.unwrap();
            assert_eq!(rejected["details"]["expected_revision"], 1);
            assert_eq!(rejected["details"]["current_revision"], 2);
            assert!(rejected.get("recovery_session").is_none());
            assert_eq!(
                serde_json::to_value(store.snapshot()).unwrap(),
                canonical_before
            );
            assert_eq!(*updates.lock().unwrap(), 1);
        } else {
            assert_eq!(retry.status(), reqwest::StatusCode::OK);
            let accepted: serde_json::Value = retry.json().await.unwrap();
            assert_eq!(accepted["session"]["draftId"], json!(id));
            assert_eq!(accepted["session"]["revision"], 2);
            assert_eq!(accepted["session"]["preservedRecoveryPath"], recovery);
            assert_eq!(*updates.lock().unwrap(), 2);
            let destination = accepted["session"]["draftPath"].as_str().unwrap();
            assert_ne!(destination, recovery);
            assert_eq!(
                store.snapshot()[0].content.analysis_text(),
                "latest private text"
            );
            let canonical_attachment = store.snapshot()[0].attachments[0].clone();
            assert_ne!(canonical_attachment, PathBuf::from(retained_attachment));
            let discard = client
                .post(format!("http://{addr}/api/v1/mail/compose/session/discard"))
                .bearer_auth(TEST_AUTH_TOKEN)
                .json(&json!({"draft_path": recovery, "expected_revision": 1}))
                .send()
                .await
                .unwrap();
            assert_eq!(discard.status(), reqwest::StatusCode::OK);
            assert_eq!(
                tokio::fs::read(&canonical_attachment).await.unwrap(),
                b"independent retry bytes"
            );
            assert_eq!(store.snapshot()[0].revision, Some(2));
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

#[tokio::test]
async fn an_unsaved_view_cannot_use_another_tabs_first_saved_snapshot_for_effects() {
    let temp = TempDir::new().unwrap();
    let socket = temp.path().join("mxr.sock");
    let account = AccountId::new();
    let store = FakeDraftStore::default();
    let effects = Arc::new(Mutex::new(0));
    let seen = effects.clone();
    let responder = draft_store_responder(store.clone(), sample_account(&account));
    let _ipc = spawn_fake_ipc_server(
        &socket,
        move |request| {
            if matches!(
                &request,
                Request::SendDraft { .. }
                    | Request::CheckDraftSafety { .. }
                    | Request::ScheduleSend { .. }
                    | Request::SaveDraftToServer { .. }
                    | Request::SaveDraft { .. }
                    | Request::UpdateDraft { .. }
            ) {
                *seen.lock().unwrap() += 1;
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
    let started: serde_json::Value = client
        .post(format!("http://{addr}/api/v1/mail/compose/session"))
        .bearer_auth(TEST_AUTH_TOKEN)
        .json(&json!({"kind": "new"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let path = compose_draft_path(&started);
    let second_view = load_compose_session(Path::new(&path)).await.unwrap();
    assert!(started["session"]["revision"].is_null());
    assert!(second_view["revision"].is_null());
    let unsaved = read_draft_session_metadata(Path::new(&path))
        .await
        .unwrap()
        .unwrap();
    assert!(ensure_compose_revision(&unsaved, None).is_ok());
    let saved: serde_json::Value = client.post(format!("http://{addr}/api/v1/mail/compose/session/update"))
        .bearer_auth(TEST_AUTH_TOKEN).json(&json!({"draft_path": path, "account_id": account,
            "expected_revision": null, "to": "alice@example.com", "cc": "", "bcc": "", "subject": "First save",
            "from": "me@example.com", "body": "winner reviewed text"})).send().await.unwrap().json().await.unwrap();
    assert_eq!(saved["session"]["revision"], 1);
    let canonical = serde_json::to_value(store.snapshot()).unwrap();
    let file = read_compose_file(Path::new(&path)).await.unwrap();
    *effects.lock().unwrap() = 0;
    for route in ["send", "safety-check", "schedule", "save"] {
        let rejected = client
            .post(format!("http://{addr}/api/v1/mail/compose/session/{route}"))
            .bearer_auth(TEST_AUTH_TOKEN)
            .json(&json!({"draft_path": path, "account_id": account,
                "expected_revision": second_view["revision"], "save_to_server": true,
                "send_at": "2027-01-01T09:00:00Z"}))
            .send()
            .await
            .unwrap();
        assert_eq!(rejected.status(), reqwest::StatusCode::CONFLICT, "{route}");
        let error: serde_json::Value = rejected.json().await.unwrap();
        assert_eq!(error["code"], "draft_revision_conflict");
        assert!(error["details"]["expected_revision"].is_null());
        assert_eq!(
            error["details"]["current_revision"],
            saved["session"]["revision"]
        );
    }
    assert_eq!(*effects.lock().unwrap(), 0);
    assert_eq!(serde_json::to_value(store.snapshot()).unwrap(), canonical);
    assert_eq!(read_compose_file(Path::new(&path)).await.unwrap(), file);
    remove_compose_file(Path::new(&path)).await.unwrap();
}
