use super::*;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn conflict_and_reopened_recoveries_own_uploaded_attachment_bytes() {
    let temp = TempDir::new().unwrap();
    let socket = temp.path().join("mxr.sock");
    let account = AccountId::new();
    let id = DraftId::new();
    let original = markdown_draft(&id, &account, "Attachments", "original", Utc::now());
    let store = FakeDraftStore::with(vec![original.clone()]);
    let effects = Arc::new(Mutex::new(0));
    let seen = effects.clone();
    let responder = draft_store_responder(store.clone(), sample_account(&account));
    let _ipc = spawn_fake_ipc_server(
        &socket,
        move |request| {
            if matches!(
                &request,
                Request::SendDraft { .. }
                    | Request::SaveDraftToServer { .. }
                    | Request::DeleteDraft { .. }
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
    let source = restore_session_path(&client, addr, &id).await;
    let other_source = temp.path().join(format!("other-{}.md", Uuid::now_v7()));
    let source_dir = compose_attachment_dir(Path::new(&source)).unwrap();
    let other_dir = compose_attachment_dir(&other_source).unwrap();
    tokio::fs::create_dir_all(&source_dir).await.unwrap();
    tokio::fs::create_dir_all(&other_dir).await.unwrap();
    let first = source_dir.join("notes.txt");
    let second = other_dir.join("notes.txt");
    mxr_compose::private_tmp::write_private_async(&first, b"first bytes")
        .await
        .unwrap();
    mxr_compose::private_tmp::write_private_async(&second, b"second bytes")
        .await
        .unwrap();
    let external = temp.path().join("user-managed.txt");
    tokio::fs::write(&external, b"external bytes")
        .await
        .unwrap();
    let mut winner = original;
    winner.content = DraftContent::markdown("winning editor");
    store.update(winner).unwrap();
    let canonical = store.snapshot();

    let conflict = client.post(format!("http://{addr}/api/v1/mail/compose/session/update"))
        .bearer_auth(TEST_AUTH_TOKEN)
        .json(&json!({"draft_path": source, "account_id": account, "expected_revision": 1,
            "to": "alice@example.com", "cc": "", "bcc": "", "subject": "Attachments", "from": "me@example.com",
            "body": "losing editor", "attach": [first, second, external]}))
        .send().await.unwrap();
    assert_eq!(conflict.status(), reqwest::StatusCode::CONFLICT);
    let failure: serde_json::Value = conflict.json().await.unwrap();
    let recovery = failure["recovery_session"]["draft_path"].as_str().unwrap();
    let mut owned: Vec<String> =
        serde_json::from_value(failure["recovery_session"]["attachment_paths"].clone()).unwrap();
    assert_ne!(owned[0], first.display().to_string());
    assert_ne!(owned[1], second.display().to_string());
    assert_ne!(
        owned[0], owned[1],
        "equal basenames must keep distinct bytes"
    );
    assert_eq!(owned[2], external.display().to_string());
    remove_compose_attachment_dir(Path::new(&source))
        .await
        .unwrap();
    remove_compose_attachment_dir(&other_source).await.unwrap();
    assert_eq!(tokio::fs::read(&owned[0]).await.unwrap(), b"first bytes");
    assert_eq!(tokio::fs::read(&owned[1]).await.unwrap(), b"second bytes");
    let recovered = load_compose_session(Path::new(recovery)).await.unwrap();
    assert_eq!(recovered["frontmatter"]["attach"], json!(owned));
    assert_eq!(
        read_draft_session_metadata(Path::new(recovery))
            .await
            .unwrap()
            .unwrap()
            .attachments,
        owned.iter().map(PathBuf::from).collect::<Vec<_>>()
    );

    // Recovery continues to preserve legacy path-backed drafts.
    let legacy = compose_attachment_dir(Path::new(recovery))
        .unwrap()
        .join("uploaded-later.txt");
    tokio::fs::create_dir_all(legacy.parent().unwrap())
        .await
        .unwrap();
    mxr_compose::private_tmp::write_private_async(&legacy, b"uploaded later")
        .await
        .unwrap();
    owned.push(legacy.display().to_string());
    let updated = client.post(format!("http://{addr}/api/v1/mail/compose/session/update"))
        .bearer_auth(TEST_AUTH_TOKEN)
        .json(&json!({"draft_path": recovery, "account_id": account, "expected_revision": 1,
            "to": "alice@example.com", "cc": "", "bcc": "", "subject": "Attachments", "from": "me@example.com",
            "body": "latest preserved body", "attach": owned}))
        .send().await.unwrap();
    assert_eq!(updated.status(), reqwest::StatusCode::OK);
    let mut copies = Vec::new();
    for _ in 0..2 {
        let response: serde_json::Value = client
            .post(format!("http://{addr}/api/v1/mail/compose/session/refresh"))
            .bearer_auth(TEST_AUTH_TOKEN)
            .json(&json!({"draft_path": recovery}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        copies.push(response["session"].clone());
    }
    let discarded = client
        .post(format!("http://{addr}/api/v1/mail/compose/session/discard"))
        .bearer_auth(TEST_AUTH_TOKEN)
        .json(&json!({"draft_path": recovery, "expected_revision": 1}))
        .send()
        .await
        .unwrap();
    assert_eq!(discarded.status(), reqwest::StatusCode::OK);
    assert!(!Path::new(&owned[0]).exists());
    for copy in &copies {
        let path = copy["draftPath"].as_str().unwrap();
        let attachments: Vec<String> =
            serde_json::from_value(copy["frontmatter"]["attach"].clone()).unwrap();
        assert_eq!(copy["bodyMarkdown"], "latest preserved body");
        assert_eq!(copy["revision"], 1);
        for (index, bytes) in [
            (0, b"first bytes".as_slice()),
            (1, b"second bytes".as_slice()),
            (3, b"uploaded later".as_slice()),
        ] {
            assert_ne!(attachments[index], owned[index]);
            assert_eq!(tokio::fs::read(&attachments[index]).await.unwrap(), bytes);
        }
        assert_eq!(attachments[2], external.display().to_string());
        assert_eq!(
            read_draft_session_metadata(Path::new(path))
                .await
                .unwrap()
                .unwrap()
                .attachments,
            attachments.iter().map(PathBuf::from).collect::<Vec<_>>()
        );
    }
    let discarded_copy = client
        .post(format!("http://{addr}/api/v1/mail/compose/session/discard"))
        .bearer_auth(TEST_AUTH_TOKEN)
        .json(&json!({"draft_path": copies[0]["draftPath"], "expected_revision": 1}))
        .send()
        .await
        .unwrap();
    assert_eq!(discarded_copy.status(), reqwest::StatusCode::OK);
    let survivor = copies[1]["draftPath"].as_str().unwrap();
    assert_eq!(
        tokio::fs::read(copies[1]["frontmatter"]["attach"][3].as_str().unwrap())
            .await
            .unwrap(),
        b"uploaded later"
    );
    assert_eq!(
        serde_json::to_value(store.snapshot()).unwrap(),
        serde_json::to_value(canonical).unwrap()
    );
    assert_eq!(*effects.lock().unwrap(), 0);
    assert_eq!(tokio::fs::read(&external).await.unwrap(), b"external bytes");
    remove_compose_file(Path::new(survivor)).await.unwrap();
    remove_compose_attachment_dir(Path::new(survivor))
        .await
        .unwrap();
    remove_compose_file(Path::new(&source)).await.unwrap();
}

#[tokio::test]
async fn failed_attachment_preservation_removes_partial_copies_without_touching_source() {
    let temp = TempDir::new().unwrap();
    let source = temp.path().join(format!("source-{}.md", Uuid::now_v7()));
    let destination = temp.path().join(format!("recovery-{}.md", Uuid::now_v7()));
    let dir = compose_attachment_dir(&source).unwrap();
    tokio::fs::create_dir_all(&dir).await.unwrap();
    let existing = dir.join("exists.txt");
    tokio::fs::write(&existing, b"retained source")
        .await
        .unwrap();
    let mut draft = markdown_draft(
        &DraftId::new(),
        &AccountId::new(),
        "Failure",
        "text",
        Utc::now(),
    );
    draft.attachments = vec![existing.clone(), dir.join("missing.txt")];
    let content =
        mxr_compose::draft_codec::draft_to_compose_file(&draft, "me@example.com").unwrap();
    assert!(copy_recovery_attachments(&destination, &content, &draft)
        .await
        .is_err());
    assert_eq!(tokio::fs::read(existing).await.unwrap(), b"retained source");
    let mut entries = tokio::fs::read_dir(compose_attachment_dir(&destination).unwrap())
        .await
        .unwrap();
    assert!(entries.next_entry().await.unwrap().is_none());
    assert!(!destination.exists());
    remove_compose_attachment_dir(&source).await.unwrap();
    remove_compose_attachment_dir(&destination).await.unwrap();
}

#[tokio::test]
async fn attachment_rewrite_keeps_raw_editor_whitespace_and_context() {
    let temp = TempDir::new().unwrap();
    let source = temp
        .path()
        .join(format!("raw-source-{}.md", Uuid::now_v7()));
    let destination = temp
        .path()
        .join(format!("raw-recovery-{}.md", Uuid::now_v7()));
    let dir = compose_attachment_dir(&source).unwrap();
    tokio::fs::create_dir_all(&dir).await.unwrap();
    let attachment = dir.join("raw.txt");
    tokio::fs::write(&attachment, b"raw bytes").await.unwrap();
    let mut draft = markdown_draft(
        &DraftId::new(),
        &AccountId::new(),
        "Raw",
        "text",
        Utc::now(),
    );
    draft.attachments = vec![attachment];
    let original =
        mxr_compose::draft_codec::draft_to_compose_file(&draft, "me@example.com").unwrap();
    let (frontmatter, _) = parse_compose_content(&original).unwrap();
    let raw = render_compose_file(
        &frontmatter,
        "  last typed text \n\n",
        Some("original context"),
    )
    .unwrap();
    let (preserved, metadata, _) = copy_recovery_attachments(&destination, &raw, &draft)
        .await
        .unwrap();
    let suffix = |content: &str| {
        content
            .strip_prefix("---")
            .unwrap()
            .split_once("\n---")
            .unwrap()
            .1
            .to_string()
    };
    assert_eq!(suffix(&preserved), suffix(&raw));
    assert_eq!(metadata.revision, draft.revision);
    assert_eq!(
        tokio::fs::read(&metadata.attachments[0]).await.unwrap(),
        b"raw bytes"
    );
    remove_compose_attachment_dir(&source).await.unwrap();
    remove_compose_attachment_dir(&destination).await.unwrap();
}
