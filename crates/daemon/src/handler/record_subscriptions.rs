//! Archive's subscriptions: IPC, the signals on Now and Archive, and the
//! renewal to-do a yearly subscription leaves in To do.
//!
//! Detection lives in `mxr_records::subscriptions` and runs over the
//! records on every call, so nothing here stores state: correcting a
//! record's amount or date changes its subscription on the next read.

use super::places::scoped_accounts;
use super::records::amount_data as amount;
use super::{HandlerError, HandlerResult};
use crate::state::AppState;
use chrono::{DateTime, Local, NaiveDate, Utc};
use mxr_core::id::{AccountId, MessageId};
use mxr_protocol::{
    subscription_copy, RecordFieldData, RecordMomentData, RecordPriceChangeData,
    RecordSubscriptionChargeData, RecordSubscriptionData, RecordSubscriptionSignalData,
    RecordSubscriptionTotalData, RecordSubscriptionsData, ResponseData,
};
use mxr_records::fields::day_at;
use mxr_records::subscriptions::{self, Cadence, Loaded, Signal, Status, Subscription};
use mxr_records::Source;
use mxr_store::{ArchiveRecord, RecordFieldValue};
use mxr_todo::money::{format_amount, plain_amount};
use std::collections::HashMap;

pub(super) async fn list(state: &AppState, account_id: Option<&AccountId>) -> HandlerResult {
    let now = Utc::now();
    let accounts = scoped_accounts(state, account_id).await?;
    let started = std::time::Instant::now();
    let loaded = load(state, &accounts, now).await?;
    let data = to_data(state, &loaded).await?;
    tracing::debug!(
        subscriptions = data.subscriptions.len(),
        elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
        "record subscriptions composed"
    );
    Ok(ResponseData::RecordSubscriptions {
        subscriptions: data,
    })
}

async fn load(
    state: &AppState,
    accounts: &[AccountId],
    now: DateTime<Utc>,
) -> Result<Loaded, HandlerError> {
    subscriptions::load(&state.store, Some(accounts), now, &Local)
        .await
        .map_err(|error| HandlerError::Message(error.to_string()))
}

async fn to_data(
    state: &AppState,
    loaded: &Loaded,
) -> Result<RecordSubscriptionsData, HandlerError> {
    let found = &loaded.detection.subscriptions;
    // The first and newest charge's records give the fields their
    // provenance: where the amount, the issuer and the dates were read.
    let mut ids: Vec<String> = Vec::new();
    for subscription in found {
        ids.push(subscription.charges[0].record_ids[0].clone());
        ids.push(subscription.last_charge().record_ids[0].clone());
    }
    ids.sort();
    ids.dedup();
    let mut winners: HashMap<(String, String), RecordFieldValue> = HashMap::new();
    for (record_id, value) in state.store.archive_record_fields(&ids).await? {
        // The store orders candidates best first.
        winners
            .entry((record_id, value.field.clone()))
            .or_insert(value);
    }
    let last_ids: Vec<String> = found
        .iter()
        .map(|s| s.last_charge().record_ids[0].clone())
        .collect();
    let mut newest: HashMap<String, (DateTime<Utc>, MessageId)> = HashMap::new();
    for source in state.store.archive_record_sources(&last_ids).await? {
        let entry = newest
            .entry(source.record_id.clone())
            .or_insert((source.message_at, source.message_id.clone()));
        if source.message_at > entry.0 {
            *entry = (source.message_at, source.message_id);
        }
    }
    let rows: Vec<RecordSubscriptionData> = found
        .iter()
        .filter_map(|subscription| {
            let mut data = subscription_data(subscription, loaded, &winners)?;
            data.message_id = newest.get(&data.record_id).map(|n| n.1.clone());
            Some(data)
        })
        .collect();
    let live = found.iter().filter(|s| s.status != Status::Ended).count();
    let ended = found.len() - live;
    let empty_state = if found.is_empty() {
        Some(subscription_copy::NONE_YET.to_string())
    } else if live == 0 {
        Some(subscription_copy::ALL_ENDED.to_string())
    } else {
        None
    };
    Ok(RecordSubscriptionsData {
        header: subscription_copy::HEADER.to_string(),
        totals: subscriptions::totals(found)
            .into_iter()
            .map(|(currency, month, year)| RecordSubscriptionTotalData {
                per_month: amount(month, &currency),
                per_year: amount(year, &currency),
                currency,
            })
            .collect(),
        signals: signal_data(found),
        subscriptions: rows,
        live: count(live),
        ended: count(ended),
        empty_state,
    })
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn day_label(day: NaiveDate) -> String {
    day.format("%-d %b %Y").to_string()
}

fn cadence_label(cadence: Cadence) -> &'static str {
    match cadence {
        Cadence::Weekly => "Weekly",
        Cadence::Monthly => "Monthly",
        Cadence::Quarterly => "Quarterly",
        Cadence::Yearly => "Yearly",
    }
}

