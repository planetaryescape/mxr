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

const MAX_AGE: Duration = Duration::from_secs(60);
const REFRESH_AHEAD: Duration = Duration::from_secs(10);
const LAND_WAIT: Duration = Duration::from_millis(50);
const MIN_REBUILD: Duration = Duration::from_secs(5);
const ACTIVE: Duration = Duration::from_secs(5 * 60);
const MAX_ENTRIES: usize = 16;
const TICK: Duration = Duration::from_millis(500);

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
    mutations: u64,
    changes: u64,
    settled: bool,
    hour: i64,
    last_read: Instant,
    building: Arc<tokio::sync::Mutex<()>>,
}

struct SharedSnapshot {
    account_id: Option<AccountId>,
    accounts: Vec<AccountId>,
    offset: String,
    now_sec: i64,
    snapshot: Arc<now::NowSnapshot>,
    built: Instant,
    changes: u64,
    mutations: u64,
    hour: i64,
}

#[derive(Default)]
pub(crate) struct ModeCache {
    entries: Mutex<Vec<Entry>>,
    shared_snapshot: Mutex<Option<SharedSnapshot>>,
    mutations: AtomicU64,
    #[cfg(test)]
    write_in_flight: std::sync::atomic::AtomicBool,
}

impl ModeCache {
    pub(super) fn shared_snapshot(
        &self,
        account_id: Option<&AccountId>,
        accounts: &[AccountId],
        offset: &str,
        now_sec: i64,
        changes: u64,
    ) -> Option<Arc<now::NowSnapshot>> {
        let slot = self.shared_snapshot.lock();
        let shared = slot.as_ref()?;
        (shared.account_id.as_ref() == account_id
            && shared.accounts == accounts
            && shared.offset == offset
            && shared.now_sec == now_sec
            && shared.changes == changes
            && shared.mutations == self.mutations.load(Ordering::Acquire)
            && shared.hour == local_hour()
            && shared.built.elapsed() < Duration::from_secs(1))
        .then(|| Arc::clone(&shared.snapshot))
    }

    pub(super) fn remember_snapshot(
        &self,
        account_id: Option<&AccountId>,
        accounts: &[AccountId],
        offset: &str,
        now_sec: i64,
        changes: u64,
        snapshot: Arc<now::NowSnapshot>,
    ) {
        *self.shared_snapshot.lock() = Some(SharedSnapshot {
            account_id: account_id.cloned(),
            accounts: accounts.to_vec(),
            offset: offset.to_string(),
            now_sec,
            snapshot,
            built: Instant::now(),
            changes,
            mutations: self.mutations.load(Ordering::Acquire),
            hour: local_hour(),
        });
    }

    pub(crate) fn note_mutation(&self) {
        self.mutations.fetch_add(1, Ordering::AcqRel);
    }

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

    #[cfg(test)]
    pub(super) fn age(&self, by: Duration) {
        for entry in self.entries.lock().iter_mut() {
            entry.built = entry.built.checked_sub(by).unwrap_or(entry.built);
            entry.started = entry.started.checked_sub(by).unwrap_or(entry.started);
        }
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.entries.lock().len()
    }

    #[cfg(test)]
    pub(super) fn previous_hour(&self) {
        for entry in self.entries.lock().iter_mut() {
            entry.hour -= 1;
        }
    }

    #[cfg(test)]
    pub(super) fn hold_write(&self, held: bool) {
        self.write_in_flight.store(held, Ordering::Release);
    }

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

    async fn build(&self, state: &AppState, view: &ModeView) -> HandlerResult {
        let feed = state.store.change_feed();
        let started = Instant::now();
        let mutations = self.mutations.load(Ordering::Acquire);
        let hour = local_hour();
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
