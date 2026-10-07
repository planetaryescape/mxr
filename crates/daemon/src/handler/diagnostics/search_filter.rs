#![cfg_attr(
    test,
    expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )
)]

use chrono::Datelike;
use mxr_core::types::{system_labels, SemanticChunkSourceKind};
use mxr_search::ast::{DateBound, DateValue, FilterKind, QueryField, QueryNode, SizeOp};

/// Returns true if `node` (or any descendant) is an
/// `is:owed-reply` filter. Used by the search executor to decide
/// whether to pre-compute the owed-thread set and apply it as a
/// post-filter (the filter itself is computed across messages +
/// contacts + screener_decisions and so can't be expressed as a
/// per-envelope Tantivy field).
pub(super) fn ast_contains_owed_reply(node: &QueryNode) -> bool {
    match node {
        QueryNode::Filter(FilterKind::Custom(name)) if name == mxr_search::FILTER_OWED_REPLY => {
            true
        }
        QueryNode::And(left, right) | QueryNode::Or(left, right) => {
            ast_contains_owed_reply(left) || ast_contains_owed_reply(right)
        }
        QueryNode::Not(inner) => ast_contains_owed_reply(inner),
        _ => false,
    }
}

/// The two places search leaves out unless a query asks for them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hidden {
    Trash,
    Spam,
}

impl Hidden {
    fn label(self) -> &'static str {
        match self {
            Self::Trash => system_labels::TRASH,
            Self::Spam => system_labels::SPAM,
        }
    }

    fn filter(self) -> FilterKind {
        match self {
            Self::Trash => FilterKind::Trash,
            Self::Spam => FilterKind::Spam,
        }
    }

    /// `NOT (in:<place> OR label:<PLACE>)`: by flag and by label, since a
    /// label change from delta sync does not always set the flag.
    fn exclusion(self) -> QueryNode {
        QueryNode::Not(Box::new(QueryNode::Or(
            Box::new(QueryNode::Filter(self.filter())),
            Box::new(QueryNode::Label(self.label().to_string())),
        )))
    }
}

/// Whether `node` asks for `place` (`in:trash`, `in:anywhere`, the label)
/// outside a negation: `-in:trash` names Trash only to leave it out.
fn asks_for(node: &QueryNode, place: Hidden) -> bool {
    match node {
        QueryNode::Filter(FilterKind::Anywhere) => true,
        QueryNode::Filter(kind) => *kind == place.filter(),
        QueryNode::Label(label) => label.eq_ignore_ascii_case(place.label()),
        QueryNode::And(left, right) | QueryNode::Or(left, right) => {
            asks_for(left, place) || asks_for(right, place)
        }
        _ => false,
    }
}

/// Whether every alternative of `node` asks for `place`, without
/// expanding it: one side of an AND is enough, both sides of an OR are
/// needed.
fn asked_everywhere(node: &QueryNode, place: Hidden) -> bool {
    match node {
        QueryNode::And(left, right) => {
            asked_everywhere(left, place) || asked_everywhere(right, place)
        }
        QueryNode::Or(left, right) => {
            asked_everywhere(left, place) && asked_everywhere(right, place)
        }
        leaf => asks_for(leaf, place),
    }
}

/// Adds the exclusion of each place `node` does not ask for.
fn guarded(node: QueryNode) -> QueryNode {
    let lifted = [Hidden::Trash, Hidden::Spam].map(|place| asks_for(&node, place));
    [Hidden::Trash, Hidden::Spam]
        .into_iter()
        .zip(lifted)
        .filter(|(_, lifted)| !lifted)
        .fold(node, |node, (place, _)| {
            QueryNode::And(Box::new(node), Box::new(place.exclusion()))
        })
}

/// Most alternatives a query is expanded into before scoping falls back to
/// the whole query.
const MAX_SCOPED_ALTERNATIVES: usize = 32;

