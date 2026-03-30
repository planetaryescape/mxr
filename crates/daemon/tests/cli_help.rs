use assert_cmd::Command;

fn assert_help_snapshot(name: &str, args: &[&str]) {
    let output = Command::cargo_bin("mxr")
        .expect("mxr binary")
        .args(args)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8(output).expect("utf8 help output");
    insta::assert_snapshot!(name, stdout);
}

#[test]
fn cli_help_snapshots_cover_all_commands() {
    let cases: &[(&str, &[&str])] = &[
        ("cli_help_root", &["--help"]),
        ("cli_help_search", &["search", "--help"]),
        ("cli_help_daemon", &["daemon", "--help"]),
        ("cli_help_count", &["count", "--help"]),
        ("cli_help_cat", &["cat", "--help"]),
        ("cli_help_thread", &["thread", "--help"]),
        ("cli_help_compose", &["compose", "--help"]),
        ("cli_help_export", &["export", "--help"]),
        ("cli_help_headers", &["headers", "--help"]),
        ("cli_help_sync", &["sync", "--help"]),
        ("cli_help_saved", &["saved", "--help"]),
        ("cli_help_semantic", &["semantic", "--help"]),
        (
            "cli_help_semantic_profile",
            &["semantic", "profile", "--help"],
        ),
        ("cli_help_status", &["status", "--help"]),
        ("cli_help_web", &["web", "--help"]),
        ("cli_help_events", &["events", "--help"]),
        ("cli_help_history", &["history", "--help"]),
        ("cli_help_notify", &["notify", "--help"]),
        ("cli_help_logs", &["logs", "--help"]),
        ("cli_help_reset", &["reset", "--help"]),
        ("cli_help_burn", &["burn", "--help"]),
        ("cli_help_bug_report", &["bug-report", "--help"]),
        ("cli_help_accounts", &["accounts", "--help"]),
        ("cli_help_doctor", &["doctor", "--help"]),
        ("cli_help_labels", &["labels", "--help"]),
        ("cli_help_rules", &["rules", "--help"]),
        ("cli_help_reply", &["reply", "--help"]),
        ("cli_help_reply_all", &["reply-all", "--help"]),
        ("cli_help_forward", &["forward", "--help"]),
        ("cli_help_drafts", &["drafts", "--help"]),
        ("cli_help_send", &["send", "--help"]),
        ("cli_help_unsubscribe", &["unsubscribe", "--help"]),
        ("cli_help_attachments", &["attachments", "--help"]),
        ("cli_help_archive", &["archive", "--help"]),
        ("cli_help_trash", &["trash", "--help"]),
        ("cli_help_spam", &["spam", "--help"]),
        ("cli_help_star", &["star", "--help"]),
        ("cli_help_unstar", &["unstar", "--help"]),
        ("cli_help_read", &["read", "--help"]),
        ("cli_help_unread", &["unread", "--help"]),
        ("cli_help_label", &["label", "--help"]),
        ("cli_help_unlabel", &["unlabel", "--help"]),
        ("cli_help_move", &["move", "--help"]),
        ("cli_help_snooze", &["snooze", "--help"]),
        ("cli_help_unsnooze", &["unsnooze", "--help"]),
        ("cli_help_snoozed", &["snoozed", "--help"]),
        ("cli_help_open", &["open", "--help"]),
        ("cli_help_config", &["config", "--help"]),
        ("cli_help_version", &["version", "--help"]),
        ("cli_help_completions", &["completions", "--help"]),
    ];

    assert_eq!(cases.len(), 50);

    for (name, args) in cases {
        assert_help_snapshot(name, args);
    }
}
