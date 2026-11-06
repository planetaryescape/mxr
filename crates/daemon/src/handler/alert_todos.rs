//! A verified alert becomes its own to-do on arrival (blueprint 22, D193).
//!
//! `mxr_updates::trust::decide` makes the decision from plain data; this
//! module reads that data from the store and the clock, and writes the
//! to-do. The same assessment feeds the digest, which only explains why a
//! suggestion was not made into a to-do and never creates one.

use super::todos::retime_record;
use super::HandlerError;
use crate::state::AppState;
use chrono::{DateTime, Local, Utc};
use mxr_core::id::AccountId;
use mxr_core::{MessageFlags, ProviderKind};
use mxr_protocol::AutoTodoData;
use mxr_store::{PlaceMessage, TodoRecord, TodoState};
use mxr_todo::provenance::{FieldProvenance, FieldSource, FieldSources};
use mxr_todo::TodoKind;
use mxr_updates::trust::{
    decide, todo_title, AlertInput, Claim, Decision, Held, History, MIN_HISTORY_MESSAGES,
};
use mxr_updates::{Fact, NeedsYou, WindowKind};
use std::collections::HashMap;

/// `origin` of a to-do made by this module.
pub(super) const ORIGIN: &str = "rule";

/// The decision with what the digest needs to explain it.
#[derive(Debug, Clone)]
pub(super) struct Assessment {
    pub decision: Decision,
    /// The verified host, when there is one.
    pub host: Option<String>,
    pub history: History,
    pub handled_at: Option<DateTime<Utc>>,
}

/// What kind of alert a message is, and when a sign-in stops mattering.
#[derive(Debug, Clone, Copy)]
pub(super) struct Alert {
    pub kind: NeedsYou,
    pub window_until: Option<DateTime<Utc>>,
}

impl Alert {
    /// The alert a message's fact states, if it states one.
    pub(super) fn of_fact(fact: &Fact) -> Option<Self> {
        Some(Self {
            kind: fact.needs_you?,
            window_until: fact
                .window
                .as_ref()
                .filter(|window| window.kind == WindowKind::SignIn)
                .map(|window| window.until),
        })
    }

    /// The email that told mxr a parcel went wrong.
    pub(super) fn delivery_problem() -> Self {
        Self {
            kind: NeedsYou::DeliveryException,
            window_until: None,
        }
    }
}

/// Decide for one alert.
pub(super) async fn assess(
    state: &AppState,
    message: &PlaceMessage,
    alert: Alert,
    now: DateTime<Utc>,
) -> Result<Assessment, HandlerError> {
    let kind = alert.kind;
    let arrival = state.store.message_arrival(&message.id).await?;
    let verified_host = arrival.as_ref().and_then(|a| a.verified_host.clone());
    let received_at = arrival.as_ref().and_then(|a| a.received_at);
    let unverified_code = arrival
        .as_ref()
        .and_then(|a| a.unverified_reason)
        .map(|reason| reason.code().to_string());
    let (mut history, mut established) = (History::default(), Vec::new());
    if let (Some(host), Some(received)) = (&verified_host, received_at) {
        let found = state
            .store
            .sender_history(&message.account_id, host, received, &message.id)
            .await?;
        history = History {
            messages: found.messages,
            oldest: found.oldest,
        };
        // Only a host with real history could be imitated.
        established = state
            .store
            .established_hosts(&message.account_id, received, MIN_HISTORY_MESSAGES)
            .await?;
    }
    let input = AlertInput {
        kind,
        enabled: state.config_snapshot().updates.auto_todo_alerts,
        verified_host: verified_host.as_deref(),
        unverified_code: unverified_code.as_deref(),
        received_at,
        now,
        in_inbox: message.in_inbox,
        is_live: arrival.as_ref().is_some_and(|arrival| arrival.is_live),
        unread: !message.flags.contains(MessageFlags::READ),
        list_mail: message.list_id.is_some() || message.evidence.list_id.is_some(),
        window_until: alert.window_until,
        history,
        established: &established,
    };
    Ok(Assessment {
        decision: decide(&input, &Local),
        host: verified_host,
        history,
        handled_at: received_at,
    })
}

