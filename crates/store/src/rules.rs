use chrono::{DateTime, Utc};
use sqlx::Row;

pub struct RuleRecordInput<'a> {
    pub id: &'a str,
    pub account_id: Option<&'a mxr_core::AccountId>,
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
        let mut connection = self.writer().acquire().await?;
        write_rule(&mut connection, rule).await
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
        let mut query = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()));
        if let Some(rule_id) = rule_id {
            query = query.bind(rule_id);
        }
        query.bind(limit).fetch_all(self.reader()).await
    }
}

pub fn row_to_rule_json(row: &sqlx::sqlite::SqliteRow) -> serde_json::Value {
    serde_json::json!({
        "id": row.get::<String, _>("id"),
        "account_id": row.get::<Option<String>, _>("account_id"),
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

impl super::Store {
    pub async fn set_rule_treatment(
        &self,
        message_id: &mxr_core::MessageId,
        rule_id: &str,
        updated_at: &str,
        treatment: &str,
        rule_name: &str,
    ) -> Result<(), sqlx::Error> {
        let mut connection = self.writer().acquire().await?;
        write_treatment(
            &mut connection,
            message_id,
            rule_id,
            updated_at,
            treatment,
            rule_name,
        )
        .await
    }

    pub async fn rule_treatments(
        &self,
        account_id: &mxr_core::AccountId,
    ) -> Result<std::collections::HashMap<mxr_core::MessageId, (String, String)>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT t.message_id, t.treatment,
            CASE WHEN r.id IS NULL THEN t.rule_name || ' (deleted rule)'
                 WHEN r.enabled = 0 THEN t.rule_name || ' (disabled rule)'
                 WHEN r.updated_at != t.rule_updated_at THEN t.rule_name || ' (earlier version)'
                 ELSE t.rule_name END AS rule_name FROM rule_treatments t
            LEFT JOIN rules r ON r.id = t.rule_id
            JOIN messages m ON m.id = t.message_id
            WHERE m.account_id = ?",
        )
        .bind(account_id.as_str())
        .fetch_all(self.reader())
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok((
                    crate::decode_id(row.try_get("message_id")?)?,
                    (row.try_get("treatment")?, row.try_get("rule_name")?),
                ))
            })
            .collect()
    }
}

impl super::Store {
    pub async fn needs_rule_treatment(
        &self,
        message_id: &mxr_core::MessageId,
    ) -> Result<bool, sqlx::Error> {
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM arrivals a WHERE a.message_id = ? AND a.mode IS NULL AND NOT EXISTS(SELECT 1 FROM rule_treatments t WHERE t.message_id = a.message_id))")
            .bind(message_id.as_str()).fetch_one(self.reader()).await
    }
}

async fn write_rule(
    connection: &mut sqlx::SqliteConnection,
    rule: RuleRecordInput<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
            "INSERT INTO rules (id, name, enabled, priority, conditions, actions, created_at, updated_at, account_id)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                enabled = excluded.enabled,
                priority = excluded.priority,
                conditions = excluded.conditions,
                actions = excluded.actions,
                updated_at = excluded.updated_at,
                account_id = excluded.account_id",
        )
        .bind(rule.id)
        .bind(rule.name)
        .bind(rule.enabled as i64)
        .bind(rule.priority as i64)
        .bind(rule.conditions_json)
        .bind(rule.actions_json)
        .bind(rule.created_at.to_rfc3339())
        .bind(rule.updated_at.to_rfc3339())
        .bind(rule.account_id.map(mxr_core::AccountId::as_str))
        .execute(connection)
        .await?;
    Ok(())
}

async fn write_treatment(
    connection: &mut sqlx::SqliteConnection,
    message_id: &mxr_core::MessageId,
    rule_id: &str,
    updated_at: &str,
    treatment: &str,
    rule_name: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO rule_treatments (message_id, rule_id, rule_updated_at, treatment, rule_name)
            VALUES (?, ?, ?, ?, ?) ON CONFLICT(message_id) DO UPDATE SET
            rule_id = excluded.rule_id, rule_updated_at = excluded.rule_updated_at,
            treatment = excluded.treatment, rule_name = excluded.rule_name",
    )
    .bind(message_id.as_str())
    .bind(rule_id)
    .bind(updated_at)
    .bind(treatment)
    .bind(rule_name)
    .execute(connection)
    .await?;
    Ok(())
}

pub struct RuleTreatmentInput {
    pub message_id: mxr_core::MessageId,
    pub rule_id: String,
    pub rule_updated_at: String,
    pub treatment: String,
    pub rule_name: String,
}
impl super::Store {
    pub async fn apply_rule_treatments(
        &self,
        rule: RuleRecordInput<'_>,
        treatments: &[RuleTreatmentInput],
        placements: &[crate::ArrivalPlacement],
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.writer().begin().await?;
        write_rule(&mut tx, rule).await?;
        for t in treatments {
            write_treatment(
                &mut tx,
                &t.message_id,
                &t.rule_id,
                &t.rule_updated_at,
                &t.treatment,
                &t.rule_name,
            )
            .await?;
        }
        let placed_at = chrono::Utc::now().timestamp();
        for p in placements {
            sqlx::query("UPDATE arrivals SET now_mode = CASE WHEN mode IS NULL OR ?2 = mode THEN NULL ELSE ?2 END,
                rule = CASE WHEN mode IS NULL THEN ?3 ELSE rule END,
                reason = CASE WHEN mode IS NULL THEN ?4 ELSE reason END,
                not_sure = CASE WHEN mode IS NULL THEN ?5 ELSE not_sure END,
                placed_at = CASE WHEN mode IS NULL THEN ?6 ELSE placed_at END,
                mode = COALESCE(mode, ?2) WHERE message_id = ?1")
                .bind(p.message_id.as_str()).bind(&p.mode).bind(&p.rule).bind(&p.reason)
                .bind(&p.not_sure).bind(placed_at).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }
}
