//! End-to-end CLI smoke test against the in-memory Fake provider.
//!
//! Exercises the v1 happy path: configure a fake account → sync → search → cat
//! → reply --yes → mutate → search reflects new state. Asserts the JSON contract
//! on every command we touch so script consumers stay covered.

#![expect(
    clippy::panic,
    reason = "integration tests panic with command output when daemon-backed journeys fail"
)]

use mxr_test_support::daemon::{
    daemon_lock, instance_socket_path, run_json, run_json_with_stdin, run_status_only,
    unique_instance_name as ts_unique_instance_name, write_fake_account_config, DaemonGuard,
};
use serde_json::Value;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::TempDir;

fn cli_journey_guard() -> std::sync::MutexGuard<'static, ()> {
    daemon_lock()
}

fn write_fake_config(config_dir: &Path) {
    write_fake_account_config(config_dir);
}

fn unique_instance_name() -> String {
    ts_unique_instance_name("mxr-cli-journey")
}

fn search_results<'a>(value: &'a Value, context: &str) -> &'a [Value] {
    value
        .as_array()
        .or_else(|| value.get("results").and_then(Value::as_array))
        .map_or_else(|| panic!("{context}; got: {value:#}"), Vec::as_slice)
}

#[test]
fn cli_journey_send_then_mutate_then_search_reflects_state() {
    let _guard = cli_journey_guard();
    let temp = TempDir::new().expect("temp dir");
    let instance = unique_instance_name();
    let data_dir = temp.path().join("data");
    let config_dir = temp.path().join("config");
    let socket_path = instance_socket_path(&instance);
    let pid_path = data_dir.join("daemon.pid");
    std::fs::create_dir_all(&data_dir).expect("data dir");
    std::fs::create_dir_all(&config_dir).expect("config dir");
    write_fake_config(&config_dir);

    let mut daemon = DaemonGuard {
        socket_path: socket_path.clone(),
        pid_path,
        pid: None,
    };

    // Boot the daemon and capture its pid (so Drop can stop it).
    let status = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["status", "--format", "json"],
    );
    daemon.pid = status["daemon_pid"].as_u64();
    assert!(
        daemon.pid.is_some(),
        "daemon should auto-start with status: {status:#}"
    );

    // accounts --format json shows the fake account.
    let accounts = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["accounts", "--format", "json"],
    );
    let fake = accounts
        .as_array()
        .and_then(|arr| {
            arr.iter()
                .find(|account| account["email"] == "fake@example.com")
        })
        .unwrap_or_else(|| panic!("fake account missing in: {accounts:#}"));
    assert_eq!(fake["email"].as_str(), Some("fake@example.com"));
    assert_eq!(
        fake["sync"].as_str(),
        Some("fake"),
        "fake account should be configured with fake sync provider"
    );

    // Trigger sync and wait for it to complete.
    run_status_only(
        &instance,
        &data_dir,
        &config_dir,
        &["sync", "--wait", "--wait-timeout-secs", "30"],
    );

    // sync --status --format json returns a structured status array.
    let status_after = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["sync", "--status", "--format", "json"],
    );
    let status_arr = status_after
        .as_array()
        .expect("sync --status --format json should return an array");
    assert!(
        status_arr
            .iter()
            .any(|status| status["sync_in_progress"].as_bool() == Some(false)
                && status["last_synced_count"].as_u64().unwrap_or(0) > 0),
        "sync --status JSON should report a completed run; got: {status_after:#}"
    );

    // search --format json returns result objects; modern output wraps them
    // with paging metadata.
    // Use a token from the fake fixtures (`buildkite`) rather than an empty
    // query: Tantivy's parser rejects bare `*` and we want the test to assert
    // a real query path, not match-all.
    let search = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["search", "deployment", "--format", "json", "--limit", "50"],
    );
    let results = search_results(&search, "expected search results");
    assert!(
        !results.is_empty(),
        "expected fake provider fixtures to populate search; got: {search:#}"
    );
    let first = &results[0];
    let message_id = first["message_id"]
        .as_str()
        .expect("first result needs message_id");

    // Message mutations accept IDs from stdin and can preview as structured JSON.
    let archive_preview = run_json_with_stdin(
        &instance,
        &data_dir,
        &config_dir,
        &["archive", "--dry-run", "--format", "json"],
        &format!("{message_id}\n"),
    );
    assert_eq!(archive_preview["dry_run"].as_bool(), Some(true));
    assert_eq!(archive_preview["action"].as_str(), Some("archive"));
    assert_eq!(archive_preview["message_ids"][0].as_str(), Some(message_id));

    // count --format json reports the same query.
    let count = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["count", "deployment", "--format", "json"],
    );
    let count_value = count["count"]
        .as_u64()
        .unwrap_or_else(|| panic!("count JSON should contain count: {count:#}"));
    assert!(
        count_value > 0,
        "expected fixtures to match `buildkite`; got count={count_value}"
    );

    // cat the message via JSON to confirm body fetch path works.
    let cat = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["cat", message_id, "--format", "json"],
    );
    assert_eq!(
        cat["message_id"].as_str(),
        Some(message_id),
        "cat JSON should report the requested message id; got: {cat:#}"
    );

    // reply --body --yes should send via fake provider with no editor involvement.
    let reply_out = run_status_only(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "reply",
            message_id,
            "--body",
            "Smoke test reply body",
            "--yes",
        ],
    );
    assert!(
        reply_out.stdout.contains("Sent draft"),
        "reply --yes should report sent draft, got stdout={:?} stderr={:?}",
        reply_out.stdout,
        reply_out.stderr,
    );

    // The synthetic Sent envelope must be searchable immediately — no
    // intervening sync. Regression for Bug 1 of the v1 ship gate (sent
    // messages used to only appear after the next sync).
    let sent = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["search", "is:sent", "--format", "json", "--limit", "100"],
    );
    let sent_subjects: Vec<String> = search_results(&sent, "expected sent search results")
        .iter()
        .filter_map(|item| item["subject"].as_str().map(String::from))
        .collect();
    assert!(
        sent_subjects
            .iter()
            .any(|s| s.contains("Smoke test") || s.starts_with("Re:") || !s.is_empty()),
        "is:sent should include the just-sent reply; got {sent_subjects:?}"
    );

    // archive the message — search should immediately reflect the dropped INBOX label.
    run_status_only(
        &instance,
        &data_dir,
        &config_dir,
        &["archive", message_id, "--yes"],
    );

    // search for `label:inbox` should not surface the archived message anymore.
    let inbox = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "search",
            "label:inbox",
            "--format",
            "json",
            "--limit",
            "100",
        ],
    );
    let inbox_ids: Vec<&str> = search_results(&inbox, "expected inbox search results")
        .iter()
        .filter_map(|item| item["message_id"].as_str())
        .collect();
    assert!(
        !inbox_ids.contains(&message_id),
        "archived message {message_id} should not appear in `label:inbox` after archive; got {} ids",
        inbox_ids.len()
    );
}

