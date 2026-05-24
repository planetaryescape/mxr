//! Deliveries: IPC handlers (list/get/resolve/dismiss/scan) plus the scan
//! engine shared by the post-sync fan-out and the `mxr deliveries scan`
//! backfill. Detection is local (`mxr_deliveries::detect`); the LLM confirm
//! step runs only on shortlisted candidates when an LLM is configured.

use super::HandlerResult;
use crate::state::AppState;
use chrono::{Duration, Utc};
use mxr_core::id::{AccountId, DeliveryId, MessageId};
use mxr_core::types::UnsubscribeMethod;
use mxr_deliveries::lifecycle::ApplyOutcome;
use mxr_deliveries::{detect, extract, lifecycle, Decision, DeliverySignal, DetectInput};
use mxr_llm::LlmFeature;
use mxr_protocol::{DeliveryData, DeliveryItemData, DeliveryScanSummary, ResponseData};
use mxr_reader::{clean, ReaderConfig};
use mxr_store::{Delivery, DeliveryListFilter};

// ---------------------------------------------------------------------------
// IPC handlers
// ---------------------------------------------------------------------------

pub(super) async fn list_deliveries(state: &AppState, filter: Option<&str>) -> HandlerResult {
    let rows = state
        .store
        .list_deliveries(parse_filter(filter))
        .await
        ?;
    let deliveries = rows.into_iter().map(to_data).collect();
    Ok(ResponseData::Deliveries { deliveries })
}

pub(super) async fn get_delivery(state: &AppState, delivery_id: &DeliveryId) -> HandlerResult {
    let row = state
        .store
        .get_delivery(delivery_id)
        .await
        ?
        .ok_or_else(|| "delivery not found".to_string())?;
    let mut data = to_data(row);
    data.message_ids = state
        .store
        .delivery_message_ids(delivery_id)
        .await
        ?;
    Ok(ResponseData::Delivery { delivery: data })
}

pub(super) async fn resolve_delivery(state: &AppState, delivery_id: &DeliveryId) -> HandlerResult {
    state
        .store
        .resolve_delivery(delivery_id, Utc::now())
        .await
        ?;
    let row = state
        .store
        .get_delivery(delivery_id)
        .await
        ?
        .ok_or_else(|| "delivery not found".to_string())?;
    Ok(ResponseData::Delivery {
        delivery: to_data(row),
    })
}

pub(super) async fn dismiss_delivery(state: &AppState, delivery_id: &DeliveryId) -> HandlerResult {
    state
        .store
        .dismiss_delivery(delivery_id, Utc::now())
        .await
        ?;
    Ok(ResponseData::Ack)
}

pub(super) async fn scan_deliveries(
    state: &AppState,
    since_days: Option<u32>,
    dry_run: bool,
) -> HandlerResult {
    let summary = scan_recent(state, since_days.unwrap_or(90), dry_run).await?;
    Ok(ResponseData::DeliveryScan { summary })
}

// ---------------------------------------------------------------------------
// Scan engine
// ---------------------------------------------------------------------------

/// Scan newly-upserted messages during the post-sync fan-out. Respects
/// `[deliveries].enabled`. Never errors the caller — failures are logged.
pub(crate) async fn scan_messages(
    state: &AppState,
    message_ids: &[MessageId],
) -> DeliveryScanSummary {
    let cfg = state.config_snapshot();
    let mut summary = DeliveryScanSummary::default();
    if !cfg.deliveries.enabled || message_ids.is_empty() {
        return summary;
    }
    let llm_enabled = cfg.llm.enabled;
    for message_id in message_ids {
        match scan_one(state, message_id, llm_enabled, false).await {
            Ok(outcome) => summary_add(&mut summary, outcome),
            Err(error) => {
                tracing::warn!(message = %message_id, %error, "delivery scan failed");
            }
        }
    }
    summary
}

/// Backfill scan over mail received within `since_days`. Explicit user action,
/// so it runs regardless of `[deliveries].enabled`. `dry_run` previews without
/// calling the LLM or writing.
async fn scan_recent(
    state: &AppState,
    since_days: u32,
    dry_run: bool,
) -> Result<DeliveryScanSummary, String> {
    let cfg = state.config_snapshot();
    let llm_enabled = cfg.llm.enabled && !dry_run;
    let since = Utc::now() - Duration::days(i64::from(since_days));
    let ids = state
        .store
        .list_message_ids_since(since)
        .await
        .map_err(|e| e.to_string())?;
    let mut summary = DeliveryScanSummary {
        dry_run,
        ..Default::default()
    };
    for message_id in &ids {
        match scan_one(state, message_id, llm_enabled, dry_run).await {
            Ok(outcome) => summary_add(&mut summary, outcome),
            Err(error) => {
                tracing::warn!(message = %message_id, %error, "delivery backfill scan failed");
            }
        }
    }
    Ok(summary)
}

#[derive(Default)]
struct ScanOutcome {
    scanned: u32,
    created: u32,
    updated: u32,
    shortlisted: u32,
}

