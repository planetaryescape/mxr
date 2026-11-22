//! Integration tests for `mxr compose --check`. Verifies the transient
//! draft path: build a Draft from CLI args, run the safety pipeline
//! against it without persisting, exit non-zero only on Blocker issues.
//!
//! Pairs with the spec in `docs/reference/ai-email.md` (User
//! Journey: `mxr compose --to alice@example.com --body "see attached"
//! --check`).

#![expect(
    clippy::expect_fun_call,
    reason = "integration tests include command output in parse failure messages"
)]

use assert_cmd::prelude::*;
use mxr_test_support::daemon::{daemon_lock, spawn_fake_daemon};
use serde_json::Value;
use std::process::Command;
use tempfile::TempDir;

/// PEM private key in the body must be a Blocker, exiting code 2.
/// This is a non-negotiable correctness check (per
/// `docs/reference/ai-email.md` "PEM private key blocks").
#[test]
fn compose_check_blocks_pem_private_key() {
    let _guard = daemon_lock();
    let temp = TempDir::new().expect("temp dir");
    let (_daemon, instance, data_dir, config_dir) = spawn_fake_daemon(&temp, "compose-check-pii");

    let body = concat!(
        "Here is the key:\n",
        "-----BEGIN RSA ",
        "PRIVATE KEY-----\nMIIabcdef\n",
        "-----END RSA ",
        "PRIVATE KEY-----\n"
    );
    let output = Command::cargo_bin("mxr")
        .expect("mxr bin")
        .env("MXR_INSTANCE", &instance)
        .env("MXR_DATA_DIR", &data_dir)
        .env("MXR_CONFIG_DIR", &config_dir)
        .env_remove("EDITOR")
        .env_remove("VISUAL")
        .args([
            "compose",
            "--to",
            "alice@example.com",
            "--body",
            body,
            "--check",
            "--no-llm",
            "--format",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .clone();

    assert_eq!(
        output.status.code(),
        Some(2),
        "exit code must be 2 for Blocker (got {:?}); stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("utf8");
    let report: Value =
        serde_json::from_str(stdout.trim()).expect(&format!("parse JSON: {stdout}"));

    let issues = report["issues"].as_array().expect("issues array");
    let pii_blocker = issues
        .iter()
        .find(|i| i["code"] == "pii_secret" && i["severity"] == "blocker");
    assert!(
        pii_blocker.is_some(),
        "expected PiiSecret Blocker issue, got: {report:#}"
    );
    assert!(
        !stdout.contains("MIIabcdef"),
        "JSON leaked raw secret material"
    );
}

/// `mxr send <draft-id> --check` must validate the effective From at preview:
/// a stored draft whose `from:` is no longer an owned address (e.g. the alias
/// was removed) fails the check, not just the eventual send. Drives the real
/// `--check` code path end-to-end through the CLI.
#[test]
fn send_check_rejects_a_stored_draft_with_an_unowned_from() {
    let _guard = daemon_lock();
    let temp = TempDir::new().expect("temp dir");
    let (_daemon, instance, data_dir, config_dir) = spawn_fake_daemon(&temp, "send-check-from");

    let run = |args: &[&str]| {
        Command::cargo_bin("mxr")
            .expect("mxr bin")
            .env("MXR_INSTANCE", &instance)
            .env("MXR_DATA_DIR", &data_dir)
            .env("MXR_CONFIG_DIR", &config_dir)
            .env_remove("EDITOR")
            .env_remove("VISUAL")
            .args(args)
            .assert()
    };

    run(&[
        "accounts",
        "addresses",
        "add",
        "--account",
        "fake",
        "alias@example.com",
    ])
    .success();
    let saved = run(&[
        "compose",
        "--from",
        "alias@example.com",
        "--to",
        "alice@example.com",
        "--subject",
        "From check",
        "--body",
        "body",
        "--no-signature",
    ])
    .success()
    .get_output()
    .clone();
    let stdout = String::from_utf8(saved.stdout).expect("utf8");
    let draft_id = stdout
        .lines()
        .find_map(|line| line.strip_prefix("Draft saved: ").map(str::to_string))
        .expect(&format!("compose should save a draft; stdout={stdout}"));
    run(&[
        "accounts",
        "addresses",
        "remove",
        "--account",
        "fake",
        "alias@example.com",
    ])
    .success();

    let failure = run(&["send", &draft_id, "--check"])
        .failure()
        .get_output()
        .clone();
    let stderr = String::from_utf8(failure.stderr).expect("utf8");
    assert!(
        stderr.to_lowercase().contains("invalid from address"),
        "send --check must reject the unowned stored From; stderr={stderr}"
    );
    assert!(
        stderr.contains("fake@example.com"),
        "error should list the registered addresses; stderr={stderr}"
    );
}
