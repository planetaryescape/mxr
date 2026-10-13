use super::*;
use futures::{SinkExt, StreamExt};
use tokio_util::codec::Framed;

#[tokio::test]
async fn legacy_tcp_keeps_real_daemon_save_send_archive_download_and_safety_contracts() {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let state = Arc::new(state);
    let directory = tempfile::tempdir().unwrap();
    let attachment = directory.path().join("host-attachment.txt");
    let bytes = b"synthetic legacy download";
    std::fs::write(&attachment, bytes).unwrap();
    let envelope = crate::test_fixtures::TestEnvelopeBuilder::new()
        .account_id(state.default_account_id())
        .build();
    state.store.upsert_envelope(&envelope).await.unwrap();
    let mut body = crate::test_fixtures::make_empty_body(&envelope.id);
    let attachment_id = mxr_core::AttachmentId::new();
    body.attachments.push(mxr_core::AttachmentMeta {
        id: attachment_id.clone(),
        message_id: envelope.id.clone(),
        filename: "host-attachment.txt".into(),
        mime_type: "text/plain".into(),
        disposition: mxr_core::AttachmentDisposition::Attachment,
        content_id: None,
        content_location: None,
        size_bytes: bytes.len() as u64,
        local_path: Some(attachment),
        provider_id: "fixture".into(),
    });
    state.store.insert_body(&body).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target = mxr_client::SelectedDaemonTarget::new(
        mxr_client::TransportAddr::Tcp(listener.local_addr().unwrap()),
        None,
    );
    let server_state = state.clone();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut framed = Framed::new(stream, IpcCodec::new());
        while let Some(Ok(message)) = framed.next().await {
            let response = handle_request(&server_state, &message).await;
            framed.send(response).await.unwrap();
        }
    });
    assert!(!target.is_paired_remote());
    let mut connection = target.connect(ClientKind::Cli).await.unwrap();
    assert!(!connection.is_paired_remote());
    let refused = connection
        .request_response(Request::GetRemoteReady, |_| {}, None)
        .await
        .unwrap();
    assert!(matches!(refused, Response::Error { .. }));
    let owner = connection.capture_workspace().await.unwrap();
    let now = chrono::Utc::now();
    let outgoing = directory.path().join("client-attachment.txt");
    std::fs::write(&outgoing, b"synthetic legacy upload").unwrap();
    let draft = mxr_core::Draft {
        id: mxr_core::DraftId::new(),
        account_id: state.default_account_id(),
        revision: None,
        from: None,
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::Address {
            name: None,
            email: "recipient@example.com".into(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Legacy compose".into(),
        content: mxr_core::DraftContent::markdown("Synthetic test body"),
        attachments: vec![outgoing],
        managed_attachments: None,
        inline_assets: vec![],
        inline_calendar_reply: None,
        created_at: now,
        updated_at: now,
    };
    let safety = connection
        .request_owned(
            Request::CheckDraftSafety {
                draft: draft.clone(),
                context: Default::default(),
            },
            &owner,
        )
        .await
        .unwrap();
    assert!(matches!(
        safety,
        ResponseData::DraftSafetyReportResponse { .. }
    ));
    let audits: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM draft_safety_runs")
        .fetch_one(state.store.reader())
        .await
        .unwrap();
    assert_eq!(audits, 1, "legacy safety retains its daemon audit behavior");
    let saved = connection
        .request_owned(Request::SaveDraft { draft }, &owner)
        .await
        .unwrap();
    let ResponseData::Draft { draft } = saved else {
        panic!("saved draft")
    };
    assert_eq!(draft.managed_attachments.as_ref().unwrap().len(), 1);
    assert_eq!(
        draft.managed_attachments.as_ref().unwrap()[0].filename,
        "client-attachment.txt"
    );
    let identity = mxr_core::SendIdentity::for_draft(&draft);
    let sent = connection
        .request_owned(
            Request::SendStoredDraft {
                draft_id: draft.id.clone(),
                send_identity: Some(identity.clone()),
                override_safety_token: None,
                managed_attachments_reviewed: true,
            },
            &owner,
        )
        .await
        .unwrap();
    assert!(matches!(sent, ResponseData::SendReceipt { .. }));
    let outcome = connection
        .request_owned(
            Request::GetSendOutcome {
                account_id: draft.account_id,
                operation_id: identity.operation_id,
            },
            &owner,
        )
        .await
        .unwrap();
    assert!(
        matches!(outcome, ResponseData::SendOutcome { outcome:Some(value) } if value.state == mxr_core::SendOutcomeState::Accepted)
    );
    assert_eq!(fake.sent_drafts().len(), 1);
    let preview = connection
        .request(Request::PreviewArchive {
            identity: ArchiveIdentityData {
                operation_id: uuid::Uuid::now_v7().to_string(),
                caller_id: uuid::Uuid::now_v7().to_string(),
                workspace: owner,
            },
            mutation: MutationCommand::Archive {
                message_ids: vec![envelope.id.clone()],
            },
        })
        .await
        .unwrap();
    assert!(matches!(preview, ResponseData::ArchiveOperation { .. }));
    let download = directory.path().join("download");
    let downloaded = mxr_client::download_attachment_to(
        &mut connection,
        &envelope.id,
        &attachment_id,
        &download,
        false,
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read(downloaded.path).unwrap(), bytes);
    drop(connection);
    tokio::time::timeout(std::time::Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap();
}