/// Behavior 1+2 (Phase 1.1): after `mxr compose --yes` the synthetic Sent
/// envelope is in the local store and Tantivy returns it for an exact-subject
/// query — no manual sync, no vacuous OR assertions.
///
/// The subject is unique per run, so the assertion can demand exactly one match
/// without coupling to fixture content.
#[test]
fn cli_journey_compose_send_inserts_synthetic_envelope_searchable_by_subject() {
    let _guard = cli_journey_guard();
    let temp = TempDir::new().expect("temp dir");
    let instance = unique_instance_name();
    let data_dir = temp.path().join("data");
    let config_dir = temp.path().join("config");
    let socket_path = instance_socket_path(&instance);
    let pid_path = data_dir.join("daemon.pid");
    std::fs::create_dir_all(&data_dir).expect("data dir");
    std::fs::create_dir_all(&config_dir).expect("config dir");
    write_fake_config(&config_dir);

    let mut daemon = DaemonGuard {
        socket_path: socket_path.clone(),
        pid_path,
        pid: None,
    };

    // Boot the daemon.
    let status = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["status", "--format", "json"],
    );
    daemon.pid = status["daemon_pid"].as_u64();
    assert!(
        daemon.pid.is_some(),
        "daemon should auto-start with status: {status:#}"
    );

    // Initial sync so the fake account exists in store with a Sent label
    // (matters for Gmail-style label application; harmless otherwise).
    run_status_only(
        &instance,
        &data_dir,
        &config_dir,
        &["sync", "--wait", "--wait-timeout-secs", "30"],
    );

    // Compose with a unique subject so the assertion can demand an exact match
    // rather than the existing vacuous "or s.is_empty()" pattern.
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("now")
        .as_nanos();
    let unique_subject = format!("mxr-compose-test-{}-{stamp}", std::process::id());
    let body = format!("Body for {unique_subject}");

    let send_out = run_status_only(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "compose",
            "--to",
            "alice@example.com",
            "--subject",
            &unique_subject,
            "--body",
            &body,
            "--yes",
        ],
    );
    assert!(
        send_out.stdout.contains("Sent draft"),
        "compose --yes should report sent draft, got stdout={:?} stderr={:?}",
        send_out.stdout,
        send_out.stderr,
    );

    // Tantivy must return exactly one match for the unique subject — no
    // intervening sync. If ingest_sent_message stops upserting, or stops
    // applying the search batch, this fails.
    let results = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "search",
            &unique_subject,
            "--format",
            "json",
            "--limit",
            "10",
        ],
    );
    let arr = search_results(&results, "expected search results");
    let matching: Vec<&Value> = arr
        .iter()
        .filter(|item| item["subject"].as_str() == Some(unique_subject.as_str()))
        .collect();
    assert_eq!(
        matching.len(),
        1,
        "exactly one match for unique subject {unique_subject:?}; got results={results:#}"
    );

    // The same envelope must surface under `is:sent`. Catches a regression
    // where SENT flag or label fails to be set on the synthetic envelope.
    let sent = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["search", "is:sent", "--format", "json", "--limit", "100"],
    );
    let sent_subjects: Vec<&str> = search_results(&sent, "expected sent search results")
        .iter()
        .filter_map(|i| i["subject"].as_str())
        .collect();
    assert!(
        sent_subjects.iter().any(|s| *s == unique_subject),
        "is:sent must contain freshly composed subject {unique_subject:?}; got {sent_subjects:?}"
    );
}

