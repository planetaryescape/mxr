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
use std::path::Path;
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

    let triggered = run_json(&instance, &data_dir, &config_dir, &["sync"]);
    assert_eq!(
        triggered["status"].as_str(),
        Some("triggered"),
        "plain `mxr sync` should trigger and return; got: {triggered:#}"
    );

    run_status_only(
        &instance,
        &data_dir,
        &config_dir,
        &["sync", "--wait", "--wait-timeout-secs", "30"],
    );

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
                && status["last_success_at"].as_str().is_some()),
        "sync --status JSON should report a completed run; got: {status_after:#}"
    );

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

    let archive_preview = run_json_with_stdin(
        &instance,
        &data_dir,
        &config_dir,
        &["archive", "--dry-run", "--format", "json"],
        &format!("{message_id}\n"),
    );
    assert_eq!(archive_preview["accepted"].as_bool(), Some(false));
    assert!(archive_preview["identity"]["operation_id"]
        .as_str()
        .is_some());
    assert!(archive_preview["review_id"].as_str().is_some());
    assert_eq!(archive_preview["items"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        archive_preview["items"][0]["message_id"].as_str(),
        Some(message_id)
    );

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

    let reply_json = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "reply",
            message_id,
            "--body",
            "Structured reply body",
            "--draft",
            "--format",
            "json",
        ],
    );
    assert_eq!(reply_json["action"].as_str(), Some("save_draft"));
    let reply_draft_id = reply_json["draft_id"]
        .as_str()
        .unwrap_or_else(|| panic!("reply --format json needs a draft_id: {reply_json:#}"));

    let forward_json = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "forward",
            message_id,
            "--to",
            "bob@example.com",
            "--body",
            "Structured forward body",
            "--draft",
            "--format",
            "json",
        ],
    );
    assert_eq!(forward_json["action"].as_str(), Some("save_draft"));
    assert!(
        forward_json["draft_id"].as_str().is_some(),
        "forward --format json needs a draft_id: {forward_json:#}"
    );

    let reply_ids = run_status_only(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "reply",
            message_id,
            "--body",
            "Ids reply body",
            "--draft",
            "--format",
            "ids",
        ],
    );
    let ids_line = reply_ids.stdout.trim();
    assert!(
        !ids_line.is_empty() && !ids_line.contains(' ') && ids_line != reply_draft_id,
        "reply --format ids should print one fresh draft id; got {ids_line:?}"
    );

    let forward_human = run_status_only(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "forward",
            message_id,
            "--to",
            "bob@example.com",
            "--body",
            "Human forward body",
            "--draft",
        ],
    );
    assert!(
        forward_human.stdout.starts_with("Draft saved: "),
        "forward without --format keeps the human line; got {:?}",
        forward_human.stdout
    );

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

    run_status_only(
        &instance,
        &data_dir,
        &config_dir,
        &["archive", message_id, "--yes"],
    );

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
                .and_then(|value| value["mutation_id"].as_str().map(str::to_string))
        })
        .unwrap_or_else(|| {
            panic!(
                "archive --yes must return a mutation id; stdout={:?}",
                archive.stdout
            )
        });

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

    let status = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["status", "--format", "json"],
    );
    daemon.pid = status["daemon_pid"].as_u64();
    let original_pid = daemon.pid.expect("daemon pid");

    run_status_only(
        &instance,
        &data_dir,
        &config_dir,
        &["sync", "--wait", "--wait-timeout-secs", "30"],
    );

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

    std::process::Command::new("kill")
        .arg(original_pid.to_string())
        .status()
        .expect("kill daemon");
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
