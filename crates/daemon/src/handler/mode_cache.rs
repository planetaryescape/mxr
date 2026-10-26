//! Ready-made Now, rail and mode views. Building one reads and classifies
//! every inbox message (about a second on a large mailbox), which is too
//! slow for a launcher. A background pass rebuilds views someone read
//! recently whenever mail changes, so a read usually finds one ready. It
//! refreshes active views before they age out. Otherwise the clients' 60s
//! polling interval meets the cache's 60s expiry and every poll is a cold
//! rebuild. Views nobody reads are dropped instead of refreshed.
//!
//! A view is only served when nothing it could show has changed since it
//! was built:
//! - no request that is not read-only has finished since (any mutation
//!   through the daemon, so your own changes always show);
//! - the store's change feed has not moved since (sync, background loops),
//!   and the build read the count with no write in flight, so it saw every
//!   commit counted (see `Store::changes_landed`);
//! - it is younger than `MAX_AGE` and was built in the current local hour,
//!   because the views word time ("due soon", the evening switch, today).
//!
//! Otherwise the read builds it, as it would without the cache.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Local, Utc};
use mxr_core::AccountId;
use mxr_protocol::{MessagesFilterData, Request, ResponseData};
use parking_lot::Mutex;
use tokio::sync::watch;

use super::{messages, modes, now, reading, updates, HandlerResult};
use crate::state::AppState;

/// Longest a view is served without rebuilding first, so what the views
/// word by the clock (an Updates cut, a to-do coming back, "due soon")
/// is never more than this late.
const MAX_AGE: Duration = Duration::from_secs(60);
const REFRESH_AHEAD: Duration = Duration::from_secs(10);
/// How long a build waits for a write in flight to finish before building
/// anyway. A build that could not wait it out may have missed a commit, so
/// it answers only the read that asked for it and is never served again.
const LAND_WAIT: Duration = Duration::from_millis(50);
/// The refresher rebuilds a view for a change at most this often, which
/// bounds the work if some table is written continuously.
const MIN_REBUILD: Duration = Duration::from_secs(5);
/// Views nobody read for this long are dropped instead of rebuilt. Keeping
/// a view fresh costs a rebuild (about a second on a large mailbox) at every
/// change, so this is the burst a client is in use, not the gap between
/// visits; a client keeps its own last copy for those.
const ACTIVE: Duration = Duration::from_secs(5 * 60);
/// Most views kept. Messages' `limit` and Updates' `cut` come from the
/// client, so the set of keys is otherwise unbounded; the least recently
/// read view goes first.
const MAX_ENTRIES: usize = 16;
const TICK: Duration = Duration::from_millis(500);

/// A cacheable read: one of the mode requests without side effects asked.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ModeView {
    Now {
        account_id: Option<AccountId>,
    },
    Rail {
        account_id: Option<AccountId>,
    },
    Messages {
        account_id: Option<AccountId>,
        /// The whole filter, the legacy top-level turn merged in: each
        /// filter is its own key. Complete, fresh attention snapshots can
        /// also answer narrower attention filters without rebuilding.
        filter: MessagesFilterData,
        limit: u32,
    },
    Updates {
        account_id: Option<AccountId>,
        cut: Option<DateTime<Utc>>,
        expired: bool,
    },
    Reading {
        account_id: Option<AccountId>,
    },
}

impl ModeView {
    /// The view a request reads, when it can be answered from the cache.
    /// Requests that mark something seen or visited write, so they always
    /// run.
    pub(crate) fn of(req: &Request) -> Option<Self> {
        Some(match req {
            Request::GetNow { account_id } => Self::Now {
                account_id: account_id.clone(),
            },
            Request::GetRail { account_id } => Self::Rail {
                account_id: account_id.clone(),
            },
            Request::ListMessages {
                account_id,
                turn,
                filter,
                limit,
            } => Self::Messages {
                account_id: account_id.clone(),
                filter: filter.with_legacy_turn(*turn),
                limit: *limit,
            },
            Request::GetUpdatesDigest {
                account_id,
                cut,
                mark_seen: false,
                expired,
            } => Self::Updates {
                account_id: account_id.clone(),
                cut: *cut,
                expired: *expired,
            },
            Request::GetReadingEdition {
                account_id,
                mark_visit: false,
            } => Self::Reading {
                account_id: account_id.clone(),
            },
            _ => return None,
        })
    }

    async fn build(&self, state: &AppState) -> HandlerResult {
        match self {
            Self::Now { account_id } => Box::pin(now::get_now(state, account_id.as_ref())).await,
            Self::Rail { account_id } => {
                Box::pin(modes::get_rail(state, account_id.as_ref())).await
            }
            Self::Messages {
                account_id,
                filter,
                limit,
            } => messages::list_messages(state, account_id.as_ref(), *filter, *limit).await,
            Self::Updates {
                account_id,
                cut,
                expired,
            } => {
                Box::pin(updates::get_digest(
                    state,
                    account_id.as_ref(),
                    *cut,
                    false,
                    *expired,
                ))
                .await
            }
            Self::Reading { account_id } => {
                let req = Request::GetReadingEdition {
                    account_id: account_id.clone(),
                    mark_visit: false,
                };
                Box::pin(reading::handle(state, None, &req)).await
            }
        }
    }
}

