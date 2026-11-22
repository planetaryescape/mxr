//! `mxr archive --search … --all` end to end: the real binary, a fake-provider
//! daemon and its store. The job and the dry run are read back through the
//! CLI, and an undo of the job is refused.

#![expect(
    clippy::unwrap_used,
    clippy::panic,
    reason = "integration tests assert directly on fixtures and command output"
)]

use mxr_test_support::daemon::{daemon_lock, run_json, run_status_only, spawn_fake_daemon};
use serde_json::Value;
use std::path::Path;
use std::process::{Command, Output};

const MXR_BIN: &str = env!("CARGO_BIN_EXE_mxr");
const INBOX: &str = "label:inbox";

fn run_failing(instance: &str, data_dir: &Path, config_dir: &Path, args: &[&str]) -> Output {
    let output = Command::new(MXR_BIN)
        .env("MXR_INSTANCE", instance)
        .env("MXR_DATA_DIR", data_dir)
        .env("MXR_CONFIG_DIR", config_dir)
        .env_remove("EDITOR")
        .env_remove("VISUAL")
        .args(args)
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "`mxr {}` should have failed\nstdout={}\nstderr={}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn inbox_ids(instance: &str, data_dir: &Path, config_dir: &Path) -> Vec<String> {
    let found = run_json(
        instance,
        data_dir,
        config_dir,
        &["search", INBOX, "--limit", "500", "--format", "json"],
    );
    found
        .as_array()
        .or_else(|| found.get("results").and_then(Value::as_array))
        .unwrap_or_else(|| panic!("search results: {found:#}"))
        .iter()
        .map(|result| result["message_id"].as_str().unwrap().to_owned())
        .collect()
}

fn fake_daemon(
    temp: &tempfile::TempDir,
) -> (
    mxr_test_support::daemon::DaemonGuard,
    String,
    std::path::PathBuf,
    std::path::PathBuf,
) {
    let (daemon, instance, data_dir, config_dir) = spawn_fake_daemon(temp, "bulk-all-cli");
    run_status_only(&instance, &data_dir, &config_dir, &["sync", "--wait"]);
    (daemon, instance, data_dir, config_dir)
}

#[test]
fn the_dry_run_counts_the_whole_selection_and_never_lists_ids() {
    let _guard = daemon_lock();
    let temp = tempfile::tempdir().unwrap();
    let (_daemon, instance, data_dir, config_dir) = fake_daemon(&temp);
    let inbox = inbox_ids(&instance, &data_dir, &config_dir);
    assert!(inbox.len() > 3, "the fake mailbox has inbox mail");

    let preview = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "archive",
            "--search",
            INBOX,
            "--all",
            "--dry-run",
            "--format",
            "json",
        ],
    );
    assert_eq!(preview["action"], "archive");
    assert_eq!(preview["dry_run"], true);
    assert_eq!(preview["all"], true);
    assert_eq!(preview["selection"]["kind"], "search");
    assert_eq!(preview["selection"]["query"], INBOX);
    assert_eq!(
        preview["selected_messages"].as_u64().unwrap() as usize,
        inbox.len()
    );
    let threads = preview["selected_threads"].as_u64().unwrap();
    assert!(threads >= 1 && threads as usize <= inbox.len());
    assert_eq!(preview["capped"], false);
    assert!(preview["preview_token"]
        .as_str()
        .is_some_and(|t| !t.is_empty()));
    assert_eq!(preview["accounts"].as_array().unwrap().len(), 1);
    assert_eq!(
        preview["accounts"][0]["count"].as_u64().unwrap() as usize,
        inbox.len()
    );
    let sample = preview["messages"].as_array().unwrap();
    assert_eq!(sample.len(), inbox.len().min(20));
    assert_eq!(preview["sample_size"], 20);
    assert_eq!(sample[0]["message_id"].as_str().unwrap(), inbox[0]);
    for message in sample {
        assert!(inbox.contains(&message["message_id"].as_str().unwrap().to_owned()));
    }
    assert!(preview.get("message_ids").is_none());
    assert!(preview.get("requested").is_none());

    assert_eq!(inbox_ids(&instance, &data_dir, &config_dir), inbox);

    let table = run_status_only(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "archive",
            "--search",
            INBOX,
            "--all",
            "--dry-run",
            "--format",
            "table",
        ],
    );
    assert!(
        table
            .stdout
            .contains(&format!("Would archive {} message(s)", inbox.len())),
        "{}",
        table.stdout
    );
    assert!(table.stdout.contains("--preview-token"), "{}", table.stdout);

    let lines = run_status_only(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "archive",
            "--search",
            INBOX,
            "--all",
            "--dry-run",
            "--format",
            "jsonl",
        ],
    );
    let first: Value = serde_json::from_str(lines.stdout.lines().next().unwrap()).unwrap();
    assert_eq!(
        first["selected_messages"].as_u64().unwrap() as usize,
        inbox.len()
    );
    assert_eq!(first["capped"], false);
}

