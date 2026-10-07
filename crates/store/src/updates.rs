//! Updates' stores (blueprint 22, phase 4): the per-message fact cache and
//! per-source tuning. The fact itself is JSON the daemon owns; the store
//! keeps the columns it is looked up by.

use chrono::{DateTime, Utc};
use mxr_core::id::{AccountId, MessageId};
use sqlx::Row;
use std::collections::HashMap;

use crate::{decode_id, decode_timestamp, encode_json};

/// One message's cached fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateFactRow {
    pub message_id: MessageId,
    pub account_id: AccountId,
    pub source_key: String,
    pub template_key: String,
    pub message_date: DateTime<Utc>,
    pub relevant_until: Option<DateTime<Utc>>,
    pub fact_json: String,
    pub rules_version: i64,
}

/// How a source is tuned, with the let-go streak behind the mute
/// suggestion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateSourceRow {
    pub account_id: AccountId,
    pub source_key: String,
    /// `every_digest`, `changes_only`, `muted` or `breakthrough`.
    pub setting: String,
    pub decided_at: Option<DateTime<Utc>>,
    /// Digests in a row let go without opening this source.
    pub let_go_streak: i64,
    pub suggested_at: Option<DateTime<Utc>>,
}

/// One source in a let go: whether any of its mail in the cut was opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLetGo {
    pub source_key: String,
    pub opened: bool,
}

const FACT_COLUMNS: &str = "message_id, account_id, source_key, template_key, message_date, \
                            relevant_until, fact_json, rules_version";

fn fact_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<UpdateFactRow, sqlx::Error> {
    Ok(UpdateFactRow {
        message_id: decode_id(row.try_get::<&str, _>("message_id")?)?,
        account_id: decode_id(row.try_get::<&str, _>("account_id")?)?,
        source_key: row.try_get("source_key")?,
        template_key: row.try_get("template_key")?,
        message_date: decode_timestamp(row.try_get("message_date")?)?,
        relevant_until: row
            .try_get::<Option<i64>, _>("relevant_until")?
            .map(decode_timestamp)
            .transpose()?,
        fact_json: row.try_get("fact_json")?,
        rules_version: row.try_get("rules_version")?,
    })
}

fn source_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<UpdateSourceRow, sqlx::Error> {
    Ok(UpdateSourceRow {
        account_id: decode_id(row.try_get::<&str, _>("account_id")?)?,
        source_key: row.try_get("source_key")?,
        setting: row.try_get("setting")?,
        decided_at: row
            .try_get::<Option<i64>, _>("decided_at")?
            .map(decode_timestamp)
            .transpose()?,
        let_go_streak: row.try_get("let_go_streak")?,
        suggested_at: row
            .try_get::<Option<i64>, _>("suggested_at")?
            .map(decode_timestamp)
            .transpose()?,
    })
}

impl super::Store {
    /// The cached facts of these messages, whatever their rules version.
    pub async fn update_facts_for(
        &self,
        message_ids: &[MessageId],
    ) -> Result<HashMap<MessageId, UpdateFactRow>, sqlx::Error> {
        let mut out = HashMap::with_capacity(message_ids.len());
        for chunk in message_ids.chunks(crate::SQLITE_BIND_CHUNK) {
            let wanted = encode_json(&chunk.iter().map(MessageId::as_str).collect::<Vec<_>>())?;
            let sql = format!(
                "SELECT {FACT_COLUMNS} FROM update_facts
                 WHERE message_id IN (SELECT value FROM json_each(?1))"
            );
            let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
                .bind(wanted)
                .fetch_all(self.reader())
                .await?;
            for row in &rows {
                let fact = fact_from_row(row)?;
                out.insert(fact.message_id.clone(), fact);
            }
        }
        Ok(out)
    }

