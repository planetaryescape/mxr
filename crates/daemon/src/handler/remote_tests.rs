use super::remote;
use crate::{
    state::AppState,
    test_fixtures::{make_empty_body, TestEnvelopeBuilder},
};
use mxr_core::{
    AttachmentDisposition, AttachmentId, AttachmentMeta, MessageId, RemoteCapability, RemoteSecret,
};
use mxr_protocol::{Request, ResponseData};
use sha2::{Digest, Sha256};
use std::sync::Arc;

fn verifier(secret: &RemoteSecret) -> RemoteSecret {
    RemoteSecret::new(base16ct::lower::encode_string(&Sha256::digest(
        secret.expose().as_bytes(),
    )))
}

async fn registered_client(state: &AppState) -> (String, RemoteSecret) {
    let now = chrono::Utc::now().timestamp();
    let mail = RemoteSecret::new("a".repeat(64));
    let cleanup = RemoteSecret::new("b".repeat(64));
    let pairing = state
        .store
        .start_remote_pairing(
            "remote test",
            &[RemoteCapability::MailRead],
            &verifier(&mail),
            &verifier(&cleanup),
            now,
        )
        .await
        .unwrap();
    state
        .store
        .approve_remote_pairing(&pairing.request_id, false, now + 1)
        .await
        .unwrap();
    let client = state
        .store
        .claim_remote_pairing(&pairing.request_id, &mail, &cleanup, now + 2)
        .await
        .unwrap();
    (client.client_id, mail)
}

#[tokio::test]
async fn revocation_denies_the_next_operation_on_an_already_authenticated_client() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let (client_id, mail) = registered_client(&state).await;

    assert!(remote::read(&state, &client_id, &mail, &Request::Ping)
        .await
        .is_ok());
    state
        .store
        .revoke_remote_client(&client_id, false, chrono::Utc::now().timestamp())
        .await
        .unwrap();
    assert!(remote::read(&state, &client_id, &mail, &Request::Ping)
        .await
        .is_err());
}

#[tokio::test]
async fn authenticated_remote_client_can_count_mail_search_results() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let (client_id, mail) = registered_client(&state).await;
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();

    let response = remote::read(
        &state,
        &client_id,
        &mail,
        &Request::Count {
            query: "deployment".into(),
            account_id: None,
            mode: None,
        },
    )
    .await
    .unwrap();

    let ResponseData::Count { count } = response else {
        panic!("Count should return the matching mailbox result count");
    };
    assert!(count > 0, "the synthetic mailbox has deployment matches");
}

#[tokio::test]
async fn body_reads_remove_host_paths_from_bodies_and_failure_details() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let (client_id, mail) = registered_client(&state).await;
    let host_path = "/private/mxr/host-cache/secret.pdf";
    let envelope = TestEnvelopeBuilder::new()
        .account_id(state.default_account_id())
        .build();
    state.store.upsert_envelope(&envelope).await.unwrap();
    let mut body = make_empty_body(&envelope.id);
    body.attachments.push(AttachmentMeta {
        id: AttachmentId::new(),
        message_id: body.message_id.clone(),
        filename: "secret.pdf".into(),
        mime_type: "application/pdf".into(),
        disposition: AttachmentDisposition::Attachment,
        content_id: None,
        content_location: None,
        size_bytes: 7,
        local_path: Some(host_path.into()),
        provider_id: "fixture-attachment".into(),
    });
    let message_id = body.message_id.clone();
    state.store.insert_body(&body).await.unwrap();

    let response = remote::read(
        &state,
        &client_id,
        &mail,
        &Request::GetBody {
            message_id: message_id.clone(),
        },
    )
    .await
    .unwrap();
    let ResponseData::Body { body } = response else {
        panic!("GetBody should return one body");
    };
    assert!(body.attachments[0].local_path.is_none());
    assert!(!serde_json::to_string(&body).unwrap().contains(host_path));

    let missing_id = MessageId::new();
    let response = remote::read(
        &state,
        &client_id,
        &mail,
        &Request::ListBodies {
            message_ids: vec![message_id, missing_id],
        },
    )
    .await
    .unwrap();
    let ResponseData::Bodies { bodies, failures } = response else {
        panic!("ListBodies should return bodies and failures");
    };
    assert_eq!(bodies.len(), 1);
    assert!(bodies[0].attachments[0].local_path.is_none());
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].error, "Body unavailable on the remote host");
    let output = serde_json::to_string(&(bodies, failures)).unwrap();
    assert!(!output.contains(host_path));
    assert!(!output.contains("Body not found in local store"));
}

