//! Integration test for `mxr drafts scheduled`: a draft scheduled through
//! `mxr send --at` shows up in the JSON listing, and cancelling it with
//! `mxr unsend` removes it. Drives the real binary against a real daemon so
//! the CLI, IPC and store agree on what "pending" means.

#![expect(
    clippy::expect_fun_call,
    reason = "integration tests include command output in parse failure messages"
)]

use assert_cmd::prelude::*;
use mxr_test_support::daemon::{daemon_lock, spawn_fake_daemon};
use serde_json::Value;
use std::process::Command;
use tempfile::TempDir;

struct Env {
    _daemon: mxr_test_support::daemon::DaemonGuard,
    instance: String,
    data_dir: std::path::PathBuf,
    config_dir: std::path::PathBuf,
}

impl Env {
    fn run(&self, args: &[&str]) -> assert_cmd::assert::Assert {
        Command::cargo_bin("mxr")
            .expect("mxr bin")
            .env("MXR_INSTANCE", &self.instance)
            .env("MXR_DATA_DIR", &self.data_dir)
            .env("MXR_CONFIG_DIR", &self.config_dir)
            .env_remove("EDITOR")
            .env_remove("VISUAL")
            .args(args)
            .assert()
    }

    fn json(&self, args: &[&str]) -> Value {
        let output = self.run(args).success().get_output().clone();
        let stdout = String::from_utf8(output.stdout).expect("utf8");
        serde_json::from_str(stdout.trim()).expect(&format!("parse JSON: {stdout}"))
    }
}

#[test]
fn drafts_scheduled_lists_a_scheduled_draft_until_it_is_unsent() {
    let _guard = daemon_lock();
    let temp = TempDir::new().expect("temp dir");
    let (daemon, instance, data_dir, config_dir) = spawn_fake_daemon(&temp, "scheduled-sends");
    let env = Env {
        _daemon: daemon,
        instance,
        data_dir,
        config_dir,
    };

    let composed = env.json(&[
        "compose",
        "--to",
        "alice@example.com",
        "--subject",
        "Later",
        "--body",
        "Going out later.",
        "--draft",
        "--no-signature",
        "--format",
        "json",
    ]);
    let draft_id = composed["draft_id"].as_str().expect("draft_id").to_string();

    assert_eq!(
        env.json(&["drafts", "--format", "json", "scheduled"]),
        serde_json::json!([]),
        "an unscheduled draft is not a scheduled send"
    );

    env.run(&["send", &draft_id, "--at", "in 2h"]).success();

    let scheduled = env.json(&["drafts", "--format", "json", "scheduled"]);
    let rows = scheduled.as_array().expect("array");
    assert_eq!(rows.len(), 1, "scheduled={scheduled:#}");
    assert_eq!(rows[0]["draft_id"], draft_id);
    assert_eq!(rows[0]["subject"], "Later");
    assert_eq!(rows[0]["to"][0]["email"], "alice@example.com");
    assert!(rows[0]["send_at"].as_str().is_some(), "row={:#}", rows[0]);

    env.run(&["unsend", &draft_id]).success();
    assert_eq!(
        env.json(&["drafts", "--format", "json", "scheduled"]),
        serde_json::json!([])
    );
}

/// An unreadable `--remind-after` must stop the send: the reminder time is
/// resolved before anything goes out, so the draft is still there after.
#[test]
fn an_unreadable_remind_after_sends_nothing() {
    let _guard = daemon_lock();
    let temp = TempDir::new().expect("temp dir");
    let (daemon, instance, data_dir, config_dir) = spawn_fake_daemon(&temp, "remind-after-bad");
    let env = Env {
        _daemon: daemon,
        instance,
        data_dir,
        config_dir,
    };

    let composed = env.json(&[
        "compose",
        "--to",
        "alice@example.com",
        "--subject",
        "Follow up",
        "--body",
        "Checking in.",
        "--draft",
        "--no-signature",
        "--format",
        "json",
    ]);
    let draft_id = composed["draft_id"].as_str().expect("draft_id").to_string();

    let output = env
        .run(&["send", &draft_id, "--remind-after", "frday"])
        .failure()
        .get_output()
        .clone();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Didn't catch \"frday\""), "stderr={stderr}");
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("Sent draft"),
        "nothing may be sent"
    );

    // The stored draft still exists, so it was never sent.
    let drafts = env.json(&["drafts", "--format", "json"]);
    let text = drafts.to_string();
    assert!(text.contains(&draft_id), "draft must remain: {drafts:#}");
}
