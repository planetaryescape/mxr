use super::*;

#[derive(Debug, Clone)]
pub struct WebServerConfig {
    pub socket_path: PathBuf,
    pub auth_token: String,
}

impl WebServerConfig {
    pub fn new(socket_path: PathBuf, auth_token: String) -> Self {
        Self {
            socket_path,
            auth_token,
        }
    }
}

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) config: WebServerConfig,
}

impl AppState {
    pub(crate) fn new(config: WebServerConfig) -> Self {
        Self { config }
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum BridgeError {
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
pub(crate) struct AuthQuery {
    #[serde(default)]
    pub(crate) token: Option<String>,
}

pub(crate) fn ensure_authorized(
    headers: &HeaderMap,
    query_token: Option<&str>,
    expected_token: &str,
) -> Result<(), BridgeError> {
    let header_token = headers
        .get("x-mxr-bridge-token")
        .and_then(|value| value.to_str().ok());
    let provided = header_token.or(query_token);
    if provided == Some(expected_token) {
        return Ok(());
    }
    Err(BridgeError::Unauthorized)
}
