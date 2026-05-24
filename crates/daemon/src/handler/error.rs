//! Internal error type for IPC request handlers.
//!
//! The wire contract is unchanged. Handler errors still serialise to the same
//! `String` they always did: every variant's `Display` renders exactly what
//! `e.to_string()` produced before, so swapping `Result<_, String>` for
//! `Result<_, HandlerError>` is behaviour-preserving (see the `Display` test).
//!
//! The win is dropping the `.map_err(|e| e.to_string())` noise that wrapped
//! nearly every fallible call. Store (`sqlx`) and provider/sync (`MxrError`)
//! failures now flow through `?` via `#[from]`; ad-hoc validation messages go
//! through `HandlerError::Message`. At the IPC boundary the error is turned
//! back into a `String` via `From<HandlerError>` for `Response::error`.

use mxr_core::error::MxrError;

#[derive(Debug, thiserror::Error)]
pub(crate) enum HandlerError {
    /// An explicit, handler-authored message (validation, "not found", etc.).
    #[error("{0}")]
    Message(String),
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
