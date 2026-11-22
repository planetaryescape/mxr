use super::*;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use mxr_core::{Draft, DraftAttachment, DraftAttachmentId, SendIdentity};
use sha2::{Digest, Sha256};

fn metadata(draft: &Draft, bytes: &[u8]) -> DraftAttachment {
    DraftAttachment {
        id: DraftAttachmentId::new(),
        draft_id: draft.id.clone(),
        account_id: draft.account_id.clone(),
        filename: "portable.bin".into(),
        mime_type: "application/octet-stream".into(),
        content_id: None,
        content_location: None,
        length_bytes: bytes.len() as u64,
        sha256: Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    }
}
async fn call(state: &Arc<AppState>, request: Request) -> Response {
    match handle_request(
        state,
        &IpcMessage {
            expected_workspace: None,
            id: 1,
            source: ClientKind::Cli,
            payload: IpcPayload::Request(request),
        },
    )
    .await
    .payload
    {
        IpcPayload::Response(response) => response,
        other => panic!("unexpected {other:?}"),
    }
}
fn ok(response: Response) -> ResponseData {
    match response {
        Response::Ok { data } => data,
        other => panic!("expected success {other:?}"),
    }
}
fn error(response: Response, expected: &str) {
    match response {
        Response::Error { message, .. } => assert!(message.contains(expected), "{message}"),
        other => panic!("expected {expected}: {other:?}"),
    }
}
fn saved(response: Response) -> Draft {
    match ok(response) {
        ResponseData::Draft { draft } => draft,
        other => panic!("expected draft {other:?}"),
    }
}
async fn fixture() -> (Arc<AppState>, Arc<mxr_provider_fake::FakeProvider>, Draft) {
    let (state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let now = chrono::Utc::now();
    let draft = Draft {
        revision: None,
        id: mxr_core::DraftId::new(),
        account_id: state.default_account_id(),
        from: None,
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![mxr_core::Address {
            name: None,
            email: "recipient@example.com".into(),
        }],
        cc: vec![],
        bcc: vec![],
        subject: "Immutable bytes".into(),
        content: mxr_core::DraftContent::markdown("synthetic attachment"),
        attachments: vec![],
        managed_attachments: Some(vec![]),
        inline_assets: vec![],
        inline_calendar_reply: None,
        created_at: now,
        updated_at: now,
    };
    let state = Arc::new(state);
    let draft = saved(call(&state, Request::SaveDraft { draft }).await);
    (state, fake, draft)
}
fn chunk(metadata: &DraftAttachment, offset: u64, bytes: &[u8]) -> Request {
    Request::WriteDraftAttachmentChunk {
        draft_id: metadata.draft_id.clone(),
        account_id: metadata.account_id.clone(),
        resource_id: metadata.id.clone(),
        offset,
        data_base64: STANDARD.encode(bytes),
    }
}
fn finalize(metadata: &DraftAttachment) -> Request {
    Request::FinalizeDraftAttachment {
        draft_id: metadata.draft_id.clone(),
        account_id: metadata.account_id.clone(),
        resource_id: metadata.id.clone(),
    }
}
async fn upload(state: &Arc<AppState>, draft: &Draft, bytes: &[u8]) -> DraftAttachment {
    let metadata = metadata(draft, bytes);
    ok(call(
        state,
        Request::BeginDraftAttachment {
            metadata: metadata.clone(),
        },
    )
    .await);
    for (index, bytes) in bytes
        .chunks(mxr_protocol::MAX_ATTACHMENT_CHUNK_BYTES as usize)
        .enumerate()
    {
        ok(call(
            state,
            chunk(
                &metadata,
                index as u64 * u64::from(mxr_protocol::MAX_ATTACHMENT_CHUNK_BYTES),
                bytes,
            ),
        )
        .await);
    }
    ok(call(state, finalize(&metadata)).await);
    metadata
}
fn send(draft: &Draft) -> Request {
    Request::SendStoredDraft {
        managed_attachments_reviewed: true,
        draft_id: draft.id.clone(),
        send_identity: Some(SendIdentity::for_draft(draft)),
        override_safety_token: None,
    }
}

#[tokio::test]
async fn draft_attachment_resources_chunks_retries_cas_and_provider_bytes() {
    let (state, fake, mut draft) = fixture().await;
    let bytes = vec![37; mxr_protocol::MAX_ATTACHMENT_CHUNK_BYTES as usize + 29];
    let resource = upload(&state, &draft, &bytes).await;
    ok(call(&state, chunk(&resource, 0, &bytes[..256 * 1024])).await);
    ok(call(&state, finalize(&resource)).await);
    error(
        call(&state, chunk(&resource, 0, b"changed")).await,
        "conflicts",
    );
    draft.managed_attachments = Some(vec![resource.clone()]);
    let before = draft.clone();
    draft = saved(call(&state, Request::UpdateDraft { draft }).await);
    error(
        call(&state, Request::UpdateDraft { draft: before }).await,
        "revision",
    );
    let mut old_wire = serde_json::to_value(&draft).unwrap();
    old_wire
        .as_object_mut()
        .unwrap()
        .remove("managed_attachments");
    let old: Draft = serde_json::from_value(old_wire).unwrap();
    error(
        call(&state, Request::UpdateDraft { draft: old.clone() }).await,
        "upgrade",
    );
    error(
        call(
            &state,
            Request::CheckDraftSafety {
                draft: old,
                context: Default::default(),
            },
        )
        .await,
        "upgrade",
    );
    let mut old_send = serde_json::to_value(send(&draft)).unwrap();
    old_send
        .as_object_mut()
        .unwrap()
        .remove("managed_attachments_reviewed");
    error(
        call(&state, serde_json::from_value(old_send).unwrap()).await,
        "upgrade",
    );
    assert!(fake.sent_drafts().is_empty());
    ok(call(&state, send(&draft)).await);
    assert_eq!(
        fake.attachment_witnesses()[0][0]["length_bytes"],
        bytes.len()
    );
    assert_eq!(fake.attachment_witnesses()[0][0]["sha256"], resource.sha256);
    assert!(state
        .store
        .get_sent_draft_receipt(&draft.id)
        .await
        .unwrap()
        .is_some());
    state
        .store
        .delete_draft_attachment(&resource.id)
        .await
        .unwrap();
    std::fs::remove_dir_all(
        state
            .store
            .draft_attachment_dir()
            .join(resource.id.as_str()),
    )
    .unwrap();
    assert!(matches!(
        ok(call(
            &state,
            Request::SendDraft {
                draft: draft.clone(),
                send_identity: Some(SendIdentity::for_draft(&draft)),
                override_safety_token: None
            }
        )
        .await),
        ResponseData::SendReceipt { .. }
    ));
    assert_eq!(fake.sent_drafts().len(), 1);
}

#[tokio::test]
async fn draft_attachment_resources_restart_sends_original_client_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("mxr.db");
    let client_file = temp.path().join("client.bin");
    let bytes = b"original client bytes after restart";
    std::fs::write(&client_file, bytes).unwrap();
    let (mut state, fake) = AppState::in_memory_with_fake().await.unwrap();
    let account = state
        .store
        .get_account(&state.default_account_id())
        .await
        .unwrap()
        .unwrap();
    state.store = Arc::new(mxr_store::Store::new(&db).await.unwrap());
    state.store.insert_account(&account).await.unwrap();
    let (_, _, mut draft) = fixture().await;
    draft.id = mxr_core::DraftId::new();
    draft.account_id = account.id.clone();
    draft.revision = None;
    let state = Arc::new(state);
    draft = saved(call(&state, Request::SaveDraft { draft }).await);
    let resource = upload(&state, &draft, &std::fs::read(&client_file).unwrap()).await;
    draft.managed_attachments = Some(vec![resource.clone()]);
    draft = saved(call(&state, Request::UpdateDraft { draft }).await);
    std::fs::write(&client_file, b"changed client bytes").unwrap();
    std::fs::remove_file(&client_file).unwrap();
    drop(state);
    let mut restarted =
        AppState::in_memory_with_sync_provider(account, fake.clone(), Some(fake.clone()))
            .await
            .unwrap();
    restarted.store = Arc::new(mxr_store::Store::new(&db).await.unwrap());
    let restarted = Arc::new(restarted);
    draft = saved(
        call(
            &restarted,
            Request::GetDraft {
                draft_id: draft.id.clone(),
            },
        )
        .await,
    );
    ok(call(
        &restarted,
        Request::CheckDraftSafety {
            draft: draft.clone(),
            context: Default::default(),
        },
    )
    .await);
    ok(call(&restarted, send(&draft)).await);
    assert_eq!(fake.attachment_witnesses()[0][0]["sha256"], resource.sha256);
    assert_eq!(
        fake.attachment_witnesses()[0][0]["length_bytes"],
        bytes.len()
    );
    assert!(restarted
        .store
        .get_sent_draft_receipt(&draft.id)
        .await
        .unwrap()
        .is_some());
}

mod compatibility;
mod lifecycle;
