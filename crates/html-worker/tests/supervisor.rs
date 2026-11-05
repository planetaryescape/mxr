//! The supervisor against a real child process (`mxr-html-worker-fixture`):
//! timeouts, crashes, memory and CPU limits, restarts, poisoned input and
//! concurrency.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mxr_html_worker::{with_supervisor, Config, FallbackReason, Job, Outcome, Supervisor};

static ECHO: Job<(), String> = Job::new("fixture.echo", |html, ()| html.to_string());
static PID: Job<(), u32> = Job::new("fixture.pid", |_, ()| 0);
static SLEEP: Job<u64, String> = Job::new("fixture.sleep", |_, _| String::new());
static ALLOCATE: Job<(usize, u64), usize> = Job::new("fixture.allocate", |_, _| 0);
static BURN: Job<u64, String> = Job::new("fixture.burn", |_, _| String::new());
static SPIN: Job<(), String> = Job::new("fixture.spin", |_, ()| String::new());
static PANIC: Job<(), String> = Job::new("fixture.panic", |_, ()| String::new());
static DIE_ON_INPUT: Job<(), String> = Job::new("fixture.die_on_input", |_, ()| String::new());

fn config() -> Config {
    let mut config = Config::new(env!("CARGO_BIN_EXE_mxr-html-worker-fixture"), Vec::new());
    config.request_timeout = Duration::from_secs(10);
    config
}

fn supervisor_with(adjust: impl FnOnce(&mut Config)) -> Supervisor {
    let mut config = config();
    adjust(&mut config);
    Supervisor::new(config)
}

fn fell_back(outcome: &Outcome, expected: FallbackReason) -> bool {
    matches!(outcome, Outcome::Fallback(reason) if *reason == expected)
}

#[test]
fn a_job_runs_in_a_child_process_that_is_reused() {
    let supervisor = supervisor_with(|config| config.max_helpers = 1);
    assert_eq!(
        ECHO.call_on(&supervisor, "<p>hi</p>", &()).as_deref(),
        Some("<p>hi</p>")
    );
    let first = PID.call_on(&supervisor, "", &()).expect("pid");
    let second = PID.call_on(&supervisor, "", &()).expect("pid");
    assert_ne!(first, std::process::id(), "the job ran in another process");
    assert_eq!(first, second, "an idle helper serves the next request");
    let stats = supervisor.stats();
    assert_eq!((stats.requests, stats.fallbacks, stats.restarts), (3, 0, 0));
}

#[test]
fn a_scoped_supervisor_routes_calls_on_this_thread_and_only_inside_the_scope() {
    // The fixture's `pid` job reports the process it ran in; the static here
    // would answer 0 if it ran in this one.
    let supervisor = Arc::new(supervisor_with(|config| config.max_helpers = 1));
    let inside = with_supervisor(Arc::clone(&supervisor), || PID.call(" ", &()));
    assert!(inside.is_some_and(|pid| pid != 0 && pid != std::process::id()));
    assert_eq!(
        PID.call("", &()),
        Some(0),
        "outside the scope the job runs here"
    );
    assert_eq!(supervisor.stats().requests, 1);
}

#[test]
fn a_job_with_a_smaller_size_cap_is_not_sent_past_it() {
    static SMALL: Job<(), String> =
        Job::new("fixture.echo", |html, ()| html.to_string()).max_html_bytes(8);
    let supervisor = supervisor_with(|_| {});
    assert_eq!(
        SMALL.call_on(&supervisor, "tiny", &()).as_deref(),
        Some("tiny")
    );
    assert_eq!(
        SMALL.call_on(&supervisor, "far too long for the cap", &()),
        None
    );
    assert_eq!(supervisor.stats().fallbacks, 1);
}

#[test]
fn a_request_that_times_out_gets_the_fallback_and_the_next_one_a_fresh_helper() {
    let supervisor = supervisor_with(|config| {
        config.max_helpers = 1;
        config.request_timeout = Duration::from_millis(300);
    });
    let before = PID.call_on(&supervisor, "", &()).expect("pid");

    let started = Instant::now();
    assert_eq!(SLEEP.call_on(&supervisor, "x", &30_000), None);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the supervisor stops waiting at the timeout, took {:?}",
        started.elapsed()
    );

    let after = PID
        .call_on(&supervisor, "", &())
        .expect("a new helper answers");
    assert_ne!(before, after, "the stuck helper was replaced");
    let stats = supervisor.stats();
    assert_eq!((stats.timeouts, stats.restarts, stats.fallbacks), (1, 1, 1));
}

