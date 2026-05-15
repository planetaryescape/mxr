//! Slice 4.1 of docs/ai-email/04-timing-cadence.md.
//!
//! Send-time optimizer: bucket `reply_pairs WHERE direction='they_replied'`
//! by recipient + (local weekday, hour) of the parent's `parent_received_at`,
//! and report median reply latency per bucket. Returns `high` confidence
//! only when there are at least 20 samples across at least 3 populated
//! buckets; `medium` starts at 8 samples.

use chrono::{DateTime, Datelike, Timelike, Utc};
use mxr_core::id::AccountId;
use sqlx::Row;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendTimeConfidence {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone)]
pub struct SendTimeBucket {
    pub weekday: u8, // 0 = Monday
    pub hour: u8,
    pub p50_seconds: i64,
    pub sample_count: u32,
}

#[derive(Debug, Clone)]
pub struct SendTimeRecommendation {
    pub recipient: String,
    pub buckets: Vec<SendTimeBucket>,
    pub best_weekday: Option<u8>,
    pub best_hour: Option<u8>,
    pub best_p50_seconds: Option<i64>,
    pub confidence: SendTimeConfidence,
    pub sample_count: u32,
}

impl SendTimeRecommendation {
    /// p50 for the (weekday, hour) bucket if the recipient has any
    /// historical reply pairs there. Returns `None` when the bucket
    /// has never been observed, so callers can distinguish "fast
    /// bucket" from "unknown slot".
    pub fn bucket_p50(&self, weekday: u8, hour: u8) -> Option<i64> {
        self.buckets
            .iter()
            .find(|b| b.weekday == weekday && b.hour == hour)
            .map(|b| b.p50_seconds)
    }
}

impl super::Store {
    /// Compute a recommendation for `recipient`.
    pub async fn send_time_recommendation(
        &self,
        account_id: &AccountId,
        recipient: &str,
    ) -> Result<SendTimeRecommendation, sqlx::Error> {
        // Pull every `they_replied` row for this recipient. Bucketing
        // happens in Rust because SQLite has no portable local-time
        // weekday/hour extractor that respects the user's TZ.
        let rows = sqlx::query(
            r#"SELECT parent_received_at, latency_seconds
               FROM reply_pairs
               WHERE account_id = ?1
                 AND direction = 'they_replied'
                 AND LOWER(counterparty_email) = LOWER(?2)
                 AND latency_seconds >= 0"#,
        )
        .bind(account_id.as_str())
        .bind(recipient)
        .fetch_all(self.reader())
        .await?;

        let mut by_bucket: std::collections::HashMap<(u8, u8), Vec<i64>> =
            std::collections::HashMap::new();
        let mut total = 0u32;
        for row in rows {
            let parent_received: i64 = row.try_get("parent_received_at")?;
            let latency: i64 = row.try_get("latency_seconds")?;
            let dt =
                DateTime::<Utc>::from_timestamp(parent_received, 0).unwrap_or_else(|| Utc::now());
            let weekday = dt.weekday().num_days_from_monday() as u8;
            let hour = dt.hour() as u8;
            by_bucket.entry((weekday, hour)).or_default().push(latency);
            total += 1;
        }

        let mut buckets: Vec<SendTimeBucket> = by_bucket
            .into_iter()
            .map(|((weekday, hour), mut samples)| {
                samples.sort_unstable();
                let p50 = median(&samples);
                SendTimeBucket {
                    weekday,
                    hour,
                    p50_seconds: p50,
                    sample_count: samples.len() as u32,
                }
            })
            .collect();
        buckets.sort_by(|a, b| a.weekday.cmp(&b.weekday).then(a.hour.cmp(&b.hour)));

        let best = buckets.iter().min_by_key(|b| b.p50_seconds).cloned();

        let confidence = if total >= 20 && buckets.len() >= 3 {
            SendTimeConfidence::High
        } else if total >= 8 {
            SendTimeConfidence::Medium
        } else {
            SendTimeConfidence::Low
        };

        Ok(SendTimeRecommendation {
            recipient: recipient.to_string(),
            buckets,
            best_weekday: best.as_ref().map(|b| b.weekday),
            best_hour: best.as_ref().map(|b| b.hour),
            best_p50_seconds: best.as_ref().map(|b| b.p50_seconds),
            confidence,
            sample_count: total,
        })
    }
}

