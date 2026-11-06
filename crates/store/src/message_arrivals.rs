//! Persist authenticated receiver handling evidence and replay-safe claims.
//! The legacy received_at column means verified handling time, not SMTP receipt.

use chrono::{DateTime, Utc};
use mxr_core::id::{AccountId, MessageId};
use mxr_core::{ArrivalEvidence, SenderCheck, UnverifiedReason};
use sqlx::{Row, SqliteConnection};

use crate::{decode_optional_timestamp, Store};

/// The stored record for one message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredArrival {
    pub received_at: Option<DateTime<Utc>>,
    pub is_live: bool,
    pub message_fingerprint: Option<String>,
    pub verified_host: Option<String>,
    pub unverified_reason: Option<UnverifiedReason>,
}

/// How much inbound mail an account already holds from one host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PriorSenderHistory {
    pub messages: u32,
    pub oldest: Option<DateTime<Utc>>,
}

/// The host of an address, lowercased, without a trailing dot.
fn host_of(email: &str) -> Option<String> {
    let host = email
        .trim()
        .rsplit_once('@')?
        .1
        .trim_end_matches(['.', '>'])
        .to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

/// Keep the first verified evidence and first sync eligibility. Unverified
/// records may gain evidence after a transient DNS failure; refetch cannot
/// change the original live/backfill decision.
pub(crate) async fn record_message_arrival_tx(
    tx: &mut SqliteConnection,
    message_id: &MessageId,
    account_id: &AccountId,
    from_email: &str,
    arrival: &ArrivalEvidence,
) -> Result<(), sqlx::Error> {
    let Some(host) = host_of(from_email) else {
        return Ok(());
    };
    let (verified_host, reason) = match &arrival.sender {
        SenderCheck::Verified { host } => (Some(host.to_ascii_lowercase()), None),
        SenderCheck::Unverified { reason } => (None, Some(reason.code())),
    };
    sqlx::query(
        "INSERT INTO message_arrivals
            (message_id, account_id, from_host, received_at, verified_host, unverified_reason, is_live, message_fingerprint)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(message_id) DO UPDATE SET
            from_host = excluded.from_host,
            received_at = COALESCE(excluded.received_at, message_arrivals.received_at),
            verified_host = excluded.verified_host,
            unverified_reason = excluded.unverified_reason,
            message_fingerprint = excluded.message_fingerprint
         WHERE message_arrivals.verified_host IS NULL",
    )
    .bind(message_id.as_str())
    .bind(account_id.as_str())
    .bind(host)
    .bind(arrival.received_at.map(|at| at.timestamp()))
    .bind(verified_host)
    .bind(reason)
    .bind(arrival.is_live)
    .bind(&arrival.message_fingerprint)
    .execute(tx)
    .await?;
    Ok(())
}

impl Store {
    /// Record one message's arrival outside a sync page (a repair or a
    /// test); the same first-record-wins rule applies.
    pub async fn record_message_arrival(
        &self,
        message_id: &MessageId,
        account_id: &AccountId,
        from_email: &str,
        arrival: &ArrivalEvidence,
    ) -> Result<(), sqlx::Error> {
        let mut connection = self.writer().acquire().await?;
        record_message_arrival_tx(&mut connection, message_id, account_id, from_email, arrival)
            .await
    }

    pub async fn message_arrival(
        &self,
        message_id: &MessageId,
    ) -> Result<Option<StoredArrival>, sqlx::Error> {
        let row = sqlx::query(
            "SELECT received_at, verified_host, unverified_reason, is_live, message_fingerprint
             FROM message_arrivals WHERE message_id = ?",
        )
        .bind(message_id.as_str())
        .fetch_optional(self.reader())
        .await?;
        row.map(|row| {
            Ok(StoredArrival {
                received_at: decode_optional_timestamp(row.try_get("received_at")?)?,
                is_live: row.try_get("is_live")?,
                message_fingerprint: row.try_get("message_fingerprint")?,
                verified_host: row.try_get("verified_host")?,
                unverified_reason: row
                    .try_get::<Option<String>, _>("unverified_reason")?
                    .and_then(|code| UnverifiedReason::from_code(&code)),
            })
        })
        .transpose()
    }

    /// Inbound mail this account received from `host` strictly before
    /// `before`, never counting `candidate`. A message the provider gave no
    /// receipt time for counts for nothing.
    pub async fn sender_history(
        &self,
        account_id: &AccountId,
        host: &str,
        before: DateTime<Utc>,
        candidate: &MessageId,
    ) -> Result<PriorSenderHistory, sqlx::Error> {
        let row = sqlx::query(
            "SELECT COUNT(DISTINCT a.message_fingerprint) AS messages, MIN(a.received_at) AS oldest
             FROM message_arrivals a
             JOIN messages m ON m.id = a.message_id
             WHERE a.account_id = ?1 AND a.from_host = ?2
               AND a.received_at IS NOT NULL AND a.received_at < ?3 AND a.observed_at < ?3
               AND a.message_id <> ?4 AND m.direction = 'inbound'
               AND a.verified_host = a.from_host AND a.message_fingerprint IS NOT NULL
               AND a.message_fingerprint IS NOT (SELECT message_fingerprint FROM message_arrivals WHERE message_id=?4)",
        )
        .bind(account_id.as_str())
        .bind(host)
        .bind(before.timestamp())
        .bind(candidate.as_str())
        .fetch_one(self.reader())
        .await?;
        Ok(PriorSenderHistory {
            messages: u32::try_from(row.try_get::<i64, _>("messages")?).unwrap_or(u32::MAX),
            oldest: decode_optional_timestamp(row.try_get("oldest")?)?,
        })
    }

    /// Hosts this account has at least `min_messages` earlier inbound
    /// messages from, with their counts.
    pub async fn established_hosts(
        &self,
        account_id: &AccountId,
        before: DateTime<Utc>,
        min_messages: u32,
    ) -> Result<Vec<(String, u32)>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT a.from_host AS host, COUNT(DISTINCT a.message_fingerprint) AS messages
             FROM message_arrivals a
             JOIN messages m ON m.id = a.message_id
             WHERE a.account_id = ?1 AND a.received_at IS NOT NULL AND a.received_at < ?2 AND a.observed_at < ?2
               AND m.direction = 'inbound' AND a.verified_host=a.from_host AND a.message_fingerprint IS NOT NULL
             GROUP BY a.from_host
             HAVING COUNT(DISTINCT a.message_fingerprint) >= ?3",
        )
        .bind(account_id.as_str())
        .bind(before.timestamp())
        .bind(i64::from(min_messages))
        .fetch_all(self.reader())
        .await?;
        rows.iter()
            .map(|row| {
                Ok((
                    row.try_get("host")?,
                    u32::try_from(row.try_get::<i64, _>("messages")?).unwrap_or(u32::MAX),
                ))
            })
            .collect()
    }

    /// Claim and insert atomically: a failed insert leaves no claim, and
    /// deleting the source/to-do cannot release an earlier successful claim.
    pub async fn insert_alert_todo(
        &self,
        record: &crate::TodoRecord,
        alert_kind: &str,
    ) -> Result<bool, sqlx::Error> {
        let Some(message_id) = &record.source_message_id else {
            return Err(sqlx::Error::Protocol(
                "An alert needs a source message".into(),
            ));
        };
        let mut tx = self.writer().begin_with("BEGIN IMMEDIATE").await?;
        let fingerprint: Option<String> = sqlx::query_scalar("SELECT a.message_fingerprint FROM message_arrivals a JOIN messages m ON m.id=a.message_id AND m.account_id=a.account_id WHERE a.message_id=? AND a.account_id=? AND a.verified_host=a.from_host AND a.verified_host=? AND a.is_live=1 AND m.direction='inbound' AND (m.flags & ?) = 0 AND EXISTS (SELECT 1 FROM message_labels ml JOIN labels l ON l.id=ml.label_id AND l.account_id=m.account_id WHERE ml.message_id=m.id AND l.role='inbox')")
            .bind(message_id.as_str()).bind(record.account_id.as_str()).bind(&record.sender_domain)
            .bind(i64::from(mxr_core::MessageFlags::READ.bits())).fetch_optional(&mut *tx).await?.flatten();
        let Some(fingerprint) = fingerprint else {
            tx.rollback().await?;
            return Ok(false);
        };
        let claimed = sqlx::query(
            "INSERT INTO alert_todo_claims(account_id, dedup_key, message_id, alert_kind, message_fingerprint)
             VALUES (?, ?, ?, ?, ?) ON CONFLICT DO NOTHING",
        )
        .bind(record.account_id.as_str())
        .bind(&record.dedup_key)
        .bind(message_id.as_str())
        .bind(alert_kind)
        .bind(fingerprint)
        .execute(&mut *tx)
        .await?;
        if claimed.rows_affected() == 0 {
            tx.rollback().await?;
            return Ok(false);
        }
        let existing: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM todos WHERE account_id=? AND source_message_id=? AND kind=?)")
            .bind(record.account_id.as_str()).bind(message_id.as_str()).bind(&record.kind).fetch_one(&mut *tx).await?;
        if existing {
            tx.commit().await?;
            return Ok(false);
        }
        let sql = format!(
            "INSERT INTO todos ({}) VALUES ({})",
            crate::todos::COLUMNS,
            std::iter::repeat_n("?", crate::todos::COLUMNS.split(',').count())
                .collect::<Vec<_>>()
                .join(", ")
        );
        crate::todos::bind_record(sqlx::query(sqlx::AssertSqlSafe(sql)), record)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(true)
    }

    /// Whether this message is the email that told mxr its parcel went
    /// wrong: it signalled a problem stage and the parcel is still in that
    /// state, neither dismissed nor settled.
    pub async fn message_reports_delivery_problem(
        &self,
        message_id: &MessageId,
    ) -> Result<bool, sqlx::Error> {
        let found: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM delivery_messages dm
             JOIN deliveries d ON d.id = dm.delivery_id
             WHERE dm.message_id = ?1
               AND dm.email_kind IN ('exception', 'attempt_fail', 'returned')
               AND d.status IN ('exception', 'attempt_fail', 'returned')
               AND d.dismissed_at IS NULL AND d.resolved_at IS NULL
             LIMIT 1",
        )
        .bind(message_id.as_str())
        .fetch_optional(self.reader())
        .await?;
        Ok(found.is_some())
    }

    /// The emails that signalled a problem stage for this parcel, newest first.
    pub async fn delivery_problem_message_ids(
        &self,
        delivery_id: &mxr_core::DeliveryId,
    ) -> Result<Vec<MessageId>, sqlx::Error> {
        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT message_id FROM delivery_messages
             WHERE delivery_id = ? AND email_kind IN ('exception', 'attempt_fail', 'returned')
             ORDER BY detected_at DESC",
        )
        .bind(delivery_id.as_str())
        .fetch_all(self.reader())
        .await?;
        rows.iter().map(|id| crate::decode_id(id)).collect()
    }
}
