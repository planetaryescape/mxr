#![expect(
    clippy::panic,
    reason = "tests fail directly on unexpected upload protocol variants"
)]

use super::*;
use futures::{SinkExt, StreamExt};
use mxr_protocol::{ClientKind, IpcCodec, IpcMessage, IpcPayload};
use mxr_transport::{MemoryTransport, ServerTransport};
use std::sync::{Arc, Mutex};
use tokio_util::codec::Framed;

fn status(upload: bool) -> ResponseData {
    serde_json::from_value(serde_json::json!({"kind": "Status", "workspace_owner_supported": upload, "draft_revision_supported": true, "send_outcome_supported": true,
        "attachment_transfer_supported": true, "draft_attachment_upload_supported": upload,
        "uptime_secs": 1, "accounts": [], "total_messages": 0})).expect("status fixture")
}
fn draft(path: &std::path::Path) -> Draft {
    let now = chrono::Utc::now();
    Draft {
        revision: None,
        id: mxr_core::DraftId::new(),
        account_id: mxr_core::AccountId::new(),
        from: None,
        reply_headers: None,
        intent: mxr_core::DraftIntent::New,
        to: vec![],
        cc: vec![],
        bcc: vec![],
        subject: "portable fixture".into(),
        content: mxr_core::DraftContent::markdown("synthetic"),
        attachments: vec![path.to_path_buf()],
        managed_attachments: None,
        inline_assets: vec![],
        inline_calendar_reply: None,
        created_at: now,
        updated_at: now,
    }
}

#[tokio::test]
async fn portable_upload_rejects_old_daemon_before_opening_client_file() {
    let transport = MemoryTransport::new();
    let mut listener = transport.bind().await.expect("listener");
    let requests = Arc::new(Mutex::new(Vec::new()));
    let seen = requests.clone();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut framed = Framed::new(stream, IpcCodec::new());
        while let Some(Ok(message)) = framed.next().await {
            let IpcPayload::Request(request) = message.payload else {
                continue;
            };
            seen.lock().expect("lock").push(request.clone());
            assert!(matches!(request, Request::GetStatus));
            let mut old_status = serde_json::to_value(status(false)).expect("status");
            old_status
                .as_object_mut()
                .expect("object")
                .remove("draft_attachment_upload_supported");
            framed
                .send(IpcMessage {
                    expected_workspace: None,
                    id: message.id,
                    source: ClientKind::Daemon,
                    payload: IpcPayload::Response(Response::Ok {
                        data: serde_json::from_value(old_status).expect("old status"),
                    }),
                })
                .await
                .expect("reply");
        }
    });
    let mut connection = IpcConnection::connect_with(&transport.connector(), ClientKind::Cli)
        .await
        .expect("connection");
    let error = connection
        .request_owned(
            Request::SaveDraft {
                draft: draft(std::path::Path::new("/nonexistent/client-only-file")),
            },
            &workspace(),
        )
        .await
        .expect_err("unsupported");
    assert!(error.to_string().contains("upgrade"));
    drop(connection);
    server.await.expect("server");
    assert_eq!(requests.lock().expect("lock").len(), 1);
}

