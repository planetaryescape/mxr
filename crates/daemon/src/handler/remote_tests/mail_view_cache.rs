use super::*;
use crate::handler::{handle_request, mode_cache::ModeView};
use mxr_protocol::{ClientKind, IpcMessage, IpcPayload, RemoteRequest, Response};

#[tokio::test]
async fn browser_presence_preserves_mail_views_without_bypassing_revocation() {
    let state = Arc::new(AppState::in_memory().await.unwrap());
    let (parent, mail) = registered_client(&state).await;
    let now = chrono::Utc::now().timestamp();
    let (code, _) = state
        .store
        .issue_browser_code(&parent, &mail, now)
        .await
        .unwrap();
    let (browser, proof, _) = state.store.claim_browser_code(&code, now).await.unwrap();
    let read = Request::GetNow { account_id: None };
    remote::read(&state, &browser, &proof, &read).await.unwrap();
    let view = ModeView::Now { account_id: None };
    assert!(state.mode_cache.cached(&state, &view).is_some());
    let session_id = browser.strip_prefix("browser:").unwrap().to_string();
    for request in [
        RemoteRequest::TouchSession {
            client_id: browser.clone(),
            mail_secret: proof.clone(),
            session_id: session_id.clone(),
            surface: "web".into(),
        },
        RemoteRequest::CloseSession {
            client_id: browser.clone(),
            mail_secret: proof.clone(),
            session_id,
        },
    ] {
        let response = handle_request(
            &state,
            &IpcMessage {
                id: 1,
                source: ClientKind::Web,
                expected_workspace: None,
                payload: IpcPayload::Request(Request::RemoteAuthority { request }),
            },
        )
        .await;
        assert!(matches!(
            response.payload,
            IpcPayload::Response(Response::Ok { .. })
        ));
        assert!(
            state.mode_cache.cached(&state, &view).is_some(),
            "presence invalidated the mail view"
        );
    }
    state
        .store
        .revoke_remote_client(&parent, false, now)
        .await
        .unwrap();
    assert!(
        remote::read(&state, &browser, &proof, &read).await.is_err(),
        "a cached view must still require current authority"
    );
}
