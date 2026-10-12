//! Local category rule preview, exact application and sync reconciliation.
use super::{HandlerError, HandlerResult};
use crate::{loops::RuleMessage, state::AppState};
use mxr_core::{CategoryId, Envelope, MessageId};
use mxr_protocol::{ResponseData, RuleCategoriesPreviewData, RuleCategoryMatchData, RuleFormData};
use mxr_rules::{Rule, RuleAction};
use mxr_store::{CategoryMembershipVersion, RuleCategorySource, RuleRecordInput};
use serde_json::Value;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

#[derive(Default)]
pub(crate) struct CategoryRulePreviews(parking_lot::Mutex<HashMap<String, Preview>>);
struct Preview {
    rule: Rule,
    delete: bool,
    selection: Selection,
    created: Instant,
}
#[derive(PartialEq)]
struct Selection {
    snapshot: String,
    result: RuleCategoriesPreviewData,
    rules: Value,
    mail: Value,
    categories: Value,
    sources: Vec<RuleCategorySource>,
    manual: Vec<CategoryMembershipVersion>,
    next: Vec<RuleCategorySource>,
}
const TTL: Duration = Duration::from_secs(600);

pub(super) fn has_categories(rule: &Rule) -> bool {
    rule.actions.iter().any(|a| {
        matches!(
            a,
            RuleAction::AddCategory { .. } | RuleAction::RemoveCategory { .. }
        )
    })
}
fn category_action(action: &RuleAction) -> Option<(&CategoryId, bool)> {
    match action {
        RuleAction::AddCategory { category_id } => Some((category_id, true)),
        RuleAction::RemoveCategory { category_id } => Some((category_id, false)),
        _ => None,
    }
}
pub(super) async fn validate(state: &AppState, rule: &Rule) -> Result<(), HandlerError> {
    if rule.name.trim().is_empty() {
        return Err("Rule name is required".into());
    }
    if let Some(account) = &rule.account_id {
        if state.store.get_account(account).await?.is_none() {
            return Err("Rule account does not exist".into());
        }
    }
    let mut decisions = HashMap::new();
    for (id, assigned) in rule.actions.iter().filter_map(category_action) {
        if !state.store.personal_category_exists(id).await? {
            return Err(format!("Category {id} not found").into());
        }
        if decisions
            .insert(id, assigned)
            .is_some_and(|prior| prior != assigned)
        {
            return Err("A rule cannot add and remove the same category".into());
        }
    }
    Ok(())
}
fn contributions(
    rule: &Rule,
    envelope: &Envelope,
    message: &RuleMessage,
) -> Vec<RuleCategorySource> {
    if !rule.enabled
        || rule
            .account_id
            .as_ref()
            .is_some_and(|a| a != &envelope.account_id)
        || rule.conditions.evaluate_known(message) != Some(true)
    {
        return Vec::new();
    }
    let mut result: Vec<_> = rule
        .actions
        .iter()
        .filter_map(category_action)
        .map(|(id, assigned)| RuleCategorySource {
            message_id: envelope.id.clone(),
            category_id: id.clone(),
            rule_id: rule.id.0.clone(),
            assigned,
            priority: rule.priority,
            rule_name: rule.name.clone(),
            rule_revision: rule.updated_at.to_rfc3339(),
        })
        .collect();
    result.sort_by_key(|s| s.category_id.to_string());
    result.dedup_by(|a, b| a.category_id == b.category_id);
    result
}
async fn selection(state: &AppState, rule: &Rule, delete: bool) -> Result<Selection, HandlerError> {
    let snapshot = state.store.category_rule_snapshot().await?;
    let stored: Vec<Rule> = state
        .store
        .list_rules()
        .await?
        .iter()
        .map(|r| serde_json::from_value(mxr_store::row_to_rule_json(r)))
        .collect::<Result<_, _>>()?;
    let rules = serde_json::to_value(&stored)?;
    let sources = state.store.category_rule_sources().await?;
    let categories = serde_json::to_value(state.store.list_personal_categories(None).await?)?;
    let mut next = Vec::new();
    let mut mail = Vec::new();
    let mut envelopes = Vec::new();
    let mut unavailable = 0;
    let accounts = state.store.list_all_accounts().await?;
    for account in accounts {
        let labels = state.store.list_labels_by_account(&account.id).await?;
        let mut offset = 0;
        loop {
            let page = state
                .store
                .list_envelopes_by_account(&account.id, 500, offset)
                .await?;
            if page.is_empty() {
                break;
            }
            offset += page.len() as u32;
            for envelope in page {
                let body = state.store.get_body(&envelope.id).await?;
                let ids = state.store.get_message_label_ids(&envelope.id).await?;
                let visible: Vec<_> = labels
                    .iter()
                    .filter(|l| ids.contains(&l.id))
                    .map(|l| l.provider_id.clone())
                    .collect();
                // Bind every input, including nonmatches, so an edit cannot widen a reviewed selection.
                mail.push(serde_json::json!([envelope, body, visible]));
                let message = RuleMessage::from_parts(envelope.clone(), body, visible);
                if !delete {
                    next.extend(contributions(rule, &envelope, &message));
                }
                if rule.enabled
                    && rule
                        .account_id
                        .as_ref()
                        .is_none_or(|a| a == &envelope.account_id)
                    && rule.conditions.evaluate_known(&message).is_none()
                {
                    unavailable += 1;
                }
                envelopes.push(envelope);
            }
        }
    }
    let mut pairs: Vec<_> = sources
        .iter()
        .filter(|s| s.rule_id == rule.id.0)
        .chain(next.iter())
        .map(|s| (s.message_id.clone(), s.category_id.clone()))
        .collect();
    pairs.sort_by_key(|(m, c)| (m.to_string(), c.to_string()));
    pairs.dedup();
    let mut manual = Vec::new();
    let mut matches = Vec::new();
    let mut accounts = Vec::new();
    for (message_id, category_id) in pairs {
        let Some(envelope) = envelopes.iter().find(|e| e.id == message_id) else {
            continue;
        };
        let version = state
            .store
            .category_membership_versions(
                None,
                std::slice::from_ref(&message_id),
                std::slice::from_ref(&category_id),
            )
            .await?
            .remove(0);
        let winner = sources
            .iter()
            .filter(|s| s.rule_id != rule.id.0)
            .chain(next.iter())
            .filter(|s| s.message_id == message_id && s.category_id == category_id)
            .min_by_key(|s| (s.priority, &s.rule_id));
        let after = version
            .manual_decision
            .unwrap_or(winner.is_some_and(|s| s.assigned));
        let reason = match version.manual_decision {
            Some(true) => "Manual inclusion".into(),
            Some(false) => "Manual exclusion".into(),
            None => winner.map_or("No matching category rule".into(), |s| {
                format!(
                    "{} by {} (priority {})",
                    if s.assigned { "Added" } else { "Removed" },
                    s.rule_name,
                    s.priority
                )
            }),
        };
        matches.push(RuleCategoryMatchData {
            message_id,
            category_id,
            account_id: envelope.account_id.clone(),
            from: envelope.from.email.clone(),
            subject: envelope.subject.clone(),
            before: version.assigned,
            after,
            manual_decision: version.manual_decision,
            winner: winner.map(|s| s.rule_id.clone()),
            reason,
        });
        if !accounts.contains(&envelope.account_id) {
            accounts.push(envelope.account_id.clone());
        }
        manual.push(version);
    }
    if state.store.category_rule_snapshot().await? != snapshot {
        return Err("Mail or category inputs changed during preview; preview again".into());
    }
    Ok(Selection { snapshot, result: RuleCategoriesPreviewData { applied: false, token: None, rule_id: rule.id.0.clone(), delete, matches, accounts, complete: true, unavailable, notice: "Complete reconciliation of this rule's old sources and current matches. Manual choices win; lower priority numbers win category conflicts. Other actions retain their sync behavior.".into() }, rules, mail: Value::Array(mail), categories, sources, manual, next })
}
pub(super) async fn run(
    state: &AppState,
    form: &RuleFormData,
    token: Option<&str>,
    delete: bool,
) -> HandlerResult {
    let _gate = state.rule_mutation_gate.lock().await;
    let mut rule = super::build_rule_from_form(state, form).await?;
    // None is the explicit All accounts choice, including edits from a previously scoped rule.
    rule.account_id = form.account_id.clone();
    validate(state, &rule).await?;
    super::rule_treatment::validate(state, &rule).await?;
    let prior: Option<Rule> = if let Some(id) = &form.id {
        state
            .store
            .get_rule_by_id_or_name(id)
            .await?
            .map(|r| serde_json::from_value(mxr_store::row_to_rule_json(&r)))
            .transpose()?
    } else {
        None
    };
    if !has_categories(&rule) && !prior.as_ref().is_some_and(has_categories) {
        return Err("This preview requires category actions or a saved category rule".into());
    }
    if delete && prior.is_none() {
        return Err("Only a saved rule can be deleted".into());
    }
    let account_ids: Vec<_> = state
        .store
        .list_all_accounts()
        .await?
        .into_iter()
        .map(|a| a.id)
        .collect();
    let _change = state.sweep_gate.change(account_ids.iter()).await;
    if let Some(token) = token {
        let preview = state
            .category_rule_previews
            .0
            .lock()
            .remove(token)
            .ok_or("Category rule preview expired or already used; preview again")?;
        if preview.created.elapsed() > TTL {
            return Err("Category rule preview expired; preview again".into());
        }
        rule.id = preview.rule.id.clone();
        rule.created_at = preview.rule.created_at;
        rule.updated_at = preview.rule.updated_at;
        if delete != preview.delete
            || serde_json::to_value(&rule)? != serde_json::to_value(&preview.rule)?
        {
            return Err("Rule changed; preview again".into());
        }
        let current = selection(state, &rule, delete).await?;
        if current != preview.selection {
            return Err(
                "Mail, categories, manual choices or rule sources changed; preview again".into(),
            );
        }
        let conditions = serde_json::to_string(&rule.conditions)?;
        let actions = serde_json::to_string(&rule.actions)?;
        state
            .store
            .apply_category_rule(
                RuleRecordInput {
                    id: &rule.id.0,
                    account_id: rule.account_id.as_ref(),
                    name: &rule.name,
                    enabled: rule.enabled,
                    priority: rule.priority,
                    conditions_json: &conditions,
                    actions_json: &actions,
                    created_at: rule.created_at,
                    updated_at: rule.updated_at,
                },
                &current.next,
                delete,
                &current.snapshot,
            )
            .await?;
        let mut result = current.result;
        result.applied = true;
        return Ok(ResponseData::RuleCategoriesResult { preview: result });
    }
    let selected = selection(state, &rule, delete).await?;
    let token = uuid::Uuid::now_v7().to_string();
    let mut result = selected.result.clone();
    result.token = Some(token.clone());
    let mut previews = state.category_rule_previews.0.lock();
    previews.retain(|_, p| p.created.elapsed() <= TTL);
    if previews.len() >= 64 {
        if let Some(id) = previews
            .iter()
            .min_by_key(|(_, p)| p.created)
            .map(|(id, _)| id.clone())
        {
            previews.remove(&id);
        }
    }
    previews.insert(
        token,
        Preview {
            rule,
            delete,
            selection: selected,
            created: Instant::now(),
        },
    );
    Ok(ResponseData::RuleCategoriesResult { preview: result })
}
pub(crate) async fn classify_message(
    state: &AppState,
    envelope: &Envelope,
    message: &RuleMessage,
) -> Result<(), HandlerError> {
    let rules: Vec<Rule> = state
        .store
        .list_rules()
        .await?
        .iter()
        .map(|r| serde_json::from_value(mxr_store::row_to_rule_json(r)))
        .collect::<Result<_, _>>()?;
    let mut sources: Vec<_> = rules
        .iter()
        .flat_map(|r| contributions(r, envelope, message))
        .collect();
    sources.sort_by_key(|s| {
        (
            s.message_id.to_string(),
            s.category_id.to_string(),
            s.priority,
            s.rule_id.clone(),
        )
    });
    state
        .store
        .reconcile_message_category_sources(&envelope.id, &sources)
        .await?;
    Ok(())
}
pub(super) async fn explain(
    state: &AppState,
    message: &MessageId,
    category: &CategoryId,
) -> HandlerResult {
    let version = state
        .store
        .category_membership_versions(
            None,
            std::slice::from_ref(message),
            std::slice::from_ref(category),
        )
        .await?
        .remove(0);
    let winner = version
        .sources
        .iter()
        .min_by_key(|s| (s.priority, &s.rule_id));
    Ok(ResponseData::CategoryMembershipExplanation {
        explanation: serde_json::json!({"message_id":message,"category_id":category,"assigned":version.assigned,"manual_decision":version.manual_decision,"winner":winner,"sources":version.sources}),
    })
}
pub(super) async fn reject_references(
    state: &AppState,
    category: &CategoryId,
) -> Result<(), String> {
    let rules: Vec<Rule> = state
        .store
        .list_rules()
        .await
        .map_err(|e| e.to_string())?
        .iter()
        .map(|r| serde_json::from_value(mxr_store::row_to_rule_json(r)))
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    let names: Vec<_> = rules
        .iter()
        .filter(|r| {
            r.actions
                .iter()
                .filter_map(category_action)
                .any(|(id, _)| id == category)
        })
        .map(|r| r.name.as_str())
        .collect();
    if names.is_empty() {
        Ok(())
    } else {
        Err(format!("Category is referenced by rules: {}. Edit or delete those rules first (including disabled rules).",names.join(", ")))
    }
}
