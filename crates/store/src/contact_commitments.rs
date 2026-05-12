use crate::{decode_id, decode_optional_timestamp, decode_timestamp};
use chrono::{DateTime, Utc};
use mxr_core::id::{AccountId, MessageId, ThreadId};
use sqlx::Row;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitmentDirection {
    Yours,
    Theirs,
}

impl CommitmentDirection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Yours => "yours",
            Self::Theirs => "theirs",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "theirs" => Self::Theirs,
            _ => Self::Yours,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitmentStatus {
    Open,
    Resolved,
    Expired,
}

impl CommitmentStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Resolved => "resolved",
            Self::Expired => "expired",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "resolved" => Self::Resolved,
            "expired" => Self::Expired,
            _ => Self::Open,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContactCommitmentRecord {
    pub id: String,
    pub account_id: AccountId,
    pub email: String,
    pub thread_id: ThreadId,
    pub direction: CommitmentDirection,
    pub status: CommitmentStatus,
    pub who_owes: String,
    pub what: String,
    pub by_when: Option<DateTime<Utc>>,
    pub evidence_msg_id: MessageId,
    pub extracted_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

impl super::Store {
    pub async fn upsert_contact_commitment(
        &self,
        record: &ContactCommitmentRecord,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"INSERT INTO contact_commitments
               (id, account_id, email, thread_id, direction, status, who_owes, what,
                by_when, evidence_msg_id, extracted_at, resolved_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(account_id, email, thread_id, direction, what, evidence_msg_id)
               DO UPDATE SET status = excluded.status, resolved_at = excluded.resolved_at"#,
        )
        .bind(&record.id)
        .bind(record.account_id.as_str())
        .bind(&record.email)
        .bind(record.thread_id.as_str())
        .bind(record.direction.as_str())
        .bind(record.status.as_str())
        .bind(&record.who_owes)
        .bind(&record.what)
        .bind(record.by_when.map(|value| value.timestamp()))
        .bind(record.evidence_msg_id.as_str())
        .bind(record.extracted_at.timestamp())
        .bind(record.resolved_at.map(|value| value.timestamp()))
        .execute(self.writer())
        .await?;
        Ok(())
    }

    pub async fn list_contact_commitments(
        &self,
        account_id: &AccountId,
        email: Option<&str>,
        status: Option<CommitmentStatus>,
    ) -> Result<Vec<ContactCommitmentRecord>, sqlx::Error> {
        let mut sql = String::from(
            r#"SELECT id, account_id, email, thread_id, direction, status, who_owes, what,
                      by_when, evidence_msg_id, extracted_at, resolved_at
               FROM contact_commitments WHERE account_id = ?"#,
        );
        if email.is_some() {
            sql.push_str(" AND email = ? COLLATE NOCASE");
        }
        if status.is_some() {
            sql.push_str(" AND status = ?");
        }
        sql.push_str(" ORDER BY extracted_at DESC");
        let mut query = sqlx::query(&sql).bind(account_id.as_str().to_string());
        if let Some(email) = email {
            query = query.bind(email.to_string());
        }
        if let Some(status) = status {
            query = query.bind(status.as_str().to_string());
        }
        let rows = query.fetch_all(self.reader()).await?;
        rows.into_iter().map(row_to_commitment).collect()
    }

    pub async fn resolve_contact_commitment(&self, id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE contact_commitments SET status = 'resolved', resolved_at = ? WHERE id = ?",
        )
        .bind(chrono::Utc::now().timestamp())
        .bind(id)
        .execute(self.writer())
        .await?;
        Ok(result.rows_affected() > 0)
    }
}

fn row_to_commitment(row: sqlx::sqlite::SqliteRow) -> Result<ContactCommitmentRecord, sqlx::Error> {
    Ok(ContactCommitmentRecord {
        id: row.get("id"),
        account_id: decode_id(row.get::<String, _>("account_id").as_str())?,
        email: row.get("email"),
        thread_id: decode_id(row.get::<String, _>("thread_id").as_str())?,
        direction: CommitmentDirection::from_str(row.get::<String, _>("direction").as_str()),
        status: CommitmentStatus::from_str(row.get::<String, _>("status").as_str()),
        who_owes: row.get("who_owes"),
        what: row.get("what"),
        by_when: decode_optional_timestamp(row.get("by_when"))?,
        evidence_msg_id: decode_id(row.get::<String, _>("evidence_msg_id").as_str())?,
        extracted_at: decode_timestamp(row.get("extracted_at"))?,
        resolved_at: decode_optional_timestamp(row.get("resolved_at"))?,
    })
}