#[tokio::test]
async fn remote_admission_rejects_writes_private_diagnostics_and_nested_wrappers() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let (client_id, mail) = registered_client(&state).await;
    assert!(remote::read(&state, &client_id, &mail, &Request::Ping)
        .await
        .is_ok());

    for request in [
        Request::GetStatus,
        Request::GetDoctorReport,
        Request::Shutdown,
        Request::Mutation {
            mutation: mxr_protocol::MutationCommand::SetRead {
                message_ids: vec![MessageId::new()],
                read: true,
            },
            client_correlation_id: None,
        },
        Request::RemoteAuthority {
            request: mxr_protocol::RemoteRequest::Status,
        },
        Request::RemoteRead {
            expected_workspace: None,
            client_id: "forged".into(),
            mail_secret: mail.clone(),
            request: Box::new(Request::Ping),
        },
    ] {
        assert!(remote::read(&state, &client_id, &mail, &request)
            .await
            .is_err());
    }
}

fn portable_draft(state: &AppState) -> mxr_core::Draft {
    serde_json::from_value(serde_json::json!({
        "id": mxr_core::DraftId::new(), "account_id": state.default_account_id(),
        "revision": null, "reply_headers": null, "intent":"new", "to":[{"email":"recipient@example.com","name":null}],
        "cc":[], "bcc":[], "subject":"Remote draft", "body_markdown":"A plain reply.",
        "attachments":[], "managed_attachments":[], "inline_assets":[],
        "created_at":chrono::Utc::now(), "updated_at":chrono::Utc::now()
    })).unwrap()
}
async fn send_registration(state: &AppState) -> (String, RemoteSecret) {
    let now = chrono::Utc::now().timestamp();
    let mail = RemoteSecret::new("c".repeat(64));
    let cleanup = RemoteSecret::new("d".repeat(64));
    let p = state
        .store
        .start_remote_pairing(
            "send test",
            &[RemoteCapability::MailRead, RemoteCapability::MailSend],
            &verifier(&mail),
            &verifier(&cleanup),
            now,
        )
        .await
        .unwrap();
    state
        .store
        .approve_remote_pairing(&p.request_id, false, now)
        .await
        .unwrap();
    let c = state
        .store
        .claim_remote_pairing(&p.request_id, &mail, &cleanup, now)
        .await
        .unwrap();
    (c.client_id, mail)
}
async fn owner(state: &AppState) -> mxr_protocol::ArchiveWorkspaceData {
    let (workspace_id, recovery_generation) = state.store.archive_workspace().await.unwrap();
    mxr_protocol::ArchiveWorkspaceData {
        workspace_id,
        recovery_generation,
    }
}

#[tokio::test]
async fn read_only_and_revoked_connections_cannot_write_or_upload() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let draft = portable_draft(&state);
    let (id, mail) = registered_client(&state).await;
    let owner = owner(&state).await;
    assert!(remote::read_owned(
        &state,
        &id,
        &mail,
        &Request::SaveDraft {
            draft: draft.clone()
        },
        Some(&owner)
    )
    .await
    .is_err());
    let metadata = mxr_core::DraftAttachment {
        id: mxr_core::DraftAttachmentId::new(),
        draft_id: draft.id.clone(),
        account_id: draft.account_id.clone(),
        filename: "fixture.bin".into(),
        mime_type: "application/octet-stream".into(),
        content_id: None,
        content_location: None,
        length_bytes: 0,
        sha256: "a".repeat(64),
    };
    assert!(remote::read_owned(
        &state,
        &id,
        &mail,
        &Request::BeginDraftAttachment { metadata },
        Some(&owner)
    )
    .await
    .is_err());
    let (id, mail) = send_registration(&state).await;
    assert!(remote::read_owned(
        &state,
        &id,
        &mail,
        &Request::SaveDraft {
            draft: draft.clone()
        },
        None
    )
    .await
    .is_err());
    let mut wrong = owner.clone();
    wrong.workspace_id = uuid::Uuid::now_v7().to_string();
    assert!(remote::read_owned(
        &state,
        &id,
        &mail,
        &Request::SaveDraft {
            draft: draft.clone()
        },
        Some(&wrong)
    )
    .await
    .is_err());
    assert!(state.store.get_draft(&draft.id).await.unwrap().is_none());
    assert!(remote::read_owned(
        &state,
        &id,
        &mail,
        &Request::SaveDraft {
            draft: draft.clone()
        },
        Some(&owner)
    )
    .await
    .is_ok());
    state
        .store
        .revoke_remote_client(&id, false, chrono::Utc::now().timestamp())
        .await
        .unwrap();
    assert!(remote::read_owned(
        &state,
        &id,
        &mail,
        &Request::GetDraft { draft_id: draft.id },
        Some(&owner)
    )
    .await
    .is_err());
}

