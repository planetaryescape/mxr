use super::super::*;

pub(crate) async fn accounts(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(&state.config.socket_path, Request::ListAccounts).await? {
        ResponseData::Accounts { accounts } => Ok(Json(json!({ "accounts": accounts }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

pub(crate) async fn test_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(account): Json<crate::mxr_protocol::AccountConfigData>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(
        &state.config.socket_path,
        Request::TestAccountConfig { account },
    )
    .await?
    {
        ResponseData::AccountOperation { result } => Ok(Json(json!({ "result": result }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

pub(crate) async fn upsert_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(account): Json<crate::mxr_protocol::AccountConfigData>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let result = run_account_save_workflow(&state.config.socket_path, account).await?;
    Ok(Json(json!({ "result": result })))
}

pub(crate) async fn set_default_account(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<SetDefaultAccountRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(
        &state.config.socket_path,
        Request::SetDefaultAccount { key: request.key },
    )
    .await?
    {
        ResponseData::AccountOperation { result } => Ok(Json(json!({ "result": result }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}
