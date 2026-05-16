use thiserror::Error;

/// Errors returned by [`parse`][crate::parse] and
/// [`parse_with`][crate::parse_with].
#[derive(Debug, Error, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParseError {
    #[error("unexpected end of input")]
    UnexpectedEnd,
    #[error("unexpected token: {0}")]
    UnexpectedToken(String),
    #[error("unmatched parenthesis")]
    UnmatchedParen,
    #[error("unmatched brace")]
    UnmatchedBrace,
    #[error("expected value after field")]
    ExpectedValue,
    /// A filter operator (`is:foo`, `has:bar`, `in:baz`) was used that
    /// is neither in the built-in set nor registered via
    /// [`ParserOptions::custom_filters`][crate::ParserOptions].
    #[error("unknown filter: {0}")]
    UnknownFilter(String),
    #[error("invalid size: {0}")]
    InvalidSize(String),
    #[error("invalid date: {0}")]
    InvalidDate(String),
}
