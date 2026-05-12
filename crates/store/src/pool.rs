use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

/// SQLite-level busy retry. Multiple connections can be told to wait
/// for the writer lock instead of erroring with SQLITE_BUSY. Pairs with
/// the pool-level `acquire_timeout` below: the pool decides how long a
/// task waits for a connection, and `busy_timeout` decides how long
/// SQLite itself waits for the underlying lock once it has one.
const BUSY_TIMEOUT: Duration = Duration::from_secs(30);

/// Pool-level wait. The default is 30s; we bump it because the writer
/// pool is single-connection and serializes all writes — sync, contacts
/// refresh, mutations, snooze wakes, reply-pair reconciler. Under
/// realistic load these queue behind a long aggregate (contacts
/// refresh) and the default trips, surfacing as cascading "Sync error /
/// Mutation Failed / Contacts refresh error" entries.
const POOL_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(90);

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
            .busy_timeout(BUSY_TIMEOUT)
            .pragma("foreign_keys", "ON");

        let writer = SqlitePoolOptions::new()
            .max_connections(1)
            .acquire_timeout(POOL_ACQUIRE_TIMEOUT)
            .connect_with(write_opts)
            .await?;

        let read_opts = SqliteConnectOptions::from_str(&db_url)?
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(BUSY_TIMEOUT)
            .pragma("foreign_keys", "ON")
            .read_only(true);

        let reader = SqlitePoolOptions::new()
            .max_connections(4)
            .acquire_timeout(POOL_ACQUIRE_TIMEOUT)
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
        // Bootstrap the version-tracking table itself. Done outside the
        // version-tracked path so existing DBs (which may have all migrations
        // applied but no version stamps) get a clean target to backfill into.
        sqlx::raw_sql(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version    INTEGER PRIMARY KEY,
                name       TEXT NOT NULL,
                applied_at INTEGER NOT NULL
            )",
        )
        .execute(&self.writer)
        .await?;

        for migration in MIGRATIONS {
            self.apply_migration(migration).await?;
        }

        self.validate_schema().await?;
        Ok(())
    }

    async fn apply_migration(&self, migration: &Migration) -> Result<(), sqlx::Error> {
        if self.is_migration_applied(migration.version).await? {
            return Ok(());
        }

        // The earlier draft wrapped each migration in a sqlx Transaction and
        // ran column-exists checks against `&mut *tx`, but that propagated a
        // higher-ranked Send bound through the daemon's request-handler future
        // (server.rs:202 spawns into a tokio JoinSet and the bound poisoned
        // unrelated handlers). Use the writer pool directly:
        // `add_column_if_missing` is idempotent, every embedded SQL file uses
        // `CREATE TABLE/INDEX IF NOT EXISTS`, and the schema_migrations stamp
        // is the last write — a crash mid-migration causes the next run to
        // re-apply the (idempotent) body and then stamp.
        match migration.kind {
            MigrationKind::Sql(sql) => {
                sqlx::raw_sql(sql).execute(&self.writer).await?;
            }
            MigrationKind::AddColumn { table, column, sql } => {
                self.add_column_if_missing(table, column, sql).await?;
            }
            MigrationKind::Composite(steps) => {
                for step in steps.iter() {
                    match step {
                        MigrationStep::Sql(sql) => {
                            sqlx::raw_sql(sql).execute(&self.writer).await?;
                        }
                        MigrationStep::AddColumn { table, column, sql } => {
                            self.add_column_if_missing(table, column, sql).await?;
                        }
                    }
                }
            }
        }

        let applied_at = chrono::Utc::now().timestamp();
        sqlx::query("INSERT INTO schema_migrations (version, name, applied_at) VALUES (?, ?, ?)")
            .bind(migration.version as i64)
            .bind(migration.name)
            .bind(applied_at)
            .execute(&self.writer)
            .await?;
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

    async fn is_migration_applied(&self, version: u32) -> Result<bool, sqlx::Error> {
        let row: Option<(i64,)> =
            sqlx::query_as("SELECT version FROM schema_migrations WHERE version = ?")
                .bind(version as i64)
                .fetch_optional(&self.writer)
                .await?;
        Ok(row.is_some())
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

struct Migration {
    version: u32,
    name: &'static str,
    kind: MigrationKind,
}

enum MigrationKind {
    Sql(&'static str),
    AddColumn {
        table: &'static str,
        column: &'static str,
        sql: &'static str,
    },
    Composite(&'static [MigrationStep]),
}

enum MigrationStep {
    Sql(&'static str),
    AddColumn {
        table: &'static str,
        column: &'static str,
        sql: &'static str,
    },
}

const SEMANTIC_SEARCH_SQL: &str = r#"
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
"#;

const ATTACHMENTS_INLINE_METADATA_STEPS: &[MigrationStep] = &[
    MigrationStep::AddColumn {
        table: "attachments",
        column: "disposition",
        sql: "ALTER TABLE attachments ADD COLUMN disposition TEXT NOT NULL DEFAULT 'unspecified'",
    },
    MigrationStep::AddColumn {
        table: "attachments",
        column: "content_id",
        sql: "ALTER TABLE attachments ADD COLUMN content_id TEXT",
    },
    MigrationStep::AddColumn {
        table: "attachments",
        column: "content_location",
        sql: "ALTER TABLE attachments ADD COLUMN content_location TEXT",
    },
    MigrationStep::Sql(
        "CREATE INDEX IF NOT EXISTS idx_attachments_content_id ON attachments(content_id)",
    ),
];

const MESSAGE_ANALYTICS_STEPS: &[MigrationStep] = &[
    MigrationStep::AddColumn {
        table: "messages",
        column: "direction",
        sql: "ALTER TABLE messages ADD COLUMN direction TEXT NOT NULL DEFAULT 'unknown' \
              CHECK (direction IN ('inbound', 'outbound', 'unknown'))",
    },
    MigrationStep::AddColumn {
        table: "messages",
        column: "list_id",
        sql: "ALTER TABLE messages ADD COLUMN list_id TEXT",
    },
    MigrationStep::AddColumn {
        table: "messages",
        column: "body_word_count",
        sql: "ALTER TABLE messages ADD COLUMN body_word_count INTEGER",
    },
    MigrationStep::AddColumn {
        table: "messages",
        column: "body_quoted_lines",
        sql: "ALTER TABLE messages ADD COLUMN body_quoted_lines INTEGER",
    },
    MigrationStep::Sql(
        "CREATE INDEX IF NOT EXISTS idx_messages_account_direction_date \
         ON messages(account_id, direction, date DESC); \
         CREATE INDEX IF NOT EXISTS idx_messages_list_id \
         ON messages(list_id) WHERE list_id IS NOT NULL; \
         CREATE INDEX IF NOT EXISTS idx_messages_from_date \
         ON messages(from_email, date DESC); \
         CREATE INDEX IF NOT EXISTS idx_attachments_mime \
         ON attachments(mime_type)",
    ),
];

const DRAFT_STATUS_STEPS: &[MigrationStep] = &[
    MigrationStep::AddColumn {
        table: "drafts",
        column: "status",
        sql: "ALTER TABLE drafts ADD COLUMN status TEXT NOT NULL DEFAULT 'draft' \
              CHECK (status IN ('draft', 'sending', 'sent'))",
    },
    MigrationStep::AddColumn {
        table: "drafts",
        column: "status_updated_at",
        sql: "ALTER TABLE drafts ADD COLUMN status_updated_at INTEGER",
    },
    MigrationStep::AddColumn {
        table: "drafts",
        column: "message_id_header",
        sql: "ALTER TABLE drafts ADD COLUMN message_id_header TEXT",
    },
    MigrationStep::Sql(
        "CREATE INDEX IF NOT EXISTS idx_drafts_status ON drafts(account_id, status)",
    ),
];

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "initial",
        kind: MigrationKind::Sql(include_str!("../migrations/001_initial.sql")),
    },
    Migration {
        version: 2,
        name: "body_metadata",
        kind: MigrationKind::AddColumn {
            table: "bodies",
            column: "metadata_json",
            sql: "ALTER TABLE bodies ADD COLUMN metadata_json TEXT NOT NULL DEFAULT '{}'",
        },
    },
    Migration {
        version: 3,
        name: "sync_runtime_status",
        kind: MigrationKind::Sql(include_str!("../migrations/003_sync_runtime_status.sql")),
    },
    Migration {
        version: 4,
        name: "semantic_search",
        kind: MigrationKind::Composite(&[
            MigrationStep::AddColumn {
                table: "saved_searches",
                column: "search_mode",
                sql: "ALTER TABLE saved_searches ADD COLUMN search_mode TEXT NOT NULL DEFAULT '\"lexical\"'",
            },
            MigrationStep::Sql(SEMANTIC_SEARCH_SQL),
        ]),
    },
    Migration {
        version: 5,
        name: "inline_attachment_metadata",
        kind: MigrationKind::Composite(ATTACHMENTS_INLINE_METADATA_STEPS),
    },
    Migration {
        version: 6,
        name: "message_events",
        kind: MigrationKind::Sql(include_str!("../migrations/006_message_events.sql")),
    },
    Migration {
        version: 7,
        name: "message_analytics_columns",
        kind: MigrationKind::Composite(MESSAGE_ANALYTICS_STEPS),
    },
    Migration {
        version: 8,
        name: "account_addresses",
        kind: MigrationKind::Sql(include_str!("../migrations/008_account_addresses.sql")),
    },
    Migration {
        version: 9,
        name: "reply_pairs",
        kind: MigrationKind::Sql(include_str!("../migrations/009_reply_pairs.sql")),
    },
    Migration {
        version: 10,
        name: "contacts",
        kind: MigrationKind::Sql(include_str!("../migrations/010_contacts.sql")),
    },
    Migration {
        version: 11,
        name: "draft_status",
        kind: MigrationKind::Composite(DRAFT_STATUS_STEPS),
    },
    Migration {
        version: 12,
        name: "mutation_undo_log",
        kind: MigrationKind::Sql(include_str!("../migrations/012_mutation_undo_log.sql")),
    },
    Migration {
        version: 13,
        name: "message_flags",
        kind: MigrationKind::Sql(include_str!("../migrations/013_message_flags.sql")),
    },
    Migration {
        version: 14,
        name: "auto_reminders",
        kind: MigrationKind::Sql(include_str!("../migrations/014_auto_reminders.sql")),
    },
    Migration {
        version: 15,
        name: "scheduled_sends",
        kind: MigrationKind::Composite(&[
            MigrationStep::AddColumn {
                table: "drafts",
                column: "send_at",
                sql: "ALTER TABLE drafts ADD COLUMN send_at INTEGER",
            },
            MigrationStep::Sql(
                "CREATE INDEX IF NOT EXISTS idx_drafts_pending_scheduled \
                 ON drafts(send_at) \
                 WHERE send_at IS NOT NULL AND status = 'draft'",
            ),
        ]),
    },
    Migration {
        version: 16,
        name: "snippets",
        kind: MigrationKind::Sql(include_str!("../migrations/016_snippets.sql")),
    },
    Migration {
        version: 17,
        name: "draft_heartbeat",
        kind: MigrationKind::AddColumn {
            table: "drafts",
            column: "last_heartbeat_at",
            sql: "ALTER TABLE drafts ADD COLUMN last_heartbeat_at INTEGER",
        },
    },
    Migration {
        version: 18,
        name: "screener_decisions",
        kind: MigrationKind::Sql(include_str!("../migrations/018_screener_decisions.sql")),
    },
    Migration {
        version: 19,
        name: "analytics_account_date_index",
        kind: MigrationKind::Sql(include_str!(
            "../migrations/019_analytics_account_date_index.sql"
        )),
    },
    Migration {
        version: 20,
        name: "signatures",
        kind: MigrationKind::Sql(include_str!("../migrations/020_signatures.sql")),
    },
    Migration {
        version: 21,
        name: "thread_summaries",
        kind: MigrationKind::Sql(include_str!("../migrations/021_thread_summaries.sql")),
    },
    Migration {
        version: 22,
        name: "contact_style",
        kind: MigrationKind::Sql(include_str!("../migrations/022_contact_style.sql")),
    },
    Migration {
        version: 23,
        name: "contact_relationship_summary",
        kind: MigrationKind::Sql(include_str!(
            "../migrations/023_contact_relationship_summary.sql"
        )),
    },
    Migration {
        version: 24,
        name: "contact_commitments",
        kind: MigrationKind::Sql(include_str!("../migrations/024_contact_commitments.sql")),
    },
    Migration {
        version: 25,
        name: "user_voice_profile",
        kind: MigrationKind::Sql(include_str!("../migrations/025_user_voice_profile.sql")),
    },
];

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
        "signatures",
        &["id", "name", "body", "created_at", "updated_at"],
    ),
    (
        "signature_defaults",
        &[
            "scope_key",
            "kind",
            "signature_id",
            "account_id",
            "from_email",
        ],
    ),
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
        "thread_summaries",
        &["thread_id", "account_id", "content_hash", "text", "model"],
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

    #[tokio::test]
    async fn fresh_store_records_all_migration_versions() {
        let store = Store::in_memory().await.unwrap();
        let rows: Vec<(i64, String)> =
            sqlx::query_as("SELECT version, name FROM schema_migrations ORDER BY version")
                .fetch_all(store.writer())
                .await
                .unwrap();
        assert_eq!(rows.len(), super::MIGRATIONS.len());
        for (i, (version, name)) in rows.iter().enumerate() {
            let expected = &super::MIGRATIONS[i];
            assert_eq!(*version, expected.version as i64);
            assert_eq!(name, expected.name);
        }
    }

    #[tokio::test]
    async fn re_running_migrations_is_idempotent() {
        let temp = tempdir().unwrap();
        let db_path = temp.path().join("idem.db");

        {
            let store = Store::new(&db_path).await.unwrap();
            drop(store);
        }

        let store = Store::new(&db_path).await.unwrap();
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM schema_migrations")
            .fetch_one(store.writer())
            .await
            .unwrap();
        assert_eq!(count.0 as usize, super::MIGRATIONS.len());
    }

    #[tokio::test]
    async fn pre_versioning_db_is_backfilled_into_schema_migrations() {
        // Simulates a user who ran an older daemon that applied every migration
        // imperatively without recording versions. On next launch the new code
        // must stamp every migration row without re-running any migration body
        // (because all `CREATE TABLE IF NOT EXISTS` / column-exists checks pass).
        let temp = tempdir().unwrap();
        let db_path = temp.path().join("legacy.db");

        // First open: produces a fully-migrated DB.
        {
            let store = Store::new(&db_path).await.unwrap();
            drop(store);
        }

        // Erase the version stamps to simulate a pre-versioning install.
        {
            let url = format!("sqlite:{}", db_path.display());
            let opts = SqliteConnectOptions::from_str(&url)
                .unwrap()
                .create_if_missing(false);
            let pool = SqlitePoolOptions::new()
                .max_connections(1)
                .connect_with(opts)
                .await
                .unwrap();
            sqlx::query("DELETE FROM schema_migrations")
                .execute(&pool)
                .await
                .unwrap();
            pool.close().await;
        }

        // Re-open: should backfill cleanly without complaints.
        let store = Store::new(&db_path).await.unwrap();
        let rows: Vec<(i64,)> =
            sqlx::query_as("SELECT version FROM schema_migrations ORDER BY version")
                .fetch_all(store.writer())
                .await
                .unwrap();
        assert_eq!(rows.len(), super::MIGRATIONS.len());
    }
}
