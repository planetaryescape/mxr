use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;

use crate::ChangeFeed;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
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

/// What a pool's `after_connect` callback returns.
type ConnectFuture<'c> =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), sqlx::Error>> + Send + 'c>>;

/// Install the change hooks on each writer connection the pool opens.
fn watch_changes(
    changes: &Arc<ChangeFeed>,
) -> impl for<'c> Fn(
    &'c mut sqlx::SqliteConnection,
    sqlx::pool::PoolConnectionMetadata,
) -> ConnectFuture<'c>
       + Send
       + Sync
       + 'static {
    let changes = Arc::clone(changes);
    move |connection, _| {
        let changes = Arc::clone(&changes);
        Box::pin(async move { changes.watch(connection).await })
    }
}

pub struct Store {
    writer: SqlitePool,
    /// Commits on `writer` that can change a mail view.
    changes: Arc<ChangeFeed>,
    resource_dir: std::path::PathBuf,
    reader: SqlitePool,
    /// Serialises the cleanup of a deleted message's files and index
    /// entries with sync storing messages, so a message id that comes back
    /// (an IMAP UID reused for a new message) is never cleaned up as the
    /// deleted one. See `lock_message_cleanup`.
    pub(crate) message_cleanup: tokio::sync::Mutex<()>,
}

impl Store {
    pub fn draft_attachment_dir(&self) -> &Path {
        &self.resource_dir
    }
    pub async fn new(db_path: &Path) -> Result<Self, sqlx::Error> {
        if db_path
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink())
        {
            return Err(sqlx::Error::Protocol(
                "Database path must not be a symlink".into(),
            ));
        }
        crate::workspace_lease::retain_shared(db_path)
            .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
        let db_url = format!("sqlite:{}", db_path.display());

        let write_opts = SqliteConnectOptions::from_str(&db_url)?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            // Authority commits must survive host power loss; keep the pooled policy constant.
            .synchronous(SqliteSynchronous::Full)
            .pragma("fullfsync", "ON")
            .busy_timeout(BUSY_TIMEOUT)
            .pragma("foreign_keys", "ON");

        let changes = Arc::new(ChangeFeed::default());
        let writer = SqlitePoolOptions::new()
            .max_connections(1)
            .acquire_timeout(POOL_ACQUIRE_TIMEOUT)
            .after_connect(watch_changes(&changes))
            .connect_with(write_opts)
            .await?;

        let read_opts = SqliteConnectOptions::from_str(&db_url)?
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .busy_timeout(BUSY_TIMEOUT)
            .pragma("foreign_keys", "ON")
            .read_only(true);

        // 6 readers: WAL readers don't block each other and are cheap.
        // Background DB work is capped below this by the daemon's
        // `BACKGROUND_DB_PERMITS` semaphore, so at least
        // `6 - BACKGROUND_DB_PERMITS` connections always stay free for
        // interactive/status traffic even under full background load.
        let reader = SqlitePoolOptions::new()
            .max_connections(6)
            .acquire_timeout(POOL_ACQUIRE_TIMEOUT)
            .connect_with(read_opts)
            .await?;

