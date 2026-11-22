//! What mxr remembers about each route (provider and model): whether it is
//! working, cooling off, or broken until something changes.
//!
//! One `RouteHealth` is shared by every caller, foreground and background,
//! so a route that failed for one request is skipped for all of them instead
//! of every item in a batch rediscovering the failure. When a route's wait is
//! over it is half-open: exactly one request (the probe) tries it while
//! everyone else keeps using the fallback.
//!
//! State is in memory only. A restart costs one failed attempt per bad route.

use crate::background::RecoverySignal;
use crate::route::{RouteKey, RouteProvider};
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tokio::time::Instant;

pub(crate) const COOLING_MIN: Duration = Duration::from_secs(60);
pub(crate) const COOLING_MAX: Duration = Duration::from_secs(30 * 60);
/// A route that is broken until a key or the config changes is still
/// retried this often, in case the change happened somewhere mxr can't see
/// (a key rotated at the provider, a model that became available).
pub(crate) const BROKEN_TTL: Duration = Duration::from_secs(60 * 60);
const RETRY_AFTER_MAX: Duration = Duration::from_secs(60 * 60);
const RETRY_AFTER_MIN: Duration = Duration::from_secs(1);

/// Why a route is out, in words fit to show: a status and a fixed phrase,
/// never text from the provider's reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FailureReason {
    pub status: Option<u16>,
    pub text: &'static str,
}

impl FailureReason {
    pub(crate) const fn status(status: u16, text: &'static str) -> Self {
        Self {
            status: Some(status),
            text,
        }
    }

    pub(crate) const fn plain(text: &'static str) -> Self {
        Self { status: None, text }
    }
}

