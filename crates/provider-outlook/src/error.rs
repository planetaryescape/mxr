use thiserror::Error;

#[derive(Debug, Error)]
pub enum OutlookError {
    #[error("OAuth2 error: {0}")]
    OAuth2(String),

    #[error("Token expired or missing — re-run `mxr accounts add outlook`")]
    TokenExpired,

    #[error("Device code expired — please run the command again")]
    DeviceCodeExpired,

    #[error("Authorization declined by user")]
    AuthDeclined,

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