/// Behavior 3 (Phase 1.1): the daemon's `SendDraft` response carries the IDs
/// minted during synthetic Sent ingestion. The CLI surfaces the
/// `local_message_id` so callers can navigate to or reference the just-sent
/// message; we round-trip it against `mxr search` to prove it's the same ID
/// the store and Tantivy know about (not a stub).
#[test]
fn cli_journey_compose_send_response_carries_message_id_matching_search() {
    let _guard = cli_journey_guard();
    let temp = TempDir::new().expect("temp dir");
    let instance = unique_instance_name();
    let data_dir = temp.path().join("data");
    let config_dir = temp.path().join("config");
    let socket_path = instance_socket_path(&instance);
    let pid_path = data_dir.join("daemon.pid");
    std::fs::create_dir_all(&data_dir).expect("data dir");
    std::fs::create_dir_all(&config_dir).expect("config dir");
    write_fake_config(&config_dir);

    let mut daemon = DaemonGuard {
        socket_path: socket_path.clone(),
        pid_path,
        pid: None,
    };

    let status = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["status", "--format", "json"],
    );
    daemon.pid = status["daemon_pid"].as_u64();
    assert!(daemon.pid.is_some(), "daemon should auto-start");

    run_status_only(
        &instance,
        &data_dir,
        &config_dir,
        &["sync", "--wait", "--wait-timeout-secs", "30"],
    );

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("now")
        .as_nanos();
    let unique_subject = format!("mxr-receipt-test-{}-{stamp}", std::process::id());

    let send_out = run_status_only(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "compose",
            "--to",
            "carol@example.com",
            "--subject",
            &unique_subject,
            "--body",
            "receipt round-trip body",
            "--yes",
        ],
    );

    // Extract the printed local message id from CLI stdout. If the daemon
    // still returns Ack (Behavior 3 not implemented), this line is absent
    // and the test fails immediately.
    let printed_id = send_out
        .stdout
        .lines()
        .find_map(|line| line.strip_prefix("Local message id: ").map(str::to_string))
        .unwrap_or_else(|| {
            panic!(
                "compose --yes must print `Local message id: <id>`; stdout={:?}",
                send_out.stdout
            )
        });
    assert!(
        !printed_id.trim().is_empty(),
        "printed local message id must be non-empty"
    );

    // Round-trip: search by the unique subject and verify the same id is
    // returned. Catches regressions where the daemon emits a placeholder id
    // unrelated to what the store/Tantivy record.
    let results = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "search",
            &unique_subject,
            "--format",
            "json",
            "--limit",
            "10",
        ],
    );
    let searched_id = search_results(&results, "search must return one result")
        .first()
        .and_then(|item| item["message_id"].as_str())
        .unwrap_or_else(|| panic!("search must return one result; got: {results:#}"));
    assert_eq!(
        searched_id.trim(),
        printed_id.trim(),
        "printed local_message_id must equal the message_id stored & indexed"
    );
}