#[tokio::test]
async fn portable_upload_sends_bounded_bytes_and_preserves_selected_send_identity() {
    let directory = tempfile::tempdir().expect("directory");
    let path = directory.path().join("client.bin");
    let bytes = vec![53; MAX_ATTACHMENT_CHUNK_BYTES as usize + 17];
    std::fs::write(&path, &bytes).expect("client file");
    let transport = MemoryTransport::new();
    let mut listener = transport.bind().await.expect("listener");
    let observed = Arc::new(Mutex::new(Vec::new()));
    let seen = observed.clone();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut framed = Framed::new(stream, IpcCodec::new());
        let mut stored: Option<Draft> = None;
        let mut declaration: Option<DraftAttachment> = None;
        while let Some(Ok(message)) = framed.next().await {
            let IpcPayload::Request(request) = message.payload else {
                continue;
            };
            let response = match request {
                Request::GetStatus => Response::Ok { data: status(true) },
                Request::GetArchiveWorkspace => Response::Ok {
                    data: ResponseData::ArchiveWorkspace {
                        workspace: workspace(),
                    },
                },
                Request::GetDraft { .. } => stored.clone().map_or_else(
                    || {
                        Response::error_kinded(
                            "Draft not found",
                            mxr_protocol::IpcErrorKind::NotFound,
                        )
                    },
                    |draft| Response::Ok {
                        data: ResponseData::Draft { draft },
                    },
                ),
                Request::SaveDraft { mut draft } => {
                    assert!(draft.attachments.is_empty());
                    draft.revision = Some(1);
                    stored = Some(draft.clone());
                    Response::Ok {
                        data: ResponseData::Draft { draft },
                    }
                }
                Request::BeginDraftAttachment { metadata } => {
                    declaration = Some(metadata.clone());
                    Response::Ok {
                        data: ResponseData::DraftAttachment { metadata },
                    }
                }
                Request::WriteDraftAttachmentChunk {
                    offset,
                    data_base64,
                    ..
                } => {
                    let chunk = STANDARD.decode(data_base64).expect("chunk");
                    assert!(chunk.len() <= MAX_ATTACHMENT_CHUNK_BYTES as usize);
                    let mut bytes = seen.lock().expect("lock");
                    assert_eq!(offset, bytes.len() as u64);
                    bytes.extend(chunk);
                    Response::Ok {
                        data: ResponseData::Ack,
                    }
                }
                Request::FinalizeDraftAttachment { .. } => Response::Ok {
                    data: ResponseData::DraftAttachment {
                        metadata: declaration.clone().expect("declaration"),
                    },
                },
                Request::UpdateDraft { mut draft } => {
                    assert_eq!(draft.revision, Some(1));
                    assert!(draft.attachments.is_empty());
                    draft.revision = Some(2);
                    stored = Some(draft.clone());
                    Response::Ok {
                        data: ResponseData::Draft { draft },
                    }
                }
                Request::SendDraft {
                    draft,
                    send_identity,
                    ..
                } => {
                    assert_eq!(
                        send_identity.as_ref().expect("identity").expected_revision,
                        2
                    );
                    assert_eq!(
                        draft.managed_attachments.as_ref().expect("resources").len(),
                        1
                    );
                    Response::Ok {
                        data: ResponseData::Ack,
                    }
                }
                other => panic!("unexpected {other:?}"),
            };
            framed
                .send(IpcMessage {
                    expected_workspace: None,
                    id: message.id,
                    source: ClientKind::Daemon,
                    payload: IpcPayload::Response(response),
                })
                .await
                .expect("reply");
        }
    });
    let mut connection = IpcConnection::connect_with(&transport.connector(), ClientKind::Cli)
        .await
        .expect("connection");
    let saved = match connection
        .request_owned(
            Request::SaveDraft {
                draft: draft(&path),
            },
            &workspace(),
        )
        .await
        .expect("upload")
    {
        ResponseData::Draft { draft } => draft,
        other => panic!("draft: {other:?}"),
    };
    assert_eq!(observed.lock().expect("lock").as_slice(), bytes);
    std::fs::remove_file(path).expect("remove client file");
    let identity = mxr_core::SendIdentity::for_draft(&saved);
    assert_eq!(identity.expected_revision, 2);
    connection
        .request_owned(
            Request::SendDraft {
                draft: saved,
                send_identity: Some(identity),
                override_safety_token: None,
            },
            &workspace(),
        )
        .await
        .expect("send");
    drop(connection);
    server.await.expect("server");
}

#[tokio::test]
async fn portable_upload_raw_variants_refuse_old_daemon_on_same_connection() {
    let transport = MemoryTransport::new();
    let mut listener = transport.bind().await.expect("listener");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut framed = Framed::new(stream, IpcCodec::new());
        let mut count = 0;
        while let Some(Ok(message)) = framed.next().await {
            assert!(matches!(
                message.payload,
                IpcPayload::Request(Request::GetStatus)
            ));
            count += 1;
            framed
                .send(IpcMessage {
                    expected_workspace: None,
                    id: message.id,
                    source: ClientKind::Daemon,
                    payload: IpcPayload::Response(Response::Ok {
                        data: status(false),
                    }),
                })
                .await
                .expect("reply");
        }
        count
    });
    let owner = draft(std::path::Path::new("/unused"));
    let metadata = mxr_core::DraftAttachment {
        id: DraftAttachmentId::new(),
        draft_id: owner.id.clone(),
        account_id: owner.account_id.clone(),
        filename: "a.txt".into(),
        mime_type: "text/plain".into(),
        length_bytes: 0,
        sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
    };
    let mut connection = IpcConnection::connect_with(&transport.connector(), ClientKind::Mcp)
        .await
        .expect("connection");
    for request in [
        Request::PreviewDraftAttachment {
            metadata: metadata.clone(),
        },
        Request::BeginDraftAttachment {
            metadata: metadata.clone(),
        },
        Request::WriteDraftAttachmentChunk {
            draft_id: owner.id.clone(),
            account_id: owner.account_id.clone(),
            resource_id: metadata.id.clone(),
            offset: 0,
            data_base64: "YQ==".into(),
        },
        Request::FinalizeDraftAttachment {
            draft_id: owner.id,
            account_id: owner.account_id,
            resource_id: metadata.id,
        },
    ] {
        let error = connection
            .request_owned(request, &workspace())
            .await
            .expect_err("unsupported");
        assert!(error.to_string().contains("upgrade"));
    }
    drop(connection);
    assert_eq!(server.await.expect("server"), 4);
}

fn workspace() -> mxr_protocol::ArchiveWorkspaceData {
    mxr_protocol::ArchiveWorkspaceData {
        workspace_id: "00000000-0000-7000-8000-000000000001".into(),
        recovery_generation: "00000000-0000-7000-8000-000000000002".into(),
    }
}
