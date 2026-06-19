use super::*;

#[derive(Debug, thiserror::Error)]
pub(super) enum BridgeError {
    #[error("failed to connect to mxr daemon at {0}")]
    Connect(String),
    #[error("ipc error: {0}")]
    Ipc(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("unexpected response from daemon")]
    UnexpectedResponse,
}

impl IntoResponse for BridgeError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            _ => StatusCode::BAD_GATEWAY,
        };
        (
            status,
            Json(serde_json::json!({ "error": self.to_string() })),
        )
            .into_response()
    }
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct AuthQuery {
    #[serde(default)]
    pub(super) token: Option<String>,
}

/// Resolve the bridge token from request headers.
///
/// 1. `Authorization: Bearer <token>` — preferred, what generated SDKs use
/// 2. `Sec-WebSocket-Protocol: bearer, <token>` — browser WebSocket clients
///    (browsers cannot set `Authorization` on WS upgrades)
/// 3. `x-mxr-bridge-token: <token>` — v0.4.x compat, kept for the v0.5
///    cycle while older local clients migrate
pub(crate) fn extract_token(headers: &HeaderMap) -> Option<&str> {
    if let Some(value) = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            v.strip_prefix("Bearer ")
                .or_else(|| v.strip_prefix("bearer "))
        })
    {
        return Some(value);
    }
    if let Some(value) = headers
        .get("sec-websocket-protocol")
        .and_then(|v| v.to_str().ok())
    {
        // browsers join multiple subprotocols with `, ` — e.g.
        // `bearer, abc123`. The token is the trailing item after the
        // `bearer` marker.
        let mut parts = value.split(',').map(str::trim);
        if parts.clone().any(|p| p == "bearer") {
            if let Some(candidate) = parts.find(|p| !p.is_empty() && *p != "bearer") {
                return Some(candidate);
            }
        }
    }
    headers
        .get("x-mxr-bridge-token")
        .and_then(|value| value.to_str().ok())
}

pub(super) fn ensure_authorized(
    headers: &HeaderMap,
    _query_token: Option<&str>,
    expected_token: &str,
) -> Result<(), BridgeError> {
    // Handlers still deserialize `token` for compatibility with older query
    // shapes, but regular HTTP routes must not authenticate from it.
    if extract_token(headers) == Some(expected_token) {
        return Ok(());
    }
    Err(BridgeError::Unauthorized)
}

pub(super) fn ensure_authorized_with_query_token(
    headers: &HeaderMap,
    query_token: Option<&str>,
    expected_token: &str,
) -> Result<(), BridgeError> {
    if extract_token(headers).or(query_token) == Some(expected_token) {
        return Ok(());
    }
    Err(BridgeError::Unauthorized)
}

pub(super) fn next_bridge_request_id() -> u64 {
    NEXT_BRIDGE_REQUEST_ID.fetch_add(1, Ordering::Relaxed)
}

pub(super) fn bridge_request_id(headers: &HeaderMap) -> u64 {
    headers
        .get("x-mxr-request-id")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or_else(next_bridge_request_id)
}

pub(super) fn bridge_error_kind(error: &BridgeError) -> &'static str {
    match error {
        BridgeError::Connect(_) => "connect",
        BridgeError::Ipc(_) => "ipc",
        BridgeError::Unauthorized => "unauthorized",
        BridgeError::UnexpectedResponse => "unexpected_response",
    }
}

pub(super) fn bridge_error_is_missing_file(error: &BridgeError) -> bool {
    matches!(
        error,
        BridgeError::Ipc(message) if message.contains("No such file or directory (os error 2)")
    )
}

pub(super) fn account_sync_kind(account: &mxr_protocol::AccountConfigData) -> &'static str {
    match account.sync {
        Some(mxr_protocol::AccountSyncConfigData::Gmail { .. }) => "gmail",
        Some(mxr_protocol::AccountSyncConfigData::Imap { .. }) => "imap",
        Some(mxr_protocol::AccountSyncConfigData::OutlookPersonal { .. }) => "outlook",
        Some(mxr_protocol::AccountSyncConfigData::OutlookWork { .. }) => "outlook-work",
        Some(mxr_protocol::AccountSyncConfigData::Fake) => "fake",
        None => "none",
    }
}

pub(super) fn account_send_kind(account: &mxr_protocol::AccountConfigData) -> &'static str {
    match account.send {
        Some(mxr_protocol::AccountSendConfigData::Gmail) => "gmail",
        Some(mxr_protocol::AccountSendConfigData::Smtp { .. }) => "smtp",
        Some(mxr_protocol::AccountSendConfigData::OutlookPersonal { .. }) => "outlook",
        Some(mxr_protocol::AccountSendConfigData::OutlookWork { .. }) => "outlook-work",
        Some(mxr_protocol::AccountSendConfigData::Fake) => "fake",
        None => "none",
    }
}

pub(super) fn account_has_inline_imap_password(account: &mxr_protocol::AccountConfigData) -> bool {
    matches!(
        account.sync,
        Some(mxr_protocol::AccountSyncConfigData::Imap {
            password: Some(ref password),
            ..
        }) if !password.is_empty()
    )
}

pub(super) fn account_has_inline_smtp_password(account: &mxr_protocol::AccountConfigData) -> bool {
    matches!(
        account.send,
        Some(mxr_protocol::AccountSendConfigData::Smtp {
            password: Some(ref password),
            ..
        }) if !password.is_empty()
    )
}