#[test]
fn a_run_is_one_job_with_one_id_that_cannot_be_undone() {
    let _guard = daemon_lock();
    let temp = tempfile::tempdir().unwrap();
    let (_daemon, instance, data_dir, config_dir) = fake_daemon(&temp);
    let before = inbox_ids(&instance, &data_dir, &config_dir);

    let run = run_status_only(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "archive", "--search", INBOX, "--all", "--yes", "--format", "json",
        ],
    );
    let output: Value = serde_json::from_str(run.stdout.trim()).unwrap();
    assert_eq!(output["all"], true);
    assert_eq!(output["dry_run"], false);
    assert_eq!(output["capped"], false);
    assert_eq!(
        output["selected_messages"].as_u64().unwrap() as usize,
        before.len()
    );
    let job = &output["job"];
    assert_eq!(job["status"], "succeeded", "{output:#}");
    assert_eq!(
        job["progress"]["succeeded"].as_u64().unwrap() as usize,
        before.len()
    );
    assert_eq!(
        job["progress"]["total"].as_u64().unwrap() as usize,
        before.len()
    );
    assert_eq!(job["undo_ids"], serde_json::json!([]), "{output:#}");
    let job_id = job["job_id"].as_str().unwrap().to_owned();
    assert_eq!(run.stdout.matches(&job_id).count(), 1, "{}", run.stdout);
    assert!(output.get("message_ids").is_none());
    assert!(run.stderr.contains("one job"), "{}", run.stderr);
    assert!(inbox_ids(&instance, &data_dir, &config_dir).is_empty());

    let jobs = run_status_only(
        &instance,
        &data_dir,
        &config_dir,
        &["jobs", &job_id, "--format", "table"],
    );
    assert!(jobs.stdout.contains(&job_id), "{}", jobs.stdout);
    assert!(!jobs.stdout.contains("mxr undo"), "{}", jobs.stdout);

    let undo = run_failing(&instance, &data_dir, &config_dir, &["undo", &job_id]);
    let stderr = String::from_utf8_lossy(&undo.stderr);
    assert!(stderr.contains("cannot be undone"), "{stderr}");
    assert!(stderr.contains("--all"), "{stderr}");
    assert!(!stderr.contains("not found"), "{stderr}");
    assert!(inbox_ids(&instance, &data_dir, &config_dir).is_empty());

    let dry = run_failing(
        &instance,
        &data_dir,
        &config_dir,
        &["undo", &job_id, "--dry-run"],
    );
    assert_eq!(dry.status.code(), undo.status.code());
    assert_eq!(String::from_utf8_lossy(&dry.stderr), stderr);
    assert!(!String::from_utf8_lossy(&dry.stdout).contains("Would undo"));
}

#[test]
fn a_preview_token_runs_exactly_the_preview_once() {
    let _guard = daemon_lock();
    let temp = tempfile::tempdir().unwrap();
    let (_daemon, instance, data_dir, config_dir) = fake_daemon(&temp);
    let before = inbox_ids(&instance, &data_dir, &config_dir);
    let preview = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "archive",
            "--search",
            INBOX,
            "--all",
            "--dry-run",
            "--format",
            "json",
        ],
    );
    let token = preview["preview_token"].as_str().unwrap().to_owned();

    let run = run_json(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "archive",
            "--search",
            INBOX,
            "--all",
            "--preview-token",
            &token,
            "--format",
            "json",
        ],
    );
    assert_eq!(run["job"]["status"], "succeeded", "{run:#}");
    assert_eq!(
        run["job"]["progress"]["succeeded"].as_u64().unwrap() as usize,
        before.len()
    );

    let again = run_failing(
        &instance,
        &data_dir,
        &config_dir,
        &[
            "archive",
            "--search",
            INBOX,
            "--all",
            "--preview-token",
            &token,
        ],
    );
    assert!(
        String::from_utf8_lossy(&again.stderr).contains("expired or was already used"),
        "{}",
        String::from_utf8_lossy(&again.stderr)
    );
}

#[test]
fn without_yes_and_without_a_terminal_it_asks_for_yes_and_changes_nothing() {
    let _guard = daemon_lock();
    let temp = tempfile::tempdir().unwrap();
    let (_daemon, instance, data_dir, config_dir) = fake_daemon(&temp);
    let before = inbox_ids(&instance, &data_dir, &config_dir);
    let output = run_failing(
        &instance,
        &data_dir,
        &config_dir,
        &["archive", "--search", INBOX, "--all"],
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Confirmation required"), "{stderr}");
    assert!(stderr.contains("cannot be undone"), "{stderr}");
    assert!(
        stderr.contains(&format!("{} message(s)", before.len())),
        "{stderr}"
    );
    assert_eq!(inbox_ids(&instance, &data_dir, &config_dir), before);
}
