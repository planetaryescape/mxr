//! The mxr binary's integration tests, built as one test program.
//!
//! Each of these files used to be its own `[[test]]` target, and every one
//! linked the whole mxr library separately: 23 links of 4 to 9 s each on
//! every `cargo test -p mxr --tests`. Run one file's tests with a module
//! filter, e.g. `scripts/cargo-test -p mxr --test integration cli_help::`.

mod accounts_repair_cli;
mod activity_invariants;
mod archive_intelligence_cli;
mod bounded_logs;
mod bug_report_cli;
mod cli_auth_exit_cli;
mod cli_exit_codes_cli;
mod cli_help;
mod cli_journey;
mod compose_check_cli;
mod compose_html_cli;
mod daemon_lifecycle;
mod demo_cli;
mod dial_stdio_cli;
mod forward_cli;
mod live_gmail_e2e;
mod modes_cli;
mod reset_cli;
mod scheduled_sends_cli;
mod selected_daemon_diagnostics;
mod stdio_server_cli;
mod timing_cadence_cli;
mod unsubscribe_cli;