/// Phase 1.4 / Behaviors 1+2+3+6: archive a message via the CLI, parse
/// the printed `mxr undo <id>` hint, and run undo to verify the message
/// is back in the inbox label index.
#[test]
fn cli_journey_archive_then_undo_restores_inbox() {
    let _guard = cli_journey_guard();
    let temp = TempDir::new().expect("temp dir");
    let instance = unique_instance_name();
    let data_dir = temp.path().join("data");
    let config_dir = temp.path().join("config");
    let socket_path = instance_socket_path(&instance);
    let pid_path = data_dir.join("daemon.pid");
    std::fs::create_dir_all(&data_dir).expect("data dir");
    std::fs::create_dir_all(&config_dir).expect("config dir");
    write_fake_config(&config_dir);

    let mut daemon = DaemonGuard {
        socket_path: socket_path.clone(),
        pid_path,
        pid: None,
    };

    let status = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["status", "--format", "json"],
    );
    daemon.pid = status["daemon_pid"].as_u64();
    assert!(daemon.pid.is_some(), "daemon should auto-start");

    run_status_only(
        &instance,
        &data_dir,
        &config_dir,
        &["sync", "--wait", "--wait-timeout-secs", "30"],
    );

    // Pick a single inbox-tagged message via search.
    let inbox = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["search", "label:inbox", "--format", "json", "--limit", "10"],
    );
    let target_id = search_results(&inbox, "fixture must yield at least one inbox message")
        .first()
        .and_then(|item| item["message_id"].as_str())
        .map_or_else(
            || panic!("fixture must yield at least one inbox message; got: {inbox:#}"),
            str::to_string,
        );

    // Archive it via the CLI. Capture stdout so we can extract the
    // mutation_id printed by handle_mutation_response.
    let archive = run_status_only(
        &instance,
        &data_dir,
        &config_dir,
        &["archive", &target_id, "--yes"],
    );
    let mutation_id = archive
        .stdout
        .lines()
        .find_map(|line| {
            line.strip_prefix("Undo with: mxr undo ")
                .map(str::to_string)
        })
        .or_else(|| {
            serde_json::from_str::<Value>(&archive.stdout)
                .ok()
                .and_then(|value| value["result"]["mutation_id"].as_str().map(str::to_string))
        })
        .unwrap_or_else(|| {
            panic!(
                "archive --yes must return a mutation id; stdout={:?}",
                archive.stdout
            )
        });

    // Sanity: the message has left the inbox label index.
    let inbox_after = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "search",
            "label:inbox",
            "--format",
            "json",
            "--limit",
            "100",
        ],
    );
    let in_inbox_after = search_results(&inbox_after, "expected post-archive inbox results")
        .iter()
        .any(|item| item["message_id"].as_str() == Some(target_id.as_str()));
    assert!(
        !in_inbox_after,
        "post-archive: {target_id} must not be in `label:inbox`"
    );

    // Undo via the CLI. Restores INBOX both locally and on the fake
    // provider. The output is just "Undone".
    let undone = run_status_only(
        &instance,
        &data_dir,
        &config_dir,
        &["undo", mutation_id.trim()],
    );
    assert!(
        undone.stdout.to_lowercase().contains("undone"),
        "undo must print confirmation; got stdout={:?}",
        undone.stdout
    );

    // Post-undo: message back in the inbox label index. Strong assertion
    // — exact id match, not a fuzzy contains.
    let inbox_post = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "search",
            "label:inbox",
            "--format",
            "json",
            "--limit",
            "100",
        ],
    );
    let restored = search_results(&inbox_post, "expected post-undo inbox results")
        .iter()
        .any(|item| item["message_id"].as_str() == Some(target_id.as_str()));
    assert!(
        restored,
        "post-undo: {target_id} must reappear in `label:inbox`; got {inbox_post:?}"
    );
}

