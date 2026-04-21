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
    Delete { name: String },
    /// Rename a label
    Rename { old: String, new: String },
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
