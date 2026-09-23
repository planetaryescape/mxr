//! Guards for background LLM work: a breaker for an endpoint that keeps
//! failing, and a ledger so the same input isn't re-sent after it already
//! succeeded or while it is backing off after a failure.
//!
//! A local model costs 10-30 s of CPU per call, and a sync can queue
//! hundreds of contacts/messages, so both guards exist to stop the daemon
//! re-spending that on work that can't or needn't change.

use crate::{LlmError, LlmFeature};
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::Instant;

/// Consecutive failed background calls before background LLM work pauses.
/// Background callers fall back to heuristics or skip, so pausing loses
/// nothing a failing endpoint was delivering anyway.
pub(crate) const BREAKER_THRESHOLD: u32 = 5;
pub(crate) const BREAKER_MIN_COOLDOWN: Duration = Duration::from_secs(5 * 60);
const BREAKER_MAX_COOLDOWN: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, Default)]
pub(crate) struct BackgroundBreaker {
    consecutive_failures: u32,
    open_until: Option<Instant>,
    /// Zero until the first trip; doubles on each re-trip up to the max.
    cooldown: Duration,
}

impl BackgroundBreaker {
    pub(crate) fn remaining(&self, now: Instant) -> Option<Duration> {
        self.open_until
            .filter(|until| *until > now)
            .map(|until| until - now)
    }

    pub(crate) fn record_success(&mut self) {
        if self.cooldown > Duration::ZERO {
            tracing::info!("background LLM calls recovered; breaker closed");
        }
        *self = Self::default();
    }

    pub(crate) fn record_failure(&mut self, now: Instant, error: &LlmError) {
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
/// In-memory only: after a restart an input costs at most one more try.
/// Past this size the ledger starts over rather than tracking age; that
/// also costs at most one more try per input.
const LEDGER_MAX_ENTRIES: usize = 20_000;

#[derive(Debug)]
enum Outcome {
    Succeeded,
    Failed { attempts: u32, retry_at: Instant },
}

#[derive(Debug)]
struct Entry {
    fingerprint: String,
    outcome: Outcome,
}

#[derive(Debug, Default)]
pub(crate) struct AttemptLedger {
    entries: HashMap<(LlmFeature, String), Entry>,
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
}

fn retry_delay(attempts: u32) -> Duration {
    let factor = 4u32.saturating_pow(attempts.saturating_sub(1));
    LEDGER_FIRST_RETRY
        .saturating_mul(factor)
        .min(LEDGER_MAX_RETRY)
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
