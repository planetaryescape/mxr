//! Integration tests for `mxr compose` with an HTML body.
//!
//! These drive the real binary against a real daemon, which is the only way to
//! prove the CLI, the IPC boundary and the safety pipeline agree. The three
//! promises under test:
//!
//! * `--check` and `--dry-run` inspect the HTML document the author supplied,
//!   not an empty draft, and create nothing.
//! * `--format json` carries the compose outcome as data. `mxr-mailmerge` reads
//!   `draft_id` out of it, so this is a contract, not a convenience.
//! * A `cid:` reference mxr cannot resolve is a warning; only active content is
//!   a refusal.

#![expect(
    clippy::expect_fun_call,
    reason = "integration tests include command output in parse failure messages"
)]

use assert_cmd::prelude::*;
use mxr_test_support::daemon::{daemon_lock, spawn_fake_daemon};
use serde_json::Value;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

const SAFE_HTML: &str = "<html><body><table><tr><td style=\"color:#333\">\
Quarterly numbers are in the table below.</td></tr></table></body></html>";

/// An AWS access key id inside the HTML only. There is no `--text-file`, so
/// nothing but the document itself carries the secret.
const SECRET_HTML: &str =
    "<html><body><p>the deploy key AKIAIOSFODNN7EXAMPLE ships today</p></body></html>";

const DANGEROUS_HTML: &str = "<html><body><p>hi</p><script>alert(1)</script></body></html>";

/// A 1x1 transparent PNG, so `--inline` has real bytes to point at.
const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
    0x42, 0x60, 0x82,
];

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
}

fn boot(temp: &TempDir, name: &str) -> Env {
    let (daemon, instance, data_dir, config_dir) = spawn_fake_daemon(temp, name);
    Env {
        _daemon: daemon,
        instance,
        data_dir,
        config_dir,
    }
}

fn write(dir: &Path, name: &str, contents: &str) -> String {
    let path = dir.join(name);
    std::fs::write(&path, contents).expect("write fixture");
    path.to_string_lossy().into_owned()
}

/// Before the CLI read the HTML on the `--check` path, this exited 0: the
/// safety pipeline saw an empty markdown draft and pronounced it safe.
#[test]
fn compose_check_refuses_dangerous_html_rather_than_calling_it_safe() {
    let _guard = daemon_lock();
    let temp = TempDir::new().expect("temp dir");
    let env = boot(&temp, "compose-html-dangerous");
    let html = write(temp.path(), "dangerous.html", DANGEROUS_HTML);

    let output = env
        .run(&[
            "compose",
            "--to",
            "alice@example.com",
            "--subject",
            "Numbers",
            "--html-file",
            &html,
            "--check",
            "--no-llm",
            "--format",
            "json",
        ])
        .failure()
        .get_output()
        .clone();

    let stderr = String::from_utf8(output.stderr).expect("utf8");
    assert!(
        stderr.contains("script"),
        "the refusal must name the offending construct; stderr={stderr}"
    );
    assert!(
        stderr.contains("not modified"),
        "the refusal must say the document was left alone; stderr={stderr}"
    );
}

/// The HTML-only draft has no `text/plain` of its own, so unless the generated
/// alternative reaches the safety pipeline every check reads an empty body.
#[test]
fn compose_check_scans_the_html_document_for_secrets() {
    let _guard = daemon_lock();
    let temp = TempDir::new().expect("temp dir");
    let env = boot(&temp, "compose-html-secret");
    let html = write(temp.path(), "secret.html", SECRET_HTML);

    let output = env
        .run(&[
            "compose",
            "--to",
            "alice@example.com",
            "--subject",
            "Deploy",
            "--html-file",
            &html,
            "--check",
            "--no-llm",
            "--format",
            "json",
        ])
        .failure()
        .get_output()
        .clone();

    assert_eq!(
        output.status.code(),
        Some(2),
        "a Blocker must exit 2; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8");
    let report: Value =
        serde_json::from_str(stdout.trim()).expect(&format!("parse JSON: {stdout}"));
    let blocker = report["issues"]
        .as_array()
        .expect("issues array")
        .iter()
        .any(|issue| issue["code"] == "pii_secret" && issue["severity"] == "blocker");
    assert!(blocker, "expected a PiiSecret blocker; report={report:#}");
    assert!(
        !stdout.contains("AKIAIOSFODNN7EXAMPLE"),
        "the report echoed the raw secret back: {stdout}"
    );
}

