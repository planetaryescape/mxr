use super::super::*;

pub(crate) async fn snooze_presets(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let config = load_config().unwrap_or_default().snooze;
    let presets = [
        build_snooze_preset("tomorrow", "Tomorrow morning", &config),
        build_snooze_preset("tonight", "Tonight", &config),
        build_snooze_preset("weekend", "Weekend", &config),
        build_snooze_preset("monday", "Next Monday", &config),
    ];
    Ok(Json(json!({ "presets": presets })))
}

pub(crate) async fn snooze(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<SnoozeRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let config = load_config().unwrap_or_default().snooze;
    let wake_at = resolve_snooze_until(&request.until, &config)?;
    let _ = ack_request(
        &state.config.socket_path,
        Request::Snooze {
            message_id: parse_message_id(&request.message_id)?,
            wake_at,
        },
    )
    .await?;
    Ok(Json(json!({ "ok": true, "wake_at": wake_at })))
}

pub(crate) async fn unsubscribe(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<UnsubscribeRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    let _ = ack_request(
        &state.config.socket_path,
        Request::Unsubscribe {
            message_id: parse_message_id(&request.message_id)?,
        },
    )
    .await?;
    Ok(Json(json!({ "ok": true })))
}

pub(crate) async fn open_attachment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<AttachmentRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(
        &state.config.socket_path,
        Request::OpenAttachment {
            message_id: parse_message_id(&request.message_id)?,
            attachment_id: parse_attachment_id(&request.attachment_id)?,
        },
    )
    .await?
    {
        ResponseData::AttachmentFile { file } => Ok(Json(json!({ "file": file }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}

pub(crate) async fn download_attachment(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    Json(request): Json<AttachmentRequest>,
) -> Result<Json<serde_json::Value>, BridgeError> {
    ensure_authorized(&headers, auth.token.as_deref(), &state.config.auth_token)?;
    match ipc_request(
        &state.config.socket_path,
        Request::DownloadAttachment {
            message_id: parse_message_id(&request.message_id)?,
            attachment_id: parse_attachment_id(&request.attachment_id)?,
        },
    )
    .await?
    {
        ResponseData::AttachmentFile { file } => Ok(Json(json!({ "file": file }))),
        _ => Err(BridgeError::UnexpectedResponse),
    }
}
