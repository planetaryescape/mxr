//! HTML parses kept off the TUI's event loop.
//!
//! Showing a message takes parses that run in the HTML helper process: the
//! reader's text render, the quote split and the styled render. The loop used
//! to wait for each, so a hostile message held the whole TUI until the
//! helper's 5 s timeout. Once [`install`] has a way to wake the loop, an answer
//! that is not cached is worked out on a thread of its own. The caller waits up
//! to [`FIRST_WAIT`], which ordinary mail beats, then draws its stand-in, and
//! the loop is woken to draw again when the answer lands.
//!
//! Without an install (unit tests, and any use outside the running app) the
//! answer is worked out inline, as before.

use std::any::{Any, TypeId};
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::{mpsc, Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::Duration;

/// How long a frame waits for a new answer before drawing the stand-in. Long
/// enough that a normal message never flashes its stand-in, short enough that
/// a hostile one costs a keypress a barely noticeable pause.
const FIRST_WAIT: Duration = Duration::from_millis(50);

/// Answers kept. A thread view asks for one per message per frame, so this
/// covers a long thread and the messages either side of it.
const CACHED: usize = 64;

type Answer = Arc<dyn Any + Send + Sync>;

static INSTALLED: OnceLock<Deferred> = OnceLock::new();

/// From now on, answers are worked out off the calling thread, and `wake` is
/// called when one lands after its caller stopped waiting. Only the first
/// install counts.
pub(crate) fn install(wake: impl Fn() + Send + Sync + 'static) {
    let _ = INSTALLED.set(Deferred::new(wake, FIRST_WAIT));
}

/// The answer for `key`, or `None` while it is being worked out. `key` must
/// cover every input `compute` reads.
pub(crate) fn answer<T, K, F>(key: &K, compute: F) -> Option<Arc<T>>
where
    T: Send + Sync + 'static,
    K: Hash + ?Sized,
    F: FnOnce() -> T + Send + 'static,
{
    match INSTALLED.get() {
        Some(deferred) => deferred.answer(key, compute),
        None => Some(Arc::new(compute())),
    }
}

enum Slot {
    Pending,
    Ready(Answer),
}

#[derive(Default)]
struct Cache {
    slots: HashMap<u64, Slot>,
    order: VecDeque<u64>,
}

impl Cache {
    fn insert(&mut self, key: u64, slot: Slot) {
        if self.slots.insert(key, slot).is_none() {
            self.order.push_back(key);
        }
        while self.order.len() > CACHED {
            if let Some(oldest) = self.order.pop_front() {
                // An evicted pending answer is stored again when it lands.
                self.slots.remove(&oldest);
            }
        }
    }

    fn ready<T: Send + Sync + 'static>(&self, key: u64) -> Option<Arc<T>> {
        match self.slots.get(&key) {
            Some(Slot::Ready(answer)) => Arc::clone(answer).downcast().ok(),
            _ => None,
        }
    }
}

struct Shared {
    cache: Mutex<Cache>,
    wake: Box<dyn Fn() + Send + Sync>,
}

impl Shared {
    fn cache(&self) -> MutexGuard<'_, Cache> {
        self.cache.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

struct Deferred {
    shared: Arc<Shared>,
    first_wait: Duration,
}

impl Deferred {
    fn new(wake: impl Fn() + Send + Sync + 'static, first_wait: Duration) -> Self {
        Self {
            shared: Arc::new(Shared {
                cache: Mutex::default(),
                wake: Box::new(wake),
            }),
            first_wait,
        }
    }

    fn answer<T, K, F>(&self, key: &K, compute: F) -> Option<Arc<T>>
    where
        T: Send + Sync + 'static,
        K: Hash + ?Sized,
        F: FnOnce() -> T + Send + 'static,
    {
        let key = fingerprint::<T, K>(key);
        {
            let mut cache = self.shared.cache();
            match cache.slots.get(&key) {
                Some(Slot::Ready(_)) => return cache.ready(key),
                Some(Slot::Pending) => return None,
                None => cache.insert(key, Slot::Pending),
            }
        }
        let (done_tx, done_rx) = mpsc::sync_channel::<()>(1);
        let shared = Arc::clone(&self.shared);
        let spawned = std::thread::Builder::new()
            .name("tui-html".to_string())
            .spawn(move || {
                let answer: Answer = Arc::new(compute());
                shared.cache().insert(key, Slot::Ready(answer));
                // The caller stopped waiting: wake the loop to draw again.
                if done_tx.send(()).is_err() {
                    (shared.wake)();
                }
            });
        if spawned.is_err() {
            // Asked again on the next frame.
            self.shared.cache().slots.remove(&key);
            return None;
        }
        match done_rx.recv_timeout(self.first_wait) {
            Ok(()) => self.shared.cache().ready(key),
            Err(_) => None,
        }
    }
}

fn fingerprint<T: 'static, K: Hash + ?Sized>(key: &K) -> u64 {
    let mut hasher = DefaultHasher::new();
    TypeId::of::<T>().hash(&mut hasher);
    key.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;

    fn counting() -> (Deferred, Arc<AtomicUsize>, mpsc::Receiver<()>) {
        let wakes = Arc::new(AtomicUsize::new(0));
        let (woke_tx, woke_rx) = mpsc::channel();
        let counter = Arc::clone(&wakes);
        let deferred = Deferred::new(
            move || {
                counter.fetch_add(1, Ordering::SeqCst);
                let _ = woke_tx.send(());
            },
            Duration::from_millis(50),
        );
        (deferred, wakes, woke_rx)
    }

    #[test]
    fn without_an_install_the_answer_is_worked_out_inline() {
        // The unit-test process never installs, which keeps every other
        // test's renders synchronous.
        assert!(INSTALLED.get().is_none());
        assert_eq!(answer(&"key", || 7_u32).as_deref(), Some(&7));
    }

    #[test]
    fn a_quick_answer_is_drawn_on_the_first_frame_without_a_wake() {
        let (deferred, wakes, _) = counting();
        assert_eq!(deferred.answer(&"quick", || 1_u32).as_deref(), Some(&1));
        assert_eq!(deferred.answer(&"quick", || 2_u32).as_deref(), Some(&1));
        assert_eq!(wakes.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_slow_answer_returns_at_once_and_wakes_the_loop_when_it_lands() {
        let (deferred, wakes, woke) = counting();
        let started = Instant::now();
        let first = deferred.answer(&"slow", || {
            std::thread::sleep(Duration::from_millis(400));
            3_u32
        });
        assert!(first.is_none());
        assert!(
            started.elapsed() < Duration::from_millis(300),
            "{:?}",
            started.elapsed()
        );
        // While it is being worked out, asking again neither waits nor starts
        // a second parse.
        assert!(deferred.answer(&"slow", || 4_u32).is_none());
        woke.recv_timeout(Duration::from_secs(10)).expect("woken");
        assert_eq!(wakes.load(Ordering::SeqCst), 1);
        assert_eq!(deferred.answer(&"slow", || 5_u32).as_deref(), Some(&3));
    }

    #[test]
    fn the_cache_keeps_only_the_newest_answers() {
        let mut cache = Cache::default();
        for key in 0..(CACHED as u64 + 3) {
            cache.insert(key, Slot::Pending);
        }
        assert_eq!(cache.slots.len(), CACHED);
        assert!(!cache.slots.contains_key(&0));
        assert!(cache.slots.contains_key(&(CACHED as u64 + 2)));
    }

    #[test]
    fn answers_of_different_types_never_share_a_key() {
        assert_ne!(
            fingerprint::<u32, str>("k"),
            fingerprint::<String, str>("k")
        );
    }
}