#[tokio::test]
async fn portable_preview_writes_no_draft_resource_audit_or_override_and_preserves_prose() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let (id, mail) = send_registration(&state).await;
    let owner = owner(&state).await;
    let mut draft = portable_draft(&state);
    draft.content =
        mxr_core::DraftContent::markdown("The prose mentions /private/a/file and stays intact.");
    let request = Request::PreviewDraft {
        draft: draft.clone(),
        context: mxr_protocol::DraftSafetyContextData::default(),
        pending_attachments: vec![],
    };
    let response = remote::read_owned(&state, &id, &mail, &request, Some(&owner))
        .await
        .unwrap();
    let ResponseData::DraftPreview {
        draft: returned, ..
    } = response
    else {
        panic!("preview");
    };
    assert_eq!(returned.content, draft.content);
    assert!(state.store.get_draft(&draft.id).await.unwrap().is_none());
    for (table, query) in [
        ("audit", "SELECT COUNT(*) FROM draft_safety_runs"),
        ("overrides", "SELECT COUNT(*) FROM draft_safety_overrides"),
        (
            "resources",
            "SELECT COUNT(*) FROM draft_attachment_resources",
        ),
    ] {
        let count: i64 = sqlx::query_scalar(query)
            .fetch_one(state.store.reader())
            .await
            .unwrap();
        assert_eq!(count, 0, "{table}");
    }
}

#[tokio::test]
async fn remote_new_and_legacy_path_drafts_are_rejected_before_host_files_are_opened() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let (id, mail) = send_registration(&state).await;
    let owner = owner(&state).await;
    let mut draft = portable_draft(&state);
    draft
        .attachments
        .push("/private/host-secret-do-not-open".into());
    let error = remote::read_owned(
        &state,
        &id,
        &mail,
        &Request::SaveDraft {
            draft: draft.clone(),
        },
        Some(&owner),
    )
    .await
    .unwrap_err()
    .into_response();
    assert!(
        matches!(error, mxr_protocol::Response::Error { code, .. } if code == "remote_path_backed_draft")
    );
    state.store.insert_draft(&draft).await.unwrap();
    let error = remote::read_owned(
        &state,
        &id,
        &mail,
        &Request::GetDraft {
            draft_id: draft.id.clone(),
        },
        Some(&owner),
    )
    .await
    .unwrap_err()
    .into_response();
    assert!(
        matches!(error, mxr_protocol::Response::Error { code, .. } if code == "remote_path_backed_draft")
    );
    draft.revision = state
        .store
        .get_draft(&draft.id)
        .await
        .unwrap()
        .unwrap()
        .revision;
    let error = remote::read_owned(
        &state,
        &id,
        &mail,
        &Request::SendStoredDraft {
            draft_id: draft.id.clone(),
            send_identity: Some(mxr_core::SendIdentity::for_draft(&draft)),
            override_safety_token: None,
            managed_attachments_reviewed: true,
        },
        Some(&owner),
    )
    .await
    .unwrap_err()
    .into_response();
    assert!(
        matches!(error, mxr_protocol::Response::Error { code, .. } if code == "remote_path_backed_draft")
    );
}

#[tokio::test]
async fn approved_send_grant_does_not_bypass_the_hosts_read_only_policy() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let (id, mail) = send_registration(&state).await;
    let owner = owner(&state).await;
    let draft = portable_draft(&state);
    let saved = remote::read_owned(
        &state,
        &id,
        &mail,
        &Request::SaveDraft { draft },
        Some(&owner),
    )
    .await
    .unwrap();
    let ResponseData::Draft { draft } = saved else {
        panic!("draft");
    };
    let mut config = state.config_snapshot();
    config.general.safety_policy = mxr_config::SafetyPolicy::ReadOnly;
    state.set_config_for_test(config).await;
    let result = remote::read_owned(
        &state,
        &id,
        &mail,
        &Request::SendStoredDraft {
            draft_id: draft.id.clone(),
            send_identity: Some(mxr_core::SendIdentity::for_draft(&draft)),
            override_safety_token: None,
            managed_attachments_reviewed: true,
        },
        Some(&owner),
    )
    .await;
    assert!(result.is_err());
    assert!(state
        .store
        .get_draft_send_outcome(&draft.id)
        .await
        .unwrap()
        .is_none());
}

