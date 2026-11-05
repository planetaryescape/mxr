//! Runs jobs in helper processes and survives their failure.
//!
//! The supervisor owns a small pool of helpers, started lazily. A request
//! leases one helper, sends a frame and waits for the reply under a wall-clock
//! timeout. A helper that times out or dies is killed and reaped, the request
//! gets the caller's fallback, and the next request starts a fresh helper.
//!
//! Callers block. Every parse in mxr already ran on the calling thread; moving
//! it to a child process keeps that shape, and on a multi-thread tokio runtime
//! the wait is wrapped in `block_in_place` so it does not stall other tasks.
//! Requests beyond the pool size queue for a free helper.

use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashSet, VecDeque};
use std::ffi::OsString;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use crate::wire::{self, Status};

/// Names of the environment variables that carry the limits to the helper.
pub(crate) const ENV_MEMORY_HEADROOM: &str = "MXR_HTML_WORKER_MEMORY_BYTES";
pub(crate) const ENV_CPU_SECONDS: &str = "MXR_HTML_WORKER_CPU_SECONDS";

/// How many failing inputs the supervisor remembers. A flood of distinct
/// hostile messages must not grow the set without bound.
const POISON_CAPACITY: usize = 256;

/// After a helper fails to start, requests fall back for this long before the
/// next attempt, so a missing binary costs one failed spawn per interval and
/// not one per message.
const SPAWN_BACKOFF: Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
pub struct Config {
    /// The helper executable. In mxr this is the `mxr` binary itself.
    pub program: PathBuf,
    /// Arguments that put that executable in helper mode.
    pub args: Vec<OsString>,
    /// Wall-clock limit for one request, measured from the send. Ordinary mail
    /// converts in milliseconds; this only has to be longer than the slowest
    /// legitimate document and short enough that a person waiting on a hostile
    /// one is not left hanging.
    pub request_timeout: Duration,
    /// How long a new helper has to report ready.
    pub startup_timeout: Duration,
    /// Helpers running at once. Requests beyond this queue.
    pub max_helpers: usize,
    /// Memory a helper may add on top of what it uses once started.
    pub memory_headroom_bytes: u64,
    /// CPU time a helper may spend on one request before the OS stops it. A
    /// backstop behind `request_timeout`, which the supervisor enforces.
    pub cpu_seconds_per_request: u64,
}

impl Config {
    pub fn new(program: impl Into<PathBuf>, args: Vec<OsString>) -> Self {
        Self {
            program: program.into(),
            args,
            request_timeout: Duration::from_secs(5),
            startup_timeout: Duration::from_secs(10),
            max_helpers: 2,
            memory_headroom_bytes: 512 * 1024 * 1024,
            cpu_seconds_per_request: 8,
        }
    }
}

/// Counts since the process started. They hold no message content.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stats {
    /// A helper is installed in this process. When false the other fields are
    /// zero and parsing runs in-process.
    pub installed: bool,
    pub requests: u64,
    /// Helpers started to replace one that timed out or died.
    pub restarts: u64,
    pub timeouts: u64,
    /// Helpers that exited or were killed by the OS mid-request or while idle.
    pub deaths: u64,
    /// Jobs that panicked or whose result could not be decoded.
    pub job_errors: u64,
    /// Requests answered with the caller's fallback instead of a result.
    pub fallbacks: u64,
}

#[derive(Default)]
struct Counters {
    requests: AtomicU64,
    restarts: AtomicU64,
    timeouts: AtomicU64,
    deaths: AtomicU64,
    job_errors: AtomicU64,
    fallbacks: AtomicU64,
}

/// Why a request got no result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackReason {
    /// The document is larger than the job accepts.
    Oversize,
    /// This exact input already timed out or killed a helper.
    KnownBad,
    /// No helper could be started.
    Unavailable,
    Timeout,
    /// The helper died on this input, again after a retry on a fresh helper.
    HelperDied,
    /// The job panicked or returned something undecodable.
    JobFailed,
}

#[derive(Debug)]
pub enum Outcome {
    Done(Vec<u8>),
    Fallback(FallbackReason),
}

#[derive(Default)]
struct Poison {
    seen: HashSet<u64>,
    order: VecDeque<u64>,
}

impl Poison {
    fn insert(&mut self, key: u64) {
        if self.seen.insert(key) {
            self.order.push_back(key);
            if self.order.len() > POISON_CAPACITY {
                if let Some(oldest) = self.order.pop_front() {
                    self.seen.remove(&oldest);
                }
            }
        }
    }
}

