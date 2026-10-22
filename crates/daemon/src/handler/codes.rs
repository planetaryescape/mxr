//! One-time codes from recent mail (`Request::ListOneTimeCodes`), for "copy
//! the code I just asked for". The code and its window come from the same
//! rules and cached facts as Updates, so a code shows the same everywhere.

use chrono::{Duration, Local, Utc};
use mxr_core::AccountId;
use mxr_protocol::{OneTimeCodeData, ResponseData};

use super::{updates, HandlerResult};
use crate::state::AppState;

/// How far back codes are looked for. Sign-up codes can last a day.
const LOOKBACK: Duration = Duration::hours(24);
/// How far ahead of this machine's clock a message may be dated and still
/// count: clocks drift. Beyond it a sender's date is not believed.
const SKEW: Duration = Duration::minutes(5);
/// Most recent messages read for codes, newest first.
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
    let codes = recent
        .iter()
        .filter_map(|message| {
            let fact = facts.get(&message.id)?;
            let code = fact.code.clone()?;
            // The code's own lifetime, not the message's Updates window: a
            // sign-in alert stays two days, its code five minutes.
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
        .take(limit)
        .collect();
    Ok(ResponseData::OneTimeCodes { codes })
}
