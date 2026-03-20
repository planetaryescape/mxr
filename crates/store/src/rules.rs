use chrono::{DateTime, Utc};
use sqlx::Row;

pub struct RuleRecordInput<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub enabled: bool,
    pub priority: i32,
    pub conditions_json: &'a str,
    pub actions_json: &'a str,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct RuleLogInput<'a> {
    pub rule_id: &'a str,
    pub rule_name: &'a str,
    pub message_id: &'a str,
    pub actions_applied_json: &'a str,
    pub timestamp: DateTime<Utc>,
    pub success: bool,
    pub error: Option<&'a str>,
}

impl super::Store {
    pub async fn upsert_rule(&self, rule: RuleRecordInput<'_>) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO rules (id, name, enabled, priority, conditions, actions, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                enabled = excluded.enabled,
                priority = excluded.priority,
                conditions = excluded.conditions,
                actions = excluded.actions,
                updated_at = excluded.updated_at",
        )
        .bind(rule.id)
        .bind(rule.name)
        .bind(rule.enabled as i64)
        .bind(rule.priority as i64)
        .bind(rule.conditions_json)
        .bind(rule.actions_json)
        .bind(rule.created_at.to_rfc3339())
        .bind(rule.updated_at.to_rfc3339())
        .execute(self.writer())
        .await?;
        Ok(())
    }

    pub async fn list_rules(&self) -> Result<Vec<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        sqlx::query("SELECT * FROM rules ORDER BY priority ASC, created_at ASC")
            .fetch_all(self.reader())
            .await
    }

    pub async fn get_rule_by_id_or_name(
        &self,
        key: &str,
    ) -> Result<Option<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        sqlx::query("SELECT * FROM rules WHERE id = ? OR name = ? ORDER BY priority ASC LIMIT 1")
            .bind(key)
            .bind(key)
            .fetch_optional(self.reader())
            .await
    }

    pub async fn delete_rule(&self, id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM rules WHERE id = ?")
            .bind(id)
            .execute(self.writer())
            .await?;
        Ok(())
    }

    pub async fn insert_rule_log(&self, log: RuleLogInput<'_>) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO rule_execution_log
             (rule_id, rule_name, message_id, actions_applied, timestamp, success, error)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(log.rule_id)
        .bind(log.rule_name)
        .bind(log.message_id)
        .bind(log.actions_applied_json)
        .bind(log.timestamp.to_rfc3339())
        .bind(log.success as i64)
        .bind(log.error)
        .execute(self.writer())
        .await?;
        Ok(())
    }

    pub async fn list_rule_logs(
        &self,
        rule_id: Option<&str>,
        limit: u32,
    ) -> Result<Vec<sqlx::sqlite::SqliteRow>, sqlx::Error> {
        let mut sql = String::from("SELECT * FROM rule_execution_log");
        if rule_id.is_some() {
            sql.push_str(" WHERE rule_id = ?");
        }
        sql.push_str(" ORDER BY timestamp DESC LIMIT ?");
        let mut query = sqlx::query(&sql);
        if let Some(rule_id) = rule_id {
            query = query.bind(rule_id);
        }
        query.bind(limit).fetch_all(self.reader()).await
    }
}

pub fn row_to_rule_json(row: &sqlx::sqlite::SqliteRow) -> serde_json::Value {
    serde_json::json!({
        "id": row.get::<String, _>("id"),
        "name": row.get::<String, _>("name"),
        "enabled": row.get::<i64, _>("enabled") != 0,
        "priority": row.get::<i64, _>("priority") as i32,
        "conditions": serde_json::from_str::<serde_json::Value>(&row.get::<String, _>("conditions")).unwrap_or(serde_json::Value::Null),
        "actions": serde_json::from_str::<serde_json::Value>(&row.get::<String, _>("actions")).unwrap_or(serde_json::Value::Null),
        "created_at": row.get::<String, _>("created_at"),
        "updated_at": row.get::<String, _>("updated_at"),
    })
}

pub fn row_to_rule_log_json(row: &sqlx::sqlite::SqliteRow) -> serde_json::Value {
    serde_json::json!({
        "rule_id": row.get::<String, _>("rule_id"),
        "rule_name": row.get::<String, _>("rule_name"),
        "message_id": row.get::<String, _>("message_id"),
        "actions_applied": serde_json::from_str::<serde_json::Value>(&row.get::<String, _>("actions_applied")).unwrap_or(serde_json::Value::Array(Vec::new())),
        "timestamp": row.get::<String, _>("timestamp"),
        "success": row.get::<i64, _>("success") != 0,
        "error": row.get::<Option<String>, _>("error"),
    })
}
