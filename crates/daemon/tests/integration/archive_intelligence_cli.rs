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

/// `mxr whois <email> --format json` for a never-seen address must
/// return a valid `EntityExplanation` JSON, not an error. The
/// "no prior interaction" path lives entirely in the daemon — no
/// LLM is required. (Acceptance: 03, line 275.)
#[test]
fn whois_unknown_email_and_term_return_structured_json() {
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

    let resp = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["whois", "ghostproject", "--format", "json"],
    );
    assert_eq!(resp["canonical_name"], "ghostproject");
    assert_eq!(resp["kind"], "unknown");
}