struct Pool {
    idle: Vec<Helper>,
    /// Idle, leased and starting helpers.
    live: usize,
    /// Helpers lost (timed out or died) and not yet replaced, so the next
    /// start counts as a restart.
    awaiting_replacement: u64,
}

pub struct Supervisor {
    config: Config,
    pool: Mutex<Pool>,
    available: Condvar,
    poison: Mutex<Poison>,
    counters: Counters,
    next_id: AtomicU64,
    spawn_retry_at: Mutex<Option<Instant>>,
}

static GLOBAL: OnceLock<Arc<Supervisor>> = OnceLock::new();

thread_local! {
    static SCOPED: RefCell<Option<Arc<Supervisor>>> = const { RefCell::new(None) };
}

/// Runs `body` with `supervisor` standing in for the installed helper on this
/// thread. For tests that exercise the production fallbacks end to end without
/// installing a process-wide helper that every other test would also see.
pub fn with_supervisor<T>(supervisor: Arc<Supervisor>, body: impl FnOnce() -> T) -> T {
    struct Restore(Option<Arc<Supervisor>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            SCOPED.with(|scoped| *scoped.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(SCOPED.with(|scoped| scoped.replace(Some(supervisor))));
    body()
}

/// Makes `config`'s helper the one every [`Job::call`](crate::Job::call) in
/// this process uses. Nothing is started until the first request. Returns
/// false if a helper was already installed.
pub fn install(config: Config) -> bool {
    GLOBAL.set(Arc::new(Supervisor::new(config))).is_ok()
}

/// The supervisor a call on this thread uses: a scoped one, else the installed
/// one.
pub(crate) fn current() -> Option<Arc<Supervisor>> {
    SCOPED
        .with(|scoped| scoped.borrow().clone())
        .or_else(|| GLOBAL.get().cloned())
}

/// Counts for this process's installed helper, or all zero when there is none.
pub fn stats() -> Stats {
    GLOBAL
        .get()
        .map(|supervisor| supervisor.stats())
        .unwrap_or_default()
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic elsewhere must not turn every later parse into a fallback.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Supervisor {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            pool: Mutex::new(Pool {
                idle: Vec::new(),
                live: 0,
                awaiting_replacement: 0,
            }),
            available: Condvar::new(),
            poison: Mutex::new(Poison::default()),
            counters: Counters::default(),
            next_id: AtomicU64::new(1),
            spawn_retry_at: Mutex::new(None),
        }
    }

    pub fn stats(&self) -> Stats {
        let read = |counter: &AtomicU64| counter.load(Ordering::Relaxed);
        Stats {
            installed: true,
            requests: read(&self.counters.requests),
            restarts: read(&self.counters.restarts),
            timeouts: read(&self.counters.timeouts),
            deaths: read(&self.counters.deaths),
            job_errors: read(&self.counters.job_errors),
            fallbacks: read(&self.counters.fallbacks),
        }
    }

    pub(crate) fn note_undecodable_result(&self) {
        self.counters.job_errors.fetch_add(1, Ordering::Relaxed);
        self.counters.fallbacks.fetch_add(1, Ordering::Relaxed);
    }

    /// Runs `kind` on `html` in a helper. A document over `max_html_bytes` is
    /// not sent and gets the fallback.
    pub fn run(&self, kind: &str, html: &[u8], params: &[u8], max_html_bytes: usize) -> Outcome {
        blocking(|| self.run_blocking(kind, html, params, max_html_bytes))
    }

    fn run_blocking(
        &self,
        kind: &str,
        html: &[u8],
        params: &[u8],
        max_html_bytes: usize,
    ) -> Outcome {
        self.counters.requests.fetch_add(1, Ordering::Relaxed);
        if html.len() > max_html_bytes {
            return self.fallback(kind, FallbackReason::Oversize);
        }
        // Hashing a large document costs a millisecond or two, so the set is
        // only consulted once it has anything in it.
        if self.is_known_bad(kind, params, html) {
            return self.fallback(kind, FallbackReason::KnownBad);
        }

        let mut deaths = 0;
        loop {
            let Ok(mut helper) = self.acquire() else {
                return self.fallback(kind, FallbackReason::Unavailable);
            };
            let id = self.next_id.fetch_add(1, Ordering::Relaxed);
            match helper.exchange(id, kind, params, html, self.config.request_timeout) {
                Exchange::Done(bytes) => {
                    self.release(helper);
                    return Outcome::Done(bytes);
                }
                Exchange::JobFailed(message) => {
                    self.release(helper);
                    self.counters.job_errors.fetch_add(1, Ordering::Relaxed);
                    tracing::warn!(kind, message, "html helper job failed; using the fallback");
                    return self.fallback(kind, FallbackReason::JobFailed);
                }
                Exchange::Timeout => {
                    self.retire(helper);
                    self.counters.timeouts.fetch_add(1, Ordering::Relaxed);
                    // Slowness is a property of the input, so a retry would
                    // spend the whole timeout again.
                    lock(&self.poison).insert(fingerprint(kind, params, html));
                    tracing::warn!(
                        kind,
                        timeout_ms = u64::try_from(self.config.request_timeout.as_millis())
                            .unwrap_or(u64::MAX),
                        "html helper timed out and was stopped; using the fallback"
                    );
                    return self.fallback(kind, FallbackReason::Timeout);
                }
                Exchange::Died => {
                    self.retire(helper);
                    self.counters.deaths.fetch_add(1, Ordering::Relaxed);
                    deaths += 1;
                    // One retry on a fresh helper tells a bad input from a
                    // helper the OS killed for some other reason. Dying twice
                    // is the input, and it is remembered so it is not tried
                    // again.
                    if deaths >= 2 {
                        lock(&self.poison).insert(fingerprint(kind, params, html));
                        tracing::warn!(
                            kind,
                            "html helper died on the same input twice; using the fallback"
                        );
                        return self.fallback(kind, FallbackReason::HelperDied);
                    }
                }
            }
        }
    }

    fn is_known_bad(&self, kind: &str, params: &[u8], html: &[u8]) -> bool {
        let poison = lock(&self.poison);
        !poison.seen.is_empty() && poison.seen.contains(&fingerprint(kind, params, html))
    }

    fn fallback(&self, kind: &str, reason: FallbackReason) -> Outcome {
        self.counters.fallbacks.fetch_add(1, Ordering::Relaxed);
        tracing::debug!(
            kind,
            ?reason,
            "html helper request answered with the fallback"
        );
        Outcome::Fallback(reason)
    }

    fn acquire(&self) -> Result<Helper, ()> {
        let mut pool = lock(&self.pool);
        loop {
            while let Some(mut helper) = pool.idle.pop() {
                if helper.is_alive() {
                    return Ok(helper);
                }
                // It died while idle. The slot is free to start another.
                pool.live -= 1;
                pool.awaiting_replacement += 1;
                self.counters.deaths.fetch_add(1, Ordering::Relaxed);
            }
            if pool.live < self.config.max_helpers {
                if lock(&self.spawn_retry_at).is_some_and(|until| Instant::now() < until) {
                    return Err(());
                }
                pool.live += 1;
                drop(pool);
                return match Helper::spawn(&self.config) {
                    Ok(helper) => {
                        let mut pool = lock(&self.pool);
                        if pool.awaiting_replacement > 0 {
                            pool.awaiting_replacement -= 1;
                            self.counters.restarts.fetch_add(1, Ordering::Relaxed);
                        }
                        Ok(helper)
                    }
                    Err(error) => {
                        tracing::warn!(%error, "html helper could not be started; parsing falls back for a while");
                        *lock(&self.spawn_retry_at) = Some(Instant::now() + SPAWN_BACKOFF);
                        lock(&self.pool).live -= 1;
                        // Every waiter, not one: each of them would find the
                        // same backoff and give up, but only if it wakes.
                        self.available.notify_all();
                        Err(())
                    }
                };
            }
            pool = self
                .available
                .wait(pool)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    fn release(&self, helper: Helper) {
        lock(&self.pool).idle.push(helper);
        self.available.notify_one();
    }

    /// Kills and reaps a helper that failed, and frees its slot.
    fn retire(&self, helper: Helper) {
        drop(helper);
        let mut pool = lock(&self.pool);
        pool.live -= 1;
        pool.awaiting_replacement += 1;
        drop(pool);
        self.available.notify_one();
    }
}

/// Runs `wait` so that it does not hold up a tokio worker thread.
fn blocking<T>(wait: impl FnOnce() -> T) -> T {
    use tokio::runtime::{Handle, RuntimeFlavor};
    match Handle::try_current() {
        // `block_in_place` panics on a current-thread runtime, where blocking
        // is the only option anyway (single-threaded test runtimes).
        Ok(handle) if handle.runtime_flavor() == RuntimeFlavor::MultiThread => {
            tokio::task::block_in_place(wait)
        }
        _ => wait(),
    }
}

fn fingerprint(kind: &str, params: &[u8], html: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    kind.hash(&mut hasher);
    params.hash(&mut hasher);
    html.hash(&mut hasher);
    hasher.finish()
}

enum Exchange {
    Done(Vec<u8>),
    JobFailed(String),
    Timeout,
    Died,
}

enum Reply {
    Frame(Vec<u8>),
    Closed,
}

struct Helper {
    child: Child,
    requests: Sender<Vec<u8>>,
    replies: Receiver<Reply>,
}

impl Helper {
    fn spawn(config: &Config) -> io::Result<Self> {
        let mut command = Command::new(&config.program);
        command
            .args(&config.args)
            // The helper needs nothing from the environment but its limits, and
            // a parser has no business reading anything else.
            .env_clear()
            .env(
                ENV_MEMORY_HEADROOM,
                config.memory_headroom_bytes.to_string(),
            )
            .env(ENV_CPU_SECONDS, config.cpu_seconds_per_request.to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        // Its own process group, so a Ctrl-C, Ctrl-Z or hangup aimed at the
        // terminal's foreground group reaches the parent and not the helper.
        // The helper ends when its stdin closes, which is what a dying parent
        // does.
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        let mut child = command.spawn()?;
        let (Some(mut stdin), Some(mut stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::other("helper pipes were not created"));
        };
        let (request_tx, request_rx) = mpsc::channel::<Vec<u8>>();
        let (reply_tx, reply_rx) = mpsc::channel::<Reply>();
        // One thread per helper does the pipe I/O so the caller can wait with a
        // timeout. It ends when the helper's pipes close (the helper was killed
        // or exited) or when the requests channel is dropped.
        let spawned = std::thread::Builder::new()
            .name("html-helper-io".to_string())
            .spawn(move || {
                match wire::read_frame(&mut stdout) {
                    Ok(Some(frame)) => {
                        let _ = reply_tx.send(Reply::Frame(frame));
                    }
                    _ => {
                        let _ = reply_tx.send(Reply::Closed);
                        return;
                    }
                }
                while let Ok(request) = request_rx.recv() {
                    let reply = match wire::write_frame(&mut stdin, &request)
                        .and_then(|()| wire::read_frame(&mut stdout))
                    {
                        Ok(Some(frame)) => Reply::Frame(frame),
                        _ => Reply::Closed,
                    };
                    let closed = matches!(reply, Reply::Closed);
                    if reply_tx.send(reply).is_err() || closed {
                        return;
                    }
                }
            });
        if let Err(error) = spawned {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        let helper = Self {
            child,
            requests: request_tx,
            replies: reply_rx,
        };
        match helper.replies.recv_timeout(config.startup_timeout) {
            Ok(Reply::Frame(frame)) => {
                let ready = wire::decode_response(&frame).is_ok_and(|reply| {
                    reply.status == Status::Ready && reply.payload == wire::READY
                });
                if !ready {
                    return Err(io::Error::other("helper sent an unexpected handshake"));
                }
            }
            Ok(Reply::Closed) | Err(RecvTimeoutError::Disconnected) => {
                return Err(io::Error::other("helper exited before it was ready"));
            }
            Err(RecvTimeoutError::Timeout) => {
                return Err(io::Error::other("helper did not become ready in time"));
            }
        }
        Ok(helper)
    }

    fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    fn exchange(
        &mut self,
        id: u64,
        kind: &str,
        params: &[u8],
        html: &[u8],
        timeout: Duration,
    ) -> Exchange {
        if self
            .requests
            .send(wire::encode_request(id, kind, params, html))
            .is_err()
        {
            return Exchange::Died;
        }
        let frame = match self.replies.recv_timeout(timeout) {
            Ok(Reply::Frame(frame)) => frame,
            Ok(Reply::Closed) | Err(RecvTimeoutError::Disconnected) => return Exchange::Died,
            Err(RecvTimeoutError::Timeout) => return Exchange::Timeout,
        };
        let Ok(reply) = wire::decode_response(&frame) else {
            return Exchange::Died;
        };
        if reply.id != id {
            return Exchange::Died;
        }
        match reply.status {
            Status::Ok => Exchange::Done(reply.payload.to_vec()),
            Status::JobError | Status::UnknownKind => {
                Exchange::JobFailed(String::from_utf8_lossy(reply.payload).into_owned())
            }
            Status::Ready => Exchange::Died,
        }
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        // Killing unblocks the I/O thread (its pipes close) and reaps the
        // child so no zombie outlives the request that failed.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
