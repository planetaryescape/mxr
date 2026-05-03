use clap::Subcommand;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum AccountsAction {
    /// Add an account
    Add { provider: String },
    /// Show account details
    Show { name: String },
    /// Test account connectivity
    Test { name: String },
    /// Re-save account passwords into the protected keychain store
    Repair { name: String },
    /// Disable an account without deleting local cached mail
    Disable { name: String },
    /// Remove an account from config; cached mail is kept unless purged
    Remove {
        name: String,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        purge_local_data: bool,
    },
}

#[derive(Subcommand)]
pub enum LabelsAction {
    /// Create a new label
    Create {
        name: String,
        #[arg(long)]
        color: Option<String>,
        /// Show what would change without creating the label.
        #[arg(long)]
        dry_run: bool,
    },
    /// Delete a label
    Delete {
        name: String,
        /// Show what would change without deleting the label.
        #[arg(long)]
        dry_run: bool,
        /// Skip the interactive confirmation prompt.
        #[arg(long)]
        yes: bool,
    },
    /// Rename a label
    Rename {
        old: String,
        new: String,
        /// Show what would change without renaming the label.
        #[arg(long)]
        dry_run: bool,
        /// Skip the interactive confirmation prompt.
        #[arg(long)]
        yes: bool,
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
    /// Print the resolved config (default).
    Show {
        #[arg(long, value_enum)]
        format: Option<crate::cli::OutputFormat>,
    },
    /// Show config file path
    Path,
    /// Open config in $EDITOR
    Edit,
    /// Get a config value
    Get {
        /// Dotted key path (e.g. general.sync_interval, appearance.theme)
        key: String,
    },
    /// Set a config value
    Set {
        /// Dotted key path (e.g. general.sync_interval, appearance.theme)
        key: String,
        /// Value to set
        value: String,
    },
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