#[test]
fn an_input_that_timed_out_is_not_tried_again() {
    let supervisor = supervisor_with(|config| {
        config.max_helpers = 1;
        config.request_timeout = Duration::from_millis(300);
    });
    assert_eq!(SLEEP.call_on(&supervisor, "poison", &30_000), None);
    let started = Instant::now();
    let again = supervisor.run("fixture.sleep", b"poison", b"30000", usize::MAX);
    assert!(fell_back(&again, FallbackReason::KnownBad), "{again:?}");
    assert!(
        started.elapsed() < Duration::from_millis(200),
        "a remembered input skips the helper, took {:?}",
        started.elapsed()
    );
    assert_eq!(
        supervisor.stats().timeouts,
        1,
        "no second timeout was spent"
    );
    // The same kind with other input still runs.
    assert_eq!(
        SLEEP.call_on(&supervisor, "other", &0).as_deref(),
        Some("slept")
    );
}

#[test]
fn an_input_that_kills_the_helper_twice_is_remembered_and_not_looped() {
    let supervisor = supervisor_with(|config| config.max_helpers = 1);
    let outcome = supervisor.run("fixture.abort", b"", b"null", usize::MAX);
    assert!(
        fell_back(&outcome, FallbackReason::HelperDied),
        "{outcome:?}"
    );
    assert_eq!(
        supervisor.stats().deaths,
        2,
        "one try and one retry, no more"
    );

    let again = supervisor.run("fixture.abort", b"", b"null", usize::MAX);
    assert!(fell_back(&again, FallbackReason::KnownBad), "{again:?}");
    assert_eq!(
        supervisor.stats().deaths,
        2,
        "the remembered input never reaches a helper"
    );
    assert_eq!(
        ECHO.call_on(&supervisor, "alive", &()).as_deref(),
        Some("alive")
    );
}

#[test]
fn a_helper_killed_from_outside_costs_a_retry_not_a_poisoned_input() {
    let supervisor = supervisor_with(|config| config.max_helpers = 1);
    let victim = PID.call_on(&supervisor, "", &()).expect("pid");
    let status = std::process::Command::new("kill")
        .args(["-9", &victim.to_string()])
        .status()
        .expect("kill runs");
    assert!(status.success());

    // The same input that was in flight when the helper vanished still works:
    // it is retried on a fresh helper and never marked bad.
    assert_eq!(
        ECHO.call_on(&supervisor, "same input", &()).as_deref(),
        Some("same input")
    );
    assert_eq!(
        ECHO.call_on(&supervisor, "same input", &()).as_deref(),
        Some("same input")
    );
    let stats = supervisor.stats();
    assert!(stats.deaths >= 1 && stats.restarts >= 1, "{stats:?}");
    assert_eq!(stats.fallbacks, 0, "{stats:?}");
}

#[test]
fn a_crash_on_one_input_leaves_other_inputs_unaffected() {
    let supervisor = supervisor_with(|config| config.max_helpers = 1);
    assert_eq!(
        DIE_ON_INPUT.call_on(&supervisor, "fine", &()).as_deref(),
        Some("fine")
    );
    assert_eq!(DIE_ON_INPUT.call_on(&supervisor, "die", &()), None);
    assert_eq!(
        DIE_ON_INPUT
            .call_on(&supervisor, "fine again", &())
            .as_deref(),
        Some("fine again")
    );
}

#[test]
fn a_helper_that_grows_past_its_memory_headroom_is_stopped() {
    let supervisor = supervisor_with(|config| {
        config.max_helpers = 1;
        config.memory_headroom_bytes = 96 * 1024 * 1024;
    });
    let parent_before = peak_resident_bytes();
    // Four times the headroom, all touched.
    let started = Instant::now();
    let outcome = supervisor.run("fixture.allocate", b"", b"[400,1500]", usize::MAX);
    assert!(
        matches!(outcome, Outcome::Fallback(_)),
        "400 MiB against 96 MiB of headroom must not succeed: {outcome:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(8),
        "the limit stops it long before the timeout, took {:?}",
        started.elapsed()
    );
    // The parent never held the memory, whatever the child did.
    assert!(
        peak_resident_bytes() < parent_before + 64 * 1024 * 1024,
        "parent peak resident size grew from {parent_before} to {}",
        peak_resident_bytes()
    );
    // Within the headroom it works, on a fresh helper.
    assert_eq!(ALLOCATE.call_on(&supervisor, "", &(16, 0)), Some(16));
}

#[test]
fn a_helper_that_burns_its_cpu_budget_is_stopped_by_the_os() {
    let supervisor = supervisor_with(|config| {
        config.max_helpers = 1;
        config.cpu_seconds_per_request = 1;
        // The wall clock is far away, so only the CPU limit can end this.
        config.request_timeout = Duration::from_secs(60);
    });
    let started = Instant::now();
    assert_eq!(SPIN.call_on(&supervisor, "", &()), None);
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "the CPU limit ended the spin, took {:?}",
        started.elapsed()
    );
    assert!(supervisor.stats().deaths >= 1);
    assert_eq!(
        ECHO.call_on(&supervisor, "after", &()).as_deref(),
        Some("after")
    );
}

