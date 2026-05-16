//! `Display` impl for `QueryNode` — round-trips an AST back to a
//! parseable query string.
//!
//! The contract is *structural* round-trip:
//! `parse(node.to_string())? == node`, not byte-identical. We normalise
//! whitespace, prefer explicit `AND`/`OR` keywords, and parenthesise
//! every compound child to preserve precedence.

use std::fmt;

use crate::ast::{DateBound, DateValue, FilterKind, QueryField, QueryNode, RelativeUnit, SizeOp};

impl fmt::Display for QueryNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Text(s) => write_term(f, s),
            Self::Exact(s) => {
                f.write_str("+")?;
                f.write_str(s)
            }
            Self::Phrase(s) => write_quoted(f, s),
            Self::Field { field, value } => {
                write!(f, "{}:", field_name(*field))?;
                write_value(f, value)
            }
            Self::Filter(kind) => write_filter(f, kind),
            Self::Label(name) => {
                f.write_str("label:")?;
                write_value(f, name)
            }
            Self::DateRange { bound, date } => write_date(f, *bound, date),
            Self::Size { op, bytes } => write_size(f, *op, *bytes),
            Self::Near {
                left,
                right,
                distance,
            } => write!(f, "\"{left} AROUND {distance} {right}\""),
            Self::And(left, right) => {
                write_compound(f, left)?;
                f.write_str(" AND ")?;
                write_compound(f, right)
            }
            Self::Or(left, right) => {
                write_compound(f, left)?;
                f.write_str(" OR ")?;
                write_compound(f, right)
            }
            Self::Not(inner) => {
                f.write_str("-")?;
                write_compound(f, inner)
            }
        }
    }
}

/// Wrap `And`/`Or`/`Not` children in parens so re-parsing preserves
/// precedence. Leaves render directly.
fn write_compound(f: &mut fmt::Formatter<'_>, node: &QueryNode) -> fmt::Result {
    match node {
        QueryNode::And(..) | QueryNode::Or(..) | QueryNode::Not(..) => {
            write!(f, "({node})")
        }
        other => write!(f, "{other}"),
    }
}

fn write_term(f: &mut fmt::Formatter<'_>, s: &str) -> fmt::Result {
    if needs_quoting(s) {
        write_quoted(f, s)
    } else {
        f.write_str(s)
    }
}

fn write_value(f: &mut fmt::Formatter<'_>, s: &str) -> fmt::Result {
    if needs_quoting(s) {
        write_quoted(f, s)
    } else {
        f.write_str(s)
    }
}

fn needs_quoting(s: &str) -> bool {
    s.is_empty()
        || s.chars().any(|c| {
            c.is_whitespace() || matches!(c, '"' | '(' | ')' | '{' | '}' | ':')
        })
}

fn write_quoted(f: &mut fmt::Formatter<'_>, s: &str) -> fmt::Result {
    f.write_str("\"")?;
    for c in s.chars() {
        if c == '"' {
            f.write_str("\\\"")?;
        } else {
            f.write_str(&c.to_string())?;
        }
    }
    f.write_str("\"")
}

fn field_name(field: QueryField) -> &'static str {
    match field {
        QueryField::From => "from",
        QueryField::To => "to",
        QueryField::Cc => "cc",
        QueryField::Bcc => "bcc",
        QueryField::Subject => "subject",
        QueryField::Body => "body",
        QueryField::Filename => "filename",
        QueryField::List => "list",
        QueryField::DeliveredTo => "deliveredto",
        QueryField::Rfc822MsgId => "rfc822msgid",
    }
}

fn write_filter(f: &mut fmt::Formatter<'_>, kind: &FilterKind) -> fmt::Result {
    match kind {
        FilterKind::Unread => f.write_str("is:unread"),
        FilterKind::Read => f.write_str("is:read"),
        FilterKind::Starred => f.write_str("is:starred"),
        FilterKind::Draft => f.write_str("is:draft"),
        FilterKind::Sent => f.write_str("is:sent"),
        FilterKind::Trash => f.write_str("is:trash"),
        FilterKind::Spam => f.write_str("is:spam"),
        FilterKind::Answered => f.write_str("is:answered"),
        FilterKind::Inbox => f.write_str("is:inbox"),
        FilterKind::Archived => f.write_str("is:archived"),
        FilterKind::Anywhere => f.write_str("in:anywhere"),
        FilterKind::HasAttachment => f.write_str("has:attachment"),
        FilterKind::HasCalendar => f.write_str("has:calendar"),
        FilterKind::HasUserLabels => f.write_str("has:userlabels"),
        FilterKind::NoUserLabels => f.write_str("has:nouserlabels"),
        FilterKind::HasDrive => f.write_str("has:drive"),
        FilterKind::HasDocument => f.write_str("has:document"),
        FilterKind::HasSpreadsheet => f.write_str("has:spreadsheet"),
        FilterKind::HasPresentation => f.write_str("has:presentation"),
        FilterKind::HasYoutube => f.write_str("has:youtube"),
        FilterKind::HasInlineImage => f.write_str("has:inline"),
        FilterKind::HasLink => f.write_str("has:link"),
        FilterKind::HasLinkHeavy => f.write_str("has:link-heavy"),
        FilterKind::NoLinks => f.write_str("has:link-none"),
        FilterKind::Custom(name) => write!(f, "is:{name}"),
    }
}

fn write_date(f: &mut fmt::Formatter<'_>, bound: DateBound, date: &DateValue) -> fmt::Result {
    // Relative durations have their own operator (`older_than:` /
    // `newer_than:`); other date values pair with `after:`/`before:`/`date:`.
    match (bound, date) {
        (DateBound::Before, DateValue::Relative { amount, unit }) => {
            write!(f, "older_than:{amount}{}", unit_suffix(*unit))
        }
        (DateBound::After, DateValue::Relative { amount, unit }) => {
            write!(f, "newer_than:{amount}{}", unit_suffix(*unit))
        }
        _ => {
            let prefix = match bound {
                DateBound::After => "after",
                DateBound::Before => "before",
                DateBound::Exact => "date",
            };
            f.write_str(prefix)?;
            f.write_str(":")?;
            write_date_value(f, date)
        }
    }
}

fn write_date_value(f: &mut fmt::Formatter<'_>, date: &DateValue) -> fmt::Result {
    match date {
        DateValue::Specific(d) => write!(f, "{}", d.format("%Y-%m-%d")),
        DateValue::Today => f.write_str("today"),
        DateValue::Yesterday => f.write_str("yesterday"),
        DateValue::ThisWeek => f.write_str("this-week"),
        DateValue::ThisMonth => f.write_str("this-month"),
        // Unreachable in practice — write_date handles Relative above.
        DateValue::Relative { amount, unit } => {
            write!(f, "{amount}{}", unit_suffix(*unit))
        }
    }
}

fn unit_suffix(unit: RelativeUnit) -> &'static str {
    match unit {
        RelativeUnit::Day => "d",
        RelativeUnit::Week => "w",
        RelativeUnit::Month => "m",
        RelativeUnit::Year => "y",
    }
}

fn write_size(f: &mut fmt::Formatter<'_>, op: SizeOp, bytes: u64) -> fmt::Result {
    let op_str = match op {
        SizeOp::LessThan => "<",
        SizeOp::LessThanOrEqual => "<=",
        SizeOp::Equal => "=",
        SizeOp::GreaterThan => ">",
        SizeOp::GreaterThanOrEqual => ">=",
    };
    // Render in raw bytes; the parser accepts a bare integer.
    write!(f, "size:{op_str}{bytes}")
}