/// The structured contract external tools read. `mxr-mailmerge` pulls
/// `draft_id` out of exactly this payload.
#[test]
fn compose_draft_with_html_emits_the_structured_json_contract() {
    let _guard = daemon_lock();
    let temp = TempDir::new().expect("temp dir");
    let env = boot(&temp, "compose-html-json");
    let html = write(
        temp.path(),
        "body.html",
        "<html><body><p>Quarterly numbers.</p><img src=\"cid:logo\"></body></html>",
    );
    let logo = temp.path().join("logo.png");
    std::fs::write(&logo, TINY_PNG).expect("write png");
    let attachment = write(temp.path(), "notes.txt", "notes");

    let output = env
        .run(&[
            "compose",
            "--to",
            "alice@example.com",
            "--subject",
            "Quarterly numbers",
            "--html-file",
            &html,
            "--inline",
            &format!("logo={}", logo.display()),
            "--attach",
            &attachment,
            "--draft",
            "--no-signature",
            "--format",
            "json",
        ])
        .success()
        .get_output()
        .clone();

    let stdout = String::from_utf8(output.stdout).expect("utf8");
    let stderr = String::from_utf8(output.stderr).expect("utf8");
    let payload: Value =
        serde_json::from_str(stdout.trim()).expect(&format!("parse JSON: {stdout}"));

    assert!(
        payload["draft_id"]
            .as_str()
            .is_some_and(|id| !id.is_empty()),
        "draft_id is the field mxr-mailmerge reads; payload={payload:#}"
    );
    assert!(
        payload["account_id"]
            .as_str()
            .is_some_and(|id| !id.is_empty()),
        "payload={payload:#}"
    );
    assert_eq!(payload["subject"], "Quarterly numbers");
    assert_eq!(payload["to"], serde_json::json!(["alice@example.com"]));
    assert_eq!(payload["content_kind"], "html");
    assert_eq!(payload["inline_count"], 1);
    assert_eq!(payload["attachment_count"], 1);

    // The bug this replaced: an HTML body with no `--text-file` validated as an
    // empty message, because the draft's analysis text was the empty string.
    assert!(
        !stderr.contains("Message body is empty"),
        "a valid HTML body must not warn about an empty message; stderr={stderr}"
    );

    // And the draft really landed, so the JSON is not describing a no-op.
    let ids = env
        .run(&["drafts", "--format", "ids"])
        .success()
        .get_output()
        .clone();
    let ids = String::from_utf8(ids.stdout).expect("utf8");
    assert_eq!(
        ids.trim(),
        payload["draft_id"].as_str().expect("draft_id"),
        "compose --draft should have stored exactly the draft it reported"
    );
}

/// A `cid:` the author did not wire up is a warning. mxr does not know what the
/// recipient's client can resolve, so it does not overrule the author.
#[test]
fn an_unresolved_cid_reference_warns_without_blocking_the_save() {
    let _guard = daemon_lock();
    let temp = TempDir::new().expect("temp dir");
    let env = boot(&temp, "compose-html-cid");
    let html = write(
        temp.path(),
        "body.html",
        "<html><body><p>hi</p><img src=\"cid:missing-banner\"></body></html>",
    );

    let output = env
        .run(&[
            "compose",
            "--to",
            "alice@example.com",
            "--subject",
            "Banner",
            "--html-file",
            &html,
            "--draft",
            "--no-signature",
            "--format",
            "json",
        ])
        .success()
        .get_output()
        .clone();

    let stderr = String::from_utf8(output.stderr).expect("utf8");
    assert!(
        stderr.contains("missing-banner"),
        "the unresolved reference must be reported; stderr={stderr}"
    );
    let stdout = String::from_utf8(output.stdout).expect("utf8");
    let payload: Value = serde_json::from_str(stdout.trim()).expect("parse JSON");
    assert_eq!(payload["content_kind"], "html");
}

/// `--dry-run` and `--check` are inspection verbs. Neither may leave a draft
/// behind.
#[test]
fn neither_dry_run_nor_check_creates_a_draft() {
    let _guard = daemon_lock();
    let temp = TempDir::new().expect("temp dir");
    let env = boot(&temp, "compose-html-nothing");
    let html = write(temp.path(), "body.html", SAFE_HTML);

    let common = [
        "compose",
        "--to",
        "alice@example.com",
        "--subject",
        "Numbers",
        "--html-file",
    ];

    let mut dry_run = common.to_vec();
    dry_run.extend_from_slice(&[html.as_str(), "--dry-run", "--no-signature"]);
    env.run(&dry_run).success();

    let mut check = common.to_vec();
    check.extend_from_slice(&[html.as_str(), "--check", "--no-llm", "--no-signature"]);
    env.run(&check).success();

    let ids = env
        .run(&["drafts", "--format", "ids"])
        .success()
        .get_output()
        .clone();
    let ids = String::from_utf8(ids.stdout).expect("utf8");
    assert!(
        ids.trim().is_empty(),
        "inspection verbs must create nothing; drafts={ids}"
    );
}