/// Whether the account is an Outlook.com or Microsoft 365 one, for the
/// wording of "doesn't give mxr a verified sender".
pub(super) async fn is_outlook(
    state: &AppState,
    account_id: &AccountId,
) -> Result<bool, HandlerError> {
    Ok(state
        .store
        .get_account(account_id)
        .await?
        .and_then(|account| account.sync_backend)
        .is_some_and(|backend| {
            matches!(
                backend.provider_kind,
                ProviderKind::OutlookPersonal | ProviderKind::OutlookWork
            )
        }))
}

/// What the digest shows beside a suggested alert.
pub(super) async fn hold_note(
    state: &AppState,
    message: &PlaceMessage,
    alert: Alert,
    now: DateTime<Utc>,
) -> Result<Option<AutoTodoData>, HandlerError> {
    let assessment = assess(state, message, alert, now).await?;
    let Decision::Hold(held) = &assessment.decision else {
        // A creatable alert not yet in To do (the scan has not reached it):
        // no reason to give.
        return Ok(None);
    };
    let outlook = is_outlook(state, &message.account_id).await?;
    let host = assessment
        .host
        .clone()
        .unwrap_or_else(|| mxr_updates::fact::sender_host(&message.from_email));
    Ok(held.text(&host, outlook).map(|text| AutoTodoData {
        state: "not_created".to_string(),
        reason_code: held.code(),
        reason_text: text,
    }))
}

/// Digest notes for the alerts in view, keyed by message.
pub(super) async fn hold_notes(
    state: &AppState,
    alerts: &[(&PlaceMessage, Alert)],
    now: DateTime<Utc>,
) -> HashMap<mxr_core::MessageId, AutoTodoData> {
    let mut notes = HashMap::new();
    for (message, alert) in alerts {
        match hold_note(state, message, *alert, now).await {
            Ok(Some(note)) => {
                notes.insert(message.id.clone(), note);
            }
            Ok(None) => {}
            Err(error) => tracing::warn!(%error, "could not explain an alert hold"),
        }
    }
    notes
}

/// Digest notes for the parcels in view that went wrong: why the email
/// behind the problem did not make a to-do, or that none is tied to it.
pub(super) async fn parcel_notes(
    state: &AppState,
    deliveries: &[mxr_store::Delivery],
    now: DateTime<Utc>,
) -> HashMap<mxr_core::DeliveryId, AutoTodoData> {
    let mut notes = HashMap::new();
    for delivery in deliveries {
        let went_wrong = matches!(
            delivery.status.as_str(),
            "exception" | "attempt_fail" | "returned"
        );
        if !went_wrong {
            continue;
        }
        match parcel_note(state, delivery, now).await {
            Ok(Some(note)) => {
                notes.insert(delivery.id.clone(), note);
            }
            Ok(None) => {}
            Err(error) => tracing::warn!(%error, "could not explain a parcel hold"),
        }
    }
    notes
}

async fn parcel_note(
    state: &AppState,
    delivery: &mxr_store::Delivery,
    now: DateTime<Utc>,
) -> Result<Option<AutoTodoData>, HandlerError> {
    let not_tied = || AutoTodoData {
        state: "not_created".to_string(),
        reason_code: Held::ParcelNotTied.code(),
        reason_text: Held::ParcelNotTied.text("", false).unwrap_or_default(),
    };
    for id in state
        .store
        .delivery_problem_message_ids(&delivery.id)
        .await?
    {
        let Some(message) = state.store.place_message(&id).await? else {
            continue;
        };
        // The newest email that reported the problem decides.
        return hold_note(state, &message, Alert::delivery_problem(), now).await;
    }
    Ok(Some(not_tied()))
}

/// The reason line stored on a created to-do.
fn created_reason(host: &str, history: History) -> String {
    let since = history
        .oldest
        .map(|oldest| {
            format!(
                " and you have had mail from it since {}",
                oldest.format("%-d %b %Y")
            )
        })
        .unwrap_or_default();
    format!("Created automatically: verified origin {host} and receiver handling age{since}.")
}

