//! Resolve user-typed label names in a parsed query AST to the
//! provider IDs that are actually indexed in Tantivy.
//!
//! Tantivy's `labels` field stores `Envelope::label_provider_ids`
//! (Gmail-style IDs like `Label_101`, `INBOX`). The mail-query parser
//! preserves whatever the user typed verbatim in `QueryNode::Label`,
//! so `label:Notto` produces `Label("Notto")`, the QueryBuilder
//! lowercases that to `notto`, and no document matches — even though
//! the user has 8 messages tagged with the label whose display name
//! is "Notto" and whose provider_id is `Label_101`.
//!
//! This module bridges that gap. We fetch the user's labels from the
//! store and rewrite every `QueryNode::Label(name_or_id)` into a
//! provider-ID-shaped node (or an `Or` over multiple provider IDs
//! when the same display name exists across accounts). When no label
//! has a matching display name we leave the original value in place
//! so raw provider IDs like `label:Label_101` and `label:INBOX` keep
//! working.
//!
//! The resolved AST is reused for the QueryBuilder, the
//! semantic-search post-filter, and the rules executor view that
//! flows through `execute_search`. Keeping resolution at the daemon
//! layer (not in `mxr-search`) means the search crate stays free of
//! store dependencies and the resolution rules can evolve with how
//! we sync labels.

use mxr_core::types::Label;
use mxr_search::ast::QueryNode;
use std::collections::HashMap;

/// Case-insensitive label-name → provider-id lookup. Multiple
/// provider IDs per name are possible across accounts.
pub(super) type LabelNameIndex = HashMap<String, Vec<String>>;

/// Build the lookup from a flat list of labels. Names are lowercased
/// so resolution matches Gmail-style case-insensitive label search.
/// Provider IDs are deduplicated per name to keep the rewritten AST
/// minimal when the same label exists on several accounts.
pub(super) fn build_label_name_index(labels: &[Label]) -> LabelNameIndex {
    let mut index: LabelNameIndex = HashMap::new();
    for label in labels {
        let key = label.name.to_lowercase();
        let bucket = index.entry(key).or_default();
        if !bucket.contains(&label.provider_id) {
            bucket.push(label.provider_id.clone());
        }
    }
    index
}

/// Walk the AST and rewrite `Label(name)` nodes whose value matches a
/// known display name. Unknown values are left alone so literal
/// provider IDs (e.g. `INBOX`, `Label_101`) still hit the index.
pub(super) fn resolve_label_names(ast: QueryNode, index: &LabelNameIndex) -> QueryNode {
    match ast {
        QueryNode::Label(value) => resolve_one(value, index),
        QueryNode::And(left, right) => QueryNode::And(
            Box::new(resolve_label_names(*left, index)),
            Box::new(resolve_label_names(*right, index)),
        ),
        QueryNode::Or(left, right) => QueryNode::Or(
            Box::new(resolve_label_names(*left, index)),
            Box::new(resolve_label_names(*right, index)),
        ),
        QueryNode::Not(inner) => QueryNode::Not(Box::new(resolve_label_names(*inner, index))),
        other => other,
    }
}

fn resolve_one(value: String, index: &LabelNameIndex) -> QueryNode {
    let Some(provider_ids) = index.get(&value.to_lowercase()) else {
        return QueryNode::Label(value);
    };
    let mut iter = provider_ids.iter();
    let Some(first) = iter.next() else {
        return QueryNode::Label(value);
    };
    let mut node = QueryNode::Label(first.clone());
    for provider_id in iter {
        node = QueryNode::Or(
            Box::new(node),
            Box::new(QueryNode::Label(provider_id.clone())),
        );
    }
    node
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::types::{Label, LabelKind};
    use mxr_core::{AccountId, LabelId};
    use mxr_search::parse_query;

    fn label(account: &AccountId, name: &str, provider_id: &str) -> Label {
        Label {
            id: LabelId::new(),
            account_id: account.clone(),
            name: name.into(),
            kind: LabelKind::User,
            color: None,
            provider_id: provider_id.into(),
            unread_count: 0,
            total_count: 0,
            role: None,
        }
    }

    #[test]
    fn resolves_display_name_to_provider_id() {
        let account = AccountId::new();
        let labels = vec![label(&account, "Notto", "Label_101")];
        let index = build_label_name_index(&labels);
        let ast = parse_query("label:Notto").unwrap();

        let resolved = resolve_label_names(ast, &index);

        assert!(
            matches!(&resolved, QueryNode::Label(id) if id == "Label_101"),
            "expected provider_id rewrite, got {resolved:?}"
        );
    }

    #[test]
    fn name_lookup_is_case_insensitive() {
        let account = AccountId::new();
        let labels = vec![label(&account, "Follow Up", "Label_42")];
        let index = build_label_name_index(&labels);
        let ast = parse_query("label:\"follow up\"").unwrap();

        let resolved = resolve_label_names(ast, &index);

        assert!(
            matches!(&resolved, QueryNode::Label(id) if id == "Label_42"),
            "expected provider_id rewrite for quoted lowercase name, got {resolved:?}"
        );
    }

    #[test]
    fn unknown_name_is_left_unchanged() {
        let account = AccountId::new();
        let labels = vec![label(&account, "Notto", "Label_101")];
        let index = build_label_name_index(&labels);
        let ast = parse_query("label:Label_101").unwrap();

        let resolved = resolve_label_names(ast, &index);

        assert!(
            matches!(&resolved, QueryNode::Label(value) if value == "Label_101"),
            "raw provider_id should pass through, got {resolved:?}"
        );
    }

    #[test]
    fn collides_across_accounts_to_or_chain() {
        let account_a = AccountId::new();
        let account_b = AccountId::new();
        let labels = vec![
            label(&account_a, "Notto", "Label_101"),
            label(&account_b, "Notto", "Label_777"),
        ];
        let index = build_label_name_index(&labels);
        let ast = parse_query("label:Notto").unwrap();

        let resolved = resolve_label_names(ast, &index);

        let QueryNode::Or(left, right) = resolved else {
            panic!("expected Or of provider_ids, got {resolved:?}");
        };
        let left_id = match *left {
            QueryNode::Label(id) => id,
            other => panic!("expected Label on left, got {other:?}"),
        };
        let right_id = match *right {
            QueryNode::Label(id) => id,
            other => panic!("expected Label on right, got {other:?}"),
        };
        let mut ids = [left_id, right_id];
        ids.sort();
        assert_eq!(ids, ["Label_101".to_string(), "Label_777".to_string()]);
    }

    #[test]
    fn rewrite_descends_through_boolean_nodes() {
        let account = AccountId::new();
        let labels = vec![label(&account, "Notto", "Label_101")];
        let index = build_label_name_index(&labels);
        let ast = parse_query("label:Notto AND -label:Drafts").unwrap();

        let resolved = resolve_label_names(ast, &index);

        // Outer And, left side resolved to Label_101, right side
        // (negated unknown) passes through unchanged.
        let QueryNode::And(left, right) = resolved else {
            panic!("expected And node, got {resolved:?}");
        };
        assert!(matches!(*left, QueryNode::Label(ref id) if id == "Label_101"));
        let QueryNode::Not(inner) = *right else {
            panic!("expected Not on right");
        };
        assert!(matches!(*inner, QueryNode::Label(ref id) if id == "Drafts"));
    }
}
