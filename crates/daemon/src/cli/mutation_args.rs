use clap::Subcommand;
use std::path::PathBuf;

#[derive(Subcommand)]
#[expect(
    clippy::large_enum_variant,
    reason = "clap keeps subcommand argument fields inline to preserve the CLI parser shape"
)]
pub enum AccountsAction {
    /// Add an account.
    ///
    /// Without flags, runs an interactive wizard. Pass any subset of the
    /// optional flags below to drive the flow non-interactively (scripts /
    /// CI). Passwords may also be sourced from `MXR_IMAP_PASSWORD`,
    /// `MXR_SMTP_PASSWORD`, or `MXR_GMAIL_CLIENT_SECRET` when stdin is not a
    /// TTY.
    Add {
        /// Provider type: `gmail`, `imap`, `imap-smtp`, `smtp`, `outlook`, or `outlook-work`.
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
        /// Maximum concurrent IMAP connections for this account.
        #[arg(long, default_value_t = 4)]
        imap_max_connections: usize,
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
    /// Re-authorize an OAuth account and keep cached mail
    Reauth { name: String },
    /// Re-save account passwords into mxr's local credential stores
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
    /// Set an account's colour and tag, the mark on its mail. Config values
    /// override what the daemon stored and derived. Pass --color, --tag or both.
    Identity {
        /// Account key, as shown by `mxr accounts`.
        key: String,
        /// Colour slot: sky, amber, green, yellow, blue, vermilion, purple or slate.
        #[arg(long)]
        color: Option<mxr_core::AccountColor>,
        /// Tag of 1 to 3 letters or digits. Must be unique across accounts.
        #[arg(long)]
        tag: Option<String>,
        /// Show the change without writing config.
        #[arg(long)]
        dry_run: bool,
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
        /// Upper bound: ignore contacts whose last inbound is older than
        /// this. Default ~3 years keeps focus on relationships you might
        /// still rebuild; raise for a wider audit.
        #[arg(long, default_value = "1095")]
        max_lookback_days: u32,
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
        /// Show the stored mail this would reclassify, without adding the address.
        #[arg(long)]
        dry_run: bool,
    },
    /// Remove an alias from an account.
    Remove {
        #[arg(long)]
        account: Option<String>,
        email: String,
        /// Show the stored mail this would reclassify, without removing the address.
        #[arg(long)]
        dry_run: bool,
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
pub enum CategoriesAction {
    Why {
        message_id: String,
        category_id: String,
    },
    Create {
        name: String,
        #[arg(long)]
        dry_run: bool,
    },
    Rename {
        category_id: String,
        new_name: String,
        #[arg(long)]
        dry_run: bool,
    },
    Show {
        category_id: String,
        #[arg(long, default_value = "50")]
        limit: u32,
        #[arg(long, default_value_t = 0)]
        offset: u32,
    },
    /// Preview selected message assignments, or apply them with a preview token.
    Assign {
        #[arg(long, value_delimiter = ',')]
        message_ids: Vec<String>,
        #[arg(long, value_delimiter = ',')]
        add: Vec<String>,
        #[arg(long, value_delimiter = ',')]
        remove: Vec<String>,
        #[arg(long)]
        preview_token: Option<String>,
    },
    /// Preview deletion, or confirm the exact preview with its token.
    Delete {
        #[arg(required_unless_present = "preview_token")]
        category_id: Option<String>,
        #[arg(long)]
        preview_token: Option<String>,
    },
    Undo {
        mutation_id: String,
    },
}

#[derive(Subcommand)]
pub enum RulesAction {
    /// Export the complete editable RuleFormData JSON for preview/apply.
    Form {
        rule: String,
    },
    /// Preview the complete category-rule reconciliation, including old contributions.
    CategoryPreview {
        rule: Option<String>,
        #[arg(long, conflicts_with = "rule")]
        form: Option<PathBuf>,
        #[arg(long)]
        delete: bool,
    },
    /// Save or delete a category rule and apply its unchanged preview.
    CategoryApply {
        rule: Option<String>,
        #[arg(long, conflicts_with = "rule")]
        form: Option<PathBuf>,
        #[arg(long)]
        preview_token: String,
        #[arg(long)]
        delete: bool,
    },
    /// Preview a saved rule or an unsaved JSON form; returns the token Apply requires.
    TreatmentPreview {
        rule: Option<String>,
        /// RuleFormData JSON file, or - for stdin. Does not save the draft.
        #[arg(long, conflicts_with = "rule")]
        form: Option<PathBuf>,
    },
    /// Apply exactly a sorting preview, rejecting changed mail or rules.
    TreatmentApply {
        rule: Option<String>,
        token: Option<String>,
        /// The unchanged RuleFormData JSON file, or - for stdin.
        #[arg(long, conflicts_with = "rule")]
        form: Option<PathBuf>,
        /// Token from a draft preview (saved rules may pass it positionally).
        #[arg(long, conflicts_with = "token")]
        preview_token: Option<String>,
    },
    List,
    Show {
        rule: String,
    },
    Add {
        name: String,
        #[arg(long)]
        account: Option<String>,
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
    /// List attachments for one message, or for every match of a
    /// `--search QUERY`. Multi-message output prefixes each section
    /// with `--- MESSAGE_ID ---`.
    List {
        message_id: Option<String>,
        #[arg(long, conflicts_with = "message_id")]
        search: Option<String>,
        #[arg(long)]
        account: Option<String>,
        #[arg(long, requires = "search", conflicts_with = "limit")]
        first: bool,
        #[arg(long, requires = "search")]
        limit: Option<u32>,
        #[arg(long)]
        format: Option<crate::cli::OutputFormat>,
    },
    /// Download attachment(s)
    Download {
        message_id: String,
        #[arg(long)]
        account: Option<String>,
        /// Attachment index (1-based, omit for all)
        index: Option<usize>,
        /// Output directory
        #[arg(long)]
        dir: Option<PathBuf>,
        /// Output format for completed local paths
        #[arg(long)]
        format: Option<crate::cli::OutputFormat>,
    },
    /// Open attachment with system handler
    #[command(name = "open")]
    OpenAttachment {
        message_id: String,
        #[arg(long)]
        account: Option<String>,
        /// Attachment index (1-based)
        index: usize,
        /// Output format for the local file
        #[arg(long)]
        format: Option<crate::cli::OutputFormat>,
    },
}
