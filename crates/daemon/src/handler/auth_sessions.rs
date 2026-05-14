use super::{resolve_gmail_credentials, upsert_account_config, HandlerResult};
use crate::state::{AppState, AuthSessionRuntime};
use mxr_protocol::{
    AccountConfigData, AccountSyncConfigData, AuthFlowData, AuthSessionData, AuthSessionId,
    AuthSessionStateData, ResponseData,
};
use parking_lot::Mutex as ParkingMutex;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use yup_oauth2::authenticator_delegate::{
    DeviceAuthResponse, DeviceFlowDelegate, InstalledFlowDelegate,
};

pub(super) async fn start_auth_session(
    state: &Arc<AppState>,
    account: AccountConfigData,
    reauthorize: bool,
    requested_flow: AuthFlowData,
) -> HandlerResult {
    match account.sync.clone() {
        Some(AccountSyncConfigData::Gmail {
            credential_source,
            client_id,
            client_secret,
            token_ref,
        }) => {
            start_gmail_auth_session(
                state,
                account,
                reauthorize,
                requested_flow,
                credential_source,
                client_id,
                client_secret,
                token_ref,
            )
            .await
        }
        Some(AccountSyncConfigData::OutlookPersonal {
            client_id,
            token_ref,
        }) => {
            start_outlook_auth_session(
                state,
                account,
                reauthorize,
                client_id,
                token_ref,
                mxr_provider_outlook::OutlookTenant::Personal,
            )
            .await
        }
        Some(AccountSyncConfigData::OutlookWork {
            client_id,
            token_ref,
        }) => {
            start_outlook_auth_session(
                state,
                account,
                reauthorize,
                client_id,
                token_ref,
                mxr_provider_outlook::OutlookTenant::Work,
            )
            .await
        }
        _ => Err("auth sessions are only available for Gmail and Outlook accounts".into()),
    }
}

#[allow(clippy::too_many_arguments)]
async fn start_gmail_auth_session(
    state: &Arc<AppState>,
    account: AccountConfigData,
    reauthorize: bool,
    requested_flow: AuthFlowData,
    credential_source: mxr_protocol::GmailCredentialSourceData,
    client_id: String,
    client_secret: Option<String>,
    token_ref: String,
) -> HandlerResult {
    let (client_id, client_secret) =
        resolve_gmail_credentials(credential_source, client_id, client_secret)?;
    let flow = match requested_flow {
        AuthFlowData::Auto => mxr_provider_gmail::auth::AuthFlow::auto_detect(),
        AuthFlowData::Installed => mxr_provider_gmail::auth::AuthFlow::Installed,
        AuthFlowData::Device => mxr_provider_gmail::auth::AuthFlow::Device,
    };
    let flow_data = auth_flow_data(flow);
    let session_id = AuthSessionId(uuid::Uuid::now_v7().to_string());
    let session = AuthSessionData {
        session_id: session_id.clone(),
        state: AuthSessionStateData::Starting,
        flow: flow_data,
        account_key: account.key.clone(),
        auth_url: None,
        user_code: None,
        verification_uri: None,
        expires_at_unix: None,
        poll_interval_secs: None,
        message: Some("Starting Gmail authorization.".into()),
        error: None,
    };
    let status = Arc::new(ParkingMutex::new(session.clone()));
    let task_status = status.clone();
    let task_session_id = session_id.clone();

    let handle = tokio::spawn(async move {
        let mut auth = crate::provider_credentials::gmail_auth(client_id, client_secret, token_ref);
        let auth_result = if reauthorize {
            auth.interactive_auth_with_delegates(
                flow,
                Some(Box::new(SessionInstalledDelegate {
                    status: task_status.clone(),
                })),
                Some(Box::new(SessionDeviceDelegate {
                    status: task_status.clone(),
                })),
            )
            .await
        } else {
            match auth.load_existing().await {
                Ok(()) => Ok(()),
                Err(_) => {
                    auth.interactive_auth_with_delegates(
                        flow,
                        Some(Box::new(SessionInstalledDelegate {
                            status: task_status.clone(),
                        })),
                        Some(Box::new(SessionDeviceDelegate {
                            status: task_status.clone(),
                        })),
                    )
                    .await
                }
            }
        };

        update_session(&task_status, |session| match auth_result {
            Ok(()) => {
                session.state = AuthSessionStateData::Authorized;
                session.message = Some("Gmail authorization completed.".into());
                session.error = None;
            }
            Err(error) => {
                session.state = AuthSessionStateData::Failed;
                session.message = None;
                session.error = Some(error.to_string());
            }
        });
        tracing::debug!(session_id = %task_session_id.0, "auth session task finished");
    });

    state.auth_sessions.lock().insert(
        session_id,
        AuthSessionRuntime {
            account,
            status,
            handle,
        },
    );

    Ok(ResponseData::AuthSession { session })
}