/// The query as alternatives (OR of ANDs), or `None` when that would exceed
/// `MAX_SCOPED_ALTERNATIVES`. Negations and leaves are kept whole.
fn alternatives(node: &QueryNode) -> Option<Vec<QueryNode>> {
    match node {
        QueryNode::Or(left, right) => {
            let mut all = alternatives(left)?;
            all.extend(alternatives(right)?);
            (all.len() <= MAX_SCOPED_ALTERNATIVES).then_some(all)
        }
        QueryNode::And(left, right) => {
            let lefts = alternatives(left)?;
            let rights = alternatives(right)?;
            if lefts.len().saturating_mul(rights.len()) > MAX_SCOPED_ALTERNATIVES {
                return None;
            }
            Some(
                lefts
                    .iter()
                    .flat_map(|l| {
                        rights
                            .iter()
                            .map(move |r| QueryNode::And(Box::new(l.clone()), Box::new(r.clone())))
                    })
                    .collect(),
            )
        }
        other => Some(vec![other.clone()]),
    }
}

/// Leaves Trash and Spam out of a query unless it asks for them, as Gmail
/// search does. One rewrite, so the lexical half, the dense filter, counts
/// and `--search` selections all apply the same default.
///
/// Each place is lifted on its own: `in:trash` lets Trash in, not Spam. And
/// the lift is scoped to the alternative that asks: in
/// `in:trash OR from:alice`, Alice's own Trash and Spam stay out. When the
/// alternatives all ask for the same places the query keeps its shape.
///
/// A query with more than `MAX_SCOPED_ALTERNATIVES` alternatives cannot be
/// scoped, so it lifts a place only when every alternative asks for it and
/// otherwise keeps the exclusion: lifting it for the whole query would let
/// in Trash or Spam the user did not ask for. `in:anywhere` beside the rest
/// of the query still reaches them.
pub(super) fn exclude_trash_and_spam_by_default(ast: QueryNode) -> QueryNode {
    let asks = |node: &QueryNode| (asks_for(node, Hidden::Trash), asks_for(node, Hidden::Spam));
    let Some(terms) = alternatives(&ast) else {
        // Lift a place only where every alternative asks for it.
        let lifted = [Hidden::Trash, Hidden::Spam].map(|place| asked_everywhere(&ast, place));
        return [Hidden::Trash, Hidden::Spam]
            .into_iter()
            .zip(lifted)
            .filter(|(_, lifted)| !lifted)
            .fold(ast, |node, (place, _)| {
                QueryNode::And(Box::new(node), Box::new(place.exclusion()))
            });
    };
    if terms.iter().any(|term| asks(term) != asks(&ast)) {
        terms
            .into_iter()
            .map(guarded)
            .reduce(|left, right| QueryNode::Or(Box::new(left), Box::new(right)))
            .unwrap_or(ast)
    } else {
        guarded(ast)
    }
}

#[derive(Debug, Clone)]
pub(super) struct SemanticQueryPlan {
    pub text: String,
    pub source_kinds: Vec<SemanticChunkSourceKind>,
}

pub(super) fn matches_structured_filters(node: &QueryNode, envelope: &mxr_core::Envelope) -> bool {
    match node {
        QueryNode::Text(_)
        | QueryNode::Exact(_)
        | QueryNode::Phrase(_)
        | QueryNode::Near { .. } => true,
        QueryNode::Field { field, value } => match field {
            QueryField::Subject
            | QueryField::Body
            | QueryField::Filename
            | QueryField::List
            | QueryField::DeliveredTo
            | QueryField::Rfc822MsgId => true,
            QueryField::From => {
                address_matches(&envelope.from.email, envelope.from.name.as_deref(), value)
            }
            QueryField::To => envelope
                .to
                .iter()
                .any(|addr| address_matches(&addr.email, addr.name.as_deref(), value)),
            QueryField::Cc => envelope
                .cc
                .iter()
                .any(|addr| address_matches(&addr.email, addr.name.as_deref(), value)),
            QueryField::Bcc => envelope
                .bcc
                .iter()
                .any(|addr| address_matches(&addr.email, addr.name.as_deref(), value)),
            _ => true,
        },
        QueryNode::Filter(filter) => matches_filter(filter, envelope),
        QueryNode::Label(label) => envelope
            .label_provider_ids
            .iter()
            .any(|provider_id| provider_id.eq_ignore_ascii_case(label)),
        QueryNode::DateRange { bound, date } => matches_date(bound, date, envelope),
        QueryNode::Size { op, bytes } => matches_size(op, *bytes, envelope.size_bytes),
        QueryNode::And(left, right) => {
            matches_structured_filters(left, envelope)
                && matches_structured_filters(right, envelope)
        }
        QueryNode::Or(left, right) => {
            matches_structured_filters(left, envelope)
                || matches_structured_filters(right, envelope)
        }
        QueryNode::Not(inner) => !matches_structured_filters(inner, envelope),
        _ => true,
    }
}

