//! Holds back Gmail calls for one account after Gmail refuses one with a rate
//! limit, until the wait Gmail asked for has passed.
//!
//! Renewing the limit is the failure this prevents: each request sent during
//! Gmail's lockout restarts its window. In memory only, so a restart clears it
//! and the first request after restart gets one more refusal.

use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// Gmail's own waits are minutes. Capping at a day keeps `Instant` arithmetic
/// from overflowing on a hostile or malformed Retry-After.
const MAX_PAUSE: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Default)]
pub(crate) struct RateLimitGate {
    pause: Mutex<Option<Pause>>,
}

struct Pause {
    until: Instant,
    /// The refusal that set this pause, shown to callers that are held back.
    detail: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Held {
    pub remaining: Duration,
    pub detail: Option<String>,
}

impl RateLimitGate {
    fn lock(&self) -> MutexGuard<'_, Option<Pause>> {
        self.pause.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// `Some` while Gmail's wait has not passed. Callers return the error
    /// without sending anything.
    pub(crate) fn held(&self, now: Instant) -> Option<Held> {
        let guard = self.lock();
        guard
            .as_ref()
            .filter(|pause| pause.until > now)
            .map(|pause| Held {
                remaining: pause.until - now,
                detail: pause.detail.clone(),
            })
    }

    /// Starts or extends the wait. A refusal that would end before the current
    /// wait does not shorten it.
    pub(crate) fn refused(&self, now: Instant, retry_after_secs: u64, detail: Option<String>) {
        let wait = Duration::from_secs(retry_after_secs).min(MAX_PAUSE);
        let until = now + wait;
        let mut guard = self.lock();
        let extends = guard.as_ref().is_none_or(|pause| pause.until <= until);
        if extends {
            *guard = Some(Pause { until, detail });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refusal_holds_calls_until_its_wait_passes() {
        let gate = RateLimitGate::default();
        let base = Instant::now();
        gate.refused(base, 900, Some("labels.list".into()));

        assert_eq!(
            gate.held(base + Duration::from_secs(100)),
            Some(Held {
                remaining: Duration::from_secs(800),
                detail: Some("labels.list".into()),
            })
        );
        assert_eq!(gate.held(base + Duration::from_secs(900)), None);
    }
}
