use crate::ipc::{ipc_call, IpcRequest};
use mxr_config::socket_path as config_socket_path;
use mxr_core::MxrError;
use mxr_protocol::{
    AccountConfigData, AccountOperationResult, AccountSyncConfigData, AuthSessionData,
    AuthSessionId, Request, Response, ResponseData,
};
use tokio::sync::mpsc;

pub(crate) fn daemon_socket_path() -> anyhow::Result<std::path::PathBuf> {
    // Route through the shared resolver so the TUI agrees with the CLI on the
    // socket (honors MXR_DAEMON_ADDR=unix://<path>). tcp:// / cmd:// are
    // CLI-only today; REJECT them with a clear message rather than silently
    // dialing the default socket (which would connect to a different daemon than
    // the user asked for).
    mxr_client::resolve_unix_socket(config_socket_path())
        .map_err(|error| anyhow::anyhow!("{error}"))
}

pub(crate) async fn request_account_operation(
    bg: &mpsc::UnboundedSender<IpcRequest>,
    request: Request,
) -> Result<AccountOperationResult, MxrError> {
    let resp = ipc_call(bg, request).await;
    match resp {
        Ok(Response::Ok {
            data: ResponseData::AccountOperation { result },
        }) => Ok(result),
        Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
        Err(e) => Err(e),
        _ => Err(MxrError::Ipc(
            "unexpected response to account operation".into(),
        )),
    }
}

pub(crate) async fn ipc_start_auth_session(
    bg: &mpsc::UnboundedSender<IpcRequest>,
    account: AccountConfigData,
    reauthorize: bool,
) -> Result<AuthSessionData, MxrError> {
    let resp = ipc_call(
        bg,
        Request::StartAuthSession {
            account,
            reauthorize,
            flow: mxr_protocol::AuthFlowData::Device,
        },
    )
    .await;
    match resp {
        Ok(Response::Ok {
            data: ResponseData::AuthSession { session },
        }) => Ok(session),
        Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
        Err(e) => Err(e),
        _ => Err(MxrError::Ipc(
            "unexpected response to StartAuthSession".into(),
        )),
    }
}

pub(crate) async fn ipc_get_auth_session(
    bg: &mpsc::UnboundedSender<IpcRequest>,
    session_id: AuthSessionId,
) -> Result<AuthSessionData, MxrError> {
    let resp = ipc_call(bg, Request::GetAuthSession { session_id }).await;
    match resp {
        Ok(Response::Ok {
            data: ResponseData::AuthSession { session },
        }) => Ok(session),
        Ok(Response::Error { message, .. }) => Err(MxrError::Ipc(message)),
        Err(e) => Err(e),
        _ => Err(MxrError::Ipc(
            "unexpected response to GetAuthSession".into(),
        )),
    }
}

pub(crate) async fn run_account_save_workflow(
    bg: &mpsc::UnboundedSender<IpcRequest>,
    account: AccountConfigData,
) -> Result<AccountOperationResult, MxrError> {
    let mut result = if matches!(
        account.sync,
        Some(
            AccountSyncConfigData::Gmail { .. }
                | AccountSyncConfigData::OutlookPersonal { .. }
                | AccountSyncConfigData::OutlookWork { .. }
        )
    ) {
        request_account_operation(
            bg,
            Request::AuthorizeAccountConfig {
                account: account.clone(),
                reauthorize: false,
            },
        )
        .await?
    } else {
        empty_account_operation_result()
    };

    if result.auth.as_ref().is_some_and(|step| !step.ok) {
        return Ok(result);
    }

    let save_result = request_account_operation(
        bg,
        Request::UpsertAccountConfig {
            account: account.clone(),
        },
    )
    .await?;
    merge_account_operation_result(&mut result, save_result);

    if result.save.as_ref().is_some_and(|step| !step.ok) {
        return Ok(result);
    }

    let test_result = request_account_operation(bg, Request::TestAccountConfig { account }).await?;
    merge_account_operation_result(&mut result, test_result);

    Ok(result)
}

/// Run upsert + test after Outlook device-code auth already completed.
/// Marks auth step as successful and proceeds with save + connectivity test.
pub(crate) async fn run_post_auth_save_workflow(
    bg: &mpsc::UnboundedSender<IpcRequest>,
    account: AccountConfigData,
) -> Result<AccountOperationResult, MxrError> {
    let mut result = empty_account_operation_result();
    result.auth = Some(mxr_protocol::AccountOperationStep {
        ok: true,
        detail: String::new(),
    });

    let save_result = request_account_operation(
        bg,
        Request::UpsertAccountConfig {
            account: account.clone(),
        },
    )
    .await?;
    merge_account_operation_result(&mut result, save_result);

    if result.save.as_ref().is_some_and(|step| !step.ok) {
        return Ok(result);
    }

    let test_result = request_account_operation(bg, Request::TestAccountConfig { account }).await?;
    merge_account_operation_result(&mut result, test_result);

    Ok(result)
}

pub(crate) fn empty_account_operation_result() -> AccountOperationResult {
    AccountOperationResult {
        ok: true,
        summary: String::new(),
        save: None,
        auth: None,
        sync: None,
        send: None,
        device_code_url: None,
        device_code_user_code: None,
    }
}

pub(crate) fn merge_account_operation_result(
    base: &mut AccountOperationResult,
    next: AccountOperationResult,
) {
    base.ok &= next.ok;
    if !next.summary.is_empty() {
        if base.summary.is_empty() {
            base.summary = next.summary;
        } else {
            base.summary = format!("{} | {}", base.summary, next.summary);
        }
    }
    if next.save.is_some() {
        base.save = next.save;
    }
    if next.auth.is_some() {
        base.auth = next.auth;
    }
    if next.sync.is_some() {
        base.sync = next.sync;
    }
    if next.send.is_some() {
        base.send = next.send;
    }
}