fn address_matches(email: &str, name: Option<&str>, value: &str) -> bool {
    let needle = value.to_ascii_lowercase();
    email.to_ascii_lowercase().contains(&needle)
        || name
            .unwrap_or_default()
            .to_ascii_lowercase()
            .contains(&needle)
}

fn matches_filter(filter: &FilterKind, envelope: &mxr_core::Envelope) -> bool {
    match filter {
        FilterKind::Unread => !envelope.flags.contains(mxr_core::MessageFlags::READ),
        FilterKind::Read => envelope.flags.contains(mxr_core::MessageFlags::READ),
        FilterKind::Starred => envelope.flags.contains(mxr_core::MessageFlags::STARRED),
        FilterKind::Draft => envelope.flags.contains(mxr_core::MessageFlags::DRAFT),
        FilterKind::Sent => envelope.flags.contains(mxr_core::MessageFlags::SENT),
        FilterKind::Trash => envelope.flags.contains(mxr_core::MessageFlags::TRASH),
        FilterKind::Spam => envelope.flags.contains(mxr_core::MessageFlags::SPAM),
        FilterKind::Answered => envelope.flags.contains(mxr_core::MessageFlags::ANSWERED),
        FilterKind::Inbox => envelope
            .label_provider_ids
            .iter()
            .any(|label| label.eq_ignore_ascii_case(system_labels::INBOX)),
        FilterKind::Archived => {
            !envelope
                .label_provider_ids
                .iter()
                .any(|label| label.eq_ignore_ascii_case(system_labels::INBOX))
                && !envelope.flags.contains(mxr_core::MessageFlags::SENT)
                && !envelope.flags.contains(mxr_core::MessageFlags::DRAFT)
                && !envelope.flags.contains(mxr_core::MessageFlags::TRASH)
                && !envelope.flags.contains(mxr_core::MessageFlags::SPAM)
        }
        FilterKind::HasAttachment => envelope.has_attachments,
        FilterKind::HasCalendar => true,
        FilterKind::Anywhere => true,
        FilterKind::HasUserLabels => envelope
            .label_provider_ids
            .iter()
            .any(|label| !is_system_label(label)),
        FilterKind::NoUserLabels => envelope
            .label_provider_ids
            .iter()
            .all(|label| is_system_label(label)),
        FilterKind::HasDrive
        | FilterKind::HasDocument
        | FilterKind::HasSpreadsheet
        | FilterKind::HasPresentation
        | FilterKind::HasYoutube
        | FilterKind::HasInlineImage => true,
        FilterKind::HasLink => envelope.link_count > 0,
        FilterKind::HasLinkHeavy => {
            matches!(envelope.link_density(), mxr_core::types::LinkDensity::Heavy)
        }
        FilterKind::NoLinks => envelope.link_count == 0,
        // Mxr-specific custom filters: owed-reply is computed at the
        // thread level (see ast_contains_owed_reply above) and the
        // per-envelope test defaults to true so it never independently
        // drops candidates. reply-later is an indexed boolean handled
        // by the QueryBuilder; here we default true for the same
        // reason.
        FilterKind::Custom(name)
            if name == mxr_search::FILTER_OWED_REPLY || name == mxr_search::FILTER_REPLY_LATER =>
        {
            true
        }
        // Unknown custom filter: default to true (no-op filter) so
        // future Gmail additions don't break searches.
        FilterKind::Custom(_) => true,
        // Forward-compat for #[non_exhaustive] mail-query additions.
        _ => true,
    }
}

