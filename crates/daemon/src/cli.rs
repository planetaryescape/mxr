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
    },
    /// Search messages
    Search {
        query: Option<String>,
        #[arg(long)]
        format: Option<OutputFormat>,
        #[arg(long, default_value = "50")]
        limit: Option<u32>,
    },
    /// Count matching messages
    Count { query: String },
    /// Display a message
    Cat {
        message_id: String,
        #[arg(long)]
        raw: bool,
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
    Headers { message_id: String },
    /// Manage saved searches
    Saved {
        #[command(subcommand)]
        action: Option<SavedAction>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Trigger or query sync
    Sync {
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        status: bool,
        #[arg(long)]
        history: bool,
    },
    /// Show daemon status
    Status {
        #[arg(long)]
        format: Option<OutputFormat>,
        #[arg(long)]
        watch: bool,
    },
    /// Watch daemon events
    Events {
        #[arg(long = "type")]
        event_type: Option<String>,
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
    },
    /// Run diagnostics
    Doctor {
        #[arg(long)]
        reindex: bool,
        #[arg(long)]
        check: bool,
        #[arg(long)]
        index_stats: bool,
        #[arg(long)]
        store_stats: bool,
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
    Drafts,
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
    Snoozed,

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
    Add { name: String, query: String },
    /// Delete a saved search
    Delete { name: String },
    /// Run a saved search
    Run { name: String },
}

#[derive(Subcommand)]
pub enum AccountsAction {
    /// Add an account
    Add { provider: String },
    /// Show account details
    Show { name: String },
    /// Test account connectivity
    Test { name: String },
}

#[derive(Subcommand)]
pub enum LabelsAction {
    /// Create a new label
    Create {
        name: String,
        #[arg(long)]
        color: Option<String>,
    },
    /// Delete a label
    Delete {
        name: String,
    },
    /// Rename a label
    Rename {
        old: String,
        new: String,
    },
}

#[derive(Subcommand)]
pub enum RulesAction {
    List,
    Show {
        rule: String,
    },
    Add {
        name: String,
        #[arg(long = "when")]
        condition: String,
        #[arg(long = "then")]
        action: String,
        #[arg(long, default_value = "100")]
        priority: i32,
    },
    Edit {
        rule: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long = "when")]
        condition: Option<String>,
        #[arg(long = "then")]
        action: Option<String>,
        #[arg(long)]
        priority: Option<i32>,
        #[arg(long, conflicts_with = "disable")]
        enable: bool,
        #[arg(long, conflicts_with = "enable")]
        disable: bool,
    },
    Validate {
        #[arg(long = "when")]
        condition: String,
        #[arg(long = "then")]
        action: String,
    },
    Enable {
        rule: String,
    },
    Disable {
        rule: String,
    },
    Delete {
        rule: String,
    },
    DryRun {
        rule: Option<String>,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        after: Option<String>,
    },
    History {
        rule: Option<String>,
        #[arg(long, default_value = "50")]
        limit: u32,
    },
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Show config file path
    Path,
}

#[derive(Subcommand)]
pub enum AttachmentAction {
    /// List attachments for a message
    List { message_id: String },
    /// Download attachment(s)
    Download {
        message_id: String,
        /// Attachment index (1-based, omit for all)
        index: Option<usize>,
        /// Output directory
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Open attachment with system handler
    #[command(name = "open")]
    OpenAttachment {
        message_id: String,
        /// Attachment index (1-based)
        index: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
    Csv,
    Ids,
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
                action: Some(LabelsAction::Rename { old, new }),
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
        let cli = Cli::parse_from(["mxr", "export", "--search", "label:work", "--format", "mbox"]);
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
}