async fn start_outlook_auth_session(
    state: &Arc<AppState>,
    account: AccountConfigData,
    reauthorize: bool,
    client_id: Option<String>,
    token_ref: String,
    tenant: mxr_provider_outlook::OutlookTenant,
) -> HandlerResult {
    let client_id = client_id
        .or_else(|| mxr_provider_outlook::OutlookAuth::bundled_client_id().map(str::to_owned))
        .ok_or_else(|| {
            "no bundled Outlook client_id; rebuild with OUTLOOK_CLIENT_ID or provide one"
                .to_string()
        })?;
    let session_id = AuthSessionId(uuid::Uuid::now_v7().to_string());
    let session = AuthSessionData {
        session_id: session_id.clone(),
        state: AuthSessionStateData::Starting,
        flow: AuthFlowData::Device,
        account_key: account.key.clone(),
        auth_url: None,
        user_code: None,
        verification_uri: None,
        expires_at_unix: None,
        poll_interval_secs: None,
        message: Some("Starting Microsoft authorization.".into()),
        error: None,
    };
    let status = Arc::new(ParkingMutex::new(session.clone()));
    let task_status = status.clone();
    let task_session_id = session_id.clone();

    let handle = tokio::spawn(async move {
        let auth = crate::provider_credentials::outlook_auth(client_id, token_ref, tenant);
        let auth_result = async {
            if !reauthorize && auth.load_tokens()?.is_some() {
                return Ok(());
            }

            let device = auth.start_device_flow().await?;
            update_session(&task_status, |session| {
                session.state = AuthSessionStateData::WaitingForUser;
                session.auth_url = device.verification_uri_complete.clone();
                session.user_code = Some(device.user_code.clone());
                session.verification_uri = Some(device.verification_uri.clone());
                session.expires_at_unix =
                    Some(chrono::Utc::now().timestamp() + device.expires_in as i64);
                session.poll_interval_secs = Some(device.interval.max(5));
                session.message = Some("Enter the Microsoft device code in a browser.".into());
                session.error = None;
            });
            let tokens = auth
                .poll_for_token(&device.device_code, device.interval)
                .await?;
            auth.save_tokens(&tokens)?;
            Ok::<(), mxr_provider_outlook::OutlookError>(())
        }
        .await;

        update_session(&task_status, |session| match auth_result {
            Ok(()) => {
                session.state = AuthSessionStateData::Authorized;
                session.message = Some("Microsoft authorization completed.".into());
                session.error = None;
            }
            Err(error) => {
                session.state = AuthSessionStateData::Failed;
                session.message = None;
                session.error = Some(error.to_string());
            }
        });
        tracing::debug!(session_id = %task_session_id.0, "auth session task finished");
    });

    state.auth_sessions.lock().insert(
        session_id,
        AuthSessionRuntime {
            account,
            status,
            handle,
        },
    );

    Ok(ResponseData::AuthSession { session })
}

pub(super) fn get_auth_session(state: &Arc<AppState>, session_id: &AuthSessionId) -> HandlerResult {
    let Some(runtime) = state
        .auth_sessions
        .lock()
        .get(session_id)
        .map(|runtime| runtime.status.lock().clone())
    else {
        return Err(format!("auth session '{}' not found", session_id.0));
    };
    Ok(ResponseData::AuthSession { session: runtime })
}

