//! People for Messages (blueprint 22, phase 3): which addresses are one
//! person, and the contact facts a person row is built from.
//!
//! An address with no link is its own person. Links are made only by the
//! user (D117); `merge_suggestion_candidates` finds the pairs mxr may
//! suggest, and nothing here merges on its own.

use crate::{decode_optional_timestamp, decode_timestamp, trace_query};
use chrono::{DateTime, Utc};
use mxr_core::id::{AccountId, ThreadId};
use mxr_core::MessageFlags;
use sqlx::Row;
use std::time::Instant;

/// One address joined to the person it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonLink {
    /// Lowercased.
    pub email: String,
    /// The person's primary address, lowercased.
    pub person_email: String,
    pub linked_at: DateTime<Utc>,
}

/// What the contacts table knows about one address.
#[derive(Debug, Clone, PartialEq)]
pub struct PersonFacts {
    pub email: String,
    pub display_name: Option<String>,
    pub first_seen_at: DateTime<Utc>,
    pub last_inbound_at: Option<DateTime<Utc>>,
    pub last_outbound_at: Option<DateTime<Utc>>,
    pub total_inbound: u32,
    pub total_outbound: u32,
    pub replied_count: u32,
    pub cadence_days_p50: Option<f64>,
    pub is_list_sender: bool,
}

/// An address you've written to, with the name it goes by: the input to
/// the merge suggestion (same name, you've written to both).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NamedCorrespondent {
    pub email: String,
    pub display_name: String,
}

/// A merge to apply: each address in `addresses` becomes part of
/// `person_email`, and anyone linked to one of them follows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonMerge {
    pub person_email: String,
    pub addresses: Vec<String>,
}

