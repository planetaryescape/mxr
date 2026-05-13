//! Slice 4.2 of docs/ai-email/04-timing-cadence.md.
//!
//! User-curated cadence watchlist. Add/remove contacts the user
//! wants alerted on when relationship cadence drifts past expected.

use crate::{decode_id, decode_optional_timestamp, decode_timestamp};
use chrono::{DateTime, Utc};
use mxr_core::id::AccountId;
use sqlx::Row;

const DEFAULT_EXPECTED_DAYS: f64 = 30.0;

#[derive(Debug, Clone, PartialEq)]
pub struct RelationshipWatchEntry {
    pub account_id: AccountId,
    pub email: String,
    pub expected_days: Option<f64>,
    pub note: Option<String>,
    pub added_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CadenceDriftRow {
    pub email: String,
    pub display_name: Option<String>,
    pub last_contact_at: Option<DateTime<Utc>>,
    pub expected_days: f64,
    pub drift_days: f64,
    pub total_volume: u32,
}

impl super::Store {
    pub async fn watch_cadence(
        &self,
        entry: &RelationshipWatchEntry,
        allow_list_sender: bool,
    ) -> Result<(), String> {
        // Reject list senders unless the override is set, with a
        // named reason so the CLI can surface it.
        let is_list: Option<i64> = sqlx::query_scalar(
            "SELECT is_list_sender FROM contacts
             WHERE account_id = ? AND LOWER(email) = LOWER(?)",
        )
        .bind(entry.account_id.as_str())
        .bind(&entry.email)
        .fetch_optional(self.reader())
        .await
        .map_err(|e| e.to_string())?
        .flatten();
        if matches!(is_list, Some(1)) && !allow_list_sender {
            return Err(format!(
                "{} is a list sender; use --allow-list-sender to watch anyway",
                entry.email
            ));
        }
        sqlx::query(
            r#"INSERT INTO relationship_watchlist (account_id, email, expected_days, note, added_at)
               VALUES (?, ?, ?, ?, ?)
               ON CONFLICT(account_id, email) DO UPDATE SET
                 expected_days = excluded.expected_days,
                 note = excluded.note"#,
        )
        .bind(entry.account_id.as_str())
        .bind(&entry.email)
        .bind(entry.expected_days)
        .bind(entry.note.as_ref())
        .bind(entry.added_at.timestamp())
        .execute(self.writer())
        .await
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn unwatch_cadence(
        &self,
        account_id: &AccountId,
        email: &str,
    ) -> Result<bool, sqlx::Error> {
        let res = sqlx::query(
            "DELETE FROM relationship_watchlist
             WHERE account_id = ? AND LOWER(email) = LOWER(?)",
        )
        .bind(account_id.as_str())
        .bind(email)
        .execute(self.writer())
        .await?;
        Ok(res.rows_affected() > 0)
    }

    pub async fn list_cadence_watch(
        &self,
        account_id: &AccountId,
    ) -> Result<Vec<RelationshipWatchEntry>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT account_id, email, expected_days, note, added_at
             FROM relationship_watchlist
             WHERE account_id = ?
             ORDER BY added_at ASC",
        )
        .bind(account_id.as_str())
        .fetch_all(self.reader())
        .await?;
        rows.into_iter()
            .map(|r| {
                Ok(RelationshipWatchEntry {
                    account_id: decode_id(r.try_get::<&str, _>("account_id")?)?,
                    email: r.try_get("email")?,
                    expected_days: r.try_get("expected_days")?,
                    note: r.try_get("note")?,
                    added_at: decode_timestamp(r.try_get("added_at")?)?,
                })
            })
            .collect()
    }

