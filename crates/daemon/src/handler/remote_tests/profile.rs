//! Remote grants and cooperative profiles must both permit every operation.
use super::*;
use mxr_core::{AccountId, Draft, DraftAttachment, DraftAttachmentId, MessageDirection};
use mxr_protocol::{ClientKind, IpcErrorKind, Response};

async fn set_profile(state: &AppState, kind: ClientKind, account: &AccountId, send: bool) {
    let mut config = state.config_snapshot();
    config.agent_surfaces.profiles.insert(
        kind.as_str().into(),
        mxr_config::AgentProfileConfig {
            safety_policy: mxr_config::SafetyPolicy::Full,
            allowed_accounts: vec![account.as_str()],
            allow_send: send,
            allow_destructive: false,
            ..Default::default()
        },
    );
    state.set_config_for_test(config).await;
}

fn metadata(draft: &Draft) -> DraftAttachment {
    DraftAttachment {
        id: DraftAttachmentId::new(),
        draft_id: draft.id.clone(),
        account_id: draft.account_id.clone(),
        filename: "synthetic.txt".into(),
        mime_type: "text/plain".into(),
        content_id: None,
        content_location: None,
        length_bytes: 0,
        sha256: base16ct::lower::encode_string(&Sha256::digest([])),
    }
}

async fn call(
    state: &Arc<AppState>,
    id: &str,
    proof: &RemoteSecret,
    kind: ClientKind,
    request: &Request,
) -> Result<ResponseData, super::super::HandlerError> {
    remote::read_owned_as(state, id, proof, request, Some(&owner(state).await), kind).await
}

fn assert_profile_denial(
    result: Result<ResponseData, super::super::HandlerError>,
    request: &Request,
) {
    let error = result
        .expect_err("host profile must refuse before effects")
        .into_response();
    assert!(
        matches!(
            error,
            Response::Error {
                kind: IpcErrorKind::Policy,
                ..
            }
        ),
        "profile did not gate {request:?}: {error:?}"
    );
}

#[tokio::test]
async fn broad_registration_cannot_bypass_profiles_on_preview_send_or_attachment_paths() {
    for kind in [ClientKind::Agent, ClientKind::Mcp] {
        let state = Arc::new(AppState::in_memory().await.unwrap());
        let allowed = state.default_account_id();
        let excluded = AccountId::new();
        state
            .store
            .insert_account(&crate::test_fixtures::test_account_with_id(
                excluded.clone(),
            ))
            .await
            .unwrap();
        set_profile(&state, kind, &allowed, false).await;
        let (id, proof) = send_registration(&state).await;
        let mut own = portable_draft(&state);
        state.store.insert_draft(&own).await.unwrap();
        own = state.store.get_draft(&own.id).await.unwrap().unwrap();
        let mut other = portable_draft(&state);
        other.account_id = excluded;
        state.store.insert_draft(&other).await.unwrap();
        other = state.store.get_draft(&other.id).await.unwrap().unwrap();
        // A real allowed preview succeeds, proving this is account filtering
        // rather than a blanket rejection of all remote agent requests.
        let preview = Request::PreviewDraft {
            draft: own.clone(),
            context: Default::default(),
            pending_attachments: vec![],
        };
        assert!(matches!(
            call(&state, &id, &proof, kind, &preview).await.unwrap(),
            ResponseData::DraftPreview { .. }
        ));
        let preview_attachment = Request::PreviewDraftAttachment {
            metadata: metadata(&own),
        };
        assert!(matches!(
            call(&state, &id, &proof, kind, &preview_attachment)
                .await
                .unwrap(),
            ResponseData::DraftAttachment { .. }
        ));
        for draft in [&own, &other] {
            for request in [
                Request::SendDraft {
                    draft: draft.clone(),
                    send_identity: None,
                    override_safety_token: None,
                },
                Request::SendStoredDraft {
                    draft_id: draft.id.clone(),
                    send_identity: None,
                    override_safety_token: None,
                    managed_attachments_reviewed: false,
                },
            ] {
                assert_profile_denial(call(&state, &id, &proof, kind, &request).await, &request);
            }
        }
        let resource = metadata(&other);
        for request in [
            Request::PreviewDraft {
                draft: other.clone(),
                context: Default::default(),
                pending_attachments: vec![],
            },
            Request::CheckDraftSafety {
                draft: other.clone(),
                context: Default::default(),
            },
            Request::PreviewDraftAttachment {
                metadata: resource.clone(),
            },
            Request::BeginDraftAttachment {
                metadata: resource.clone(),
            },
            Request::WriteDraftAttachmentChunk {
                account_id: other.account_id.clone(),
                draft_id: other.id.clone(),
                resource_id: resource.id.clone(),
                offset: 0,
                data_base64: String::new(),
            },
            Request::FinalizeDraftAttachment {
                account_id: other.account_id.clone(),
                draft_id: other.id.clone(),
                resource_id: resource.id.clone(),
            },
            // Declaring an allowed owner does not overwrite an excluded draft.
            Request::SaveDraft {
                draft: Draft {
                    account_id: own.account_id.clone(),
                    ..other.clone()
                },
            },
        ] {
            assert_profile_denial(call(&state, &id, &proof, kind, &request).await, &request);
        }
        let resources: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM draft_attachment_resources")
            .fetch_one(state.store.reader())
            .await
            .unwrap();
        assert_eq!(resources, 0, "denied upload paths wrote no resource");
        assert_eq!(
            state
                .store
                .get_draft(&other.id)
                .await
                .unwrap()
                .unwrap()
                .account_id,
            other.account_id
        );
    }
}

