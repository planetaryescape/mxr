//! Activity recorder. Single seam between the IPC dispatcher and the
//! `user_activity` table. See `docs/activity-log.md`.
//!
//! Design tenets:
//! - Fire-and-forget: dispatcher hands an entry off; the actual SQLite
//!   write happens on a worker task and never blocks the IPC response.
//! - Bounded mpsc with `try_send` so a pathological backpressure scenario
//!   drops the entry with a `warn!` rather than freezing the daemon.
//! - Pause is a runtime flag observed inside the worker. Pause/resume
//!   markers go through a separate `ForceRecord` variant so they land in
//!   the log even when paused (otherwise users would never know it took
//!   effect).
//! - `MXR_ACTIVITY=off` disables the recorder entirely. Set on daemon
//!   startup; honored for the lifetime of the process.

pub mod mapper;
pub mod tier;

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;

use mxr_protocol::ClientKind;
use mxr_store::{ActivityInsert, Store, Tier};
use tokio::sync::mpsc;

/// Owned form of an activity entry. The mapper produces these; the
/// recorder owns them through the channel and hands borrowed
/// `ActivityInsert` to the store at write time.
#[derive(Debug, Clone)]
pub struct OwnedEntry {
    pub ts: i64,
    pub account_id: Option<String>,
    pub source: ClientKind,
    pub action: String,
    pub target_kind: Option<String>,
    pub target_id: Option<String>,
    pub tier: Tier,
    pub context: Option<serde_json::Value>,
}

/// Message types crossing the recorder channel. `ForceRecord` is for
/// pause/resume markers that must land even when the recorder is paused.
#[derive(Debug)]
enum RecorderMessage {
    Record(OwnedEntry),
    ForceRecord(OwnedEntry),
    Pause { until: Option<i64> },
    Resume,
}

/// Handle to the recorder. Cheap to clone (everything inside is `Arc`).
#[derive(Clone)]
pub struct Recorder {
    inner: Arc<Inner>,
}

struct Inner {
    tx: mpsc::Sender<RecorderMessage>,
    paused: AtomicBool,
    paused_until: AtomicI64,
    enabled: bool,
}

impl Recorder {
    /// Spawn the worker. `MXR_ACTIVITY=off` disables the recorder so all
    /// `record` calls become no-ops.
    pub fn spawn(store: Arc<Store>) -> Self {
        let enabled = std::env::var("MXR_ACTIVITY").as_deref() != Ok("off");
        let (tx, rx) = mpsc::channel::<RecorderMessage>(1024);
        let paused = AtomicBool::new(false);
        let paused_until = AtomicI64::new(0);
        let inner = Arc::new(Inner {
            tx,
            paused,
            paused_until,
            enabled,
        });
        if enabled {
            tokio::spawn(worker_loop(rx, store, inner.clone()));
        } else {
            tracing::info!("activity recorder disabled via MXR_ACTIVITY=off");
            // Drain the channel quietly so try_send doesn't pile up.
            tokio::spawn(drain_loop(rx));
        }
        Self { inner }
    }