fn median(sorted: &[i64]) -> i64 {
    if sorted.is_empty() {
        return 0;
    }
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Store;
    use chrono::TimeZone;
    use mxr_core::id::*;

    async fn fixture() -> (Store, AccountId) {
        let store = Store::in_memory().await.unwrap();
        let account = mxr_core::Account {
            id: AccountId::new(),
            name: "T".into(),
            email: "me@example.com".into(),
            sync_backend: None,
            send_backend: None,
            enabled: true,
        };
        store.insert_account(&account).await.unwrap();
        (store, account.id)
    }

    /// Insert a `they_replied` reply_pair via direct SQL (the
    /// production path requires both messages to exist first; for
    /// the bucketing logic we only need the timestamp + latency).
    async fn insert_they_replied(
        store: &Store,
        account_id: &AccountId,
        counterparty: &str,
        parent_received_at: DateTime<Utc>,
        latency_seconds: i64,
    ) {
        let parent_id = MessageId::new();
        let reply_id = MessageId::new();
        let now = Utc::now().timestamp();
        let parent_secs = parent_received_at.timestamp();
        let replied_secs = parent_secs + latency_seconds;
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
            .bind(counterparty)
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
        .bind(counterparty)
        .bind(parent_secs)
        .bind(replied_secs)
        .bind(latency_seconds)
        .bind(now)
        .execute(store.writer())
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn fastest_bucket_wins() {
        let (store, account) = fixture().await;
        // Friday 14:00 UTC = weekday=4, hour=14: fast (60s).
        // Monday 09:00 UTC = weekday=0, hour=9: slow (3 days).
        for _ in 0..3 {
            let t = Utc.with_ymd_and_hms(2026, 5, 1, 14, 0, 0).unwrap(); // Friday
            insert_they_replied(&store, &account, "alice@example.com", t, 60).await;
        }
        for _ in 0..3 {
            let t = Utc.with_ymd_and_hms(2026, 4, 27, 9, 0, 0).unwrap(); // Monday
            insert_they_replied(&store, &account, "alice@example.com", t, 3 * 86_400).await;
        }
        let rec = store
            .send_time_recommendation(&account, "alice@example.com")
            .await
            .unwrap();
        assert_eq!(rec.best_weekday, Some(4));
        assert_eq!(rec.best_hour, Some(14));
        assert_eq!(rec.best_p50_seconds, Some(60));
        assert_eq!(rec.sample_count, 6);
    }

    #[tokio::test]
    async fn low_sample_yields_low_confidence() {
        let (store, account) = fixture().await;
        for _ in 0..3 {
            let t = Utc.with_ymd_and_hms(2026, 5, 1, 14, 0, 0).unwrap();
            insert_they_replied(&store, &account, "alice@example.com", t, 600).await;
        }
        let rec = store
            .send_time_recommendation(&account, "alice@example.com")
            .await
            .unwrap();
        assert_eq!(rec.confidence, SendTimeConfidence::Low);
    }

    #[tokio::test]
    async fn medium_confidence_at_eight_samples() {
        let (store, account) = fixture().await;
        for _ in 0..8 {
            let t = Utc.with_ymd_and_hms(2026, 5, 1, 14, 0, 0).unwrap();
            insert_they_replied(&store, &account, "alice@example.com", t, 600).await;
        }
        let rec = store
            .send_time_recommendation(&account, "alice@example.com")
            .await
            .unwrap();
        assert_eq!(rec.confidence, SendTimeConfidence::Medium);
    }

    #[tokio::test]
    async fn high_confidence_requires_twenty_samples_and_three_buckets() {
        let (store, account) = fixture().await;
        for (count, t) in [
            (8, Utc.with_ymd_and_hms(2026, 5, 1, 14, 0, 0).unwrap()),
            (6, Utc.with_ymd_and_hms(2026, 5, 4, 9, 0, 0).unwrap()),
            (6, Utc.with_ymd_and_hms(2026, 5, 5, 11, 0, 0).unwrap()),
        ] {
            for _ in 0..count {
                insert_they_replied(&store, &account, "alice@example.com", t, 600).await;
            }
        }
        let rec = store
            .send_time_recommendation(&account, "alice@example.com")
            .await
            .unwrap();
        assert_eq!(rec.confidence, SendTimeConfidence::High);
    }

    #[tokio::test]
    async fn twenty_samples_in_one_bucket_stays_medium_confidence() {
        let (store, account) = fixture().await;
        for _ in 0..20 {
            let t = Utc.with_ymd_and_hms(2026, 5, 1, 14, 0, 0).unwrap();
            insert_they_replied(&store, &account, "alice@example.com", t, 600).await;
        }
        let rec = store
            .send_time_recommendation(&account, "alice@example.com")
            .await
            .unwrap();
        assert_eq!(rec.confidence, SendTimeConfidence::Medium);
    }

    /// `bucket_p50` lets a proposed send time be evaluated against
    /// the recommendation without re-querying the store. Returns the
    /// matching bucket's p50, or `None` if the recipient has no
    /// history in that (weekday, hour) slot.
    #[tokio::test]
    async fn bucket_p50_returns_matching_slot_or_none() {
        let (store, account) = fixture().await;
        // Friday 14:00 fast (60s), Monday 09:00 slow (3 days).
        for _ in 0..3 {
            let t = Utc.with_ymd_and_hms(2026, 5, 1, 14, 0, 0).unwrap();
            insert_they_replied(&store, &account, "alice@example.com", t, 60).await;
        }
        for _ in 0..3 {
            let t = Utc.with_ymd_and_hms(2026, 4, 27, 9, 0, 0).unwrap();
            insert_they_replied(&store, &account, "alice@example.com", t, 3 * 86_400).await;
        }
        let rec = store
            .send_time_recommendation(&account, "alice@example.com")
            .await
            .unwrap();
        assert_eq!(rec.bucket_p50(4, 14), Some(60), "Fri 14:00 bucket present");
        assert_eq!(
            rec.bucket_p50(0, 9),
            Some(3 * 86_400),
            "Mon 09:00 bucket present"
        );
        assert_eq!(
            rec.bucket_p50(2, 3),
            None,
            "Wed 03:00 has no history -> None"
        );
    }

    #[tokio::test]
    async fn unknown_recipient_returns_empty_with_low_confidence() {
        let (store, account) = fixture().await;
        let rec = store
            .send_time_recommendation(&account, "ghost@example.com")
            .await
            .unwrap();
        assert!(rec.buckets.is_empty());
        assert_eq!(rec.confidence, SendTimeConfidence::Low);
        assert_eq!(rec.sample_count, 0);
        assert!(rec.best_weekday.is_none());
    }
}
