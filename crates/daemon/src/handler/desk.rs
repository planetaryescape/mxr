//! `Request::GetDesk`: what needs you, not what arrived.
//!
//! Composes lanes from what the store already knows: thread direction and
//! inbox state, contacts, screener decisions, reply latencies, open
//! commitments and cadence drift. Local reads only; no LLM on this path.
//! The lane rules live in `desk_lanes.rs`.

use super::desk_lanes::{
    apply_pace, clean_subject, dedupe_by_precedence, sort_lane, thread_lanes, AccountInputs,
    PaceDirection, DESK_WINDOW_DAYS, DUE_AHEAD_DAYS,
};
use super::HandlerResult;
use crate::state::AppState;
use chrono::{DateTime, Duration, Utc};
use mxr_core::id::AccountId;
use mxr_core::types::{AccountAddressLookup, CalendarPartstat};
use mxr_protocol::{DeskElsewhereData, DeskLaneData, DeskLaneKind, DeskRowData, ResponseData};
use mxr_store::{CommitmentDirection, CommitmentStatus, DeliveryListFilter, ScreenerDisposition};
use std::collections::{HashMap, HashSet};

pub(super) async fn get_desk(
    state: &AppState,
    account_id: Option<&AccountId>,
    lane_limit: u32,
) -> HandlerResult {
    let started = std::time::Instant::now();
    let now = Utc::now();
    let accounts: Vec<AccountId> = match account_id {
        Some(id) => vec![id.clone()],
        None => state
            .store
            .list_accounts()
            .await?
            .into_iter()
            .filter(|account| account.enabled)
            .map(|account| account.id)
            .collect(),
    };

    let mut rows = Vec::new();
    let mut elsewhere = DeskElsewhereData::default();
    let mut last_from_people_at: Option<DateTime<Utc>> = None;
    for account in &accounts {
        let desk = account_desk(state, account, now).await?;
        rows.extend(desk.rows);
        elsewhere.reading += desk.elsewhere.reading;
        elsewhere.paper_trail += desk.elsewhere.paper_trail;
        elsewhere.screener += desk.elsewhere.screener;
        elsewhere.invites += desk.elsewhere.invites;
        last_from_people_at = last_from_people_at.max(desk.last_from_people_at);
    }
    elsewhere.deliveries = count_active_deliveries(state, &accounts).await?;

    let rows = dedupe_by_precedence(rows);
    let lane = |kind: DeskLaneKind| {
        let mut lane_rows: Vec<DeskRowData> = rows
            .iter()
            .filter(|row| row.lane == kind)
            .cloned()
            .collect();
        sort_lane(kind, &mut lane_rows);
        let total = lane_rows.len() as u32;
        lane_rows.truncate(lane_limit as usize);
        DeskLaneData {
            rows: lane_rows,
            total,
        }
    };
    let data = ResponseData::Desk {
        account_id: account_id.cloned(),
        owed: lane(DeskLaneKind::Owed),
        due: lane(DeskLaneKind::Due),
        waiting: lane(DeskLaneKind::Waiting),
        people_new: lane(DeskLaneKind::PeopleNew),
        elsewhere,
        last_from_people_at,
        generated_at: now,
    };
    tracing::debug!(
        accounts = accounts.len(),
        elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
        "desk composed"
    );
    Ok(data)
}

struct AccountDesk {
    rows: Vec<DeskRowData>,
    elsewhere: DeskElsewhereData,
    last_from_people_at: Option<DateTime<Utc>>,
}

