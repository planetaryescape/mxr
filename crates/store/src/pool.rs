use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;
use std::path::Path;
use std::str::FromStr;

pub struct Store {
    writer: SqlitePool,
    reader: SqlitePool,
}

impl Store {
    pub async fn new(db_path: &Path) -> Result<Self, sqlx::Error> {
        let db_url = format!("sqlite:{}", db_path.display());

        let write_opts = SqliteConnectOptions::from_str(&db_url)?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .pragma("foreign_keys", "ON");

        let writer = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(write_opts)
            .await?;

        let read_opts = SqliteConnectOptions::from_str(&db_url)?
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .pragma("foreign_keys", "ON")
            .read_only(true);

        let reader = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(read_opts)
            .await?;

        let store = Self { writer, reader };
        store.run_migrations().await?;
        Ok(store)
    }

    pub async fn in_memory() -> Result<Self, sqlx::Error> {
        let opts = SqliteConnectOptions::from_str("sqlite::memory:")?
            .journal_mode(SqliteJournalMode::Wal)
            .pragma("foreign_keys", "ON");

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await?;

        let store = Self {
            writer: pool.clone(),
            reader: pool,
        };
        store.run_migrations().await?;
        Ok(store)
    }

    async fn run_migrations(&self) -> Result<(), sqlx::Error> {
        sqlx::raw_sql(include_str!("../migrations/001_initial.sql"))
            .execute(&self.writer)
            .await?;
        self.add_column_if_missing(
            "bodies",
            "metadata_json",
            "ALTER TABLE bodies ADD COLUMN metadata_json TEXT NOT NULL DEFAULT '{}'",
        )
        .await?;
        sqlx::raw_sql(include_str!("../migrations/003_sync_runtime_status.sql"))
            .execute(&self.writer)
            .await?;
        self.add_column_if_missing(
            "saved_searches",
            "search_mode",
            "ALTER TABLE saved_searches ADD COLUMN search_mode TEXT NOT NULL DEFAULT '\"lexical\"'",
        )
        .await?;
        sqlx::raw_sql(
            r#"
CREATE TABLE IF NOT EXISTS semantic_profiles (
    id                 TEXT PRIMARY KEY,
    profile_name       TEXT NOT NULL UNIQUE,
    backend            TEXT NOT NULL,
    model_revision     TEXT NOT NULL,
    dimensions         INTEGER NOT NULL,
    status             TEXT NOT NULL,
    installed_at       INTEGER,
    activated_at       INTEGER,
    last_indexed_at    INTEGER,
    progress_completed INTEGER NOT NULL DEFAULT 0,
    progress_total     INTEGER NOT NULL DEFAULT 0,
    last_error         TEXT
);

CREATE TABLE IF NOT EXISTS semantic_chunks (
    id            TEXT PRIMARY KEY,
    message_id    TEXT NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    source_kind   TEXT NOT NULL,
    ordinal       INTEGER NOT NULL,
    normalized    TEXT NOT NULL,
    content_hash  TEXT NOT NULL,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL,
    UNIQUE(message_id, source_kind, ordinal)
);

CREATE INDEX IF NOT EXISTS idx_semantic_chunks_message_id
    ON semantic_chunks(message_id);

CREATE TABLE IF NOT EXISTS semantic_embeddings (
    chunk_id      TEXT NOT NULL REFERENCES semantic_chunks(id) ON DELETE CASCADE,
    profile_id    TEXT NOT NULL REFERENCES semantic_profiles(id) ON DELETE CASCADE,
    dimensions    INTEGER NOT NULL,
    vector_blob   BLOB NOT NULL,
    status        TEXT NOT NULL,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL,
    PRIMARY KEY (chunk_id, profile_id)
);

CREATE INDEX IF NOT EXISTS idx_semantic_embeddings_profile_id
    ON semantic_embeddings(profile_id);
"#,
        )
        .execute(&self.writer)
        .await?;
        self.add_column_if_missing(
            "attachments",
            "disposition",
            "ALTER TABLE attachments ADD COLUMN disposition TEXT NOT NULL DEFAULT 'unspecified'",
        )
        .await?;
        self.add_column_if_missing(
            "attachments",
            "content_id",
            "ALTER TABLE attachments ADD COLUMN content_id TEXT",
        )
        .await?;
        self.add_column_if_missing(
            "attachments",
            "content_location",
            "ALTER TABLE attachments ADD COLUMN content_location TEXT",
        )
        .await?;
        sqlx::raw_sql(
            "CREATE INDEX IF NOT EXISTS idx_attachments_content_id ON attachments(content_id)",
        )
        .execute(&self.writer)
        .await?;
        sqlx::raw_sql(include_str!("../migrations/006_message_events.sql"))
            .execute(&self.writer)
            .await?;
        self.add_column_if_missing(
            "messages",
            "direction",
            "ALTER TABLE messages ADD COLUMN direction TEXT NOT NULL DEFAULT 'unknown' \
             CHECK (direction IN ('inbound', 'outbound', 'unknown'))",
        )
        .await?;
        self.add_column_if_missing(
            "messages",
            "list_id",
            "ALTER TABLE messages ADD COLUMN list_id TEXT",
        )
        .await?;
        self.add_column_if_missing(
            "messages",
            "body_word_count",
            "ALTER TABLE messages ADD COLUMN body_word_count INTEGER",
        )
        .await?;
        self.add_column_if_missing(
            "messages",
            "body_quoted_lines",
            "ALTER TABLE messages ADD COLUMN body_quoted_lines INTEGER",
        )
        .await?;
        sqlx::raw_sql(
            "CREATE INDEX IF NOT EXISTS idx_messages_account_direction_date \
             ON messages(account_id, direction, date DESC); \
             CREATE INDEX IF NOT EXISTS idx_messages_list_id \
             ON messages(list_id) WHERE list_id IS NOT NULL; \
             CREATE INDEX IF NOT EXISTS idx_messages_from_date \
             ON messages(from_email, date DESC); \
             CREATE INDEX IF NOT EXISTS idx_attachments_mime \
             ON attachments(mime_type)",
        )
        .execute(&self.writer)
        .await?;
        sqlx::raw_sql(include_str!("../migrations/008_account_addresses.sql"))
            .execute(&self.writer)
            .await?;
        sqlx::raw_sql(include_str!("../migrations/009_reply_pairs.sql"))
            .execute(&self.writer)
            .await?;
        sqlx::raw_sql(include_str!("../migrations/010_contacts.sql"))
            .execute(&self.writer)
            .await?;
        self.add_column_if_missing(
            "drafts",
            "status",
            "ALTER TABLE drafts ADD COLUMN status TEXT NOT NULL DEFAULT 'draft' \
             CHECK (status IN ('draft', 'sending', 'sent'))",
        )
        .await?;
        self.add_column_if_missing(
            "drafts",
            "status_updated_at",
            "ALTER TABLE drafts ADD COLUMN status_updated_at INTEGER",
        )
        .await?;
        self.add_column_if_missing(
            "drafts",
            "message_id_header",
            "ALTER TABLE drafts ADD COLUMN message_id_header TEXT",
        )
        .await?;
        sqlx::raw_sql("CREATE INDEX IF NOT EXISTS idx_drafts_status ON drafts(account_id, status)")
            .execute(&self.writer)
            .await?;
        self.validate_schema().await?;
        Ok(())
    }