fn subscription_data(
    subscription: &Subscription,
    loaded: &Loaded,
    winners: &HashMap<(String, String), RecordFieldValue>,
) -> Option<RecordSubscriptionData> {
    let currency = subscription.currency.as_deref();
    let money = |minor: i64| currency.map(|c| amount(minor, c));
    let last = subscription.last_charge();
    let last_id = &last.record_ids[0];
    let last_record: &ArchiveRecord = loaded.records.get(last_id)?;
    let n = subscription.charges.len();
    let one_offs = loaded
        .detection
        .one_offs
        .get(&subscription.issuer_key)
        .map_or(0, Vec::len);
    let charges = subscription
        .charges
        .iter()
        .map(|charge| RecordSubscriptionChargeData {
            record_ids: charge.record_ids.clone(),
            date: charge.at(),
            amount: charge.amount_minor.and_then(money),
            checked: charge.checked,
        })
        .collect();
    let price_changes = subscription
        .price_changes
        .iter()
        .filter_map(|change| {
            let currency = currency?;
            Some(RecordPriceChangeData {
                date: day_at(change.day),
                record_id: change.record_id.clone(),
                from: amount(change.from_minor, currency),
                to: amount(change.to_minor, currency),
                label: format!(
                    "{} to {} on {}",
                    format_amount(change.from_minor, currency),
                    format_amount(change.to_minor, currency),
                    day_label(change.day)
                ),
            })
        })
        .collect();
    let record_ids = [&subscription.charges[0].record_ids[0], last_id];
    Some(RecordSubscriptionData {
        id: subscription.id.clone(),
        account_id: last_record.account_id.clone(),
        issuer: subscription.issuer.clone(),
        product: subscription.product.clone(),
        title: subscription.title(),
        cadence: subscription.cadence.as_str().to_string(),
        cadence_label: cadence_label(subscription.cadence).to_string(),
        amount: subscription.amount_minor.and_then(money),
        yearly_cost: subscription.yearly_cost_minor().and_then(money),
        start: day_at(subscription.start()),
        last_charge: last.at(),
        next_expected: subscription.next_expected.map(day_at),
        status: subscription.status.as_str().to_string(),
        status_reason: subscription.status_reason.clone(),
        confirmed: subscription.confirmed(),
        charge_count: count(n),
        charges,
        price_changes,
        fields: fields(subscription, record_ids, winners),
        one_offs: count(one_offs),
        record_id: last_id.clone(),
        thread_id: last_record.thread_id.clone(),
        message_id: None,
        why: format!(
            "Here because: {n} {} from {} about {} apart (worked out from your records).",
            if n == 1 { "charge" } else { "charges" },
            subscription.issuer,
            subscription.cadence.interval()
        ),
    })
}

/// Every field with where it came from. What a record said keeps that
/// record's provenance (a rule's amount stays unchecked); what was worked
/// out says how, and is checked only when everything it rests on is.
fn fields(
    subscription: &Subscription,
    [first_id, last_id]: [&String; 2],
    winners: &HashMap<(String, String), RecordFieldValue>,
) -> Vec<RecordFieldData> {
    let from_record =
        |record_id: &String, field: &str, label: &str, value: String, copy: String| {
            let winner = winners.get(&(record_id.clone(), field.to_string()));
            let source = winner.map_or(Source::Rule, |w| Source::parse(&w.source));
            RecordFieldData {
                field: field.to_string(),
                label: label.to_string(),
                value,
                copy,
                source: source.as_str().to_string(),
                source_label: source.describe().to_string(),
                checked: winner.is_some_and(|w| w.checked),
                evidence: winner.and_then(|w| w.evidence.clone()),
                message_id: winner.and_then(|w| w.message_id.clone()),
            }
        };
    let n = subscription.charges.len();
    let derived = |field: &str, label: &str, value: String, checked: bool, evidence: String| {
        RecordFieldData {
            field: field.to_string(),
            label: label.to_string(),
            copy: value.clone(),
            value,
            source: "derived".to_string(),
            source_label: format!("your {n} charges"),
            checked,
            evidence: Some(evidence),
            message_id: None,
        }
    };
    let cadence_checked = subscription.confirmed() && subscription.dates_checked();
    let mut out = vec![from_record(
        last_id,
        "issuer",
        "Issuer",
        subscription.issuer.clone(),
        subscription.issuer.clone(),
    )];
    if let Some(product) = &subscription.product {
        out.push(from_record(
            last_id,
            "title",
            "Plan",
            product.clone(),
            product.clone(),
        ));
    }
    out.push(derived(
        "cadence",
        "Every",
        subscription.cadence.interval().to_string(),
        cadence_checked,
        if subscription.confirmed() {
            format!(
                "{n} charges about {} apart",
                subscription.cadence.interval()
            )
        } else {
            format!(
                "seen twice, {} apart; unconfirmed until the next charge",
                subscription.cadence.interval()
            )
        },
    ));
    if let (Some(minor), Some(currency)) = (subscription.amount_minor, &subscription.currency) {
        out.push(from_record(
            last_id,
            "amount",
            "Amount",
            format_amount(minor, currency),
            plain_amount(minor),
        ));
    }
    if let (Some(yearly), Some(currency)) =
        (subscription.yearly_cost_minor(), &subscription.currency)
    {
        out.push(derived(
            "yearly_cost",
            "A year",
            format_amount(yearly, currency),
            subscription.amount_checked && cadence_checked,
            format!(
                "the newest amount times {} charges a year",
                subscription.cadence.per_year()
            ),
        ));
    }
    let start = day_label(subscription.start());
    out.push(from_record(
        first_id,
        "issued_at",
        "Since",
        start.clone(),
        start,
    ));
    let last = day_label(subscription.last_charge().day);
    out.push(from_record(
        last_id,
        "issued_at",
        "Last charge",
        last.clone(),
        last,
    ));
    if let Some(next) = subscription.next_expected {
        let shown = day_label(next);
        out.push(derived(
            "next_expected",
            "Next charge",
            shown,
            false,
            format!(
                "expected {} after the last charge; a guess until it comes",
                subscription.cadence.interval()
            ),
        ));
    }
    out.push(derived(
        "status",
        "Status",
        subscription.status.as_str().to_string(),
        false,
        subscription.status_reason.clone(),
    ));
    out
}

