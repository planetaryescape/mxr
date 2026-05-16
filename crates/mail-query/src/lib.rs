//! Parser and typed AST for Gmail-style email search queries.
//!
//! ```
//! use mail_query::{parse, FilterKind, QueryField, QueryNode};
//!
//! let ast = parse("from:alice is:unread -has:attachment").expect("parses");
//! match ast {
//!     QueryNode::And(_, _) => {} // top-level is a conjunction
//!     other => panic!("expected And, got {other:?}"),
//! }
//! ```
//!
//! The crate covers Gmail's documented operator surface
//! (<https://support.google.com/mail/answer/7190>): address fields
//! (`from:`, `to:`, `cc:`, `bcc:`, `deliveredto:`, `rfc822msgid:`,
//! `list:`), content fields (`subject:`, `body:`, `filename:`), `is:`
//! and `has:` filters, `label:` and `category:`, size and date operators
//! with relative durations (`older_than:5d`), boolean operators (`AND`,
//! `OR`, `NOT`, `-`), and proximity (`AROUND<n>`). It also recognises
//! `+word` as an exact-match (no-stemming) hint.
//!
//! # What this crate does *not* do
//!
//! - It does not execute queries. The output is a portable
//!   [`QueryNode`]; backends translate it to their own query language
//!   (tantivy, meilisearch, SQL FTS, IMAP SEARCH, …).
//! - It does not resolve `older_than:5d` to a concrete date at parse
//!   time. Backends do that when building an executable query, using
//!   their own `now`. This is what lets a saved query mean the same
//!   thing tomorrow as today and lets the AST round-trip through
//!   [`Display`][std::fmt::Display] without embedding a date.
//! - It does not implement IMAP SEARCH grammar (RFC 3501 §6.4.4) — that
//!   is a separate, future crate. The vocabularies overlap but the
//!   grammars do not.
//!
//! # Extensibility
//!
//! Filter names that Gmail adds over time, or your application's own
//! `is:owed-reply`, route through [`FilterKind::Custom`]. Register the
//! names you want to accept via [`ParserOptions::register_custom_filter`]
//! before calling [`parse_with`].
//!
//! ```
//! use mail_query::{parse_with, FilterKind, ParserOptions, QueryNode};
//!
//! let mut options = ParserOptions::new();
//! options.register_custom_filter("owed-reply");
//!
//! let ast = parse_with("is:owed-reply", &options).expect("parses");
//! assert_eq!(
//!     ast,
//!     QueryNode::Filter(FilterKind::Custom("owed-reply".into()))
//! );
//! ```
//!
//! # Feature flags
//!
//! - `serde` — adds `Serialize`/`Deserialize` derives to every AST type.
//!
//! # Forward compatibility
//!
//! Every public enum is `#[non_exhaustive]`. New variants (for new Gmail
//! operators) are non-breaking additions. Pattern-matching callers must
//! include a `_ => …` arm.

#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(unsafe_code, unused_must_use)]

mod ast;
mod display;
mod error;
mod options;
mod parser;
mod visitor;

pub use ast::{
    DateBound, DateValue, FilterKind, QueryField, QueryNode, RelativeUnit, SizeOp,
};
pub use error::ParseError;
pub use options::ParserOptions;
pub use parser::{parse, parse_with};
pub use visitor::Visitor;
