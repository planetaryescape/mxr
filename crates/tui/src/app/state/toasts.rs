use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Default lifetime for transient toasts. Mirrors the long-standing
/// status-bar convention of clearing transient messages after ~5s.
pub const TOAST_DEFAULT_TTL: Duration = Duration::from_secs(5);

/// Maximum toasts rendered at once. Older toasts stay queued and become
/// visible as newer ones expire, so a burst of completions can't paper
/// over the whole screen.
pub const TOAST_MAX_VISIBLE: usize = 3;

/// Hard cap on queued toasts. Past this, the oldest entries are dropped —
/// nobody reads 50 stale "Done" notifications.
const TOAST_QUEUE_CAPACITY: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastSeverity {
    Info,
    Success,
    Warn,
    Error,
}

/// A transient, self-expiring notification rendered as a stacked box
/// above the status bar. Use for completed/failed operations the user
/// should notice without blocking; ambient state (mailbox counts, sync
/// status) stays on `App::status_message` / the status bar.
#[derive(Debug, Clone)]
pub struct Toast {
    pub text: String,
    pub severity: ToastSeverity,
    pub created_at: Instant,
    pub ttl: Duration,
    /// Optional trailing hint, e.g. "u to undo". When set, the renderer
    /// appends the hint plus a live countdown of the remaining window.
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

/// FIFO queue of toasts owned by `App`. Newest entries are pushed to the
/// back; rendering shows the most recent `TOAST_MAX_VISIBLE` with the
/// newest closest to the status bar.
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

    /// Drop expired toasts. Called from the main-loop tick alongside the
    /// other time-based sweeps (`tick_pending_undo` etc).
    pub fn sweep_expired(&mut self, now: Instant) {
        self.entries.retain(|toast| !toast.expired(now));
    }

    /// Most recent unexpired toasts, newest first, capped at
    /// [`TOAST_MAX_VISIBLE`].
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
        queue.push(toast_at("fresh", t0 + Duration::from_secs(4), Duration::from_secs(5)));

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
    fn visible_skips_expired_toasts_even_before_sweep() {
        let t0 = Instant::now();
        let mut queue = ToastQueue::default();
        queue.push(toast_at("expired", t0, Duration::from_secs(1)));
        queue.push(toast_at("live", t0, Duration::from_secs(60)));

        let visible = queue.visible(t0 + Duration::from_secs(2));
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].text, "live");
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
    fn remaining_counts_down_and_saturates_at_zero() {
        let t0 = Instant::now();
        let toast = toast_at("x", t0, Duration::from_secs(10));
        assert_eq!(toast.remaining(t0 + Duration::from_secs(4)).as_secs(), 6);
        assert!(toast.remaining(t0 + Duration::from_secs(11)).is_zero());
        assert!(toast.expired(t0 + Duration::from_secs(11)));
    }
}