fn is_system_label(label: &str) -> bool {
    let label = label.to_ascii_uppercase();
    label.starts_with("CATEGORY_")
        || matches!(
            label.as_str(),
            "INBOX"
                | "SENT"
                | "DRAFT"
                | "DRAFTS"
                | "TRASH"
                | "DELETED"
                | "SPAM"
                | "JUNK"
                | "STARRED"
                | "UNREAD"
                | "IMPORTANT"
                | "CHAT"
                | "ARCHIVE"
                | "ARCHIVED"
                | "SNOOZED"
                | "MUTED"
                | "MUTE"
        )
}

fn matches_date(bound: &DateBound, date: &DateValue, envelope: &mxr_core::Envelope) -> bool {
    let message_date = envelope.date.date_naive();
    let resolved = resolve_date_value(date);
    match bound {
        DateBound::After => message_date >= resolved,
        DateBound::Before => message_date < resolved,
        DateBound::Exact => message_date == resolved,
        _ => true,
    }
}

fn resolve_date_value(value: &DateValue) -> chrono::NaiveDate {
    let today = chrono::Local::now().date_naive();
    match value {
        DateValue::Specific(date) => *date,
        DateValue::Today => today,
        DateValue::Yesterday => today.pred_opt().unwrap_or(today),
        DateValue::ThisWeek => {
            let weekday = today.weekday().num_days_from_monday();
            today - chrono::Duration::days(i64::from(weekday))
        }
        DateValue::ThisMonth => {
            chrono::NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap_or(today)
        }
        DateValue::Relative { amount, unit } => {
            let days = match unit {
                mxr_search::RelativeUnit::Day => i64::from(*amount),
                mxr_search::RelativeUnit::Week => i64::from(*amount) * 7,
                mxr_search::RelativeUnit::Month => i64::from(*amount) * 30,
                mxr_search::RelativeUnit::Year => i64::from(*amount) * 365,
                _ => i64::from(*amount),
            };
            today - chrono::Duration::days(days)
        }
        _ => today,
    }
}

fn matches_size(op: &SizeOp, bytes: u64, actual: u64) -> bool {
    match op {
        SizeOp::LessThan => actual < bytes,
        SizeOp::LessThanOrEqual => actual <= bytes,
        SizeOp::Equal => actual == bytes,
        SizeOp::GreaterThan => actual > bytes,
        SizeOp::GreaterThanOrEqual => actual >= bytes,
        _ => true,
    }
}

pub(super) fn semantic_query_plan(ast: &QueryNode) -> Option<SemanticQueryPlan> {
    let mut parts = Vec::new();
    let mut source_kinds = Vec::new();
    let mut use_all_sources = false;
    collect_semantic_terms(
        ast,
        false,
        &mut parts,
        &mut source_kinds,
        &mut use_all_sources,
    );
    let text = parts.join(" ").trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(SemanticQueryPlan {
            text,
            source_kinds: if use_all_sources || source_kinds.is_empty() {
                all_semantic_source_kinds()
            } else {
                source_kinds
            },
        })
    }
}

fn collect_semantic_terms(
    node: &QueryNode,
    negated: bool,
    parts: &mut Vec<String>,
    source_kinds: &mut Vec<SemanticChunkSourceKind>,
    use_all_sources: &mut bool,
) {
    match node {
        QueryNode::Text(text) if !negated => {
            parts.push(text.clone());
            *use_all_sources = true;
        }
        QueryNode::Phrase(text) if !negated => {
            parts.push(text.clone());
            *use_all_sources = true;
        }
        QueryNode::Near { left, right, .. } if !negated => {
            parts.push(format!("{left} {right}"));
            *use_all_sources = true;
        }
        QueryNode::Field { field, value }
            if !negated
                && matches!(
                    field,
                    QueryField::Subject | QueryField::Body | QueryField::Filename
                ) =>
        {
            parts.push(value.clone());
            if !*use_all_sources {
                for source_kind in source_kinds_for_field(field) {
                    push_source_kind(source_kinds, *source_kind);
                }
            }
        }
        QueryNode::And(left, right) | QueryNode::Or(left, right) => {
            collect_semantic_terms(left, negated, parts, source_kinds, use_all_sources);
            collect_semantic_terms(right, negated, parts, source_kinds, use_all_sources);
        }
        QueryNode::Not(inner) => {
            collect_semantic_terms(inner, true, parts, source_kinds, use_all_sources);
        }
        _ => {}
    }
}