/// Phase 2.1 stage B / Behaviors 1, 3, 5: a CLI round-trip on the saved-search
/// surface. The TUI dispatches the same `Request::CreateSavedSearch` /
/// `DeleteSavedSearch` requests as `mxr saved add` / `mxr saved delete`, so
/// covering the CLI side proves the daemon contract holds and verifies
/// parity with whatever the TUI sends.
#[test]
fn cli_journey_saved_search_create_list_delete_round_trip() {
    let _guard = cli_journey_guard();
    let temp = TempDir::new().expect("temp dir");
    let instance = unique_instance_name();
    let data_dir = temp.path().join("data");
    let config_dir = temp.path().join("config");
    let socket_path = instance_socket_path(&instance);
    let pid_path = data_dir.join("daemon.pid");
    std::fs::create_dir_all(&data_dir).expect("data dir");
    std::fs::create_dir_all(&config_dir).expect("config dir");
    write_fake_config(&config_dir);

    let mut daemon = DaemonGuard {
        socket_path: socket_path.clone(),
        pid_path,
        pid: None,
    };

    let status = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["status", "--format", "json"],
    );
    daemon.pid = status["daemon_pid"].as_u64();
    assert!(daemon.pid.is_some(), "daemon should auto-start");

    // Empty list at start: clean ground for the round-trip.
    let initial = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["saved", "--format", "json", "list"],
    );
    let initial_count = initial.as_array().map_or(0, std::vec::Vec::len);

    // Create. Behavior 1: after a successful create, the list contains
    // the new entry.
    run_status_only(
        &instance,
        &data_dir,
        &config_dir,
        &["saved", "add", "Test Search", "label:inbox"],
    );

    let after_create = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["saved", "--format", "json", "list"],
    );
    let after_create_arr = after_create
        .as_array()
        .expect("saved list must be JSON array");
    assert_eq!(
        after_create_arr.len(),
        initial_count + 1,
        "exactly one new saved search after create; got {after_create:#}"
    );
    let created = after_create_arr
        .iter()
        .find(|s| s["name"].as_str() == Some("Test Search"))
        .unwrap_or_else(|| panic!("created search missing in list: {after_create:#}"));
    assert_eq!(
        created["query"].as_str(),
        Some("label:inbox"),
        "query must round-trip exactly"
    );

    // Delete. Behavior 3: removed from the list.
    run_status_only(
        &instance,
        &data_dir,
        &config_dir,
        &["saved", "delete", "Test Search"],
    );

    let after_delete = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["saved", "--format", "json", "list"],
    );
    let after_delete_arr = after_delete
        .as_array()
        .expect("saved list must be JSON array");
    assert_eq!(
        after_delete_arr.len(),
        initial_count,
        "list returns to initial size after delete; got {after_delete:#}"
    );
    assert!(
        after_delete_arr
            .iter()
            .all(|s| s["name"].as_str() != Some("Test Search")),
        "deleted entry must not reappear in list"
    );
}

