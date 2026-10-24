//! Annual renewal reviews, inferred from Archive's charge history.
//!
//! The latest merged charge identifies the cycle, not the subscription's
//! oldest charge or its mutable title, amount or estimated renewal date.

use crate::subscriptions::{self, Cadence, Status};
use chrono::{DateTime, TimeZone, Utc};
use mxr_core::id::AccountId;
use mxr_store::{Store, TodoRecord, TodoState, TodoUpsert};
use mxr_todo::provenance::{FieldProvenance, FieldSource, FieldSources};
use mxr_todo::timing::{lead_time, start_of_day, TimingInput};
use mxr_todo::TodoKind;

/// Inspectable proposal; `plan` does not reserve the cycle or write tasks.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub record_ids: Vec<String>,
    pub task: TodoRecord,
}

/// The same selection feeds previews, tests and automatic filing. No task
/// is proposed without source mail. New reviews start within fourteen local
/// days; an existing review can take a corrected estimate outside that window.
pub async fn plan<Tz: TimeZone>(
    store: &Store,
    accounts: &[AccountId],
    now: DateTime<Utc>,
    tz: &Tz,
    morning_hour: u8,
) -> anyhow::Result<Vec<Candidate>> {
    let loaded = subscriptions::load(store, Some(accounts), now, tz).await?;
    let today = now.with_timezone(tz).date_naive();
    let mut out = Vec::new();
    for subscription in loaded.detection.subscriptions {
        if subscription.cadence != Cadence::Yearly || subscription.status != Status::Active {
            continue;
        }
        let Some(day) = subscription.next_expected else {
            continue;
        };
        let record_ids = subscription.last_charge().record_ids.clone();
        let sources = store.archive_record_sources(&record_ids).await?;
        let Some(source) = sources.into_iter().max_by(|a, b| {
            a.message_at
                .cmp(&b.message_at)
                .then_with(|| a.message_id.as_str().cmp(&b.message_id.as_str()))
        }) else {
            continue;
        };
        let Some(record) = loaded.records.get(&source.record_id) else {
            continue;
        };
        let account = &record.account_id;
        let dedup_key = store
            .record_renewal_cycle_key(account, &record_ids, false)
            .await?;
        // A corrected estimate can move outside the creation window;
        // refresh the existing review without creating an early/late one.
        if !(0..=subscriptions::RENEWAL_LEAD_DAYS).contains(&(day - today).num_days())
            && store
                .get_todo_by_dedup(account, &dedup_key)
                .await?
                .is_none()
        {
            continue;
        }
        let due = start_of_day(day, tz, 12);
        let input = TimingInput {
            kind: TodoKind::Renewal,
            doc_type: None,
            arrived: source.message_at,
            due: Some(due),
            event_start: None,
        };
        let timing = lead_time(&input, tz, morning_hour);
        let estimate = format!(
            "Estimated renewal around {} from {} annual charges; check with the provider",
            day.format("%-d %b %Y"),
            subscription.charges.len()
        );
        let mut fields = FieldSources::default();
        let inferred = |evidence: String| {
            FieldProvenance::with_evidence(FieldSource::Rule, evidence).unchecked()
        };
        fields.set(
            "title",
            inferred(format!("Review {} renewal", subscription.title())),
        );
        fields.set("due_at", inferred(estimate.clone()));
        fields.set(
            "act_by_at",
            inferred("Review before the estimated renewal date".into()),
        );
        fields.set(
            "surface_at",
            FieldProvenance::with_evidence(FieldSource::Table, timing.surface_rule),
        );
        if let Some(amount) = subscription.amount_minor {
            fields.set("amount", inferred(format!("Estimated from the last charge: {amount} minor units {}; the next price is not verified", subscription.currency.as_deref().unwrap_or("(currency unknown)"))));
        }
        let task = TodoRecord {
            id: mxr_todo::pass::new_todo_id(),
            account_id: account.clone(),
            thread_id: source.thread_id,
            source_message_id: Some(source.message_id),
            source_date: Some(source.message_at),
            kind: TodoKind::Renewal.as_str().into(),
            verb: "review".into(),
            doc_type: None,
            title: format!("Review {} renewal", subscription.title()),
            counterparty: Some(subscription.issuer),
            sender_domain: None,
            amount_minor: subscription.amount_minor,
            currency: subscription.currency,
            due_at: Some(due),
            due_words: Some(estimate.clone()),
            act_by_at: timing.act_by,
            surface_at: timing.surface_at,
            scheduled_for: None,
            action_url: None,
            action_domain: None,
            // An estimate must not silently close a review the user has
            // yet to do, including when cancellation is later inferred.
            relevant_until: None,
            window_source: None,
            state: TodoState::Open,
            expired_at: None,
            expired_at_birth: false,
            catchup: None,
            looks_done_message_id: None,
            looks_done_reason: None,
            origin: "rule".into(),
            reason: estimate,
            field_sources: fields.to_json(),
            user_edited: false,
            commitment_id: None,
            rules_version: mxr_todo::RULES_VERSION,
            dedup_key,
            surfaced_at: None,
            created_at: now,
            updated_at: now,
            done_at: None,
            dismissed_at: None,
        };
        out.push(Candidate { record_ids, task });
    }
    Ok(out)
}

/// Aliases commit before the protected task upsert. If it fails or the
/// process stops between writes, replay recreates the missing task at the
/// reserved key. User edits and state decisions remain authoritative.
pub async fn apply(store: &Store, candidates: Vec<Candidate>) -> anyhow::Result<u64> {
    let mut changed = 0;
    for Candidate {
        record_ids,
        mut task,
    } in candidates
    {
        task.dedup_key = store
            .record_renewal_cycle_key(&task.account_id, &record_ids, true)
            .await?;
        if !matches!(
            store.upsert_detected_todo(&task).await?,
            TodoUpsert::Unchanged
        ) {
            changed += 1;
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests;
