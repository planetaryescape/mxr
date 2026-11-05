//! Guards for background LLM work: a breaker for an endpoint that keeps
//! failing, and a ledger so the same input isn't re-sent after it already
//! succeeded or while it is backing off after a failure.
//!
//! A local model costs 10-30 s of CPU per call, and a sync can queue
//! hundreds of contacts/messages, so both guards exist to stop the daemon
//! re-spending that on work that can't or needn't change.
//!
//! The two guards take different failures. An endpoint failure (unreachable,
//! timed out, rate limited, 5xx, bad key) goes to the breaker and leaves the
//! input untouched, so an outage of any length costs no input its retries;
//! the input is put off instead and re-run once the endpoint answers. A
//! failure of the input itself (an unusable reply, a request-specific 4xx)
//! goes to the ledger.

use crate::{LlmError, LlmFeature};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::watch;
use tokio::time::Instant;

/// Consecutive failed background calls before background LLM work pauses.
/// Background callers fall back to heuristics or skip, so pausing loses
/// nothing a failing endpoint was delivering anyway.
pub(crate) const BREAKER_THRESHOLD: u32 = 5;
pub(crate) const BREAKER_MIN_COOLDOWN: Duration = Duration::from_secs(5 * 60);
const BREAKER_MAX_COOLDOWN: Duration = Duration::from_secs(60 * 60);
/// The most a cheap endpoint probe may take. A probe is a listing request, so
/// anything slower says the endpoint isn't ready for real work either.
pub(crate) const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct BackgroundBreaker {
    consecutive_failures: u32,
    open_until: Option<Instant>,
    /// Zero until the first trip; doubles on each re-trip up to the max.
    cooldown: Duration,
    /// The cooldown is over and one caller is checking the endpoint; every
    /// other caller is turned away until it reports.
    probing: bool,
}

/// What the breaker lets one background call do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Gate {
    Closed,
    Open(Duration),
    /// The cooldown is over and this caller checks the endpoint for everyone.
    Probe,
}

impl BackgroundBreaker {
    pub(crate) fn admit(&mut self, now: Instant) -> Gate {
        let Some(until) = self.open_until else {
            return Gate::Closed;
        };
        if until > now {
            return Gate::Open(until - now);
        }
        if self.probing {
            // Another caller is checking right now; its answer is moments away.
            return Gate::Open(Duration::from_secs(1));
        }
        self.probing = true;
        Gate::Probe
    }

    /// The time left before the next probe, or `None` when calls go through.
    pub(crate) fn remaining(&self, now: Instant) -> Option<Duration> {
        self.open_until
            .filter(|until| *until > now)
            .map(|until| until - now)
    }

    pub(crate) fn release_probe(&mut self) {
        self.probing = false;
    }

    /// The endpoint answered. Returns true when that ends an outage.
    pub(crate) fn record_success(&mut self) -> bool {
        let recovered = self.cooldown > Duration::ZERO;
        if recovered {
            tracing::info!("background LLM calls recovered; breaker closed");
        }
        *self = Self::default();
        recovered
    }

    pub(crate) fn record_failure(&mut self, now: Instant, error: &LlmError) {
        self.probing = false;
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        // Past the threshold every failure is a half-open probe failing, so
        // it re-opens straight away with a longer cooldown.
        if self.consecutive_failures < BREAKER_THRESHOLD {
            return;
        }
        self.cooldown = if self.cooldown == Duration::ZERO {
            BREAKER_MIN_COOLDOWN
        } else {
            (self.cooldown * 2).min(BREAKER_MAX_COOLDOWN)
        };
        self.open_until = Some(now + self.cooldown);
        tracing::warn!(
            consecutive_failures = self.consecutive_failures,
            cooldown_secs = self.cooldown.as_secs(),
            %error,
            "background LLM calls paused after repeated failures"
        );
    }
}

/// Failed inputs retry after 1 h, 4 h, 16 h, then daily, and stop after
/// `LEDGER_MAX_ATTEMPTS` until the input itself changes (new mail).
pub(crate) const LEDGER_FIRST_RETRY: Duration = Duration::from_secs(60 * 60);
const LEDGER_MAX_RETRY: Duration = Duration::from_secs(24 * 60 * 60);
pub(crate) const LEDGER_MAX_ATTEMPTS: u32 = 5;
/// Past this size the ledger starts over rather than tracking age; that
/// costs at most one more try per input. The same bound caps the inputs put
/// off by an outage, past which new ones wait for their next mail instead.
const LEDGER_MAX_ENTRIES: usize = 20_000;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Outcome {
    Succeeded,
    Failed { attempts: u32, retry_at: Instant },
}

#[derive(Debug, Clone)]
struct Entry {
    fingerprint: String,
    outcome: Outcome,
}

