use std::collections::HashSet;
use std::fmt;

use crate::parser::canonical_filter_name;

/// Caller-provided configuration for the parser.
///
/// # Custom filters
///
/// The built-in filter vocabulary (`is:unread`, `has:attachment`, etc.)
/// is fixed at compile time. Anything else — `is:owed-reply`,
/// `has:reaction`, `is:my-app-flag` — returns
/// [`ParseError::UnknownFilter`][crate::ParseError::UnknownFilter] by
/// default. Register the names you want to accept so they parse as
/// [`FilterKind::Custom`][crate::FilterKind::Custom] instead.
///
/// ```
/// use mail_query::{parse_with, FilterKind, ParserOptions, QueryNode};
///
/// let mut options = ParserOptions::default();
/// options.register_custom_filter("owed-reply");
///
/// let ast = parse_with("is:owed-reply", &options).expect("parses");
/// assert_eq!(
///     ast,
///     QueryNode::Filter(FilterKind::Custom("owed-reply".into()))
/// );
/// ```
#[derive(Default)]
pub struct ParserOptions {
    custom_filters: HashSet<String>,
}

impl fmt::Debug for ParserOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParserOptions")
            .field("custom_filters", &self.custom_filters)
            .finish()
    }
}

impl ParserOptions {
    /// New empty options. Equivalent to [`Default::default`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a custom filter name. The crate canonicalises to
    /// lowercase + hyphenated form (`Reply_Later` becomes `reply-later`),
    /// so callers can pass any casing.
    pub fn register_custom_filter(&mut self, name: impl Into<String>) -> &mut Self {
        let canonical = canonical_filter_name(&name.into());
        self.custom_filters.insert(canonical);
        self
    }

    /// Register many custom filter names at once.
    pub fn register_custom_filters<I, S>(&mut self, names: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for name in names {
            self.register_custom_filter(name);
        }
        self
    }

    /// True if `name` (already canonicalised) is registered.
    pub(crate) fn has_custom_filter(&self, canonical: &str) -> bool {
        self.custom_filters.contains(canonical)
    }
}
