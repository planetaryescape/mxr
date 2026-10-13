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
    let mail = RemoteSecret::new("a".repeat(64));
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
            client_id: "forged".into(),
            mail_secret: mail.clone(),
            request: Box::new(Request::Ping),
        },
    ] {
        assert!(remote::read(&state, "forged", &mail, &request)
            .await
            .is_err());
    }
}