struct DiagnosticSendFailure {
    known_rejection: bool,
}

#[async_trait::async_trait]
impl mxr_core::MailSendProvider for DiagnosticSendFailure {
    fn name(&self) -> &str {
        "diagnostic-failure"
    }

    async fn send(
        &self,
        _draft: &mxr_core::Draft,
        _from: &mxr_core::Address,
        _message_id: &str,
    ) -> Result<mxr_core::SendReceipt, mxr_core::MxrError> {
        Err(mxr_core::MxrError::Provider(
            "Cannot read /private/host/smtp.toml; run mxr accounts repair for consulting-smtp"
                .into(),
        ))
    }

    async fn send_classified(
        &self,
        draft: &mxr_core::Draft,
        from: &mxr_core::Address,
        message_id: &str,
    ) -> Result<mxr_core::SendReceipt, mxr_core::SendFailure> {
        self.send(draft, from, message_id).await.map_err(|error| {
            if self.known_rejection {
                mxr_core::SendFailure::NotAccepted(error)
            } else {
                mxr_core::SendFailure::Unknown(error)
            }
        })
    }
}

#[tokio::test]
async fn remote_provider_errors_keep_typed_outcome_without_host_diagnostics() {
    for (known_rejection, expected_state, expected_code) in [
        (
            true,
            mxr_core::SendOutcomeState::NotAccepted,
            "send_not_accepted",
        ),
        (
            false,
            mxr_core::SendOutcomeState::Unknown,
            "send_outcome_unknown",
        ),
    ] {
        let account_id = mxr_core::AccountId::new();
        let account = crate::test_fixtures::test_account_with_id(account_id.clone());
        let sync = Arc::new(mxr_provider_fake::FakeProvider::new(account_id));
        let state = Arc::new(
            AppState::in_memory_with_sync_provider(
                account,
                sync,
                Some(Arc::new(DiagnosticSendFailure { known_rejection })),
            )
            .await
            .unwrap(),
        );
        let (client_id, mail) = send_registration(&state).await;
        let owner = owner(&state).await;
        let saved = remote::read_owned(
            &state,
            &client_id,
            &mail,
            &Request::SaveDraft {
                draft: portable_draft(&state),
            },
            Some(&owner),
        )
        .await
        .unwrap();
        let ResponseData::Draft { draft } = saved else {
            panic!("expected saved draft");
        };
        let identity = mxr_core::SendIdentity::for_draft(&draft);
        let response = remote::read_owned(
            &state,
            &client_id,
            &mail,
            &Request::SendStoredDraft {
                draft_id: draft.id,
                send_identity: Some(identity.clone()),
                override_safety_token: None,
                managed_attachments_reviewed: true,
            },
            Some(&owner),
        )
        .await
        .unwrap_err()
        .into_response();
        let wire = serde_json::to_string(&response).unwrap();
        for private_detail in [
            "/private/host/smtp.toml",
            "mxr accounts repair",
            "consulting-smtp",
        ] {
            assert!(!wire.contains(private_detail), "leaked {private_detail}");
        }
        let mxr_protocol::Response::Error {
            code,
            details: Some(details),
            retryable,
            ..
        } = response
        else {
            panic!("expected typed send outcome");
        };
        assert_eq!(code, expected_code);
        assert!(!retryable);
        let outcome: mxr_core::DraftSendOutcome =
            serde_json::from_value(details["outcome"].clone()).unwrap();
        assert_eq!(outcome.state, expected_state);
        assert_eq!(outcome.identity, identity);
        let (stored, _) = state
            .store
            .get_send_outcome(&identity.operation_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            serde_json::to_value(outcome).unwrap(),
            serde_json::to_value(stored).unwrap()
        );
    }
}

