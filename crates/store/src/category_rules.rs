use mxr_core::{CategoryId, MessageId};
use serde::{Deserialize, Serialize};
use sqlx::Row;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleCategorySource {
    pub message_id: MessageId,
    pub category_id: CategoryId,
    pub rule_id: String,
    pub assigned: bool,
    pub priority: i32,
    pub rule_name: String,
    pub rule_revision: String,
}

impl super::Store {
    pub async fn category_rule_snapshot(&self) -> Result<String, sqlx::Error> {
        let mut tx = self.reader().begin().await?;
        let value = snapshot(&mut tx, None).await?;
        tx.commit().await?;
        Ok(value)
    }
    pub async fn category_rule_message_snapshot(
        &self,
        message: &MessageId,
    ) -> Result<String, sqlx::Error> {
        let mut tx = self.reader().begin().await?;
        let result = snapshot(&mut tx, Some(message)).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub async fn category_rule_sources(&self) -> Result<Vec<RuleCategorySource>, sqlx::Error> {
        let mut connection = self.reader().acquire().await?;
        read_sources(&mut connection, None).await
    }

    /// Saving and replacing this rule's complete contribution is one transaction.
    pub async fn apply_category_rule(
        &self,
        rule: super::rules::RuleRecordInput<'_>,
        sources: &[RuleCategorySource],
        delete: bool,
        expected: &str,
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.writer().begin().await?;
        if snapshot(&mut tx, None).await? != expected {
            return Err(sqlx::Error::Protocol(
                "Category rule inputs changed; preview again".into(),
            ));
        }
        let rule_id = rule.id.to_string();
        if delete {
            sqlx::query("DELETE FROM rules WHERE id = ?")
                .bind(&rule_id)
                .execute(&mut *tx)
                .await?;
        } else {
            super::rules::write_rule(&mut tx, rule).await?;
            sqlx::query("DELETE FROM rule_category_sources WHERE rule_id = ?")
                .bind(&rule_id)
                .execute(&mut *tx)
                .await?;
            for source in sources {
                write_source(&mut tx, source).await?;
            }
        }
        tx.commit().await
    }

    /// Sync and replay replace only the sources for this message, preserving manual decisions.
    pub async fn reconcile_message_category_sources(
        &self,
        id: &MessageId,
        sources: &[RuleCategorySource],
        expected: &str,
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.writer().begin().await?;
        if snapshot(&mut tx, Some(id)).await? != expected {
            return Err(sqlx::Error::Protocol(
                "Category sync eligibility changed; retry reconciliation".into(),
            ));
        }
        let current = read_sources(&mut tx, Some(id)).await?;
        if current
            .iter()
            .filter(|s| &s.message_id == id)
            .cloned()
            .collect::<Vec<_>>()
            == sources
        {
            return Ok(());
        }
        sqlx::query("DELETE FROM rule_category_sources WHERE message_id = ?")
            .bind(id.as_str())
            .execute(&mut *tx)
            .await?;
        for source in sources {
            write_source(&mut tx, source).await?;
        }
        tx.commit().await
    }
}

async fn write_source(
    tx: &mut sqlx::SqliteConnection,
    s: &RuleCategorySource,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO rule_category_sources(message_id,category_id,rule_id,assigned,priority,rule_name,rule_revision) VALUES(?,?,?,?,?,?,?) ON CONFLICT(message_id,category_id,rule_id) DO UPDATE SET assigned=excluded.assigned,priority=excluded.priority,rule_name=excluded.rule_name,rule_revision=excluded.rule_revision")
        .bind(s.message_id.as_str()).bind(s.category_id.as_str()).bind(&s.rule_id).bind(s.assigned as i64).bind(s.priority).bind(&s.rule_name).bind(&s.rule_revision).execute(tx).await?;
    Ok(())
}

// A transaction snapshot closes the race between daemon revalidation and sync persistence.
// Table/column names come only from this fixed internal allowlist and SQLite schema.
async fn snapshot(
    connection: &mut sqlx::SqliteConnection,
    message: Option<&MessageId>,
) -> Result<String, sqlx::Error> {
    let mut result = Vec::new();
    for table in [
        "accounts",
        "messages",
        "bodies",
        "labels",
        "message_labels",
        "rules",
        "personal_categories",
        "message_category_memberships",
        "rule_category_sources",
    ] {
        let columns = sqlx::query(sqlx::AssertSqlSafe(
            format!("PRAGMA table_info({table})").as_str(),
        ))
        .fetch_all(&mut *connection)
        .await?;
        let columns: Vec<String> = columns.iter().map(|r| r.get("name")).collect();
        let fields = columns
            .iter()
            .map(|c| format!("'{c}',\"{c}\""))
            .collect::<Vec<_>>()
            .join(",");
        let order = columns
            .iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(",");
        let column = match table {
            "messages" => Some("id"),
            "bodies"
            | "message_labels"
            | "message_category_memberships"
            | "rule_category_sources" => Some("message_id"),
            _ => None,
        };
        let filter = if message.is_some() {
            column
                .map(|c| format!(" WHERE {c} = ?"))
                .unwrap_or_default()
        } else {
            String::new()
        };
        let query = format!("SELECT COALESCE(json_group_array(json(row)), '[]') FROM (SELECT json_object({fields}) AS row FROM {table}{filter} ORDER BY {order})");
        let mut query = sqlx::query_scalar(sqlx::AssertSqlSafe(query.as_str()));
        if let (Some(message), Some(_)) = (message, column) {
            query = query.bind(message.as_str());
        }
        let value: String = query.fetch_one(&mut *connection).await?;
        result.push(value);
    }
    Ok(result.join("\n"))
}

pub(crate) async fn read_sources(
    connection: &mut sqlx::SqliteConnection,
    message: Option<&MessageId>,
) -> Result<Vec<RuleCategorySource>, sqlx::Error> {
    let sql = if message.is_some() {
        "SELECT * FROM rule_category_sources WHERE message_id = ? ORDER BY message_id,category_id,priority,rule_id"
    } else {
        "SELECT * FROM rule_category_sources ORDER BY message_id,category_id,priority,rule_id"
    };
    let mut query = sqlx::query(sql);
    if let Some(message) = message {
        query = query.bind(message.as_str());
    }
    let rows = query.fetch_all(connection).await?;
    rows.into_iter()
        .map(|row| {
            Ok(RuleCategorySource {
                message_id: row
                    .get::<String, _>("message_id")
                    .parse()
                    .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
                category_id: row
                    .get::<String, _>("category_id")
                    .parse()
                    .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
                rule_id: row.get("rule_id"),
                assigned: row.get::<i64, _>("assigned") != 0,
                priority: row.get::<i64, _>("priority") as i32,
                rule_name: row.get("rule_name"),
                rule_revision: row.get("rule_revision"),
            })
        })
        .collect()
}
