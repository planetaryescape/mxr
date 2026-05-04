#![cfg_attr(
    test,
    allow(clippy::bool_assert_comparison, clippy::panic, clippy::unwrap_used)
)]

mod mutation_args;
mod search_args;

pub use mutation_args::*;
pub use search_args::*;

use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "mxr", about = "Terminal email client", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    /// Start the daemon explicitly
    Daemon {
        /// Run in foreground (for debugging / systemd)
        #[arg(long)]
        foreground: bool,
        /// Hidden instance marker used by daemon autostart to identify the child process.
        #[arg(long, hide = true)]
        instance: Option<String>,
    },
    /// Restart the daemon with the current binary
    Restart,
    /// Search messages
    Search {
        query: Option<String>,
        #[arg(long)]
        format: Option<OutputFormat>,
        #[arg(long, default_value = "50")]
        limit: Option<u32>,
        #[arg(long, value_enum)]
        mode: Option<SearchModeArg>,
        #[arg(long, value_enum)]
        sort: Option<SearchSortArg>,
        #[arg(long)]
        explain: bool,
    },
    /// Count matching messages
    Count {
        query: String,
        #[arg(long, value_enum)]
        mode: Option<SearchModeArg>,
        #[arg(long, value_enum)]
        format: Option<OutputFormat>,
    },
    /// Display a message
    Cat {
        message_id: String,
        #[arg(long, value_enum)]
        view: Option<BodyViewArg>,
        #[arg(
            long,
            conflicts_with = "view",
            conflicts_with = "raw",
            conflicts_with = "html"
        )]
        assets: bool,
        #[arg(long, conflicts_with = "view")]
        #[arg(long)]
        raw: bool,
        #[arg(long, conflicts_with = "view")]
        #[arg(long)]
        html: bool,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Display a thread
    Thread {
        thread_id: String,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Export a thread or matching search results
    Export {
        thread_id: Option<String>,
        #[arg(long)]
        search: Option<String>,
        #[arg(long, default_value = "markdown")]
        format: String,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Show message headers
    Headers {
        message_id: String,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Manage saved searches
    Saved {
        #[command(subcommand)]
        action: Option<SavedAction>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Manage semantic search profiles and indexing
    Semantic {
        #[command(subcommand)]
        action: Option<SemanticAction>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// List senders with unsubscribe support
    Subscriptions {
        #[arg(long, default_value = "200")]
        limit: u32,
        /// Rank by newsletter ROI: lowest open-rate first, ties broken by
        /// archived-unread descending. Highlights the lists most worth dropping.
        #[arg(long)]
        rank: bool,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Roll up disk consumption by sender, mimetype, or label.
    Storage {
        /// Group by which dimension. One of: sender, mimetype, label.
        #[arg(long, value_enum, default_value_t = StorageGroupByArg::Sender)]
        by: StorageGroupByArg,
        /// Maximum buckets to return.
        #[arg(long, default_value = "50")]
        limit: u32,
        /// Restrict to a single account by id.
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Surface relationship analytics from the materialized contacts table.
    Contacts {
        #[command(subcommand)]
        action: ContactsAction,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Reply-latency percentiles (clock + business-hours) per direction.
    ResponseTime {
        /// Measure their reply time to my outbound (`they_replied`). Default
        /// is mine: how long I take to reply to inbound messages.
        #[arg(long)]
        theirs: bool,
        /// Restrict to a single counterparty by email.
        #[arg(long)]
        counterparty: Option<String>,
        /// Restrict to reply pairs from the last N days.
        #[arg(long)]
        since_days: Option<u32>,
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// List stale threads waiting for a reply (mine = my turn, theirs = theirs).
    Stale {
        /// Latest message in thread is inbound (I owe a reply). Default if neither flag set.
        #[arg(long, conflicts_with = "theirs")]
        mine: bool,
        /// Latest message in thread is outbound (they owe a reply).
        #[arg(long, conflicts_with = "mine")]
        theirs: bool,
        /// Threshold in days; threads with more recent activity are excluded.
        #[arg(long, default_value = "14")]
        older_than_days: u32,
        #[arg(long, default_value = "100")]
        limit: u32,
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Trigger or query sync
    Sync {
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        status: bool,
        /// Wait for the triggered sync to finish before returning.
        /// Useful in scripts and CLI smoke tests.
        #[arg(long)]
        wait: bool,
        /// Maximum seconds to wait when --wait is set. Default 60.
        #[arg(long, default_value_t = 60)]
        wait_timeout_secs: u64,
        /// Output format. Honored by `--status`; ignored by trigger mode today.
        #[arg(long, value_enum)]
        format: Option<OutputFormat>,
    },
    /// Show daemon status
    Status {
        #[arg(long)]
        format: Option<OutputFormat>,
        #[arg(long)]
        watch: bool,
    },
    /// Start a local HTTP/WebSocket bridge over daemon IPC
    Web {
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value = "0")]
        port: u16,
        #[arg(long)]
        print_url: bool,
    },
    /// Watch daemon events
    Events {
        #[arg(long = "type")]
        event_type: Option<String>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Show persisted event history
    History {
        #[arg(long)]
        category: Option<String>,
        #[arg(long)]
        level: Option<String>,
        #[arg(long, default_value = "50")]
        limit: u32,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Unread summary for status bars
    Notify {
        #[arg(long)]
        format: Option<OutputFormat>,
        #[arg(long)]
        watch: bool,
    },
    /// View daemon logs
    Logs {
        #[arg(long)]
        no_follow: bool,
        #[arg(long)]
        level: Option<String>,
        #[arg(long)]
        since: Option<String>,
        #[arg(long)]
        purge: bool,
        /// Output format. `json`/`jsonl` emit one JSON object per line with
        /// `{ timestamp, level, message }` fields parsed from the log line.
        #[arg(long, value_enum)]
        format: Option<OutputFormat>,
    },
    /// Destroy local mxr runtime state after stopping the daemon. Preserves config.toml and credentials by default. Use --including-config to also delete config.toml. Destructive; use --dry-run to preview.
    Reset {
        /// Required explicit scope marker for destructive execution
        #[arg(long, required = true)]
        hard: bool,
        /// Show the exact reset plan without deleting anything
        #[arg(long)]
        dry_run: bool,
        /// Also delete config.toml. Credentials/keychain remain preserved.
        #[arg(long)]
        including_config: bool,
        /// Required for non-interactive destructive execution only
        #[arg(long = "yes-i-understand-this-destroys-local-state")]
        yes_i_understand_this_destroys_local_state: bool,
    },
    /// Destroy local mxr runtime state after stopping the daemon. Alias for `mxr reset --hard`. Preserves config.toml and credentials by default. Use --including-config to also delete config.toml. Destructive; use --dry-run to preview.
    Burn {
        /// Show the exact reset plan without deleting anything
        #[arg(long)]
        dry_run: bool,
        /// Also delete config.toml. Credentials/keychain remain preserved.
        #[arg(long)]
        including_config: bool,
        /// Required for non-interactive destructive execution only
        #[arg(long = "yes-i-understand-this-destroys-local-state")]
        yes_i_understand_this_destroys_local_state: bool,
    },
    /// Generate a sanitized diagnostic bundle
    BugReport {
        #[arg(long)]
        edit: bool,
        #[arg(long)]
        stdout: bool,
        #[arg(long)]
        clipboard: bool,
        #[arg(long)]
        github: bool,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long)]
        verbose: bool,
        #[arg(long)]
        full_logs: bool,
        #[arg(long)]
        no_sanitize: bool,
        #[arg(long)]
        since: Option<String>,
    },
    /// Manage accounts
    Accounts {
        #[command(subcommand)]
        action: Option<AccountsAction>,
        /// Output format for `accounts` (the no-subcommand listing).
        #[arg(long, value_enum, global = true)]
        format: Option<OutputFormat>,
    },
    /// Run diagnostics
    Doctor {
        #[arg(long)]
        reindex: bool,
        #[arg(long)]
        reindex_semantic: bool,
        #[arg(long)]
        check: bool,
        #[arg(long)]
        semantic_status: bool,
        #[arg(long)]
        verbose: bool,
        #[arg(long)]
        index_stats: bool,
        #[arg(long)]
        store_stats: bool,
        /// Reclassify Unknown directions, backfill list_ids, resolve reply
        /// pair pending, refresh contacts, fill business-hours latency. Idempotent.
        #[arg(long)]
        rebuild_analytics: bool,
        /// Force a full refresh of the materialized contacts table.
        #[arg(long)]
        refresh_contacts: bool,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Manage labels
    Labels {
        #[command(subcommand)]
        action: Option<LabelsAction>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Manage rules
    Rules {
        #[command(subcommand)]
        action: Option<RulesAction>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },

    // --- Phase 2: Compose ---
    /// Compose a new email
    Compose {
        /// Recipient(s), comma-separated
        #[arg(long)]
        to: Option<String>,
        /// CC recipient(s)
        #[arg(long)]
        cc: Option<String>,
        /// BCC recipient(s)
        #[arg(long)]
        bcc: Option<String>,
        /// Subject line
        #[arg(long)]
        subject: Option<String>,
        /// Message body as string
        #[arg(long, conflicts_with = "body_stdin")]
        body: Option<String>,
        /// Read message body from stdin
        #[arg(long, conflicts_with = "body")]
        body_stdin: bool,
        /// File path to attach (repeatable)
        #[arg(long, action = clap::ArgAction::Append)]
        attach: Vec<PathBuf>,
        /// Account name to send from
        #[arg(long)]
        from: Option<String>,
        /// Skip confirmation prompt
        #[arg(long)]
        yes: bool,
        /// Show what would be sent without sending
        #[arg(long)]
        dry_run: bool,
    },
    /// Reply to a message
    Reply {
        /// Message ID to reply to
        message_id: String,
        /// Inline reply body (skip $EDITOR)
        #[arg(long)]
        body: Option<String>,
        /// Read reply body from stdin
        #[arg(long)]
        body_stdin: bool,
        /// Skip confirmation
        #[arg(long)]
        yes: bool,
        /// Show what would be sent
        #[arg(long)]
        dry_run: bool,
    },
    /// Reply to all recipients
    ReplyAll {
        /// Message ID to reply to
        message_id: String,
        /// Inline reply body
        #[arg(long)]
        body: Option<String>,
        /// Read body from stdin
        #[arg(long)]
        body_stdin: bool,
        /// Skip confirmation
        #[arg(long)]
        yes: bool,
        /// Show what would be sent
        #[arg(long)]
        dry_run: bool,
    },
    /// Forward a message
    Forward {
        /// Message ID to forward
        message_id: String,
        /// Forward to recipient(s)
        #[arg(long)]
        to: Option<String>,
        /// Inline body
        #[arg(long)]
        body: Option<String>,
        /// Read body from stdin
        #[arg(long)]
        body_stdin: bool,
        /// Skip confirmation
        #[arg(long)]
        yes: bool,
        /// Show what would be sent
        #[arg(long)]
        dry_run: bool,
    },
    /// List drafts
    Drafts {
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Send a draft by ID
    Send {
        /// Draft ID to send
        draft_id: String,
    },

    // --- Phase 2: Mutations ---
    /// Archive a message (remove from inbox)
    Archive {
        /// Message ID
        message_id: Option<String>,
        /// Operate on messages matching search query
        #[arg(long)]
        search: Option<String>,
        /// Skip confirmation
        #[arg(long)]
        yes: bool,
        /// Show what would happen
        #[arg(long)]
        dry_run: bool,
    },
    /// Mark message as read and archive it
    #[command(name = "read-archive")]
    ReadArchive {
        message_id: Option<String>,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
    },
    /// Move message to trash
    Trash {
        message_id: Option<String>,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
    },
    /// Report message as spam
    Spam {
        message_id: Option<String>,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
    },
    /// Star a message
    Star {
        message_id: Option<String>,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
    },
    /// Unstar a message
    Unstar {
        message_id: Option<String>,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
    },
    /// Mark message as read
    #[command(name = "read")]
    MarkRead {
        message_id: Option<String>,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
    },
    /// Mark message as unread
    Unread {
        message_id: Option<String>,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
    },
    /// Apply a label to a message
    Label {
        /// Label name
        name: String,
        /// Message ID
        message_id: Option<String>,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
    },
    /// Remove a label from a message
    Unlabel {
        /// Label name
        name: String,
        /// Message ID
        message_id: Option<String>,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
    },
    /// Move message to a label/folder
    #[command(name = "move")]
    MoveMsg {
        /// Target label
        label: String,
        /// Message ID
        message_id: Option<String>,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
    },

    // --- Phase 2: Snooze ---
    /// Snooze a message until a specified time
    Snooze {
        message_id: Option<String>,
        /// When to resurface: tomorrow|monday|weekend|tonight|ISO8601
        #[arg(long)]
        until: String,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
    },
    /// Unsnooze a message
    Unsnooze {
        message_id: Option<String>,
        /// Unsnooze all
        #[arg(long)]
        all: bool,
    },
    /// List snoozed messages
    Snoozed {
        #[arg(long)]
        format: Option<OutputFormat>,
    },

    // --- Phase 2: Unsubscribe ---
    /// Unsubscribe from a mailing list
    Unsubscribe {
        message_id: Option<String>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        dry_run: bool,
    },

    /// Open message in browser
    Open { message_id: String },

    /// Manage message attachments
    Attachments {
        #[command(subcommand)]
        action: AttachmentAction,
    },
    /// Configuration management
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },
    /// Show version
    Version,
    /// Generate shell completions
    Completions { shell: String },
}

#[derive(Subcommand)]
pub enum SavedAction {
    /// List saved searches
    List,
    /// Add a saved search
    Add {
        name: String,
        query: String,
        #[arg(long, value_enum)]
        mode: Option<SearchModeArg>,
    },
    /// Delete a saved search
    Delete { name: String },
    /// Run a saved search
    Run { name: String },
}

#[derive(Debug, Clone, PartialEq, Eq, ValueEnum)]
pub enum BodyViewArg {
    Reader,
    Raw,
    Html,
    Headers,
}

#[derive(Debug, Clone, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
    Jsonl,
    Csv,
    Ids,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[clap(rename_all = "lowercase")]
pub enum StorageGroupByArg {
    Sender,
    Mimetype,
    Label,
}

impl From<StorageGroupByArg> for mxr_core::types::StorageGroupBy {
    fn from(value: StorageGroupByArg) -> Self {
        match value {
            StorageGroupByArg::Sender => Self::Sender,
            StorageGroupByArg::Mimetype => Self::Mimetype,
            StorageGroupByArg::Label => Self::Label,
        }
    }
}

pub fn unsupported_command_guidance(args: &[String]) -> Option<String> {
    let command = args.get(1)?.as_str();
    match command {
        "start" => Some(
            "Unknown subcommand `start`. Use `mxr daemon` to start the daemon, `mxr daemon --foreground` to debug it, or `mxr status` to inspect it.".to_string(),
        ),
        "stop" => Some(format!(
            "Unknown subcommand `{command}`. Use `mxr status`, `mxr logs --level error`, or run `mxr daemon --foreground` in a terminal for diagnosis."
        )),
        "daemon" => match args.get(2).map(String::as_str) {
            Some("start") => Some(
                "`mxr daemon` starts the daemon directly. Use `mxr daemon` or `mxr daemon --foreground`.".to_string(),
            ),
            Some("status") => Some(
                "`mxr daemon` has no `status` verb. Use `mxr status`.".to_string(),
            ),
            Some("logs") => Some(
                "`mxr daemon` has no `logs` verb. Use `mxr logs`.".to_string(),
            ),
            Some("stop") => Some(
                "`mxr daemon` has no stop verb. Use `mxr status`, `mxr logs --level error`, or `mxr daemon --foreground`.".to_string(),
            ),
            Some("restart") => Some(
                "`mxr daemon` has no restart verb. Use `mxr restart`.".to_string(),
            ),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_labels_create_subcommand() {
        let cli = Cli::parse_from(["mxr", "labels", "create", "Urgent", "--color", "#ff6600"]);
        match cli.command {
            Some(Command::Labels {
                action:
                    Some(LabelsAction::Create {
                        name,
                        color: Some(color),
                        ..
                    }),
                ..
            }) => {
                assert_eq!(name, "Urgent");
                assert_eq!(color, "#ff6600");
            }
            other => panic!("unexpected parse result: {:?}", other.map(|_| "command")),
        }
    }

    #[test]
    fn parses_labels_rename_subcommand() {
        let cli = Cli::parse_from(["mxr", "labels", "rename", "Old", "New"]);
        match cli.command {
            Some(Command::Labels {
                action: Some(LabelsAction::Rename { old, new, .. }),
                ..
            }) => {
                assert_eq!(old, "Old");
                assert_eq!(new, "New");
            }
            other => panic!("unexpected parse result: {:?}", other.map(|_| "command")),
        }
    }

    #[test]
    fn parses_export_search_subcommand() {
        let cli = Cli::parse_from([
            "mxr",
            "export",
            "--search",
            "label:work",
            "--format",
            "mbox",
        ]);
        match cli.command {
            Some(Command::Export {
                thread_id: None,
                search: Some(search),
                format,
                output: None,
            }) => {
                assert_eq!(search, "label:work");
                assert_eq!(format, "mbox");
            }
            other => panic!("unexpected parse result: {:?}", other.map(|_| "command")),
        }
    }

    #[test]
    fn parses_web_subcommand() {
        let cli = Cli::parse_from([
            "mxr",
            "web",
            "--host",
            "127.0.0.1",
            "--port",
            "4321",
            "--print-url",
        ]);
        match cli.command {
            Some(Command::Web {
                host,
                port,
                print_url,
            }) => {
                assert_eq!(host, "127.0.0.1");
                assert_eq!(port, 4321);
                assert!(print_url);
            }
            other => panic!("unexpected parse result: {:?}", other.map(|_| "command")),
        }
    }

    #[test]
    fn parses_rules_add_subcommand() {
        let cli = Cli::parse_from([
            "mxr",
            "rules",
            "add",
            "Archive newsletters",
            "--when",
            "label:newsletters",
            "--then",
            "archive",
        ]);
        match cli.command {
            Some(Command::Rules {
                action:
                    Some(RulesAction::Add {
                        name,
                        condition,
                        action,
                        priority,
                    }),
                ..
            }) => {
                assert_eq!(name, "Archive newsletters");
                assert_eq!(condition, "label:newsletters");
                assert_eq!(action, "archive");
                assert_eq!(priority, 100);
            }
            other => panic!("unexpected parse result: {:?}", other.map(|_| "command")),
        }
    }

    #[test]
    fn parses_bug_report_flags() {
        let cli = Cli::parse_from([
            "mxr",
            "bug-report",
            "--stdout",
            "--clipboard",
            "--since",
            "2h",
        ]);
        match cli.command {
            Some(Command::BugReport {
                stdout,
                clipboard,
                since,
                edit,
                github,
                output,
                verbose,
                full_logs,
                no_sanitize,
            }) => {
                assert!(stdout);
                assert!(clipboard);
                assert_eq!(since.as_deref(), Some("2h"));
                assert!(!edit);
                assert!(!github);
                assert!(output.is_none());
                assert!(!verbose);
                assert!(!full_logs);
                assert!(!no_sanitize);
            }
            other => panic!("unexpected parse result: {:?}", other.map(|_| "command")),
        }
    }

    #[test]
    fn parses_accounts_disable_subcommand() {
        let cli = Cli::parse_from(["mxr", "accounts", "disable", "consulting"]);
        match cli.command {
            Some(Command::Accounts {
                action: Some(AccountsAction::Disable { name }),
                ..
            }) => assert_eq!(name, "consulting"),
            other => panic!("unexpected parse result: {:?}", other.map(|_| "command")),
        }
    }

    #[test]
    fn parses_accounts_remove_subcommand() {
        let cli = Cli::parse_from([
            "mxr",
            "accounts",
            "remove",
            "consulting",
            "--dry-run",
            "--yes",
            "--purge-local-data",
        ]);
        match cli.command {
            Some(Command::Accounts {
                action:
                    Some(AccountsAction::Remove {
                        name,
                        dry_run,
                        yes,
                        purge_local_data,
                    }),
                ..
            }) => {
                assert_eq!(name, "consulting");
                assert!(dry_run);
                assert!(yes);
                assert!(purge_local_data);
            }
            other => panic!("unexpected parse result: {:?}", other.map(|_| "command")),
        }
    }

    #[test]
    fn parses_rules_edit_subcommand() {
        let cli = Cli::parse_from([
            "mxr",
            "rules",
            "edit",
            "rule-1",
            "--when",
            "label:work",
            "--then",
            "archive",
            "--priority",
            "50",
            "--disable",
        ]);
        match cli.command {
            Some(Command::Rules {
                action:
                    Some(RulesAction::Edit {
                        rule,
                        condition,
                        action,
                        priority,
                        enable,
                        disable,
                        ..
                    }),
                ..
            }) => {
                assert_eq!(rule, "rule-1");
                assert_eq!(condition.as_deref(), Some("label:work"));
                assert_eq!(action.as_deref(), Some("archive"));
                assert_eq!(priority, Some(50));
                assert!(!enable);
                assert!(disable);
            }
            other => panic!("unexpected parse result: {:?}", other.map(|_| "command")),
        }
    }

    #[test]
    fn suggests_root_start_replacement() {
        let guidance = unsupported_command_guidance(&["mxr".into(), "start".into()]);
        assert!(guidance.unwrap().contains("mxr daemon"));
    }

    #[test]
    fn suggests_daemon_status_replacement() {
        let guidance =
            unsupported_command_guidance(&["mxr".into(), "daemon".into(), "status".into()]);
        assert_eq!(
            guidance.as_deref(),
            Some("`mxr daemon` has no `status` verb. Use `mxr status`.")
        );
    }

    #[test]
    fn parses_restart_subcommand() {
        let cli = Cli::parse_from(["mxr", "restart"]);
        assert!(matches!(cli.command, Some(Command::Restart)));
    }
}