    /// Store derived facts, replacing older ones for the same message.
    pub async fn upsert_update_facts(
        &self,
        facts: &[UpdateFactRow],
        now: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        if facts.is_empty() {
            return Ok(());
        }
        let mut tx = self.writer().begin().await?;
        for fact in facts {
            sqlx::query(
                "INSERT INTO update_facts
                     (message_id, account_id, source_key, template_key, message_date,
                      relevant_until, fact_json, rules_version, computed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(message_id) DO UPDATE SET
                     source_key = excluded.source_key,
                     template_key = excluded.template_key,
                     message_date = excluded.message_date,
                     relevant_until = excluded.relevant_until,
                     fact_json = excluded.fact_json,
                     rules_version = excluded.rules_version,
                     computed_at = excluded.computed_at",
            )
            .bind(fact.message_id.as_str())
            .bind(fact.account_id.as_str())
            .bind(&fact.source_key)
            .bind(&fact.template_key)
            .bind(fact.message_date.timestamp())
            .bind(fact.relevant_until.map(|at| at.timestamp()))
            .bind(&fact.fact_json)
            .bind(fact.rules_version)
            .bind(now.timestamp())
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await
    }

    /// The stored plain text and HTML of these messages' bodies, for the
    /// rules that read the first line. Messages without a stored body are
    /// absent.
    pub async fn update_body_texts(
        &self,
        message_ids: &[MessageId],
    ) -> Result<HashMap<MessageId, (Option<String>, Option<String>)>, sqlx::Error> {
        let mut out = HashMap::with_capacity(message_ids.len());
        for chunk in message_ids.chunks(crate::SQLITE_BIND_CHUNK) {
            let wanted = encode_json(&chunk.iter().map(MessageId::as_str).collect::<Vec<_>>())?;
            let rows = sqlx::query(
                "SELECT message_id, text_plain, text_html FROM bodies
                 WHERE message_id IN (SELECT value FROM json_each(?1))",
            )
            .bind(wanted)
            .fetch_all(self.reader())
            .await?;
            for row in &rows {
                out.insert(
                    decode_id(row.try_get::<&str, _>("message_id")?)?,
                    (row.try_get("text_plain")?, row.try_get("text_html")?),
                );
            }
        }
        Ok(out)
    }

    /// Whether any of these accounts ever had an update read into a fact:
    /// the difference between "never had any" and "clear for now".
    pub async fn any_update_facts(&self, account_ids: &[AccountId]) -> Result<bool, sqlx::Error> {
        let wanted = encode_json(
            &account_ids
                .iter()
                .map(AccountId::as_str)
                .collect::<Vec<_>>(),
        )?;
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM update_facts
                            WHERE account_id IN (SELECT value FROM json_each(?1)))",
        )
        .bind(wanted)
        .fetch_one(self.reader())
        .await
    }

    /// The `Authentication-Results` headers stored with these messages'
    /// bodies, topmost first. Messages without a stored body are absent.
    pub async fn update_auth_results(
        &self,
        message_ids: &[MessageId],
    ) -> Result<HashMap<MessageId, Vec<String>>, sqlx::Error> {
        let mut out = HashMap::with_capacity(message_ids.len());
        for chunk in message_ids.chunks(crate::SQLITE_BIND_CHUNK) {
            let wanted = encode_json(&chunk.iter().map(MessageId::as_str).collect::<Vec<_>>())?;
            let rows = sqlx::query(
                "SELECT message_id, json_extract(metadata_json, '$.auth_results') AS auth
                 FROM bodies WHERE message_id IN (SELECT value FROM json_each(?1))",
            )
            .bind(wanted)
            .fetch_all(self.reader())
            .await?;
            for row in &rows {
                let auth: Option<String> = row.try_get("auth")?;
                let results: Vec<String> = auth
                    .as_deref()
                    .map(serde_json::from_str)
                    .transpose()
                    .map_err(|e| sqlx::Error::Decode(Box::new(e)))?
                    .unwrap_or_default();
                out.insert(decode_id(row.try_get::<&str, _>("message_id")?)?, results);
            }
        }
        Ok(out)
    }