#[cfg(unix)]
#[test]
fn the_cpu_budget_is_armed_under_an_inherited_hard_cpu_limit() {
    // `ulimit -t` sets the soft and hard limits, as a shell or service manager
    // with a CPU cap would. Raising that hard limit is refused, so a re-arm
    // that asked for an unlimited hard limit left the budget unarmed and the
    // spin ran until the wall clock.
    let mut config = Config::new(
        "/bin/sh",
        vec![
            "-c".into(),
            format!(
                "ulimit -t 120 && exec '{}'",
                env!("CARGO_BIN_EXE_mxr-html-worker-fixture")
            )
            .into(),
        ],
    );
    config.max_helpers = 1;
    config.cpu_seconds_per_request = 1;
    config.request_timeout = Duration::from_secs(60);
    let supervisor = Supervisor::new(config);
    let started = Instant::now();
    assert_eq!(SPIN.call_on(&supervisor, "", &()), None);
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "the per-request CPU budget ended the spin, took {:?}",
        started.elapsed()
    );
    assert!(supervisor.stats().deaths >= 1);
    assert_eq!(supervisor.stats().timeouts, 0);
}

#[test]
fn the_cpu_budget_is_per_request_not_for_the_helpers_life() {
    let supervisor = supervisor_with(|config| {
        config.max_helpers = 1;
        config.cpu_seconds_per_request = 1;
    });
    let helper = PID.call_on(&supervisor, "", &()).expect("pid");
    // Twelve requests of 150 ms of CPU each are about 1.8 s in total, past the
    // 1 s budget a lifetime limit would allow, while each request alone is well
    // inside it.
    for _ in 0..12 {
        assert_eq!(
            BURN.call_on(&supervisor, "", &150).as_deref(),
            Some("burned")
        );
    }
    assert_eq!(
        PID.call_on(&supervisor, "", &()),
        Some(helper),
        "the same helper served them all"
    );
}

#[test]
fn a_panicking_job_falls_back_and_keeps_the_helper() {
    let supervisor = supervisor_with(|config| config.max_helpers = 1);
    let helper = PID.call_on(&supervisor, "", &()).expect("pid");
    assert_eq!(PANIC.call_on(&supervisor, "x", &()), None);
    assert_eq!(PID.call_on(&supervisor, "", &()), Some(helper));
    let stats = supervisor.stats();
    assert_eq!((stats.job_errors, stats.deaths, stats.restarts), (1, 0, 0));
}

#[test]
fn an_unknown_kind_is_a_fallback_not_a_hang() {
    let supervisor = supervisor_with(|_| {});
    let outcome = supervisor.run("fixture.no_such_job", b"x", b"null", usize::MAX);
    assert!(
        fell_back(&outcome, FallbackReason::JobFailed),
        "{outcome:?}"
    );
}

#[test]
fn a_document_over_the_size_cap_is_not_sent() {
    let supervisor = supervisor_with(|_| {});
    let outcome = supervisor.run("fixture.echo", &vec![b'a'; 2048], b"null", 1024);
    assert!(fell_back(&outcome, FallbackReason::Oversize), "{outcome:?}");
    let stats = supervisor.stats();
    assert_eq!((stats.requests, stats.fallbacks, stats.restarts), (1, 1, 0));
}

