//! A bounded sorting preview and its exact application, shared by every client.
use super::{arrivals, desk, HandlerError, HandlerResult};
use crate::{loops::RuleMessage, state::AppState};
use mxr_protocol::{
    ResponseData, RuleFormData, RuleTreatmentMatchData, RuleTreatmentPreviewData,
    RuleTreatmentSelectionData,
};
use mxr_rules::{Rule, RuleAction, RuleEngine};
use serde_json::Value;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

#[derive(Default)]
pub(crate) struct TreatmentPreviews(parking_lot::Mutex<HashMap<String, Preview>>);
struct Preview {
    rule: Rule,
    result: RuleTreatmentSelectionData,
    placements: Vec<mxr_store::ArrivalPlacement>,
    rules: Value,
    created: Instant,
}
const LIMIT: u32 = 200;
const TTL: Duration = Duration::from_secs(600);

pub(super) async fn validate(state: &AppState, rule: &Rule) -> Result<(), HandlerError> {
    let treatments = rule
        .actions
        .iter()
        .filter(|a| matches!(a, RuleAction::SetTreatment { .. }))
        .count();
    if treatments == 0 {
        return Ok(());
    }
    if treatments != 1 {
        return Err("A rule must have exactly one sorting treatment".into());
    }
    let account = rule
        .account_id
        .as_ref()
        .ok_or("Sorting rules require an account")?;
    if state.store.get_account(account).await?.is_none() {
        return Err("Sorting account does not exist".into());
    }
    if rule.name.trim().is_empty() {
        return Err("Rule name is required".into());
    }
    validate_conditions(&rule.conditions)?;
    Ok(())
}
fn validate_conditions(c: &mxr_rules::Conditions) -> Result<(), HandlerError> {
    use mxr_rules::{Conditions as C, FieldCondition as F, StringMatch};
    match c {
        C::And { conditions } | C::Or { conditions } => {
            if conditions.is_empty() {
                return Err("Sorting condition group cannot be empty".into());
            }
            for c in conditions {
                validate_conditions(c)?;
            }
        }
        C::Not { condition } => validate_conditions(condition)?,
        C::Field(F::From { pattern } | F::To { pattern } | F::Subject { pattern }) => {
            if let StringMatch::Regex(pattern) = pattern {
                regex::Regex::new(pattern).map_err(|e| e.to_string())?;
            }
        }
        C::Field(F::BodyContains { .. } | F::LinkDensity { .. }) => {
            return Err("Sorting rules support header conditions only; body and link-density conditions are not supported".into());
        }
        _ => {}
    }
    Ok(())
}

async fn selection(
    state: &AppState,
    rule: &Rule,
) -> Result<
    (
        RuleTreatmentSelectionData,
        Value,
        Vec<mxr_store::ArrivalPlacement>,
    ),
    HandlerError,
