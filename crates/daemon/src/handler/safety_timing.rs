//! Slice 4.1 wiring (C2.5): turn `SendTimeRecommendation` into a
//! `Severity::Info` safety hint when the proposed send time hits a
//! materially slower bucket than the recipient's fastest one.
//!
//! The hint appears in `mxr send <draft> --check` output and the
//! TUI safety modal. It never blocks; the user can ignore it.
//!
//! Conditions for emitting the hint:
//! * `context.proposed_send_at` is `Some` (caller wants timing input).
//! * Recipient's confidence band is Medium or High (don't warn on
//!   weak data).
//! * `proposed_p50 >= 2 * best_p50` (at least 2x slower than the
//!   fastest historic bucket).

use crate::state::AppState;
use chrono::{DateTime, Datelike, Timelike, Utc};
use mxr_core::types::{
    CitationRef, Draft, DraftSafetyIssue, DraftSafetyIssueCode, DraftSafetySeverity,
};
use mxr_store::SendTimeConfidence;

/// Run the timing check against every `to`/`cc` recipient. Returns
/// at most one Info issue listing the slow recipients (a single
/// row in the modal is easier to scan than N).
pub(crate) async fn check_send_time(
    state: &AppState,
    draft: &Draft,
    proposed_at: DateTime<Utc>,
) -> Vec<DraftSafetyIssue> {
    let weekday = proposed_at.weekday().num_days_from_monday() as u8;
    let hour = proposed_at.hour() as u8;

    let mut slow = Vec::new();
    for addr in draft.to.iter().chain(draft.cc.iter()) {
        let rec = match state
            .store
            .send_time_recommendation(&draft.account_id, &addr.email)
            .await
        {
            Ok(r) => r,
            Err(_) => continue,
        };
        if !is_eligible(&rec) {
            continue;
        }
        let proposed_p50 = match rec.bucket_p50(weekday, hour) {
            Some(v) => v,
            None => continue,
        };
        let best_p50 = match rec.best_p50_seconds {
            Some(v) if v > 0 => v,
            _ => continue,
        };
        if proposed_p50 >= best_p50.saturating_mul(2) {
            slow.push((addr.email.clone(), best_p50, rec));
        }
    }

    if slow.is_empty() {
        return Vec::new();
    }

    let names: Vec<String> = slow.iter().map(|(email, _, _)| email.clone()).collect();
    let summary_recipients = names.join(", ");
    let (best_label, faster_window) = slow
        .iter()
        .min_by_key(|(_, best, _)| *best)
        .map(|(_, best, rec)| {
            let label = format_window(rec.best_weekday, rec.best_hour);
            (humanize_seconds(*best), label)
        })
        .unwrap_or_else(|| ("?".into(), "(unknown)".into()));

    let citations = slow
        .iter()
        .map(|(email, _, _)| CitationRef {
            message_id: None,
            thread_id: None,
            field: "to".into(),
            quote: email.clone(),
        })
        .collect();

    vec![DraftSafetyIssue::new(
        DraftSafetyIssueCode::SendTimeNote,
        DraftSafetySeverity::Info,
        format!(
            "Proposed send time is at least 2x slower than {summary_recipients}'s historic best ({best_label} typical at {faster_window})."
        ),
    )
    .with_citations(citations)]
}

fn is_eligible(rec: &mxr_store::SendTimeRecommendation) -> bool {
    matches!(
        rec.confidence,
        SendTimeConfidence::Medium | SendTimeConfidence::High
    )
}

const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

fn format_window(weekday: Option<u8>, hour: Option<u8>) -> String {
    match (weekday, hour) {
        (Some(w), Some(h)) => format!(
            "{} {h:02}:00",
            WEEKDAYS.get(w as usize).copied().unwrap_or("?")
        ),
        _ => "(unknown)".into(),
    }
}

