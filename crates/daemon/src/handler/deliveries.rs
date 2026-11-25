use super::HandlerResult;
use crate::state::AppState;
use chrono::{Duration, Utc};
use mxr_core::id::{AccountId, DeliveryId, MessageId};
use mxr_core::types::{MessageFlags, UnsubscribeMethod};
use mxr_deliveries::lifecycle::ApplyOutcome;
use mxr_deliveries::{detect, extract, lifecycle, Decision, DeliverySignal, DetectInput};
use mxr_llm::{BackfillHeldJob, LlmFeature};
use mxr_protocol::{DeliveryData, DeliveryItemData, DeliveryScanSummary, ResponseData};
use mxr_reader::{clean, ReaderConfig};
use mxr_store::{Delivery, DeliveryListFilter};

pub(super) async fn list_deliveries(
    state: &AppState,
    account_id: Option<&AccountId>,
    filter: Option<&str>,
) -> HandlerResult {
    let wanted = parse_filter(filter);
    let rows = state.store.list_deliveries(wanted).await?;
    let now = Utc::now();
    let deliveries = rows
        .into_iter()
        .filter(|row| account_id.is_none_or(|account_id| row.account_id == *account_id))
        .filter(|row| {
            wanted != DeliveryListFilter::Active
                || super::updates_digest::parcel_quiet_since(row).is_none_or(|quiet| quiet > now)
        })
        .map(to_data)
        .collect();
    Ok(ResponseData::Deliveries { deliveries })
}

pub(super) async fn get_delivery(state: &AppState, delivery_id: &DeliveryId) -> HandlerResult {
    let row = state
        .store
        .get_delivery(delivery_id)
        .await?
        .ok_or_else(|| "delivery not found".to_string())?;
    let mut data = to_data(row);
    data.message_ids = state.store.delivery_message_ids(delivery_id).await?;
    Ok(ResponseData::Delivery { delivery: data })
}

pub(super) async fn resolve_delivery(state: &AppState, delivery_id: &DeliveryId) -> HandlerResult {
    state
        .store
        .resolve_delivery(delivery_id, Utc::now())
        .await?;
    let row = state
        .store
        .get_delivery(delivery_id)
        .await?
        .ok_or_else(|| "delivery not found".to_string())?;
    Ok(ResponseData::Delivery {
        delivery: to_data(row),
    })
}

pub(super) async fn dismiss_delivery(state: &AppState, delivery_id: &DeliveryId) -> HandlerResult {
    state
        .store
        .dismiss_delivery(delivery_id, Utc::now())
        .await?;
    Ok(ResponseData::Ack)
}

pub(super) async fn scan_deliveries(
    state: &AppState,
    account_id: Option<&AccountId>,
    since_days: Option<u32>,
    dry_run: bool,
) -> HandlerResult {
    let summary = scan_recent(state, account_id, since_days.unwrap_or(90), dry_run).await?;
    Ok(ResponseData::DeliveryScan { summary })
}

/// Scan newly synced mail. Only mail that arrived live goes to the model;
/// backfilled mail gets the heuristic alone until `mxr deliveries scan`.
pub(crate) async fn scan_messages(
    state: &AppState,
    message_ids: &[MessageId],
) -> DeliveryScanSummary {
    scan_synced(state, message_ids, LlmScope::LiveOnly).await
}

/// Scan again mail an endpoint outage put off. Each was already allowed to
/// reach the model, by arriving live or by an explicit scan.
pub(crate) async fn rescan_put_off(
    state: &AppState,
    message_ids: &[MessageId],
) -> DeliveryScanSummary {
    scan_synced(state, message_ids, LlmScope::Any).await
}

/// Which mail a scan may send to the model.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LlmScope {
    LiveOnly,
    Any,
}

async fn scan_synced(
    state: &AppState,
    message_ids: &[MessageId],
    scope: LlmScope,
) -> DeliveryScanSummary {
    let cfg = state.config_snapshot();
    let mut summary = DeliveryScanSummary::default();
    if !cfg.deliveries.enabled || message_ids.is_empty() {
        return summary;
    }
    let llm_enabled = cfg.llm.enabled;
    for message_id in message_ids {
        match scan_one(state, message_id, llm_enabled, false, false, scope).await {
            Ok(outcome) => summary_add(&mut summary, outcome),
            Err(error) => {
                tracing::warn!(message = %message_id, %error, "delivery scan failed");
            }
        }
    }
    summary
}

async fn scan_recent(
    state: &AppState,
    account_id: Option<&AccountId>,
    since_days: u32,
    dry_run: bool,
) -> Result<DeliveryScanSummary, String> {
    let cfg = state.config_snapshot();
    let llm_enabled = cfg.llm.enabled;
    let since = Utc::now() - Duration::days(i64::from(since_days));
    let ids = state
        .store
        .list_message_ids_since(since, account_id)
        .await
        .map_err(|e| e.to_string())?;
    let mut summary = DeliveryScanSummary {
        dry_run,
        ..Default::default()
    };
    for message_id in &ids {
        match scan_one(state, message_id, llm_enabled, dry_run, true, LlmScope::Any).await {
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
    model_calls: u32,
}

async fn scan_one(
    state: &AppState,
    message_id: &MessageId,
    llm_enabled: bool,
    dry_run: bool,
    force_llm: bool,
    scope: LlmScope,
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

    if envelope
        .flags
        .intersects(MessageFlags::SPAM | MessageFlags::TRASH)
    {
        return Ok(outcome);
    }

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

    if signal.decision == Decision::ShortlistLlm {
        if dry_run {
            outcome.shortlisted = 1;
            outcome.model_calls = u32::from(llm_enabled);
            return Ok(outcome);
        }
        let held = llm_enabled
            && scope == LlmScope::LiveOnly
            && !state
                .store
                .message_arrived_live(message_id)
                .await
                .map_err(|e| e.to_string())?;
        if held {
            state
                .llm
                .record_backfill_hold(BackfillHeldJob::DeliveryExtraction, &message_id.as_str());
        } else if llm_enabled {
            let runtime = state.llm.for_feature(LlmFeature::DeliveryExtraction);
            let enriched = extract::enrich(
                &runtime,
                &message_id.as_str(),
                force_llm,
                &extract::LlmInput {
                    from_name,
                    from_domain: &from_domain,
                    subject: &envelope.subject,
                    body_text: &cleaned,
                    received_at: envelope.date,
                    time_prefs: state.snooze_time_prefs(),
                },
                signal,
            )
            .await;
            signal = enriched.signal;
            outcome.model_calls = u32::from(enriched.sent);
        }
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

fn summary_add(summary: &mut DeliveryScanSummary, outcome: ScanOutcome) {
    summary.scanned += outcome.scanned;
    summary.created += outcome.created;
    summary.updated += outcome.updated;
    summary.shortlisted += outcome.shortlisted;
    summary.model_calls += outcome.model_calls;
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
