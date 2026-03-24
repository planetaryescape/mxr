use crate::mxr_core::MxrError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GmailError {
    #[error("Authentication expired — re-auth required")]
    AuthExpired,

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Rate limited — retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },

    #[error("Gmail API error (HTTP {status}): {body}")]
    Api { status: u16, body: String },

    #[error(transparent)]
    Http(#[from] reqwest::Error),

    #[error("Auth error: {0}")]
    Auth(String),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<GmailError> for MxrError {
    fn from(e: GmailError) -> Self {
        match e {
            GmailError::NotFound(msg) => MxrError::NotFound(msg),
            other => MxrError::Provider(other.to_string()),
        }
    }
}
