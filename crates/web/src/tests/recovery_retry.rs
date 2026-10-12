use super::draft_attachment_bytes::DraftAttachmentBytes;
use super::*;
use std::sync::{Arc, Mutex};

const RETRY_BYTES: &[u8] = b"independent retry bytes";
const RETRY_SHA256: &str = "16bd2c90f074a832a75aea55417341ff7de61fbdcbc7047f197c891a448d6dbe";

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
        let resources = DraftAttachmentBytes::new(account.clone(), RETRY_BYTES, RETRY_SHA256);
        let uploaded = resources.clone();
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
                if let Some(response) = uploaded.respond(&request) {
                    return Some(response);
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
        let content = read_compose_file(Path::new(&source)).await.unwrap();
        replace_compose_file(Path::new(&source), format!("{content}\n# --- context (stripped before sending) ---\n# Preserved recovery context\n")).await.unwrap();
        let dir = compose_attachment_dir(Path::new(&source)).unwrap();
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let upload = dir.join("retry.txt");
        tokio::fs::write(&upload, RETRY_BYTES).await.unwrap();
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
        let resource_count_before = resources.len();
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
            RETRY_BYTES
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
            assert_eq!(resources.len(), resource_count_before);
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
            let canonical = store.snapshot()[0].clone();
            assert!(canonical.attachments.is_empty());
            let managed = canonical
                .managed_attachments
                .as_ref()
                .expect("managed attachment set");
            assert_eq!(managed.len(), 1);
            let resource = &managed[0];
            assert_eq!(resource.draft_id, canonical.id);
            assert_eq!(resource.account_id, canonical.account_id);
            assert_eq!(resource.length_bytes, RETRY_BYTES.len() as u64);
            assert_eq!(resource.sha256, RETRY_SHA256);
            let canonical_session = load_compose_session(Path::new(destination)).await.unwrap();
            assert_eq!(
                canonical_session["frontmatter"]["attach"],
                json!([format!(
                    "mxr-resource:{}/{}",
                    resource.id, resource.filename
                )])
            );
            assert_eq!(canonical_session["bodyMarkdown"], "latest private text");
            assert_eq!(canonical_session["frontmatter"]["subject"], "Retry");
            assert_eq!(canonical_session["frontmatter"]["from"], "me@example.com");
            assert_eq!(
                extract_compose_context(canonical_session["rawContent"].as_str().unwrap())
                    .as_deref(),
                Some("Preserved recovery context")
            );
            resources.assert_owned(&canonical);
            let discard = client
                .post(format!("http://{addr}/api/v1/mail/compose/session/discard"))
                .bearer_auth(TEST_AUTH_TOKEN)
                .json(&json!({"draft_path": recovery, "expected_revision": 1}))
                .send()
                .await
                .unwrap();
            assert_eq!(discard.status(), reqwest::StatusCode::OK);
            assert!(!Path::new(retained_attachment).exists());
            resources.assert_owned(&canonical);
            assert_eq!(store.snapshot()[0].revision, Some(2));
            let second = temp.path().join("second.txt");
            tokio::fs::write(&second, RETRY_BYTES).await.unwrap();
            let updated = client.post(format!("http://{addr}/api/v1/mail/compose/session/update"))
                .bearer_auth(TEST_AUTH_TOKEN).json(&json!({"draft_path": destination, "account_id": account,
                    "expected_revision": 2, "to": "alice@example.com", "cc": "", "bcc": "", "subject": "Retry", "from": "me@example.com",
                    "body": "normal updated text", "attach": [canonical_session["frontmatter"]["attach"][0], second]})).send().await.unwrap();
            assert_eq!(updated.status(), reqwest::StatusCode::OK);
            let updated: serde_json::Value = updated.json().await.unwrap();
            let normal = store.snapshot()[0].clone();
            assert_eq!(normal.revision, Some(3));
            assert!(normal.attachments.is_empty());
            assert_eq!(normal.managed_attachments.as_ref().unwrap().len(), 2);
            let tokens =
                mxr_compose::draft_codec::frontmatter_from_draft(&normal, "me@example.com").attach;
            assert_eq!(updated["session"]["frontmatter"]["attach"], json!(tokens));
            assert_eq!(
                extract_compose_context(updated["session"]["rawContent"].as_str().unwrap())
                    .as_deref(),
                Some("Preserved recovery context")
            );
            resources.assert_owned(&normal);
            tokio::fs::remove_file(second).await.unwrap();
            let uploaded_count = resources.len();
            let edited = client.post(format!("http://{addr}/api/v1/mail/compose/session/update"))
                .bearer_auth(TEST_AUTH_TOKEN).json(&json!({"draft_path": destination, "account_id": account,
                    "expected_revision": 3, "to": "alice@example.com", "cc": "", "bcc": "", "subject": "Retry", "from": "me@example.com",
                    "body": "edited with canonical tokens", "attach": tokens})).send().await.unwrap();
            assert_eq!(edited.status(), reqwest::StatusCode::OK);
            assert_eq!(resources.len(), uploaded_count);
            let edited = store.snapshot()[0].clone();
            assert_eq!(edited.revision, Some(4));
            assert_eq!(edited.managed_attachments, normal.managed_attachments);
            resources.assert_owned(&edited);
            assert_eq!(*effects.lock().unwrap(), 0);
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
