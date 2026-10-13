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

fn declaration(filename: &str, bytes: &[u8]) -> DraftAttachment {
    DraftAttachment {
        id: DraftAttachmentId::new(),
        draft_id: mxr_core::DraftId::new(),
        account_id: mxr_core::AccountId::new(),
        filename: filename.into(),
        mime_type: "application/octet-stream".into(),
        length_bytes: bytes.len() as u64,
        sha256: base16ct::lower::encode_string(&Sha256::digest(bytes)),
    }
}

#[test]
fn preview_attachment_comparison_preserves_set_multiplicity_and_managed_bytes() {
    let a = declaration("same.bin", b"original A");
    let b = declaration("same.bin", b"original B");
    let reviewed = [a.clone(), b.clone()];
    assert!(check_preview_attachments(
        &reviewed,
        &[],
        &[
            ("same.bin".into(), b"original B".to_vec()),
            ("same.bin".into(), b"original A".to_vec()),
        ]
    )
    .is_ok());
    assert!(check_preview_attachments(
        &reviewed,
        &[],
        &[
            ("same.bin".into(), b"original B".to_vec()),
            ("same.bin".into(), b"original B".to_vec()),
        ]
    )
    .is_err());
    assert!(check_preview_attachments(&reviewed, &[a.clone()], &[]).is_err());
    assert!(check_preview_attachments(&reviewed, &[], &[]).is_err());
    assert!(check_preview_attachments(&[a.clone()], &[a.clone(), a.clone()], &[]).is_err());
    let mut uploaded_a = a.clone();
    uploaded_a.id = DraftAttachmentId::new();
    assert!(check_preview_attachments(&reviewed, &[uploaded_a, b], &[]).is_ok());
    assert!(check_preview_attachments(&[a.clone(), a.clone()], &[a.clone(), a], &[]).is_ok());
}

#[tokio::test]
async fn preview_transfer_rejects_changed_session_without_consuming_original_review() {
    let transport = MemoryTransport::new();
    let _listener = transport.bind().await.expect("listener");
    let mut previous = IpcConnection::connect_with(&transport.connector(), ClientKind::Tui)
        .await
        .expect("connection");
    previous.selected_remote = true;
    previous.remote_workspace = Some(workspace());
    previous.remote_registration_identity = Some(("host".into(), "client".into()));
    previous.selection_generation = Some(7);
    let draft_id = mxr_core::DraftId::new();
    previous
        .preview_attachment_declarations
        .insert(draft_id.clone(), vec![declaration("a.bin", b"reviewed")]);
    for mismatch in 0..5 {
        let mut replacement = IpcConnection::connect_with(&transport.connector(), ClientKind::Tui)
            .await
            .expect("connection");
        replacement.selected_remote = true;
        replacement.remote_workspace = previous.remote_workspace.clone();
        replacement.remote_registration_identity = previous.remote_registration_identity.clone();
        replacement.selection_generation = previous.selection_generation;
        match mismatch {
            0 => {
                replacement
                    .remote_registration_identity
                    .as_mut()
                    .expect("identity")
                    .0 = "other host".into();
            }
            1 => {
                replacement
                    .remote_registration_identity
                    .as_mut()
                    .expect("identity")
                    .1 = "other client".into();
            }
            2 => {
                replacement
                    .remote_workspace
                    .as_mut()
                    .expect("owner")
                    .recovery_generation = "restored".into();
            }
            3 => replacement.selection_generation = Some(8),
            _ => replacement.selected_remote = false,
        }
        assert!(replacement
            .retain_draft_attachment_previews_from(&mut previous)
            .is_err());
        assert!(previous
            .preview_attachment_declarations
            .contains_key(&draft_id));
        assert!(replacement.preview_attachment_declarations.is_empty());
    }
    let mut replacement = IpcConnection::connect_with(&transport.connector(), ClientKind::Tui)
        .await
        .expect("connection");
    replacement.selected_remote = true;
    replacement.remote_workspace = previous.remote_workspace.clone();
    replacement.remote_registration_identity = previous.remote_registration_identity.clone();
    replacement.selection_generation = previous.selection_generation;
    replacement
        .retain_draft_attachment_previews_from(&mut previous)
        .expect("same session");
    assert!(previous.preview_attachment_declarations.is_empty());
    assert!(replacement
        .preview_attachment_declarations
        .contains_key(&draft_id));
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

#[tokio::test]
async fn remote_preview_uses_declarations_without_upload_and_changed_bytes_fail_before_save() {
    let directory = tempfile::tempdir().expect("directory");
    let path = directory.path().join("client.bin");
    std::fs::write(&path, b"reviewed bytes").expect("write");
    let transport = MemoryTransport::new();
    let mut listener = transport.bind().await.expect("listener");
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        let mut framed = Framed::new(stream, IpcCodec::new());
        let mut requests = Vec::new();
        while let Some(Ok(message)) = framed.next().await {
            assert!(matches!(
                message.payload,
                IpcPayload::Request(Request::GetRemoteReady)
            ));
            requests.push(message.payload.clone());
            framed
                .send(IpcMessage {
                    expected_workspace: None,
                    id: message.id,
                    source: ClientKind::Daemon,
                    payload: IpcPayload::Response(Response::Ok {
                        data: ResponseData::RemoteReady {
                            attachment_transfer_supported: true,
                            capabilities: vec![
                                mxr_core::RemoteCapability::MailRead,
                                mxr_core::RemoteCapability::MailSend,
                            ],
                            draft_attachment_upload_supported: true,
                            draft_revision_supported: true,
                            send_outcome_supported: true,
                            protocol_version: mxr_protocol::IPC_PROTOCOL_VERSION,
                            host_id: "fixture".into(),
                            workspace: workspace(),
                            client_id: "fixture".into(),
                        },
                    }),
                })
                .await
                .expect("reply");
        }
        requests
    });
    let mut connection = IpcConnection::connect_with(&transport.connector(), ClientKind::Tui)
        .await
        .expect("connection");
    connection.selected_remote = true;
    let draft = draft(&path);
    let preview = connection
        .prepare_draft_attachment_request(
            Request::CheckDraftSafety {
                draft: draft.clone(),
                context: Default::default(),
            },
            Some(&workspace()),
        )
        .await
        .expect("preview");
    let Request::PreviewDraft {
        draft: portable,
        pending_attachments,
        ..
    } = preview
    else {
        panic!("portable preview");
    };
    assert!(portable.attachments.is_empty());
    assert_eq!(pending_attachments.len(), 1);
    assert_eq!(pending_attachments[0].filename, "client.bin");
    assert!(!serde_json::to_string(&portable)
        .expect("JSON")
        .contains(path.to_str().expect("path")));
    std::fs::write(&path, b"changed bytes").expect("write");
    let error = connection
        .prepare_draft_attachment_request(Request::SaveDraft { draft }, Some(&workspace()))
        .await
        .expect_err("changed bytes");
    assert!(error.to_string().contains("changed since preview"));
    drop(connection);
    assert_eq!(server.await.expect("server").len(), 1);
}