    /// Non-blocking submit. Dropped on backpressure (channel full) with a
    /// `warn!` log so overload is observable. A failed send is *never*
    /// propagated to the user-facing IPC response — activity is
    /// observability, not correctness.
    pub fn record(&self, entry: OwnedEntry) {
        if !self.inner.enabled {
            return;
        }
        match self.inner.tx.try_send(RecorderMessage::Record(entry)) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                tracing::warn!("activity recorder backpressure; dropping entry");
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                tracing::warn!("activity recorder channel closed; dropping entry");
            }
        }
    }

    /// Force-record an entry even when the recorder is paused. Used for
    /// `activity.paused` / `activity.resumed` markers so a user can see
    /// when activity was suspended without the suspension hiding itself.
    pub fn record_forced(&self, entry: OwnedEntry) {
        if !self.inner.enabled {
            return;
        }
        if let Err(err) = self.inner.tx.try_send(RecorderMessage::ForceRecord(entry)) {
            tracing::warn!(error = %err, "activity recorder force-record failed");
        }
    }

    /// Pause recording. Until clears the optional auto-resume timestamp
    /// (unix ms). `None` means indefinite.
    pub fn pause(&self, until: Option<i64>) {
        let _ = self.inner.tx.try_send(RecorderMessage::Pause { until });
    }

    pub fn resume(&self) {
        let _ = self.inner.tx.try_send(RecorderMessage::Resume);
    }

    /// Snapshot of paused state. Returns `(paused, paused_until_ms)`. The
    /// values are not transactional with the worker — they reflect the
    /// last-known state and may lag by one channel hop.
    pub fn pause_status(&self) -> (bool, Option<i64>) {
        let paused = self.inner.paused.load(Ordering::SeqCst);
        let until = self.inner.paused_until.load(Ordering::SeqCst);
        let until = if until == 0 { None } else { Some(until) };
        (paused, until)
    }

    pub fn is_enabled(&self) -> bool {
        self.inner.enabled
    }
}

/// In-memory cache key for the compaction window. `(account_id, action, target_id)`
/// triples are coalesced if the new row arrives within `COALESCE_WINDOW_MS`
/// of the cached entry. Cache holds the row's id, last-seen ts, and current
/// count so we can issue an UPDATE without re-reading the DB.
type CompactionKey = (Option<String>, String, Option<String>);

const COALESCE_WINDOW_MS: i64 = 250;
const COALESCE_CACHE_SIZE: usize = 32;

struct CompactionEntry {
    id: i64,
    ts: i64,
    count: u64,
}

async fn worker_loop(
    mut rx: mpsc::Receiver<RecorderMessage>,
    store: Arc<Store>,
    inner: Arc<Inner>,
) {
    let mut cache: std::collections::HashMap<CompactionKey, CompactionEntry> =
        std::collections::HashMap::new();

    while let Some(msg) = rx.recv().await {
        match msg {
            RecorderMessage::Record(entry) => {
                let now = current_unix_ms();
                let paused = inner.paused.load(Ordering::SeqCst);
                let until = inner.paused_until.load(Ordering::SeqCst);
                if paused {
                    if until != 0 && now >= until {
                        // Auto-resume window elapsed.
                        inner.paused.store(false, Ordering::SeqCst);
                        inner.paused_until.store(0, Ordering::SeqCst);
                        // Synthesize a resumed marker so the user can see it.
                        write_entry(
                            &store,
                            OwnedEntry {
                                ts: now,
                                account_id: None,
                                source: ClientKind::Daemon,
                                action: "activity.resumed".into(),
                                target_kind: None,
                                target_id: None,
                                tier: Tier::Important,
                                context: Some(
                                    serde_json::json!({ "auto": true, "reason": "auto_resume" }),
                                ),
                            },
                        )
                        .await;
                        // Fall through and record this one too.
                    } else {
                        // Still paused → drop the entry silently. This is
                        // the documented contract: pause means no new
                        // rows from user-driven activity.
                        continue;
                    }
                }
                // Phase 9 compaction: only ephemeral/standard tier rows
                // are eligible for write-time coalescing. Important-tier
                // mutations are always written as-is to preserve audit
                // fidelity.
                let coalesce_eligible =
                    matches!(entry.tier, Tier::Ephemeral | Tier::Standard);
                if coalesce_eligible {
                    let key: CompactionKey = (
                        entry.account_id.clone(),
                        entry.action.clone(),
                        entry.target_id.clone(),
                    );
                    if let Some(cached) = cache.get_mut(&key) {
                        if entry.ts - cached.ts <= COALESCE_WINDOW_MS {
                            cached.count += 1;
                            cached.ts = entry.ts;
                            if let Err(err) = store
                                .coalesce_activity(cached.id, cached.ts, cached.count)
                                .await
                            {
                                tracing::warn!(error = %err, "activity coalesce failed");
                            }
                            continue;
                        }
                    }
                    // Cache miss / expired: insert fresh and update cache.
                    match write_entry_returning_id(&store, entry).await {
                        Some(id) => {
                            if cache.len() >= COALESCE_CACHE_SIZE {
                                // Drop the oldest entry. HashMap doesn't preserve
                                // order, so we evict by lowest `ts`. Cheap because
                                // the cache is bounded.
                                if let Some(oldest) =
                                    cache.iter().min_by_key(|(_, v)| v.ts).map(|(k, _)| k.clone())
                                {
                                    cache.remove(&oldest);
                                }
                            }
                            cache.insert(
                                key,
                                CompactionEntry {
                                    id,
                                    ts: now,
                                    count: 1,
                                },
                            );
                        }
                        None => { /* write failed; warn already logged */ }
                    }
                } else {
                    write_entry(&store, entry).await;
                }
            }
            RecorderMessage::ForceRecord(entry) => write_entry(&store, entry).await,
            RecorderMessage::Pause { until } => {
                inner.paused.store(true, Ordering::SeqCst);
                inner
                    .paused_until
                    .store(until.unwrap_or(0), Ordering::SeqCst);
            }
            RecorderMessage::Resume => {
                inner.paused.store(false, Ordering::SeqCst);
                inner.paused_until.store(0, Ordering::SeqCst);
            }
        }
    }
}

