//! Audit log + override tokens for the pre-send safety pipeline.
//!
//! - `draft_safety_runs` records each safety check (issues already
//!   redacted by the safety crate). Used by `mxr doctor` and TUI debug
//!   panels.
//! - `draft_safety_overrides` issues single-use bypass tokens for
//!   Blocker-severity issues. Tokens are minted by the daemon when a
//!   `--check` returns Blocked, and consumed at most once on the
//!   following `SendDraft` / `SendStoredDraft` call.

use chrono::{DateTime, Utc};
use mxr_core::id::{AccountId, DraftId};
use mxr_core::types::{DraftSafetyIssueCode, DraftSafetyReport, DraftSafetyVerdict};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftSafetyRunRecord {
    pub id: String,
    pub draft_id: Option<DraftId>,
    pub account_id: AccountId,
    pub verdict: DraftSafetyVerdict,
    pub issues_json: String,
    pub checked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftSafetyOverrideRecord {
    pub token: String,
    pub draft_id: Option<DraftId>,
    pub issue_kinds: Vec<DraftSafetyIssueCode>,
    pub created_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
}

impl super::Store {
    /// Persist one run of the safety pipeline. Returns the audit row id.
    /// `report.issues` should already be redacted (the safety crate
    /// guarantees `detail` previews never echo raw secrets).
    pub async fn record_safety_run(
        &self,
        account_id: &AccountId,
        draft_id: Option<&DraftId>,
        report: &DraftSafetyReport,
    ) -> Result<String, sqlx::Error> {
        let id = Uuid::now_v7().to_string();
        let account = account_id.as_str();
        let draft = draft_id.map(|d| d.as_str());
        let verdict = match report.verdict {
            DraftSafetyVerdict::Safe => "safe",
            DraftSafetyVerdict::Warn => "warn",
            DraftSafetyVerdict::Blocked => "blocked",
        };
        let issues_json = serde_json::to_string(&report.issues).unwrap_or_else(|_| "[]".into());
        let checked_at = report.checked_at.unwrap_or_else(Utc::now).timestamp();
        sqlx::query(
            r#"INSERT INTO draft_safety_runs
               (id, draft_id, account_id, verdict, issues_json, checked_at)
               VALUES (?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&id)
        .bind(draft)
        .bind(account)
        .bind(verdict)
        .bind(&issues_json)
        .bind(checked_at)
        .execute(self.writer())
        .await?;
        Ok(id)
    }

    /// Mint a single-use override token authorising a future send to
    /// bypass the listed Blocker issue kinds. Returns the token string.
    pub async fn mint_safety_override(
        &self,
        draft_id: Option<&DraftId>,
        issue_kinds: &[DraftSafetyIssueCode],
    ) -> Result<String, sqlx::Error> {
        let token = Uuid::now_v7().to_string();
        let draft = draft_id.map(|d| d.as_str());
        let kinds_json = serde_json::to_string(issue_kinds).unwrap_or_else(|_| "[]".into());
        let now = Utc::now().timestamp();
        sqlx::query(
            r#"INSERT INTO draft_safety_overrides
               (token, draft_id, issue_kinds_json, created_at, used_at)
               VALUES (?, ?, ?, ?, NULL)"#,
        )
        .bind(&token)
        .bind(draft)
        .bind(&kinds_json)
        .bind(now)
        .execute(self.writer())
        .await?;
        Ok(token)
    }

    /// Atomically consume a single-use override token. Returns the set
    /// of issue codes the token covers. `Ok(None)` means the token does
    /// not exist OR has already been used.
    pub async fn consume_safety_override(
        &self,
        token: &str,
    ) -> Result<Option<Vec<DraftSafetyIssueCode>>, sqlx::Error> {
        let now = Utc::now().timestamp();
        // Atomic UPDATE … WHERE used_at IS NULL RETURNING. Guarantees
        // single-use even under concurrent send attempts.
        let row = sqlx::query(
            r#"UPDATE draft_safety_overrides
               SET used_at = ?
               WHERE token = ? AND used_at IS NULL
               RETURNING issue_kinds_json"#,
        )
        .bind(now)
        .bind(token)
        .fetch_optional(self.writer())
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let kinds_json: String = row.get(0);
        let kinds: Vec<DraftSafetyIssueCode> =
            serde_json::from_str(&kinds_json).unwrap_or_default();
        Ok(Some(kinds))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Store;
    use mxr_core::types::{DraftSafetyIssue, DraftSafetySeverity};

    async fn temp_store() -> Store {
        let temp_dir = std::env::temp_dir().join(format!(
            "mxr-safety-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).unwrap();
        Store::new(&temp_dir.join("mxr.db")).await.unwrap()
    }

    #[tokio::test]
    async fn record_run_round_trips_verdict() {
        let store = temp_store().await;
        let account = AccountId::new();
        let report = DraftSafetyReport {
            allowed: false,
            verdict: DraftSafetyVerdict::Blocked,
            checked_at: Some(Utc::now()),
            issues: vec![DraftSafetyIssue::new(
                DraftSafetyIssueCode::PiiSecret,
                DraftSafetySeverity::Blocker,
                "secret",
            )],
        };
        let id = store
            .record_safety_run(&account, None, &report)
            .await
            .unwrap();
        assert!(!id.is_empty());
        // Verify by querying directly.
        let row = sqlx::query("SELECT verdict, issues_json FROM draft_safety_runs WHERE id = ?")
            .bind(&id)
            .fetch_one(store.reader())
            .await
            .unwrap();
        let verdict: String = row.get(0);
        let issues_json: String = row.get(1);
        assert_eq!(verdict, "blocked");
        assert!(issues_json.contains("pii_secret"));
    }

    #[tokio::test]
    async fn override_token_is_single_use() {
        let store = temp_store().await;
        let token = store
            .mint_safety_override(None, &[DraftSafetyIssueCode::PiiSecret])
            .await
            .unwrap();

        // First consume returns the issue kinds.
        let kinds = store.consume_safety_override(&token).await.unwrap();
        assert_eq!(kinds, Some(vec![DraftSafetyIssueCode::PiiSecret]));

        // Second consume returns None (token already used).
        let again = store.consume_safety_override(&token).await.unwrap();
        assert_eq!(again, None);
    }

    #[tokio::test]
    async fn unknown_token_returns_none() {
        let store = temp_store().await;
        let result = store
            .consume_safety_override("00000000-0000-0000-0000-000000000000")
            .await
            .unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn override_records_all_listed_issue_kinds() {
        let store = temp_store().await;
        let kinds_in = vec![
            DraftSafetyIssueCode::PiiSecret,
            DraftSafetyIssueCode::WrongRecipient,
        ];
        let token = store.mint_safety_override(None, &kinds_in).await.unwrap();
        let kinds_out = store
            .consume_safety_override(&token)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(kinds_out, kinds_in);
    }
}