    pub async fn list_cadence_drift(
        &self,
        account_id: &AccountId,
    ) -> Result<Vec<CadenceDriftRow>, sqlx::Error> {
        let rows = sqlx::query(
            r#"SELECT
                  w.email AS email,
                  contacts.display_name AS display_name,
                  COALESCE(
                    MAX(contacts.last_inbound_at, contacts.last_outbound_at),
                    contacts.last_inbound_at,
                    contacts.last_outbound_at
                  ) AS last_contact_at,
                  COALESCE(w.expected_days, contacts.cadence_days_p50, ?2) AS expected_days,
                  COALESCE(contacts.total_inbound, 0) + COALESCE(contacts.total_outbound, 0) AS volume
                FROM relationship_watchlist w
                LEFT JOIN contacts
                  ON contacts.account_id = w.account_id
                 AND LOWER(contacts.email) = LOWER(w.email)
                WHERE w.account_id = ?1"#,
        )
        .bind(account_id.as_str())
        .bind(DEFAULT_EXPECTED_DAYS)
        .fetch_all(self.reader())
        .await?;

        let now = Utc::now();
        let mut out = Vec::new();
        for r in rows {
            let last_secs: Option<i64> = r.try_get("last_contact_at").ok();
            let last_contact_at = last_secs.and_then(|s| decode_optional_timestamp(Some(s)).ok().flatten());
            let expected_days: f64 = r.try_get::<f64, _>("expected_days").unwrap_or(DEFAULT_EXPECTED_DAYS);
            let drift_days = match last_contact_at {
                Some(when) => {
                    let elapsed = (now - when).num_seconds() as f64 / 86_400.0;
                    elapsed - expected_days
                }
                None => f64::INFINITY,
            };
            if drift_days <= 0.0 {
                continue;
            }
            let display_name: Option<String> = r.try_get("display_name").ok();
            let volume: i64 = r.try_get("volume").unwrap_or(0);
            out.push(CadenceDriftRow {
                email: r.try_get("email")?,
                display_name,
                last_contact_at,
                expected_days,
                drift_days,
                total_volume: volume.max(0) as u32,
            });
        }
        out.sort_by(|a, b| {
            b.drift_days
                .partial_cmp(&a.drift_days)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(b.total_volume.cmp(&a.total_volume))
        });
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Store;
    use chrono::Duration;

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

    fn entry(account: &AccountId, email: &str) -> RelationshipWatchEntry {
        RelationshipWatchEntry {
            account_id: account.clone(),
            email: email.into(),
            expected_days: None,
            note: None,
            added_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn watch_then_list_round_trips() {
        let (store, account) = fixture().await;
        store
            .watch_cadence(&entry(&account, "alice@example.com"), false)
            .await
            .unwrap();
        let rows = store.list_cadence_watch(&account).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].email, "alice@example.com");
    }

    #[tokio::test]
    async fn unwatch_removes_row() {
        let (store, account) = fixture().await;
        store
            .watch_cadence(&entry(&account, "alice@example.com"), false)
            .await
            .unwrap();
        let removed = store
            .unwatch_cadence(&account, "alice@example.com")
            .await
            .unwrap();
        assert!(removed);
        assert!(store.list_cadence_watch(&account).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn list_sender_is_rejected_without_override() {
        let (store, account) = fixture().await;
        // Insert a contacts row marked as list sender.
        sqlx::query(
            "INSERT INTO contacts (account_id, email, display_name, first_seen_at, last_seen_at,
               last_inbound_at, last_outbound_at, total_inbound, total_outbound, replied_count,
               cadence_days_p50, is_list_sender, list_id, refreshed_at)
             VALUES (?, ?, NULL, 0, 0, NULL, NULL, 0, 0, 0, NULL, 1, NULL, 0)",
        )
        .bind(account.as_str())
        .bind("newsletter@example.com")
        .execute(store.writer())
        .await
        .unwrap();
        let err = store
            .watch_cadence(&entry(&account, "newsletter@example.com"), false)
            .await
            .expect_err("must reject list sender by default");
        assert!(err.contains("list sender"), "{err}");
        assert!(err.contains("--allow-list-sender"), "{err}");
        // With override flag, succeeds.
        store
            .watch_cadence(&entry(&account, "newsletter@example.com"), true)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn drift_reports_only_overdue_contacts() {
        let (store, account) = fixture().await;
        let recent = Utc::now() - Duration::days(2);
        let stale = Utc::now() - Duration::days(60);
        // Recent contact: cadence 7d, drift would be -5d => excluded.
        sqlx::query(
            "INSERT INTO contacts (account_id, email, display_name, first_seen_at, last_seen_at,
               last_inbound_at, last_outbound_at, total_inbound, total_outbound, replied_count,
               cadence_days_p50, is_list_sender, list_id, refreshed_at)
             VALUES (?, ?, NULL, 0, ?, ?, NULL, 5, 5, 5, 7.0, 0, NULL, 0)",
        )
        .bind(account.as_str())
        .bind("alice@example.com")
        .bind(recent.timestamp())
        .bind(recent.timestamp())
        .execute(store.writer())
        .await
        .unwrap();
        // Stale contact: cadence 7d, drift would be 53d => included.
        sqlx::query(
            "INSERT INTO contacts (account_id, email, display_name, first_seen_at, last_seen_at,
               last_inbound_at, last_outbound_at, total_inbound, total_outbound, replied_count,
               cadence_days_p50, is_list_sender, list_id, refreshed_at)
             VALUES (?, ?, NULL, 0, ?, ?, NULL, 8, 8, 8, 7.0, 0, NULL, 0)",
        )
        .bind(account.as_str())
        .bind("bob@example.com")
        .bind(stale.timestamp())
        .bind(stale.timestamp())
        .execute(store.writer())
        .await
        .unwrap();
        store
            .watch_cadence(&entry(&account, "alice@example.com"), false)
            .await
            .unwrap();
        store
            .watch_cadence(&entry(&account, "bob@example.com"), false)
            .await
            .unwrap();
        let drift = store.list_cadence_drift(&account).await.unwrap();
        assert_eq!(drift.len(), 1);
        assert_eq!(drift[0].email, "bob@example.com");
        assert!(drift[0].drift_days >= 50.0);
    }
}
