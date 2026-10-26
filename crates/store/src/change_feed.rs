//! Commits that can change what a mail view shows, counted on the writer
//! connection so cached views know when to rebuild.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use sqlx::sqlite::SqliteConnection;

/// Tables whose rows never change a mail view: logs, sync bookkeeping,
/// search and semantic indexes, the activity log, and caches the views fill
/// themselves. Every other table counts, so a new table can only cause an
/// extra rebuild, never a stale view.
const IGNORED_TABLES: &[&str] = &[
    "event_log",
    "sync_log",
    "sync_runtime_status",
    "provider_meta",
    "rule_execution_log",
    "mutation_dedup_log",
    "schema_migrations",
    "search_reindex_pending",
    "semantic_chunks",
    "semantic_embeddings",
    "semantic_profiles",
    "update_facts",
    "conversation_summaries",
    "conversation_view_inputs",
    "conversation_facet_clock",
    "conversation_candidate_sets",
    "assessment_inputs",
    "assessment_thread_inputs",
    "assessment_sender_inputs",
    "history_text_fingerprints",
    "assistant_conversations",
    "assistant_messages",
    "saved_activity_filters",
];

/// FTS shadow tables carry their parent's name as a prefix.
const IGNORED_PREFIXES: &[&str] = &["messages_fts", "user_activity"];

fn ignored(table: &str) -> bool {
    IGNORED_TABLES.contains(&table)
        || IGNORED_PREFIXES
            .iter()
            .any(|prefix| table.starts_with(prefix))
}

/// A count of committed transactions that touched a table a view reads.
#[derive(Debug)]
pub struct ChangeFeed {
    seq: AtomicU64,
    /// Milliseconds after `started` of the latest counted commit.
    last_ms: AtomicU64,
    started: Instant,
}

impl Default for ChangeFeed {
    fn default() -> Self {
        Self {
            seq: AtomicU64::new(0),
            last_ms: AtomicU64::new(0),
            started: Instant::now(),
        }
    }
}

impl ChangeFeed {
    /// Moves on with every counted commit.
    pub fn seq(&self) -> u64 {
        self.seq.load(Ordering::Acquire)
    }

    /// Time since the latest counted commit (since start when none).
    ///
    /// SQLite's commit hook runs just before the commit lands, so a reader
    /// should wait until the feed has been quiet briefly before trusting that
    /// it sees the change.
    pub fn quiet_for(&self) -> Duration {
        let last = Duration::from_millis(self.last_ms.load(Ordering::Acquire));
        self.started.elapsed().saturating_sub(last)
    }

    fn record(&self) {
        let now = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.last_ms.store(now, Ordering::Release);
        self.seq.fetch_add(1, Ordering::AcqRel);
    }

