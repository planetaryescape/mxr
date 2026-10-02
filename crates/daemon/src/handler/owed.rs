//! `Request::ListOwedReplies`: the replies you owe.
//!
//! The default is the desk's You owe lane, computed by the desk itself so
//! the two never disagree (D103). `all` keeps the raw store list (every
//! thread whose latest inbound has no later outbound) for scripts.

use super::desk::{compose_desk, desk_lane};
use super::desk_lanes::{pace_ratio, pace_seconds};
use super::HandlerResult;
use crate::state::AppState;
use chrono::{DateTime, Duration, Utc};
use mxr_core::AccountId;
use mxr_protocol::{DeskLaneKind, OwedReplyRowData, ResponseData};

const SECONDS_PER_DAY: f64 = 86_400.0;

/// Which rows `ListOwedReplies` returns and how many.
#[derive(Debug, Clone, Copy)]
pub(super) struct OwedQuery {
    pub older_than_days: Option<u32>,
    pub within_days: Option<u32>,
    pub limit: u32,
    pub all: bool,
}

pub(super) async fn list_owed_replies(
    state: &AppState,
    account_id: &AccountId,
    query: OwedQuery,
) -> HandlerResult {
    let rows = if query.all {
        raw_rows(state, account_id, query).await?
    } else {
        desk_rows(state, account_id, query, Utc::now()).await?
    };
    Ok(ResponseData::OwedReplies { rows })
}

/// The desk's You owe lane in its own order, narrowed by the windows,
/// which apply to the latest message from them.
async fn desk_rows(
    state: &AppState,
    account_id: &AccountId,
    query: OwedQuery,
    now: DateTime<Utc>,
) -> Result<Vec<OwedReplyRowData>, super::HandlerError> {
    let desk = compose_desk(state, Some(account_id), now).await?;
    let owed = desk_lane(&desk.rows, DeskLaneKind::Owed, u32::MAX);
    let days_ago = |days: u32| now - Duration::days(i64::from(days));
    Ok(owed
        .rows
        .into_iter()
        .filter(|row| {
            query
                .within_days
                .is_none_or(|days| row.since >= days_ago(days))
                && query
                    .older_than_days
                    .is_none_or(|days| row.since <= days_ago(days))
        })
        .take(query.limit as usize)
        .map(|row| OwedReplyRowData {
            overdue_score: pace_ratio(&row),
            expected_days: pace_seconds(&row) as f64 / SECONDS_PER_DAY,
            waiting_days: row.age_seconds as f64 / SECONDS_PER_DAY,
            usual_seconds: row.usual_seconds,
            thread_id: row.thread_id,
            latest_inbound_msg_id: row.message_id,
            from_email: row.counterparty_email,
            from_name: row.counterparty_name,
            subject: row.subject,
            latest_inbound_at: row.since,
        })
        .collect())
}

async fn raw_rows(
    state: &AppState,
    account_id: &AccountId,
    query: OwedQuery,
) -> Result<Vec<OwedReplyRowData>, super::HandlerError> {
    let rows = state
        .store
        .list_owed_replies(
            account_id,
            query.older_than_days,
            query.within_days,
            query.limit,
        )
        .await?;
    Ok(rows
        .into_iter()
        .map(|r| OwedReplyRowData {
            thread_id: r.thread_id,
            latest_inbound_msg_id: r.latest_inbound_msg_id,
            from_email: r.from_email,
            from_name: r.from_name,
            subject: r.subject,
            latest_inbound_at: r.latest_inbound_at,
            waiting_days: r.waiting_days,
            expected_days: r.expected_days,
            overdue_score: r.overdue_score,
            usual_seconds: None,
        })
        .collect())
}
