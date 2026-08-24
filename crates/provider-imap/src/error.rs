#[derive(Debug, thiserror::Error)]
pub enum ImapProviderError {
    #[error("Authentication failed: {0}")]
    Auth(String),
    #[error("Connection error: {0}")]
    Connection(String),
    #[error("Keyring error: {0}")]
    Keyring(String),
    #[error("Protocol error: {0}")]
    Protocol(String),
    #[error("Invalid provider ID: {0}")]
    InvalidProviderId(String),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Fetch error: {0}")]
    Fetch(String),
    #[error(
        "Fetch error: IMAP server returned a response mxr could not parse. This looks like a server compatibility issue, not necessarily a bad username or password."
    )]
    MalformedFetchResponse,
    /// A command exceeded its deadline. Distinguished from `Connection`
    /// because a timeout leaves the connection in an unknown protocol
    /// state: callers that tolerate other errors by issuing further
    /// commands on the same session (e.g. the QRESYNC→SELECT fallback)
    /// must NOT do so after a timeout.
    #[error("Timeout: {0}")]
    Timeout(String),
    #[error("UIDVALIDITY changed (was {old}, now {new}) — requires full resync")]
    UidValidityChanged { old: u32, new: u32 },
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl ImapProviderError {
    pub(crate) fn protocol_detail(detail: impl Into<String>) -> Self {
        Self::Protocol(sanitize_imap_detail(&detail.into()))
    }

    pub(crate) fn fetch_detail(detail: impl Into<String>) -> Self {
        let detail = detail.into();
        if is_imap_parse_detail(&detail) {
            Self::MalformedFetchResponse
        } else {
            Self::Fetch(sanitize_imap_detail(&detail))
        }
    }

    /// True when the underlying connection may be mid-response and must
    /// not be reused for further commands.
    pub fn is_timeout(&self) -> bool {
        matches!(self, Self::Timeout(_))
    }

    /// True when a FETCH response could not be framed by the IMAP parser.
    /// The connection is desynchronised and must be discarded before retrying.
    pub fn is_malformed_fetch_response(&self) -> bool {
        matches!(self, Self::MalformedFetchResponse)
    }
}

fn sanitize_imap_detail(detail: &str) -> String {
    let lower = detail.to_ascii_lowercase();

    if lower.contains("during parsing of") && lower.contains("* namespace ") {
        return "IMAP server returned a NAMESPACE response in an unsupported format during folder discovery. This looks like a server compatibility issue, not necessarily a bad username or password.".into();
    }

    if lower.contains("during parsing of") || lower.contains("input: [") {
        return "IMAP server returned a response mxr could not parse. This looks like a server compatibility issue, not necessarily a bad username or password.".into();
    }

    detail.to_string()
}

fn is_imap_parse_detail(detail: &str) -> bool {
    let lower = detail.to_ascii_lowercase();
    lower.contains("during parsing of") || lower.contains("input: [")
}

impl From<ImapProviderError> for mxr_core::error::MxrError {
    fn from(e: ImapProviderError) -> Self {
        Self::Provider(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{sanitize_imap_detail, ImapProviderError};

    #[test]
    fn namespace_parse_errors_are_sanitized_for_users() {
        let detail = "io: Error(Error { input: [42, 32], code: TakeWhile1 }) during parsing of \"* NAMESPACE ((\\\"\\\" \\\"/\\\")) NIL NIL\\r\\nA0002 OK NAMESPACE completed\\r\\n\"";
        let sanitized = sanitize_imap_detail(detail);
        assert!(sanitized.contains("NAMESPACE response"));
        assert!(!sanitized.contains("input: [42, 32]"));
    }

    #[test]
    fn generic_parse_errors_hide_raw_buffer_dump() {
        let detail =
            "io: Error(Error { input: [1, 2, 3], code: Tag }) during parsing of \"* FOO ...\"";
        let sanitized = sanitize_imap_detail(detail);
        assert!(sanitized.contains("could not parse"));
        assert!(!sanitized.contains("[1, 2, 3]"));
    }

    #[test]
    fn fetch_parse_errors_are_distinguishable_for_connection_recovery() {
        let error = ImapProviderError::fetch_detail(
            "io: Error(Error { input: [1, 2, 3], code: Tag }) during parsing of fetch",
        );

        assert!(error.is_malformed_fetch_response());
        assert!(!error.to_string().contains("[1, 2, 3]"));
    }
}
