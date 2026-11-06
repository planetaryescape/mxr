//! The helper process limits itself before it reads its first request.
//!
//! Three limits, because none works everywhere:
//!
//! * Address space (`RLIMIT_AS`), Linux only. It makes an allocation past the
//!   cap fail, and Rust aborts the process. macOS reserves hundreds of
//!   gigabytes of address space for every process, so a cap near the real
//!   usage is refused there (`EINVAL`), and `RLIMIT_DATA` is not enforced.
//! * A memory watchdog, everywhere: a thread that compares the process's peak
//!   resident size with its starting size plus the headroom and aborts when it
//!   is past. On Linux it backs up the address-space cap for memory that is
//!   resident without being a single huge allocation; on macOS it is the only
//!   memory limit. It samples, so a very fast allocator can overshoot by a few
//!   tens of megabytes before it is caught.
//! * CPU time (`RLIMIT_CPU`), re-armed before every request so the budget is
//!   per request and not for the helper's whole life. The supervisor's
//!   wall-clock timeout is the primary bound; this is the backstop for a helper
//!   whose parent is gone.

use std::time::Duration;

use crate::supervisor::{ENV_CPU_SECONDS, ENV_MEMORY_HEADROOM};

const DEFAULT_MEMORY_HEADROOM: u64 = 512 * 1024 * 1024;
const DEFAULT_CPU_SECONDS: u64 = 8;
const WATCHDOG_INTERVAL: Duration = Duration::from_millis(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Limits {
    pub memory_headroom: u64,
    pub cpu_seconds: u64,
}

impl Limits {
    pub(crate) fn from_env() -> Self {
        let read = |name: &str, default: u64| {
            std::env::var(name)
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(default)
        };
        Self {
            memory_headroom: read(ENV_MEMORY_HEADROOM, DEFAULT_MEMORY_HEADROOM),
            cpu_seconds: read(ENV_CPU_SECONDS, DEFAULT_CPU_SECONDS),
        }
    }
}

/// Applies the process-wide limits and starts the memory watchdog.
#[cfg(unix)]
pub(crate) fn apply(limits: Limits) {
    #[cfg(target_os = "linux")]
    limit_address_space(limits.memory_headroom);
    start_watchdog(limits.memory_headroom);
}

/// Re-arms the CPU limit to `limits.cpu_seconds` beyond what this process has
/// used so far. Called before each request.
#[cfg(unix)]
pub(crate) fn rearm_cpu(limits: Limits) {
    use nix::sys::resource::{getrlimit, getrusage, setrlimit, Resource, UsageWho};
    let (Ok(usage), Ok((_, hard))) = (
        getrusage(UsageWho::RUSAGE_SELF),
        getrlimit(Resource::RLIMIT_CPU),
    ) else {
        return;
    };
    let used = |time: nix::sys::time::TimeVal| {
        u64::try_from(time.tv_sec()).unwrap_or(0) + u64::from(time.tv_usec() > 0)
    };
    let soft = cpu_soft_limit(
        used(usage.user_time()) + used(usage.system_time()),
        limits.cpu_seconds,
        hard,
    );
    // The hard limit is kept as inherited: raising a finite one is refused
    // (EPERM), which would leave the per-request budget unarmed, and lowering
    // it could never be undone. The soft limit's SIGXCPU ends the process.
    if let Err(error) = setrlimit(Resource::RLIMIT_CPU, soft, hard) {
        eprintln!("html helper: could not re-arm the CPU limit: {error}");
    }
}

/// The soft CPU limit for the next request: what has been used plus the
/// budget, never above the hard limit the process inherited.
#[cfg(unix)]
fn cpu_soft_limit(used_seconds: u64, budget_seconds: u64, hard: u64) -> u64 {
    used_seconds.saturating_add(budget_seconds).min(hard)
}

#[cfg(target_os = "linux")]
fn limit_address_space(headroom: u64) {
    use nix::sys::resource::{setrlimit, Resource};
    let Some(current) = current_address_space() else {
        return;
    };
    let cap = current.saturating_add(headroom);
    if let Err(error) = setrlimit(Resource::RLIMIT_AS, cap, cap) {
        // The watchdog below still bounds resident memory.
        eprintln!("html helper: could not set the address-space limit: {error}");
    }
}

/// Virtual size in bytes, from `/proc/self/status`.
#[cfg(target_os = "linux")]
fn current_address_space() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|line| line.starts_with("VmSize:"))?;
    let kib: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kib * 1024)
}

/// Peak resident size so far, in bytes.
#[cfg(unix)]
fn peak_resident_bytes() -> Option<u64> {
    use nix::sys::resource::{getrusage, UsageWho};
    let peak = u64::try_from(getrusage(UsageWho::RUSAGE_SELF).ok()?.max_rss()).ok()?;
    // Linux reports kibibytes, macOS bytes.
    Some(if cfg!(target_os = "macos") {
        peak
    } else {
        peak * 1024
    })
}

#[cfg(unix)]
fn start_watchdog(headroom: u64) {
    let Some(start) = peak_resident_bytes() else {
        return;
    };
    let ceiling = start.saturating_add(headroom);
    let spawned = std::thread::Builder::new()
        .name("html-helper-watchdog".to_string())
        .spawn(move || loop {
            std::thread::sleep(WATCHDOG_INTERVAL);
            if peak_resident_bytes().is_some_and(|peak| peak > ceiling) {
                // abort, not exit: exit runs handlers that may need the
                // allocator, which is the thing that is out of room.
                std::process::abort();
            }
        });
    if let Err(error) = spawned {
        eprintln!("html helper: could not start the memory watchdog: {error}");
    }
}

#[cfg(not(unix))]
pub(crate) fn apply(_limits: Limits) {}

#[cfg(not(unix))]
pub(crate) fn rearm_cpu(_limits: Limits) {}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn the_cpu_budget_is_counted_from_what_has_been_used() {
        assert_eq!(cpu_soft_limit(3, 8, nix::sys::resource::RLIM_INFINITY), 11);
    }

    #[test]
    fn the_cpu_budget_never_asks_past_an_inherited_hard_limit() {
        assert_eq!(cpu_soft_limit(55, 8, 60), 60);
        assert_eq!(cpu_soft_limit(u64::MAX, 8, 60), 60);
    }
}
