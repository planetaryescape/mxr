#![cfg_attr(
    test,
    expect(
        clippy::panic,
        clippy::unwrap_used,
        reason = "unit tests use panic and unwrap to keep fixture failures direct"
    )
)]

mod account;
mod analytics;
mod auto_reminders;
mod body;
mod calendar;
mod contact_commitments;
mod contact_relationship_summary;
mod contact_style;
mod contacts;
mod contacts_refresh_handle;
mod context_briefings;
mod decision_log;
mod deliveries;
mod diagnostics;
mod draft;
mod draft_commitments;
mod draft_recovery;
mod draft_safety;
mod event_log;
mod keywords;
mod label;
mod message;
mod message_events;
mod message_flags;
mod mutation_dedup;
mod owed_replies;
mod pool;
mod relationship_watchlist;
mod reply_pairs;
mod rules;
mod scheduled_sends;
mod screener;
mod search;
mod semantic;
mod send_time;
mod sender_profile;
mod signatures;
mod snippets;
mod snooze;
mod sync_cursor;
mod sync_log;
mod sync_runtime_status;
#[cfg(test)]
mod test_fixtures;
mod thread;
mod thread_summary;
mod undo;
mod user_activity;
mod user_voice_profile;
mod wrapped;

pub use calendar::CalendarInviteRecord;
pub use contact_commitments::{CommitmentDirection, CommitmentStatus, ContactCommitmentRecord};
pub use contact_relationship_summary::ContactRelationshipSummaryRecord;
pub use contact_style::{ContactStyleRecord, RelationshipMessageSample};
pub use contacts_refresh_handle::ContactsRefreshHandle;
pub use context_briefings::{new_briefing_id, BriefingKind, ContextBriefing};
pub use decision_log::{decision_id, source_hash as decision_source_hash, DecisionLogEntry};
pub use deliveries::{Delivery, DeliveryItem, DeliveryListFilter};
pub use diagnostics::StoreRecordCounts;
pub use draft::SentDraftReceipt;
pub use draft_commitments::{new_candidate_id, DraftCommitmentCandidate};
pub use draft_safety::{DraftSafetyOverrideRecord, DraftSafetyRunRecord};
pub use event_log::{EventLogEntry, EventLogFilter, EventLogRefs};
pub use owed_replies::OwedReplyRow;
pub use pool::Store;
pub use relationship_watchlist::{CadenceDriftRow, RelationshipWatchEntry};
pub use rules::{row_to_rule_json, row_to_rule_log_json, RuleLogInput, RuleRecordInput};
pub use screener::{ScreenerDecision, ScreenerDisposition, ScreenerQueueEntry};
pub use send_time::{SendTimeBucket, SendTimeConfidence, SendTimeRecommendation};
pub use sender_profile::{
    SenderEmailReference, SenderProfile, SenderSummary, SenderUnansweredQuestion,
    SenderWeeklyActivity,
};
pub use signatures::{Signature, SignatureDefault, SignatureKind, SignatureScope};
pub use snippets::Snippet;
pub use sync_log::{SyncLogEntry, SyncStatus};
pub use sync_runtime_status::{SyncRuntimeStatus, SyncRuntimeStatusUpdate};
pub use thread_summary::{thread_summary_content_hash, ThreadSummaryRecord};
pub use undo::{UndoEntry, UndoEntrySnapshot, UndoableMutationKind};
pub use user_activity::{
    ActivityCursor, ActivityFilter, ActivityInsert, ActivityPage, ActivityRow, SavedActivityFilter,
    Tier,
};
pub use user_voice_profile::{
    UserVoiceMessageSample, UserVoiceProfileRecord, UserVoiceRegisterMode,
};

pub struct SavedSearchUpdate<'a> {
    pub new_name: Option<&'a str>,
    pub query: Option<&'a str>,
    pub search_mode: Option<&'a mxr_core::types::SearchMode>,
    pub sort: Option<&'a mxr_core::types::SortOrder>,
    pub icon: Option<&'a str>,
    pub position: Option<i32>,
}

use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::error::Error as StdError;
use std::str::FromStr;
use std::time::Instant;

pub(crate) fn encode_json<T: Serialize>(value: &T) -> Result<String, sqlx::Error> {
    serde_json::to_string(value).map_err(|error| sqlx::Error::Encode(Box::new(error)))
}

pub(crate) fn decode_json<T: DeserializeOwned>(value: &str) -> Result<T, sqlx::Error> {
    serde_json::from_str(value).map_err(sqlx::Error::decode)
}

pub(crate) fn decode_id<T>(value: &str) -> Result<T, sqlx::Error>
where
    T: FromStr,
    T::Err: StdError + Send + Sync + 'static,
{
    value.parse().map_err(sqlx::Error::decode)
}

pub(crate) fn decode_timestamp(value: i64) -> Result<DateTime<Utc>, sqlx::Error> {
    DateTime::from_timestamp(value, 0)
        .ok_or_else(|| sqlx::Error::Protocol(format!("invalid unix timestamp: {value}")))
}

pub(crate) fn decode_optional_timestamp(
    value: Option<i64>,
) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
    value.map(decode_timestamp).transpose()
}

pub(crate) fn trace_query(operation: &'static str, started_at: Instant, row_count: usize) {
    tracing::trace!(
        operation,
        row_count,
        elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0,
        "store query"
    );
}

pub(crate) fn trace_lookup(operation: &'static str, started_at: Instant, found: bool) {
    tracing::trace!(
        operation,
        found,
        elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0,
        "store lookup"
    );
}

/// SQL predicate for analytics/story queries that should ignore pure
/// self-addressed messages while still keeping real outbound mail. The
/// surrounding query must alias `messages` as `m`.
pub(crate) const NON_SELF_ADDRESSED_MESSAGE_PREDICATE: &str = r#"NOT (
    EXISTS (
        SELECT 1
        FROM account_addresses self_from
        WHERE LOWER(self_from.email) = LOWER(m.from_email)
    )
    AND EXISTS (
        SELECT 1
        FROM (
            SELECT json_extract(value, '$.email') AS email FROM json_each(m.to_addrs)
            UNION ALL
            SELECT json_extract(value, '$.email') AS email FROM json_each(m.cc_addrs)
            UNION ALL
            SELECT json_extract(value, '$.email') AS email FROM json_each(m.bcc_addrs)
        ) self_recipients
        JOIN account_addresses self_to
          ON LOWER(self_to.email) = LOWER(self_recipients.email)
        WHERE self_recipients.email IS NOT NULL
          AND self_recipients.email != ''
    )
    AND NOT EXISTS (
        SELECT 1
        FROM (
            SELECT json_extract(value, '$.email') AS email FROM json_each(m.to_addrs)
            UNION ALL
            SELECT json_extract(value, '$.email') AS email FROM json_each(m.cc_addrs)
            UNION ALL
            SELECT json_extract(value, '$.email') AS email FROM json_each(m.bcc_addrs)
        ) non_self_recipients
        WHERE non_self_recipients.email IS NOT NULL
          AND non_self_recipients.email != ''
          AND NOT EXISTS (
              SELECT 1
              FROM account_addresses self_addr
              WHERE LOWER(self_addr.email) = LOWER(non_self_recipients.email)
          )
    )
)"#;

#[cfg(test)]
mod tests;