struct Entry {
    view: ModeView,
    data: Option<ResponseData>,
    started: Instant,
    built: Instant,
    /// `ModeCache::mutations` when the build started.
    mutations: u64,
    /// The change feed's sequence when the build started.
    changes: u64,
    /// `changes` was read with no write in flight, so the build saw every
    /// commit it counts; see `Store::changes_landed`.
    settled: bool,
    /// The local hour the build started in; see `local_hour`.
    hour: i64,
    last_read: Instant,
    /// Held while this view builds, so concurrent readers wait for one build.
    building: Arc<tokio::sync::Mutex<()>>,
}

#[derive(Default)]
pub(crate) struct ModeCache {
    entries: Mutex<Vec<Entry>>,
    mutations: AtomicU64,
    /// Tests: pretend a write stays in flight.
    #[cfg(test)]
    write_in_flight: std::sync::atomic::AtomicBool,
}

impl ModeCache {
    /// A request that may have changed mail finished: nothing built before
    /// it may answer a read.
    pub(crate) fn note_mutation(&self) {
        self.mutations.fetch_add(1, Ordering::AcqRel);
    }

    /// The view, from the cache when nothing it shows can have changed,
    /// otherwise built now.
    pub(crate) async fn read(&self, state: &AppState, view: ModeView) -> HandlerResult {
        let arrived = Instant::now();
        let feed = state.store.change_feed();
        let lock = {
            let mut entries = self.entries.lock();
            if let ModeView::Messages {
                account_id, filter, ..
            } = &view
            {
                if matches!(
                    filter.turn,
                    None | Some(mxr_protocol::MessagesTurnData::Mine)
                ) {
                    for entry in entries.iter_mut() {
                        let source = matches!(&entry.view, ModeView::Messages {
                            account_id: scope, filter: base, ..
                        } if scope == account_id && !base.narrows()
                            && matches!(base.turn, None | Some(mxr_protocol::MessagesTurnData::Mine)));
                        if source {
                            if let Some(ResponseData::Messages { messages }) =
                                self.servable(entry, feed.seq())
                            {
                                entry.last_read = Instant::now();
                                return Ok(ResponseData::Messages {
                                    messages: messages::narrow_attention(messages, *filter),
                                });
                            }
                        }
                    }
                }
            }
            let index = match entries.iter().position(|e| e.view == view) {
                Some(index) => index,
                None => {
                    if entries.len() >= MAX_ENTRIES {
                        if let Some(oldest) = entries
                            .iter()
                            .enumerate()
                            .min_by_key(|(_, e)| e.last_read)
                            .map(|(index, _)| index)
                        {
                            entries.swap_remove(oldest);
                        }
                    }
                    entries.push(Entry::new(view.clone()));
                    entries.len() - 1
                }
            };
            let entry = &mut entries[index];
            entry.last_read = Instant::now();
            if let Some(data) = self.servable(entry, feed.seq()) {
                return Ok(data);
            }
            Arc::clone(&entry.building)
        };
        let _building = lock.lock().await;
        // Another reader or the refresher may have built it meanwhile. A
        // build that started after this read arrived is as fresh as one this
        // read would start, even one that could not wait out a write, so
        // readers queued behind it take it instead of each building again.
        let fresh = {
            let entries = self.entries.lock();
            entries.iter().find(|e| e.view == view).and_then(|entry| {
                self.servable(entry, feed.seq()).or_else(|| {
                    (entry.started >= arrived
                        && entry.mutations == self.mutations.load(Ordering::Acquire)
                        && entry.changes == feed.seq())
                    .then(|| entry.data.clone())
                    .flatten()
                })
            })
        };
        if let Some(data) = fresh {
            return Ok(data);
        }
        self.build(state, &view).await
    }

    /// Make every cached view look `by` older.
    #[cfg(test)]
    pub(super) fn age(&self, by: Duration) {
        for entry in self.entries.lock().iter_mut() {
            entry.built = entry.built.checked_sub(by).unwrap_or(entry.built);
            entry.started = entry.started.checked_sub(by).unwrap_or(entry.started);
        }
    }