pub(super) fn cancel_auth_session(
    state: &Arc<AppState>,
    session_id: &AuthSessionId,
) -> HandlerResult {
    let Some(runtime) = state.auth_sessions.lock().remove(session_id) else {
        return Err(format!("auth session '{}' not found", session_id.0));
    };
    runtime.handle.abort();
    update_session(&runtime.status, |session| {
        session.state = AuthSessionStateData::Cancelled;
        session.message = Some("Gmail authorization cancelled.".into());
        session.error = None;
    });
    let session = runtime.status.lock().clone();
    Ok(ResponseData::AuthSession { session })
}

pub(super) async fn complete_auth_session(
    state: &Arc<AppState>,
    session_id: &AuthSessionId,
    save_account: bool,
) -> HandlerResult {
    let (account, status) = {
        let sessions = state.auth_sessions.lock();
        let Some(runtime) = sessions.get(session_id) else {
            return Err(format!("auth session '{}' not found", session_id.0));
        };
        (runtime.account.clone(), runtime.status.clone())
    };

    if status.lock().state != AuthSessionStateData::Authorized {
        return Ok(ResponseData::AuthSession {
            session: status.lock().clone(),
        });
    }

    if save_account {
        let result = upsert_account_config(state, account).await;
        update_session(&status, |session| {
            if result.ok {
                session.message = Some(result.summary.clone());
                session.error = None;
            } else {
                session.state = AuthSessionStateData::Failed;
                session.message = None;
                session.error = Some(result.summary.clone());
            }
        });
    }

    let session = status.lock().clone();
    Ok(ResponseData::AuthSession { session })
}

fn auth_flow_data(flow: mxr_provider_gmail::auth::AuthFlow) -> AuthFlowData {
    match flow {
        mxr_provider_gmail::auth::AuthFlow::Installed => AuthFlowData::Installed,
        mxr_provider_gmail::auth::AuthFlow::Device => AuthFlowData::Device,
    }
}

fn update_session(
    status: &Arc<ParkingMutex<AuthSessionData>>,
    update: impl FnOnce(&mut AuthSessionData),
) {
    let mut session = status.lock();
    update(&mut session);
}

struct SessionInstalledDelegate {
    status: Arc<ParkingMutex<AuthSessionData>>,
}

impl InstalledFlowDelegate for SessionInstalledDelegate {
    fn present_user_url<'a>(
        &'a self,
        url: &'a str,
        need_code: bool,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
        let status = self.status.clone();
        let auth_url = url.to_string();
        Box::pin(async move {
            update_session(&status, |session| {
                session.state = AuthSessionStateData::WaitingForUser;
                session.auth_url = Some(auth_url);
                session.message = Some("Open the Google authorization URL.".into());
                session.error = None;
            });
            if need_code {
                Err("manual installed-flow code entry is not supported by mxr auth sessions".into())
            } else {
                Ok(String::new())
            }
        })
    }
}

struct SessionDeviceDelegate {
    status: Arc<ParkingMutex<AuthSessionData>>,
}

impl DeviceFlowDelegate for SessionDeviceDelegate {
    fn present_user_code<'a>(
        &'a self,
        device_auth_resp: &'a DeviceAuthResponse,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        let status = self.status.clone();
        let user_code = device_auth_resp.user_code.clone();
        let verification_uri = device_auth_resp.verification_uri.clone();
        let expires_at_unix = device_auth_resp.expires_at.unix_timestamp();
        let poll_interval_secs = device_auth_resp.interval.as_secs();
        Box::pin(async move {
            update_session(&status, |session| {
                session.state = AuthSessionStateData::WaitingForUser;
                session.user_code = Some(user_code);
                session.verification_uri = Some(verification_uri);
                session.expires_at_unix = Some(expires_at_unix);
                session.poll_interval_secs = Some(poll_interval_secs);
                session.message = Some("Enter the Google device code in a browser.".into());
                session.error = None;
            });
        })
    }
}
