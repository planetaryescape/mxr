use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub const TOAST_DEFAULT_TTL: Duration = Duration::from_secs(5);

pub const TOAST_MAX_VISIBLE: usize = 3;

const TOAST_QUEUE_CAPACITY: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastSeverity {
    Info,
    Success,
    Warn,
    Error,
}

#[derive(Debug, Clone)]
pub struct Toast {
    pub text: String,
    pub severity: ToastSeverity,
    pub created_at: Instant,
    pub ttl: Duration,
    pub action_hint: Option<String>,
}

impl Toast {
    pub fn new(text: impl Into<String>, severity: ToastSeverity) -> Self {
        Self {
            text: text.into(),
            severity,
            created_at: Instant::now(),
            ttl: TOAST_DEFAULT_TTL,
            action_hint: None,
        }
    }

    pub fn info(text: impl Into<String>) -> Self {
        Self::new(text, ToastSeverity::Info)
    }

    pub fn success(text: impl Into<String>) -> Self {
        Self::new(text, ToastSeverity::Success)
    }

    pub fn warn(text: impl Into<String>) -> Self {
        Self::new(text, ToastSeverity::Warn)
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self::new(text, ToastSeverity::Error)
    }

    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    pub fn with_action_hint(mut self, hint: impl Into<String>) -> Self {
        self.action_hint = Some(hint.into());
        self
    }

    pub fn remaining(&self, now: Instant) -> Duration {
        self.ttl
            .saturating_sub(now.saturating_duration_since(self.created_at))
    }

    pub fn expired(&self, now: Instant) -> bool {
        self.remaining(now).is_zero()
    }
}

#[derive(Debug, Default)]
pub struct ToastQueue {
    entries: VecDeque<Toast>,
}

impl ToastQueue {
    pub fn push(&mut self, toast: Toast) {
        if self.entries.len() >= TOAST_QUEUE_CAPACITY {
            self.entries.pop_front();
        }
        self.entries.push_back(toast);
    }

    pub fn sweep_expired(&mut self, now: Instant) {
        self.entries.retain(|toast| !toast.expired(now));
    }

    pub fn visible(&self, now: Instant) -> Vec<&Toast> {
        self.entries
            .iter()
            .rev()
            .filter(|toast| !toast.expired(now))
            .take(TOAST_MAX_VISIBLE)
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn next_expiry(&self, now: Instant) -> Option<Duration> {
        self.entries.iter().map(|toast| toast.remaining(now)).min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toast_at(text: &str, created_at: Instant, ttl: Duration) -> Toast {
        Toast {
            text: text.into(),
            severity: ToastSeverity::Info,
            created_at,
            ttl,
            action_hint: None,
        }
    }

    #[test]
    fn sweep_expired_drops_only_stale_toasts() {
        let t0 = Instant::now();
        let mut queue = ToastQueue::default();
        queue.push(toast_at("old", t0, Duration::from_secs(5)));
        queue.push(toast_at(
            "fresh",
            t0 + Duration::from_secs(4),
            Duration::from_secs(5),
        ));

        queue.sweep_expired(t0 + Duration::from_secs(6));

        let visible = queue.visible(t0 + Duration::from_secs(6));
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].text, "fresh");
    }

    #[test]
    fn visible_returns_newest_first_and_caps_at_max() {
        let t0 = Instant::now();
        let mut queue = ToastQueue::default();
        for i in 0..5 {
            queue.push(toast_at(&format!("t{i}"), t0, Duration::from_secs(60)));
        }

        let visible = queue.visible(t0);
        assert_eq!(visible.len(), TOAST_MAX_VISIBLE);
        let texts: Vec<&str> = visible.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(texts, vec!["t4", "t3", "t2"]);
    }

    #[test]
    fn queue_capacity_drops_oldest_entries() {
        let t0 = Instant::now();
        let mut queue = ToastQueue::default();
        for i in 0..(TOAST_QUEUE_CAPACITY + 2) {
            queue.push(toast_at(&format!("t{i}"), t0, Duration::from_secs(60)));
        }

        assert_eq!(queue.entries.len(), TOAST_QUEUE_CAPACITY);
        assert_eq!(queue.entries.front().unwrap().text, "t2");
    }

    #[test]
    fn next_expiry_requests_a_sweep_before_the_next_live_deadline() {
        let t0 = Instant::now();
        let mut queue = ToastQueue::default();
        queue.push(toast_at("expired", t0, Duration::from_secs(1)));
        queue.push(toast_at("later", t0, Duration::from_secs(10)));
        queue.push(toast_at(
            "sooner",
            t0 + Duration::from_secs(2),
            Duration::from_secs(5),
        ));

        let now = t0 + Duration::from_secs(3);
        assert_eq!(queue.next_expiry(now), Some(Duration::ZERO));
        queue.sweep_expired(now);
        assert_eq!(queue.next_expiry(now), Some(Duration::from_secs(4)));
        let after_all_expired = t0 + Duration::from_secs(10);
        queue.sweep_expired(after_all_expired);
        assert_eq!(queue.next_expiry(after_all_expired), None);
    }
}