impl super::Store {
    /// Every link in the account.
    pub async fn person_links(
        &self,
        account_id: &AccountId,
    ) -> Result<Vec<PersonLink>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT LOWER(email) AS email, LOWER(person_email) AS person_email, linked_at
             FROM person_links WHERE account_id = ?1 ORDER BY email",
        )
        .bind(account_id.as_str())
        .fetch_all(self.reader())
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(PersonLink {
                    email: row.try_get("email")?,
                    person_email: row.try_get("person_email")?,
                    linked_at: decode_timestamp(row.try_get("linked_at")?)?,
                })
            })
            .collect()
    }

    /// Apply a merge in one transaction. Addresses already linked to
    /// someone move; anyone linked to a merged address follows it, so a
    /// person never points at an address that is itself linked.
    pub async fn merge_people(
        &self,
        account_id: &AccountId,
        merge: &PersonMerge,
        at: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        let person = merge.person_email.to_ascii_lowercase();
        let mut tx = self.writer().begin().await?;
        // The target must not itself be linked to someone else.
        sqlx::query("DELETE FROM person_links WHERE account_id = ?1 AND email = ?2")
            .bind(account_id.as_str())
            .bind(&person)
            .execute(&mut *tx)
            .await?;
        for address in &merge.addresses {
            let address = address.to_ascii_lowercase();
            if address == person {
                continue;
            }
            sqlx::query(
                "UPDATE person_links SET person_email = ?3, linked_at = ?4
                 WHERE account_id = ?1 AND person_email = ?2",
            )
            .bind(account_id.as_str())
            .bind(&address)
            .bind(&person)
            .bind(at.timestamp())
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "INSERT INTO person_links (account_id, email, person_email, linked_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(account_id, email) DO UPDATE SET
                   person_email = excluded.person_email,
                   linked_at = excluded.linked_at",
            )
            .bind(account_id.as_str())
            .bind(&address)
            .bind(&person)
            .bind(at.timestamp())
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await
    }

    /// Take one address back out of its person. When it was the person's
    /// primary address, the oldest remaining link becomes the primary.
    /// Returns whether anything changed.
    pub async fn split_person(
        &self,
        account_id: &AccountId,
        address: &str,
    ) -> Result<bool, sqlx::Error> {
        let address = address.to_ascii_lowercase();
        let mut tx = self.writer().begin().await?;
        let removed = sqlx::query("DELETE FROM person_links WHERE account_id = ?1 AND email = ?2")
            .bind(account_id.as_str())
            .bind(&address)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        let heir: Option<String> = sqlx::query_scalar(
            "SELECT LOWER(email) FROM person_links
             WHERE account_id = ?1 AND person_email = ?2
             ORDER BY linked_at, email LIMIT 1",
        )
        .bind(account_id.as_str())
        .bind(&address)
        .fetch_optional(&mut *tx)
        .await?;
        let mut moved = 0;
        if let Some(heir) = heir {
            sqlx::query("DELETE FROM person_links WHERE account_id = ?1 AND email = ?2")
                .bind(account_id.as_str())
                .bind(&heir)
                .execute(&mut *tx)
                .await?;
            moved = sqlx::query(
                "UPDATE person_links SET person_email = ?3
                 WHERE account_id = ?1 AND person_email = ?2",
            )
            .bind(account_id.as_str())
            .bind(&address)
            .bind(&heir)
            .execute(&mut *tx)
            .await?
            .rows_affected()
                + 1;
        }
        tx.commit().await?;
        Ok(removed + moved > 0)
    }

    /// Contact facts for `emails` (matched case-insensitively).
    pub async fn people_facts(
        &self,
        account_id: &AccountId,
        emails: &[String],
    ) -> Result<Vec<PersonFacts>, sqlx::Error> {
        if emails.is_empty() {
            return Ok(Vec::new());
        }
        let started_at = Instant::now();
        let wanted = serde_json::to_string(emails).map_err(|e| sqlx::Error::Encode(Box::new(e)))?;
        let rows = sqlx::query(
            r#"SELECT email, display_name, first_seen_at, last_inbound_at, last_outbound_at,
                      total_inbound, total_outbound, replied_count, cadence_days_p50,
                      is_list_sender
               FROM contacts
               WHERE account_id = ?1
                 AND email IN (SELECT LOWER(value) FROM json_each(?2))"#,
        )
        .bind(account_id.as_str())
        .bind(wanted)
        .fetch_all(self.reader())
        .await?;
        let facts = rows
            .into_iter()
            .map(|row| {
                Ok(PersonFacts {
                    email: row.try_get::<String, _>("email")?.to_ascii_lowercase(),
                    display_name: row.try_get("display_name")?,
                    first_seen_at: decode_timestamp(row.try_get("first_seen_at")?)?,
                    last_inbound_at: decode_optional_timestamp(row.try_get("last_inbound_at")?)?,
                    last_outbound_at: decode_optional_timestamp(row.try_get("last_outbound_at")?)?,
                    total_inbound: count(row.try_get("total_inbound")?),
                    total_outbound: count(row.try_get("total_outbound")?),
                    replied_count: count(row.try_get("replied_count")?),
                    cadence_days_p50: row.try_get("cadence_days_p50")?,
                    is_list_sender: row.try_get::<i64, _>("is_list_sender")? != 0,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        trace_query("people.facts", started_at, facts.len());
        Ok(facts)
    }

    /// Addresses you've written to that share a display name with another
    /// such address, grouped by name: what a merge suggestion is made of.
    pub async fn merge_suggestion_candidates(
        &self,
        account_id: &AccountId,
    ) -> Result<Vec<NamedCorrespondent>, sqlx::Error> {
        let started_at = Instant::now();
        let rows = sqlx::query(
            r#"WITH named AS (
                   SELECT LOWER(email) AS email, TRIM(display_name) AS display_name
                   FROM contacts
                   WHERE account_id = ?1
                     AND total_outbound > 0
                     AND is_list_sender = 0
                     AND display_name IS NOT NULL
                     AND TRIM(display_name) <> ''
                     AND INSTR(display_name, '@') = 0
               )
               SELECT email, display_name FROM named
               WHERE LOWER(display_name) IN (
                   SELECT LOWER(display_name) FROM named
                   GROUP BY LOWER(display_name) HAVING COUNT(*) >= 2
               )
               ORDER BY LOWER(display_name), email"#,
        )
        .bind(account_id.as_str())
        .fetch_all(self.reader())
        .await?;
        let named = rows
            .into_iter()
            .map(|row| {
                Ok(NamedCorrespondent {
                    email: row.try_get("email")?,
                    display_name: row.try_get("display_name")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        trace_query("people.merge_candidates", started_at, named.len());
        Ok(named)
    }

    /// The person's conversations, newest first: threads where they wrote,
    /// or where you wrote to them (To, Cc or Bcc), with mail dated at or
    /// after `since`.
    pub async fn person_thread_ids(
        &self,
        account_id: &AccountId,
        emails: &[String],
        since: DateTime<Utc>,
        limit: u32,
    ) -> Result<Vec<ThreadId>, sqlx::Error> {
        if emails.is_empty() {
            return Ok(Vec::new());
        }
        let started_at = Instant::now();
        let hidden_flags = i64::from((MessageFlags::TRASH | MessageFlags::SPAM).bits());
        let wanted: Vec<String> = emails.iter().map(|e| e.to_ascii_lowercase()).collect();
        let wanted =
            serde_json::to_string(&wanted).map_err(|e| sqlx::Error::Encode(Box::new(e)))?;
        let rows = sqlx::query(
            r#"SELECT m.thread_id AS thread_id, MAX(m.date) AS last
               FROM messages m
               WHERE m.account_id = ?1
                 AND m.date >= ?3
                 AND (m.flags & ?4) = 0
                 AND (
                     LOWER(m.from_email) IN (SELECT value FROM json_each(?2))
                     OR (m.direction = 'outbound' AND EXISTS (
                         SELECT 1
                         FROM json_each(json_array(json(m.to_addrs), json(m.cc_addrs), json(m.bcc_addrs))) lists,
                              json_each(lists.value) a
                         WHERE LOWER(json_extract(a.value, '$.email'))
                               IN (SELECT value FROM json_each(?2))
                     ))
                 )
               GROUP BY m.thread_id
               ORDER BY last DESC
               LIMIT ?5"#,
        )
        .bind(account_id.as_str())
        .bind(wanted)
        .bind(since.timestamp())
        .bind(hidden_flags)
        .bind(i64::from(limit))
        .fetch_all(self.reader())
        .await?;
        let threads = rows
            .into_iter()
            .map(|row| crate::decode_id(row.try_get::<&str, _>("thread_id")?))
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        trace_query("people.thread_ids", started_at, threads.len());
        Ok(threads)
    }
}

fn count(value: i64) -> u32 {
    u32::try_from(value.max(0)).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests;
