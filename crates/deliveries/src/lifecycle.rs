//! Lifecycle: collapse the many emails of one shipment into a single delivery
//! row. Correlates by tracking number, then merchant+order; advances status
//! monotonically (never regressing a terminal row); resolves on delivered.
//!
//! The DB orchestration is [`apply`]; the merge/build math is pure
//! ([`merge_into`], [`build_new`]) and unit-tested without a database.

use crate::{compute_dedup_key, Decision, DeliverySignal, DeliveryStatus};
use chrono::{DateTime, Utc};
use mxr_core::id::{AccountId, DeliveryId, MessageId, ThreadId};
use mxr_store::{Delivery, Store};

/// What [`apply`] did with a signal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyOutcome {
    /// A new delivery row was inserted.
    Created(DeliveryId),
    /// An existing delivery was merged/advanced.
    Updated(DeliveryId),
    /// Nothing persisted (not a `Create`, or no correlation key).
    Skipped,
}

impl ApplyOutcome {
    pub fn id(&self) -> Option<&DeliveryId> {
        match self {
            Self::Created(id) | Self::Updated(id) => Some(id),
            Self::Skipped => None,
        }
    }
}

/// Apply a Stage-1/2 signal for one message: insert a new delivery or merge
/// into the correlated existing one, then link provenance.
pub async fn apply(
    store: &Store,
    account_id: &AccountId,
    message_id: &MessageId,
    thread_id: Option<&ThreadId>,
    signal: &DeliverySignal,
    now: DateTime<Utc>,
) -> anyhow::Result<ApplyOutcome> {
    if signal.decision != Decision::Create {
        return Ok(ApplyOutcome::Skipped);
    }
    let Some(primary) = signal.dedup_key.clone() else {
        return Ok(ApplyOutcome::Skipped);
    };
    let stage = signal.stage.unwrap_or(DeliveryStatus::InTransit);

    // Correlate: try the primary key, then the order-only key — so a shipped
    // email carrying a tracking number merges into the earlier order row.
    let mut existing = store.get_delivery_by_dedup(account_id, &primary).await?;
    if existing.is_none() {
        if let Some(secondary) = compute_dedup_key(
            None,
            signal.merchant.as_deref(),
            signal.order_number.as_deref(),
        ) {
            if secondary != primary {
                existing = store.get_delivery_by_dedup(account_id, &secondary).await?;
            }
        }
    }

    let (id, created) = match existing {
        Some(mut d) => {
            merge_into(&mut d, signal, stage, &primary, thread_id, now);
            store.update_delivery(&d).await?;
            (d.id, false)
        }
        None => {
            let d = build_new(account_id, primary, signal, stage, thread_id, now);
            let id = d.id.clone();
            store.insert_delivery(&d).await?;
            (id, true)
        }
    };

    store
        .link_delivery_message(&id, message_id, thread_id, Some(stage.as_str()), now)
        .await?;
    Ok(if created {
        ApplyOutcome::Created(id)
    } else {
        ApplyOutcome::Updated(id)
    })
}

/// Build a fresh delivery row from a signal.
pub fn build_new(
    account_id: &AccountId,
    dedup_key: String,
    signal: &DeliverySignal,
    stage: DeliveryStatus,
    thread_id: Option<&ThreadId>,
    now: DateTime<Utc>,
) -> Delivery {
    let delivered = stage == DeliveryStatus::Delivered;
    Delivery {
        id: DeliveryId::new(),
        account_id: account_id.clone(),
        dedup_key,
        merchant: signal.merchant.clone(),
        carrier: signal.carrier.clone(),
        tracking_number: signal.primary_tracking_number(),
        tracking_url: signal.tracking_url.clone(),
        order_number: signal.order_number.clone(),
        status: stage.as_str().to_string(),
        eta_from: signal.eta_from,
        eta_until: signal.eta_until,
        delivered_at: delivered.then_some(now),
        items: signal.items.clone(),
        confidence: signal.confidence as f64,
        source: signal.source.as_str().to_string(),
        thread_id: thread_id.cloned(),
        last_event_at: now,
        created_at: now,
        updated_at: now,
        resolved_at: delivered.then_some(now),
        dismissed_at: None,
    }
}