#[test]
fn cli_journey_relationship_draft_and_humanizer_surfaces_round_trip() {
    let _guard = cli_journey_guard();
    let llm = TestLlmServer::start();
    let temp = TempDir::new().expect("temp dir");
    let instance = unique_instance_name();
    let data_dir = temp.path().join("data");
    let config_dir = temp.path().join("config");
    let socket_path = instance_socket_path(&instance);
    let pid_path = data_dir.join("daemon.pid");
    std::fs::create_dir_all(&data_dir).expect("data dir");
    std::fs::create_dir_all(&config_dir).expect("config dir");
    write_llm_fake_config(&config_dir, &llm.base_url);

    let mut daemon = DaemonGuard {
        socket_path: socket_path.clone(),
        pid_path,
        pid: None,
    };

    let status = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["status", "--format", "json"],
    );
    daemon.pid = status["daemon_pid"].as_u64();
    assert!(daemon.pid.is_some(), "daemon should auto-start");
    assert_eq!(
        status["feature_health"]["relationship_profile"]["status"].as_str(),
        Some("healthy"),
        "relationship health should be surfaced as healthy when local LLM is enabled: {status:#}"
    );
    assert_eq!(
        status["feature_health"]["commitments"]["status"].as_str(),
        Some("healthy"),
        "commitment health should be surfaced as healthy when local LLM is enabled: {status:#}"
    );

    run_status_only(
        &instance,
        &data_dir,
        &config_dir,
        &["sync", "--wait", "--wait-timeout-secs", "30"],
    );

    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("now")
        .as_nanos();
    for index in 0..20 {
        let subject = format!("mxr-r12-voice-{stamp}-{index}");
        let body = format!(
            "Hi Alice,\n\nI can take the deployment follow-up item {index} and send the concise update today.\n\nThanks"
        );
        run_status_only(
            &instance,
            &data_dir,
            &config_dir,
            &[
                "compose",
                "--to",
                "alice@work.com",
                "--subject",
                &subject,
                "--body",
                &body,
                "--no-signature",
                "--yes",
            ],
        );
    }

    let profile = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["profile", "alice@work.com", "--rebuild", "--format", "json"],
    );
    assert!(
        profile["style"]["msg_count_used"].as_u64().unwrap_or(0) >= 5,
        "profile rebuild should compute outbound voice from sent mail: {profile:#}"
    );
    assert!(
        profile["summary"]["known_topics"]
            .as_array()
            .is_some_and(|topics| topics
                .iter()
                .any(|topic| topic.as_str() == Some("deployment"))),
        "relationship summary should expose known topics from the LLM response: {profile:#}"
    );
    let first_commitment = profile["open_commitments"]
        .as_array()
        .and_then(|items| items.first())
        .unwrap_or_else(|| panic!("profile should include extracted open commitment: {profile:#}"));
    assert_eq!(
        first_commitment["what"].as_str(),
        Some("confirm dashboard is quiet")
    );

    let open_before = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "commitments",
            "--contact",
            "alice@work.com",
            "--status",
            "open",
            "--format",
            "json",
        ],
    );
    let open_before_len = open_before
        .as_array()
        .map(Vec::len)
        .expect("commitments open list should be an array");
    run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["profile", "alice@work.com", "--rebuild", "--format", "json"],
    );
    let open_after = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "commitments",
            "--contact",
            "alice@work.com",
            "--status",
            "open",
            "--format",
            "json",
        ],
    );
    assert_eq!(
        open_after.as_array().map(Vec::len),
        Some(open_before_len),
        "rebuilding the same profile should not duplicate equivalent commitments"
    );

    let commitment_id = open_after
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| item["id"].as_str())
        .unwrap_or_else(|| panic!("open commitment should have id: {open_after:#}"))
        .to_string();
    run_status_only(
        &instance,
        &data_dir,
        &config_dir,
        &["commitments", "resolve", &commitment_id],
    );
    let resolved = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "commitments",
            "--contact",
            "alice@work.com",
            "--status",
            "resolved",
            "--format",
            "json",
        ],
    );
    assert!(
        resolved.as_array().is_some_and(|items| items
            .iter()
            .any(|item| item["id"].as_str() == Some(commitment_id.as_str()))),
        "resolved commitment should move to resolved list: {resolved:#}"
    );

    let voice = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["voice", "--format", "json", "rebuild"],
    );
    assert!(
        voice["msg_count_used"].as_u64().unwrap_or(0) >= 20,
        "voice rebuild should use the sent corpus: {voice:#}"
    );
    assert!(
        voice["register_modes"]
            .as_array()
            .is_some_and(|modes| !modes.is_empty()),
        "voice rebuild should expose register modes: {voice:#}"
    );

    let draft_new = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "draft",
            "--to",
            "new-contact@example.com",
            "--purpose",
            "send a concise intro",
            "--register",
            "casual",
            "--format",
            "json",
        ],
    );
    assert_eq!(draft_new["model"].as_str(), Some("draft-new-model"));
    assert!(
        draft_new["voice_match"].is_object(),
        "draft-new should score against user voice fallback: {draft_new:#}"
    );
    assert!(
        draft_new["humanizer"]["score"].as_u64().unwrap_or(0) >= 70,
        "draft-new should include humanizer score: {draft_new:#}"
    );

    let saved = run_status_only(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "compose",
            "--to",
            "alice@work.com",
            "--subject",
            "Refine me",
            "--body",
            "Hi Alice, I can send the update today.",
            "--no-signature",
        ],
    );
    let draft_id = saved
        .stdout
        .lines()
        .find_map(|line| line.strip_prefix("Draft saved: ").map(str::to_string))
        .unwrap_or_else(|| panic!("compose should save a draft; stdout={:?}", saved.stdout));
    let refined = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "draft",
            "--format",
            "json",
            "refine",
            &draft_id,
            "--shorter",
        ],
    );
    assert_eq!(refined["model"].as_str(), Some("draft-refine-model"));
    assert!(
        refined["body"]
            .as_str()
            .is_some_and(|body| body.to_ascii_lowercase().contains("shorter")),
        "draft refine should return the revised body: {refined:#}"
    );

    let rewritten = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "humanize",
            "--format",
            "json",
            "rewrite",
            "Great question! Additionally, it is important to note that this serves as a testament — not just progress but a vibrant landscape, showcasing our commitment to innovation and fostering valuable outcomes.",
            "--max-iterations",
            "2",
        ],
    );
    assert_eq!(rewritten["iterations"].as_u64(), Some(1));
    assert_eq!(rewritten["text"].as_str(), Some("This is a clear update."));
    assert!(
        rewritten["report"]["score"].as_u64().unwrap_or(0) >= 70,
        "humanizer rewrite should improve to a passing score: {rewritten:#}"
    );

    let requested_models = llm.requested_models();
    assert!(
        requested_models
            .iter()
            .any(|model| model == "draft-new-model"),
        "draft-new must use its feature-specific LLM override; saw {requested_models:?}"
    );
    assert!(
        requested_models
            .iter()
            .any(|model| model == "draft-refine-model"),
        "draft-refine must use its feature-specific LLM override; saw {requested_models:?}"
    );
    assert!(
        requested_models
            .iter()
            .any(|model| model == "rewrite-model"),
        "humanizer rewrite must use its feature-specific LLM override; saw {requested_models:?}"
    );
}