> {
    let account = rule
        .account_id
        .as_ref()
        .ok_or("Sorting rules require an account")?;
    let rows = state.store.list_rules().await?;
    let stored: Vec<Rule> = rows
        .iter()
        .map(|r| serde_json::from_value(mxr_store::row_to_rule_json(r)))
        .collect::<Result<_, _>>()?;
    let version = serde_json::to_value(&stored)?;
    let mut rules: Vec<_> = stored.into_iter().filter(|r| r.id != rule.id).collect();
    rules.push(rule.clone());
    let engine = RuleEngine::new(rules);
    let envelopes = state
        .store
        .list_envelopes_by_account(account, LIMIT + 1, 0)
        .await?;
    let complete = envelopes.len() <= LIMIT as usize;
    let threads: Vec<_> = envelopes
        .iter()
        .take(LIMIT as usize)
        .map(|e| e.thread_id.clone())
        .collect();
    let messages = state
        .store
        .desk_messages_in_threads(account, &threads)
        .await?;
    let mut senders = desk::Senders::load(state, account, &messages).await?;
    let is_self = desk::self_matcher(state, account).await?;
    let shape = super::conversation_shape::shape_config(state);
    let labels = state.store.list_labels_by_account(account).await?;
    let from: Vec<_> = messages
        .iter()
        .map(|m| m.from.email.to_ascii_lowercase())
        .collect();
    let written_to = state.store.addresses_written_to(account, &from).await?;
    let mut matches = Vec::new();
    let mut placements = Vec::new();
    let mut unavailable = 0;
    for envelope in envelopes.into_iter().take(LIMIT as usize) {
        let Some(message) = messages.iter().find(|m| m.id == envelope.id) else {
            continue;
        };
        if !message.in_inbox
            || message.trashed
            || message.direction == "outbound"
            || is_self(&envelope.from.email)
        {
            continue;
        }
        let body = state.store.get_body(&envelope.id).await?;
        let ids = state.store.get_message_label_ids(&envelope.id).await?;
        let visible_labels = labels
            .iter()
            .filter(|l| ids.contains(&l.id))
            .map(|l| l.provider_id.clone())
            .collect();
        let view = RuleMessage::from_parts(envelope.clone(), body, visible_labels);
        match rule.conditions.evaluate_known(&view) {
            None => {
                unavailable += 1;
                continue;
            }
            Some(false) => continue,
            Some(true) => {}
        }
        let thread: Vec<_> = messages
            .iter()
            .filter(|m| m.thread_id == envelope.thread_id)
            .cloned()
            .collect();
        let before = arrivals::place_message(
            message,
            &thread,
            &senders,
            &written_to,
            &is_self,
            shape,
            envelope.flags.contains(mxr_core::MessageFlags::SPAM),
        );
        let winning = engine.treatment(&view, account);
        let prior = senders.treatments.get(&envelope.id).cloned();
        if let Some((winner, treatment)) = winning {
            senders.treatments.insert(
                envelope.id.clone(),
                (treatment.as_str().into(), winner.name.clone()),
            );
        }
        let after = arrivals::place_message(
            message,
            &thread,
            &senders,
            &written_to,
            &is_self,
            shape,
            envelope.flags.contains(mxr_core::MessageFlags::SPAM),
        );
        senders.treatments.remove(&envelope.id);
        if let Some(prior) = prior {
            senders.treatments.insert(envelope.id.clone(), prior);
        }
        if winning.is_some() {
            placements.push(after.clone());
        }
        matches.push(RuleTreatmentMatchData {
            message_id: envelope.id,
            from: envelope.from.email,
            subject: envelope.subject,
            before: before.mode,
            after: after.mode,
            reason: after.reason,
            winner: winning.map(|(r, _)| r.id.0.clone()),
            treatment: winning.map(|(_, t)| t.as_str().to_string()),
            blocked: after.rule == "decision" || after.rule == "moved" || after.rule == "spam",
        });
    }
    Ok((RuleTreatmentSelectionData {matches, complete, scan_limit: LIMIT, unavailable,
        notice: "Apply changes only sorting for this selection. Other actions keep their existing sync behavior. Sorting uses header conditions and inbound inbox mail only. Saving, editing, disabling or deleting a rule affects future arrivals; historical placements remain until you explicitly apply a sorting preview.".into()}, version, placements))
}

