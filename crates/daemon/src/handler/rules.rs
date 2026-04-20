use super::helpers::{dry_run_rules, persist_rule};
use super::{build_rule_from_form, parse_rule_value, rule_to_form_data, HandlerResult};
use crate::state::AppState;
use mxr_protocol::ResponseData;
use mxr_rules::Rule;
use std::sync::Arc;

pub(super) async fn list_rules(state: &Arc<AppState>) -> HandlerResult {
    let rows = state.store.list_rules().await.map_err(|e| e.to_string())?;
    Ok(ResponseData::Rules {
        rules: rows.iter().map(mxr_store::row_to_rule_json).collect(),
    })
}

pub(super) async fn get_rule(state: &Arc<AppState>, rule: &str) -> HandlerResult {
    match state
        .store
        .get_rule_by_id_or_name(rule)
        .await
        .map_err(|e| e.to_string())?
    {
        Some(row) => Ok(ResponseData::RuleData {
            rule: mxr_store::row_to_rule_json(&row),
        }),
        None => Err(format!("Rule not found: {rule}")),
    }
}

pub(super) async fn get_rule_form(state: &Arc<AppState>, rule: &str) -> HandlerResult {
    match state
        .store
        .get_rule_by_id_or_name(rule)
        .await
        .map_err(|e| e.to_string())?
    {
        Some(row) => {
            let parsed: Rule = serde_json::from_value(mxr_store::row_to_rule_json(&row))
                .map_err(|e| e.to_string())?;
            let form = rule_to_form_data(&parsed)?;
            Ok(ResponseData::RuleFormData { form })
        }
        None => Err(format!("Rule not found: {rule}")),
    }
}

pub(super) async fn upsert_rule_value(
    state: &Arc<AppState>,
    value: serde_json::Value,
) -> HandlerResult {
    let rule = parse_rule_value(value.clone())?;
    persist_rule(state, &rule).await?;
    Ok(ResponseData::RuleData { rule: value })
}

pub(super) async fn delete_rule(state: &Arc<AppState>, rule: &str) -> HandlerResult {
    match state
        .store
        .get_rule_by_id_or_name(rule)
        .await
        .map_err(|e| e.to_string())?
    {
        Some(row) => {
            let id = mxr_store::row_to_rule_json(&row)["id"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            state
                .store
                .delete_rule(&id)
                .await
                .map_err(|e| e.to_string())?;
            Ok(ResponseData::Ack)
        }
        None => Err(format!("Rule not found: {rule}")),
    }
}

pub(super) async fn upsert_rule_form(
    state: &Arc<AppState>,
    existing_rule: Option<&String>,
    name: &str,
    condition: &str,
    action: &str,
    priority: i32,
    enabled: bool,
) -> HandlerResult {
    let rule = build_rule_from_form(
        state,
        existing_rule,
        name,
        condition,
        action,
        priority,
        enabled,
    )
    .await?;
    let value = serde_json::to_value(&rule).map_err(|e| e.to_string())?;
    persist_rule(state, &rule).await?;
    Ok(ResponseData::RuleData { rule: value })
}

pub(super) async fn list_rule_history(
    state: &Arc<AppState>,
    rule: Option<&String>,
    limit: u32,
) -> HandlerResult {
    let resolved_rule_id = if let Some(rule) = rule {
        match state
            .store
            .get_rule_by_id_or_name(rule)
            .await
            .map_err(|e| e.to_string())?
        {
            Some(row) => Some(
                mxr_store::row_to_rule_json(&row)["id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            ),
            None => return Err(format!("Rule not found: {rule}")),
        }
    } else {
        None
    };

    let rows = state
        .store
        .list_rule_logs(resolved_rule_id.as_deref(), limit)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ResponseData::RuleHistory {
        entries: rows.iter().map(mxr_store::row_to_rule_log_json).collect(),
    })
}

pub(super) async fn dry_run(
    state: &Arc<AppState>,
    rule: Option<&String>,
    all: bool,
    after: Option<&String>,
) -> HandlerResult {
    let results = dry_run_rules(state, rule.cloned(), all, after.cloned()).await?;
    Ok(ResponseData::RuleDryRun {
        results: results
            .into_iter()
            .map(|result| serde_json::to_value(result).unwrap_or(serde_json::Value::Null))
            .collect(),
    })
}
