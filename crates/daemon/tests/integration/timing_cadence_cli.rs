//! Daemon-backed CLI smoke tests for Track 4 (Timing & Cadence).
//!
//! Pairs with `docs/reference/ai-email.md`. Asserts the JSON
//! acceptance criteria for `mxr send-time` and `mxr cadence watch/
//! list/drift/unwatch` against a real daemon over the Unix socket.
//!
//! These tests are intentionally LLM-free — Track 4 is statistical.

#![expect(
    clippy::unwrap_used,
    reason = "integration tests use panic and unwrap to keep fixture failures direct"
)]

use mxr_test_support::daemon::{daemon_lock, run_json, run_status_only, spawn_fake_daemon};
use tempfile::TempDir;

/// `mxr cadence watch <email> --expected-days N`, then `list`, then
/// `unwatch`. JSON round-trip must reflect each mutation. This is the
/// acceptance contract for 04-timing-cadence.md (line 181 "returns
/// only explicit watched contacts").
#[test]
fn cadence_watch_list_unwatch_round_trip() {
    let _guard = daemon_lock();
    let temp = TempDir::new().expect("temp dir");
    let (_daemon, instance, data_dir, config_dir) = spawn_fake_daemon(&temp, "cadence-rt");

    let list0 = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["cadence", "list", "--format", "json"],
    );
    let arr0 = list0.as_array().expect("list returns JSON array");
    assert!(arr0.is_empty(), "fresh daemon has no watchlist: {list0}");

    run_status_only(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "cadence",
            "watch",
            "alice@example.com",
            "--every",
            "14d",
            "--allow-list-sender",
        ],
    );

    let list1 = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["cadence", "list", "--format", "json"],
    );
    let arr1 = list1.as_array().expect("list returns JSON array");
    assert_eq!(arr1.len(), 1, "exactly one watched contact: {list1}");
    assert_eq!(arr1[0]["email"], "alice@example.com");
    assert!(
        (arr1[0]["expected_days"].as_f64().unwrap() - 14.0).abs() < 0.001,
        "expected_days echoes the input: {list1}"
    );

    let drift = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["cadence", "drift", "--format", "json"],
    );
    let drift_arr = drift.as_array().expect("drift returns JSON array");
    assert!(
        drift_arr.is_empty(),
        "never-seen watched contacts are not overdue yet: {drift}"
    );

    run_status_only(
        &instance,
        &data_dir,
        &config_dir,
        &["cadence", "unwatch", "alice@example.com"],
    );
    let list2 = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["cadence", "list", "--format", "json"],
    );
    let arr2 = list2.as_array().expect("list returns JSON array");
    assert!(arr2.is_empty(), "after unwatch, list is empty: {list2}");

    let drift2 = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["cadence", "drift", "--format", "json"],
    );
    let drift2_arr = drift2.as_array().expect("drift returns JSON array");
    assert!(
        drift2_arr.is_empty(),
        "after unwatch, drift is empty: {drift2}"
    );
}
