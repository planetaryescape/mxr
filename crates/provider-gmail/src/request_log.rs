//! Gmail API requests one account made recently, for the rate-limit warning.
//!
//! In memory only. A restart resets the counts, which is fine: the question
//! this answers is whether mxr itself is hammering the account right now.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

const MINUTE: Duration = Duration::from_secs(60);
const HOUR: Duration = Duration::from_secs(60 * 60);

#[derive(Default)]
pub(crate) struct RequestLog {
    entries: Mutex<VecDeque<(Instant, &'static str)>>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct RequestSummary {
    pub last_minute: usize,
    pub last_hour: usize,
    pub last_hour_by_endpoint: BTreeMap<&'static str, usize>,
}

impl RequestSummary {
    /// `messages.get=812, history.list=3`, or `none` when nothing was sent.
    pub(crate) fn endpoint_counts(&self) -> String {
        if self.last_hour_by_endpoint.is_empty() {
            return "none".to_string();
        }
        self.last_hour_by_endpoint
            .iter()
            .map(|(endpoint, count)| format!("{endpoint}={count}"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl RequestLog {
    fn lock(&self) -> MutexGuard<'_, VecDeque<(Instant, &'static str)>> {
        // A panic while holding this lock cannot leave the counts wrong in a
        // way that matters, so keep counting rather than poisoning every request.
        self.entries.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn record(&self, endpoint: &'static str, now: Instant) {
        let mut entries = self.lock();
        entries.push_back((now, endpoint));
        while entries
            .front()
            .is_some_and(|(at, _)| now.saturating_duration_since(*at) >= HOUR)
        {
            entries.pop_front();
        }
    }

    pub(crate) fn summary(&self, now: Instant) -> RequestSummary {
        let entries = self.lock();
        let mut summary = RequestSummary::default();
        for (at, endpoint) in entries.iter() {
            let age = now.saturating_duration_since(*at);
            if age >= HOUR {
                continue;
            }
            summary.last_hour += 1;
            if age < MINUTE {
                summary.last_minute += 1;
            }
            *summary.last_hour_by_endpoint.entry(endpoint).or_insert(0) += 1;
        }
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_the_last_minute_and_hour_by_endpoint() {
        let log = RequestLog::default();
        let base = Instant::now();
        log.record("messages.get", base);
        log.record("messages.get", base + Duration::from_secs(30 * 60));
        log.record("history.list", base + Duration::from_secs(59 * 60 + 30));

        let summary = log.summary(base + Duration::from_secs(59 * 60 + 45));

        assert_eq!(summary.last_hour, 3);
        assert_eq!(summary.last_minute, 1);
        assert_eq!(summary.endpoint_counts(), "history.list=1, messages.get=2");
    }

    #[test]
    fn requests_older_than_an_hour_are_dropped() {
        let log = RequestLog::default();
        let base = Instant::now();
        log.record("messages.get", base);
        log.record("messages.get", base + Duration::from_secs(3600));

        let summary = log.summary(base + Duration::from_secs(3601));

        assert_eq!(summary.last_hour, 1);
        assert_eq!(log.lock().len(), 1, "record prunes what the window dropped");
    }

    #[test]
    fn an_idle_account_reports_no_endpoints() {
        let summary = RequestLog::default().summary(Instant::now());

        assert_eq!(summary, RequestSummary::default());
        assert_eq!(summary.endpoint_counts(), "none");
    }
}
