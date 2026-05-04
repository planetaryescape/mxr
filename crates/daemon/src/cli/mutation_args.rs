use clap::Subcommand;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum AccountsAction {
    /// Add an account.
    ///
    /// Without flags, runs an interactive wizard. Pass any subset of the
    /// optional flags below to drive the flow non-interactively (scripts /
    /// CI). Passwords may also be sourced from `MXR_IMAP_PASSWORD`,
    /// `MXR_SMTP_PASSWORD`, or `MXR_GMAIL_CLIENT_SECRET` when stdin is not a
    /// TTY.
    Add {
        /// Provider type: `gmail`, `imap`, `imap-smtp`, or `smtp`.
        provider: String,
        /// Account key (the short identifier used by other commands).
        #[arg(long)]
        account_name: Option<String>,
        /// Email address for the account.
        #[arg(long)]
        email: Option<String>,
        /// Display name shown in From: headers; defaults to account name.
        #[arg(long)]
        display_name: Option<String>,
        // Gmail-specific.
        /// Use the bundled OAuth client (true) or supply your own (false).
        #[arg(long)]
        gmail_bundled: Option<bool>,
        /// Custom Gmail OAuth client ID (only when `--gmail-bundled=false`).
        #[arg(long)]
        gmail_client_id: Option<String>,
        /// Custom Gmail OAuth client secret.
        #[arg(long)]
        gmail_client_secret: Option<String>,
        // IMAP-specific.
        #[arg(long)]
        imap_host: Option<String>,
        #[arg(long, default_value_t = 993)]
        imap_port: u16,
        /// IMAP requires authentication. Default true.
        #[arg(long)]
        imap_no_auth: bool,
        #[arg(long)]
        imap_username: Option<String>,
        /// IMAP password (or set `MXR_IMAP_PASSWORD`).
        #[arg(long)]
        imap_password: Option<String>,
        // SMTP-specific.
        #[arg(long)]
        smtp_host: Option<String>,
        #[arg(long, default_value_t = 587)]
        smtp_port: u16,
        #[arg(long)]
        smtp_no_auth: bool,
        #[arg(long)]
        smtp_username: Option<String>,
        /// SMTP password (or set `MXR_SMTP_PASSWORD`).
        #[arg(long)]
        smtp_password: Option<String>,
    },
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
    /// Manage owned addresses (aliases) for an account. Direction inference
    /// uses these to classify inbound vs outbound mail.
    Addresses {
        #[command(subcommand)]
        op: AddressesOp,
    },
}

#[derive(Subcommand)]
pub enum ContactsAction {
    /// Rank contacts by reply imbalance (|inbound - outbound| / max).
    Asymmetry {
        /// Filter out contacts with fewer than this many inbound messages.
        #[arg(long, default_value = "3")]
        min_inbound: u32,
        #[arg(long, default_value = "50")]
        limit: u32,
        #[arg(long)]
        account: Option<String>,
    },
    /// List contacts where the most recent inbound is older than the
    /// outbound by more than --threshold-days days. Surfaces "going cold"
    /// relationships.
    Decay {
        #[arg(long, default_value = "30")]
        threshold_days: u32,
        #[arg(long, default_value = "50")]
        limit: u32,
        #[arg(long)]
        account: Option<String>,
    },
    /// Force a full refresh of the materialized `contacts` table.
    Refresh,
}

#[derive(Subcommand)]
pub enum AddressesOp {
    /// List addresses owned by an account.
    List {
        /// Account name or id. Defaults to the configured default account.
        #[arg(long)]
        account: Option<String>,
    },
    /// Add an address as an alias on an account.
    Add {
        /// Account name or id. Defaults to the configured default account.
        #[arg(long)]
        account: Option<String>,
        email: String,
        /// Mark as primary; demotes the previous primary atomically.
        #[arg(long)]
        primary: bool,
    },
    /// Remove an alias from an account.
    Remove {
        #[arg(long)]
        account: Option<String>,
        email: String,
    },
    /// Promote an existing alias to primary (demoting the previous primary).
    SetPrimary {
        #[arg(long)]
        account: Option<String>,
        email: String,
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
