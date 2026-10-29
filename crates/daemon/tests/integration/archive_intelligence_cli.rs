//! Daemon-backed CLI smoke tests for Tracks 3 & 6 (Archive Intelligence
//! + Knowledge Graph). Pairs with `docs/reference/ai-email.md`.
//!
//! These tests exercise the CLI/socket plumbing rather than the LLM —
//! the fake-provider fixture ships with no LLM configured, so the
//! daemon's "LLM disabled" degradation path is what we assert on the
//! ask and decisions paths. The whois email path is fully LLM-free
//! (it reads the materialized `contacts` table), so we get a real
//! assertion there.

use mxr_test_support::daemon::{daemon_lock, run_json, spawn_fake_daemon};
use serde_json::Value;
use tempfile::TempDir;

/// `mxr ask "<question>" --format json` against an empty archive
/// with the LLM disabled. The daemon must still return a structured
/// JSON answer with `text`, `citations`, and `retrieval` — the
/// spec's acceptance criterion (03, line 101) is the JSON shape, not
/// the answer content.
#[test]
fn ask_with_llm_disabled_returns_structured_json() {
    let _guard = daemon_lock();
    let temp = TempDir::new().expect("temp dir");
    let (_daemon, instance, data_dir, config_dir) = spawn_fake_daemon(&temp, "ask-empty");

    let resp = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["ask", "what did we decide?", "--format", "json"],
    );

    // Top-level shape: text + citations + retrieval.
    assert!(resp["text"].is_string(), "answer.text is a string: {resp}");
    assert!(
        resp["citations"].is_array(),
        "answer.citations is an array: {resp}"
    );
    let retrieval = &resp["retrieval"];
    assert!(
        retrieval["requested_mode"].is_string(),
        "retrieval.requested_mode present: {resp}"
    );
    assert!(
        retrieval["executed_mode"].is_string(),
        "retrieval.executed_mode present: {resp}"
    );
    assert!(
        retrieval["candidate_count"].is_number(),
        "retrieval.candidate_count present: {resp}"
    );
}

/// `mxr decisions --format json` on an empty archive returns an
/// empty array, not an error.
#[test]
fn decisions_list_empty_returns_empty_array() {
    let _guard = daemon_lock();
    let temp = TempDir::new().expect("temp dir");
    let (_daemon, instance, data_dir, config_dir) = spawn_fake_daemon(&temp, "decisions-empty");

    let resp = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["decisions", "--format", "json"],
    );
    let arr = resp.as_array().expect("decisions list is JSON array");
    assert!(arr.is_empty(), "empty archive yields empty list: {resp}");
}

/// `mxr decisions show <id>` for an unknown id must exit non-zero
/// (not return null), so scripts can branch on existence.
#[test]
fn decisions_show_unknown_id_exits_nonzero() {
    let _guard = daemon_lock();
    let temp = TempDir::new().expect("temp dir");
    let (_daemon, instance, data_dir, config_dir) = spawn_fake_daemon(&temp, "decisions-show-404");

    let output = assert_cmd::Command::cargo_bin("mxr")
        .expect("mxr bin")
        .env("MXR_INSTANCE", &instance)
        .env("MXR_DATA_DIR", &data_dir)
        .env("MXR_CONFIG_DIR", &config_dir)
        .env_remove("EDITOR")
        .env_remove("VISUAL")
        .args([
            "decisions",
            "show",
            "nonexistent-id-aabbccdd",
            "--format",
            "json",
        ])
        .assert()
        .failure()
        .get_output()
        .clone();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not found") || stderr.contains("Error"),
        "stderr names the missing id problem: {stderr}"
    );
}

/// `mxr whois <email> --format json` for a never-seen address must
/// return a valid `EntityExplanation` JSON, not an error. The
/// "no prior interaction" path lives entirely in the daemon — no
/// LLM is required. (Acceptance: 03, line 275.)
#[test]
fn whois_unknown_email_returns_structured_json() {
    let _guard = daemon_lock();
    let temp = TempDir::new().expect("temp dir");
    let (_daemon, instance, data_dir, config_dir) = spawn_fake_daemon(&temp, "whois-empty");

    let resp = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["whois", "ghost@example.com", "--format", "json"],
    );
    assert_eq!(resp["canonical_name"], "ghost@example.com");
    assert_eq!(resp["kind"], "person");
    assert!(
        resp["summary"]
            .as_str()
            .is_some_and(|s| s.contains("No prior interaction")),
        "summary says no prior interaction: {resp}"
    );
    assert!(
        resp["citations"]
            .as_array()
            .is_some_and(std::vec::Vec::is_empty),
        "no citations for unknown contact: {resp}"
    );
}

/// `mxr whois <term>` for a term with zero corpus must also return a
/// valid JSON (kind=unknown, no citations) — the daemon path differs
/// from the email-shaped one and we want both wired.
#[test]
fn whois_unknown_term_returns_structured_json() {
    let _guard = daemon_lock();
    let temp = TempDir::new().expect("temp dir");
    let (_daemon, instance, data_dir, config_dir) = spawn_fake_daemon(&temp, "whois-term");

    let resp = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["whois", "ghostproject", "--format", "json"],
    );
    assert_eq!(resp["canonical_name"], "ghostproject");
    assert_eq!(resp["kind"], "unknown");
    let _ = Value::Null; // anchor — keep Value import meaningful
}