async fn write_entry_returning_id(store: &Store, e: OwnedEntry) -> Option<i64> {
    let insert = mxr_store::ActivityInsert {
        ts: e.ts,
        account_id: e.account_id.as_deref(),
        source: e.source.as_str(),
        action: &e.action,
        target_kind: e.target_kind.as_deref(),
        target_id: e.target_id.as_deref(),
        tier: e.tier,
        context: e.context.as_ref(),
    };
    match store.record_activity(insert).await {
        Ok(id) => Some(id),
        Err(err) => {
            tracing::warn!(error = %err, action = %e.action, "activity write failed");
            None
        }
    }
}

async fn drain_loop(mut rx: mpsc::Receiver<RecorderMessage>) {
    while rx.recv().await.is_some() {}
}

async fn write_entry(store: &Store, e: OwnedEntry) {
    let insert = ActivityInsert {
        ts: e.ts,
        account_id: e.account_id.as_deref(),
        source: e.source.as_str(),
        action: &e.action,
        target_kind: e.target_kind.as_deref(),
        target_id: e.target_id.as_deref(),
        tier: e.tier,
        context: e.context.as_ref(),
    };
    if let Err(err) = store.record_activity(insert).await {
        tracing::warn!(error = %err, action = %e.action, "activity write failed");
    }
}

