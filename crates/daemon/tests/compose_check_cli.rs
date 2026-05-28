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

/// `mxr compose --to ... --body "see attached" --check --format json`
/// must emit a MissingAttachment Warning and exit 0 (warnings are not
/// blockers). The JSON contract is what scripts consume.
#[test]
fn compose_check_warns_on_missing_attachment_without_files() {
    let _guard = daemon_lock();
    let temp = TempDir::new().expect("temp dir");
    let (_daemon, instance, data_dir, config_dir) =
        spawn_fake_daemon(&temp, "compose-check-attach");

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
            "see attached for the deck",
            "--check",
            "--no-llm",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .clone();

    let stdout = String::from_utf8(output.stdout).expect("utf8");
    let report: Value =
        serde_json::from_str(stdout.trim()).expect(&format!("parse JSON: {stdout}"));

    let issues = report["issues"].as_array().expect("issues array");
    let has_missing_attachment = issues.iter().any(|i| i["code"] == "missing_attachment");
    assert!(
        has_missing_attachment,
        "expected MissingAttachment issue, got: {report:#}"
    );
    // Warning is non-blocking: allowed=true.
    assert_eq!(report["allowed"], Value::Bool(true));
}

/// PEM private key in the body must be a Blocker, exiting code 2.
/// This is a non-negotiable correctness check (per
/// `docs/reference/ai-email.md` "PEM private key blocks").
#[test]
fn compose_check_blocks_pem_private_key() {
    let _guard = daemon_lock();
    let temp = TempDir::new().expect("temp dir");
    let (_daemon, instance, data_dir, config_dir) = spawn_fake_daemon(&temp, "compose-check-pii");

    let body = "Here is the key:\n-----BEGIN RSA PRIVATE KEY-----\nMIIabcdef\n-----END RSA PRIVATE KEY-----\n";
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
    // Verifies the no-raw-secrets contract: JSON must not echo the
    // private key body verbatim.
    assert!(
        !stdout.contains("MIIabcdef"),
        "JSON leaked raw secret material"
    );
}

/// Sanity: a clean draft to a single recipient with a real body
/// passes safety (no blockers, no attachment-keyword false-positive).
/// Confirms the path returns when nothing is wrong.
#[test]
fn compose_check_passes_clean_draft() {
    let _guard = daemon_lock();
    let temp = TempDir::new().expect("temp dir");
    let (_daemon, instance, data_dir, config_dir) = spawn_fake_daemon(&temp, "compose-check-clean");

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
            "Quick note: the meeting is moved to 3pm.",
            "--check",
            "--no-llm",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .get_output()
        .clone();

    let stdout = String::from_utf8(output.stdout).expect("utf8");
    let report: Value = serde_json::from_str(stdout.trim()).expect("parse JSON");
    assert_eq!(report["allowed"], Value::Bool(true));
    let blockers: Vec<_> = report["issues"]
        .as_array()
        .map(|a| a.iter().filter(|i| i["severity"] == "blocker").collect())
        .unwrap_or_default();
    assert!(blockers.is_empty(), "no blockers expected: {report:#}");
}