fn humanize_seconds(secs: i64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86_400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86_400)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mxr_core::types::*;
    use mxr_store::Store;
    use std::sync::Arc;

    async fn fixture() -> (Arc<AppState>, mxr_core::AccountId) {
        let (state, _) = AppState::in_memory_with_fake().await.unwrap();
        let state = Arc::new(state);
        let account_id = state.store.list_accounts().await.unwrap()[0].id.clone();
        (state, account_id)
    }

    /// Insert a `they_replied` reply_pair with a synthetic message
    /// pair so the bucketing query can find it. Mirrors the helper
    /// used by the send_time tests in mxr-store.
    async fn insert_reply(
        store: &Store,
        account_id: &mxr_core::AccountId,
        recipient: &str,
        parent_received_at: chrono::DateTime<Utc>,
        latency_seconds: i64,
    ) {
        use mxr_core::id::*;
        let parent_id = MessageId::new();
        let reply_id = MessageId::new();
        let parent_secs = parent_received_at.timestamp();
        let replied_secs = parent_secs + latency_seconds;
        let now = Utc::now().timestamp();
        for (id, dir) in [(&parent_id, "outbound"), (&reply_id, "inbound")] {
            sqlx::query(
                "INSERT INTO messages (id, account_id, provider_id, thread_id, message_id_header,
                  in_reply_to, reference_headers, from_name, from_email, to_addrs, cc_addrs,
                  bcc_addrs, subject, date, flags, snippet, has_attachments, size_bytes,
                  unsubscribe_method, direction)
                  VALUES (?, ?, ?, ?, NULL, NULL, NULL, NULL, ?, '[]', '[]', '[]', '', ?, 0, '', 0,
                          0, NULL, ?)",
            )
            .bind(id.as_str())
            .bind(account_id.as_str())
            .bind(format!("p-{}", uuid::Uuid::now_v7()))
            .bind(format!("th-{}", uuid::Uuid::now_v7()))
            .bind(recipient)
            .bind(parent_secs)
            .bind(dir)
            .execute(store.writer())
            .await
            .unwrap();
        }
        sqlx::query(
            "INSERT INTO reply_pairs (reply_message_id, parent_message_id, account_id,
              counterparty_email, direction, parent_received_at, replied_at, latency_seconds,
              business_hours_latency_seconds, created_at)
              VALUES (?, ?, ?, ?, 'they_replied', ?, ?, ?, NULL, ?)",
        )
        .bind(reply_id.as_str())
        .bind(parent_id.as_str())
        .bind(account_id.as_str())
        .bind(recipient)
        .bind(parent_secs)
        .bind(replied_secs)
        .bind(latency_seconds)
        .bind(now)
        .execute(store.writer())
        .await
        .unwrap();
    }

    fn draft_to(account_id: &mxr_core::AccountId, recipient: &str) -> Draft {
        Draft {
            id: mxr_core::DraftId::new(),
            account_id: account_id.clone(),
            reply_headers: None,
            intent: DraftIntent::New,
            to: vec![Address {
                name: None,
                email: recipient.into(),
            }],
            cc: vec![],
            bcc: vec![],
            subject: "test".into(),
            body_markdown: "hi".into(),
            attachments: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    /// Helper: a Friday 14:00 UTC datetime (the test fixture's
    /// fast bucket).
    fn fri_14_utc() -> chrono::DateTime<Utc> {
        use chrono::TimeZone;
        Utc.with_ymd_and_hms(2026, 5, 1, 14, 0, 0).unwrap()
    }

    /// Mon 09:00 UTC — the test fixture's slow bucket.
    fn mon_09_utc() -> chrono::DateTime<Utc> {
        use chrono::TimeZone;
        Utc.with_ymd_and_hms(2026, 4, 27, 9, 0, 0).unwrap()
    }

    /// Seed a recipient with: 5 fast Fri 14:00 replies (60s) + 5 slow
    /// Mon 09:00 replies (3 days). Fast bucket p50 = 60s, slow bucket
    /// p50 = 3 days. Sample count >= 8 -> Medium confidence.
    async fn seed_medium_confidence(state: &AppState, account: &mxr_core::AccountId, email: &str) {
        for _ in 0..5 {
            insert_reply(&state.store, account, email, fri_14_utc(), 60).await;
        }
        for _ in 0..5 {
            insert_reply(&state.store, account, email, mon_09_utc(), 3 * 86_400).await;
        }
    }

    #[tokio::test]
    async fn slow_bucket_emits_info_issue() {
        let (state, account) = fixture().await;
        seed_medium_confidence(&state, &account, "alice@example.com").await;
        let draft = draft_to(&account, "alice@example.com");
        // Proposed: Mon 09:00 UTC -> the slow bucket.
        let issues = check_send_time(&state, &draft, mon_09_utc()).await;
        assert_eq!(issues.len(), 1);
        let issue = &issues[0];
        assert_eq!(issue.severity, DraftSafetySeverity::Info);
        assert_eq!(issue.code, DraftSafetyIssueCode::SendTimeNote);
        assert!(
            issue.message.contains("alice@example.com"),
            "issue message names recipient: {}",
            issue.message
        );
    }

    #[tokio::test]
    async fn fast_bucket_emits_no_issue() {
        let (state, account) = fixture().await;
        seed_medium_confidence(&state, &account, "alice@example.com").await;
        let draft = draft_to(&account, "alice@example.com");
        // Proposed: Fri 14:00 UTC -> the fast bucket itself.
        let issues = check_send_time(&state, &draft, fri_14_utc()).await;
        assert!(
            issues.is_empty(),
            "fast bucket must NOT emit an Info issue: {issues:?}"
        );
    }

    #[tokio::test]
    async fn low_confidence_emits_no_issue() {
        let (state, account) = fixture().await;
        // Only 3 samples -> Low confidence.
        for _ in 0..3 {
            insert_reply(&state.store, &account, "bob@example.com", mon_09_utc(), 60).await;
        }
        let draft = draft_to(&account, "bob@example.com");
        let issues = check_send_time(&state, &draft, mon_09_utc()).await;
        assert!(
            issues.is_empty(),
            "Low confidence must not warn -- avoid noise on weak data"
        );
    }

    #[tokio::test]
    async fn proposed_bucket_with_no_history_emits_no_issue() {
        let (state, account) = fixture().await;
        seed_medium_confidence(&state, &account, "alice@example.com").await;
        let draft = draft_to(&account, "alice@example.com");
        // Wed 03:00 UTC -- a bucket the recipient has never been
        // contacted in. We can't compare against an unknown bucket.
        use chrono::TimeZone;
        let wed_3am = Utc.with_ymd_and_hms(2026, 4, 29, 3, 0, 0).unwrap();
        let issues = check_send_time(&state, &draft, wed_3am).await;
        assert!(issues.is_empty(), "no historical data => no warning");
    }

    #[tokio::test]
    async fn unknown_recipient_emits_no_issue() {
        let (state, account) = fixture().await;
        let draft = draft_to(&account, "ghost@example.com");
        let issues = check_send_time(&state, &draft, mon_09_utc()).await;
        assert!(issues.is_empty());
    }
}