#[derive(Debug, Default)]
pub(crate) struct AttemptLedger {
    entries: HashMap<(LlmFeature, String), Entry>,
    /// Inputs that weren't sent, or got no answer, because the endpoint was
    /// out. Sorted so a recovery pass runs them in a stable order.
    deferred: HashMap<LlmFeature, BTreeSet<String>>,
}

impl AttemptLedger {
    pub(crate) fn is_due(
        &self,
        feature: LlmFeature,
        key: &str,
        fingerprint: &str,
        now: Instant,
    ) -> bool {
        let Some(entry) = self.entries.get(&(feature, key.to_string())) else {
            return true;
        };
        if entry.fingerprint != fingerprint {
            return true;
        }
        match entry.outcome {
            Outcome::Succeeded => false,
            Outcome::Failed { attempts, retry_at } => {
                attempts < LEDGER_MAX_ATTEMPTS && now >= retry_at
            }
        }
    }

    pub(crate) fn record(
        &mut self,
        feature: LlmFeature,
        key: &str,
        fingerprint: &str,
        succeeded: bool,
        now: Instant,
    ) {
        self.undefer(feature, key);
        let map_key = (feature, key.to_string());
        let previous_failures = match self.entries.get(&map_key) {
            Some(Entry {
                fingerprint: previous,
                outcome: Outcome::Failed { attempts, .. },
            }) if previous == fingerprint => *attempts,
            _ => 0,
        };
        if self.entries.len() >= LEDGER_MAX_ENTRIES && !self.entries.contains_key(&map_key) {
            self.entries.clear();
        }
        let outcome = if succeeded {
            Outcome::Succeeded
        } else {
            let attempts = previous_failures + 1;
            if attempts == LEDGER_MAX_ATTEMPTS {
                tracing::debug!(
                    ?feature,
                    attempts,
                    "background LLM input given up until it changes"
                );
            }
            Outcome::Failed {
                attempts,
                retry_at: now + retry_delay(attempts),
            }
        };
        self.entries.insert(
            map_key,
            Entry {
                fingerprint: fingerprint.to_string(),
                outcome,
            },
        );
    }

    /// Put an input off until the endpoint is back, without counting it as a
    /// try.
    pub(crate) fn defer(&mut self, feature: LlmFeature, key: &str) {
        let total: usize = self.deferred.values().map(BTreeSet::len).sum();
        let keys = self.deferred.entry(feature).or_default();
        if total >= LEDGER_MAX_ENTRIES && !keys.contains(key) {
            return;
        }
        keys.insert(key.to_string());
    }

    fn undefer(&mut self, feature: LlmFeature, key: &str) {
        if let Some(keys) = self.deferred.get_mut(&feature) {
            keys.remove(key);
        }
    }

    pub(crate) fn has_deferred(&self, feature: LlmFeature) -> bool {
        self.deferred
            .get(&feature)
            .is_some_and(|keys| !keys.is_empty())
    }

    /// The inputs put off for `feature`, removed: a recovery pass runs each
    /// once, and one that the endpoint fails again is put off again.
    pub(crate) fn take_deferred(&mut self, feature: LlmFeature) -> Vec<String> {
        self.deferred
            .remove(&feature)
            .map(|keys| keys.into_iter().collect())
            .unwrap_or_default()
    }

    /// Forget every verdict but keep the inputs still waiting for an answer:
    /// a new provider deserves a fresh attempt at everything, and the work
    /// put off by an outage is still owed.
    pub(crate) fn reset_keeping_deferred(&mut self) {
        self.entries.clear();
    }
}

fn retry_delay(attempts: u32) -> Duration {
    let factor = 4u32.saturating_pow(attempts.saturating_sub(1));
    LEDGER_FIRST_RETRY
        .saturating_mul(factor)
        .min(LEDGER_MAX_RETRY)
}

/// Wakes whoever re-runs put-off work when an endpoint starts answering
/// again. Clones share one channel.
#[derive(Debug, Clone)]
pub(crate) struct RecoverySignal(Arc<watch::Sender<u64>>);

impl Default for RecoverySignal {
    fn default() -> Self {
        Self(Arc::new(watch::channel(0).0))
    }
}

impl RecoverySignal {
    pub(crate) fn notify(&self) {
        self.0
            .send_modify(|generation| *generation = generation.wrapping_add(1));
    }

    pub(crate) fn subscribe(&self) -> watch::Receiver<u64> {
        self.0.subscribe()
    }
}

// --- Saved across restarts ------------------------------------------------

