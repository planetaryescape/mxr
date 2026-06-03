use super::helpers::{dry_run_rules, persist_rule};
use super::{
    build_rule_from_form, conditions_to_query, parse_rule_value, rule_actions_to_string,
    rule_to_form_data, HandlerResult,
};
use crate::state::AppState;
use mxr_protocol::ResponseData;
use mxr_rules::{Rule, RuleAction};
use std::sync::atomic::{AtomicBool, Ordering};

static SHELL_HOOK_WARNING_EMITTED: AtomicBool = AtomicBool::new(false);

pub(super) async fn list_rules(state: &AppState) -> HandlerResult {
    let rows = state.store.list_rules().await?;
    Ok(ResponseData::Rules {
        rules: rows
            .iter()
            .map(|row| rule_json_with_form_fields(mxr_store::row_to_rule_json(row)))
            .collect(),
    })
}

fn rule_json_with_form_fields(mut value: serde_json::Value) -> serde_json::Value {
    if let Ok(rule) = serde_json::from_value::<Rule>(value.clone()) {
        if let Ok(condition) = conditions_to_query(&rule.conditions) {
            value["condition"] = serde_json::Value::String(condition);
        }
        if let Ok(action) = rule_actions_to_string(&rule.actions) {
            value["action"] = serde_json::Value::String(action);
        }
    }
    value
}

pub(super) async fn get_rule(state: &AppState, rule: &str) -> HandlerResult {
    match state.store.get_rule_by_id_or_name(rule).await? {
        Some(row) => Ok(ResponseData::RuleData {
            rule: mxr_store::row_to_rule_json(&row),
        }),
        None => Err(format!("Rule not found: {rule}").into()),
    }
}

pub(super) async fn get_rule_form(state: &AppState, rule: &str) -> HandlerResult {
    match state.store.get_rule_by_id_or_name(rule).await? {
        Some(row) => {
            let parsed: Rule = serde_json::from_value(mxr_store::row_to_rule_json(&row))?;
            let form = rule_to_form_data(&parsed)?;
            Ok(ResponseData::RuleFormData { form })
        }
        None => Err(format!("Rule not found: {rule}").into()),
    }
}

pub(super) async fn upsert_rule_value(state: &AppState, value: serde_json::Value) -> HandlerResult {
    let rule = parse_rule_value(value.clone())?;
    warn_once_for_enabled_shell_hook(&rule);
    persist_rule(state, &rule).await?;
    Ok(ResponseData::RuleData { rule: value })
}

pub(super) async fn delete_rule(state: &AppState, rule: &str) -> HandlerResult {
    match state.store.get_rule_by_id_or_name(rule).await? {
        Some(row) => {
            let id = mxr_store::row_to_rule_json(&row)["id"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            state.store.delete_rule(&id).await?;
            Ok(ResponseData::Ack)
        }
        None => Err(format!("Rule not found: {rule}").into()),
    }
}

pub(super) async fn upsert_rule_form(
    state: &AppState,
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
    let value = serde_json::to_value(&rule)?;
    warn_once_for_enabled_shell_hook(&rule);
    persist_rule(state, &rule).await?;
    Ok(ResponseData::RuleData { rule: value })
}

fn warn_once_for_enabled_shell_hook(rule: &Rule) {
    if !rule.enabled
        || !rule
            .actions
            .iter()
            .any(|action| matches!(action, RuleAction::ShellHook { .. }))
    {
        return;
    }
    if !SHELL_HOOK_WARNING_EMITTED.swap(true, Ordering::Relaxed) {
        tracing::warn!(
            rule_id = %rule.id,
            "enabled shell-hook rule; hook commands are trusted local configuration and execute with the user's OS privileges"
        );
    }
}

pub(super) async fn list_rule_history(
    state: &AppState,
    rule: Option<&String>,
    limit: u32,
) -> HandlerResult {
    let resolved_rule_id = if let Some(rule) = rule {
        match state.store.get_rule_by_id_or_name(rule).await? {
            Some(row) => Some(
                mxr_store::row_to_rule_json(&row)["id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            ),
            None => return Err(format!("Rule not found: {rule}").into()),
        }
    } else {
        None
    };

    let rows = state
        .store
        .list_rule_logs(resolved_rule_id.as_deref(), limit)
        .await?;
    Ok(ResponseData::RuleHistory {
        entries: rows.iter().map(mxr_store::row_to_rule_log_json).collect(),
    })
}

pub(super) async fn dry_run(
    state: &AppState,
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