    /// Whether this account has inbound mail from `domain` (or a host
    /// under it) dated before `before`: a source you already knew.
    pub async fn has_mail_from_domain_before(
        &self,
        account_id: &AccountId,
        domain: &str,
        before: DateTime<Utc>,
    ) -> Result<bool, sqlx::Error> {
        let domain = domain.to_ascii_lowercase();
        let at = format!("@{domain}");
        let sub = format!(".{domain}");
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS (SELECT 1 FROM messages
                            WHERE account_id = ?1 AND date < ?2 AND direction != 'outbound'
                              AND (substr(lower(from_email), -length(?3)) = ?3
                                   OR substr(lower(from_email), -length(?4)) = ?4))",
        )
        .bind(account_id.as_str())
        .bind(before.timestamp())
        .bind(at)
        .bind(sub)
        .fetch_one(self.reader())
        .await
    }

    /// Every tuned or tracked source of one account.
    pub async fn update_sources(
        &self,
        account_id: &AccountId,
    ) -> Result<HashMap<String, UpdateSourceRow>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT account_id, source_key, setting, decided_at, let_go_streak, suggested_at
             FROM update_sources WHERE account_id = ?1",
        )
        .bind(account_id.as_str())
        .fetch_all(self.reader())
        .await?;
        rows.iter()
            .map(|row| source_from_row(row).map(|source| (source.source_key.clone(), source)))
            .collect()
    }

    /// Tune a source; returns the setting it had (`every_digest` when it
    /// had none), so undo can put it back. Deciding resets the streak.
    pub async fn set_update_source(
        &self,
        account_id: &AccountId,
        source_key: &str,
        setting: &str,
        now: DateTime<Utc>,
    ) -> Result<String, sqlx::Error> {
        let mut tx = self.writer().begin().await?;
        let prior: Option<String> = sqlx::query_scalar(
            "SELECT setting FROM update_sources WHERE account_id = ?1 AND source_key = ?2",
        )
        .bind(account_id.as_str())
        .bind(source_key)
        .fetch_optional(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO update_sources (account_id, source_key, setting, decided_at, let_go_streak)
             VALUES (?1, ?2, ?3, ?4, 0)
             ON CONFLICT(account_id, source_key) DO UPDATE SET
                 setting = excluded.setting,
                 decided_at = excluded.decided_at,
                 let_go_streak = 0",
        )
        .bind(account_id.as_str())
        .bind(source_key)
        .bind(setting)
        .bind(now.timestamp())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(prior.unwrap_or_else(|| "every_digest".to_string()))
    }

    /// Count a let go per source: a source let go unopened adds one to its
    /// streak, an opened one starts again. Returns each source's streak.
    /// A source counts once per cut: letting go of the same cut again
    /// (after an undo) leaves its streak alone.
    pub async fn record_update_let_go(
        &self,
        account_id: &AccountId,
        sources: &[SourceLetGo],
        cut_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<HashMap<String, i64>, sqlx::Error> {
        let mut out = HashMap::with_capacity(sources.len());
        if sources.is_empty() {
            return Ok(out);
        }
        let mut tx = self.writer().begin().await?;
        for source in sources {
            let streak: i64 = sqlx::query_scalar(
                "INSERT INTO update_sources (account_id, source_key, let_go_streak, last_let_go_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(account_id, source_key) DO UPDATE SET
                     let_go_streak = CASE
                         WHEN ?5 THEN 0
                         WHEN last_let_go_at IS NOT NULL AND last_let_go_at >= ?6 THEN let_go_streak
                         ELSE let_go_streak + 1 END,
                     last_let_go_at = excluded.last_let_go_at
                 RETURNING let_go_streak",
            )
            .bind(account_id.as_str())
            .bind(&source.source_key)
            .bind(i64::from(!source.opened))
            .bind(now.timestamp())
            .bind(source.opened)
            .bind(cut_at.timestamp())
            .fetch_one(&mut *tx)
            .await?;
            out.insert(source.source_key.clone(), streak);
        }
        tx.commit().await?;
        Ok(out)
    }

    /// The mute question was shown for this source: ask again no sooner
    /// than a month from now.
    pub async fn mark_update_source_suggested(
        &self,
        account_id: &AccountId,
        source_key: &str,
        now: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO update_sources (account_id, source_key, suggested_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(account_id, source_key) DO UPDATE SET suggested_at = excluded.suggested_at",
        )
        .bind(account_id.as_str())
        .bind(source_key)
        .bind(now.timestamp())
        .execute(self.writer())
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::test_fixtures::TestEnvelopeBuilder;
    use crate::Store;
    use chrono::{Duration, Utc};
    use mxr_core::id::AccountId;

    use super::{SourceLetGo, UpdateFactRow};

    #[tokio::test]
    async fn facts_round_trip_and_leave_with_their_message() {
        let store = Store::in_memory().await.unwrap();
        let account = crate::test_fixtures::test_account();
        store.insert_account(&account).await.unwrap();
        let envelope = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        store.upsert_envelope(&envelope).await.unwrap();
        let now = Utc::now();
        let row = UpdateFactRow {
            message_id: envelope.id.clone(),
            account_id: account.id.clone(),
            source_key: "strava.com".into(),
            template_key: "your week: <n> runs".into(),
            message_date: envelope.date,
            relevant_until: Some(now + Duration::days(2)),
            fact_json: "{}".into(),
            rules_version: 1,
        };
        store
            .upsert_update_facts(std::slice::from_ref(&row), now)
            .await
            .unwrap();
        let got = store
            .update_facts_for(std::slice::from_ref(&envelope.id))
            .await
            .unwrap();
        assert_eq!(
            got.get(&envelope.id).map(|r| r.source_key.as_str()),
            Some("strava.com")
        );

        store
            .delete_messages_and_derived(&account.id, std::slice::from_ref(&envelope.provider_id))
            .await
            .unwrap();
        assert!(store
            .update_facts_for(&[envelope.id])
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn tuning_and_streaks() {
        let store = Store::in_memory().await.unwrap();
        let account = crate::test_fixtures::test_account();
        store.insert_account(&account).await.unwrap();
        let id: AccountId = account.id.clone();
        let now = Utc::now();
        let unopened = [SourceLetGo {
            source_key: "strava.com".into(),
            opened: false,
        }];
        for expected in 1..=3 {
            let cut = now + Duration::hours(expected);
            let streaks = store
                .record_update_let_go(&id, &unopened, cut, cut)
                .await
                .unwrap();
            assert_eq!(streaks["strava.com"], expected);
        }
        // The same cut again counts once.
        let again = now + Duration::hours(3);
        let streaks = store
            .record_update_let_go(&id, &unopened, again, again)
            .await
            .unwrap();
        assert_eq!(streaks["strava.com"], 3);
        let opened = [SourceLetGo {
            source_key: "strava.com".into(),
            opened: true,
        }];
        let later = now + Duration::hours(9);
        assert_eq!(
            store
                .record_update_let_go(&id, &opened, later, later)
                .await
                .unwrap()["strava.com"],
            0
        );
        assert_eq!(
            store
                .set_update_source(&id, "strava.com", "muted", now)
                .await
                .unwrap(),
            "every_digest"
        );
        assert_eq!(
            store
                .set_update_source(&id, "strava.com", "changes_only", now)
                .await
                .unwrap(),
            "muted"
        );
        store
            .mark_update_source_suggested(&id, "strava.com", now)
            .await
            .unwrap();
        let sources = store.update_sources(&id).await.unwrap();
        assert_eq!(sources["strava.com"].setting, "changes_only");
        assert!(sources["strava.com"].suggested_at.is_some());
    }
}
