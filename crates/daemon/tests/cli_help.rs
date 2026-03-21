use assert_cmd::Command;

fn assert_help_snapshot(name: &str, args: &[&str]) {
    let output = Command::cargo_bin("mxr")
        .unwrap()
        .args(args)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8(output).unwrap();
    insta::assert_snapshot!(name, stdout);
}

#[test]
fn root_help_snapshot() {
    assert_help_snapshot("cli_help_root", &["--help"]);
}

#[test]
fn search_help_snapshot() {
    assert_help_snapshot("cli_help_search", &["search", "--help"]);
}

#[test]
fn daemon_help_snapshot() {
    assert_help_snapshot("cli_help_daemon", &["daemon", "--help"]);
}

#[test]
fn count_help_snapshot() {
    assert_help_snapshot("cli_help_count", &["count", "--help"]);
}

#[test]
fn cat_help_snapshot() {
    assert_help_snapshot("cli_help_cat", &["cat", "--help"]);
}

#[test]
fn thread_help_snapshot() {
    assert_help_snapshot("cli_help_thread", &["thread", "--help"]);
}

#[test]
fn compose_help_snapshot() {
    assert_help_snapshot("cli_help_compose", &["compose", "--help"]);
}

#[test]
fn export_help_snapshot() {
    assert_help_snapshot("cli_help_export", &["export", "--help"]);
}

#[test]
fn headers_help_snapshot() {
    assert_help_snapshot("cli_help_headers", &["headers", "--help"]);
}

#[test]
fn sync_help_snapshot() {
    assert_help_snapshot("cli_help_sync", &["sync", "--help"]);
}

#[test]
fn saved_help_snapshot() {
    assert_help_snapshot("cli_help_saved", &["saved", "--help"]);
}

#[test]
fn semantic_help_snapshot() {
    assert_help_snapshot("cli_help_semantic", &["semantic", "--help"]);
}

#[test]
fn semantic_profile_help_snapshot() {
    assert_help_snapshot(
        "cli_help_semantic_profile",
        &["semantic", "profile", "--help"],
    );
}

#[test]
fn status_help_snapshot() {
    assert_help_snapshot("cli_help_status", &["status", "--help"]);
}

#[test]
fn events_help_snapshot() {
    assert_help_snapshot("cli_help_events", &["events", "--help"]);
}

#[test]
fn history_help_snapshot() {
    assert_help_snapshot("cli_help_history", &["history", "--help"]);
}

#[test]
fn notify_help_snapshot() {
    assert_help_snapshot("cli_help_notify", &["notify", "--help"]);
}

#[test]
fn logs_help_snapshot() {
    assert_help_snapshot("cli_help_logs", &["logs", "--help"]);
}

#[test]
fn bug_report_help_snapshot() {
    assert_help_snapshot("cli_help_bug_report", &["bug-report", "--help"]);
}

#[test]
fn accounts_help_snapshot() {
    assert_help_snapshot("cli_help_accounts", &["accounts", "--help"]);
}

#[test]
fn doctor_help_snapshot() {
    assert_help_snapshot("cli_help_doctor", &["doctor", "--help"]);
}

#[test]
fn labels_help_snapshot() {
    assert_help_snapshot("cli_help_labels", &["labels", "--help"]);
}

#[test]
fn rules_help_snapshot() {
    assert_help_snapshot("cli_help_rules", &["rules", "--help"]);
}

#[test]
fn reply_help_snapshot() {
    assert_help_snapshot("cli_help_reply", &["reply", "--help"]);
}

#[test]
fn reply_all_help_snapshot() {
    assert_help_snapshot("cli_help_reply_all", &["reply-all", "--help"]);
}

#[test]
fn forward_help_snapshot() {
    assert_help_snapshot("cli_help_forward", &["forward", "--help"]);
}

#[test]
fn drafts_help_snapshot() {
    assert_help_snapshot("cli_help_drafts", &["drafts", "--help"]);
}

#[test]
fn send_help_snapshot() {
    assert_help_snapshot("cli_help_send", &["send", "--help"]);
}

#[test]
fn unsubscribe_help_snapshot() {
    assert_help_snapshot("cli_help_unsubscribe", &["unsubscribe", "--help"]);
}

#[test]
fn attachments_help_snapshot() {
    assert_help_snapshot("cli_help_attachments", &["attachments", "--help"]);
}

#[test]
fn archive_help_snapshot() {
    assert_help_snapshot("cli_help_archive", &["archive", "--help"]);
}

#[test]
fn trash_help_snapshot() {
    assert_help_snapshot("cli_help_trash", &["trash", "--help"]);
}

#[test]
fn spam_help_snapshot() {
    assert_help_snapshot("cli_help_spam", &["spam", "--help"]);
}

#[test]
fn star_help_snapshot() {
    assert_help_snapshot("cli_help_star", &["star", "--help"]);
}

#[test]
fn unstar_help_snapshot() {
    assert_help_snapshot("cli_help_unstar", &["unstar", "--help"]);
}

#[test]
fn read_help_snapshot() {
    assert_help_snapshot("cli_help_read", &["read", "--help"]);
}

#[test]
fn unread_help_snapshot() {
    assert_help_snapshot("cli_help_unread", &["unread", "--help"]);
}

#[test]
fn label_help_snapshot() {
    assert_help_snapshot("cli_help_label", &["label", "--help"]);
}

#[test]
fn unlabel_help_snapshot() {
    assert_help_snapshot("cli_help_unlabel", &["unlabel", "--help"]);
}

#[test]
fn move_help_snapshot() {
    assert_help_snapshot("cli_help_move", &["move", "--help"]);
}

#[test]
fn snooze_help_snapshot() {
    assert_help_snapshot("cli_help_snooze", &["snooze", "--help"]);
}

#[test]
fn unsnooze_help_snapshot() {
    assert_help_snapshot("cli_help_unsnooze", &["unsnooze", "--help"]);
}

#[test]
fn snoozed_help_snapshot() {
    assert_help_snapshot("cli_help_snoozed", &["snoozed", "--help"]);
}

#[test]
fn open_help_snapshot() {
    assert_help_snapshot("cli_help_open", &["open", "--help"]);
}

#[test]
fn config_help_snapshot() {
    assert_help_snapshot("cli_help_config", &["config", "--help"]);
}

#[test]
fn version_help_snapshot() {
    assert_help_snapshot("cli_help_version", &["version", "--help"]);
}

#[test]
fn completions_help_snapshot() {
    assert_help_snapshot("cli_help_completions", &["completions", "--help"]);
}