pub(super) async fn run(
    state: &AppState,
    form: &RuleFormData,
    token: Option<&str>,
) -> HandlerResult {
    let _sorting = state.rule_mutation_gate.lock().await;
    let rule = super::build_rule_from_form(
        state,
        form.id.as_ref(),
        &form.name,
        &form.condition,
        &form.action,
        form.priority,
        form.enabled,
        form.account_id.as_ref(),
    )
    .await?;
    validate(state, &rule).await?;
    if !rule
        .actions
        .iter()
        .any(|a| matches!(a, RuleAction::SetTreatment { .. }))
    {
        return Err("This preview requires a sorting treatment".into());
    }
    if let Some(token) = token {
        let preview = state
            .treatment_previews
            .0
            .lock()
            .remove(token)
            .ok_or("Preview expired or already applied; preview again")?;
        if preview.created.elapsed() > TTL {
            return Err("Preview expired; preview again".into());
        }
        let mut current = rule.clone();
        current.id = preview.rule.id.clone();
        current.created_at = preview.rule.created_at;
        current.updated_at = preview.rule.updated_at;
        if serde_json::to_value(&current)? != serde_json::to_value(&preview.rule)? {
            return Err("Rule changed; preview again".into());
        }
        let (result, versions, placements) = selection(state, &preview.rule).await?;
        if result != preview.result || versions != preview.rules || placements != preview.placements
        {
            return Err("Mail or rules changed; preview again".into());
        }
        let mut applied_rule = preview.rule.clone();
        applied_rule.updated_at = chrono::Utc::now();

        let account = applied_rule
            .account_id
            .as_ref()
            .ok_or("Sorting rules require an account")?;
        let mut treatments = Vec::new();
        for item in &result.matches {
            let id = item.message_id.clone();
            if let (Some(winner_id), Some(treatment)) =
                (item.winner.as_deref(), item.treatment.as_deref())
            {
                let winner = if winner_id == applied_rule.id.0 {
                    applied_rule.clone()
                } else {
                    let row = state
                        .store
                        .get_rule_by_id_or_name(winner_id)
                        .await?
                        .ok_or("Winning rule disappeared")?;
                    serde_json::from_value(mxr_store::row_to_rule_json(&row))?
                };
                treatments.push(mxr_store::RuleTreatmentInput {
                    message_id: id.clone(),
                    rule_id: winner.id.0.clone(),
                    rule_updated_at: winner.updated_at.to_rfc3339(),
                    treatment: treatment.to_string(),
                    rule_name: winner.name.clone(),
                });
            }
        }
        let conditions_json = serde_json::to_string(&applied_rule.conditions)?;
        let actions_json = serde_json::to_string(&applied_rule.actions)?;
        state
            .store
            .apply_rule_treatments(
                mxr_store::RuleRecordInput {
                    id: &applied_rule.id.0,
                    account_id: applied_rule.account_id.as_ref(),
                    name: &applied_rule.name,
                    enabled: applied_rule.enabled,
                    priority: applied_rule.priority,
                    conditions_json: &conditions_json,
                    actions_json: &actions_json,
                    created_at: applied_rule.created_at,
                    updated_at: applied_rule.updated_at,
                },
                &treatments,
                &placements,
            )
            .await?;
        arrivals::modes_changed(state, account);
        return Ok(ResponseData::RuleTreatmentResult {
            preview: RuleTreatmentPreviewData {
                applied: true,
                token: None,
                result,
                rule_id: Some(applied_rule.id.0),
            },
        });
    }
    let (result, rules, placements) = selection(state, &rule).await?;
    let token = uuid::Uuid::now_v7().to_string();
    let mut previews = state.treatment_previews.0.lock();
    previews.retain(|_, p| p.created.elapsed() <= TTL);
    if previews.len() >= 64 {
        if let Some(oldest) = previews
            .iter()
            .min_by_key(|(_, p)| p.created)
            .map(|(token, _)| token.clone())
        {
            previews.remove(&oldest);
        }
    }
    previews.insert(
        token.clone(),
        Preview {
            rule,
            result: result.clone(),
            placements,
            rules,
            created: Instant::now(),
        },
    );
    Ok(ResponseData::RuleTreatmentResult {
        preview: RuleTreatmentPreviewData {
            applied: false,
            token: Some(token),
            result,
            rule_id: None,
        },
    })
}

/// Callers hold rule_mutation_gate through local treatment and first placement.
/// Provider actions and shell hooks run after this gate has been released.
pub(crate) async fn classify_pending(
    state: &AppState,
    account: &mxr_core::AccountId,
    ids: &[mxr_core::MessageId],
) -> Result<(), HandlerError> {
    let stored: Vec<Rule> = state
        .store
        .list_rules()
        .await?
        .iter()
        .map(|r| serde_json::from_value(mxr_store::row_to_rule_json(r)))
        .collect::<Result<_, _>>()?;
    let engine = RuleEngine::new(stored);
    let labels = state.store.list_labels_by_account(account).await?;
    let is_self = desk::self_matcher(state, account).await?;
    for id in ids {
        if !state.store.needs_rule_treatment(id).await? {
            continue;
        }
        let Some(envelope) = state.store.get_envelope(id).await? else {
            continue;
        };
        if &envelope.account_id != account
            || is_self(&envelope.from.email)
            || envelope.flags.intersects(
                mxr_core::MessageFlags::TRASH
                    | mxr_core::MessageFlags::SPAM
                    | mxr_core::MessageFlags::SENT,
            )
        {
            continue;
        }
        let label_ids = state.store.get_message_label_ids(id).await?;
        let visible: Vec<_> = labels
            .iter()
            .filter(|l| label_ids.contains(&l.id))
            .map(|l| l.provider_id.clone())
            .collect();
        if !visible.iter().any(|l| l == "INBOX")
            || visible.iter().any(|l| l == "TRASH" || l == "SPAM")
        {
            continue;
        }
        let message = RuleMessage::from_parts(envelope, None, visible);
        if let Some((rule, treatment)) = engine.treatment(&message, account) {
            state
                .store
                .set_rule_treatment(
                    id,
                    &rule.id.0,
                    &rule.updated_at.to_rfc3339(),
                    treatment.as_str(),
                    &rule.name,
                )
                .await?;
        }
    }
    Ok(())
}
