use caseless::default_case_fold_str;
use mxr_core::{CategoryId, MessageId, PersonalCategory};
use serde::{Deserialize, Serialize};
use sqlx::{AssertSqlSafe, Row};
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

const UNDO_TTL_SECONDS: i64 = 60;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategoryMembershipVersion {
    pub message_id: MessageId,
    pub category_id: CategoryId,
    pub revision: i64,
    pub assigned: bool,
    #[serde(default)]
    pub manual_decision: Option<bool>,
    #[serde(default)]
    pub sources: Vec<crate::RuleCategorySource>,
}

#[derive(Debug, Clone)]
pub struct CategoryMembershipChange {
    pub before: CategoryMembershipVersion,
    pub assigned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UndoEntry {
    message_id: MessageId,
    category_id: CategoryId,
    prior_assigned: bool,
    #[serde(default, deserialize_with = "deserialize_prior_manual_decision")]
    prior_manual_decision: Option<Option<bool>>,
    after_revision: i64,
}

// Missing means a C1 snapshot; explicit null is a C2 undo of an absent override.
fn deserialize_prior_manual_decision<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Option<bool>>, D::Error> {
    Option::<bool>::deserialize(deserializer).map(Some)
}

pub fn normalize_category_name(input: &str) -> Result<(String, String), String> {
    let name: String = input.trim().nfkc().collect();
    if name.is_empty() || name.chars().all(char::is_control) || name.chars().any(char::is_control) {
        return Err("category name must contain visible text and no control characters".into());
    }
    let key: String = default_case_fold_str(&name).nfkc().collect();
    Ok((name, key))
}

impl super::Store {
    pub async fn preview_personal_category_name(
        &self,
        category_id: Option<&CategoryId>,
        input: &str,
    ) -> Result<(String, bool), String> {
        let (name, key) = normalize_category_name(input)?;
        if let Some(category_id) = category_id {
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM personal_categories WHERE id = ?)")
                    .bind(category_id.as_str())
                    .fetch_one(self.reader())
                    .await
                    .map_err(|error| error.to_string())?;
            if !exists {
                return Err("category not found".into());
            }
        }
        let duplicate: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM personal_categories WHERE normalized_name = ? AND (? IS NULL OR id <> ?))",
        )
        .bind(key)
        .bind(category_id.map(CategoryId::as_str))
        .bind(category_id.map(CategoryId::as_str))
        .fetch_one(self.reader())
        .await
        .map_err(|error| error.to_string())?;
        Ok((name, !duplicate))
    }

    pub async fn list_personal_categories(
        &self,
        account_id: Option<&mxr_core::AccountId>,
    ) -> Result<Vec<PersonalCategory>, String> {
        let rows = sqlx::query(
            "SELECT c.id, c.name, COUNT(m.id) AS total_count,
                    SUM(CASE WHEN m.id IS NOT NULL AND (m.flags & 1) = 0 THEN 1 ELSE 0 END) AS unread_count
             FROM personal_categories c
             LEFT JOIN effective_category_memberships cm ON cm.category_id = c.id AND cm.assigned = 1
             LEFT JOIN messages m ON m.id = cm.message_id AND (? IS NULL OR m.account_id = ?)
             GROUP BY c.id, c.name ORDER BY c.normalized_name, c.id",
        )
        .bind(account_id.map(mxr_core::AccountId::as_str))
        .bind(account_id.map(mxr_core::AccountId::as_str))
        .fetch_all(self.reader())
        .await
        .map_err(|error| error.to_string())?;
        rows.into_iter()
            .map(|row| {
                Ok(PersonalCategory {
                    id: row
                        .get::<String, _>("id")
                        .parse()
                        .map_err(|error: uuid::Error| error.to_string())?,
                    name: row.get("name"),
                    unread_count: row.get::<i64, _>("unread_count").max(0) as u32,
                    total_count: row.get::<i64, _>("total_count").max(0) as u32,
                })
            })
            .collect()
    }

    pub async fn create_personal_category(&self, name: &str) -> Result<PersonalCategory, String> {
        let (name, key) = normalize_category_name(name)?;
        let id = CategoryId::new();
        let now = chrono::Utc::now().timestamp();
        sqlx::query("INSERT INTO personal_categories (id, name, normalized_name, created_at, updated_at) VALUES (?, ?, ?, ?, ?)")
            .bind(id.as_str()).bind(&name).bind(key).bind(now).bind(now)
            .execute(self.writer()).await.map_err(|error| {
                if error.to_string().contains("UNIQUE constraint failed") {
                    "a category with that name already exists".to_string()
                } else { error.to_string() }
            })?;
        Ok(PersonalCategory {
            id,
            name,
            unread_count: 0,
            total_count: 0,
        })
    }

    pub async fn rename_personal_category(
        &self,
        category_id: &CategoryId,
        input: &str,
    ) -> Result<PersonalCategory, String> {
        let (name, key) = normalize_category_name(input)?;
        let now = chrono::Utc::now().timestamp();
        let result = sqlx::query("UPDATE personal_categories SET name = ?, normalized_name = ?, updated_at = ? WHERE id = ?")
            .bind(&name).bind(key).bind(now).bind(category_id.as_str())
            .execute(self.writer()).await.map_err(|error| {
                if error.to_string().contains("UNIQUE constraint failed") {
                    "a category with that name already exists".to_string()
                } else { error.to_string() }
            })?;
        if result.rows_affected() == 0 {
            return Err("category not found".into());
        }
        let mut category = self
            .list_personal_categories(None)
            .await?
            .into_iter()
            .find(|item| item.id == *category_id)
            .ok_or_else(|| "category not found".to_string())?;
        category.name = name;
        Ok(category)
    }

    pub async fn personal_category_exists(&self, category_id: &CategoryId) -> Result<bool, String> {
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM personal_categories WHERE id = ?)")
            .bind(category_id.as_str())
            .fetch_one(self.reader())
            .await
            .map_err(|error| error.to_string())
    }

    pub async fn get_personal_category(
        &self,
        category_id: &CategoryId,
    ) -> Result<(String, u32), String> {
        let row = sqlx::query(
            "SELECT c.name, COUNT(cm.message_id) AS total_count FROM personal_categories c
             LEFT JOIN effective_category_memberships cm ON cm.category_id = c.id AND cm.assigned = 1
             WHERE c.id = ? GROUP BY c.id, c.name",
        )
        .bind(category_id.as_str())
        .fetch_optional(self.reader())
        .await
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "category not found".to_string())?;
        Ok((
            row.get("name"),
            row.get::<i64, _>("total_count").max(0) as u32,
        ))
    }

    pub async fn category_membership_versions(
        &self,
        account_id: Option<&mxr_core::AccountId>,
        message_ids: &[MessageId],
        category_ids: &[CategoryId],
    ) -> Result<Vec<CategoryMembershipVersion>, String> {
        if message_ids.is_empty() || category_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut tx = self.writer().begin().await.map_err(|e| e.to_string())?;
        for message_id in message_ids {
            let row = sqlx::query("SELECT account_id FROM messages WHERE id = ?")
                .bind(message_id.as_str())
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "message not found".to_string())?;
            if account_id
                .is_some_and(|account| row.get::<String, _>("account_id") != account.as_str())
            {
                return Err("message is outside the requested account".into());
            }
        }
        for category_id in category_ids {
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM personal_categories WHERE id = ?)")
                    .bind(category_id.as_str())
                    .fetch_one(&mut *tx)
                    .await
                    .map_err(|e| e.to_string())?;
            if !exists {
                return Err("category not found".into());
            }
        }
        let mut sources = Vec::new();
        for id in message_ids {
            sources.extend(
                crate::category_rules::read_sources(&mut tx, Some(id))
                    .await
                    .map_err(|e| e.to_string())?,
            );
        }
        let mut result = Vec::with_capacity(message_ids.len() * category_ids.len());
        for message_id in message_ids {
            for category_id in category_ids {
                let row = sqlx::query("SELECT assigned, revision, manual_decision FROM message_category_memberships WHERE message_id = ? AND category_id = ?")
                    .bind(message_id.as_str()).bind(category_id.as_str())
                    .fetch_optional(&mut *tx).await.map_err(|e| e.to_string())?;
                let manual_decision = row
                    .as_ref()
                    .and_then(|r| r.get::<Option<i64>, _>("manual_decision"))
                    .map(|v| v != 0);
                let (_, revision) = row.map_or((false, 0), |row| {
                    (
                        row.get::<i64, _>("assigned") != 0,
                        row.get::<i64, _>("revision"),
                    )
                });
                result.push(CategoryMembershipVersion {
                    message_id: message_id.clone(),
                    category_id: category_id.clone(),
                    revision,
                    assigned: manual_decision.unwrap_or_else(|| {
                        sources
                            .iter()
                            .filter(|s| {
                                s.message_id == *message_id && s.category_id == *category_id
                            })
                            .min_by_key(|s| (s.priority, &s.rule_id))
                            .is_some_and(|s| s.assigned)
                    }),
                    manual_decision,
                    sources: sources
                        .iter()
                        .filter(|s| s.message_id == *message_id && s.category_id == *category_id)
                        .cloned()
                        .collect(),
                });
            }
        }
        tx.commit().await.map_err(|e| e.to_string())?;
        Ok(result)
    }

    pub async fn list_category_message_ids(
        &self,
        category_id: &CategoryId,
        account_id: Option<&mxr_core::AccountId>,
        limit: u32,
        offset: u32,
    ) -> Result<(Vec<MessageId>, u32), String> {
        let account_filter = account_id.map_or("", |_| " AND m.account_id = ?");
        let sql = format!("SELECT m.id FROM messages m JOIN effective_category_memberships cm ON cm.message_id = m.id WHERE cm.category_id = ? AND cm.assigned = 1{account_filter} ORDER BY m.date DESC, m.id LIMIT ? OFFSET ?");
        let count_sql = format!("SELECT COUNT(*) FROM messages m JOIN effective_category_memberships cm ON cm.message_id = m.id WHERE cm.category_id = ? AND cm.assigned = 1{account_filter}");
        let mut query =
            sqlx::query_scalar::<_, String>(AssertSqlSafe(sql.as_str())).bind(category_id.as_str());
        let mut count_query = sqlx::query_scalar::<_, i64>(AssertSqlSafe(count_sql.as_str()))
            .bind(category_id.as_str());
        if let Some(account_id) = account_id {
            query = query.bind(account_id.as_str());
            count_query = count_query.bind(account_id.as_str());
        }
        let ids = query
            .bind(limit.min(500))
            .bind(offset)
            .fetch_all(self.reader())
            .await
            .map_err(|e| e.to_string())?;
        let total = count_query
            .fetch_one(self.reader())
            .await
            .map_err(|e| e.to_string())?;
        Ok((
            ids.into_iter().filter_map(|id| id.parse().ok()).collect(),
            total.max(0) as u32,
        ))
    }

    pub async fn apply_category_membership_changes(
        &self,
        changes: &[CategoryMembershipChange],
    ) -> Result<(String, u32, u32), String> {
        let mut tx = self.writer().begin().await.map_err(|e| e.to_string())?;
        let now = chrono::Utc::now().timestamp();
        let mut undo = Vec::new();
        let (mut added, mut removed) = (0, 0);
        for change in changes {
            let row = sqlx::query("SELECT assigned, revision, manual_decision FROM message_category_memberships WHERE message_id = ? AND category_id = ?")
                .bind(change.before.message_id.as_str()).bind(change.before.category_id.as_str())
                .fetch_optional(&mut *tx).await.map_err(|e| e.to_string())?;
            let prior_manual_decision = row
                .as_ref()
                .and_then(|r| r.get::<Option<i64>, _>("manual_decision"))
                .map(|v| v != 0);
            let (_, revision) = row.map_or((false, 0), |row| {
                (
                    row.get::<i64, _>("assigned") != 0,
                    row.get::<i64, _>("revision"),
                )
            });
            if revision != change.before.revision
                || prior_manual_decision != change.before.manual_decision
            {
                return Err("category membership changed after preview; preview again".into());
            }
            if prior_manual_decision == Some(change.assigned) {
                continue;
            }
            let after_revision = revision + 1;
            sqlx::query("INSERT INTO message_category_memberships (message_id, category_id, assigned, revision, updated_at, manual_decision) VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(message_id, category_id) DO UPDATE SET assigned = excluded.assigned, revision = excluded.revision, updated_at = excluded.updated_at, manual_decision = excluded.manual_decision")
                .bind(change.before.message_id.as_str()).bind(change.before.category_id.as_str()).bind(if change.assigned { 1i64 } else { 0 }).bind(after_revision).bind(now).bind(change.assigned as i64)
                .execute(&mut *tx).await.map_err(|e| e.to_string())?;
            undo.push(UndoEntry {
                message_id: change.before.message_id.clone(),
                category_id: change.before.category_id.clone(),
                prior_assigned: change.before.assigned,
                prior_manual_decision: Some(prior_manual_decision),
                after_revision,
            });
            if change.assigned != change.before.assigned {
                if change.assigned {
                    added += 1;
                } else {
                    removed += 1;
                }
            }
        }
        let mutation_id = Uuid::now_v7().to_string();
        if !undo.is_empty() {
            sqlx::query("DELETE FROM category_membership_undo WHERE expires_at < ?")
                .bind(now)
                .execute(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
            sqlx::query(
                "INSERT INTO category_membership_undo (id, snapshot, expires_at) VALUES (?, ?, ?)",
            )
            .bind(&mutation_id)
            .bind(serde_json::to_string(&undo).map_err(|e| e.to_string())?)
            .bind(now + UNDO_TTL_SECONDS)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        }
        tx.commit().await.map_err(|e| e.to_string())?;
        Ok((
            if undo.is_empty() {
                String::new()
            } else {
                mutation_id
            },
            added,
            removed,
        ))
    }

    pub async fn undo_category_membership(&self, mutation_id: &str) -> Result<(u32, u32), String> {
        let mut tx = self.writer().begin().await.map_err(|e| e.to_string())?;
        let row =
            sqlx::query("SELECT snapshot, expires_at FROM category_membership_undo WHERE id = ?")
                .bind(mutation_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "category membership undo not found or already used".to_string())?;
        if row.get::<i64, _>("expires_at") < chrono::Utc::now().timestamp() {
            return Err("category membership undo expired".into());
        }
        let entries: Vec<UndoEntry> =
            serde_json::from_str(&row.get::<String, _>("snapshot")).map_err(|e| e.to_string())?;
        let now = chrono::Utc::now().timestamp();
        let mut restored = 0;
        let mut skipped = 0;
        for entry in entries {
            let current = sqlx::query("SELECT assigned, revision, manual_decision FROM message_category_memberships WHERE message_id = ? AND category_id = ?")
                .bind(entry.message_id.as_str()).bind(entry.category_id.as_str()).fetch_optional(&mut *tx).await.map_err(|e| e.to_string())?;
            let Some(current) = current else {
                skipped += 1;
                continue;
            };
            if current.get::<i64, _>("revision") != entry.after_revision {
                skipped += 1;
                continue;
            }
            let revision = entry.after_revision + 1;
            sqlx::query("UPDATE message_category_memberships SET assigned = ?, revision = ?, updated_at = ?, manual_decision = ? WHERE message_id = ? AND category_id = ?")
                .bind(if entry.prior_assigned { 1i64 } else { 0 }).bind(revision).bind(now).bind(entry.prior_manual_decision.unwrap_or(Some(entry.prior_assigned)).map(i64::from)).bind(entry.message_id.as_str()).bind(entry.category_id.as_str())
                .execute(&mut *tx).await.map_err(|e| e.to_string())?;
            restored += 1;
        }
        sqlx::query("DELETE FROM category_membership_undo WHERE id = ?")
            .bind(mutation_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        tx.commit().await.map_err(|e| e.to_string())?;
        Ok((restored, skipped))
    }

    pub async fn delete_personal_category_if_unchanged(
        &self,
        category_id: &CategoryId,
        expected_name: &str,
        expected: &[CategoryMembershipVersion],
    ) -> Result<String, String> {
        let mut tx = self.writer().begin().await.map_err(|e| e.to_string())?;
        let row = sqlx::query("SELECT name FROM personal_categories WHERE id = ?")
            .bind(category_id.as_str())
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "category not found".to_string())?;
        let name: String = row.get("name");
        if name != expected_name {
            return Err("category name changed after preview; preview again".into());
        }
        let rows = sqlx::query("SELECT message_id, revision, assigned FROM message_category_memberships WHERE category_id = ? AND assigned = 1 ORDER BY message_id")
            .bind(category_id.as_str()).fetch_all(&mut *tx).await.map_err(|e| e.to_string())?;
        let current: Vec<_> = rows
            .into_iter()
            .filter_map(|row| {
                row.get::<String, _>("message_id")
                    .parse()
                    .ok()
                    .map(|message_id| CategoryMembershipVersion {
                        message_id,
                        category_id: category_id.clone(),
                        revision: row.get("revision"),
                        assigned: true,
                        manual_decision: Some(true),
                        sources: Vec::new(),
                    })
            })
            .collect();
        let mut expected = expected.to_vec();
        expected.sort_by_key(|a| a.message_id.to_string());
        if current != expected {
            return Err("category memberships changed after preview; preview again".into());
        }
        sqlx::query("DELETE FROM personal_categories WHERE id = ?")
            .bind(category_id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        tx.commit().await.map_err(|e| e.to_string())?;
        Ok(name)
    }

    pub async fn category_exists_in_account(
        &self,
        category_id: &CategoryId,
        account_id: &mxr_core::AccountId,
    ) -> Result<bool, String> {
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM effective_category_memberships cm JOIN messages m ON m.id = cm.message_id WHERE cm.category_id = ? AND cm.assigned = 1 AND m.account_id = ?)")
            .bind(category_id.as_str()).bind(account_id.as_str()).fetch_one(self.reader()).await.map_err(|e| e.to_string())
    }

    pub async fn category_visible_message_ids(
        &self,
        category_id: &CategoryId,
        account_id: &mxr_core::AccountId,
    ) -> Result<Vec<MessageId>, String> {
        let rows = sqlx::query("SELECT m.id FROM effective_category_memberships cm JOIN messages m ON m.id = cm.message_id WHERE cm.category_id = ? AND cm.assigned = 1 AND m.account_id = ?")
            .bind(category_id.as_str()).bind(account_id.as_str()).fetch_all(self.reader()).await.map_err(|e| e.to_string())?;
        Ok(rows
            .into_iter()
            .filter_map(|row| row.get::<String, _>("id").parse().ok())
            .collect())
    }

    pub async fn all_category_membership_versions(
        &self,
        category_id: &CategoryId,
    ) -> Result<Vec<CategoryMembershipVersion>, String> {
        let rows = sqlx::query("SELECT message_id, revision FROM message_category_memberships WHERE category_id = ? AND assigned = 1 ORDER BY message_id")
            .bind(category_id.as_str()).fetch_all(self.reader()).await.map_err(|e| e.to_string())?;
        rows.into_iter()
            .map(|row| {
                Ok(CategoryMembershipVersion {
                    message_id: row
                        .get::<String, _>("message_id")
                        .parse()
                        .map_err(|e: uuid::Error| e.to_string())?,
                    category_id: category_id.clone(),
                    revision: row.get("revision"),
                    assigned: true,
                    manual_decision: Some(true),
                    sources: Vec::new(),
                })
            })
            .collect()
    }

    pub async fn category_undo_accounts(
        &self,
        mutation_id: &str,
    ) -> Result<Vec<mxr_core::AccountId>, String> {
        let snapshot: String =
            sqlx::query_scalar("SELECT snapshot FROM category_membership_undo WHERE id = ?")
                .bind(mutation_id)
                .fetch_optional(self.reader())
                .await
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "category membership undo not found or already used".to_string())?;
        let entries: Vec<UndoEntry> = serde_json::from_str(&snapshot).map_err(|e| e.to_string())?;
        let mut accounts = Vec::new();
        for entry in entries {
            let account: Option<String> =
                sqlx::query_scalar("SELECT account_id FROM messages WHERE id = ?")
                    .bind(entry.message_id.as_str())
                    .fetch_optional(self.reader())
                    .await
                    .map_err(|e| e.to_string())?;
            if let Some(account) = account {
                let account = account.parse().map_err(|e: uuid::Error| e.to_string())?;
                if !accounts.contains(&account) {
                    accounts.push(account);
                }
            }
        }
        Ok(accounts)
    }

    pub async fn category_membership_preview_state(
        &self,
        account_id: Option<&mxr_core::AccountId>,
        message_ids: &[MessageId],
        category_ids: &[CategoryId],
    ) -> Result<Vec<CategoryMembershipVersion>, String> {
        self.category_membership_versions(account_id, message_ids, category_ids)
            .await
    }

    pub async fn category_membership_states(
        &self,
        account_id: Option<&mxr_core::AccountId>,
        message_ids: &[MessageId],
        category_ids: &[CategoryId],
    ) -> Result<Vec<mxr_core::CategoryMembershipState>, String> {
        let versions = self
            .category_membership_versions(account_id, message_ids, category_ids)
            .await?;
        category_ids
            .iter()
            .map(|category_id| {
                Ok(mxr_core::CategoryMembershipState {
                    category_id: category_id.clone(),
                    selected_messages: message_ids.len() as u32,
                    assigned_messages: versions
                        .iter()
                        .filter(|version| &version.category_id == category_id && version.assigned)
                        .count() as u32,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::{test_account, TestEnvelopeBuilder};
    use mxr_core::MessageFlags;

    #[test]
    fn category_names_use_nfkc_and_full_casefold() {
        assert_eq!(normalize_category_name("  Straẞe  ").unwrap().1, "strasse");
        assert_eq!(
            normalize_category_name("e\u{301}").unwrap(),
            normalize_category_name("é").unwrap()
        );
        assert!(normalize_category_name("\u{0000}").is_err());
    }

    #[tokio::test]
    async fn category_membership_undo_is_revision_bound_across_aba() {
        let store = super::super::pool::Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let envelope = TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .flags(MessageFlags::empty())
            .build();
        store.upsert_envelope(&envelope).await.unwrap();
        let category = store.create_personal_category("Travel").await.unwrap();
        let original = store
            .category_membership_versions(
                Some(&account.id),
                std::slice::from_ref(&envelope.id),
                std::slice::from_ref(&category.id),
            )
            .await
            .unwrap()
            .remove(0);
        assert_eq!((original.revision, original.assigned), (0, false));

        let (first_undo, added, removed) = store
            .apply_category_membership_changes(&[CategoryMembershipChange {
                before: original,
                assigned: true,
            }])
            .await
            .unwrap();
        assert_eq!((added, removed), (1, 0));
        let after_add = store
            .category_membership_versions(
                Some(&account.id),
                std::slice::from_ref(&envelope.id),
                std::slice::from_ref(&category.id),
            )
            .await
            .unwrap()
            .remove(0);
        let (_, added, removed) = store
            .apply_category_membership_changes(&[CategoryMembershipChange {
                before: after_add,
                assigned: false,
            }])
            .await
            .unwrap();
        assert_eq!((added, removed), (0, 1));
        let after_remove = store
            .category_membership_versions(
                Some(&account.id),
                std::slice::from_ref(&envelope.id),
                std::slice::from_ref(&category.id),
            )
            .await
            .unwrap()
            .remove(0);
        store
            .apply_category_membership_changes(&[CategoryMembershipChange {
                before: after_remove,
                assigned: true,
            }])
            .await
            .unwrap();

        let (restored, skipped) = store.undo_category_membership(&first_undo).await.unwrap();
        assert_eq!((restored, skipped), (0, 1));
        let current = store
            .category_membership_versions(Some(&account.id), &[envelope.id], &[category.id])
            .await
            .unwrap()
            .remove(0);
        assert_eq!((current.revision, current.assigned), (3, true));
    }

    #[tokio::test]
    async fn scoped_category_lists_hide_names_without_visible_memberships() {
        let store = super::super::pool::Store::in_memory().await.unwrap();
        let visible_account = test_account();
        let hidden_account = test_account();
        store.insert_account(&visible_account).await.unwrap();
        store.insert_account(&hidden_account).await.unwrap();
        let visible_message = TestEnvelopeBuilder::new()
            .account_id(visible_account.id.clone())
            .build();
        let hidden_message = TestEnvelopeBuilder::new()
            .account_id(hidden_account.id.clone())
            .build();
        store.upsert_envelope(&visible_message).await.unwrap();
        store.upsert_envelope(&hidden_message).await.unwrap();
        let visible = store.create_personal_category("Visible").await.unwrap();
        let hidden = store.create_personal_category("Hidden name").await.unwrap();
        for (message, category) in [(&visible_message, &visible), (&hidden_message, &hidden)] {
            let before = store
                .category_membership_versions(
                    None,
                    std::slice::from_ref(&message.id),
                    std::slice::from_ref(&category.id),
                )
                .await
                .unwrap()
                .remove(0);
            store
                .apply_category_membership_changes(&[CategoryMembershipChange {
                    before,
                    assigned: true,
                }])
                .await
                .unwrap();
        }
        let scoped = store
            .list_personal_categories(Some(&visible_account.id))
            .await
            .unwrap();
        assert_eq!(scoped.len(), 2);
        let visible_scoped = scoped
            .iter()
            .find(|category| category.id == visible.id)
            .unwrap();
        let hidden_scoped = scoped
            .iter()
            .find(|category| category.id == hidden.id)
            .unwrap();
        assert_eq!(visible_scoped.total_count, 1);
        assert_eq!(hidden_scoped.total_count, 0);
        assert!(!store
            .category_exists_in_account(&hidden.id, &visible_account.id)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn delete_preview_stales_after_category_rename() {
        let store = super::super::pool::Store::in_memory().await.unwrap();
        let category = store.create_personal_category("Travel").await.unwrap();
        let expected = store
            .all_category_membership_versions(&category.id)
            .await
            .unwrap();
        store
            .rename_personal_category(&category.id, "Trips")
            .await
            .unwrap();
        let error = store
            .delete_personal_category_if_unchanged(&category.id, "Travel", &expected)
            .await
            .unwrap_err();
        assert!(error.contains("name changed after preview"));
        assert_eq!(
            store.get_personal_category(&category.id).await.unwrap().0,
            "Trips"
        );
    }
}
