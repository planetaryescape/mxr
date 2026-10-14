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

use crate::route::{Route, RouteProvider};
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tokio::time::Instant;

/// First cooling period; doubles on each further failure.
pub(crate) const COOLING_MIN: Duration = Duration::from_secs(60);
pub(crate) const COOLING_MAX: Duration = Duration::from_secs(30 * 60);
/// A route that is broken until a key or the config changes is still
/// retried this often, in case the change happened somewhere mxr can't see
/// (a key rotated at the provider, a model that became available).
pub(crate) const BROKEN_TTL: Duration = Duration::from_secs(60 * 60);
/// A `retry-after` longer than this is not honoured in full.
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

/// A route-level failure: the route itself is the problem, not the request.
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
    /// The wait is over; the next request will probe the route.
    Recovering,
    /// A probe is in flight.
    Probing,
}

/// A route's health at one moment, for choosing and for display.
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
    entries: HashMap<Route, Entry>,
}

impl Book {
    fn next_epoch(&mut self) -> u64 {
        self.last_epoch += 1;
        self.last_epoch
    }
}

/// Shared health of every route. Cheap to clone; clones share state.
#[derive(Debug, Clone, Default)]
pub struct RouteHealth {
    book: Arc<Mutex<Book>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TicketKind {
    /// The route looked healthy.
    Use,
    /// The route's wait was over and this request is the one trying it.
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
    route: Route,
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
    pub(crate) fn admit(&self, route: &Route) -> Admission {
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
    pub(crate) fn force(&self, route: &Route) -> Ticket {
        let mut book = self.book();
        let fresh_epoch = book.next_epoch();
        let epoch = book
            .entries
            .get(route)
            .map_or(fresh_epoch, |entry| entry.epoch);
        self.ticket(route, epoch, TicketKind::Forced)
    }

    fn ticket(&self, route: &Route, epoch: u64, kind: TicketKind) -> Ticket {
        Ticket {
            health: self.clone(),
            route: route.clone(),
            epoch,
            kind,
            settled: false,
        }
    }

    pub fn view(&self, route: &Route) -> RouteView {
        let now = Instant::now();
        self.book()
            .entries
            .get(route)
            .map_or(RouteView::OK, |entry| entry.view(now))
    }

    /// Forget everything: the config changed, so what was learned about the
    /// old setup says nothing about the new one.
    pub fn reset(&self) {
        self.book().entries.clear();
    }

    /// Forget one provider's routes: its key was stored, replaced or
    /// removed, so a "bad key" verdict no longer applies.
    pub fn forget_provider(&self, provider: RouteProvider) {
        self.book()
            .entries
            .retain(|route, _| route.provider != provider);
    }

    /// Record a failure as an HTTP reply with `status` would, so tests in
    /// other crates can put a route in a known state without a server.
    #[doc(hidden)]
    pub fn record_http_failure_for_test(&self, route: &Route, status: u16) {
        use crate::failure::{classify_status, ErrorClass};
        if let Admission::Use(ticket) | Admission::Probe(ticket) = self.admit(route) {
            if let ErrorClass::Route(failure) = classify_status(status, None, "") {
                ticket.failed(failure);
            }
        }
    }

    fn settle(&self, ticket: &Ticket, failure: Option<RouteFailure>) {
        let mut book = self.book();
        let next_epoch = book.next_epoch();
        let Some(entry) = book.entries.get_mut(&ticket.route) else {
            return;
        };
        if entry.epoch != ticket.epoch {
            return;
        }
        if ticket.kind == TicketKind::Probe {
            entry.probing = false;
        }
        match failure {
            None => {
                if entry.unhealthy.is_some() {
                    tracing::info!(
                        provider = %ticket.route.provider,
                        model = %ticket.route.model,
                        "LLM route recovered"
                    );
                    entry.unhealthy = None;
                    entry.failures = 0;
                    entry.probing = false;
                    entry.epoch = next_epoch;
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
                    provider = %ticket.route.provider,
                    model = %ticket.route.model,
                    %reason,
                    retry_secs = delay.as_secs(),
                    broken,
                    "LLM route is out"
                );
            }
        }
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

/// 1 min for the first failure, doubling to 30 min.
fn cooling_backoff(failures: u32) -> Duration {
    let doublings = failures.saturating_sub(1).min(16);
    COOLING_MIN
        .saturating_mul(1u32 << doublings)
        .min(COOLING_MAX)
}

impl Ticket {
    /// The route answered (even if the answer was a refusal): healthy.
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

    #[test]
    fn backoff_doubles_from_a_minute_to_half_an_hour() {
        let minutes = |n: u64| Duration::from_secs(n * 60);
        let steps: Vec<_> = (1..=7).map(cooling_backoff).collect();
        assert_eq!(
            steps,
            [
                minutes(1),
                minutes(2),
                minutes(4),
                minutes(8),
                minutes(16),
                minutes(30),
                minutes(30)
            ]
        );
        assert_eq!(cooling_backoff(u32::MAX), minutes(30));
    }

    #[tokio::test(start_paused = true)]
    async fn an_unknown_route_is_used() {
        let health = RouteHealth::default();
        assert!(matches!(health.admit(&anthropic()), Admission::Use(_)));
        assert_eq!(health.view(&anthropic()), RouteView::OK);
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
    async fn each_failed_probe_doubles_the_wait() {
        let health = RouteHealth::default();
        fail(&health, cooling());
        for expected in [2, 4, 8, 16, 30, 30] {
            tokio::time::advance(COOLING_MAX).await;
            let Admission::Probe(probe) = health.admit(&anthropic()) else {
                panic!("expected the probe");
            };
            probe.failed(cooling());
            assert_eq!(
                health.view(&anthropic()).retry_in,
                Duration::from_secs(expected * 60)
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn retry_after_replaces_the_backoff_within_limits() {
        let health = RouteHealth::default();
        let after = |secs| RouteFailure::Cooling {
            reason: RATE_LIMITED,
            retry_after: Some(Duration::from_secs(secs)),
        };
        fail(&health, after(7));
        assert_eq!(health.view(&anthropic()).retry_in, Duration::from_secs(7));

        health.reset();
        fail(&health, after(0));
        assert_eq!(health.view(&anthropic()).retry_in, RETRY_AFTER_MIN);

        health.reset();
        fail(&health, after(86_400));
        assert_eq!(health.view(&anthropic()).retry_in, RETRY_AFTER_MAX);
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

        // The next failure starts again from a minute.
        fail(&health, cooling());
        assert_eq!(health.view(&anthropic()).retry_in, COOLING_MIN);
    }

    #[tokio::test(start_paused = true)]
    async fn the_probe_is_exclusive_until_it_reports() {
        let health = RouteHealth::default();
        fail(&health, cooling());
        tokio::time::advance(COOLING_MIN).await;

        let Admission::Probe(probe) = health.admit(&anthropic()) else {
            panic!("expected the probe");
        };
        for _ in 0..5 {
            let Admission::Skip(view) = health.admit(&anthropic()) else {
                panic!("only one probe at a time");
            };
            assert_eq!(view.state, RouteState::Probing);
        }
        probe.succeeded();
        assert!(matches!(health.admit(&anthropic()), Admission::Use(_)));
    }

    #[tokio::test(start_paused = true)]
    async fn a_probe_dropped_without_a_result_is_released_not_punished() {
        let health = RouteHealth::default();
        fail(&health, cooling());
        tokio::time::advance(COOLING_MIN).await;

        drop(health.admit(&anthropic()));
        let view = health.view(&anthropic());
        assert_eq!(view.state, RouteState::Recovering);
        // Same failure count: the next failure is the second, not the third.
        assert!(matches!(health.admit(&anthropic()), Admission::Probe(_)));
    }

    #[tokio::test(start_paused = true)]
    async fn failures_from_requests_already_in_flight_do_not_escalate_the_backoff() {
        let health = RouteHealth::default();
        // Eight requests were admitted while the route looked healthy.
        let tickets: Vec<_> = (0..8).map(|_| ticket(health.admit(&anthropic()))).collect();
        for ticket in tickets {
            ticket.failed(cooling());
        }
        assert_eq!(health.view(&anthropic()).retry_in, COOLING_MIN);
    }

    #[tokio::test(start_paused = true)]
    async fn a_success_from_before_the_failure_does_not_undo_it() {
        let health = RouteHealth::default();
        let early = ticket(health.admit(&anthropic()));
        fail(&health, cooling());
        early.succeeded();
        assert_eq!(health.view(&anthropic()).state, RouteState::Cooling);
    }

    #[tokio::test(start_paused = true)]
    async fn outcomes_that_arrive_after_a_reset_are_ignored() {
        let health = RouteHealth::default();
        let in_flight = ticket(health.admit(&anthropic()));
        health.reset();
        in_flight.failed(RouteFailure::Broken { reason: BAD_KEY });
        assert_eq!(health.view(&anthropic()), RouteView::OK);
    }

    #[tokio::test(start_paused = true)]
    async fn a_new_key_clears_only_that_providers_verdicts() {
        let health = RouteHealth::default();
        let openrouter = route(RouteProvider::OpenRouter);
        fail(&health, RouteFailure::Broken { reason: BAD_KEY });
        ticket(health.admit(&openrouter)).failed(cooling());

        // A request to Anthropic is in flight when the key changes.
        health.forget_provider(RouteProvider::Anthropic);
        assert_eq!(health.view(&anthropic()), RouteView::OK);
        assert_eq!(health.view(&openrouter).state, RouteState::Cooling);
    }

    #[tokio::test(start_paused = true)]
    async fn a_forgotten_providers_in_flight_failure_is_dropped() {
        let health = RouteHealth::default();
        let in_flight = ticket(health.admit(&anthropic()));
        health.forget_provider(RouteProvider::Anthropic);
        in_flight.failed(RouteFailure::Broken { reason: BAD_KEY });
        assert_eq!(health.view(&anthropic()), RouteView::OK);
    }

    #[tokio::test(start_paused = true)]
    async fn a_reset_does_not_strand_another_providers_probe() {
        let health = RouteHealth::default();
        let openrouter = route(RouteProvider::OpenRouter);
        ticket(health.admit(&openrouter)).failed(cooling());
        tokio::time::advance(COOLING_MIN).await;
        let Admission::Probe(probe) = health.admit(&openrouter) else {
            panic!("expected the probe");
        };
        health.forget_provider(RouteProvider::Anthropic);
        probe.succeeded();
        assert_eq!(health.view(&openrouter), RouteView::OK);
    }

    #[tokio::test(start_paused = true)]
    async fn a_forced_attempt_clears_the_route_on_success_and_changes_nothing_on_failure() {
        let health = RouteHealth::default();
        fail(&health, cooling());
        health.force(&anthropic()).failed(cooling());
        assert_eq!(health.view(&anthropic()).retry_in, COOLING_MIN);

        health.force(&anthropic()).succeeded();
        assert_eq!(health.view(&anthropic()), RouteView::OK);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_callers_after_the_wait_share_exactly_one_probe() {
        // Real time: `pause` needs a single-threaded runtime, and this test is
        // about callers racing on separate threads.
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
                        // Hold the probe while the others have all asked.
                        tokio::task::yield_now().await;
                        Some(probe)
                    }
                    Admission::Use(_) => panic!("a recovering route is not healthy"),
                    Admission::Skip(_) => None,
                }
            }));
        }
        // Keep every ticket alive until all callers have asked: dropping an
        // unsettled probe frees it, so a caller still asking could then get a
        // second one, which is correct but not what this test measures.
        let mut held = Vec::new();
        for task in tasks {
            held.push(task.await.unwrap());
        }
        assert_eq!(held.iter().filter(|ticket| ticket.is_some()).count(), 1);
    }
}