pub(super) fn has_negated_semantic_terms(node: &QueryNode) -> bool {
    match node {
        QueryNode::Not(inner) => contains_semantic_term(inner),
        QueryNode::And(left, right) | QueryNode::Or(left, right) => {
            has_negated_semantic_terms(left) || has_negated_semantic_terms(right)
        }
        _ => false,
    }
}

fn contains_semantic_term(node: &QueryNode) -> bool {
    match node {
        QueryNode::Text(_) | QueryNode::Phrase(_) | QueryNode::Near { .. } => true,
        QueryNode::Field { field, .. } => matches!(
            field,
            QueryField::Subject | QueryField::Body | QueryField::Filename
        ),
        QueryNode::And(left, right) | QueryNode::Or(left, right) => {
            contains_semantic_term(left) || contains_semantic_term(right)
        }
        QueryNode::Not(inner) => contains_semantic_term(inner),
        _ => false,
    }
}

fn all_semantic_source_kinds() -> Vec<SemanticChunkSourceKind> {
    vec![
        SemanticChunkSourceKind::Header,
        SemanticChunkSourceKind::Body,
        SemanticChunkSourceKind::AttachmentSummary,
        SemanticChunkSourceKind::AttachmentText,
        SemanticChunkSourceKind::Highlight,
    ]
}

fn source_kinds_for_field(field: &QueryField) -> &'static [SemanticChunkSourceKind] {
    match field {
        QueryField::Subject => &[SemanticChunkSourceKind::Header],
        // A highlight is a passage of the body the user kept.
        QueryField::Body => &[
            SemanticChunkSourceKind::Body,
            SemanticChunkSourceKind::Highlight,
        ],
        QueryField::Filename => &[
            SemanticChunkSourceKind::AttachmentSummary,
            SemanticChunkSourceKind::AttachmentText,
        ],
        QueryField::From
        | QueryField::To
        | QueryField::Cc
        | QueryField::Bcc
        | QueryField::List
        | QueryField::DeliveredTo
        | QueryField::Rfc822MsgId => &[],
        _ => &[],
    }
}