    /// Count this connection's commits that touched a counted table.
    pub(crate) async fn watch(
        self: &Arc<Self>,
        connection: &mut SqliteConnection,
    ) -> Result<(), sqlx::Error> {
        let touched = Arc::new(AtomicBool::new(false));
        let mut handle = connection.lock_handle().await?;
        let on_update = Arc::clone(&touched);
        handle.set_update_hook(move |change| {
            if !ignored(change.table) {
                on_update.store(true, Ordering::Release);
            }
        });
        let on_commit = Arc::clone(&touched);
        let feed = Arc::clone(self);
        handle.set_commit_hook(move || {
            if on_commit.swap(false, Ordering::AcqRel) {
                feed.record();
            }
            true
        });
        handle.set_rollback_hook(move || touched.store(false, Ordering::Release));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::test_fixtures::*;
    use crate::Store;

    /// Sync rewrites every label and its counts, and saves its cursor, on
    /// every pass. Unchanged values must not count, or an idle daemon
    /// rebuilds every cached view after each sync.
    #[tokio::test]
    async fn repeating_unchanged_sync_writes_counts_nothing() {
        use mxr_core::id::LabelId;
        use mxr_core::types::{Label, LabelKind, SyncCursor};

        let store = Store::in_memory().await.unwrap();
        let account = test_account();
        store.insert_account(&account).await.unwrap();
        let mut label = Label {
            id: LabelId::from_scoped_provider_id(&account.id, "fake", "INBOX"),
            account_id: account.id.clone(),
            name: "Inbox".into(),
            kind: LabelKind::System,
            color: None,
            provider_id: "INBOX".into(),
            unread_count: 0,
            total_count: 0,
            role: None,
        };
        let cursor = SyncCursor::from_bytes(br#"{"v":"1","history_id":42}"#.to_vec());
        store.upsert_label(&label).await.unwrap();
        store.recalculate_label_counts(&account.id).await.unwrap();
        store.set_sync_cursor(&account.id, &cursor).await.unwrap();

        let feed = store.change_feed();
        let settled = feed.seq();
        store.upsert_label(&label).await.unwrap();
        store.recalculate_label_counts(&account.id).await.unwrap();
        store.set_sync_cursor(&account.id, &cursor).await.unwrap();
        assert_eq!(feed.seq(), settled, "nothing changed");

        label.name = "Inbox (renamed)".into();
        store.upsert_label(&label).await.unwrap();
        assert!(feed.seq() > settled, "a renamed label is a change");
        let renamed = feed.seq();
        let moved = SyncCursor::from_bytes(br#"{"v":"1","history_id":43}"#.to_vec());
        store.set_sync_cursor(&account.id, &moved).await.unwrap();
        assert!(feed.seq() > renamed, "a moved cursor is a write");
        assert_eq!(
            store
                .list_sync_cursors()
                .await
                .unwrap()
                .get(&account.id)
                .map(SyncCursor::as_bytes),
            Some(moved.as_bytes())
        );
    }

    /// A transaction holds the writer through its commit, so the landed
    /// count waits for it, and then includes it. Needs a file-backed store:
    /// the in-memory one shares a single connection for reads and writes.
    #[tokio::test]
    async fn the_landed_count_waits_for_a_commit_in_flight() {
        use std::time::Duration;

        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(&dir.path().join("store.db")).await.unwrap();
        let before = store
            .changes_landed(Duration::from_secs(5))
            .await
            .expect("an idle writer");

        let mut tx = store.writer().begin().await.unwrap();
        sqlx::query("UPDATE accounts SET name = 'x' WHERE 0")
            .execute(&mut *tx)
            .await
            .unwrap();
        assert_eq!(
            store.changes_landed(Duration::from_millis(50)).await,
            None,
            "a transaction holds the writer"
        );
        tx.rollback().await.unwrap();

        store.insert_account(&test_account()).await.unwrap();
        let after = store
            .changes_landed(Duration::from_secs(5))
            .await
            .expect("the writer is free again");
        assert!(after > before, "the committed insert is in the count");
    }

    #[tokio::test]
    async fn view_tables_count_and_bookkeeping_does_not() {
        let store = Store::in_memory().await.unwrap();
        let feed = store.change_feed();
        let start = feed.seq();

        let log = "INSERT INTO event_log (timestamp, level, category, summary) VALUES (0, 'info', 'sync', 'x')";
        sqlx::query(log).execute(store.writer()).await.unwrap();
        assert_eq!(feed.seq(), start, "the event log is not a view change");

        let account = test_account();
        store.insert_account(&account).await.unwrap();
        assert!(feed.seq() > start, "an account row is a view change");

        let before_rollback = feed.seq();
        let mut tx = store.writer().begin().await.unwrap();
        sqlx::query("DELETE FROM accounts")
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.rollback().await.unwrap();
        let mut tx = store.writer().begin().await.unwrap();
        sqlx::query(log).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(
            feed.seq(),
            before_rollback,
            "a rolled-back change is not carried into the next commit"
        );
    }
}
