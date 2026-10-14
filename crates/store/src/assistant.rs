//! The in-app assistant's conversations (`assistant_conversations`,
//! `assistant_messages`). The store keeps the transcript as opaque JSON:
//! the daemon owns its shape, and messages are only ever appended.

use chrono::Utc;
use sqlx::Row;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssistantConversationRow {
    pub id: String,
    pub title: String,
    /// `idle`, `running` or `awaiting_user`.
    pub status: String,
    pub last_outcome: Option<String>,
    pub last_error: Option<String>,
    pub pending_json: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub message_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssistantMessageRow {
    pub seq: i64,
    pub message_json: String,
    pub meta_json: Option<String>,
    pub created_at: i64,
}

/// The columns `conversation_from_row` reads, from `assistant_conversations c`.
macro_rules! conversation_columns {
    () => {
        "c.id, c.title, c.status, c.last_outcome, c.last_error, c.pending_json, c.created_at, \
         c.updated_at, (SELECT COUNT(*) FROM assistant_messages m WHERE m.conversation_id = c.id) \
         AS message_count"
    };
}

fn conversation_from_row(row: &sqlx::sqlite::SqliteRow) -> AssistantConversationRow {
    AssistantConversationRow {
        id: row.get("id"),
        title: row.get("title"),
        status: row.get("status"),
        last_outcome: row.get("last_outcome"),
        last_error: row.get("last_error"),
        pending_json: row.get("pending_json"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
        message_count: row.get("message_count"),
    }
}

impl super::Store {
    pub async fn create_assistant_conversation(
        &self,
        id: &str,
        title: &str,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp();
        sqlx::query(
            "INSERT INTO assistant_conversations (id, title, status, created_at, updated_at)
             VALUES (?1, ?2, 'idle', ?3, ?3)",
        )
        .bind(id)
        .bind(title)
        .bind(now)
        .execute(self.writer())
        .await?;
        Ok(())
    }

    /// Most recently active first.
    pub async fn list_assistant_conversations(
        &self,
        limit: u32,
    ) -> Result<Vec<AssistantConversationRow>, sqlx::Error> {
        let rows = sqlx::query(concat!(
            "SELECT ",
            conversation_columns!(),
            " FROM assistant_conversations c ORDER BY c.updated_at DESC, c.id DESC LIMIT ?1"
        ))
        .bind(i64::from(limit))
        .fetch_all(self.reader())
        .await?;
        Ok(rows.iter().map(conversation_from_row).collect())
    }

    pub async fn get_assistant_conversation(
        &self,
        id: &str,
    ) -> Result<Option<AssistantConversationRow>, sqlx::Error> {
        let row = sqlx::query(concat!(
            "SELECT ",
            conversation_columns!(),
            " FROM assistant_conversations c WHERE c.id = ?1"
        ))
        .bind(id)
        .fetch_optional(self.reader())
        .await?;
        Ok(row.as_ref().map(conversation_from_row))
    }

    /// Append one message and return its sequence number, starting at 1.
    pub async fn append_assistant_message(
        &self,
        conversation_id: &str,
        message_json: &str,
        meta_json: Option<&str>,
    ) -> Result<i64, sqlx::Error> {
        let now = Utc::now().timestamp();
        let mut tx = self.writer().begin().await?;
        let seq: i64 = sqlx::query(
            "SELECT COALESCE(MAX(seq), 0) + 1 AS next FROM assistant_messages
             WHERE conversation_id = ?1",
        )
        .bind(conversation_id)
        .fetch_one(&mut *tx)
        .await?
        .get("next");
        sqlx::query(
            "INSERT INTO assistant_messages (conversation_id, seq, message_json, meta_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(conversation_id)
        .bind(seq)
        .bind(message_json)
        .bind(meta_json)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE assistant_conversations SET updated_at = ?2 WHERE id = ?1")
            .bind(conversation_id)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(seq)
    }

    /// The whole transcript in order.
    pub async fn assistant_messages(
        &self,
        conversation_id: &str,
    ) -> Result<Vec<AssistantMessageRow>, sqlx::Error> {
        let rows = sqlx::query(
            "SELECT seq, message_json, meta_json, created_at FROM assistant_messages
             WHERE conversation_id = ?1 ORDER BY seq",
        )
        .bind(conversation_id)
        .fetch_all(self.reader())
        .await?;
        Ok(rows
            .iter()
            .map(|row| AssistantMessageRow {
                seq: row.get("seq"),
                message_json: row.get("message_json"),
                meta_json: row.get("meta_json"),
                created_at: row.get("created_at"),
            })
            .collect())
    }

    /// Record where a conversation stands. `pending_json` is replaced
    /// whole: `None` clears it.
    pub async fn set_assistant_state(
        &self,
        id: &str,
        status: &str,
        last_outcome: Option<&str>,
        last_error: Option<&str>,
        pending_json: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE assistant_conversations
             SET status = ?2, last_outcome = ?3, last_error = ?4, pending_json = ?5, updated_at = ?6
             WHERE id = ?1",
        )
        .bind(id)
        .bind(status)
        .bind(last_outcome)
        .bind(last_error)
        .bind(pending_json)
        .bind(Utc::now().timestamp())
        .execute(self.writer())
        .await?;
        Ok(())
    }

    /// Delete a conversation and its transcript. False when there was none.
    pub async fn delete_assistant_conversation(&self, id: &str) -> Result<bool, sqlx::Error> {
        let mut tx = self.writer().begin().await?;
        sqlx::query("DELETE FROM assistant_messages WHERE conversation_id = ?1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        let deleted = sqlx::query("DELETE FROM assistant_conversations WHERE id = ?1")
            .bind(id)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        tx.commit().await?;
        Ok(deleted > 0)
    }

    /// At daemon start nothing is running, so a conversation left `running`
    /// was cut off. Calls still waiting for the user stay waiting.
    pub async fn mark_interrupted_assistant_turns(&self) -> Result<u64, sqlx::Error> {
        Ok(sqlx::query(
            "UPDATE assistant_conversations
             SET status = 'idle', last_outcome = 'interrupted', last_error = NULL
             WHERE status = 'running'",
        )
        .execute(self.writer())
        .await?
        .rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use crate::Store;

    #[tokio::test]
    async fn messages_append_in_order_and_go_with_their_conversation() {
        let store = Store::in_memory().await.unwrap();
        store
            .create_assistant_conversation("c1", "Bank mail")
            .await
            .unwrap();
        assert_eq!(
            store
                .append_assistant_message("c1", "{\"role\":\"user\"}", None)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            store
                .append_assistant_message("c1", "{\"role\":\"assistant\"}", Some("{}"))
                .await
                .unwrap(),
            2
        );
        let messages = store.assistant_messages("c1").await.unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1].meta_json.as_deref(), Some("{}"));
        let conversation = store.get_assistant_conversation("c1").await.unwrap().unwrap();
        assert_eq!(conversation.message_count, 2);
        assert_eq!(conversation.status, "idle");

        assert!(store.delete_assistant_conversation("c1").await.unwrap());
        assert!(store.assistant_messages("c1").await.unwrap().is_empty());
        assert!(!store.delete_assistant_conversation("c1").await.unwrap());
    }

    #[tokio::test]
    async fn a_restart_marks_running_turns_interrupted_and_keeps_waiting_ones() {
        let store = Store::in_memory().await.unwrap();
        for id in ["running", "waiting"] {
            store.create_assistant_conversation(id, id).await.unwrap();
        }
        store
            .set_assistant_state("running", "running", None, None, None)
            .await
            .unwrap();
        store
            .set_assistant_state("waiting", "awaiting_user", None, None, Some("[{}]"))
            .await
            .unwrap();
        assert_eq!(store.mark_interrupted_assistant_turns().await.unwrap(), 1);
        let running = store.get_assistant_conversation("running").await.unwrap().unwrap();
        assert_eq!(running.status, "idle");
        assert_eq!(running.last_outcome.as_deref(), Some("interrupted"));
        let waiting = store.get_assistant_conversation("waiting").await.unwrap().unwrap();
        assert_eq!(waiting.status, "awaiting_user");
        assert_eq!(waiting.pending_json.as_deref(), Some("[{}]"));
        let listed = store.list_assistant_conversations(10).await.unwrap();
        assert_eq!(listed.len(), 2);
    }
}
