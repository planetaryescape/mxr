//! Slice 5.1 / 5.2 of docs/reference/ai-email.md
//!
//! Briefings cache for threads and recipients. The cache is keyed on
//! (account_id, kind, subject_key); regeneration with an unchanged
//! `content_hash` reuses the existing row.

use crate::{decode_id, decode_timestamp};
use chrono::{DateTime, Utc};
use mxr_core::id::AccountId;
use mxr_core::types::CitationRef;
use sqlx::Row;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BriefingKind {
    Thread,
    Recipient,
}

impl BriefingKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Thread => "thread",
            Self::Recipient => "recipient",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextBriefing {
    pub id: String,
    pub account_id: AccountId,
    pub kind: BriefingKind,
    pub subject_key: String,
    pub content_hash: String,
    pub body_markdown: String,
    pub citations: Vec<CitationRef>,
    pub generated_at: DateTime<Utc>,
}

impl super::Store {
    pub async fn get_context_briefing(
        &self,
        account_id: &AccountId,
        kind: BriefingKind,
        subject_key: &str,
    ) -> Result<Option<ContextBriefing>, sqlx::Error> {
        let row = sqlx::query(
            r#"SELECT id, account_id, kind, subject_key, content_hash, body_markdown,
                      citations_json, generated_at
               FROM context_briefings
               WHERE account_id = ? AND kind = ? AND subject_key = ?"#,
        )
        .bind(account_id.as_str())
        .bind(kind.as_str())
        .bind(subject_key)
        .fetch_optional(self.reader())
        .await?;
        match row {
            None => Ok(None),
            Some(r) => {
                let citations_json: String = r.try_get("citations_json")?;
                let citations: Vec<CitationRef> =
                    serde_json::from_str(&citations_json).unwrap_or_default();
                Ok(Some(ContextBriefing {
                    id: r.try_get("id")?,
                    account_id: decode_id(r.try_get::<&str, _>("account_id")?)?,
                    kind: match r.try_get::<String, _>("kind")?.as_str() {
                        "recipient" => BriefingKind::Recipient,
                        _ => BriefingKind::Thread,
                    },
                    subject_key: r.try_get("subject_key")?,
                    content_hash: r.try_get("content_hash")?,
                    body_markdown: r.try_get("body_markdown")?,
                    citations,
                    generated_at: decode_timestamp(r.try_get("generated_at")?)?,
                }))
            }
        }
    }

    pub async fn upsert_context_briefing(&self, b: &ContextBriefing) -> Result<(), sqlx::Error> {
        let citations_json = serde_json::to_string(&b.citations).unwrap_or_else(|_| "[]".into());
        sqlx::query(
            r#"INSERT INTO context_briefings
               (id, account_id, kind, subject_key, content_hash, body_markdown,
                citations_json, generated_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(account_id, kind, subject_key) DO UPDATE SET
                 content_hash = excluded.content_hash,
                 body_markdown = excluded.body_markdown,
                 citations_json = excluded.citations_json,
                 generated_at = excluded.generated_at"#,
        )
        .bind(&b.id)
        .bind(b.account_id.as_str())
        .bind(b.kind.as_str())
        .bind(&b.subject_key)
        .bind(&b.content_hash)
        .bind(&b.body_markdown)
        .bind(&citations_json)
        .bind(b.generated_at.timestamp())
        .execute(self.writer())
        .await?;
        Ok(())
    }
}

pub fn new_briefing_id() -> String {
    Uuid::now_v7().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Store;

    async fn fixture() -> (Store, AccountId) {
        let store = Store::in_memory().await.unwrap();
        let acct = mxr_core::Account {
            id: AccountId::new(),
            name: "T".into(),
            email: "me@example.com".into(),
            sync_backend: None,
            send_backend: None,
            enabled: true,
        };
        store.insert_account(&acct).await.unwrap();
        (store, acct.id)
    }

    fn briefing(account: &AccountId, key: &str, body: &str, hash: &str) -> ContextBriefing {
        ContextBriefing {
            id: new_briefing_id(),
            account_id: account.clone(),
            kind: BriefingKind::Thread,
            subject_key: key.into(),
            content_hash: hash.into(),
            body_markdown: body.into(),
            citations: vec![],
            generated_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn upsert_then_get_round_trips() {
        let (store, account) = fixture().await;
        let b = briefing(&account, "th-1", "summary", "hash-1");
        store.upsert_context_briefing(&b).await.unwrap();
        let got = store
            .get_context_briefing(&account, BriefingKind::Thread, "th-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got.body_markdown, "summary");
        assert_eq!(got.content_hash, "hash-1");
    }

    #[tokio::test]
    async fn upsert_with_new_hash_replaces_body() {
        let (store, account) = fixture().await;
        store
            .upsert_context_briefing(&briefing(&account, "th-1", "v1", "h1"))
            .await
            .unwrap();
        store
            .upsert_context_briefing(&briefing(&account, "th-1", "v2", "h2"))
            .await
            .unwrap();
        let got = store
            .get_context_briefing(&account, BriefingKind::Thread, "th-1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got.body_markdown, "v2");
        assert_eq!(got.content_hash, "h2");
    }

    #[tokio::test]
    async fn missing_briefing_returns_none() {
        let (store, account) = fixture().await;
        let got = store
            .get_context_briefing(&account, BriefingKind::Recipient, "ghost@example.com")
            .await
            .unwrap();
        assert!(got.is_none());
    }
}