    async fn add_column_if_missing(
        &self,
        table: &str,
        column: &str,
        sql: &str,
    ) -> Result<(), sqlx::Error> {
        if !self.column_exists(table, column).await? {
            sqlx::raw_sql(sql).execute(&self.writer).await?;
        }
        Ok(())
    }

    async fn column_exists(&self, table: &str, column: &str) -> Result<bool, sqlx::Error> {
        let query = format!("PRAGMA table_info({table})");
        let rows = sqlx::query_as::<_, (i64, String, String, i64, Option<String>, i64)>(&query)
            .fetch_all(&self.writer)
            .await?;
        Ok(rows.iter().any(|(_, name, _, _, _, _)| name == column))
    }

    async fn validate_schema(&self) -> Result<(), sqlx::Error> {
        for (table, columns) in REQUIRED_COLUMNS {
            for column in *columns {
                if !self.column_exists(table, column).await? {
                    return Err(sqlx::Error::Protocol(format!(
                        "store schema is missing required column {table}.{column}"
                    )));
                }
            }
        }
        Ok(())
    }

    pub fn writer(&self) -> &SqlitePool {
        &self.writer
    }

    pub fn reader(&self) -> &SqlitePool {
        &self.reader
    }
}

const REQUIRED_COLUMNS: &[(&str, &[&str])] = &[
    (
        "bodies",
        &[
            "message_id",
            "text_plain",
            "text_html",
            "fetched_at",
            "metadata_json",
        ],
    ),
    (
        "attachments",
        &[
            "id",
            "message_id",
            "filename",
            "mime_type",
            "size_bytes",
            "local_path",
            "provider_id",
            "disposition",
            "content_id",
            "content_location",
        ],
    ),
    ("saved_searches", &["id", "name", "query", "search_mode"]),
    (
        "sync_runtime_status",
        &["account_id", "sync_in_progress", "updated_at"],
    ),
    (
        "semantic_profiles",
        &[
            "id",
            "profile_name",
            "backend",
            "model_revision",
            "dimensions",
        ],
    ),
    (
        "message_events",
        &[
            "id",
            "message_id",
            "account_id",
            "event_type",
            "source",
            "label_id",
            "occurred_at",
            "metadata_json",
        ],
    ),
    (
        "messages",
        &[
            "direction",
            "list_id",
            "body_word_count",
            "body_quoted_lines",
        ],
    ),
    ("account_addresses", &["account_id", "email", "is_primary"]),
    (
        "reply_pairs",
        &[
            "reply_message_id",
            "parent_message_id",
            "account_id",
            "counterparty_email",
            "direction",
            "parent_received_at",
            "replied_at",
            "latency_seconds",
            "business_hours_latency_seconds",
            "created_at",
        ],
    ),
    (
        "reply_pair_pending",
        &[
            "reply_message_id",
            "in_reply_to_header",
            "account_id",
            "created_at",
        ],
    ),
    (
        "contacts",
        &[
            "account_id",
            "email",
            "display_name",
            "first_seen_at",
            "last_seen_at",
            "last_inbound_at",
            "last_outbound_at",
            "total_inbound",
            "total_outbound",
            "replied_count",
            "cadence_days_p50",
            "is_list_sender",
            "list_id",
            "refreshed_at",
        ],
    ),
];

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::Store;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::str::FromStr;
    use tempfile::tempdir;

    #[tokio::test]
    async fn opening_malformed_partial_db_errors_instead_of_silently_accepting_it() {
        let temp = tempdir().unwrap();
        let db_path = temp.path().join("partial.db");
        let url = format!("sqlite:{}", db_path.display());
        let opts = SqliteConnectOptions::from_str(&url)
            .unwrap()
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();

        sqlx::raw_sql("CREATE TABLE bodies (message_id TEXT PRIMARY KEY)")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        let result = Store::new(&db_path).await;
        assert!(
            result.is_err(),
            "malformed partial store opened successfully"
        );
        let error = result.err().expect("error checked above");

        assert!(
            error.to_string().contains("bodies.text_plain"),
            "expected missing column error, got {error}"
        );
    }
}