async fn scan_one(
    state: &AppState,
    message_id: &MessageId,
    llm_enabled: bool,
    dry_run: bool,
) -> Result<ScanOutcome, String> {
    let mut outcome = ScanOutcome::default();
    let Some(envelope) = state
        .store
        .get_envelope(message_id)
        .await
        .map_err(|e| e.to_string())?
    else {
        return Ok(outcome);
    };
    outcome.scanned = 1;

    let body = state
        .store
        .get_body(message_id)
        .await
        .map_err(|e| e.to_string())?;
    let cleaned = clean(
        body.as_ref().and_then(|b| b.text_plain.as_deref()),
        body.as_ref().and_then(|b| b.text_html.as_deref()),
        &ReaderConfig::default(),
    )
    .content;
    let from_domain = domain_of(&envelope.from.email);
    let from_name = envelope.from.name.as_deref().unwrap_or("");

    let mut signal = detect(&DetectInput {
        from_name,
        from_domain: &from_domain,
        subject: &envelope.subject,
        body_text: &cleaned,
        body_html: body.as_ref().and_then(|b| b.text_html.as_deref()),
        link_count: envelope.link_count,
        body_word_count: envelope.body_word_count,
        has_unsubscribe: !matches!(envelope.unsubscribe, UnsubscribeMethod::None),
    });

    // Resolve a shortlist candidate: LLM confirm (live only), else degrade.
    if signal.decision == Decision::ShortlistLlm {
        if dry_run {
            outcome.shortlisted = 1;
            return Ok(outcome);
        }
        if llm_enabled {
            let runtime = state.llm.for_feature(LlmFeature::DeliveryExtraction);
            signal = extract::enrich(
                &runtime,
                &extract::LlmInput {
                    from_name,
                    from_domain: &from_domain,
                    subject: &envelope.subject,
                    body_text: &cleaned,
                },
                signal,
            )
            .await;
        }
        // If still shortlisted (LLM off/blocked/errored), degrade to
        // heuristic-only: keep only when a checksum-valid tracking # exists.
        if signal.decision == Decision::ShortlistLlm {
            signal.decision = degrade(&signal);
        }
    }

    match signal.decision {
        Decision::Create => {
            if dry_run {
                if lookup_exists(state, &envelope.account_id, &signal).await? {
                    outcome.updated = 1;
                } else {
                    outcome.created = 1;
                }
            } else {
                match lifecycle::apply(
                    &state.store,
                    &envelope.account_id,
                    message_id,
                    Some(&envelope.thread_id),
                    &signal,
                    Utc::now(),
                )
                .await
                .map_err(|e| e.to_string())?
                {
                    ApplyOutcome::Created(_) => outcome.created = 1,
                    ApplyOutcome::Updated(_) => outcome.updated = 1,
                    ApplyOutcome::Skipped => {}
                }
            }
        }
        Decision::ShortlistLlm => outcome.shortlisted = 1,
        Decision::Reject => {}
    }
    Ok(outcome)
}

/// Heuristic-only resolution of a shortlist candidate (no LLM available):
/// create only when there's both a tracking number AND a real shipping context
/// (carrier/merchant sender, shipping subject, or schema.org). Without LLM
/// confirmation, a bare tracking-number match from an unrelated email is
/// almost always a false positive (loose formats match codes/hashes).
fn degrade(signal: &DeliverySignal) -> Decision {
    if signal.primary_tracking_number().is_some()
        && mxr_deliveries::is_corroborated(&signal.matched_signals)
    {
        Decision::Create
    } else {
        Decision::Reject
    }
}

async fn lookup_exists(
    state: &AppState,
    account_id: &AccountId,
    signal: &DeliverySignal,
) -> Result<bool, String> {
    let Some(key) = signal.dedup_key.as_deref() else {
        return Ok(false);
    };
    Ok(state
        .store
        .get_delivery_by_dedup(account_id, key)
        .await
        .map_err(|e| e.to_string())?
        .is_some())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn summary_add(summary: &mut DeliveryScanSummary, outcome: ScanOutcome) {
    summary.scanned += outcome.scanned;
    summary.created += outcome.created;
    summary.updated += outcome.updated;
    summary.shortlisted += outcome.shortlisted;
}

fn parse_filter(filter: Option<&str>) -> DeliveryListFilter {
    match filter.unwrap_or("active") {
        "delivered" => DeliveryListFilter::Delivered,
        "all" => DeliveryListFilter::All,
        "dismissed" => DeliveryListFilter::Dismissed,
        _ => DeliveryListFilter::Active,
    }
}

fn domain_of(email: &str) -> String {
    email.rsplit('@').next().unwrap_or("").trim().to_lowercase()
}

fn to_data(d: Delivery) -> DeliveryData {
    DeliveryData {
        id: d.id,
        account_id: d.account_id,
        merchant: d.merchant,
        carrier: d.carrier,
        tracking_number: d.tracking_number,
        tracking_url: d.tracking_url,
        order_number: d.order_number,
        status: d.status,
        eta_from: d.eta_from,
        eta_until: d.eta_until,
        delivered_at: d.delivered_at,
        items: d
            .items
            .into_iter()
            .map(|i| DeliveryItemData {
                name: i.name,
                quantity: i.quantity,
            })
            .collect(),
        confidence: d.confidence,
        source: d.source,
        thread_id: d.thread_id,
        last_event_at: d.last_event_at,
        created_at: d.created_at,
        updated_at: d.updated_at,
        resolved_at: d.resolved_at,
        dismissed_at: d.dismissed_at,
        message_ids: Vec::new(),
    }
}
