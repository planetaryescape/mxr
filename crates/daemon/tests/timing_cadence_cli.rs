//! Daemon-backed CLI smoke tests for Track 4 (Timing & Cadence).
//!
//! Pairs with `docs/ai-email/04-timing-cadence.md`. Asserts the JSON
//! acceptance criteria for `mxr send-time` and `mxr cadence watch/
//! list/drift/unwatch` against a real daemon over the Unix socket.
//!
//! These tests are intentionally LLM-free — Track 4 is statistical.

#![expect(
    clippy::panic,
    clippy::unwrap_used,
    reason = "integration tests use panic and unwrap to keep fixture failures direct"
)]

use mxr_test_support::daemon::{daemon_lock, run_json, run_status_only, spawn_fake_daemon};
use serde_json::Value;
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

    // Empty start state.
    let list0 = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["cadence", "list", "--format", "json"],
    );
    let arr0 = list0.as_array().expect("list returns JSON array");
    assert!(arr0.is_empty(), "fresh daemon has no watchlist: {list0}");

    // Watch a contact with a 14-day expected cadence. The watch
    // subcommand prints "watching" on success — no JSON contract there,
    // so we exit-status-only it.
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

    // List shows the row with expected_days = 14.
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

    // Unknown watched contacts are allowed, but they do not start
    // drifting until mxr has observed at least one inbound/outbound
    // contact timestamp for that address.
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

    // Unwatch removes the row from list AND drift — the spec is
    // explicit that drift is watchlist-scoped.
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

/// `mxr send-time <email>` returns a recommendation JSON even when
/// the recipient has zero history. The acceptance criterion (04, line
/// 99) is "returns table and JSON" — we only assert JSON.
#[test]
fn send_time_zero_history_returns_low_confidence_json() {
    let _guard = daemon_lock();
    let temp = TempDir::new().expect("temp dir");
    let (_daemon, instance, data_dir, config_dir) = spawn_fake_daemon(&temp, "send-time-empty");

    let resp = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["send-time", "ghost@example.com", "--format", "json"],
    );
    let rows = resp["recipient_rows"]
        .as_array()
        .expect("recipient_rows array");
    assert_eq!(rows.len(), 1, "single recipient returns one row: {resp}");
    assert_eq!(rows[0]["email"], "ghost@example.com");
    assert_eq!(rows[0]["sample_count"], 0);
    // No samples → Low confidence. The enum serializes as a string.
    assert_eq!(
        resp["confidence"].as_str(),
        Some("low"),
        "zero samples → low confidence: {resp}"
    );
    let windows = rows[0]["best_windows"]
        .as_array()
        .expect("best_windows array");
    assert!(
        windows.is_empty(),
        "no history → empty best windows: {resp}"
    );
    assert!(
        resp.get("proposed_at").is_none_or(Value::is_null),
        "no --at flag → no proposed_at in response: {resp}"
    );
}

/// `mxr send-time <email> --at "tomorrow 9am" --format json` must
/// echo the proposed slot in the JSON, even when the recipient has
/// no history (in which case proposed_expected_reply_seconds is null
/// — but proposed_at, proposed_weekday, proposed_hour are still set).
/// This is the acceptance criterion for `--at` (04, line 22).
#[test]
fn send_time_with_at_echoes_proposed_slot() {
    let _guard = daemon_lock();
    let temp = TempDir::new().expect("temp dir");
    let (_daemon, instance, data_dir, config_dir) = spawn_fake_daemon(&temp, "send-time-at");

    let resp = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "send-time",
            "ghost@example.com",
            "--at",
            "tomorrow 9am",
            "--format",
            "json",
        ],
    );
    assert!(
        resp["proposed_at"].is_string(),
        "proposed_at is an RFC3339 string: {resp}"
    );
    let wd = resp["proposed_weekday"]
        .as_u64()
        .unwrap_or_else(|| panic!("proposed_weekday missing: {resp}"));
    assert!(wd < 7, "weekday 0..6: got {wd}");
    let hr = resp["proposed_hour"]
        .as_u64()
        .unwrap_or_else(|| panic!("proposed_hour missing: {resp}"));
    assert_eq!(hr, 9, "9am UTC → hour 9: {resp}");
    // No history means no expected p50 for the proposed slot.
    assert!(
        resp["recipient_rows"][0]
            .get("proposed_expected_reply_seconds")
            .is_none_or(Value::is_null),
        "no history → proposed_expected_reply_seconds is null: {resp}"
    );
}

/// Multiple recipients must be preserved as separate rows so callers
/// can display the worst useful timing delta without losing per-contact
/// evidence. With no history each row is still present, low-confidence,
/// and has zero samples.
#[test]
fn send_time_multiple_recipients_returns_per_recipient_rows() {
    let _guard = daemon_lock();
    let temp = TempDir::new().expect("temp dir");
    let (_daemon, instance, data_dir, config_dir) = spawn_fake_daemon(&temp, "send-time-multi");

    let resp = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "send-time",
            "alice@example.com",
            "bob@example.com",
            "--format",
            "json",
        ],
    );

    let rows = resp["recipient_rows"]
        .as_array()
        .unwrap_or_else(|| panic!("recipient_rows missing: {resp}"));
    assert_eq!(rows.len(), 2, "one row per recipient: {resp}");
    assert_eq!(rows[0]["email"], "alice@example.com");
    assert_eq!(rows[1]["email"], "bob@example.com");
    assert_eq!(rows[0]["sample_count"], 0);
    assert_eq!(rows[1]["sample_count"], 0);
    assert_eq!(resp["confidence"].as_str(), Some("low"));
}