#[tokio::test]
async fn remote_account_discovery_omits_provider_config_but_preserves_local_accounts() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let mut config = state.config_snapshot();
    config.accounts.clear();
    let account = |name: &str, email: &str, sync: serde_json::Value, send: serde_json::Value| {
        serde_json::from_value::<mxr_config::AccountConfig>(serde_json::json!({
            "name":name, "email":email, "enabled":true, "sync":sync, "send":send,
        }))
        .unwrap()
    };
    config.general.default_account = Some("gmail-fixture".into());
    config.accounts.insert("gmail-fixture".into(), account(
        "Synthetic Gmail", "gmail@example.com",
        serde_json::json!({"type":"gmail", "credential_source":"custom", "client_id":"SYNTHETIC_OAUTH_CLIENT_ID", "client_secret":"SYNTHETIC_OAUTH_CLIENT_SECRET", "token_ref":"SYNTHETIC_GMAIL_TOKEN_REF"}),
        serde_json::json!({"type":"gmail"}),
    ));
    config.accounts.insert("imap-fixture".into(), account(
        "Synthetic IMAP", "imap@example.com",
        serde_json::json!({"type":"imap", "host":"SYNTHETIC_PRIVATE_IMAP_HOST", "port":993, "username":"SYNTHETIC_IMAP_USERNAME", "password_ref":"SYNTHETIC_IMAP_PASSWORD_REF", "auth_required":true, "use_tls":true, "max_connections":4}),
        serde_json::json!({"type":"smtp", "host":"SYNTHETIC_PRIVATE_SMTP_HOST", "port":465, "username":"SYNTHETIC_SMTP_USERNAME", "password_ref":"SYNTHETIC_SMTP_PASSWORD_REF", "auth_required":true, "use_tls":true}),
    ));
    state.set_config_for_test(config).await;
    let local = super::dispatch(
        &state,
        mxr_protocol::ClientKind::Cli,
        &mxr_transport::PeerInfo::local(),
        &Request::ListAccounts,
    )
    .await;
    let local_json = serde_json::to_string(&local).unwrap();
    let mxr_protocol::Response::Ok {
        data: ResponseData::Accounts { mut accounts },
    } = local
    else {
        panic!("local accounts");
    };
    let (client_id, mail) = send_registration(&state).await;
    let remote = remote::read(&state, &client_id, &mail, &Request::ListAccounts)
        .await
        .unwrap();
    let wire = serde_json::to_string(&remote).unwrap();
    for sentinel in [
        "SYNTHETIC_OAUTH_CLIENT_ID",
        "SYNTHETIC_OAUTH_CLIENT_SECRET",
        "SYNTHETIC_GMAIL_TOKEN_REF",
        "SYNTHETIC_PRIVATE_IMAP_HOST",
        "SYNTHETIC_IMAP_USERNAME",
        "SYNTHETIC_IMAP_PASSWORD_REF",
        "SYNTHETIC_PRIVATE_SMTP_HOST",
        "SYNTHETIC_SMTP_USERNAME",
        "SYNTHETIC_SMTP_PASSWORD_REF",
    ] {
        assert!(
            local_json.contains(sentinel),
            "local config lost {sentinel}"
        );
        assert!(!wire.contains(sentinel), "remote config leaked {sentinel}");
    }
    for key in [
        "client_secret",
        "client_id",
        "token_ref",
        "host",
        "username",
        "password_ref",
        "password",
        "credential_source",
        "port",
        "auth_required",
        "use_tls",
        "max_connections",
    ] {
        assert!(
            !wire.contains(&format!("\"{key}\":")),
            "remote config key {key}"
        );
    }
    for account in &mut accounts {
        account.sync = None;
        account.send = None;
    }
    assert_eq!(
        serde_json::to_value(remote).unwrap(),
        serde_json::to_value(ResponseData::Accounts { accounts }).unwrap()
    );
    let local_after = super::dispatch(
        &state,
        mxr_protocol::ClientKind::Cli,
        &mxr_transport::PeerInfo::local(),
        &Request::ListAccounts,
    )
    .await;
    assert_eq!(
        serde_json::to_value(local_after).unwrap(),
        serde_json::from_str::<serde_json::Value>(&local_json).unwrap()
    );
    let (read_only, mail) = registered_client(&state).await;
    let refusal = remote::read(&state, &read_only, &mail, &Request::ListAccounts)
        .await
        .unwrap_err()
        .into_response();
    assert!(
        matches!(refusal, mxr_protocol::Response::Error { kind:mxr_protocol::IpcErrorKind::Auth, code, .. } if code == "remote_authority_denied")
    );
}