fn push_source_kind(
    source_kinds: &mut Vec<SemanticChunkSourceKind>,
    source_kind: SemanticChunkSourceKind,
) {
    if !source_kinds.contains(&source_kind) {
        source_kinds.push(source_kind);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_search::parse_query;

    /// Which of Trash and Spam a sample message in each place reaches the
    /// results through, for a query over Alice's and Bob's mail.
    fn reached(query: &str) -> Vec<&'static str> {
        let ast = exclude_trash_and_spam_by_default(parse_query(query).unwrap());
        let mut reached = Vec::new();
        for (name, from, flags) in [
            (
                "alice inbox",
                "alice@example.com",
                mxr_core::MessageFlags::empty(),
            ),
            (
                "alice trash",
                "alice@example.com",
                mxr_core::MessageFlags::TRASH,
            ),
            (
                "alice spam",
                "alice@example.com",
                mxr_core::MessageFlags::SPAM,
            ),
            (
                "bob trash",
                "bob@example.com",
                mxr_core::MessageFlags::TRASH,
            ),
            ("bob spam", "bob@example.com", mxr_core::MessageFlags::SPAM),
        ] {
            let envelope = mxr_core::Envelope {
                from: mxr_core::Address {
                    name: None,
                    email: from.to_string(),
                },
                flags,
                ..crate::test_fixtures::TestEnvelopeBuilder::new().build()
            };
            if matches_structured_filters(&ast, &envelope) {
                reached.push(name);
            }
        }
        reached
    }

    #[test]
    fn trash_and_spam_are_lifted_each_on_their_own_and_only_where_asked() {
        assert_eq!(reached("from:alice@example.com"), vec!["alice inbox"]);
        assert_eq!(
            reached("in:trash"),
            vec!["alice trash", "bob trash"],
            "in:trash must not let Spam in"
        );
        assert_eq!(
            reached("in:trash OR from:alice@example.com"),
            vec!["alice inbox", "alice trash", "bob trash"],
            "Alice's Spam came in through the in:trash alternative"
        );
        assert_eq!(
            reached("from:alice@example.com (in:spam OR in:inbox)"),
            vec!["alice spam"]
        );
        assert_eq!(reached("-in:trash"), vec!["alice inbox"]);
        assert_eq!(reached("in:anywhere").len(), 5);
    }

    /// Past the expansion cap a query cannot be scoped; it keeps both
    /// exclusions rather than letting Trash in for every alternative.
    #[test]
    fn a_query_too_large_to_scope_keeps_both_exclusions() {
        let large = "in:trash OR (a OR b) (c OR d) (e OR f) (g OR h) (i OR j) (k OR l)";
        assert!(alternatives(&parse_query(large).unwrap()).is_none());
        assert_eq!(reached(large), vec!["alice inbox"]);
        assert_eq!(
            reached(&format!("in:anywhere ({large})")).len(),
            5,
            "in:anywhere still reaches Trash and Spam"
        );
    }

    #[test]
    fn semantic_query_plan_uses_all_sources_for_unfielded_text() {
        let ast = parse_query("house of cards").unwrap();
        let plan = semantic_query_plan(&ast).unwrap();

        assert_eq!(plan.text, "house of cards");
        assert_eq!(
            plan.source_kinds,
            vec![
                SemanticChunkSourceKind::Header,
                SemanticChunkSourceKind::Body,
                SemanticChunkSourceKind::AttachmentSummary,
                SemanticChunkSourceKind::AttachmentText,
                SemanticChunkSourceKind::Highlight,
            ]
        );
    }

    #[test]
    fn semantic_query_plan_maps_subject_body_and_filename_fields() {
        let ast = parse_query("subject:cards body:house filename:deck").unwrap();
        let plan = semantic_query_plan(&ast).unwrap();

        assert_eq!(plan.text, "cards house deck");
        assert_eq!(
            plan.source_kinds,
            vec![
                SemanticChunkSourceKind::Header,
                SemanticChunkSourceKind::Body,
                SemanticChunkSourceKind::Highlight,
                SemanticChunkSourceKind::AttachmentSummary,
                SemanticChunkSourceKind::AttachmentText,
            ]
        );
    }

    #[test]
    fn semantic_query_plan_falls_back_to_all_sources_when_text_is_unfielded() {
        let ast = parse_query("subject:cards house").unwrap();
        let plan = semantic_query_plan(&ast).unwrap();

        assert_eq!(plan.text, "cards house");
        assert_eq!(
            plan.source_kinds,
            vec![
                SemanticChunkSourceKind::Header,
                SemanticChunkSourceKind::Body,
                SemanticChunkSourceKind::AttachmentSummary,
                SemanticChunkSourceKind::AttachmentText,
                SemanticChunkSourceKind::Highlight,
            ]
        );
    }

    #[test]
    fn semantic_query_plan_ignores_negated_terms_and_reports_negation() {
        let ast = parse_query("body:deployment -filename:report").unwrap();
        let plan = semantic_query_plan(&ast).unwrap();

        assert_eq!(plan.text, "deployment");
        assert_eq!(
            plan.source_kinds,
            vec![
                SemanticChunkSourceKind::Body,
                SemanticChunkSourceKind::Highlight
            ]
        );
        assert!(has_negated_semantic_terms(&ast));
    }
}