async fn account_desk(
    state: &AppState,
    account_id: &AccountId,
    now: DateTime<Utc>,
) -> Result<AccountDesk, super::HandlerError> {
    let store = &state.store;
    let messages = store
        .desk_thread_messages(account_id, now - Duration::days(DESK_WINDOW_DAYS))
        .await?;

    let mut emails: Vec<String> = messages
        .iter()
        .flat_map(|m| {
            std::iter::once(m.from.email.to_ascii_lowercase())
                .chain(m.to.iter().map(|a| a.email.to_ascii_lowercase()))
        })
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    emails.sort_unstable();
    let contacts: HashMap<String, _> = store
        .desk_contacts(account_id, &emails)
        .await?
        .into_iter()
        .map(|contact| (contact.email.to_ascii_lowercase(), contact))
        .collect();
    let screener: HashMap<String, ScreenerDisposition> = store
        .list_screener_decisions(account_id)
        .await?
        .into_iter()
        .map(|decision| {
            (
                decision.sender_email.to_ascii_lowercase(),
                decision.disposition,
            )
        })
        .collect();

    let addresses = state.account_addresses.clone();
    let account_email = store
        .get_account(account_id)
        .await?
        .map(|account| account.email.to_ascii_lowercase());
    let is_self = move |email: &str| {
        addresses.is_account_address(account_id, email)
            || account_email
                .as_deref()
                .is_some_and(|own| own.eq_ignore_ascii_case(email))
    };
    let mut lanes = thread_lanes(&AccountInputs {
        account_id,
        messages: &messages,
        contacts: &contacts,
        screener: &screener,
        is_self: &is_self,
        now,
    });

    let mut rows: Vec<(DeskRowData, Option<PaceDirection>)> = lanes
        .rows
        .drain(..)
        .map(|draft| (draft.row, draft.pace))
        .collect();
    rows.extend(due_rows(state, account_id, &lanes.subjects, &lanes.message_ids, now).await?);
    rows.extend(drift_rows(state, account_id, &rows, now).await?);

    // Usual pace for every counterparty in one query.
    let mut paced: Vec<String> = rows
        .iter()
        .filter(|(_, pace)| pace.is_some())
        .map(|(row, _)| row.counterparty_email.to_ascii_lowercase())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    paced.sort_unstable();
    let mut latencies: HashMap<(String, &'static str), Vec<i64>> = HashMap::new();
    for latency in store.desk_reply_latencies(account_id, &paced).await? {
        let direction = if latency.direction == "i_replied" {
            "mine"
        } else {
            "theirs"
        };
        latencies
            .entry((latency.email, direction))
            .or_default()
            .push(latency.latency_seconds);
    }
    let rows = rows
        .into_iter()
        .map(|(mut row, pace)| {
            if let Some(pace) = pace {
                let key = (
                    row.counterparty_email.to_ascii_lowercase(),
                    match pace {
                        PaceDirection::Mine => "mine",
                        PaceDirection::Theirs => "theirs",
                    },
                );
                apply_pace(&mut row, latencies.get(&key).map(Vec::as_slice));
            }
            row
        })
        .collect();

    lanes.elsewhere.invites = count_pending_invites(state, account_id, now).await?;
    Ok(AccountDesk {
        rows,
        elsewhere: lanes.elsewhere,
        last_from_people_at: lanes.last_from_people_at,
    })
}

/// Promises you made that are due within the week, or overdue this month.
async fn due_rows(
    state: &AppState,
    account_id: &AccountId,
    subjects: &HashMap<mxr_core::id::ThreadId, String>,
    message_ids: &HashMap<mxr_core::id::ThreadId, Vec<mxr_core::id::MessageId>>,
    now: DateTime<Utc>,
) -> Result<Vec<(DeskRowData, Option<PaceDirection>)>, super::HandlerError> {
    let commitments = state
        .store
        .list_contact_commitments(account_id, None, Some(CommitmentStatus::Open))
        .await?;
    let horizon = now + Duration::days(DUE_AHEAD_DAYS);
    let floor = now - Duration::days(DESK_WINDOW_DAYS);
    let mut rows = Vec::new();
    for commitment in commitments {
        let Some(due) = commitment.by_when else {
            continue;
        };
        if commitment.direction != CommitmentDirection::Yours || due > horizon || due < floor {
            continue;
        }
        let subject = match subjects.get(&commitment.thread_id) {
            Some(subject) => subject.clone(),
            None => state
                .store
                .get_envelope(&commitment.evidence_msg_id)
                .await?
                .map(|envelope| clean_subject(&envelope.subject))
                .unwrap_or_default(),
        };
        let age_seconds = (now - due).num_seconds();
        rows.push((
            DeskRowData {
                lane: DeskLaneKind::Due,
                account_id: account_id.clone(),
                thread_id: commitment.thread_id.clone(),
                message_id: commitment.evidence_msg_id.clone(),
                message_ids: message_ids
                    .get(&commitment.thread_id)
                    .cloned()
                    .unwrap_or_else(|| vec![commitment.evidence_msg_id.clone()]),
                counterparty_email: commitment.email.clone(),
                counterparty_name: None,
                subject,
                reason: format!("\u{201c}{}\u{201d}", commitment.what.trim()),
                since: due,
                age_seconds,
                usual_seconds: None,
                usual_samples: 0,
                overdue: age_seconds > 0,
                unread: false,
                starred: false,
                commitment_id: Some(commitment.id),
            },
            None,
        ));
    }
    Ok(rows)
}

/// Watched contacts who have gone quiet longer than usual join the waiting
/// lane, anchored on the latest conversation with them.
async fn drift_rows(
    state: &AppState,
    account_id: &AccountId,
    existing: &[(DeskRowData, Option<PaceDirection>)],
    now: DateTime<Utc>,
) -> Result<Vec<(DeskRowData, Option<PaceDirection>)>, super::HandlerError> {
    let already: HashSet<String> = existing
        .iter()
        .map(|(row, _)| row.counterparty_email.to_ascii_lowercase())
        .collect();
    let mut rows = Vec::new();
    for drift in state.store.list_cadence_drift(account_id).await? {
        if already.contains(&drift.email.to_ascii_lowercase()) {
            continue;
        }
        let Some(last_contact) = drift.last_contact_at else {
            continue;
        };
        let Some(exchange) = state
            .store
            .desk_latest_exchange(account_id, &drift.email, last_contact)
            .await?
        else {
            continue;
        };
        let expected_seconds = (drift.expected_days * 86_400.0).round() as i64;
        rows.push((
            DeskRowData {
                lane: DeskLaneKind::Waiting,
                account_id: account_id.clone(),
                thread_id: exchange.thread_id,
                message_id: exchange.message_id.clone(),
                message_ids: vec![exchange.message_id],
                counterparty_email: drift.email,
                counterparty_name: drift.display_name,
                subject: clean_subject(&exchange.subject),
                reason: format!(
                    "usually in touch every {}",
                    days_phrase(drift.expected_days)
                ),
                since: last_contact,
                age_seconds: (now - last_contact).num_seconds(),
                usual_seconds: Some(expected_seconds),
                // The pace is the watch's cadence, not measured replies.
                usual_samples: 0,
                overdue: true,
                unread: false,
                starred: false,
                commitment_id: None,
            },
            None,
        ));
    }
    Ok(rows)
}

fn days_phrase(days: f64) -> String {
    let days = days.round().max(1.0) as i64;
    match days {
        1 => "day".to_string(),
        7 => "week".to_string(),
        14 => "two weeks".to_string(),
        d if d % 7 == 0 => format!("{} weeks", d / 7),
        d => format!("{d} days"),
    }
}

async fn count_active_deliveries(
    state: &AppState,
    accounts: &[AccountId],
) -> Result<u32, super::HandlerError> {
    let deliveries = state
        .store
        .list_deliveries(DeliveryListFilter::Active)
        .await?;
    Ok(deliveries
        .iter()
        .filter(|delivery| accounts.contains(&delivery.account_id))
        .count() as u32)
}

/// Upcoming invitations that still want your answer, by the same rule the
/// invites page uses to show response buttons.
async fn count_pending_invites(
    state: &AppState,
    account_id: &AccountId,
    now: DateTime<Utc>,
) -> Result<u32, super::HandlerError> {
    let ResponseData::Invites { invites } =
        super::mailbox::list_invites(state, Some(account_id), 200).await?
    else {
        return Ok(0);
    };
    Ok(invites
        .iter()
        .filter(|invite| {
            let metadata = &invite.metadata;
            let method = metadata
                .method
                .as_deref()
                .unwrap_or("")
                .to_ascii_uppercase();
            let cancelled = method == "CANCEL"
                || metadata
                    .status
                    .as_deref()
                    .is_some_and(|status| status.eq_ignore_ascii_case("CANCELLED"));
            let upcoming = metadata
                .starts_at
                .as_deref()
                .and_then(|start| DateTime::parse_from_rfc3339(start).ok())
                .is_none_or(|start| start.with_timezone(&Utc) >= now);
            (method.is_empty() || method == "REQUEST")
                && !cancelled
                && upcoming
                && matches!(
                    metadata.viewer_partstat,
                    None | Some(CalendarPartstat::NeedsAction | CalendarPartstat::Delegated)
                )
        })
        .count() as u32)
}
