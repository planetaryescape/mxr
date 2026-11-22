use chrono::{Duration, Local, Utc};
use mxr_core::AccountId;
use mxr_protocol::{OneTimeCodeData, ResponseData};

use super::{updates, HandlerResult};
use crate::state::AppState;

const LOOKBACK: Duration = Duration::hours(24);
const SKEW: Duration = Duration::minutes(5);
const SCANNED: u32 = 200;
const DEFAULT_LIMIT: u32 = 10;
const MAX_LIMIT: u32 = 50;

pub(super) async fn list(
    state: &AppState,
    account_id: Option<&AccountId>,
    limit: Option<u32>,
) -> HandlerResult {
    let now = Utc::now();
    let limit = limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT) as usize;
    let recent = state
        .store
        .recent_received(account_id, now - LOOKBACK, now + SKEW, SCANNED)
        .await?;
    let messages: Vec<_> = recent.iter().collect();
    let facts = updates::facts_for(state, &messages, &Local).await?;
    let mut codes: Vec<OneTimeCodeData> = recent
        .iter()
        .filter_map(|message| {
            let fact = facts.get(&message.id)?;
            let code = fact.code.clone()?;
            let expires_at = fact
                .code_expires
                .unwrap_or(message.date + Duration::minutes(10));
            Some(OneTimeCodeData {
                code,
                current: expires_at > now,
                expires_at,
                received_at: message.date,
                sender_name: fact.source_name.clone(),
                sender_host: fact.sender_host.clone(),
                sender_email: message.from_email.to_ascii_lowercase(),
                subject: message.subject.clone(),
                account_id: message.account_id.clone(),
                message_id: message.id.clone(),
                thread_id: message.thread_id.clone(),
            })
        })
        .collect();
    codes.sort_by_key(|code| !code.current);
    codes.truncate(limit);
    Ok(ResponseData::OneTimeCodes { codes })
}
