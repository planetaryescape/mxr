//! A yearly subscription's next charge, as a renewal to-do.
//!
//! To do surfaces a renewal 14 days before its date (the lead-time table)
//! so there is time to compare quotes. A renewal email does that already;
//! many yearly charges come with no notice, so the charge history stands
//! in for one. The to-do is built from the dates of past charges, never
//! from anything an email asks for, and carries no action link.

use super::{Cadence, Loaded, Status, Subscription};
use crate::fields::day_at;
use chrono::{DateTime, TimeZone, Utc};
use mxr_store::{Store, TodoRecord, TodoState};
use mxr_todo::action_link::registrable_domain;
use mxr_todo::money::format_amount;
use mxr_todo::pass::new_todo_id;
use mxr_todo::provenance::{FieldProvenance, FieldSource, FieldSources};
use mxr_todo::timing::{lead_time, window, TimingInput};
use mxr_todo::{TodoKind, RULES_VERSION};

/// A renewal to-do from mail within this many days of the expected
/// charge is the same renewal: the email wins and none is added.
const SAME_RENEWAL_DAYS: i64 = 30;

/// The dedup key: one per subscription per renewal date, so ticking it
/// off or dismissing it holds until next year's. It names the issuer,
/// product and currency rather than the subscription's id, which moves
/// while the first run files older charges.
pub fn renewal_key(subscription: &Subscription, due: DateTime<Utc>) -> String {
    format!(
        "renewal|subscription|{}|{}|{}|due:{}",
        subscription.issuer_key,
        subscription.product_key,
        subscription.currency.as_deref().unwrap_or_default(),
        due.format("%Y-%m-%d")
    )
}

/// Adds or refreshes the renewal to-do of each yearly subscription whose
/// next charge is within To do's lead time. Returns how many it wrote.
pub async fn file_renewals<Tz>(
    store: &Store,
    loaded: &Loaded,
    now: DateTime<Utc>,
    tz: &Tz,
    morning_hour: u8,
) -> anyhow::Result<u32>
where
    Tz: TimeZone,
{
    let mut written = 0;
    for subscription in &loaded.detection.subscriptions {
        if subscription.cadence != Cadence::Yearly || subscription.status != Status::Active {
            continue;
        }
        let Some(next) = subscription.next_expected else {
            continue;
        };
        let due = day_at(next);
        let last_id = &subscription.last_charge().record_ids[0];
        let Some(record) = loaded.records.get(last_id) else {
            continue;
        };
        let input = TimingInput {
            kind: TodoKind::Renewal,
            doc_type: None,
            arrived: now,
            due: Some(due),
            event_start: None,
        };
        let timing = lead_time(&input, tz, morning_hour);
        // Not yet: To do would hold it until its surface time anyway, and a
        // year-ahead row is noise in every list that counts open to-dos.
        if timing.act_by.is_none() || timing.surface_at.is_some_and(|at| at > now) {
            continue;
        }
        let horizon = window(&input);
        if horizon.until.is_some_and(|until| until < now) {
            continue;
        }
        let Some(source) = store
            .archive_record_sources(std::slice::from_ref(last_id))
            .await?
            .into_iter()
            .max_by(|a, b| a.message_at.cmp(&b.message_at))
        else {
            continue;
        };
        let domain = store
            .record_sender_emails(std::slice::from_ref(last_id))
            .await?
            .into_iter()
            .find_map(|(_, sender)| {
                registrable_domain(sender.rsplit('@').next().unwrap_or_default())
            });
        if let Some(domain) = &domain {
            let from_mail = store
                .list_open_todos_for_domain(&record.account_id, domain)
                .await?
                .into_iter()
                .any(|todo| {
                    todo.kind == TodoKind::Renewal.as_str()
                        && !todo.dedup_key.starts_with("renewal|subscription|")
                        && todo
                            .due_at
                            .is_some_and(|at| (at - due).num_days().abs() <= SAME_RENEWAL_DAYS)
                });
            if from_mail {
                continue;
            }
        }
        let title = subscription.title();
        let last_day = subscription.last_charge().day.format("%-d %b %Y");
        let mut fields = FieldSources::default();
        fields.set(
            "due_at",
            FieldProvenance::with_evidence(
                FieldSource::Rule,
                format!("a year after the last charge on {last_day}"),
            )
            .unchecked(),
        );
        if let (Some(minor), Some(currency)) = (subscription.amount_minor, &subscription.currency) {
            let amount = FieldProvenance::with_evidence(
                FieldSource::Rule,
                format!("the last charge, {}", format_amount(minor, currency)),
            );
            fields.set(
                "amount",
                if subscription.amount_checked {
                    amount
                } else {
                    amount.unchecked()
                },
            );
        }
        if timing.act_by.is_some() {
            fields.set(
                "act_by_at",
                FieldProvenance::with_evidence(FieldSource::Table, timing.act_by_rule),
            );
        }
        fields.set(
            "surface_at",
            FieldProvenance::with_evidence(FieldSource::Table, timing.surface_rule),
        );
        if horizon.until.is_some() {
            fields.set(
                "relevant_until",
                FieldProvenance::with_evidence(FieldSource::Table, horizon.rule),
            );
        }
        let todo = TodoRecord {
            id: new_todo_id(),
            account_id: record.account_id.clone(),
            thread_id: source.thread_id.clone(),
            source_message_id: Some(source.message_id.clone()),
            source_date: Some(source.message_at),
            kind: TodoKind::Renewal.as_str().to_string(),
            verb: TodoKind::Renewal.default_verb().to_string(),
            doc_type: None,
            title: format!("{title}, renews around {}", next.format("%-d %b")),
            counterparty: Some(subscription.issuer.clone()),
            sender_domain: domain,
            amount_minor: subscription.amount_minor,
            currency: subscription.currency.clone(),
            due_at: Some(due),
            due_words: Some(format!("expected around {}", next.format("%-d %b %Y"))),
            act_by_at: timing.act_by,
            surface_at: timing.surface_at,
            scheduled_for: None,
            action_url: None,
            action_domain: None,
            relevant_until: horizon.until,
            window_source: horizon.until.map(|_| "rule".to_string()),
            state: TodoState::Open,
            expired_at: None,
            expired_at_birth: false,
            catchup: None,
            looks_done_message_id: None,
            looks_done_reason: None,
            origin: "rule".to_string(),
            reason: format!(
                "charged once a year by {}, last on {last_day}; worked out from your records",
                subscription.issuer
            ),
            field_sources: fields.to_json(),
            user_edited: false,
            commitment_id: None,
            rules_version: RULES_VERSION,
            dedup_key: renewal_key(subscription, due),
            surfaced_at: None,
            created_at: now,
            updated_at: now,
            done_at: None,
            dismissed_at: None,
        };
        if !matches!(
            store.upsert_detected_todo(&todo).await?,
            mxr_store::TodoUpsert::Unchanged
        ) {
            written += 1;
        }
    }
    Ok(written)
}
