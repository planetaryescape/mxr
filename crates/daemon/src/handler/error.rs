//! Internal error type for IPC request handlers.
//!
//! Store and provider failures keep their existing display messages. Draft
//! revision conflicts additionally carry a machine-readable code and the
//! expected/current revision so every client can preserve unsent edits.

use mxr_core::error::MxrError;
use mxr_protocol::{IpcErrorKind, Response};

#[derive(Debug, thiserror::Error)]
pub(crate) enum HandlerError {
    /// An explicit, handler-authored message (validation, "not found", etc.).
    #[error("{0}")]
    Message(String),
    /// An explicit invalid-request validation failure. Carries the wire kind
    /// so it is classified as `IpcErrorKind::InvalidRequest` instead of being
    /// string-sniffed into `Internal`.
    #[error("{0}")]
    InvalidRequest(String),
    #[error("Draft revision conflict: expected {expected:?}, current {current:?}; unsent edits were not saved")]
    DraftConflict {
        expected: Option<i64>,
        current: Option<i64>,
    },
    /// A storage-layer failure. `sqlx::Error` is what the store returns today.
    #[error(transparent)]
    Store(#[from] sqlx::Error),
    /// A provider/sync/core failure carried as the shared `MxrError`.
    #[error(transparent)]
    Core(#[from] MxrError),
    /// JSON (de)serialisation failure from a handler that builds/parses JSON.
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}

impl HandlerError {
    /// Convert to a wire `Response`, preserving the explicit `IpcErrorKind` for
    /// variants that carry one so kinded errors don't fall back to the
    /// substring classifier in `Response::error`.
    pub(crate) fn into_response(self) -> Response {
        match self {
            Self::DraftConflict { expected, current } => Response::Error {
                message: format!("Draft revision conflict: expected {expected:?}, current {current:?}; unsent edits were not saved"),
                kind: IpcErrorKind::InvalidRequest,
                code: "draft_revision_conflict".into(), retryable: false,
                details: Some(serde_json::json!({"expected_revision": expected, "current_revision": current})),
            },
            Self::InvalidRequest(message) => {
                Response::error_kinded(message, IpcErrorKind::InvalidRequest)
            }
            other => Response::error(other),
        }
    }
}

impl From<String> for HandlerError {
    fn from(message: String) -> Self {
        Self::Message(message)
    }
}

impl From<&str> for HandlerError {
    fn from(message: &str) -> Self {
        Self::Message(message.to_string())
    }
}

impl From<HandlerError> for String {
    fn from(error: HandlerError) -> Self {
        error.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_is_wire_identical_to_the_source_string() {
        // The migration rests on this: a HandlerError must stringify to exactly
        // what `.map_err(|e| e.to_string())` produced before. If a variant's
        // Display drifts from its source, the IPC error wire changes.
        assert_eq!(
            HandlerError::Message("boom".to_string()).to_string(),
            "boom"
        );
        assert_eq!(
            HandlerError::Store(sqlx::Error::RowNotFound).to_string(),
            sqlx::Error::RowNotFound.to_string()
        );
        assert_eq!(
            HandlerError::Core(MxrError::Provider("nope".to_string())).to_string(),
            MxrError::Provider("nope".to_string()).to_string()
        );
    }

    #[test]
    fn string_conversions_round_trip() {
        let from_owned: HandlerError = "x".to_string().into();
        assert!(matches!(from_owned, HandlerError::Message(m) if m == "x"));
        let from_borrowed: HandlerError = "y".into();
        assert!(matches!(from_borrowed, HandlerError::Message(m) if m == "y"));
        assert_eq!(String::from(HandlerError::Message("z".to_string())), "z");
    }
}