    /// How many views are kept.
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.entries.lock().len()
    }

    /// Make every cached view look built in the previous local hour.
    #[cfg(test)]
    pub(super) fn previous_hour(&self) {
        for entry in self.entries.lock().iter_mut() {
            entry.hour -= 1;
        }
    }

    /// Pretend a write is in flight, as if a slow commit were still landing.
    #[cfg(test)]
    pub(super) fn hold_write(&self, held: bool) {
        self.write_in_flight.store(held, Ordering::Release);
    }

    /// A view as a read would get it now, without building one.
    #[cfg(test)]
    pub(super) fn cached(&self, state: &AppState, view: &ModeView) -> Option<ResponseData> {
        let changes = state.store.change_feed().seq();
        let entries = self.entries.lock();
        let entry = entries.iter().find(|e| &e.view == view)?;
        self.servable(entry, changes)
    }

    async fn changes_landed(&self, state: &AppState) -> Option<u64> {
        #[cfg(test)]
        if self.write_in_flight.load(Ordering::Acquire) {
            return None;
        }
        state.store.changes_landed(LAND_WAIT).await
    }

    fn servable(&self, entry: &Entry, changes: u64) -> Option<ResponseData> {
        let good = entry.settled
            && entry.mutations == self.mutations.load(Ordering::Acquire)
            && entry.changes == changes
            && entry.built.elapsed() < MAX_AGE
            && entry.hour == local_hour();
        good.then(|| entry.data.clone()).flatten()
    }

    /// Build `view` and keep the result unless a later build already
    /// replaced it. The caller holds the view's build lock.
    async fn build(&self, state: &AppState, view: &ModeView) -> HandlerResult {
        let feed = state.store.change_feed();
        let started = Instant::now();
        let mutations = self.mutations.load(Ordering::Acquire);
        let hour = local_hour();
        // The count first: a commit whose hook runs after this read is not
        // counted, so the next read rebuilds. Read with no write in flight,
        // every commit it counts is visible to the build.
        let (changes, settled) = match self.changes_landed(state).await {
            Some(changes) => (changes, true),
            None => (feed.seq(), false),
        };
        let result = view.build(state).await;
        if let Ok(data) = &result {
            let mut entries = self.entries.lock();
            if let Some(entry) = entries.iter_mut().find(|e| &e.view == view) {
                if entry.data.is_none() || entry.started <= started {
                    entry.data = Some(data.clone());
                    entry.started = started;
                    entry.built = Instant::now();
                    entry.mutations = mutations;
                    entry.changes = changes;
                    entry.settled = settled;
                    entry.hour = hour;
                }
            }
        }
        result
    }

    /// One refresher pass: drop idle views and rebuild the active ones a
    /// change, a mutation or a new hour has made stale.
    pub(super) async fn refresh(&self, state: &AppState) {
        let feed = state.store.change_feed();
        let seq = feed.seq();
        let mutations = self.mutations.load(Ordering::Acquire);
        let due: Vec<(ModeView, Arc<tokio::sync::Mutex<()>>)> = {
            let mut entries = self.entries.lock();
            entries.retain(|e| e.last_read.elapsed() < ACTIVE);
            entries
                .iter()
                .filter(|e| e.data.is_some())
                .filter(|e| {
                    // A view that may have missed a commit is never served, so
                    // it is rebuilt like a changed one, at most every
                    // `MIN_REBUILD` while a long write keeps the writer.
                    let changed =
                        (!e.settled || e.changes != seq) && e.built.elapsed() >= MIN_REBUILD;
                    changed
                        || e.mutations != mutations
                        || e.hour != local_hour()
                        || e.built.elapsed() >= MAX_AGE - REFRESH_AHEAD
                })
                .map(|e| (e.view.clone(), Arc::clone(&e.building)))
                .collect()
        };
        for (view, lock) in due {
            // A reader is already building it.
            let Ok(_building) = lock.try_lock() else {
                continue;
            };
            let started = Instant::now();
            match self.build(state, &view).await {
                Ok(_) => tracing::debug!(
                    ?view,
                    elapsed_ms = started.elapsed().as_millis(),
                    "mode view refreshed"
                ),
                Err(error) => tracing::warn!(?view, %error, "mode view refresh failed"),
            }
        }
    }
}

/// Hours since the epoch in local time: changes at every local hour, so at
/// the evening switch and at midnight.
fn local_hour() -> i64 {
    Local::now()
        .naive_local()
        .and_utc()
        .timestamp()
        .div_euclid(3600)
}

impl Entry {
    fn new(view: ModeView) -> Self {
        let now = Instant::now();
        Self {
            view,
            data: None,
            started: now,
            built: now,
            mutations: 0,
            changes: 0,
            settled: false,
            hour: local_hour(),
            last_read: now,
            building: Arc::new(tokio::sync::Mutex::new(())),
        }
    }
}

/// Keep recently read views fresh until shutdown.
pub(crate) async fn refresher_loop(state: Arc<AppState>, mut shutdown_rx: watch::Receiver<bool>) {
    let mut ticker = tokio::time::interval(TICK);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = ticker.tick() => {}
            changed = shutdown_rx.changed() => {
                if changed.is_err() || *shutdown_rx.borrow_and_update() {
                    break;
                }
                continue;
            }
        }
        state.mode_cache.refresh(&state).await;
    }
}
