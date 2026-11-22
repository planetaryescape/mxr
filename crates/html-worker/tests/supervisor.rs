//! The supervisor against a real child process (`mxr-html-worker-fixture`):
//! timeouts, crashes, memory and CPU limits, restarts, poisoned input and
//! concurrency.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use mxr_html_worker::{Config, FallbackReason, Job, Outcome, Supervisor};

static ECHO: Job<(), String> = Job::new("fixture.echo", |html, ()| html.to_string());
static PID: Job<(), u32> = Job::new("fixture.pid", |_, ()| 0);
static SLEEP: Job<u64, String> = Job::new("fixture.sleep", |_, _| String::new());
static ALLOCATE: Job<(usize, u64), usize> = Job::new("fixture.allocate", |_, _| 0);
static SPIN: Job<(), String> = Job::new("fixture.spin", |_, ()| String::new());
static PANIC: Job<(), String> = Job::new("fixture.panic", |_, ()| String::new());
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
fn a_helper_that_grows_past_its_memory_headroom_is_stopped() {
    let supervisor = supervisor_with(|config| {
        config.max_helpers = 1;
        config.memory_headroom_bytes = 96 * 1024 * 1024;
    });
    let parent_before = peak_resident_bytes();
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
    assert!(
        peak_resident_bytes() < parent_before + 64 * 1024 * 1024,
        "parent peak resident size grew from {parent_before} to {}",
        peak_resident_bytes()
    );
    assert_eq!(ALLOCATE.call_on(&supervisor, "", &(16, 0)), Some(16));
}

#[test]
fn a_helper_that_burns_its_cpu_budget_is_stopped_by_the_os() {
    let supervisor = supervisor_with(|config| {
        config.max_helpers = 1;
        config.cpu_seconds_per_request = 1;
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
fn a_helper_from_another_release_is_refused_at_the_handshake() {
    // What macOS gives a daemon or TUI whose binary was upgraded under it: the
    // helper starts from the new file. Its answers may not match this build's
    let supervisor = Supervisor::new(Config::new(
        env!("CARGO_BIN_EXE_mxr-html-worker-fixture"),
        vec!["--another-release".into()],
    ));
    let outcome = supervisor.run("fixture.echo", b"x", b"null", usize::MAX);
    assert!(
        fell_back(&outcome, FallbackReason::Unavailable),
        "{outcome:?}"
    );
    assert_eq!(ECHO.call_on(&supervisor, "x", &()), None);
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
