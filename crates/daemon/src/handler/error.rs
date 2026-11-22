use mxr_core::error::MxrError;
use mxr_protocol::{IpcErrorKind, Response};

#[derive(Debug, thiserror::Error)]
pub(crate) enum HandlerError {
    #[error("{0}")]
    Message(String),
    #[error("{0}")]
    InvalidRequest(String),
    #[error("{0}")]
    SearchIndexUnavailable(String),
    #[error("Classification pending: this email is still sorting")]
    ClassificationPending,
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
    SendOutcome {
        outcome: Box<mxr_core::DraftSendOutcome>,
        diagnostic: Option<String>,
    },
    #[error("Remote operation was refused")]
    RemoteResponse(Box<Response>),
    #[error("{message}")]
    UnsubscribeAttempt {
        message: String,
        code: &'static str,
        kind: IpcErrorKind,
        retryable: bool,
    },
    #[error(transparent)]
    Store(#[from] sqlx::Error),
    #[error(transparent)]
    Core(#[from] MxrError),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}

impl HandlerError {
    pub(crate) fn into_response(self) -> Response {
        match self {
            Self::ClassificationPending => Response::Error {
                message: self.to_string(), kind: IpcErrorKind::InvalidRequest,
                code: "classification_pending".into(), retryable: true, details: None,
            },
            Self::RemoteResponse(response) => *response,
            Self::PortableDraft => Response::Error {
                message: self.to_string(), kind: IpcErrorKind::InvalidRequest,
                code: "remote_path_backed_draft".into(), retryable: false, details: None,
            },
            Self::SendOutcome { outcome, diagnostic } => Response::Error {
                message: {
                    let guidance = match outcome.state {
                        mxr_core::SendOutcomeState::Unknown => "Send acceptance is uncertain; query the original operation and do not resend",
                        _ => "Provider did not accept this operation; review before starting a new send operation",
                    };
                    diagnostic.map_or_else(|| guidance.into(), |error| format!("{guidance}: {error}"))
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
            Self::SearchIndexUnavailable(message) => Response::Error {
                message,
                kind: IpcErrorKind::Unsupported,
                code: "search_index_unavailable".into(),
                retryable: false,
                details: None,
            },
            Self::InvalidRequest(message) => {
                Response::error_kinded(message, IpcErrorKind::InvalidRequest)
            }
            Self::Core(error @ MxrError::ProviderAuth(_)) => {
                Response::error_kinded(error.to_string(), IpcErrorKind::Auth)
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
    fn rejected_credentials_reach_the_wire_as_auth_with_the_same_message() {
        let response =
            HandlerError::Core(MxrError::ProviderAuth("oauth token revoked".to_string()))
                .into_response();
        assert!(matches!(
            &response,
            Response::Error { message, kind: IpcErrorKind::Auth, code, retryable: false, .. }
                if message == "Provider error: oauth token revoked" && code == "auth"
        ));
    }

    #[test]
    fn display_is_wire_identical_to_the_source_string() {
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
}
