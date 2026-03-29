use super::*;

pub(crate) async fn default_account(
    socket_path: &Path,
) -> Result<(AccountId, String), BridgeError> {
    let mut accounts = match ipc_request(socket_path, Request::ListAccounts).await? {
        ResponseData::Accounts { accounts } => accounts,
        _ => return Err(BridgeError::UnexpectedResponse),
    };
    if accounts.is_empty() {
        return Err(BridgeError::Ipc("No runtime account configured".into()));
    }
    let index = accounts
        .iter()
        .position(|account| account.is_default)
        .unwrap_or(0);
    let account = accounts.swap_remove(index);
    Ok((account.account_id, account.email))
}

pub(crate) async fn account_summary(
    socket_path: &Path,
    account_id: &AccountId,
) -> Result<mxr_protocol::AccountSummaryData, BridgeError> {
    match ipc_request(socket_path, Request::ListAccounts).await? {
        ResponseData::Accounts { accounts } => accounts
            .into_iter()
            .find(|account| &account.account_id == account_id)
            .ok_or_else(|| BridgeError::Ipc("Account not found for compose session".into())),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

pub(crate) async fn envelope_for_message(
    socket_path: &Path,
    message_id: &str,
) -> Result<Envelope, BridgeError> {
    match ipc_request(
        socket_path,
        Request::GetEnvelope {
            message_id: parse_message_id(message_id)?,
        },
    )
    .await?
    {
        ResponseData::Envelope { envelope } => Ok(envelope),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

pub(crate) fn resolved_editor_command() -> String {
    std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| "vi".to_string())
}

pub(crate) async fn request_account_operation(
    socket_path: &Path,
    request: Request,
) -> Result<mxr_protocol::AccountOperationResult, BridgeError> {
    match ipc_request(socket_path, request).await? {
        ResponseData::AccountOperation { result } => Ok(result),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

pub(crate) async fn run_account_save_workflow(
    socket_path: &Path,
    account: mxr_protocol::AccountConfigData,
) -> Result<mxr_protocol::AccountOperationResult, BridgeError> {
    let mut result = if account.sync.as_ref().is_some_and(|sync| {
        matches!(
            sync,
            mxr_protocol::AccountSyncConfigData::Gmail { .. }
        )
    }) {
        request_account_operation(
            socket_path,
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

    merge_account_operation_result(
        &mut result,
        request_account_operation(
            socket_path,
            Request::UpsertAccountConfig {
                account: account.clone(),
            },
        )
        .await?,
    );

    if result.save.as_ref().is_some_and(|step| !step.ok) {
        return Ok(result);
    }

    merge_account_operation_result(
        &mut result,
        request_account_operation(socket_path, Request::TestAccountConfig { account }).await?,
    );

    Ok(result)
}

pub(crate) fn empty_account_operation_result() -> mxr_protocol::AccountOperationResult {
    mxr_protocol::AccountOperationResult {
        ok: true,
        summary: String::new(),
        save: None,
        auth: None,
        sync: None,
        send: None,
    }
}

pub(crate) fn merge_account_operation_result(
    base: &mut mxr_protocol::AccountOperationResult,
    next: mxr_protocol::AccountOperationResult,
) {
    base.ok &= next.ok;
    if !next.summary.is_empty() {
        base.summary = next.summary;
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

pub(crate) fn build_snooze_preset(
    name: &str,
    label: &str,
    config: &mxr_config::SnoozeConfig,
) -> serde_json::Value {
    let wake_at = resolve_snooze_until(name, config).unwrap_or_else(|_| Utc::now());
    json!({
        "id": name,
        "label": label,
        "wakeAt": wake_at,
    })
}

pub(crate) fn resolve_snooze_until(
    until: &str,
    config: &mxr_config::SnoozeConfig,
) -> Result<DateTime<Utc>, BridgeError> {
    mxr_config::snooze::parse_snooze_until(until, config)
        .ok_or_else(|| BridgeError::Ipc(format!("invalid snooze time: {until}")))
}