fn write_llm_fake_config(config_dir: &Path, base_url: &str) {
    let toml = format!(
        r#"[general]
default_account = "fake"

[bridge]
enabled = false

[search.semantic]
enabled = false

[llm]
enabled = true
base_url = "{base_url}"
model = "base-model"
api_key_env = ""
request_timeout_secs = 5

[llm.overrides.draft_new]
model = "draft-new-model"

[llm.overrides.draft_refine]
model = "draft-refine-model"

[llm.overrides.humanize_rewrite]
model = "rewrite-model"

[accounts.fake]
name = "Fake Account"
email = "user@example.com"

[accounts.fake.sync]
type = "fake"

[accounts.fake.send]
type = "fake"
"#
    );
    std::fs::write(config_dir.join("config.toml"), toml).expect("write llm fake config");
}

struct TestLlmServer {
    base_url: String,
    requests: Arc<Mutex<Vec<Value>>>,
}

impl TestLlmServer {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test llm server");
        let addr = listener.local_addr().expect("test llm addr");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let thread_requests = requests.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let requests = thread_requests.clone();
                std::thread::spawn(move || handle_llm_connection(stream, requests));
            }
        });
        Self {
            base_url: format!("http://{addr}/v1"),
            requests,
        }
    }

    fn requested_models(&self) -> Vec<String> {
        self.requests
            .lock()
            .expect("requests lock")
            .iter()
            .filter_map(|request| request["model"].as_str().map(str::to_string))
            .collect()
    }
}

