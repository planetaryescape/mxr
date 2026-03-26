use crate::mxr_core::types::system_labels;
use crate::mxr_search::ast::{DateBound, DateValue, FilterKind, QueryField, QueryNode, SizeOp};
use chrono::Datelike;

pub(super) fn matches_structured_filters(
    node: &QueryNode,
    envelope: &crate::mxr_core::Envelope,
) -> bool {
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

fn matches_filter(filter: &FilterKind, envelope: &crate::mxr_core::Envelope) -> bool {
    match filter {
        FilterKind::Unread => !envelope.flags.contains(crate::mxr_core::MessageFlags::READ),
        FilterKind::Read => envelope.flags.contains(crate::mxr_core::MessageFlags::READ),
        FilterKind::Starred => envelope
            .flags
            .contains(crate::mxr_core::MessageFlags::STARRED),
        FilterKind::Draft => envelope
            .flags
            .contains(crate::mxr_core::MessageFlags::DRAFT),
        FilterKind::Sent => envelope.flags.contains(crate::mxr_core::MessageFlags::SENT),
        FilterKind::Trash => envelope
            .flags
            .contains(crate::mxr_core::MessageFlags::TRASH),
        FilterKind::Spam => envelope.flags.contains(crate::mxr_core::MessageFlags::SPAM),
        FilterKind::Answered => envelope
            .flags
            .contains(crate::mxr_core::MessageFlags::ANSWERED),
        FilterKind::Inbox => envelope
            .label_provider_ids
            .iter()
            .any(|label| label.eq_ignore_ascii_case(system_labels::INBOX)),
        FilterKind::Archived => {
            !envelope
                .label_provider_ids
                .iter()
                .any(|label| label.eq_ignore_ascii_case(system_labels::INBOX))
                && !envelope.flags.contains(crate::mxr_core::MessageFlags::SENT)
                && !envelope
                    .flags
                    .contains(crate::mxr_core::MessageFlags::DRAFT)
                && !envelope
                    .flags
                    .contains(crate::mxr_core::MessageFlags::TRASH)
                && !envelope.flags.contains(crate::mxr_core::MessageFlags::SPAM)
        }
        FilterKind::HasAttachment => envelope.has_attachments,
    }
}

fn matches_date(bound: &DateBound, date: &DateValue, envelope: &crate::mxr_core::Envelope) -> bool {
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

pub(super) fn semantic_query_text(ast: &QueryNode) -> Option<String> {
    let mut parts = Vec::new();
    collect_semantic_terms(ast, false, &mut parts);
    let query = parts.join(" ").trim().to_string();
    if query.is_empty() {
        None
    } else {
        Some(query)
    }
}

fn collect_semantic_terms(node: &QueryNode, negated: bool, parts: &mut Vec<String>) {
    match node {
        QueryNode::Text(text) if !negated => parts.push(text.clone()),
        QueryNode::Phrase(text) if !negated => parts.push(text.clone()),
        QueryNode::Field { field, value }
            if !negated
                && matches!(
                    field,
                    QueryField::Subject | QueryField::Body | QueryField::Filename
                ) =>
        {
            parts.push(value.clone());
        }
        QueryNode::And(left, right) | QueryNode::Or(left, right) => {
            collect_semantic_terms(left, negated, parts);
            collect_semantic_terms(right, negated, parts);
        }
        QueryNode::Not(inner) => collect_semantic_terms(inner, true, parts),
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
