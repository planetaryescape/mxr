//! `mxr modes` against a real daemon: the copy each mode teaches itself
//! with, as JSON, and the first-encounter card's seen state surviving
//! between commands.

#![expect(
    clippy::unwrap_used,
    reason = "integration tests unwrap to keep fixture failures direct"
)]

use mxr_test_support::daemon::{daemon_lock, run_json, spawn_fake_daemon};
use tempfile::TempDir;

#[test]
fn modes_explain_prints_the_to_do_guide_and_card_state_holds() {
    let _guard = daemon_lock();
    let temp = TempDir::new().expect("temp dir");
    let (_daemon, instance, data_dir, config_dir) = spawn_fake_daemon(&temp, "modes-explain");

    let guides = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["modes", "explain", "todo", "--format", "json"],
    );
    insta::assert_snapshot!(
        "modes_explain_todo",
        serde_json::to_string_pretty(&guides).unwrap()
    );

    let closed = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["modes", "card", "todo", "--format", "json"],
    );
    assert_eq!(closed[0]["card_seen"], true, "{closed}");
    let again = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["modes", "explain", "--format", "json"],
    );
    assert_eq!(again.as_array().unwrap().len(), 1);
    assert_eq!(again[0]["card_seen"], true, "a new command sees it closed");

    let shown = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["modes", "card", "todo", "--show", "--format", "json"],
    );
    assert_eq!(shown[0]["card_seen"], false, "{shown}");
}
