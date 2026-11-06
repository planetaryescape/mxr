use mxr_core::MxrError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GmailError {
    #[error("Mail request was not submitted during {phase:?}: {source}")]
    NotSubmitted {
        phase: mxr_core::SendSubmissionPhase,
        source: Box<Self>,
    },
    #[error("Authentication expired — re-auth required")]
    AuthExpired,

    #[error("Not found: {0}")]
    NotFound(String),

    /// `detail` names the refused endpoint and Gmail's reason, when Gmail said
    /// anything usable. Shown to the user through `MxrError::RateLimited`.
    #[error("Rate limited — retry after {retry_after_secs}s")]
    RateLimited {
        retry_after_secs: u64,
        detail: Option<String>,
    },

    #[error("Gmail API error (HTTP {status}): {body}")]
    Api { status: u16, body: String },

    #[error(transparent)]
    Http(#[from] reqwest::Error),

    #[error("Auth error: {0}")]
    Auth(String),

    #[error("Token refresh temporarily unavailable")]
    AuthTemporary,

    #[error("Parse error: {0}")]
    Parse(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<GmailError> for MxrError {
    fn from(e: GmailError) -> Self {
        match e {
            GmailError::NotSubmitted { source, .. } => Self::from(*source),
            GmailError::NotFound(msg) => Self::NotFound(msg),
            GmailError::RateLimited {
                retry_after_secs,
                detail,
            } => Self::RateLimited {
                retry_after_secs,
                detail,
            },
            error @ (GmailError::AuthExpired | GmailError::Auth(_)) => {
                Self::ProviderAuth(error.to_string())
            }
            other => Self::Provider(other.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_reaches_the_sync_error_text_with_its_reason_and_retry() {
        let mxr: MxrError = GmailError::RateLimited {
            retry_after_secs: 1234,
            detail: Some(
                "Gmail messages.get (rateLimitExceeded: User-rate limit exceeded; 412 requests in the last hour)"
                    .into(),
            ),
        }
        .into();

        assert_eq!(
            mxr.to_string(),
            "Rate limited by Gmail messages.get (rateLimitExceeded: User-rate limit exceeded; 412 requests in the last hour) — retry after 1234s"
        );
        assert!(matches!(
            mxr,
            MxrError::RateLimited {
                retry_after_secs: 1234,
                ..
            }
        ));
    }

    #[test]
    fn rejected_credentials_are_typed_as_auth_with_the_same_text() {
        let mxr: MxrError = GmailError::AuthExpired.into();
        assert_eq!(
            mxr.to_string(),
            "Provider error: Authentication expired — re-auth required"
        );
        assert!(matches!(mxr, MxrError::ProviderAuth(_)));

        let mxr: MxrError = GmailError::Auth("OAuth2 invalid_grant".into()).into();
        assert_eq!(
            mxr.to_string(),
            "Provider error: Auth error: OAuth2 invalid_grant"
        );
        assert!(matches!(mxr, MxrError::ProviderAuth(_)));
    }

    #[test]
    fn token_cache_failures_stay_untyped_provider_errors() {
        let mxr: MxrError = GmailError::AuthTemporary.into();
        assert!(matches!(mxr, MxrError::Provider(_)));
    }

    #[test]
    fn rate_limit_without_detail_keeps_the_plain_text() {
        let mxr: MxrError = GmailError::RateLimited {
            retry_after_secs: 60,
            detail: None,
        }
        .into();

        assert_eq!(mxr.to_string(), "Rate limited — retry after 60s");
    }
}
