# Test Quality Audit

Generated: 2026-03-26T23:30:05Z

## Summary
- Files audited: 99
- Tests audited: 811
- Average score: 30.0/30
- Critical files (<12): 0

## Per-file scores

| File | Tests | Score | Grade | Action | Weak asserts | Snapshots |
|---|---:|---:|---|---|---:|---:|
| crates/compose/src/attachments.rs | 4 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/compose/src/editor.rs | 3 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/compose/src/email.rs | 3 | 30/30 | high_confidence | keep | 0 | 2 |
| crates/compose/src/frontmatter.rs | 6 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/compose/src/lib.rs | 8 | 30/30 | high_confidence | keep | 1 | 0 |
| crates/compose/src/parse.rs | 7 | 30/30 | high_confidence | keep | 0 | 3 |
| crates/compose/src/render.rs | 3 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/config/src/lib.rs | 11 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/config/src/snooze.rs | 9 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/core/src/id.rs | 1 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/core/src/lib.rs | 7 | 30/30 | high_confidence | keep | 1 | 0 |
| crates/daemon/src/cli/mod.rs | 10 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/daemon/src/commands/bug_report.rs | 4 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/daemon/src/commands/cat.rs | 3 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/daemon/src/commands/doctor.rs | 2 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/daemon/src/commands/events.rs | 3 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/daemon/src/commands/export.rs | 2 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/daemon/src/commands/history.rs | 2 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/daemon/src/commands/labels.rs | 2 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/daemon/src/commands/logs.rs | 1 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/daemon/src/commands/mutations/compose.rs | 4 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/daemon/src/commands/notify.rs | 2 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/daemon/src/commands/rules.rs | 3 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/daemon/src/commands/status.rs | 2 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/daemon/src/commands/subscriptions.rs | 2 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/daemon/src/handler/mod.rs | 60 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/daemon/src/loops.rs | 1 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/daemon/src/reindex.rs | 4 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/daemon/src/server.rs | 8 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/daemon/src/state.rs | 3 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/daemon/src/unsubscribe.rs | 2 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/daemon/tests/cli_help.rs | 1 | 30/30 | high_confidence | keep | 0 | 1 |
| crates/daemon/tests/daemon_lifecycle.rs | 1 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/export/src/json.rs | 10 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/export/src/lib.rs | 2 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/export/src/llm.rs | 13 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/export/src/markdown.rs | 10 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/export/src/mbox.rs | 12 | 30/30 | high_confidence | keep | 0 | 1 |
| crates/protocol/src/lib.rs | 7 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/provider-fake/src/lib.rs | 10 | 30/30 | high_confidence | keep | 1 | 0 |
| crates/provider-gmail/src/client.rs | 6 | 30/30 | high_confidence | keep | 1 | 0 |
| crates/provider-gmail/src/parse.rs | 22 | 30/30 | high_confidence | keep | 2 | 2 |
| crates/provider-gmail/src/provider.rs | 3 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/provider-gmail/src/send.rs | 4 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/provider-gmail/tests/live_smoke.rs | 3 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/provider-imap/src/config.rs | 1 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/provider-imap/src/error.rs | 2 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/provider-imap/src/folders.rs | 5 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/provider-imap/src/lib.rs | 24 | 30/30 | high_confidence | keep | 1 | 0 |
| crates/provider-imap/src/parse.rs | 19 | 30/30 | high_confidence | keep | 4 | 2 |
| crates/provider-imap/src/session.rs | 3 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/provider-imap/src/types.rs | 5 | 30/30 | high_confidence | keep | 1 | 0 |
| crates/provider-imap/tests/integration.rs | 8 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/provider-imap/tests/live_smoke.rs | 3 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/provider-smtp/src/config.rs | 1 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/provider-smtp/src/lib.rs | 3 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/provider-smtp/tests/live_smoke.rs | 1 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/reader/src/boilerplate.rs | 2 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/reader/src/html.rs | 11 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/reader/src/pipeline.rs | 6 | 30/30 | high_confidence | keep | 1 | 0 |
| crates/reader/src/quotes.rs | 3 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/reader/src/signatures.rs | 4 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/reader/src/tracking.rs | 3 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/reader/tests/integration.rs | 5 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/reader/tests/standards.rs | 2 | 30/30 | high_confidence | keep | 0 | 2 |
| crates/rules/src/action.rs | 2 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/rules/src/condition.rs | 25 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/rules/src/engine.rs | 10 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/rules/src/history.rs | 4 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/rules/src/shell_hook.rs | 7 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/search/src/lib.rs | 18 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/search/src/parser.rs | 25 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/search/src/query_builder.rs | 8 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/search/src/saved.rs | 3 | 30/30 | high_confidence | keep | 1 | 0 |
| crates/semantic/src/attachments.rs | 4 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/semantic/src/lib.rs | 4 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/store/src/diagnostics.rs | 1 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/store/src/lib.rs | 26 | 30/30 | high_confidence | keep | 3 | 0 |
| crates/sync/src/lib.rs | 25 | 30/30 | high_confidence | keep | 4 | 0 |
| crates/sync/src/threading.rs | 8 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/tui/src/app/draw.rs | 1 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/tui/src/app/mod.rs | 1 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/tui/src/keybindings.rs | 12 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/tui/src/lib.rs | 158 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/tui/src/ui/accounts_page.rs | 4 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/tui/src/ui/bulk_confirm_modal.rs | 1 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/tui/src/ui/command_palette.rs | 2 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/tui/src/ui/error_modal.rs | 1 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/tui/src/ui/help_modal.rs | 7 | 30/30 | high_confidence | keep | 0 | 2 |
| crates/tui/src/ui/hint_bar.rs | 1 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/tui/src/ui/label_picker.rs | 1 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/tui/src/ui/mail_list.rs | 5 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/tui/src/ui/message_view.rs | 3 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/tui/src/ui/search_query.rs | 1 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/tui/src/ui/send_confirm_modal.rs | 2 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/tui/src/ui/sidebar.rs | 2 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/tui/src/ui/url_modal.rs | 5 | 30/30 | high_confidence | keep | 0 | 0 |
| crates/tui/tests/snapshots.rs | 22 | 30/30 | high_confidence | keep | 0 | 21 |
| crates/web/src/lib.rs | 15 | 30/30 | high_confidence | keep | 0 | 0 |
