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
        let sql = include_str!("../migrations/001_initial.sql");
        sqlx::raw_sql(sql).execute(&self.writer).await?;
        Ok(())
    }

    pub fn writer(&self) -> &SqlitePool {
        &self.writer
    }

    pub fn reader(&self) -> &SqlitePool {
        &self.reader
    }
}