#[tokio::test]
async fn permissive_profile_cannot_broaden_registration_and_missing_profiles_are_denied() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let kind = ClientKind::Mcp;
    let account = state.default_account_id();
    let (id, proof) = registered_client(&state).await;
    let error = call(&state, &id, &proof, kind, &Request::GetRemoteReady)
        .await
        .unwrap_err()
        .into_response();
    assert!(matches!(
        error,
        Response::Error {
            kind: IpcErrorKind::Policy,
            ..
        }
    ));
    set_profile(&state, kind, &account, true).await;
    let mut config = state.config_snapshot();
    config
        .agent_surfaces
        .profiles
        .get_mut("mcp")
        .unwrap()
        .allow_destructive = true;
    state.set_config_for_test(config).await;
    let request = Request::SaveDraft {
        draft: portable_draft(&state),
    };
    let error = call(&state, &id, &proof, kind, &request)
        .await
        .unwrap_err()
        .into_response();
    assert!(
        matches!(
            error,
            Response::Error {
                kind: IpcErrorKind::Auth,
                ..
            }
        ),
        "profile broadened a read-only registration: {error:?}"
    );
    let request = Request::mutation(mxr_protocol::MutationCommand::Trash {
        message_ids: vec![],
    });
    let error = call(&state, &id, &proof, kind, &request)
        .await
        .unwrap_err()
        .into_response();
    assert!(matches!(
        error,
        Response::Error {
            kind: IpcErrorKind::Auth,
            ..
        }
    ));
    assert!(matches!(
        call(&state, &id, &proof, kind, &Request::Ping)
            .await
            .unwrap(),
        ResponseData::Pong
    ));
    let error = call(
        &state,
        &id,
        &RemoteSecret::new("wrong".into()),
        kind,
        &Request::Ping,
    )
    .await
    .unwrap_err()
    .into_response();
    assert!(matches!(
        error,
        Response::Error {
            kind: IpcErrorKind::Auth,
            ..
        }
    ));
}

#[tokio::test]
async fn paired_profile_refuses_excluded_reads_while_cli_reads_remain_ordinary() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let own = state.default_account_id();
    let excluded = AccountId::new();
    state
        .store
        .insert_account(&crate::test_fixtures::test_account_with_id(
            excluded.clone(),
        ))
        .await
        .unwrap();
    set_profile(&state, ClientKind::Mcp, &own, false).await;
    let (id, proof) = registered_client(&state).await;
    let thread = mxr_core::ThreadId::new();
    let mut mails = Vec::new();
    for (account, subject) in [(own.clone(), "allowed"), (excluded, "excluded secret")] {
        let mail = TestEnvelopeBuilder::new()
            .account_id(account)
            .thread_id(thread.clone())
            .subject(subject)
            .build();
        state
            .store
            .upsert_envelope_with_direction(&mail, MessageDirection::Inbound)
            .await
            .unwrap();
        mails.push(mail);
    }
    let request = Request::GetEnvelope {
        message_id: mails[1].id.clone(),
    };
    assert_profile_denial(
        call(&state, &id, &proof, ClientKind::Mcp, &request).await,
        &request,
    );
    assert!(
        matches!(call(&state, &id, &proof, ClientKind::Cli, &request).await.unwrap(), ResponseData::Envelope { envelope } if envelope.id == mails[1].id)
    );
    let request = Request::ListEnvelopes {
        label_id: None,
        account_id: Some(own),
        limit: 20,
        offset: 0,
        label_role: None,
    };
    let ResponseData::Envelopes { envelopes } =
        call(&state, &id, &proof, ClientKind::Mcp, &request)
            .await
            .unwrap()
    else {
        panic!("envelopes");
    };
    assert_eq!(envelopes.len(), 1);
    assert_eq!(envelopes[0].id, mails[0].id);
    // A shared thread request cannot reveal the excluded account's content.
    let request = Request::GetThread { thread_id: thread };
    assert_profile_denial(
        call(&state, &id, &proof, ClientKind::Mcp, &request).await,
        &request,
    );
}