/// On arrival: create the alert's to-do if the decision says so. Returns
/// the number of tasks made. A claim already held (a re-sync, a
/// reclassification, the same alert twice in a local day, a to-do the user
/// since dismissed) makes nothing.
pub(super) async fn create_on_arrival(
    state: &AppState,
    message: &PlaceMessage,
    alert: Alert,
    now: DateTime<Utc>,
) -> Result<u64, HandlerError> {
    let assessment = assess(state, message, alert, now).await?;
    let Decision::Create(claim) = &assessment.decision else {
        return Ok(0);
    };
    let mut record = alert_record(
        state,
        message,
        claim,
        assessment.history,
        assessment.handled_at,
        now,
    );
    let requests = enrich_from_stored_ask(state, message, claim, &mut record, now).await?;
    if !state
        .store
        .insert_alert_todo_with_requests(&record, claim.kind.as_str(), &requests)
        .await?
    {
        return Ok(0);
    }
    // Kind only: the host and the message stay out of the activity log.
    state.activity.record(crate::activity::OwnedEntry {
        ts: now.timestamp_millis(),
        account_id: Some(message.account_id.to_string()),
        source: mxr_protocol::ClientKind::Daemon,
        action: "todo.alert_created".into(),
        target_kind: Some("todo".into()),
        target_id: Some(record.id.clone()),
        tier: mxr_store::Tier::Important,
        context: Some(serde_json::json!({ "kind": claim.kind.as_str() })),
    });
    Ok(1 + requests.len() as u64)
}

/// Asks projection uses the same receiver gate before acknowledging its work.
/// This avoids creating an untrusted payment task before the Updates scan.
pub(super) async fn project_payment_ask(
    state: &AppState,
    id: &mxr_core::MessageId,
    now: DateTime<Utc>,
) -> Result<u64, HandlerError> {
    let Some(envelope) = state.store.get_envelope(id).await? else {
        return Ok(0);
    };
    let answers = state
        .store
        .asks_for_messages(&envelope.account_id, std::slice::from_ref(id))
        .await?;
    let Some(answer) = answers.get(id) else {
        return Ok(0);
    };
    if !mxr_todo::asks::items::has_payment_failure_notice(answer) {
        return Ok(0);
    }
    if !mxr_todo::asks::items::detections(answer, &envelope)
        .iter()
        .any(|(_, d)| d.kind == TodoKind::PaymentFailed)
    {
        return Ok(0);
    }
    if let Some(message) = state.store.place_message(id).await? {
        if message.direction == "inbound" {
            return create_on_arrival(
                state,
                &message,
                Alert {
                    kind: NeedsYou::PaymentFailed,
                    window_until: None,
                },
                now,
            )
            .await;
        }
    }
    Ok(0)
}

async fn enrich_from_stored_ask(
    state: &AppState,
    message: &PlaceMessage,
    claim: &Claim,
    record: &mut TodoRecord,
    now: DateTime<Utc>,
) -> Result<Vec<TodoRecord>, HandlerError> {
    if claim.kind != NeedsYou::PaymentFailed {
        return Ok(Vec::new());
    }
    let Some(envelope) = state.store.get_envelope(&message.id).await? else {
        return Ok(Vec::new());
    };
    let answers = state
        .store
        .asks_for_messages(&message.account_id, std::slice::from_ref(&message.id))
        .await?;
    let Some(answer) = answers.get(&message.id) else {
        return Ok(Vec::new());
    };
    let mut payments: Vec<_> = mxr_todo::asks::items::detections(answer, &envelope)
        .into_iter()
        .filter(|(_, d)| d.kind == TodoKind::PaymentFailed)
        .map(|(_, d)| d)
        .collect();
    if payments.is_empty() {
        return Ok(Vec::new());
    }
    let primary = payments
        .iter()
        .position(|d| mxr_todo::asks::items::has_failure_status(&d.reason))
        .unwrap_or(0);
    let detection = payments.remove(primary);
    let rows = state
        .store
        .list_todo_scan_rows(std::slice::from_ref(&message.id))
        .await?;
    let Some(row) = rows.first() else {
        return Ok(Vec::new());
    };
    let cfg = super::todos::pass_config(state, now);
    let (projected, _) = mxr_todo::pass::stored_record(&cfg, row, detection, false);
    record.amount_minor = projected.amount_minor;
    record.currency = projected.currency;
    record.due_at = projected.due_at;
    record.due_words = projected.due_words;
    if projected.counterparty.is_some() {
        record.counterparty = projected.counterparty;
    }
    let verified = FieldSources::from_json(&record.field_sources);
    let mut fields = FieldSources::from_json(&projected.field_sources);
    fields.0.extend(verified.0.clone());
    retime_record(record, &mut fields, state.snooze_time_prefs().morning_hour);
    record.field_sources = fields.to_json();
    let mut requests = Vec::new();
    for detection in payments
        .into_iter()
        .filter(|d| !mxr_todo::asks::items::has_failure_status(&d.reason))
    {
        let (mut request, _) = mxr_todo::pass::stored_record(&cfg, row, detection, false);
        request.dedup_key = format!(
            "{}|owner|{}",
            record.dedup_key,
            mxr_store::assessment_fingerprint(&request.dedup_key)
        );
        request.title = format!("Update payment details · {}", claim.host);
        request.counterparty = Some(claim.host.clone());
        request.sender_domain = Some(claim.host.clone());
        request.source_date = record.source_date;
        request.state = TodoState::Open;
        request.expired_at = None;
        request.expired_at_birth = false;
        request.catchup = None;
        request.action_url = None;
        request.action_domain = None;
        request.reason = format!("{} Source request: {}", record.reason, request.reason);
        let mut fields = FieldSources::from_json(&request.field_sources);
        fields.0.extend(verified.0.clone());
        fields.set(
            "alert_role",
            FieldProvenance::with_evidence(FieldSource::Rule, "independent_owner_request"),
        );
        retime_record(
            &mut request,
            &mut fields,
            state.snooze_time_prefs().morning_hour,
        );
        request.field_sources = fields.to_json();
        requests.push(request);
    }
    Ok(requests)
}

