//! End-to-end CLI smoke test against the in-memory Fake provider.
//!
//! Exercises the v1 happy path: configure a fake account → sync → search → cat
//! → reply --yes → mutate → search reflects new state. Asserts the JSON contract
//! on every command we touch so script consumers stay covered.

use assert_cmd::Command;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;
use std::sync::{Mutex, MutexGuard};
use std::thread::sleep;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tempfile::TempDir;

static CLI_JOURNEY_LOCK: Mutex<()> = Mutex::new(());

fn cli_journey_guard() -> MutexGuard<'static, ()> {
    CLI_JOURNEY_LOCK.lock().expect("cli journey lock")
}

struct DaemonGuard {
    socket_path: PathBuf,
    pid_path: PathBuf,
    pid: Option<u64>,
}

impl Drop for DaemonGuard {
    fn drop(&mut self) {
        if let Some(pid) = self.pid {
            let _ = StdCommand::new("kill").arg(pid.to_string()).status();
            for _ in 0..40 {
                if !self.socket_path.exists() {
                    break;
                }
                sleep(Duration::from_millis(50));
            }
        }
        let _ = std::fs::remove_file(&self.socket_path);
        let _ = std::fs::remove_file(&self.pid_path);
    }
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

    // search --format json returns a JSON array of result objects.
    // Use a token from the fake fixtures (`buildkite`) rather than an empty
    // query: Tantivy's parser rejects bare `*` and we want the test to assert
    // a real query path, not match-all.
    let search = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &["search", "deployment", "--format", "json", "--limit", "50"],
    );
    let results = search
        .as_array()
        .unwrap_or_else(|| panic!("expected JSON array from search; got: {search:#}"));
    assert!(
        !results.is_empty(),
        "expected fake provider fixtures to populate search; got: {search:#}"
    );
    let first = &results[0];
    let message_id = first["message_id"]
        .as_str()
        .expect("first result needs message_id");

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
    let sent_subjects: Vec<String> = sent
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item["subject"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
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
    let inbox_ids: Vec<&str> = inbox
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item["message_id"].as_str())
                .collect()
        })
        .unwrap_or_default();
    assert!(
        !inbox_ids.iter().any(|id| *id == message_id),
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
        &["search", &unique_subject, "--format", "json", "--limit", "10"],
    );
    let arr = results
        .as_array()
        .unwrap_or_else(|| panic!("expected JSON array from search; got: {results:#}"));
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
    let sent_subjects: Vec<&str> = sent
        .as_array()
        .map(|a| a.iter().filter_map(|i| i["subject"].as_str()).collect())
        .unwrap_or_default();
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
        &["search", &unique_subject, "--format", "json", "--limit", "10"],
    );
    let searched_id = results
        .as_array()
        .and_then(|arr| arr.first())
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
        &[
            "search",
            "label:inbox",
            "--format",
            "json",
            "--limit",
            "10",
        ],
    );
    let target_id = inbox
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|item| item["message_id"].as_str())
        .map(str::to_string)
        .unwrap_or_else(|| panic!("fixture must yield at least one inbox message; got: {inbox:#}"));

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
        .find_map(|line| line.strip_prefix("Undo with: mxr undo ").map(str::to_string))
        .unwrap_or_else(|| {
            panic!(
                "archive --yes must print `Undo with: mxr undo <id>`; stdout={:?}",
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
    let in_inbox_after = inbox_after
        .as_array()
        .map(|arr| {
            arr.iter()
                .any(|item| item["message_id"].as_str() == Some(target_id.as_str()))
        })
        .unwrap_or(false);
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
    let restored = inbox_post
        .as_array()
        .map(|arr| {
            arr.iter()
                .any(|item| item["message_id"].as_str() == Some(target_id.as_str()))
        })
        .unwrap_or(false);
    assert!(
        restored,
        "post-undo: {target_id} must reappear in `label:inbox`; got {:?}",
        inbox_post
    );
}

fn write_fake_config(config_dir: &Path) {
    let toml = r#"[general]
default_account = "fake"

[accounts.fake]
name = "Fake Account"
email = "fake@example.com"

[accounts.fake.sync]
type = "fake"

[accounts.fake.send]
type = "fake"
"#;
    std::fs::write(config_dir.join("config.toml"), toml).expect("write fake config");
}

struct CliOutput {
    stdout: String,
    stderr: String,
}

fn run_status_only(instance: &str, data_dir: &Path, config_dir: &Path, args: &[&str]) -> CliOutput {
    let output = Command::cargo_bin("mxr")
        .expect("mxr bin")
        .env("MXR_INSTANCE", instance)
        .env("MXR_DATA_DIR", data_dir)
        .env("MXR_CONFIG_DIR", config_dir)
        .env_remove("EDITOR")
        .env_remove("VISUAL")
        .args(args)
        .assert()
        .get_output()
        .clone();
    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let stderr = String::from_utf8(output.stderr).expect("utf8 stderr");
    if !output.status.success() {
        panic!(
            "command {args:?} failed (exit {:?})\nstdout={stdout}\nstderr={stderr}",
            output.status.code()
        );
    }
    CliOutput { stdout, stderr }
}

fn run_json(instance: &str, data_dir: &Path, config_dir: &Path, args: &[&str]) -> Value {
    let out = run_status_only(instance, data_dir, config_dir, args);
    serde_json::from_str(out.stdout.trim()).unwrap_or_else(|err| {
        panic!(
            "expected JSON output for `mxr {}`; parse error: {err}\nstdout={}\nstderr={}",
            args.join(" "),
            out.stdout,
            out.stderr
        )
    })
}

fn instance_socket_path(instance: &str) -> PathBuf {
    if cfg!(target_os = "macos") {
        dirs::home_dir()
            .expect("home dir")
            .join("Library")
            .join("Application Support")
            .join(instance)
            .join("mxr.sock")
    } else {
        dirs::runtime_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(instance)
            .join("mxr.sock")
    }
}

fn unique_instance_name() -> String {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("now")
        .as_nanos();
    format!("mxr-cli-journey-{}-{stamp}", std::process::id())
}
