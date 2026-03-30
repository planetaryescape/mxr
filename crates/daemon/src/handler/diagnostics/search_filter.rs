#![cfg_attr(test, allow(clippy::unwrap_used))]

use chrono::Datelike;
use mxr_core::types::{system_labels, SemanticChunkSourceKind};
use mxr_search::ast::{DateBound, DateValue, FilterKind, QueryField, QueryNode, SizeOp};

#[derive(Debug, Clone)]
pub(super) struct SemanticQueryPlan {
    pub text: String,
    pub source_kinds: Vec<SemanticChunkSourceKind>,
}

pub(super) fn matches_structured_filters(node: &QueryNode, envelope: &mxr_core::Envelope) -> bool {
    match node {
        QueryNode::Text(_) | QueryNode::Phrase(_) => true,
        QueryNode::Field { field, value } => match field {
            QueryField::Subject | QueryField::Body | QueryField::Filename => true,
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
    }
}

fn matches_date(bound: &DateBound, date: &DateValue, envelope: &mxr_core::Envelope) -> bool {
    let message_date = envelope.date.date_naive();
    let resolved = resolve_date_value(date);
    match bound {
        DateBound::After => message_date >= resolved,
        DateBound::Before => message_date < resolved,
        DateBound::Exact => message_date == resolved,
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
    }
}

fn matches_size(op: &SizeOp, bytes: u64, actual: u64) -> bool {
    match op {
        SizeOp::LessThan => actual < bytes,
        SizeOp::LessThanOrEqual => actual <= bytes,
        SizeOp::Equal => actual == bytes,
        SizeOp::GreaterThan => actual > bytes,
        SizeOp::GreaterThanOrEqual => actual >= bytes,
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
            collect_semantic_terms(inner, true, parts, source_kinds, use_all_sources)
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
        QueryNode::Text(_) | QueryNode::Phrase(_) => true,
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
    ]
}

fn source_kinds_for_field(field: &QueryField) -> &'static [SemanticChunkSourceKind] {
    match field {
        QueryField::Subject => &[SemanticChunkSourceKind::Header],
        QueryField::Body => &[SemanticChunkSourceKind::Body],
        QueryField::Filename => &[
            SemanticChunkSourceKind::AttachmentSummary,
            SemanticChunkSourceKind::AttachmentText,
        ],
        QueryField::From | QueryField::To | QueryField::Cc | QueryField::Bcc => &[],
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
            ]
        );
    }

    #[test]
    fn semantic_query_plan_ignores_negated_terms_and_reports_negation() {
        let ast = parse_query("body:deployment -filename:report").unwrap();
        let plan = semantic_query_plan(&ast).unwrap();

        assert_eq!(plan.text, "deployment");
        assert_eq!(plan.source_kinds, vec![SemanticChunkSourceKind::Body]);
        assert!(has_negated_semantic_terms(&ast));
    }
}