pub(super) fn alert_record(
    state: &AppState,
    message: &PlaceMessage,
    claim: &Claim,
    history: History,
    handled_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> TodoRecord {
    let kind = match claim.kind {
        NeedsYou::PaymentFailed => TodoKind::PaymentFailed,
        NeedsYou::SignIn => TodoKind::Verify,
        NeedsYou::DeliveryException => TodoKind::Other,
    };
    let title = todo_title(claim.kind, &claim.host);
    let reason = created_reason(&claim.host, history);
    let mut fields = FieldSources::default();
    for field in ["title", "kind", "counterparty"] {
        fields.set(
            field,
            FieldProvenance::with_evidence(
                FieldSource::Rule,
                format!("the provider confirmed {} sent it", claim.host),
            ),
        );
    }
    fields.set(
        "requester",
        FieldProvenance::with_evidence(
            FieldSource::Rule,
            format!("{} sent this alert", claim.host),
        ),
    );
    let id = mxr_todo::pass::new_todo_id();
    let mut record = TodoRecord {
        id,
        account_id: message.account_id.clone(),
        thread_id: Some(message.thread_id.clone()),
        source_message_id: Some(message.id.clone()),
        source_date: handled_at,
        source_filed_at: None,
        source_unread: Some(true),
        kind: kind.as_str().to_string(),
        verb: title
            .split_whitespace()
            .next()
            .unwrap_or_else(|| kind.default_verb())
            .to_ascii_lowercase(),
        doc_type: None,
        title,
        counterparty: Some(claim.host.clone()),
        sender_domain: Some(claim.host.clone()),
        amount_minor: None,
        currency: None,
        due_at: None,
        due_words: None,
        act_by_at: None,
        surface_at: None,
        scheduled_for: None,
        action_url: None,
        action_domain: None,
        relevant_until: None,
        window_source: None,
        state: TodoState::Open,
        expired_at: None,
        expired_at_birth: false,
        catchup: None,
        looks_done_message_id: None,
        looks_done_reason: None,
        origin: ORIGIN.to_string(),
        reason,
        field_sources: String::new(),
        user_edited: false,
        commitment_id: None,
        rules_version: mxr_todo::RULES_VERSION,
        dedup_key: claim.key.clone(),
        surfaced_at: None,
        created_at: now,
        updated_at: now,
        done_at: None,
        dismissed_at: None,
    };
    retime_record(
        &mut record,
        &mut fields,
        state.snooze_time_prefs().morning_hour,
    );
    record.field_sources = fields.to_json();
    record
}

/// `t` on an alert keeps working; this is the response data for a created
/// to-do's line.
pub(super) fn created_note(record: &TodoRecord) -> Option<AutoTodoData> {
    (record.origin == ORIGIN && record.dedup_key.starts_with("alert|")).then(|| AutoTodoData {
        state: "created".to_string(),
        reason_code: "verified_alert".to_string(),
        reason_text: record.reason.clone(),
    })
}