#[test]
fn a_missing_helper_program_falls_back_and_is_not_respawned_on_every_request() {
    let supervisor = Supervisor::new(Config::new("/nonexistent/mxr-helper", Vec::new()));
    let first = supervisor.run("fixture.echo", b"x", b"null", usize::MAX);
    assert!(fell_back(&first, FallbackReason::Unavailable), "{first:?}");
    let started = Instant::now();
    for _ in 0..50 {
        assert!(fell_back(
            &supervisor.run("fixture.echo", b"x", b"null", usize::MAX),
            FallbackReason::Unavailable
        ));
    }
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn a_program_that_exits_before_the_handshake_is_unavailable() {
    let supervisor = Supervisor::new(Config::new("/usr/bin/true", Vec::new()));
    let outcome = supervisor.run("fixture.echo", b"x", b"null", usize::MAX);
    assert!(
        fell_back(&outcome, FallbackReason::Unavailable),
        "{outcome:?}"
    );
}

#[test]
fn many_parallel_requests_all_complete_with_their_own_answers() {
    let supervisor = Arc::new(supervisor_with(|config| config.max_helpers = 2));
    let done = Arc::new(AtomicUsize::new(0));
    let threads: Vec<_> = (0..16)
        .map(|thread| {
            let supervisor = Arc::clone(&supervisor);
            let done = Arc::clone(&done);
            std::thread::spawn(move || {
                for request in 0..20 {
                    let input = format!("thread {thread} request {request}");
                    assert_eq!(ECHO.call_on(&supervisor, &input, &()), Some(input));
                    done.fetch_add(1, Ordering::Relaxed);
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().expect("worker thread");
    }
    assert_eq!(done.load(Ordering::Relaxed), 320);
    let stats = supervisor.stats();
    assert_eq!((stats.requests, stats.fallbacks), (320, 0));
}

#[test]
fn a_crash_in_the_middle_of_a_batch_only_affects_the_request_in_flight() {
    let supervisor = Arc::new(supervisor_with(|config| config.max_helpers = 2));
    let crashed = Arc::new(AtomicBool::new(false));
    let threads: Vec<_> = (0..8)
        .map(|thread| {
            let supervisor = Arc::clone(&supervisor);
            let crashed = Arc::clone(&crashed);
            std::thread::spawn(move || {
                let mut lost = 0;
                for request in 0..25 {
                    if thread == 0 && request == 10 {
                        let outcome = supervisor.run("fixture.abort", b"", b"null", usize::MAX);
                        assert!(matches!(outcome, Outcome::Fallback(_)));
                        crashed.store(true, Ordering::Relaxed);
                        continue;
                    }
                    let input = format!("{thread}-{request}");
                    match ECHO.call_on(&supervisor, &input, &()) {
                        Some(echoed) => assert_eq!(echoed, input),
                        None => lost += 1,
                    }
                }
                lost
            })
        })
        .collect();
    let lost: usize = threads.into_iter().map(|t| t.join().expect("thread")).sum();
    assert!(crashed.load(Ordering::Relaxed));
    assert_eq!(lost, 0, "no echo request was lost to the neighbour's crash");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn waiting_on_a_helper_does_not_stall_the_async_runtime() {
    let supervisor = Arc::new(supervisor_with(|config| config.max_helpers = 2));
    // Warm both helpers so spawning is not part of what is measured.
    let _ = ECHO.call_on(&supervisor, "warm", &());

    let ticks = Arc::new(AtomicUsize::new(0));
    let ticker = {
        let ticks = Arc::clone(&ticks);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(10)).await;
                ticks.fetch_add(1, Ordering::Relaxed);
            }
        })
    };
    // More callers than runtime threads, each blocked on a 300 ms job.
    let callers: Vec<_> = (0..4)
        .map(|_| {
            let supervisor = Arc::clone(&supervisor);
            tokio::spawn(async move { SLEEP.call_on(&supervisor, "", &300) })
        })
        .collect();
    for caller in callers {
        assert_eq!(caller.await.expect("task").as_deref(), Some("slept"));
    }
    ticker.abort();
    assert!(
        ticks.load(Ordering::Relaxed) >= 15,
        "the runtime kept running timers while jobs were blocked: {} ticks in about 600 ms",
        ticks.load(Ordering::Relaxed)
    );
}

fn peak_resident_bytes() -> u64 {
    // ps reports resident size in KiB on both macOS and Linux; the parent's
    // current size is close enough to its peak for a bound this loose.
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .expect("ps runs");
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u64>()
        .expect("ps prints a number")
        * 1024
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_call_from_a_blocking_task_on_a_multi_thread_runtime_works() {
    let supervisor = Arc::new(supervisor_with(|_| {}));
    let echoed =
        tokio::task::spawn_blocking(move || ECHO.call_on(&supervisor, "from a blocking task", &()))
            .await
            .expect("task");
    assert_eq!(echoed.as_deref(), Some("from a blocking task"));
}

#[tokio::test(flavor = "current_thread")]
async fn a_call_on_a_single_thread_runtime_blocks_instead_of_panicking() {
    let supervisor = supervisor_with(|_| {});
    assert_eq!(
        ECHO.call_on(&supervisor, "single thread", &()).as_deref(),
        Some("single thread")
    );
}

#[test]
fn a_helper_is_in_its_own_process_group() {
    let supervisor = supervisor_with(|_| {});
    let helper = PID.call_on(&supervisor, "", &()).expect("pid");
    let group_of = |pid: u32| {
        let output = std::process::Command::new("ps")
            .args(["-o", "pgid=", "-p", &pid.to_string()])
            .output()
            .expect("ps runs");
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    };
    assert_ne!(group_of(helper), group_of(std::process::id()));
}

#[test]
fn callers_queued_behind_a_failing_start_all_get_their_fallback() {
    let supervisor = Arc::new(Supervisor::new({
        let mut config = Config::new("/usr/bin/true", Vec::new());
        config.max_helpers = 1;
        config
    }));
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let supervisor = Arc::clone(&supervisor);
            std::thread::spawn(move || ECHO.call_on(&supervisor, "x", &()))
        })
        .collect();
    for thread in threads {
        assert_eq!(thread.join().expect("thread"), None);
    }
}
