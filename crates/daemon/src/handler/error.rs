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
    #[error("{message}")]
    Archive {
        message: String,
        code: &'static str,
        identity: Box<mxr_protocol::ArchiveIdentityData>,
        kind: IpcErrorKind,
        state: &'static str,
    },
    #[error("{0}")]
    RemoteAuthorityDenied(String),
    #[error("Path-backed drafts cannot be used remotely; recreate this draft using portable attachment uploads on the client")]
    PortableDraft,
    #[error("Send outcome requires recovery; query the original operation before another attempt")]
    SendOutcome(Box<mxr_core::DraftSendOutcome>),
    /// Already sanitized at authenticated remote admission.
    #[error("Remote operation was refused")]
    RemoteResponse(Box<Response>),
    /// A failed unsubscribe attempt. `retryable` is set here, not inferred from
    /// the message: a retry of an unsent attempt may go out, while a retry of
    /// an attempt whose outcome is unknown or already recorded as sent would be
    /// acknowledged without sending.
    #[error("{message}")]
    UnsubscribeAttempt {
        message: String,
        code: &'static str,
        kind: IpcErrorKind,
        retryable: bool,
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
            Self::RemoteResponse(response) => *response,
            Self::PortableDraft => Response::Error {
                message: self.to_string(), kind: IpcErrorKind::InvalidRequest,
                code: "remote_path_backed_draft".into(), retryable: false, details: None,
            },
            Self::SendOutcome(outcome) => Response::Error {
                message: match outcome.state {
                    mxr_core::SendOutcomeState::Unknown => "Send acceptance is uncertain; query the original operation and do not resend".into(),
                    _ => "Provider did not accept this operation; review before starting a new send operation".into(),
                },
                kind: IpcErrorKind::InvalidRequest,
                code: match outcome.state { mxr_core::SendOutcomeState::Unknown => "send_outcome_unknown", _ => "send_not_accepted" }.into(),
                retryable: false,
                details: Some(serde_json::json!({"outcome": outcome})),
            },
            Self::RemoteAuthorityDenied(message) => Response::Error {
                message, kind: IpcErrorKind::Auth, code: "remote_authority_denied".into(), retryable: false, details: None,
            },
            Self::DraftConflict { expected, current } => Response::Error {
                message: format!("Draft revision conflict: expected {expected:?}, current {current:?}; unsent edits were not saved"),
                kind: IpcErrorKind::InvalidRequest,
                code: "draft_revision_conflict".into(), retryable: false,
                details: Some(serde_json::json!({"expected_revision": expected, "current_revision": current})),
            },
            Self::Archive { message, code, identity, kind, state } => Response::Error {
                message, kind, code: code.into(), retryable: false,
                details: Some(serde_json::json!({"identity": identity, "state": state, "retry_guidance": "Query this identity; never create a new identity to retry an uncertain effect"})),
            },
            Self::UnsubscribeAttempt { message, code, kind, retryable } => Response::Error {
                message, kind, code: code.into(), retryable, details: None,
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