/// Merge a signal into an existing row: monotonic status, gap-fill fields,
/// dedup-key migration, ETA refresh.
pub fn merge_into(
    d: &mut Delivery,
    signal: &DeliverySignal,
    stage: DeliveryStatus,
    primary_key: &str,
    thread_id: Option<&ThreadId>,
    now: DateTime<Utc>,
) {
    // Status advances forward only; a terminal row never regresses.
    let current = DeliveryStatus::parse(&d.status);
    let advance = match current {
        Some(c) if c.is_terminal() => false,
        Some(c) => stage.rank() >= c.rank(),
        None => true,
    };
    if advance {
        d.status = stage.as_str().to_string();
        if stage == DeliveryStatus::Delivered {
            d.delivered_at.get_or_insert(now);
            d.resolved_at.get_or_insert(now);
        }
    }

    // Migrate the key (order-only → tracking) and fill any gaps.
    d.dedup_key = primary_key.to_string();
    fill(&mut d.carrier, &signal.carrier);
    if d.tracking_number.is_none() {
        d.tracking_number = signal.primary_tracking_number();
    }
    fill(&mut d.tracking_url, &signal.tracking_url);
    fill(&mut d.order_number, &signal.order_number);
    fill(&mut d.merchant, &signal.merchant);
    if signal.eta_from.is_some() {
        d.eta_from = signal.eta_from;
    }
    if signal.eta_until.is_some() {
        d.eta_until = signal.eta_until;
    }
    if d.items.is_empty() && !signal.items.is_empty() {
        d.items = signal.items.clone();
    }
    if let Some(t) = thread_id {
        d.thread_id = Some(t.clone());
    }
    let conf = signal.confidence as f64;
    if conf > d.confidence {
        d.confidence = conf;
    }
    d.last_event_at = now;
    d.updated_at = now;
}

fn fill(slot: &mut Option<String>, value: &Option<String>) {
    if slot.is_none() {
        if let Some(v) = value {
            if !v.trim().is_empty() {
                *slot = Some(v.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DetectionSource, ValidatedTracking};
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2024, 5, 7, 12, 0, 0).unwrap()
    }

    fn signal(
        stage: DeliveryStatus,
        tracking: Option<&str>,
        merchant: Option<&str>,
        order: Option<&str>,
    ) -> DeliverySignal {
        let tracking_numbers = tracking
            .map(|t| {
                vec![ValidatedTracking {
                    number: t.to_string(),
                    carrier: "ups".into(),
                    tracking_url: None,
                }]
            })
            .unwrap_or_default();
        DeliverySignal {
            decision: Decision::Create,
            confidence: 0.9,
            source: DetectionSource::Heuristic,
            stage: Some(stage),
            carrier: Some("ups".into()),
            tracking_numbers,
            tracking_url: None,
            order_number: order.map(str::to_string),
            merchant: merchant.map(str::to_string),
            eta_from: None,
            eta_until: None,
            items: vec![],
            dedup_key: compute_dedup_key(tracking, merchant, order),
            post_delivery_noise: false,
            matched_signals: vec![],
        }
    }

    fn account() -> AccountId {
        AccountId::new()
    }

    #[test]
    fn build_new_marks_delivered_resolved() {
        let acct = account();
        let s = signal(DeliveryStatus::Delivered, Some("1Z1"), None, None);
        let d = build_new(
            &acct,
            "1Z1".into(),
            &s,
            DeliveryStatus::Delivered,
            None,
            now(),
        );
        assert_eq!(d.status, "delivered");
        assert_eq!(d.delivered_at, Some(now()));
        assert_eq!(d.resolved_at, Some(now()));
    }

    #[test]
    fn merge_advances_status_forward() {
        let acct = account();
        let ordered = signal(DeliveryStatus::Ordered, None, Some("Acme"), Some("A1"));
        let mut d = build_new(
            &acct,
            "acme|a1".into(),
            &ordered,
            DeliveryStatus::Ordered,
            None,
            now(),
        );

        // Shipped email gains a tracking number → advance + migrate key.
        let shipped = signal(
            DeliveryStatus::InTransit,
            Some("1Z9"),
            Some("Acme"),
            Some("A1"),
        );
        merge_into(
            &mut d,
            &shipped,
            DeliveryStatus::InTransit,
            "1Z9",
            None,
            now(),
        );
        assert_eq!(d.status, "in_transit");
        assert_eq!(d.dedup_key, "1Z9");
        assert_eq!(d.tracking_number.as_deref(), Some("1Z9"));

        // Delivered email closes it out.
        let delivered = signal(
            DeliveryStatus::Delivered,
            Some("1Z9"),
            Some("Acme"),
            Some("A1"),
        );
        merge_into(
            &mut d,
            &delivered,
            DeliveryStatus::Delivered,
            "1Z9",
            None,
            now(),
        );
        assert_eq!(d.status, "delivered");
        assert!(d.delivered_at.is_some());
    }

    #[test]
    fn merge_never_regresses_terminal() {
        let acct = account();
        let delivered = signal(DeliveryStatus::Delivered, Some("1Z9"), None, None);
        let mut d = build_new(
            &acct,
            "1Z9".into(),
            &delivered,
            DeliveryStatus::Delivered,
            None,
            now(),
        );

        // A late, stale "in transit" email must not reopen it.
        let stale = signal(DeliveryStatus::InTransit, Some("1Z9"), None, None);
        merge_into(
            &mut d,
            &stale,
            DeliveryStatus::InTransit,
            "1Z9",
            None,
            now(),
        );
        assert_eq!(d.status, "delivered", "terminal row stays delivered");
    }
}
