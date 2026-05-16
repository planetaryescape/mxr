//! Abstract syntax tree for parsed Gmail-style email queries.
//!
//! All public enums are `#[non_exhaustive]` so the crate can add new
//! variants (for new Gmail operators) without breaking downstream
//! pattern-matching. Callers must include a `_ => ...` arm.

use chrono::NaiveDate;

/// Root AST node for a parsed query.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum QueryNode {
    /// A bare term. Subject to the backend's tokenizer/stemmer.
    Text(String),

    /// `+word` — exact-match form, the backend should disable stemming
    /// for this term. New in v0.1.0; mirrors Gmail's `+word` semantics.
    Exact(String),

    /// `"quoted phrase"` — multi-word phrase.
    Phrase(String),

    /// `field:value` — e.g. `from:alice`, `subject:invoice`.
    Field { field: QueryField, value: String },

    /// `is:unread`, `has:attachment`, etc. See [`FilterKind`].
    Filter(FilterKind),

    /// `label:work`, `category:promotions` (categories normalise to
    /// canonical `CATEGORY_*` labels).
    Label(String),

    /// `after:2024-01-01`, `older_than:5d`, `date:today`, etc. The
    /// `Relative` variant deliberately is *not* resolved to a concrete
    /// date at parse time — backends call
    /// [`ParserOptions::now_provider`][crate::ParserOptions] to evaluate
    /// it. See the README for the rationale.
    DateRange { bound: DateBound, date: DateValue },

    /// `size:>5M`, `larger:200K`, etc.
    Size { op: SizeOp, bytes: u64 },

    /// `foo AROUND 3 bar` — word proximity.
    Near {
        left: String,
        right: String,
        distance: u32,
    },

    /// Conjunction. `parse` builds left-associative trees.
    And(Box<QueryNode>, Box<QueryNode>),

    /// Disjunction. Left-associative.
    Or(Box<QueryNode>, Box<QueryNode>),

    /// `-foo` or `NOT foo`.
    Not(Box<QueryNode>),
}

/// Built-in `field:` names. New Gmail field operators will land as
/// additional variants here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum QueryField {
    From,
    To,
    Cc,
    Bcc,
    Subject,
    Body,
    Filename,
    List,
    DeliveredTo,
    Rfc822MsgId,
}

/// `is:` and `has:` filter values.
///
/// The closed set covers Gmail-documented operators. Operators that
/// Gmail adds over time, color-star variants beyond the common set, and
/// caller-specific filters (e.g. application-defined `is:owed-reply`)
/// land in [`FilterKind::Custom`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum FilterKind {
    // `is:` family
    Unread,
    Read,
    Starred,
    Draft,
    Sent,
    Trash,
    Spam,
    Answered,
    Inbox,
    Archived,
    /// `in:anywhere` / `in:all` — search every folder, including spam and trash.
    Anywhere,

    // `has:` family
    HasAttachment,
    HasCalendar,
    HasUserLabels,
    NoUserLabels,
    HasDrive,
    HasDocument,
    HasSpreadsheet,
    HasPresentation,
    HasYoutube,
    HasInlineImage,
    HasLink,
    HasLinkHeavy,
    NoLinks,

    /// Escape hatch for filters not in the closed set. The carried
    /// string is the operator value as parsed (lowercased, hyphenated
    /// canonical form). Examples:
    /// - Gmail's `has:reaction` → `Custom("reaction")`
    /// - Color-star variants → `Custom("yellow-star")` etc., when the
    ///   caller has registered them via
    ///   [`ParserOptions::custom_filters`][crate::ParserOptions].
    /// - Application-defined filters: `Custom("owed-reply")`,
    ///   `Custom("reply-later")`, etc.
    Custom(String),
}

/// Date bound for [`QueryNode::DateRange`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum DateBound {
    /// `after:`, `newer:`, `newer_than:`.
    After,
    /// `before:`, `older:`, `older_than:`.
    Before,
    /// `date:`.
    Exact,
}

/// Date value for [`QueryNode::DateRange`].
///
/// The parser does *not* resolve `Relative` against a concrete `now` —
/// that's deliberate. A query parsed today and serialised back via
/// [`Display`][std::fmt::Display] must mean the same thing tomorrow.
/// Backends resolve `Relative` against
/// [`ParserOptions::now_provider`][crate::ParserOptions] when building
/// an executable query.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DateValue {
    Specific(NaiveDate),
    Today,
    Yesterday,
    ThisWeek,
    ThisMonth,
    /// `older_than:5d`, `newer_than:2w`, etc. — a duration relative to
    /// "now". Resolution happens at query-execution time.
    Relative { amount: u32, unit: RelativeUnit },
}

/// Time unit for [`DateValue::Relative`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum RelativeUnit {
    Day,
    Week,
    Month,
    Year,
}

/// Comparison operator for [`QueryNode::Size`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum SizeOp {
    LessThan,
    LessThanOrEqual,
    Equal,
    GreaterThan,
    GreaterThanOrEqual,
}