fn signal_data(found: &[Subscription]) -> Vec<RecordSubscriptionSignalData> {
    let by_id: HashMap<&str, &Subscription> = found.iter().map(|s| (s.id.as_str(), s)).collect();
    subscriptions::signals(found)
        .into_iter()
        .filter_map(|signal| {
            let (kind, id, at, label) = match &signal {
                Signal::PriceChange {
                    subscription_id,
                    change,
                } => {
                    let subscription = by_id.get(subscription_id.as_str())?;
                    let currency = subscription.currency.as_deref()?;
                    let verb = if change.to_minor > change.from_minor {
                        "went up"
                    } else {
                        "went down"
                    };
                    (
                        "price_change",
                        subscription_id,
                        change.day,
                        format!(
                            "{} {verb} from {} to {} on {}",
                            subscription.title(),
                            format_amount(change.from_minor, currency),
                            format_amount(change.to_minor, currency),
                            change.day.format("%-d %b")
                        ),
                    )
                }
                Signal::MissedCharge {
                    subscription_id,
                    expected,
                } => {
                    let subscription = by_id.get(subscription_id.as_str())?;
                    (
                        "missed_charge",
                        subscription_id,
                        *expected,
                        format!(
                            "No {} charge since {}; expected around {}",
                            subscription.title(),
                            subscription.last_charge().day.format("%-d %b"),
                            expected.format("%-d %b")
                        ),
                    )
                }
            };
            let subscription = by_id.get(id.as_str())?;
            Some(RecordSubscriptionSignalData {
                kind: kind.to_string(),
                subscription_id: id.clone(),
                record_id: subscription.last_charge().record_ids[0].clone(),
                at: day_at(at),
                label,
            })
        })
        .collect()
}

/// The signals as lines for Now and Archive's strip, until Updates' Needs
/// a look is there to take them: a price change or a missed charge reads
/// as a moment of its own.
pub(super) async fn signal_moments(
    state: &AppState,
    accounts: &[AccountId],
    now: DateTime<Utc>,
) -> Result<Vec<RecordMomentData>, HandlerError> {
    let loaded = load(state, accounts, now).await?;
    Ok(signal_data(&loaded.detection.subscriptions)
        .into_iter()
        .map(|signal| RecordMomentData {
            kind: signal.kind,
            record_id: signal.record_id,
            group_id: Some(signal.subscription_id),
            at: signal.at,
            label: signal.label,
        })
        .collect())
}

/// The renewal to-do of each yearly subscription whose next charge is
/// within To do's lead time. Runs on the records tick.
pub(crate) async fn file_renewals(
    state: &AppState,
    now: DateTime<Utc>,
) -> Result<u32, HandlerError> {
    let accounts = scoped_accounts(state, None).await?;
    let loaded = load(state, &accounts, now).await?;
    subscriptions::file_renewals(
        &state.store,
        &loaded,
        now,
        &Local,
        state.snooze_time_prefs().morning_hour,
    )
    .await
    .map_err(|error| HandlerError::Message(error.to_string()))
}