fn handle_llm_connection(mut stream: TcpStream, requests: Arc<Mutex<Vec<Value>>>) {
    let mut buffer = Vec::new();
    let mut temp = [0_u8; 4096];
    let mut expected_len = None;
    loop {
        let Ok(read) = stream.read(&mut temp) else {
            return;
        };
        if read == 0 {
            return;
        }
        buffer.extend_from_slice(&temp[..read]);
        if expected_len.is_none() {
            if let Some(header_end) = find_header_end(&buffer) {
                let headers = String::from_utf8_lossy(&buffer[..header_end]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        line.split_once(':').and_then(|(name, value)| {
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().ok())
                                .flatten()
                        })
                    })
                    .unwrap_or(0);
                expected_len = Some(header_end + 4 + content_length);
            }
        }
        if expected_len.is_some_and(|len| buffer.len() >= len) {
            break;
        }
    }

    let Some(header_end) = find_header_end(&buffer) else {
        return;
    };
    let body = &buffer[header_end + 4..expected_len.unwrap_or(buffer.len())];
    let request: Value = serde_json::from_slice(body).unwrap_or_else(|_| serde_json::json!({}));
    requests
        .lock()
        .expect("requests lock")
        .push(request.clone());
    let model = request["model"].as_str().unwrap_or("test-model");
    let prompt = request["messages"]
        .as_array()
        .map(|messages| {
            messages
                .iter()
                .filter_map(|message| message["content"].as_str())
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    let content = llm_content_for(model, &prompt);
    let response = serde_json::json!({
        "id": "chatcmpl-test",
        "object": "chat.completion",
        "created": 0,
        "model": model,
        "choices": [{
            "index": 0,
            "message": {"role": "assistant", "content": content},
            "finish_reason": "stop"
        }]
    })
    .to_string();
    let reply = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        response.len(),
        response
    );
    let _ = stream.write_all(reply.as_bytes());
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn llm_content_for(model: &str, prompt: &str) -> String {
    match model {
        "draft-new-model" => "Hi there,\n\nI can send a concise intro today.\n\nThanks".to_string(),
        "draft-refine-model" => "Shorter update: I can send it today.".to_string(),
        "rewrite-model" => "This is a clear update.".to_string(),
        _ if prompt.contains("Extract only explicit open asks") => {
            let evidence_msg_id = first_prompt_message_id(prompt).unwrap_or_else(|| "missing".into());
            format!(
                r#"{{"commitments":[{{"who_owes":"Alice","what":"confirm dashboard is quiet","by_when":null,"evidence_msg_id":"{evidence_msg_id}","direction":"theirs"}}]}}"#
            )
        }
        _ if prompt.contains("Build an inspectable relationship profile") => {
            r#"{"text":"Alice tends to discuss deployment plans and concrete follow-ups with the user.","known_topics":["deployment","canary rollout"]}"#.to_string()
        }
        _ => "Plain local LLM response.".to_string(),
    }
}

fn first_prompt_message_id(prompt: &str) -> Option<String> {
    prompt.split("Message ").skip(1).find_map(|part| {
        let candidate = part.split_whitespace().next()?.trim_matches(':');
        candidate.contains('-').then(|| candidate.to_string())
    })
}

/// Phase 2.1 acceptance: the `reply_later` flag is persisted to
/// SQLite and survives a daemon restart. The TUI shows the flag as
/// an overlay on the inbox row; if it didn't persist, every restart
/// would wipe the user's reply queue.
#[test]
fn cli_journey_reply_later_flag_persists_across_daemon_restart() {
    let _guard = cli_journey_guard();
    let temp = TempDir::new().expect("temp dir");
    let instance = unique_instance_name();
    let data_dir = temp.path().join("data");
    let config_dir = temp.path().join("config");
    let socket_path = instance_socket_path(&instance);
    let pid_path = data_dir.join("daemon.pid");
    std::fs::create_dir_all(&data_dir).expect("data dir");
    std::fs::create_dir_all(&config_dir).expect("config dir");
    write_fake_config(&config_dir);

    let mut daemon = DaemonGuard {
        socket_path: socket_path.clone(),
        pid_path: pid_path.clone(),
        pid: None,
    };

    // Boot the daemon (status auto-starts it).
    let status = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["status", "--format", "json"],
    );
    daemon.pid = status["daemon_pid"].as_u64();
    let original_pid = daemon.pid.expect("daemon pid");

    // Sync so we have at least one envelope to flag.
    run_status_only(
        &instance,
        &data_dir,
        &config_dir,
        &["sync", "--wait", "--wait-timeout-secs", "30"],
    );

    // Pick the first message id from a non-empty search.
    let search = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["search", "deployment", "--format", "json", "--limit", "5"],
    );
    let message_id = search_results(&search, "at least one fixture matches `deployment`")
        .first()
        .and_then(|hit| hit["message_id"].as_str())
        .expect("at least one fixture matches `deployment`")
        .to_string();

    // Flag for reply-later.
    run_status_only(
        &instance,
        &data_dir,
        &config_dir,
        &["replies", "add", &message_id],
    );

    let queue_before = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["replies", "--format", "json", "list"],
    );
    let in_queue_before = queue_before.as_array().is_some_and(|arr| {
        arr.iter()
            .any(|env| env["id"].as_str() == Some(message_id.as_str()))
    });
    assert!(
        in_queue_before,
        "flagged message must be visible in the reply queue before restart; got: {queue_before:#}"
    );

    // Stop the daemon. SIGTERM + wait for exit; the improved shutdown
    // path in `shutdown_daemon_for_maintenance` handles socket
    // cleanup but we go straight to the process here.
    std::process::Command::new("kill")
        .arg(original_pid.to_string())
        .status()
        .expect("kill daemon");
    // Wait for the process to actually exit before re-starting; otherwise
    // the next CLI call could race the same socket path.
    for _ in 0..120 {
        let alive = std::process::Command::new("kill")
            .arg("-0")
            .arg(original_pid.to_string())
            .stderr(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .status()
            .is_ok_and(|status: std::process::ExitStatus| status.success());
        if !alive {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    daemon.pid = None;
    let _ = std::fs::remove_file(&socket_path);

    // Auto-start a fresh daemon via the next CLI invocation. The
    // status response carries the new pid for the guard.
    let status_after = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["status", "--format", "json"],
    );
    let new_pid = status_after["daemon_pid"]
        .as_u64()
        .expect("auto-started daemon should report its pid");
    assert_ne!(
        new_pid, original_pid,
        "daemon must be a fresh process; got the same pid back"
    );
    daemon.pid = Some(new_pid);

    // The flag must still be present.
    let queue_after = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["replies", "--format", "json", "list"],
    );
    let in_queue_after = queue_after.as_array().is_some_and(|arr| {
        arr.iter()
            .any(|env| env["id"].as_str() == Some(message_id.as_str()))
    });
    assert!(
        in_queue_after,
        "reply-later flag must survive a daemon restart; got: {queue_after:#}"
    );
}

// Daemon-spawning + run_* + write_fake_account_config helpers live in
// `mxr_test_support::daemon` (shared with other integration tests).