        let store = Self {
            writer,
            changes,
            reader,
            resource_dir: db_path.with_extension("draft-attachments"),
            message_cleanup: tokio::sync::Mutex::new(()),
        };
        store.run_migrations().await?;
        Ok(store)
    }

    // Only private staging databases use this constructor; the offline caller owns the workspace lease.
    pub(crate) async fn fresh_backup(db_path: &Path) -> Result<Self, sqlx::Error> {
        if db_path.exists() {
            return Err(sqlx::Error::Protocol(
                "Backup staging database already exists".into(),
            ));
        }
        let options = SqliteConnectOptions::new()
            .filename(db_path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Delete)
            .synchronous(SqliteSynchronous::Full)
            .pragma("foreign_keys", "ON");
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        let store = Self {
            writer: pool.clone(),
            changes: Arc::new(ChangeFeed::default()),
            reader: pool,
            resource_dir: db_path.with_extension("draft-attachments"),
            message_cleanup: tokio::sync::Mutex::new(()),
        };
        store.run_migrations().await?;
        Ok(store)
    }

    pub async fn in_memory() -> Result<Self, sqlx::Error> {
        let opts = SqliteConnectOptions::from_str("sqlite::memory:")?
            .journal_mode(SqliteJournalMode::Wal)
            .pragma("foreign_keys", "ON");

        let changes = Arc::new(ChangeFeed::default());
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .after_connect(watch_changes(&changes))
            .connect_with(opts)
            .await?;

        let store = Self {
            resource_dir: std::env::temp_dir()
                .join(format!("mxr-draft-attachments-{}", uuid::Uuid::now_v7())),
            writer: pool.clone(),
            changes,
            reader: pool,
            message_cleanup: tokio::sync::Mutex::new(()),
        };
        store.run_migrations().await?;
        Ok(store)
    }

    pub(crate) async fn run_migrations(&self) -> Result<(), sqlx::Error> {
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
        // `add_column_if_missing` is idempotent, embedded SQL files are kept
        // rerunnable, and the few table-rewrite migrations use custom
        // recovery below. The schema_migrations stamp is the last write.
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
                            sqlx::raw_sql(sqlx::AssertSqlSafe(*sql))
                                .execute(&self.writer)
                                .await?;
                        }
                        MigrationStep::AddColumn { table, column, sql } => {
                            self.add_column_if_missing(table, column, sql).await?;
                        }
                    }
                }
            }
            MigrationKind::DecisionLogStableId => {
                self.apply_decision_log_stable_id_migration().await?;
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
            sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
                .execute(&self.writer)
                .await?;
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
        let rows = sqlx::query_as::<_, (i64, String, String, i64, Option<String>, i64)>(
            sqlx::AssertSqlSafe(query.as_str()),
        )
        .fetch_all(&self.writer)
        .await?;
        Ok(rows.iter().any(|(_, name, _, _, _, _)| name == column))
    }

    async fn table_exists(&self, table: &str) -> Result<bool, sqlx::Error> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT name FROM sqlite_schema WHERE type = 'table' AND name = ?")
                .bind(table)
                .fetch_optional(&self.writer)
                .await?;
        Ok(row.is_some())
    }

    async fn apply_decision_log_stable_id_migration(&self) -> Result<(), sqlx::Error> {
        let has_decision_log = self.table_exists("decision_log").await?;
        let has_temp = self.table_exists("decision_log_v2").await?;
        let rewrite_sql = match (has_decision_log, has_temp) {
            (true, true) => DECISION_LOG_DROP_TEMP_AND_REBUILD_SQL,
            (true, false) => DECISION_LOG_REBUILD_SQL,
            (false, true) => DECISION_LOG_PROMOTE_TEMP_SQL,
            (false, false) => DECISION_LOG_CREATE_FINAL_SQL,
        };

        self.run_decision_log_rewrite(rewrite_sql).await?;
        sqlx::raw_sql(DECISION_LOG_INDEX_SQL)
            .execute(&self.writer)
            .await?;
        Ok(())
    }

    async fn run_decision_log_rewrite(&self, sql: &str) -> Result<(), sqlx::Error> {
        sqlx::raw_sql("PRAGMA foreign_keys = OFF")
            .execute(&self.writer)
            .await?;

        let result = async {
            sqlx::raw_sql("BEGIN IMMEDIATE")
                .execute(&self.writer)
                .await?;
            if let Err(error) = sqlx::raw_sql(sqlx::AssertSqlSafe(sql))
                .execute(&self.writer)
                .await
            {
                let _ = sqlx::raw_sql("ROLLBACK").execute(&self.writer).await;
                return Err(error);
            }
            if let Err(error) = sqlx::raw_sql("COMMIT").execute(&self.writer).await {
                let _ = sqlx::raw_sql("ROLLBACK").execute(&self.writer).await;
                return Err(error);
            }
            Ok(())
        }
        .await;

        let foreign_keys_result = sqlx::raw_sql("PRAGMA foreign_keys = ON")
            .execute(&self.writer)
            .await;
        match (result, foreign_keys_result) {
            (Err(error), _) => Err(error),
            (Ok(()), Err(error)) => Err(error),
            (Ok(()), Ok(_)) => Ok(()),
        }
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

    /// Commits that can change a mail view; see [`ChangeFeed`].
    pub fn change_feed(&self) -> &ChangeFeed {
        &self.changes
    }

    pub fn writer(&self) -> &SqlitePool {
        &self.writer
    }

    /// The change feed's count once no write is in flight, or `None` if the
    /// writer stays busy for `wait`.
    ///
    /// SQLite's commit hook counts a commit before readers can see it, so the
    /// count alone may include a commit a read would miss. The writer pool
    /// has one connection, and sqlx hands it on only once its last holder is
    /// done with it, a cancelled or closing commit included. Holding it,
    /// nothing can be mid-commit, so every commit in the count is visible.
    pub async fn changes_landed(&self, wait: Duration) -> Option<u64> {
        let writer = tokio::time::timeout(wait, self.writer.acquire())
            .await
            .ok()?
            .ok()?;
        let seq = self.changes.seq();
        drop(writer);
        Some(seq)
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
    DecisionLogStableId,
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

const CONTACT_STYLE_DRIFT_STEPS: &[MigrationStep] = &[
    MigrationStep::AddColumn {
        table: "contact_style",
        column: "drift_detected",
        sql: "ALTER TABLE contact_style ADD COLUMN drift_detected INTEGER NOT NULL DEFAULT 0",
    },
    MigrationStep::AddColumn {
        table: "contact_style",
        column: "drift_reason",
        sql: "ALTER TABLE contact_style ADD COLUMN drift_reason TEXT",
    },
    MigrationStep::AddColumn {
        table: "contact_style",
        column: "drift_detected_at",
        sql: "ALTER TABLE contact_style ADD COLUMN drift_detected_at INTEGER",
    },
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

const DECISION_LOG_CREATE_FINAL_SQL: &str = r#"
CREATE TABLE decision_log (
    id                  TEXT PRIMARY KEY,
    account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    thread_id           TEXT NOT NULL,
    topic               TEXT,
    decision            TEXT NOT NULL,
    rationale           TEXT,
    evidence_msg_ids    TEXT NOT NULL,
    decided_at          INTEGER,
    extracted_at        INTEGER NOT NULL,
    source_hash         TEXT NOT NULL
);
"#;

const DECISION_LOG_REBUILD_SQL: &str = r#"
CREATE TABLE decision_log_v2 (
    id                  TEXT PRIMARY KEY,
    account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    thread_id           TEXT NOT NULL,
    topic               TEXT,
    decision            TEXT NOT NULL,
    rationale           TEXT,
    evidence_msg_ids    TEXT NOT NULL,
    decided_at          INTEGER,
    extracted_at        INTEGER NOT NULL,
    source_hash         TEXT NOT NULL
);

INSERT OR REPLACE INTO decision_log_v2
    (id, account_id, thread_id, topic, decision, rationale,
     evidence_msg_ids, decided_at, extracted_at, source_hash)
SELECT
    id, account_id, thread_id, topic, decision, rationale,
    evidence_msg_ids, decided_at, extracted_at, source_hash
FROM decision_log;

DROP TABLE decision_log;
ALTER TABLE decision_log_v2 RENAME TO decision_log;
"#;

const DECISION_LOG_DROP_TEMP_AND_REBUILD_SQL: &str = r#"
DROP TABLE decision_log_v2;
CREATE TABLE decision_log_v2 (
    id                  TEXT PRIMARY KEY,
    account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    thread_id           TEXT NOT NULL,
    topic               TEXT,
    decision            TEXT NOT NULL,
    rationale           TEXT,
    evidence_msg_ids    TEXT NOT NULL,
    decided_at          INTEGER,
    extracted_at        INTEGER NOT NULL,
    source_hash         TEXT NOT NULL
);

INSERT OR REPLACE INTO decision_log_v2
    (id, account_id, thread_id, topic, decision, rationale,
     evidence_msg_ids, decided_at, extracted_at, source_hash)
SELECT
    id, account_id, thread_id, topic, decision, rationale,
    evidence_msg_ids, decided_at, extracted_at, source_hash
FROM decision_log;

DROP TABLE decision_log;
ALTER TABLE decision_log_v2 RENAME TO decision_log;
"#;

const DECISION_LOG_PROMOTE_TEMP_SQL: &str = r#"
ALTER TABLE decision_log_v2 RENAME TO decision_log;
"#;

const DECISION_LOG_INDEX_SQL: &str = r#"
CREATE INDEX IF NOT EXISTS idx_decision_log_account_topic
    ON decision_log(account_id, topic);

CREATE INDEX IF NOT EXISTS idx_decision_log_account_decided
    ON decision_log(account_id, decided_at DESC);
"#;

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
    Migration {
        version: 26,
        name: "contact_style_drift",
        // Three ALTER TABLE ADD COLUMN steps. SQLite has no `ADD COLUMN
        // IF NOT EXISTS`, so we use idempotent `AddColumn` steps that
        // probe via `PRAGMA table_info`. This keeps the migration
        // re-runnable when a pre-versioning DB needs backfilling.
        kind: MigrationKind::Composite(CONTACT_STYLE_DRIFT_STEPS),
    },
    Migration {
        version: 27,
        name: "draft_intent",
        kind: MigrationKind::AddColumn {
            table: "drafts",
            column: "intent",
            sql: "ALTER TABLE drafts ADD COLUMN intent TEXT NOT NULL DEFAULT 'new'",
        },
    },
    Migration {
        version: 28,
        name: "draft_safety_audit",
        kind: MigrationKind::Sql(include_str!("../migrations/028_draft_safety_audit.sql")),
    },
    Migration {
        version: 29,
        name: "draft_commitment_candidates",
        kind: MigrationKind::Sql(include_str!(
            "../migrations/029_draft_commitment_candidates.sql"
        )),
    },
    Migration {
        version: 30,
        name: "decision_log",
        kind: MigrationKind::Sql(include_str!("../migrations/030_decision_log.sql")),
    },
    Migration {
        version: 31,
        name: "relationship_watchlist",
        kind: MigrationKind::Sql(include_str!(
            "../migrations/031_relationship_watchlist.sql"
        )),
    },
    Migration {
        version: 32,
        name: "context_briefings",
        kind: MigrationKind::Sql(include_str!(
            "../migrations/032_context_briefings.sql"
        )),
    },
    Migration {
        version: 33,
        name: "link_count",
        kind: MigrationKind::AddColumn {
            table: "messages",
            column: "link_count",
            sql: "ALTER TABLE messages ADD COLUMN link_count INTEGER NOT NULL DEFAULT 0",
        },
    },
    Migration {
        version: 34,
        name: "decision_log_stable_id",
        kind: MigrationKind::DecisionLogStableId,
    },
    Migration {
        version: 35,
        name: "user_activity",
        kind: MigrationKind::Sql(include_str!("../migrations/035_user_activity.sql")),
    },
    Migration {
        version: 36,
        name: "user_activity_fts",
        kind: MigrationKind::Sql(include_str!("../migrations/036_user_activity_fts.sql")),
    },
    Migration {
        version: 37,
        name: "saved_activity_filters",
        kind: MigrationKind::Sql(include_str!(
            "../migrations/037_saved_activity_filters.sql"
        )),
    },
    Migration {
        version: 38,
        name: "calendar_invites",
        kind: MigrationKind::Sql(include_str!("../migrations/038_calendar_invites.sql")),
    },
    Migration {
        version: 39,
        name: "draft_inline_calendar_reply",
        kind: MigrationKind::AddColumn {
            table: "drafts",
            column: "inline_calendar_reply_json",
            sql: "ALTER TABLE drafts ADD COLUMN inline_calendar_reply_json TEXT",
        },
    },
    Migration {
        version: 40,
        name: "mutation_dedup_log",
        kind: MigrationKind::Sql(include_str!("../migrations/040_mutation_dedup_log.sql")),
    },
    Migration {
        version: 41,
        name: "message_keywords",
        kind: MigrationKind::Sql(include_str!("../migrations/041_message_keywords.sql")),
    },
    Migration {
        version: 42,
        name: "deliveries",
        kind: MigrationKind::Sql(include_str!("../migrations/042_deliveries.sql")),
    },
    Migration {
        version: 43,
        name: "sent_draft_receipts",
        kind: MigrationKind::Sql(include_str!("../migrations/043_sent_draft_receipts.sql")),
    },
    Migration {
        version: 44,
        name: "triage_cache",
        kind: MigrationKind::Sql(include_str!("../migrations/044_triage_cache.sql")),
    },
    Migration {
        version: 45,
        name: "mutation_jobs",
        kind: MigrationKind::Sql(include_str!("../migrations/045_mutation_jobs.sql")),
    },
    Migration {
        version: 46,
        name: "scheduled_send_attempts",
        kind: MigrationKind::Sql(include_str!("../migrations/046_scheduled_send_attempts.sql")),
    },
    Migration {
        version: 47,
        name: "message_id_header_index",
        kind: MigrationKind::Sql(include_str!("../migrations/047_message_id_header_index.sql")),
    },
    Migration {
        version: 48,
        name: "draft_from_address",
        kind: MigrationKind::AddColumn {
            table: "drafts",
            column: "from_addr",
            // Per-message From override, JSON-encoded `Address`. NULL means
            // "send from the account's primary address" (default behaviour).
            sql: "ALTER TABLE drafts ADD COLUMN from_addr TEXT",
        },
    },
    Migration {
        version: 49,
        name: "draft_html_content",
        // Additive and forward-only: existing rows keep `body_markdown` and
        // take `content_kind = 'markdown'` from the default, so they decode as
        // `DraftContent::Markdown` without any row being rewritten.
        // Mirrors migrations/049_draft_html_content.sql.
        kind: MigrationKind::Composite(&[
            MigrationStep::AddColumn {
                table: "drafts",
                column: "body_html",
                // The caller's HTML document, stored byte-for-byte.
                sql: "ALTER TABLE drafts ADD COLUMN body_html TEXT",
            },
            MigrationStep::AddColumn {
                table: "drafts",
                column: "body_text",
                // Caller-supplied text/plain alternative; NULL means the
                // outbound builder generates one from the HTML.
                sql: "ALTER TABLE drafts ADD COLUMN body_text TEXT",
            },
            MigrationStep::AddColumn {
                table: "drafts",
                column: "inline_assets",
                // JSON array of `{cid, path}`. Paths only, like `attachments`,
                // so image bytes stay out of the DB and off the IPC frame.
                sql: "ALTER TABLE drafts ADD COLUMN inline_assets TEXT NOT NULL DEFAULT '[]'",
            },
            MigrationStep::AddColumn {
                table: "drafts",
                column: "content_kind",
                sql: "ALTER TABLE drafts ADD COLUMN content_kind TEXT NOT NULL DEFAULT 'markdown' \
                      CHECK (content_kind IN ('markdown', 'html'))",
            },
        ]),
    },
    Migration {
        version: 50,
        name: "provider_draft_id",
        kind: MigrationKind::Composite(&[
            MigrationStep::AddColumn {
                table: "drafts",
                column: "provider_draft_id",
                // Stable provider draft resource ID. The provider's nested
                // message ID may change whenever the draft content is replaced.
                sql: "ALTER TABLE drafts ADD COLUMN provider_draft_id TEXT",
            },
            MigrationStep::AddColumn {
                table: "drafts",
                column: "provider_draft_revision",
                // Gmail's nested message ID changes on every replacement and
                // is therefore a cheap remote-edit revision marker.
                sql: "ALTER TABLE drafts ADD COLUMN provider_draft_revision TEXT",
            },
        ]),
    },
    Migration {
        version: 51,
        name: "desk_dismissals",
        kind: MigrationKind::Sql(include_str!("../migrations/051_desk_dismissals.sql")),
    },
    Migration {
        version: 52,
        name: "message_pins",
        kind: MigrationKind::Sql(include_str!("../migrations/052_message_pins.sql")),
    },
    Migration {
        version: 53,
        name: "mutation_undo_desk_state",
        kind: MigrationKind::AddColumn {
            table: "mutation_undo_log",
            column: "desk_state_json",
            // The desk's Done also changes dismissals and promises; undo
            // puts those back from here (JSON `DeskUndo`, NULL otherwise).
            sql: "ALTER TABLE mutation_undo_log ADD COLUMN desk_state_json TEXT",
        },
    },
    Migration {
        version: 54,
        name: "owed_replies_index",
        kind: MigrationKind::Sql(include_str!("../migrations/054_owed_replies_index.sql")),
    },
    Migration {
        version: 55,
        name: "history_text_fingerprints",
        kind: MigrationKind::Sql(include_str!(
            "../migrations/055_history_text_fingerprints.sql"
        )),
    },
    Migration {
        version: 56,
        name: "reply_later_due",
        // Timed reply later. Mirrors migrations/056_reply_later_due.sql.
        kind: MigrationKind::Composite(&[
            MigrationStep::AddColumn {
                table: "message_flags",
                column: "reply_later_due_at",
                // When a timed reply later comes back; NULL is untimed.
                sql: "ALTER TABLE message_flags ADD COLUMN reply_later_due_at INTEGER",
            },
            MigrationStep::AddColumn {
                table: "message_flags",
                column: "reply_later_returned_at",
                // Set once, by the wake loop that announces the return.
                sql: "ALTER TABLE message_flags ADD COLUMN reply_later_returned_at INTEGER",
            },
            MigrationStep::Sql(
                "CREATE INDEX IF NOT EXISTS idx_message_flags_reply_later_due
                     ON message_flags (reply_later_due_at)
                     WHERE reply_later = 1
                       AND reply_later_due_at IS NOT NULL
                       AND reply_later_returned_at IS NULL",
            ),
            // A reminder's replies filed in another thread, by parent.
            MigrationStep::Sql(
                "CREATE INDEX IF NOT EXISTS idx_reply_pairs_parent
                     ON reply_pairs (parent_message_id, direction)",
            ),
        ]),
    },    Migration {
        version: 57,
        name: "search_reindex_pending",
        kind: MigrationKind::Sql(include_str!(
            "../migrations/057_search_reindex_pending.sql"
        )),
    },
    Migration {
        version: 58,
        name: "message_deletion",
        kind: MigrationKind::Sql(include_str!("../migrations/058_message_deletion.sql")),
    },
    Migration {
        version: 59,
        name: "todos",
        kind: MigrationKind::Sql(include_str!("../migrations/059_todos.sql")),
    },
    Migration {
        version: 60,
        name: "mode_guide_seen",
        kind: MigrationKind::Sql(include_str!("../migrations/060_mode_guide_seen.sql")),
    },
    Migration {
        version: 61,
        name: "mode_done",
        kind: MigrationKind::Sql(include_str!("../migrations/061_mode_done.sql")),
    },
    Migration {
        version: 62,
        name: "done_watermark_by_date",
        kind: MigrationKind::Composite(&[
            MigrationStep::AddColumn {
                table: "desk_dismissals",
                column: "through_date",
                sql: "ALTER TABLE desk_dismissals ADD COLUMN through_date INTEGER NOT NULL DEFAULT 0",
            },
            MigrationStep::AddColumn {
                table: "desk_dismissals",
                column: "through_message_id",
                sql: "ALTER TABLE desk_dismissals ADD COLUMN through_message_id TEXT NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000'",
            },
            MigrationStep::AddColumn {
                table: "mode_done",
                column: "through_date",
                sql: "ALTER TABLE mode_done ADD COLUMN through_date INTEGER NOT NULL DEFAULT 0",
            },
            MigrationStep::AddColumn {
                table: "mode_done",
                column: "through_message_id",
                sql: "ALTER TABLE mode_done ADD COLUMN through_message_id TEXT NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000'",
            },
            MigrationStep::Sql(include_str!("../migrations/062_done_watermark_by_date.sql")),
        ]),
    },
    // Done marks also keep the ids of the messages they saw, so a message
    // the mark never saw is new whatever its Date header says. NULL on
    // older marks, which keep the (date, id) watermark and count.
    Migration {
        version: 63,
        name: "done_marks_covered_ids",
        kind: MigrationKind::Composite(&[
            MigrationStep::AddColumn {
                table: "desk_dismissals",
                column: "covered_ids",
                sql: "ALTER TABLE desk_dismissals ADD COLUMN covered_ids TEXT",
            },
            MigrationStep::AddColumn {
                table: "mode_done",
                column: "covered_ids",
                sql: "ALTER TABLE mode_done ADD COLUMN covered_ids TEXT",
            },
        ]),
    },
    Migration {
        version: 64,
        name: "messages",
        kind: MigrationKind::Sql(include_str!("../migrations/064_messages.sql")),
    },
    Migration {
        version: 65,
        name: "updates",
        kind: MigrationKind::Sql(include_str!("../migrations/065_updates.sql")),
    },
    Migration {
        version: 66,
        name: "reading",
        kind: MigrationKind::Sql(include_str!("../migrations/066_reading.sql")),
    },
    // Archive: records built from mail, with a value per field per source.
    // 66 belongs to the Reading phase.
    Migration {
        version: 67,
        name: "records",
        kind: MigrationKind::Sql(include_str!("../migrations/067_records.sql")),
    },
    // When mxr stored each message: freshness reads arrival time from it
    // instead of a Date header that may run ahead.
    Migration {
        version: 68,
        name: "message_stored_at",
        kind: MigrationKind::AddColumn {
            table: "messages",
            column: "stored_at",
            sql: "ALTER TABLE messages ADD COLUMN stored_at INTEGER",
        },
    },
    // Hints replace the page-top first-encounter cards (D118, amended
    // 2026-10-07): seen state is per hint, not per mode. The old per-mode
    // rows retired cards that no longer exist, so they are dropped, not
    // carried over.
    Migration {
        version: 69,
        name: "hint_seen",
        kind: MigrationKind::Sql(
            "CREATE TABLE IF NOT EXISTS hint_seen (
                 hint_id TEXT PRIMARY KEY,
                 seen_at INTEGER NOT NULL
             );
             DROP TABLE IF EXISTS mode_guide_seen;",
        ),
    },
    // Where each email went when it arrived, and the user's corrections
    // (D119). 68 belongs to message stored_at, 69 to contextual hints.
    Migration {
        version: 70,
        name: "arrivals",
        kind: MigrationKind::Sql(include_str!("../migrations/070_arrivals.sql")),
    },
    Migration {
        version: 71,
        name: "rule_treatments",
        kind: MigrationKind::Composite(&[
            MigrationStep::AddColumn {table:"rules",column:"account_id",sql:"ALTER TABLE rules ADD COLUMN account_id TEXT REFERENCES accounts(id) ON DELETE CASCADE"},
            MigrationStep::Sql(include_str!("../migrations/071_rule_treatments.sql")),
        ]),
    },
    Migration {
        version: 72,
        name: "draft_revision",
        kind: MigrationKind::Composite(&[MigrationStep::AddColumn {
            table: "drafts", column: "revision",
            sql: include_str!("../migrations/072_draft_revision.sql"),
        }]),
    },
    Migration { version: 73, name: "draft_send_outcomes", kind: MigrationKind::Sql(include_str!("../migrations/073_draft_send_outcomes.sql")) },
    Migration { version: 74, name: "scheduled_send_revision", kind: MigrationKind::Composite(&[
        MigrationStep::AddColumn { table: "drafts", column: "scheduled_send_revision", sql: include_str!("../migrations/074_scheduled_send_revision.sql") },
        MigrationStep::Sql("UPDATE drafts SET scheduled_send_revision = revision WHERE send_at IS NOT NULL AND scheduled_send_revision IS NULL"),
    ]) },
    Migration { version: 75, name: "archive_workspace", kind: MigrationKind::Sql(include_str!("../migrations/075_archive_workspace.sql")) },
    Migration {
        version: 76,
        name: "record_renewal_cycles",
        kind: MigrationKind::Sql(include_str!("../migrations/076_record_renewal_cycles.sql")),
    },
    Migration {
        version: 77,
        name: "personal_categories",
        kind: MigrationKind::Sql(include_str!("../migrations/077_personal_categories.sql")),
    },
    Migration {
        version: 78,
        name: "todo_resolution_suggestions",
        kind: MigrationKind::Sql(include_str!("../migrations/078_todo_resolution_suggestions.sql")),
    },
    Migration {
        version: 79,
        name: "draft_attachment_resources",
        kind: MigrationKind::Composite(&[
            MigrationStep::AddColumn { table: "drafts", column: "managed_attachments", sql: "ALTER TABLE drafts ADD COLUMN managed_attachments TEXT NOT NULL DEFAULT 'null'" },
            MigrationStep::Sql(include_str!("../migrations/079_draft_attachment_resources.sql")),
        ]),
    },
    Migration { version: 80, name: "rule_category_sources", kind: MigrationKind::Composite(&[MigrationStep::AddColumn {table:"message_category_memberships",column:"manual_decision",sql:include_str!("../migrations/080_category_manual_decision.sql")},MigrationStep::Sql(include_str!("../migrations/080_rule_category_sources.sql"))]) },
    Migration { version: 81, name: "restore_recovery", kind: MigrationKind::Sql(include_str!("../migrations/081_restore_recovery.sql")) },
    Migration { version: 82, name: "todo_resolution_queue", kind: MigrationKind::Sql(include_str!("../migrations/082_todo_resolution_queue.sql")) },
    Migration { version: 83, name: "remote_authority", kind: MigrationKind::Sql(include_str!("../migrations/083_remote_authority.sql")) },
    Migration { version: 84, name: "remote_pairing_retirements", kind: MigrationKind::Sql(include_str!("../migrations/084_remote_pairing_retirements.sql")) },
    Migration { version: 85, name: "remote_send_grant", kind: MigrationKind::Composite(&[
        MigrationStep::AddColumn { table: "remote_pairings", column: "allow_send", sql: include_str!("../migrations/085_remote_pairing_send.sql") },
        MigrationStep::AddColumn { table: "remote_clients", column: "allow_send", sql: include_str!("../migrations/085_remote_client_send.sql") },
    ]) },
    Migration { version: 86, name: "reading_marginalia", kind: MigrationKind::Composite(READING_MARGINALIA_STEPS) },
    Migration { version: 87, name: "reading_highlight_paragraph", kind: MigrationKind::AddColumn { table: "reading_highlights", column: "paragraph_index", sql: include_str!("../migrations/087_reading_highlight_paragraph.sql") } },
    Migration { version: 88, name: "assistant", kind: MigrationKind::Sql(include_str!("../migrations/088_assistant.sql")) },
    Migration { version: 89, name: "activity_sources", kind: MigrationKind::Sql(include_str!("../migrations/089_activity_sources.sql")) },
    Migration { version: 90, name: "assistant_trust_and_summary", kind: MigrationKind::Composite(ASSISTANT_TRUST_AND_SUMMARY_STEPS) },
    Migration { version: 91, name: "todo_microsoft", kind: MigrationKind::Sql(include_str!("../migrations/091_todo_microsoft.sql")) },
    Migration { version: 92, name: "messages_account_thread_index", kind: MigrationKind::Sql(include_str!("../migrations/092_messages_account_thread_index.sql")) },
];

/// The user's choice to let reversible changes run without asking, and the
/// summary that replaces older messages in what the model reads once a
/// conversation outgrows its context window (the messages themselves stay).
const ASSISTANT_TRUST_AND_SUMMARY_STEPS: &[MigrationStep] = &[
    MigrationStep::AddColumn {
        table: "assistant_conversations",
        column: "auto_approve",
        sql: "ALTER TABLE assistant_conversations ADD COLUMN auto_approve INTEGER NOT NULL DEFAULT 0 CHECK (auto_approve IN (0, 1))",
    },
    MigrationStep::AddColumn {
        table: "assistant_conversations",
        column: "summary",
        sql: "ALTER TABLE assistant_conversations ADD COLUMN summary TEXT",
    },
    MigrationStep::AddColumn {
        table: "assistant_conversations",
        column: "summary_through_seq",
        sql: "ALTER TABLE assistant_conversations ADD COLUMN summary_through_seq INTEGER",
    },
];

const READING_MARGINALIA_STEPS: &[MigrationStep] = &[
    MigrationStep::AddColumn {
        table: "reading_highlights",
        column: "origin",
        sql: "ALTER TABLE reading_highlights ADD COLUMN origin TEXT NOT NULL DEFAULT 'mxr'",
    },
    MigrationStep::AddColumn {
        table: "reading_highlights",
        column: "origin_id",
        sql: "ALTER TABLE reading_highlights ADD COLUMN origin_id TEXT",
    },
    MigrationStep::Sql(include_str!("../migrations/086_reading_marginalia.sql")),
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
        "triage_cache",
        &[
            "message_id",
            "account_id",
            "thread_id",
            "prompt_version",
            "content_hash",
            "verdict",
            "verdict_line",
            "model",
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
    (
        "user_activity",
        &[
            "id",
            "ts",
            "account_id",
            "source",
            "action",
            "target_kind",
            "target_id",
            "tier",
            "context_json",
            "redacted",
        ],
    ),
    (
        "calendar_invites",
        &[
            "id",
            "account_id",
            "message_id",
            "method",
            "uid",
            "metadata_json",
            "raw_ics",
        ],
    ),
];

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "tests unwrap fixture setup for direct failures"
    )]

    use super::Store;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use std::path::Path;
    use std::str::FromStr;
    use tempfile::tempdir;

    async fn open_test_writer(db_path: &Path) -> sqlx::SqlitePool {
        let url = format!("sqlite:{}", db_path.display());
        let opts = SqliteConnectOptions::from_str(&url)
            .unwrap()
            .create_if_missing(false);
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap()
    }

    async fn store_at_version(db_path: &Path, version: u32) -> Store {
        let url = format!("sqlite:{}", db_path.display());
        let opts = SqliteConnectOptions::from_str(&url)
            .unwrap()
            .create_if_missing(true)
            .pragma("foreign_keys", "ON");
        let writer = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();
        let store = Store {
            writer: writer.clone(),
            changes: std::sync::Arc::new(crate::ChangeFeed::default()),
            reader: writer,
            resource_dir: db_path.with_extension("draft-attachments"),
            message_cleanup: tokio::sync::Mutex::new(()),
        };
        sqlx::raw_sql(
            "CREATE TABLE schema_migrations (
                version INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                applied_at INTEGER NOT NULL
            )",
        )
        .execute(store.writer())
        .await
        .unwrap();

        for migration in super::MIGRATIONS
            .iter()
            .filter(|migration| migration.version <= version)
        {
            store.apply_migration(migration).await.unwrap();
        }

        let latest: (i64,) = sqlx::query_as("SELECT MAX(version) FROM schema_migrations")
            .fetch_one(store.writer())
            .await
            .unwrap();
        assert_eq!(latest.0, version as i64);
        store
    }

    async fn test_table_exists(pool: &sqlx::SqlitePool, table: &str) -> bool {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT name FROM sqlite_schema WHERE type = 'table' AND name = ?")
                .bind(table)
                .fetch_optional(pool)
                .await
                .unwrap();
        row.is_some()
    }

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
    async fn renewal_migration_upgrades_version_75_without_losing_user_state() {
        let temp = tempdir().unwrap();
        let db_path = temp.path().join("version-75.db");

        {
            let store = store_at_version(&db_path, 75).await;
            sqlx::query(
                "INSERT INTO archive_workspace (singleton, workspace_id, recovery_generation)
                 VALUES (1, 'existing-workspace', 'generation-before-renewals')",
            )
            .execute(store.writer())
            .await
            .unwrap();

            sqlx::query(
                "INSERT INTO accounts (id, name, email, created_at, updated_at)
                 VALUES ('version-75-account', 'Personal', 'me@example.com', 1, 1)",
            )
            .execute(store.writer())
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO draft_send_outcomes
                 (operation_id, draft_id, account_id, reviewed_revision, command_hash,
                  state, provider_message_id, updated_at)
                 VALUES
                 ('accepted-op', 'sent-draft', 'version-75-account', 3, 'accepted-hash',
                  'accepted', 'provider-message-9', 42),
                 ('unknown-op', 'sending-draft', 'version-75-account', 4, 'unknown-hash',
                  'unknown', NULL, 43)",
            )
            .execute(store.writer())
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO todos
                 (id, account_id, kind, verb, title, origin, reason, rules_version,
                  dedup_key, created_at, updated_at, scheduled_for, user_edited)
                 VALUES ('manual-renewal', 'version-75-account', 'renewal', 'renew',
                         'Call about renewal', 'manual', 'Created by the user', 1,
                         'manual-renewal-key', 10, 11, 1800000000, 1)",
            )
            .execute(store.writer())
            .await
            .unwrap();

            let latest_version: (i64,) =
                sqlx::query_as("SELECT MAX(version) FROM schema_migrations")
                    .fetch_one(store.writer())
                    .await
                    .unwrap();
            assert_eq!(latest_version.0, 75);
        }

        let store = Store::new(&db_path).await.unwrap();
        let applied: (String,) =
            sqlx::query_as("SELECT name FROM schema_migrations WHERE version = 76")
                .fetch_one(store.writer())
                .await
                .unwrap();
        assert_eq!(applied.0, "record_renewal_cycles");
        assert!(test_table_exists(store.writer(), "record_renewal_cycles").await);
        assert!(test_table_exists(store.writer(), "archive_workspace").await);
        sqlx::query(
            "INSERT INTO record_renewal_cycles (account_id, record_id, dedup_key)
             VALUES ('version-75-account', 'record-1', 'record-renewal|record-1')",
        )
        .execute(store.writer())
        .await
        .unwrap();

        let outcomes: Vec<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT operation_id, state, provider_message_id FROM draft_send_outcomes
             ORDER BY operation_id",
        )
        .fetch_all(store.writer())
        .await
        .unwrap();
        assert_eq!(
            outcomes,
            vec![
                (
                    "accepted-op".into(),
                    "accepted".into(),
                    Some("provider-message-9".into()),
                ),
                ("unknown-op".into(), "unknown".into(), None),
            ]
        );

        let manual_todo: (String, String, String, i64, i64) = sqlx::query_as(
            "SELECT title, origin, reason, scheduled_for, user_edited
             FROM todos WHERE id = 'manual-renewal'",
        )
        .fetch_one(store.writer())
        .await
        .unwrap();
        assert_eq!(
            manual_todo,
            (
                "Call about renewal".into(),
                "manual".into(),
                "Created by the user".into(),
                1_800_000_000,
                1,
            )
        );

        drop(store);
        let reopened = Store::new(&db_path).await.unwrap();
        let archived_workspace: (String, String) = sqlx::query_as(
            "SELECT workspace_id, recovery_generation FROM archive_workspace WHERE singleton = 1",
        )
        .fetch_one(reopened.writer())
        .await
        .unwrap();
        assert_eq!(
            archived_workspace,
            (
                "existing-workspace".into(),
                "generation-before-renewals".into(),
            )
        );
        let cycle_key: (String,) = sqlx::query_as(
            "SELECT dedup_key FROM record_renewal_cycles
             WHERE account_id = 'version-75-account' AND record_id = 'record-1'",
        )
        .fetch_one(reopened.writer())
        .await
        .unwrap();
        assert_eq!(cycle_key.0, "record-renewal|record-1");
    }

    #[tokio::test]
    async fn draft_attachment_resources_migration_upgrades_78_and_recovers_partial_79() {
        let temp = tempdir().unwrap();
        let db_path = temp.path().join("version-78.db");
        let draft_id = mxr_core::DraftId::new();
        let account_id = mxr_core::AccountId::new();
        {
            let store = store_at_version(&db_path, 78).await;
            sqlx::query("INSERT INTO accounts (id,name,email,created_at,updated_at) VALUES (?, 'Fixture','fixture@example.com',1,1)")
                .bind(account_id.as_str()).execute(store.writer()).await.unwrap();
            sqlx::query("INSERT INTO drafts (id,account_id,subject,body_markdown,attachments,created_at,updated_at,revision) VALUES (?,?,'Kept draft','Kept body','[\"/legacy/a.txt\"]',1,2,3)")
                .bind(draft_id.as_str()).bind(account_id.as_str()).execute(store.writer()).await.unwrap();
            sqlx::query("INSERT INTO personal_categories (id,name,normalized_name,created_at,updated_at) VALUES ('kept','Kept','kept',1,1)")
                .execute(store.writer()).await.unwrap();
            sqlx::query("INSERT INTO draft_send_outcomes (operation_id,draft_id,account_id,reviewed_revision,command_hash,state,updated_at) VALUES ('kept-op',?,?,3,'kept-hash','unknown',2)")
                .bind(draft_id.as_str()).bind(account_id.as_str()).execute(store.writer()).await.unwrap();
            store.writer().close().await;
        }
        for partially_applied in [false, true] {
            let store = Store::new(&db_path).await.unwrap();
            let draft = store.get_draft(&draft_id).await.unwrap().unwrap();
            assert_eq!(
                draft.attachments,
                vec![std::path::PathBuf::from("/legacy/a.txt")]
            );
            assert_eq!(draft.managed_attachments, None);
            assert_eq!(draft.revision, Some(3));
            assert_eq!(draft.content.markdown_source(), Some("Kept body"));
            assert!(test_table_exists(store.writer(), "draft_attachment_resources").await);
            let stamps: (i64,) =
                sqlx::query_as("SELECT COUNT(*) FROM schema_migrations WHERE version IN (78,79)")
                    .fetch_one(store.writer())
                    .await
                    .unwrap();
            assert_eq!(stamps.0, 2);
            let category: (String,) =
                sqlx::query_as("SELECT name FROM personal_categories WHERE id='kept'")
                    .fetch_one(store.reader())
                    .await
                    .unwrap();
            assert_eq!(category.0, "Kept");
            let outcome: (String, String) = sqlx::query_as(
                "SELECT state,command_hash FROM draft_send_outcomes WHERE operation_id='kept-op'",
            )
            .fetch_one(store.reader())
            .await
            .unwrap();
            assert_eq!(outcome, ("unknown".into(), "kept-hash".into()));
            if !partially_applied {
                sqlx::query("DELETE FROM schema_migrations WHERE version=79")
                    .execute(store.writer())
                    .await
                    .unwrap();
                sqlx::query("DROP TABLE draft_attachment_resources")
                    .execute(store.writer())
                    .await
                    .unwrap();
            }
            store.reader().close().await;
            store.writer().close().await;
        }
    }

    #[tokio::test]
    async fn category_rule_sources_upgrade_actual_version_77_and_legacy_undo() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("c1-77.db");
        let account = crate::test_fixtures::test_account();
        let message = crate::test_fixtures::TestEnvelopeBuilder::new()
            .account_id(account.id.clone())
            .build();
        let category = mxr_core::CategoryId::new();
        {
            let store = store_at_version(&path, 77).await;
            store.insert_account(&account).await.unwrap();
            store.upsert_envelope(&message).await.unwrap();
            sqlx::query("INSERT INTO personal_categories(id,name,normalized_name,created_at,updated_at) VALUES(?,'Travel','travel',1,1)").bind(category.as_str()).execute(store.writer()).await.unwrap();
            sqlx::query("INSERT INTO message_category_memberships(message_id,category_id,assigned,revision,updated_at) VALUES(?,?,0,2,1)").bind(message.id.as_str()).bind(category.as_str()).execute(store.writer()).await.unwrap();
            let snapshot = serde_json::json!([{"message_id":message.id,"category_id":category,"prior_assigned":true,"after_revision":2}]);
            sqlx::query(
                "INSERT INTO category_membership_undo(id,snapshot,expires_at) VALUES('legacy',?,?)",
            )
            .bind(snapshot.to_string())
            .bind(chrono::Utc::now().timestamp() + 60)
            .execute(store.writer())
            .await
            .unwrap();
        }
        let store = Store::new(&path).await.unwrap();
        let before = store
            .category_membership_versions(
                None,
                std::slice::from_ref(&message.id),
                std::slice::from_ref(&category),
            )
            .await
            .unwrap();
        assert_eq!(before[0].manual_decision, Some(false));
        assert!(!before[0].assigned);
        assert_eq!(
            store.undo_category_membership("legacy").await.unwrap(),
            (1, 0)
        );
        let after = store
            .category_membership_versions(
                None,
                std::slice::from_ref(&message.id),
                std::slice::from_ref(&category),
            )
            .await
            .unwrap();
        assert_eq!(after[0].manual_decision, Some(true));
        assert!(after[0].assigned);
        assert!(store.category_rule_sources().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn categories_migration_upgrades_version_76_without_losing_user_state() {
        let temp = tempdir().unwrap();
        let db_path = temp.path().join("version-76.db");

        {
            let store = store_at_version(&db_path, 76).await;

            sqlx::query(
                "INSERT INTO archive_workspace (singleton, workspace_id, recovery_generation)
                 VALUES (1, 'kept-workspace', 'kept-generation')",
            )
            .execute(store.writer())
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO accounts (id, name, email, created_at, updated_at)
                 VALUES ('version-76-account', 'Personal', 'me@example.com', 1, 1)",
            )
            .execute(store.writer())
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO draft_send_outcomes
                 (operation_id, draft_id, account_id, reviewed_revision, command_hash,
                  state, provider_message_id, updated_at)
                 VALUES ('kept-op', 'kept-draft', 'version-76-account', 3, 'kept-hash',
                         'accepted', 'provider-message-keep', 42)",
            )
            .execute(store.writer())
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO todos
                 (id, account_id, kind, verb, title, origin, reason, rules_version,
                  dedup_key, created_at, updated_at, scheduled_for, user_edited)
                 VALUES ('kept-task', 'version-76-account', 'renewal', 'renew',
                         'Keep this task', 'manual', 'User-created task', 1,
                         'kept-task-key', 10, 11, 1800000000, 1)",
            )
            .execute(store.writer())
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO record_renewal_cycles (account_id, record_id, dedup_key)
                 VALUES ('version-76-account', 'kept-record', 'kept-cycle-key')",
            )
            .execute(store.writer())
            .await
            .unwrap();

            let latest_version: (i64,) =
                sqlx::query_as("SELECT MAX(version) FROM schema_migrations")
                    .fetch_one(store.writer())
                    .await
                    .unwrap();
            assert_eq!(latest_version.0, 76);
        }

        let store = Store::new(&db_path).await.unwrap();
        let applied: (String,) =
            sqlx::query_as("SELECT name FROM schema_migrations WHERE version = 77")
                .fetch_one(store.writer())
                .await
                .unwrap();
        assert_eq!(applied.0, "personal_categories");
        assert!(test_table_exists(store.writer(), "personal_categories").await);
        assert!(test_table_exists(store.writer(), "message_category_memberships").await);
        assert!(test_table_exists(store.writer(), "category_membership_undo").await);

        let archive: (String, String) = sqlx::query_as(
            "SELECT workspace_id, recovery_generation FROM archive_workspace WHERE singleton = 1",
        )
        .fetch_one(store.writer())
        .await
        .unwrap();
        assert_eq!(archive, ("kept-workspace".into(), "kept-generation".into()));
        let outcome: (String, String) = sqlx::query_as(
            "SELECT operation_id, provider_message_id FROM draft_send_outcomes WHERE operation_id = 'kept-op'",
        )
        .fetch_one(store.writer())
        .await
        .unwrap();
        assert_eq!(outcome, ("kept-op".into(), "provider-message-keep".into()));
        let task: (String, String) =
            sqlx::query_as("SELECT title, dedup_key FROM todos WHERE id = 'kept-task'")
                .fetch_one(store.writer())
                .await
                .unwrap();
        assert_eq!(task, ("Keep this task".into(), "kept-task-key".into()));
        let cycle: (String,) = sqlx::query_as(
            "SELECT dedup_key FROM record_renewal_cycles
             WHERE account_id = 'version-76-account' AND record_id = 'kept-record'",
        )
        .fetch_one(store.writer())
        .await
        .unwrap();
        assert_eq!(cycle.0, "kept-cycle-key");
    }

    #[tokio::test]
    async fn todo_resolution_migration_upgrades_version_77_without_rewriting_tasks() {
        let temp = tempdir().unwrap();
        let db_path = temp.path().join("version-77.db");

        {
            let store = store_at_version(&db_path, 77).await;
            sqlx::query("INSERT INTO personal_categories (id, name, normalized_name, created_at, updated_at) VALUES ('kept-category', 'Travel', 'travel', 10, 11)")
                .execute(store.writer()).await.unwrap();
            sqlx::query(
                "INSERT INTO accounts (id, name, email, created_at, updated_at)
                 VALUES ('version-77-account', 'Personal', 'me@example.com', 1, 1)",
            )
            .execute(store.writer())
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO todos
                 (id, account_id, kind, verb, title, origin, reason, rules_version,
                  dedup_key, created_at, updated_at, scheduled_for, user_edited,
                  due_words, field_sources)
                 VALUES ('manual-renewal', 'version-77-account', 'renewal', 'renew',
                         'Call about renewal', 'manual', 'Created by the user', 1,
                         'manual-renewal-key', 10, 11, 1800000000, 1,
                         'before 1 Nov', '{\"due_at\":\"user\"}')",
            )
            .execute(store.writer())
            .await
            .unwrap();
            let version: (i64,) = sqlx::query_as("SELECT MAX(version) FROM schema_migrations")
                .fetch_one(store.writer())
                .await
                .unwrap();
            assert_eq!(version.0, 77);
        }

        let store = Store::new(&db_path).await.unwrap();
        let version: (i64,) = sqlx::query_as(
            "SELECT version FROM schema_migrations WHERE name = 'todo_resolution_suggestions'",
        )
        .fetch_one(store.writer())
        .await
        .unwrap();
        assert_eq!(version.0, 78);
        let category: (String, String) =
            sqlx::query_as("SELECT id, name FROM personal_categories WHERE id = 'kept-category'")
                .fetch_one(store.writer())
                .await
                .unwrap();
        assert_eq!(category, ("kept-category".into(), "Travel".into()));
        assert!(test_table_exists(store.writer(), "todo_resolution_suggestions").await);
        assert!(test_table_exists(store.writer(), "todo_resolution_evidence").await);
        let preserved: (String, String, String, String, i64, i64) = sqlx::query_as(
            "SELECT title, origin, reason, due_words, scheduled_for, user_edited
             FROM todos WHERE id = 'manual-renewal'",
        )
        .fetch_one(store.writer())
        .await
        .unwrap();
        assert_eq!(
            preserved,
            (
                "Call about renewal".into(),
                "manual".into(),
                "Created by the user".into(),
                "before 1 Nov".into(),
                1_800_000_000,
                1,
            )
        );
    }

    #[tokio::test]
    async fn marginalia_migration_upgrades_83_and_85_keeping_highlights_as_made_in_mxr() {
        // 83 also crosses the two remote migrations that precede it on main.
        for from in [83, 85] {
            let temp = tempdir().unwrap();
            let db_path = temp.path().join(format!("version-{from}.db"));
            {
                let store = store_at_version(&db_path, from).await;
                // Only the upgrade is under test, so skip the parent rows.
                sqlx::query("PRAGMA foreign_keys = OFF")
                    .execute(store.writer())
                    .await
                    .unwrap();
                sqlx::query(
                    "INSERT INTO reading_highlights
                     (id, account_id, message_id, idx, view, quote, note, created_at)
                     VALUES ('h1', 'a', 'm', 0, 'issue', 'kept quote', NULL, 1)",
                )
                .execute(store.writer())
                .await
                .unwrap();
            }

            let store = Store::new(&db_path).await.unwrap();
            let versions: Vec<(i64,)> = sqlx::query_as(
                "SELECT version FROM schema_migrations WHERE version >= 83 ORDER BY version",
            )
            .fetch_all(store.writer())
            .await
            .unwrap();
            assert_eq!(
                versions.iter().map(|v| v.0).collect::<Vec<_>>(),
                (83..=92).collect::<Vec<_>>(),
                "from {from}"
            );
            assert!(test_table_exists(store.writer(), "reading_marginalia").await);
            let row: (String, String, Option<String>) = sqlx::query_as(
                "SELECT quote, origin, origin_id FROM reading_highlights WHERE id = 'h1'",
            )
            .fetch_one(store.writer())
            .await
            .unwrap();
            assert_eq!(
                row,
                ("kept quote".into(), "mxr".into(), None),
                "from {from}"
            );
        }
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

    async fn seed_decision_log_migration_034_crash_state(
        db_path: &Path,
        drop_original: bool,
    ) -> (String, String) {
        {
            let store = Store::new(db_path).await.unwrap();
            let account = mxr_core::Account {
                id: mxr_core::AccountId::new(),
                name: "Test".into(),
                email: "me@example.com".into(),
                sync_backend: None,
                send_backend: None,
                enabled: true,
            };
            store.insert_account(&account).await.unwrap();
            sqlx::query(
                r#"INSERT INTO decision_log
                   (id, account_id, thread_id, topic, decision, rationale,
                    evidence_msg_ids, decided_at, extracted_at, source_hash)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            )
            .bind("decision-1")
            .bind(account.id.as_str())
            .bind(mxr_core::ThreadId::new().as_str())
            .bind("launch")
            .bind("Ship GA")
            .bind("risk accepted")
            .bind("[]")
            .bind(chrono::Utc::now().timestamp())
            .bind(chrono::Utc::now().timestamp())
            .bind("source-v1")
            .execute(store.writer())
            .await
            .unwrap();
        }

        let pool = open_test_writer(db_path).await;
        sqlx::query("DELETE FROM schema_migrations WHERE version = 34")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::raw_sql(
            r#"DROP TABLE IF EXISTS decision_log_v2;
               CREATE TABLE decision_log_v2 (
                   id                  TEXT PRIMARY KEY,
                   account_id          TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
                   thread_id           TEXT NOT NULL,
                   topic               TEXT,
                   decision            TEXT NOT NULL,
                   rationale           TEXT,
                   evidence_msg_ids    TEXT NOT NULL,
                   decided_at          INTEGER,
                   extracted_at        INTEGER NOT NULL,
                   source_hash         TEXT NOT NULL
               );
               INSERT OR REPLACE INTO decision_log_v2
                   (id, account_id, thread_id, topic, decision, rationale,
                    evidence_msg_ids, decided_at, extracted_at, source_hash)
               SELECT
                   id, account_id, thread_id, topic, decision, rationale,
                   evidence_msg_ids, decided_at, extracted_at, source_hash
               FROM decision_log;"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        if drop_original {
            sqlx::raw_sql("DROP TABLE decision_log")
                .execute(&pool)
                .await
                .unwrap();
        }
        pool.close().await;
        ("decision-1".into(), "Ship GA".into())
    }

    #[tokio::test]
    async fn migration_034_recovers_when_temp_table_exists_before_schema_stamp() {
        let temp = tempdir().unwrap();
        let db_path = temp.path().join("decision-log-v2-crash.db");
        let (decision_id, decision) =
            seed_decision_log_migration_034_crash_state(&db_path, false).await;

        let store = Store::new(&db_path)
            .await
            .expect("migration 034 should recover a leftover decision_log_v2 table");

        let row: (String,) = sqlx::query_as("SELECT decision FROM decision_log WHERE id = ?")
            .bind(&decision_id)
            .fetch_one(store.reader())
            .await
            .unwrap();
        assert_eq!(row.0, decision);
        assert!(
            !test_table_exists(store.reader(), "decision_log_v2").await,
            "recovery must clean up the stale temp table"
        );
        assert!(store.is_migration_applied(34).await.unwrap());
    }

    #[tokio::test]
    async fn migration_034_recovers_when_original_was_dropped_before_schema_stamp() {
        let temp = tempdir().unwrap();
        let db_path = temp.path().join("decision-log-dropped-crash.db");
        let (decision_id, decision) =
            seed_decision_log_migration_034_crash_state(&db_path, true).await;

        let store = Store::new(&db_path)
            .await
            .expect("migration 034 should recover by promoting decision_log_v2");

        let row: (String,) = sqlx::query_as("SELECT decision FROM decision_log WHERE id = ?")
            .bind(&decision_id)
            .fetch_one(store.reader())
            .await
            .unwrap();
        assert_eq!(row.0, decision);
        assert!(
            !test_table_exists(store.reader(), "decision_log_v2").await,
            "recovery must not leave the temp table after promotion"
        );
        assert!(store.is_migration_applied(34).await.unwrap());
    }
}