pub fn current_unix_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use mxr_store::ActivityFilter;
    use std::time::Duration;

    async fn fresh_recorder() -> (Recorder, Arc<Store>) {
        let store = Arc::new(Store::in_memory().await.unwrap());
        let rec = Recorder::spawn(store.clone());
        (rec, store)
    }

    fn entry(action: &str) -> OwnedEntry {
        OwnedEntry {
            ts: current_unix_ms(),
            account_id: None,
            source: ClientKind::Tui,
            action: action.into(),
            target_kind: None,
            target_id: None,
            tier: Tier::Important,
            context: None,
        }
    }

    // The worker is async; let it drain.
    async fn drain() {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    #[tokio::test]
    async fn record_lands_a_row_in_the_store() {
        let (rec, store) = fresh_recorder().await;
        rec.record(entry("mail.archive"));
        drain().await;

        let page = store
            .list_activity(&ActivityFilter::default(), 10, None)
            .await
            .unwrap();
        assert_eq!(page.rows.len(), 1);
        assert_eq!(page.rows[0].action, "mail.archive");
        assert_eq!(page.rows[0].source, "tui");
    }

    #[tokio::test]
    async fn pause_then_record_drops_entries_until_resume() {
        let (rec, store) = fresh_recorder().await;
        rec.pause(None);
        drain().await;
        rec.record(entry("mail.archive"));
        rec.record(entry("mail.read"));
        drain().await;

        let before = store
            .list_activity(&ActivityFilter::default(), 10, None)
            .await
            .unwrap();
        assert!(before.rows.is_empty(), "paused recorder drops new entries");

        rec.resume();
        drain().await;
        rec.record(entry("mail.send"));
        drain().await;

        let after = store
            .list_activity(&ActivityFilter::default(), 10, None)
            .await
            .unwrap();
        let actions: Vec<&str> = after.rows.iter().map(|r| r.action.as_str()).collect();
        assert_eq!(actions, vec!["mail.send"]);
    }

    #[tokio::test]
    async fn force_record_lands_even_when_paused() {
        let (rec, store) = fresh_recorder().await;
        rec.pause(None);
        drain().await;

        rec.record_forced(entry("activity.paused"));
        drain().await;

        let page = store
            .list_activity(&ActivityFilter::default(), 10, None)
            .await
            .unwrap();
        assert_eq!(page.rows.len(), 1);
        assert_eq!(page.rows[0].action, "activity.paused");
    }

    #[tokio::test]
    async fn rapid_fire_ephemeral_writes_coalesce_into_one_row_with_count() {
        let (rec, store) = fresh_recorder().await;
        // Three ephemeral records for the same (action, target) inside the window.
        let make = |ts: i64| OwnedEntry {
            ts,
            account_id: None,
            source: ClientKind::Tui,
            action: "view.open_screen".into(),
            target_kind: None,
            target_id: None,
            tier: Tier::Ephemeral,
            context: None,
        };
        let base = current_unix_ms();
        rec.record(make(base));
        rec.record(make(base + 50));
        rec.record(make(base + 100));
        drain().await;

        let page = store
            .list_activity(&ActivityFilter::default(), 10, None)
            .await
            .unwrap();
        assert_eq!(page.rows.len(), 1, "three writes coalesced into one row");
        let ctx: serde_json::Value =
            serde_json::from_str(page.rows[0].context_json.as_ref().unwrap()).unwrap();
        assert_eq!(ctx["count"], 3);
    }

    #[tokio::test]
    async fn important_tier_writes_never_coalesce() {
        let (rec, store) = fresh_recorder().await;
        let make = |ts: i64| OwnedEntry {
            ts,
            account_id: None,
            source: ClientKind::Tui,
            action: "mail.send".into(),
            target_kind: Some("draft".into()),
            target_id: Some("d_1".into()),
            tier: Tier::Important,
            context: None,
        };
        let base = current_unix_ms();
        rec.record(make(base));
        rec.record(make(base + 50));
        rec.record(make(base + 100));
        drain().await;

        let page = store
            .list_activity(&ActivityFilter::default(), 10, None)
            .await
            .unwrap();
        assert_eq!(
            page.rows.len(),
            3,
            "important mutations preserved verbatim"
        );
    }

    #[tokio::test]
    async fn auto_resume_kicks_in_when_paused_until_elapses() {
        let (rec, store) = fresh_recorder().await;
        // Pause "until 1 ms in the past" — the next record() triggers the auto-resume branch.
        let now = current_unix_ms();
        rec.pause(Some(now - 1));
        drain().await;

        rec.record(entry("mail.send"));
        drain().await;

        // Two rows expected: the synthesized activity.resumed marker (Daemon source)
        // and the mail.send that flowed through after auto-resume.
        let page = store
            .list_activity(&ActivityFilter::default(), 10, None)
            .await
            .unwrap();
        let actions: Vec<&str> = page.rows.iter().map(|r| r.action.as_str()).collect();
        // Newest first; mail.send recorded after the resumed marker.
        assert!(actions.contains(&"activity.resumed"));
        assert!(actions.contains(&"mail.send"));
    }
}
