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
    let stdout = normalize_help_output(&stdout);
    insta::assert_snapshot!(name, stdout);
}

fn normalize_help_output(stdout: &str) -> String {
    let mut normalized = stdout
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");

    if stdout.ends_with('\n') {
        normalized.push('\n');
    }

    normalized
}

#[test]
fn cli_help_snapshots_cover_all_commands() {
    let cases: &[(&str, &[&str])] = &[
        ("cli_help_root", &["--help"]),
        ("cli_help_search", &["search", "--help"]),
        ("cli_help_triage", &["triage", "--help"]),
        ("cli_help_daemon", &["daemon", "--help"]),
        ("cli_help_restart", &["restart", "--help"]),
        ("cli_help_count", &["count", "--help"]),
        ("cli_help_cat", &["cat", "--help"]),
        ("cli_help_thread", &["thread", "--help"]),
        ("cli_help_compose", &["compose", "--help"]),
        ("cli_help_export", &["export", "--help"]),
        ("cli_help_headers", &["headers", "--help"]),
        ("cli_help_sync", &["sync", "--help"]),
        ("cli_help_saved", &["saved", "--help"]),
        ("cli_help_saved_list", &["saved", "list", "--help"]),
        ("cli_help_saved_add", &["saved", "add", "--help"]),
        ("cli_help_saved_delete", &["saved", "delete", "--help"]),
        ("cli_help_saved_run", &["saved", "run", "--help"]),
        ("cli_help_semantic", &["semantic", "--help"]),
        (
            "cli_help_semantic_status",
            &["semantic", "status", "--help"],
        ),
        (
            "cli_help_semantic_enable",
            &["semantic", "enable", "--help"],
        ),
        (
            "cli_help_semantic_disable",
            &["semantic", "disable", "--help"],
        ),
        (
            "cli_help_semantic_reindex",
            &["semantic", "reindex", "--help"],
        ),
        (
            "cli_help_semantic_profile",
            &["semantic", "profile", "--help"],
        ),
        (
            "cli_help_semantic_profile_list",
            &["semantic", "profile", "list", "--help"],
        ),
        (
            "cli_help_semantic_profile_install",
            &["semantic", "profile", "install", "--help"],
        ),
        (
            "cli_help_semantic_profile_use",
            &["semantic", "profile", "use", "--help"],
        ),
        ("cli_help_status", &["status", "--help"]),
        ("cli_help_web", &["web", "--help"]),
        ("cli_help_events", &["events", "--help"]),
        ("cli_help_history", &["history", "--help"]),
        ("cli_help_notify", &["notify", "--help"]),
        ("cli_help_chimes", &["chimes", "--help"]),
        ("cli_help_chimes_status", &["chimes", "status", "--help"]),
        ("cli_help_chimes_enable", &["chimes", "enable", "--help"]),
        ("cli_help_chimes_disable", &["chimes", "disable", "--help"]),
        ("cli_help_chimes_set", &["chimes", "set", "--help"]),
        ("cli_help_chimes_test", &["chimes", "test", "--help"]),
        ("cli_help_activity", &["activity", "--help"]),
        ("cli_help_activity_list", &["activity", "list", "--help"]),
        ("cli_help_activity_stats", &["activity", "stats", "--help"]),
        (
            "cli_help_activity_export",
            &["activity", "export", "--help"],
        ),
        ("cli_help_activity_prune", &["activity", "prune", "--help"]),
        (
            "cli_help_activity_redact",
            &["activity", "redact", "--help"],
        ),
        ("cli_help_activity_clear", &["activity", "clear", "--help"]),
        ("cli_help_activity_pause", &["activity", "pause", "--help"]),
        (
            "cli_help_activity_resume",
            &["activity", "resume", "--help"],
        ),
        ("cli_help_activity_saved", &["activity", "saved", "--help"]),
        (
            "cli_help_activity_recall",
            &["activity", "recall", "--help"],
        ),
        (
            "cli_help_activity_replay",
            &["activity", "replay", "--help"],
        ),
        ("cli_help_activity_tail", &["activity", "tail", "--help"]),
        ("cli_help_logs", &["logs", "--help"]),
        ("cli_help_reset", &["reset", "--help"]),
        ("cli_help_burn", &["burn", "--help"]),
        ("cli_help_bug_report", &["bug-report", "--help"]),
        ("cli_help_accounts", &["accounts", "--help"]),
        ("cli_help_accounts_add", &["accounts", "add", "--help"]),
        ("cli_help_accounts_show", &["accounts", "show", "--help"]),
        ("cli_help_accounts_test", &["accounts", "test", "--help"]),
        (
            "cli_help_accounts_reauth",
            &["accounts", "reauth", "--help"],
        ),
        (
            "cli_help_accounts_repair",
            &["accounts", "repair", "--help"],
        ),
        (
            "cli_help_accounts_disable",
            &["accounts", "disable", "--help"],
        ),
        (
            "cli_help_accounts_remove",
            &["accounts", "remove", "--help"],
        ),
        (
            "cli_help_accounts_addresses",
            &["accounts", "addresses", "--help"],
        ),
        (
            "cli_help_accounts_addresses_list",
            &["accounts", "addresses", "list", "--help"],
        ),
        (
            "cli_help_accounts_addresses_add",
            &["accounts", "addresses", "add", "--help"],
        ),
        (
            "cli_help_accounts_addresses_remove",
            &["accounts", "addresses", "remove", "--help"],
        ),
        (
            "cli_help_accounts_addresses_set-primary",
            &["accounts", "addresses", "set-primary", "--help"],
        ),
        ("cli_help_doctor", &["doctor", "--help"]),
        ("cli_help_labels", &["labels", "--help"]),
        ("cli_help_labels_create", &["labels", "create", "--help"]),
        ("cli_help_labels_delete", &["labels", "delete", "--help"]),
        ("cli_help_labels_rename", &["labels", "rename", "--help"]),
        ("cli_help_rules", &["rules", "--help"]),
        ("cli_help_rules_list", &["rules", "list", "--help"]),
        ("cli_help_rules_show", &["rules", "show", "--help"]),
        ("cli_help_rules_add", &["rules", "add", "--help"]),
        ("cli_help_rules_edit", &["rules", "edit", "--help"]),
        ("cli_help_rules_validate", &["rules", "validate", "--help"]),
        ("cli_help_rules_enable", &["rules", "enable", "--help"]),
        ("cli_help_rules_disable", &["rules", "disable", "--help"]),
        ("cli_help_rules_delete", &["rules", "delete", "--help"]),
        ("cli_help_rules_dry-run", &["rules", "dry-run", "--help"]),
        ("cli_help_rules_history", &["rules", "history", "--help"]),
        ("cli_help_reply", &["reply", "--help"]),
        ("cli_help_reply_all", &["reply-all", "--help"]),
        ("cli_help_forward", &["forward", "--help"]),
        ("cli_help_drafts", &["drafts", "--help"]),
        ("cli_help_drafts_list", &["drafts", "list", "--help"]),
        ("cli_help_drafts_recover", &["drafts", "recover", "--help"]),
        ("cli_help_drafts_resume", &["drafts", "resume", "--help"]),
        ("cli_help_drafts_discard", &["drafts", "discard", "--help"]),
        ("cli_help_drafts_edit", &["drafts", "edit", "--help"]),
        ("cli_help_send", &["send", "--help"]),
        ("cli_help_unsubscribe", &["unsubscribe", "--help"]),
        ("cli_help_attachments", &["attachments", "--help"]),
        (
            "cli_help_attachments_list",
            &["attachments", "list", "--help"],
        ),
        (
            "cli_help_attachments_download",
            &["attachments", "download", "--help"],
        ),
        (
            "cli_help_attachments_open",
            &["attachments", "open", "--help"],
        ),
        ("cli_help_invite", &["invite", "--help"]),
        ("cli_help_invite_show", &["invite", "show", "--help"]),
        ("cli_help_invite_reply", &["invite", "reply", "--help"]),
        ("cli_help_invites", &["invites", "--help"]),
        ("cli_help_invites_list", &["invites", "list", "--help"]),
        (
            "cli_help_invites_backfill",
            &["invites", "backfill", "--help"],
        ),
        ("cli_help_archive", &["archive", "--help"]),
        ("cli_help_read_archive", &["read-archive", "--help"]),
        ("cli_help_route", &["route", "--help"]),
        ("cli_help_undo", &["undo", "--help"]),
        ("cli_help_jobs", &["jobs", "--help"]),
        ("cli_help_subscriptions", &["subscriptions", "--help"]),
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
        ("cli_help_replies", &["replies", "--help"]),
        ("cli_help_replies_list", &["replies", "list", "--help"]),
        ("cli_help_replies_add", &["replies", "add", "--help"]),
        ("cli_help_replies_remove", &["replies", "remove", "--help"]),
        ("cli_help_remind", &["remind", "--help"]),
        ("cli_help_unsend", &["unsend", "--help"]),
        ("cli_help_snippets", &["snippets", "--help"]),
        ("cli_help_snippets_list", &["snippets", "list", "--help"]),
        ("cli_help_snippets_set", &["snippets", "set", "--help"]),
        (
            "cli_help_snippets_remove",
            &["snippets", "remove", "--help"],
        ),
        ("cli_help_deliveries", &["deliveries", "--help"]),
        (
            "cli_help_deliveries_list",
            &["deliveries", "list", "--help"],
        ),
        ("cli_help_deliveries_get", &["deliveries", "get", "--help"]),
        (
            "cli_help_deliveries_resolve",
            &["deliveries", "resolve", "--help"],
        ),
        (
            "cli_help_deliveries_dismiss",
            &["deliveries", "dismiss", "--help"],
        ),
        (
            "cli_help_deliveries_scan",
            &["deliveries", "scan", "--help"],
        ),
        ("cli_help_sender", &["sender", "--help"]),
        ("cli_help_senders", &["senders", "--help"]),
        ("cli_help_screener", &["screener", "--help"]),
        ("cli_help_screener_queue", &["screener", "queue", "--help"]),
        ("cli_help_screener_list", &["screener", "list", "--help"]),
        ("cli_help_screener_allow", &["screener", "allow", "--help"]),
        ("cli_help_screener_deny", &["screener", "deny", "--help"]),
        ("cli_help_screener_feed", &["screener", "feed", "--help"]),
        (
            "cli_help_screener_paper-trail",
            &["screener", "paper-trail", "--help"],
        ),
        ("cli_help_screener_clear", &["screener", "clear", "--help"]),
        ("cli_help_setup", &["setup", "--help"]),
        ("cli_help_summarize", &["summarize", "--help"]),
        ("cli_help_draft_assist", &["draft-assist", "--help"]),
        ("cli_help_llm", &["llm", "--help"]),
        ("cli_help_llm_status", &["llm", "status", "--help"]),
        ("cli_help_open", &["open", "--help"]),
        ("cli_help_config", &["config", "--help"]),
        ("cli_help_config_show", &["config", "show", "--help"]),
        ("cli_help_config_path", &["config", "path", "--help"]),
        ("cli_help_config_edit", &["config", "edit", "--help"]),
        ("cli_help_config_get", &["config", "get", "--help"]),
        ("cli_help_config_set", &["config", "set", "--help"]),
        ("cli_help_version", &["version", "--help"]),
        ("cli_help_completions", &["completions", "--help"]),
        // Analytics commands (Slices 5, 11, 12+13, 14).
        ("cli_help_storage", &["storage", "--help"]),
        ("cli_help_stale", &["stale", "--help"]),
        ("cli_help_contacts", &["contacts", "--help"]),
        (
            "cli_help_contacts_asymmetry",
            &["contacts", "asymmetry", "--help"],
        ),
        ("cli_help_contacts_decay", &["contacts", "decay", "--help"]),
        (
            "cli_help_contacts_refresh",
            &["contacts", "refresh", "--help"],
        ),
        ("cli_help_response_time", &["response-time", "--help"]),
        ("cli_help_wrapped", &["wrapped", "--help"]),
        // AI-email roadmap commands.
        ("cli_help_owed", &["owed", "--help"]),
        ("cli_help_ask", &["ask", "--help"]),
        ("cli_help_decisions", &["decisions", "--help"]),
        (
            "cli_help_decisions_rebuild",
            &["decisions", "rebuild", "--help"],
        ),
        ("cli_help_decisions_show", &["decisions", "show", "--help"]),
        ("cli_help_send_time", &["send-time", "--help"]),
        ("cli_help_whois", &["whois", "--help"]),
        (
            "cli_help_suggest_recipients",
            &["suggest-recipients", "--help"],
        ),
        ("cli_help_expert", &["expert", "--help"]),
        ("cli_help_cadence", &["cadence", "--help"]),
        ("cli_help_cadence_watch", &["cadence", "watch", "--help"]),
        (
            "cli_help_cadence_unwatch",
            &["cadence", "unwatch", "--help"],
        ),
        ("cli_help_cadence_list", &["cadence", "list", "--help"]),
        ("cli_help_cadence_drift", &["cadence", "drift", "--help"]),
        ("cli_help_briefing", &["briefing", "--help"]),
        (
            "cli_help_briefing_thread",
            &["briefing", "thread", "--help"],
        ),
        (
            "cli_help_briefing_recipient",
            &["briefing", "recipient", "--help"],
        ),
    ];

    assert_eq!(cases.len(), 187);

    for (name, args) in cases {
        assert_help_snapshot(name, args);
    }
}
