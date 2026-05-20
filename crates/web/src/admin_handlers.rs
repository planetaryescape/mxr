use super::*;

pub(super) async fn status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(&state.config.socket_path, Request::GetStatus).await? {
        ResponseData::Status {
            uptime_secs,
            accounts,
            total_messages,
            daemon_pid,
            sync_statuses,
            protocol_version,
            daemon_version,
            daemon_build_id,
            repair_required,
            semantic_runtime,
            feature_health,
        } => Ok(Json(serde_json::json!({
            "uptime_secs": uptime_secs,
            "accounts": accounts,
            "total_messages": total_messages,
            "daemon_pid": daemon_pid,
            "sync_statuses": sync_statuses,
            "protocol_version": protocol_version,
            "daemon_version": daemon_version,
            "daemon_build_id": daemon_build_id,
            "repair_required": repair_required,
            "semantic_runtime": semantic_runtime,
            "feature_health": feature_health,
            "instance": mxr_config::app_instance_name(),
            "is_demo": mxr_config::is_demo_instance(),
        }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

pub(super) async fn shell(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let lens = MailboxLensRequest::default();
    let chrome = build_bridge_chrome(&state.config.socket_path, &lens).await?;
    Ok(Json(json!({
        "shell": chrome.shell,
        "sidebar": chrome.sidebar,
    })))
}

pub(super) async fn diagnostics(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(&state.config.socket_path, Request::GetDoctorReport).await? {
        ResponseData::DoctorReport { report } => Ok(Json(json!({ "report": report }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

pub(super) async fn generate_bug_report(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(
        &state.config.socket_path,
        Request::GenerateBugReport {
            verbose: false,
            full_logs: false,
            since: None,
        },
    )
    .await?
    {
        ResponseData::BugReport { content } => Ok(Json(json!({ "content": content }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}