/// The breaker and ledger as saved between daemon runs. Times are wall-clock
/// seconds since the Unix epoch, because a monotonic instant means nothing
/// to the next process. Holds feature names, input keys (contact or message
/// ids) and content fingerprints (hashes), never mail text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackgroundState {
    version: u32,
    breaker: SavedBreaker,
    attempts: Vec<SavedAttempt>,
    deferred: Vec<SavedInput>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct SavedBreaker {
    consecutive_failures: u32,
    cooldown_secs: u64,
    open_until: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SavedAttempt {
    feature: LlmFeature,
    key: String,
    fingerprint: String,
    /// `None` when the input succeeded.
    failed: Option<SavedFailure>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SavedFailure {
    attempts: u32,
    retry_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SavedInput {
    feature: LlmFeature,
    key: String,
}

const STATE_VERSION: u32 = 1;

/// Converts between this process's monotonic clock and the wall clock, both
/// read once so every time in one save or load shifts by the same amount.
#[derive(Clone, Copy)]
pub(crate) struct Clocks {
    pub now: Instant,
    pub wall: SystemTime,
}

impl Clocks {
    pub(crate) fn current() -> Self {
        Self {
            now: Instant::now(),
            wall: SystemTime::now(),
        }
    }

    fn unix_secs(self, at: Instant) -> u64 {
        let wall = if at >= self.now {
            self.wall.checked_add(at - self.now)
        } else {
            self.wall.checked_sub(self.now - at)
        };
        wall.and_then(|wall| wall.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |since| since.as_secs())
    }

    /// A saved time on this process's clock. A time already past becomes
    /// now: what was waiting for it is due.
    fn instant_at(self, secs: u64) -> Instant {
        let at = UNIX_EPOCH + Duration::from_secs(secs);
        match at.duration_since(self.wall) {
            Ok(ahead) => self.now + ahead,
            Err(_) => self.now,
        }
    }
}

impl BackgroundState {
    pub(crate) fn capture(
        breaker: &BackgroundBreaker,
        ledger: &AttemptLedger,
        clocks: Clocks,
    ) -> Self {
        let mut attempts: Vec<SavedAttempt> = ledger
            .entries
            .iter()
            .map(|((feature, key), entry)| SavedAttempt {
                feature: *feature,
                key: key.clone(),
                fingerprint: entry.fingerprint.clone(),
                failed: match entry.outcome {
                    Outcome::Succeeded => None,
                    Outcome::Failed { attempts, retry_at } => Some(SavedFailure {
                        attempts,
                        retry_at: clocks.unix_secs(retry_at),
                    }),
                },
            })
            .collect();
        attempts.sort_by(|a, b| (a.feature.key(), &a.key).cmp(&(b.feature.key(), &b.key)));
        let mut deferred: Vec<SavedInput> = ledger
            .deferred
            .iter()
            .flat_map(|(feature, keys)| {
                keys.iter().map(|key| SavedInput {
                    feature: *feature,
                    key: key.clone(),
                })
            })
            .collect();
        deferred.sort_by(|a, b| (a.feature.key(), &a.key).cmp(&(b.feature.key(), &b.key)));
        Self {
            version: STATE_VERSION,
            breaker: SavedBreaker {
                consecutive_failures: breaker.consecutive_failures,
                cooldown_secs: breaker.cooldown.as_secs(),
                open_until: breaker.open_until.map(|until| clocks.unix_secs(until)),
            },
            attempts,
            deferred,
        }
    }

    /// The breaker and ledger this state describes. A probe in flight when it
    /// was saved died with that process, so none is restored.
    pub(crate) fn restore(&self, clocks: Clocks) -> (BackgroundBreaker, AttemptLedger) {
        let breaker = BackgroundBreaker {
            consecutive_failures: self.breaker.consecutive_failures,
            open_until: self.breaker.open_until.map(|secs| clocks.instant_at(secs)),
            cooldown: Duration::from_secs(self.breaker.cooldown_secs),
            probing: false,
        };
        let mut ledger = AttemptLedger::default();
        for saved in self.attempts.iter().take(LEDGER_MAX_ENTRIES) {
            let outcome = match &saved.failed {
                None => Outcome::Succeeded,
                Some(failure) => Outcome::Failed {
                    attempts: failure.attempts,
                    retry_at: clocks.instant_at(failure.retry_at),
                },
            };
            ledger.entries.insert(
                (saved.feature, saved.key.clone()),
                Entry {
                    fingerprint: saved.fingerprint.clone(),
                    outcome,
                },
            );
        }
        for saved in &self.deferred {
            ledger.defer(saved.feature, &saved.key);
        }
        (breaker, ledger)
    }

    /// Whether this state was written by a version this build understands.
    pub fn is_supported(&self) -> bool {
        self.version == STATE_VERSION
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_delay_grows_four_fold_and_caps_at_a_day() {
        let hours = |n: u64| Duration::from_secs(n * 60 * 60);
        assert_eq!(retry_delay(1), hours(1));
        assert_eq!(retry_delay(2), hours(4));
        assert_eq!(retry_delay(3), hours(16));
        assert_eq!(retry_delay(4), hours(24));
        assert_eq!(retry_delay(40), hours(24));
    }
}