impl fmt::Display for FailureReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.status {
            Some(status) => write!(f, "{status} {}", self.text),
            None => f.write_str(self.text),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RouteFailure {
    /// Temporary (rate limit, overload, outage, timeout). Retried after a
    /// backoff, or after the server's `retry-after`.
    Cooling {
        reason: FailureReason,
        retry_after: Option<Duration>,
    },
    /// Needs a change from the user (bad key, model not found). Retried
    /// after [`BROKEN_TTL`] unless the key or config changes first.
    Broken { reason: FailureReason },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteState {
    Ok,
    Cooling,
    Broken,
    Recovering,
    Probing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteView {
    pub state: RouteState,
    pub reason: Option<FailureReason>,
    /// Time left until the route may be tried again; zero once it may be.
    pub retry_in: Duration,
}

impl RouteView {
    const OK: Self = Self {
        state: RouteState::Ok,
        reason: None,
        retry_in: Duration::ZERO,
    };
}

#[derive(Debug, Clone, Copy)]
struct Unhealthy {
    broken: bool,
    until: Instant,
    reason: FailureReason,
}

#[derive(Debug)]
struct Entry {
    /// Names this stretch of the route's history. Every transition (and
    /// every new entry) takes a number nobody had before, so an outcome
    /// reported by a request that started under an earlier stretch matches
    /// nothing and is dropped instead of re-escalating the backoff.
    epoch: u64,
    failures: u32,
    unhealthy: Option<Unhealthy>,
    probing: bool,
}

impl Entry {
    fn view(&self, now: Instant) -> RouteView {
        let Some(unhealthy) = self.unhealthy else {
            return RouteView::OK;
        };
        let remaining = unhealthy.until.saturating_duration_since(now);
        let state = if remaining > Duration::ZERO {
            if unhealthy.broken {
                RouteState::Broken
            } else {
                RouteState::Cooling
            }
        } else if self.probing {
            RouteState::Probing
        } else {
            RouteState::Recovering
        };
        RouteView {
            state,
            reason: Some(unhealthy.reason),
            retry_in: remaining,
        }
    }
}

#[derive(Debug, Default)]
struct Book {
    last_epoch: u64,
    entries: HashMap<RouteKey, Entry>,
}

impl Book {
    fn next_epoch(&mut self) -> u64 {
        self.last_epoch += 1;
        self.last_epoch
    }
}

#[derive(Debug, Clone, Default)]
pub struct RouteHealth {
    book: Arc<Mutex<Book>>,
    /// Told when a route that was out answers again, so work put off while
    /// every route was out can run.
    recovery: RecoverySignal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TicketKind {
    Use,
    Probe,
    /// Tried although the route is out, because nothing else was left. Its
    /// success clears the route; its failure says nothing new.
    Forced,
}

/// Permission to send one request to a route, and the duty to report how it
/// went. Dropping it unreported (the caller timed out or gave up) releases a
/// probe without counting against the route.
#[must_use = "report the outcome with succeeded() or failed()"]
pub(crate) struct Ticket {
    health: RouteHealth,
    route: RouteKey,
    epoch: u64,
    kind: TicketKind,
    settled: bool,
}

pub(crate) enum Admission {
    Use(Ticket),
    Probe(Ticket),
    Skip(RouteView),
}

impl RouteHealth {
    fn book(&self) -> MutexGuard<'_, Book> {
        self.book
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Decide, as one step under the lock, whether a request may go to
    /// `route`. Two callers can never both be handed the probe.
    pub(crate) fn admit(&self, route: impl Into<RouteKey>) -> Admission {
        let route = route.into();
        let now = Instant::now();
        let mut book = self.book();
        let fresh_epoch = book.next_epoch();
        let entry = book.entries.entry(route.clone()).or_insert(Entry {
            epoch: fresh_epoch,
            failures: 0,
            unhealthy: None,
            probing: false,
        });
        let view = entry.view(now);
        let kind = match view.state {
            RouteState::Ok => TicketKind::Use,
            RouteState::Recovering => {
                entry.probing = true;
                TicketKind::Probe
            }
            RouteState::Cooling | RouteState::Broken | RouteState::Probing => {
                return Admission::Skip(view)
            }
        };
        let ticket = self.ticket(route, entry.epoch, kind);
        match kind {
            TicketKind::Probe => Admission::Probe(ticket),
            _ => Admission::Use(ticket),
        }
    }

    /// A ticket for a route that is out, for the last resort when every
    /// route is.
    pub(crate) fn force(&self, route: impl Into<RouteKey>) -> Ticket {
        let route = route.into();
        let mut book = self.book();
        let fresh_epoch = book.next_epoch();
        let epoch = book
            .entries
            .get(&route)
            .map_or(fresh_epoch, |entry| entry.epoch);
        drop(book);
        self.ticket(route, epoch, TicketKind::Forced)
    }

    fn ticket(&self, route: RouteKey, epoch: u64, kind: TicketKind) -> Ticket {
        Ticket {
            health: self.clone(),
            route,
            epoch,
            kind,
            settled: false,
        }
    }

    pub fn view(&self, route: impl Into<RouteKey>) -> RouteView {
        let route = route.into();
        let now = Instant::now();
        self.book()
            .entries
            .get(&route)
            .map_or(RouteView::OK, |entry| entry.view(now))
    }

    pub(crate) fn recovery_signal(&self) -> &RecoverySignal {
        &self.recovery
    }

    /// Forget everything: the config changed, so what was learned about the
    /// old setup says nothing about the new one.
    pub fn reset(&self) {
        self.book().entries.clear();
    }

    /// Forget one provider's routes: its key was stored, replaced or
    /// removed, so a "bad key" verdict no longer applies.
    pub fn forget_provider(&self, provider: RouteProvider) {
        self.book().entries.retain(
            |route, _| !matches!(route, RouteKey::Hosted(route) if route.provider == provider),
        );
    }

    /// Record a failure as an HTTP reply with `status` would, so tests in
    /// other crates can put a route in a known state without a server.
    #[doc(hidden)]
    pub fn record_http_failure_for_test(&self, route: impl Into<RouteKey>, status: u16) {
        use crate::failure::{classify_status, ErrorClass};
        if let Admission::Use(ticket) | Admission::Probe(ticket) = self.admit(route) {
            if let ErrorClass::Route(failure) = classify_status(status, None, "") {
                ticket.failed(failure);
            }
        }
    }

    fn settle(&self, ticket: &Ticket, failure: Option<RouteFailure>) {
        let recovered = self.settle_book(ticket, failure);
        if recovered {
            self.recovery.notify();
        }
    }

    fn settle_book(&self, ticket: &Ticket, failure: Option<RouteFailure>) -> bool {
        let mut book = self.book();
        let next_epoch = book.next_epoch();
        let Some(entry) = book.entries.get_mut(&ticket.route) else {
            return false;
        };
        if entry.epoch != ticket.epoch {
            return false;
        }
        if ticket.kind == TicketKind::Probe {
            entry.probing = false;
        }
        match failure {
            None => {
                if entry.unhealthy.is_some() {
                    tracing::info!(
                        provider = ticket.route.provider_name(),
                        model = ticket.route.model(),
                        "LLM route recovered"
                    );
                    entry.unhealthy = None;
                    entry.failures = 0;
                    entry.probing = false;
                    entry.epoch = next_epoch;
                    return true;
                }
            }
            Some(_) if ticket.kind == TicketKind::Forced => {}
            Some(failure) => {
                entry.failures = entry.failures.saturating_add(1);
                let (broken, reason, delay) = match failure {
                    RouteFailure::Cooling {
                        reason,
                        retry_after,
                    } => (
                        false,
                        reason,
                        retry_after.map_or_else(
                            || cooling_backoff(entry.failures),
                            |after| after.clamp(RETRY_AFTER_MIN, RETRY_AFTER_MAX),
                        ),
                    ),
                    RouteFailure::Broken { reason } => (true, reason, BROKEN_TTL),
                };
                entry.unhealthy = Some(Unhealthy {
                    broken,
                    until: Instant::now() + delay,
                    reason,
                });
                entry.epoch = next_epoch;
                tracing::warn!(
                    provider = ticket.route.provider_name(),
                    model = ticket.route.model(),
                    %reason,
                    retry_secs = delay.as_secs(),
                    broken,
                    "LLM route is out"
                );
            }
        }
        false
    }

    fn release_probe(&self, ticket: &Ticket) {
        let mut book = self.book();
        if let Some(entry) = book.entries.get_mut(&ticket.route) {
            if entry.epoch == ticket.epoch {
                entry.probing = false;
            }
        }
    }
}

fn cooling_backoff(failures: u32) -> Duration {
    let doublings = failures.saturating_sub(1).min(16);
    COOLING_MIN
        .saturating_mul(1u32 << doublings)
        .min(COOLING_MAX)
}

impl Ticket {
    pub(crate) fn succeeded(mut self) {
        self.settled = true;
        self.health.settle(&self, None);
    }

    pub(crate) fn failed(mut self, failure: RouteFailure) {
        self.settled = true;
        self.health.settle(&self, Some(failure));
    }
}

impl Drop for Ticket {
    fn drop(&mut self) {
        if !self.settled && self.kind == TicketKind::Probe {
            self.health.release_probe(self);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::route::Route;

    fn route(provider: RouteProvider) -> Route {
        Route {
            provider,
            model: "m".into(),
        }
    }

    fn anthropic() -> Route {
        route(RouteProvider::Anthropic)
    }

    const RATE_LIMITED: FailureReason = FailureReason::status(429, "rate limited");
    const BAD_KEY: FailureReason = FailureReason::status(401, "invalid key");

    fn cooling() -> RouteFailure {
        RouteFailure::Cooling {
            reason: RATE_LIMITED,
            retry_after: None,
        }
    }

    fn ticket(admission: Admission) -> Ticket {
        match admission {
            Admission::Use(ticket) | Admission::Probe(ticket) => ticket,
            Admission::Skip(view) => panic!("expected a ticket, route is {view:?}"),
        }
    }

    fn fail(health: &RouteHealth, failure: RouteFailure) {
        ticket(health.admit(&anthropic())).failed(failure);
    }

    #[tokio::test(start_paused = true)]
    async fn a_cooling_route_is_skipped_until_its_wait_is_over() {
        let health = RouteHealth::default();
        fail(&health, cooling());
        let Admission::Skip(view) = health.admit(&anthropic()) else {
            panic!("a cooling route must be skipped");
        };
        assert_eq!(view.state, RouteState::Cooling);
        assert_eq!(view.reason, Some(RATE_LIMITED));
        assert_eq!(view.retry_in, COOLING_MIN);

        tokio::time::advance(COOLING_MIN - Duration::from_secs(1)).await;
        assert!(matches!(health.admit(&anthropic()), Admission::Skip(_)));
        tokio::time::advance(Duration::from_secs(1)).await;
        assert_eq!(health.view(&anthropic()).state, RouteState::Recovering);
        assert!(matches!(health.admit(&anthropic()), Admission::Probe(_)));
    }

    #[tokio::test(start_paused = true)]
    async fn a_broken_route_waits_an_hour_and_says_why() {
        let health = RouteHealth::default();
        fail(&health, RouteFailure::Broken { reason: BAD_KEY });
        let view = health.view(&anthropic());
        assert_eq!(view.state, RouteState::Broken);
        assert_eq!(view.reason.unwrap().to_string(), "401 invalid key");
        assert_eq!(view.retry_in, BROKEN_TTL);

        tokio::time::advance(BROKEN_TTL).await;
        assert!(matches!(health.admit(&anthropic()), Admission::Probe(_)));
    }

    #[tokio::test(start_paused = true)]
    async fn a_successful_probe_resets_the_route_and_its_backoff() {
        let health = RouteHealth::default();
        fail(&health, cooling());
        tokio::time::advance(COOLING_MAX).await;
        let Admission::Probe(probe) = health.admit(&anthropic()) else {
            panic!("expected the probe");
        };
        probe.failed(cooling());
        tokio::time::advance(COOLING_MAX).await;
        let Admission::Probe(probe) = health.admit(&anthropic()) else {
            panic!("expected the probe");
        };
        probe.succeeded();
        assert_eq!(health.view(&anthropic()), RouteView::OK);
        assert!(matches!(health.admit(&anthropic()), Admission::Use(_)));

        fail(&health, cooling());
        assert_eq!(health.view(&anthropic()).retry_in, COOLING_MIN);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_callers_after_the_wait_share_exactly_one_probe() {
        let health = RouteHealth::default();
        fail(
            &health,
            RouteFailure::Cooling {
                reason: RATE_LIMITED,
                retry_after: Some(RETRY_AFTER_MIN),
            },
        );
        tokio::time::sleep(RETRY_AFTER_MIN + Duration::from_millis(50)).await;

        let barrier = Arc::new(tokio::sync::Barrier::new(32));
        let mut tasks = Vec::new();
        for _ in 0..32 {
            let health = health.clone();
            let barrier = barrier.clone();
            tasks.push(tokio::spawn(async move {
                barrier.wait().await;
                match health.admit(&anthropic()) {
                    Admission::Probe(probe) => {
                        tokio::task::yield_now().await;
                        Some(probe)
                    }
                    Admission::Use(_) => panic!("a recovering route is not healthy"),
                    Admission::Skip(_) => None,
                }
            }));
        }
        let mut held = Vec::new();
        for task in tasks {
            held.push(task.await.unwrap());
        }
        assert_eq!(held.iter().filter(|ticket| ticket.is_some()).count(), 1);
    }
}
