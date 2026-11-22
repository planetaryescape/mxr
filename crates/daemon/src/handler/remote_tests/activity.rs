use super::*;
use crate::handler::handle_request;
use mxr_protocol::{ClientKind, IpcMessage, IpcPayload, MutationCommand, Response};
use mxr_store::{ActivityFilter, ActivityRow};

async fn synced_message(state: &AppState) -> MessageId {
    state
        .sync_engine
        .sync_account(state.default_provider().as_ref())
        .await
        .unwrap();
    state
        .store
        .list_envelopes_by_account(&state.default_account_id(), 1, 0)
        .await
        .unwrap()
        .remove(0)
        .id
}

async fn archive_from_paired_client(
    state: &Arc<AppState>,
    client_id: &str,
    mail: &RemoteSecret,
    message_id: MessageId,
) -> Response {
    let message = IpcMessage {
        expected_workspace: None,
        id: 1,
        source: ClientKind::Cli,
        payload: IpcPayload::Request(Request::RemoteRead {
            expected_workspace: Some(owner(state).await),
            client_id: client_id.into(),
            mail_secret: mail.clone(),
            request: Box::new(Request::mutation(MutationCommand::Archive {
                message_ids: vec![message_id],
            })),
        }),
    };
    let IpcPayload::Response(response) = handle_request(state, &message).await.payload else {
        panic!("expected a response");
    };
    response
}

async fn recorded_archives(state: &AppState) -> Vec<ActivityRow> {
    crate::activity::settle(&state.activity).await;
    state
        .store
        .list_activity(
            &ActivityFilter {
                actions: vec!["mail.archive".into()],
                ..Default::default()
            },
            10,
            None,
        )
        .await
        .unwrap()
        .rows
}

#[tokio::test]
async fn a_paired_clients_archive_is_recorded_once_without_its_credentials() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let (client_id, mail) = workflow_client(
        &state,
        &[RemoteCapability::MailRead, RemoteCapability::MailTriage],
    )
    .await;
    let message_id = synced_message(&state).await;

    let response = archive_from_paired_client(&state, &client_id, &mail, message_id.clone()).await;

    assert!(matches!(response, Response::Ok { .. }), "{response:?}");
    let rows = recorded_archives(&state).await;
    assert_eq!(rows.len(), 1, "one remote action must write one row");
    assert_eq!(rows[0].source, "cli");
    assert_eq!(rows[0].target_id, Some(message_id.as_str().clone()));
    assert!(!format!("{:?}", rows[0]).contains(mail.expose()));
}

#[tokio::test]
async fn a_disabled_activity_log_drops_remote_actions() {
    let mut state = AppState::in_memory().await.unwrap();
    state.activity = crate::activity::Recorder::spawn(
        state.store.clone(),
        &mxr_config::ActivityConfig {
            enabled: false,
            ..Default::default()
        },
    );
    let state = Arc::new(state);
    let (client_id, mail) = workflow_client(
        &state,
        &[RemoteCapability::MailRead, RemoteCapability::MailTriage],
    )
    .await;
    let message_id = synced_message(&state).await;

    let response = archive_from_paired_client(&state, &client_id, &mail, message_id).await;

    assert!(matches!(response, Response::Ok { .. }), "{response:?}");
    assert!(recorded_archives(&state).await.is_empty());
}
