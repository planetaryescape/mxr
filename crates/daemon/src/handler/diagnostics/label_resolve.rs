use mxr_core::types::Label;
use mxr_search::ast::QueryNode;
use std::collections::HashMap;

pub(super) type LabelNameIndex = HashMap<String, Vec<String>>;

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
}
