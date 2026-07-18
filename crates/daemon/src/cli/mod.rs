#![cfg_attr(
    test,
    expect(
        clippy::panic,
        clippy::unwrap_used,
        reason = "CLI parser tests use panic and unwrap for direct assertion failures"
    )
)]

mod mutation_args;
mod search_args;

pub use mutation_args::*;
pub use search_args::*;

use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "mxr",
    about = "Terminal email client (pronounced \"Mixer\")",
    version = concat!(env!("CARGO_PKG_VERSION"), " (Mixer)"),
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum McpCommand {
    /// Serve MCP over stdio
    Serve,
}

#[derive(Subcommand)]
pub enum DaemonAction {
    /// Pipe raw bytes between stdin/stdout and the local daemon's Unix
    /// socket (the Docker `connhelper` model). Lets any transport that can
    /// exec a process and pipe stdio reach the daemon — for example
    /// `ssh host mxr daemon dial-stdio` or
    /// `docker exec -i <container> mxr daemon dial-stdio`. The caller still
    /// needs local Unix-socket access on the daemon's machine, so this adds
    /// no new trust surface.
    ///
    /// Same-machine assumptions degrade over remote links: the `$EDITOR`
    /// compose flow, attachment paths, and daemon autostart all target the
    /// daemon's host, not your terminal. Intended for scripting and agent use.
    #[command(name = "dial-stdio")]
    DialStdio,
}

#[derive(Subcommand)]
pub enum Command {
    /// Start the daemon explicitly
    Daemon {
        /// Daemon subcommand. Omit to start the daemon.
        #[command(subcommand)]
        action: Option<DaemonAction>,
        /// Run in foreground (for debugging / systemd)
        #[arg(long)]
        foreground: bool,
        /// Hidden instance marker used by daemon autostart to identify the child process.
        #[arg(long, hide = true)]
        instance: Option<String>,
        /// Disable the HTTP bridge for this daemon run, regardless of config.
        #[arg(long)]
        no_bridge: bool,
        /// Override the bridge port. Useful for tests and ephemeral bridges.
        #[arg(long)]
        bridge_port: Option<u16>,
    },
    /// Restart the daemon with the current binary
    Restart,
    /// Model Context Protocol server commands
    Mcp {
        #[command(subcommand)]
        action: McpCommand,
    },
    /// Search messages
    Search {
        query: Option<String>,
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        format: Option<OutputFormat>,
        #[arg(long, default_value = "50")]
        limit: Option<u32>,
        /// Skip this many matching results before returning a page.
        #[arg(long, default_value_t = 0)]
        offset: u32,
        #[arg(long, value_enum)]
        mode: Option<SearchModeArg>,
        #[arg(long, value_enum)]
        sort: Option<SearchSortArg>,
        /// Aggregate the query result set by a field instead of listing messages.
        #[arg(long, value_enum)]
        group_by: Option<SearchGroupByArg>,
        #[arg(long)]
        explain: bool,
        /// Classify matching messages with cached ACTION/FYI/ROUTINE verdicts.
        #[arg(long)]
        triage: bool,
    },
    /// Classify search results as ACTION/FYI/ROUTINE using the cached summarizer verdict
    Triage {
        query: String,
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        format: Option<OutputFormat>,
        #[arg(long, default_value = "50")]
        limit: Option<u32>,
        #[arg(long, value_enum)]
        mode: Option<SearchModeArg>,
        #[arg(long, value_enum)]
        sort: Option<TriageSortArg>,
        #[arg(long, value_enum)]
        verdict: Option<TriageVerdictArg>,
    },
    /// Count matching messages
    Count {
        query: String,
        #[arg(long)]
        account: Option<String>,
        #[arg(long, value_enum)]
        mode: Option<SearchModeArg>,
        /// Print a bare integer, ignoring output format and tty detection.
        #[arg(long)]
        quiet: bool,
        /// Aggregate the query result set by a field instead of printing one count.
        #[arg(long, value_enum)]
        group_by: Option<SearchGroupByArg>,
        #[arg(long, value_enum)]
        format: Option<OutputFormat>,
    },
    /// Display a message. Pass a positional ID, pipe IDs on stdin, or
    /// resolve a list with `--search QUERY` (then `--first` for the
    /// most recent match or `--limit N` for the top N).
    Cat {
        /// Message ID (or omit when piping IDs / using --search).
        message_id: Option<String>,
        /// Resolve target(s) by query instead of a positional ID. Iterates
        /// over each match with a separator.
        #[arg(long, conflicts_with = "message_id")]
        search: Option<String>,
        #[arg(long)]
        account: Option<String>,
        /// Only display the most recent match (when --search is used).
        #[arg(long, requires = "search", conflicts_with = "limit")]
        first: bool,
        /// Cap the number of matches displayed (when --search is used).
        #[arg(long, requires = "search")]
        limit: Option<u32>,
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
    /// Display a thread. Pass a positional ID, pipe IDs on stdin, or
    /// resolve a list with `--search QUERY` (deduplicated by thread).
    Thread {
        thread_id: Option<String>,
        #[arg(long, conflicts_with = "thread_id")]
        search: Option<String>,
        #[arg(long)]
        account: Option<String>,
        #[arg(long, requires = "search", conflicts_with = "limit")]
        first: bool,
        #[arg(long, requires = "search")]
        limit: Option<u32>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// List threads in date-descending order. Each returned thread
    /// includes its constituent message IDs (date-ascending). Filter
    /// by account or label; paginate with --limit/--offset.
    Threads {
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        label: Option<String>,
        #[arg(long, default_value_t = 50)]
        limit: u32,
        #[arg(long, default_value_t = 0)]
        offset: u32,
        #[arg(long)]
        sort: Option<ThreadsSort>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Export a thread or matching search results
    Export {
        thread_id: Option<String>,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        account: Option<String>,
        #[arg(long, default_value = "markdown")]
        format: String,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Show message headers. Pass a positional ID, pipe IDs on stdin,
    /// or resolve a list with `--search QUERY`.
    Headers {
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
        format: Option<OutputFormat>,
    },
    /// Manage saved searches
    Saved {
        #[command(subcommand)]
        action: Option<SavedAction>,
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Manage the reply-later queue
    Replies {
        #[command(subcommand)]
        action: Option<RepliesAction>,
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Summarise an email thread using the configured LLM (Ollama, LM
    /// Studio, OpenAI, etc.). Requires `[llm] enabled = true` in config.
    /// Pass a positional thread ID or use `--search QUERY` plus `--first`
    /// (most recent match) or `--limit N` to summarize multiple threads
    /// in one go. Multi-summary output is separated by `--- THREAD_ID ---`.
    Summarize {
        /// Thread ID to summarise.
        thread_id: Option<String>,
        #[arg(long, conflicts_with = "thread_id")]
        search: Option<String>,
        #[arg(long)]
        account: Option<String>,
        /// Summarize only the most recent matching thread.
        #[arg(long, requires = "search", conflicts_with = "limit")]
        first: bool,
        /// Cap the number of threads summarized when --search is used.
        /// Each summary is an LLM call — keep this low when targeting
        /// metered cloud endpoints.
        #[arg(long, requires = "search")]
        limit: Option<u32>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Generate a draft reply for a thread, grounded on the thread
    /// context plus the user's instruction. Output goes to stdout — pipe
    /// it into `$EDITOR` or your scratch buffer. Never auto-sends.
    /// Two equivalent forms:
    ///
    ///   - `mxr draft-assist THREAD_ID "decline politely"` (legacy positional)
    ///   - `mxr draft-assist --search 'from:acme' --first --instruct "..."` (search)
    ///
    /// The instruction can be a positional second argument OR provided
    /// via `--instruct` — pick whichever composes better. `--search`
    /// requires `--instruct`.
    DraftAssist {
        /// Thread ID to reply to.
        thread_id: Option<String>,
        /// Plain-language instruction (e.g. `"decline politely"`).
        /// Required unless `--instruct` is provided.
        instruction: Option<String>,
        /// Resolve target thread by query instead of a positional ID.
        #[arg(long, conflicts_with = "thread_id")]
        search: Option<String>,
        #[arg(long)]
        account: Option<String>,
        /// Use only the most recent matching thread.
        #[arg(long, requires = "search", conflicts_with = "limit")]
        first: bool,
        /// Cap the number of threads drafted for when --search is used.
        #[arg(long, requires = "search")]
        limit: Option<u32>,
        /// Long form of the positional instruction. Required when
        /// `--search` is used (no positional fallback).
        #[arg(long = "instruct", value_name = "TEXT")]
        instruct: Option<String>,
        /// Override the inferred tone (otherwise matched to how you write to
        /// this person).
        #[arg(long, value_enum)]
        register: Option<VoiceRegisterArg>,
        /// Override the inferred length.
        #[arg(long, value_enum)]
        length: Option<DraftLengthArg>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Draft a new email or refine an existing local draft with LLM assistance.
    Draft {
        #[command(subcommand)]
        action: Option<DraftAction>,
        /// Recipient email for a new draft.
        #[arg(long, requires = "purpose")]
        to: Option<String>,
        /// Plain-language purpose for a new draft.
        #[arg(long, requires = "to")]
        purpose: Option<String>,
        /// Account key, email, or id.
        #[arg(long)]
        account: Option<String>,
        /// Register to use when recipient has no relationship profile.
        #[arg(long, value_enum)]
        register: Option<VoiceRegisterArg>,
        /// Desired length.
        #[arg(long, value_enum)]
        length: Option<DraftLengthArg>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// First-run setup wizard for demo, Gmail, or IMAP/SMTP.
    Setup {
        /// Legacy helper that drops a fake-provider account into the current
        /// config. Prefer `mxr demo` for an isolated two-account demo profile.
        #[arg(long)]
        demo: bool,
        /// Account key to use when writing the demo entry. Defaults to
        /// `demo`. Used to namespace the entry — handy if you want to
        /// keep a real account configured alongside.
        #[arg(long, default_value = "demo")]
        key: String,
        /// Skip the safety check that refuses to overwrite an existing
        /// account with the same key.
        #[arg(long)]
        force: bool,
    },
    /// Launch an isolated, realistic demo inbox without touching your real config.
    ///
    /// Once started, demo mode is sticky: every subsequent `mxr` command
    /// (search, cat, archive, web, etc.) operates on the demo profile until
    /// you run `mxr demo stop`.
    Demo {
        /// Optional subcommand. Defaults to starting the demo when omitted.
        #[command(subcommand)]
        action: Option<DemoAction>,
        /// Reset the demo profile before launching. Equivalent to `mxr demo reset`
        /// followed by `mxr demo`, kept as a flag for backward compatibility.
        #[arg(long)]
        reset: bool,
        /// Number of synthetic demo messages to seed. Defaults to a large mailbox
        /// so search, analytics, and video demos feel real.
        #[arg(long, default_value_t = 50_000)]
        messages: usize,
        /// Seed and sync the demo profile, but do not open the TUI.
        #[arg(long)]
        no_tui: bool,
    },
    /// Triage unknown senders: classify them as allow / deny / feed /
    /// paper-trail. Local-only consent metadata; never roundtrips to
    /// the provider.
    Screener {
        #[command(subcommand)]
        action: Option<ScreenerAction>,
        /// Restrict to a specific account.
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Show per-sender relationship aggregates: volume, response cadence,
    /// open threads. The unfair advantage of having local SQLite — every
    /// other email tool reasons over messages, not people.
    Sender {
        /// Email address (must match an existing contact).
        email: String,
        /// Restrict to a specific account; defaults to the only-or-default
        /// account if exactly one is configured.
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Show or rebuild the inspectable relationship profile for a contact.
    Profile {
        email: String,
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        rebuild: bool,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Find people who have answered similar questions before.
    /// Either give the message id of the question OR a free-text
    /// query; the daemon ranks answerers (not askers).
    Expert {
        /// Message id of the question. Mutually exclusive with --query.
        message_id: Option<String>,
        #[arg(long, conflicts_with = "message_id")]
        query: Option<String>,
        /// Include the current user in results (excluded by default).
        #[arg(long)]
        include_self: bool,
        #[arg(long)]
        account: Option<String>,
        #[arg(long, default_value_t = 5)]
        limit: u32,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Suggest "maybe include" Cc recipients for a draft. Excludes
    /// addresses already on the draft and never reveals Bcc'd
    /// addresses from prior threads.
    SuggestRecipients {
        /// Stored draft id. Mutually exclusive with --subject.
        #[arg(long, conflicts_with = "subject")]
        draft: Option<String>,
        /// Subject for an ephemeral draft. Pair with --body-stdin
        /// for the body.
        #[arg(long)]
        subject: Option<String>,
        /// Read the body from stdin.
        #[arg(long = "body-stdin")]
        body_stdin: bool,
        #[arg(long)]
        account: Option<String>,
        #[arg(long, default_value_t = 5)]
        limit: u32,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Explain an entity (email or term) using local evidence.
    Whois {
        query: String,
        #[arg(long)]
        account: Option<String>,
        #[arg(long, default_value_t = 10)]
        limit: u32,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Render a thread or recipient briefing.
    Briefing {
        #[command(subcommand)]
        action: BriefingAction,
    },
    /// Manage the cadence watchlist.
    Cadence {
        #[command(subcommand)]
        action: CadenceAction,
    },
    /// Show the recipient's typical reply-time bucket.
    ///
    /// When `--at` is given, also reports the expected reply time
    /// for that slot so you can compare a proposed send time
    /// against the recipient's fastest bucket. Same time forms as
    /// `mxr remind --when`: `tomorrow 9am`, `fri 19:00`, `in 2h`,
    /// or RFC3339.
    SendTime {
        #[arg(required = true)]
        recipients: Vec<String>,
        #[arg(long)]
        account: Option<String>,
        /// Proposed send time to evaluate against the recipient's
        /// historical buckets.
        #[arg(long)]
        at: Option<String>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// List or rebuild the citation-backed decision log.
    Decisions {
        #[command(subcommand)]
        action: Option<DecisionsAction>,
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        topic: Option<String>,
        #[arg(long = "since")]
        since_days: Option<u32>,
        #[arg(long, default_value_t = 50)]
        limit: u32,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Ask a question against the local archive. Returns a citation-
    /// validated answer; the daemon rejects LLM citations that point
    /// to messages outside the retrieved set.
    Ask {
        /// The question to ask.
        question: String,
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        from: Option<String>,
        #[arg(long)]
        to: Option<String>,
        #[arg(long)]
        after: Option<String>,
        #[arg(long)]
        before: Option<String>,
        #[arg(long, value_enum, default_value_t = ArchiveAskModeArg::Hybrid)]
        mode: ArchiveAskModeArg,
        #[arg(long, default_value_t = 8)]
        limit: u32,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// List threads where the user owes a reply, ranked by how
    /// overdue they are relative to the recipient's typical cadence.
    Owed {
        #[arg(long)]
        account: Option<String>,
        /// Only include threads waiting at least this many days.
        #[arg(long = "since")]
        older_than_days: Option<u32>,
        /// Only include threads whose latest inbound landed in the
        /// last N days (excludes ancient unanswered threads).
        #[arg(long = "within")]
        within_days: Option<u32>,
        #[arg(long, default_value_t = 50)]
        limit: u32,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// List or resolve relationship commitments.
    Commitments {
        #[command(subcommand)]
        action: Option<CommitmentsAction>,
        #[arg(long = "contact")]
        contact: Option<String>,
        #[arg(long)]
        status: Option<CommitmentStatusArg>,
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Show or rebuild the account-level user voice profile.
    Voice {
        #[command(subcommand)]
        action: Option<VoiceAction>,
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Score or rewrite text using the deterministic humanizer gate.
    Humanize {
        #[command(subcommand)]
        action: HumanizeAction,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Manage compose snippets (`;name` expansions)
    Snippets {
        #[command(subcommand)]
        action: Option<SnippetsAction>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Track packages and deliveries detected in your mail
    Deliveries {
        #[command(subcommand)]
        action: Option<DeliveriesAction>,
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Manage outgoing compose signatures
    Signatures {
        #[command(subcommand)]
        action: Option<SignaturesAction>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Set or cancel a follow-up reminder on an outbound message.
    /// Reminders fire if no reply has arrived by the given time —
    /// surfacing the message back to the user as a follow-up.
    Remind {
        message_id: String,
        #[arg(long)]
        account: Option<String>,
        /// When to fire the reminder. Same forms accepted by `mxr snooze --until`:
        /// `in 2h`, `in 5d`, `tomorrow 9am`, `monday 17:00`, RFC3339.
        #[arg(long, conflicts_with = "cancel")]
        when: Option<String>,
        /// Cancel an existing reminder on this message.
        #[arg(long)]
        cancel: bool,
    },
    /// Manage semantic search profiles and indexing
    Semantic {
        #[command(subcommand)]
        action: Option<SemanticAction>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Inspect local/cloud LLM provider status
    Llm {
        #[command(subcommand)]
        action: Option<LlmAction>,
        #[arg(long, global = true)]
        format: Option<OutputFormat>,
    },
    /// Manage local notification chimes
    Chimes {
        #[command(subcommand)]
        action: Option<ChimesAction>,
        #[arg(long, global = true)]
        format: Option<OutputFormat>,
    },
    /// List senders with unsubscribe support
    #[command(alias = "unsub")]
    Subscriptions {
        #[arg(long, default_value = "200")]
        limit: u32,
        #[arg(long)]
        account: Option<String>,
        /// Rank by newsletter ROI: lowest open-rate first, ties broken by
        /// archived-unread descending. Highlights the lists most worth dropping.
        #[arg(long)]
        rank: bool,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// List top inbound senders by message volume.
    ///
    /// `--since` restricts the count window to recent messages so the
    /// ranking reflects *current* noise, not all-time history. Accepts
    /// shorthand durations like `7d` / `4w` / `12h`, or an RFC-3339
    /// timestamp (`2026-02-01T00:00:00Z`).
    Senders {
        #[arg(long, default_value = "20")]
        top: u32,
        #[arg(long)]
        account: Option<String>,
        /// Only count messages received within this window. Defaults
        /// to unbounded (all-time).
        #[arg(long)]
        since: Option<String>,
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
    /// Year-in-review summary: volume, time patterns, top contacts,
    /// reply discipline, storage, newsletters, superlatives. Like Spotify
    /// Wrapped but for your inbox.
    Wrapped {
        /// Year-to-date (default if no window flag given): Jan 1 → now.
        #[arg(long, conflicts_with_all = ["year", "since_days"])]
        ytd: bool,
        /// Specific calendar year (Jan 1 → Dec 31 UTC).
        #[arg(long, conflicts_with_all = ["ytd", "since_days"])]
        year: Option<i32>,
        /// Last N days. Useful for quarterly or ad-hoc reviews.
        #[arg(long, conflicts_with_all = ["ytd", "year"])]
        since_days: Option<u32>,
        #[arg(long)]
        account: Option<String>,
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
        /// Upper bound: threads idle for longer than this are also excluded
        /// (default ~1 year). Keeps the result actionable instead of
        /// surfacing decade-old archives. Set high to widen the window.
        #[arg(long, default_value = "365")]
        within_days: u32,
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
    /// Start or reopen the local HTTP/WebSocket bridge and open the web app in
    /// the default browser. Runs detached by default; use `mxr web stop` to
    /// stop the detached bridge.
    Web {
        #[command(subcommand)]
        action: Option<WebAction>,
        /// Bind address for the bridge. Defaults to loopback.
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// Bridge port. Defaults to a fixed local URL port. `0` picks an
        /// ephemeral port (useful for tests).
        #[arg(long, default_value_t = 42829)]
        port: u16,
        /// Print the launch URL instead of opening the browser.
        #[arg(long)]
        print_url: bool,
        /// Do not open the system browser; just print the URL.
        #[arg(long)]
        no_open: bool,
        /// Try the next available port if `--port` is in use.
        #[arg(long)]
        auto_port: bool,
        /// Fail immediately if `--port` is in use. This is the default;
        /// kept as an explicit compatibility flag.
        #[arg(long)]
        strict_port: bool,
        /// Open the browser pointed at a manually configured remote bridge.
        /// Prefer SSH/Tailscale tunnels unless you've set up TLS, CORS, and Host allowlists.
        /// Format: `host[:port]`, e.g. `mxr.example.com` or `mxr.example.com:443`.
        /// Reads the per-host token from `~/.config/mxr/bridge-tokens/<host>.token`.
        #[arg(long, value_name = "HOST")]
        remote_host: Option<String>,
        /// Run the bridge in the foreground instead of detaching it.
        #[arg(long)]
        foreground: bool,
        /// Internal marker for the detached child process.
        #[arg(long, hide = true)]
        detached_child: bool,
    },
    /// Watch daemon events
    Events {
        #[arg(long = "type")]
        event_type: Option<String>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Browse the local user-activity log (the git-reflog for your inbox).
    /// Strictly local: never transmitted off-device. See `mxr activity --help`.
    #[command(alias = "act")]
    Activity {
        #[command(subcommand)]
        action: ActivityAction,
    },
    /// Show persisted event history
    History {
        /// Exact category match (e.g. `mutation`, `sync`).
        #[arg(long)]
        category: Option<String>,
        /// Category prefix (e.g. `sync.`). Matches `category LIKE prefix%`.
        #[arg(long)]
        category_prefix: Option<String>,
        #[arg(long)]
        level: Option<String>,
        /// Free-text substring match against `summary` and `details`. Case-insensitive.
        #[arg(long)]
        search: Option<String>,
        /// Lower-bound timestamp. Accepts `1h`/`3d`/`2w` or ISO date.
        #[arg(long)]
        since: Option<String>,
        /// Upper-bound timestamp. Accepts `1h`/`3d`/`2w` or ISO date.
        #[arg(long)]
        until: Option<String>,
        /// Result offset for paging.
        #[arg(long, default_value_t = 0)]
        offset: u32,
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
        /// Free-text substring filter applied to each log line. Case-insensitive.
        #[arg(long)]
        search: Option<String>,
        /// Maximum lines to return when not following. Default 200.
        #[arg(long, default_value_t = 200)]
        limit: u32,
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
        /// Backfill semantic chunks/embeddings for existing messages.
        #[arg(long)]
        backfill_semantic: bool,
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
        /// Recompute the `link_count` + `body_word_count` for every message so
        /// the tri-state link indicator and `has:link*` filters populate on
        /// rows synced before the link-extractor existed.
        #[arg(long)]
        recompute_link_counts: bool,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Manage labels
    Labels {
        #[command(subcommand)]
        action: Option<LabelsAction>,
        #[arg(long)]
        account: Option<String>,
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
        /// Insert this signature by name instead of the scoped default
        #[arg(long, conflicts_with = "no_signature")]
        signature: Option<String>,
        /// Do not insert any signature
        #[arg(long)]
        no_signature: bool,
        /// Skip confirmation prompt
        #[arg(long)]
        yes: bool,
        /// Show what would be sent without sending
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        format: Option<OutputFormat>,
        /// Build a transient draft from these args and run the
        /// pre-send safety pipeline against it without sending or
        /// saving. Exit non-zero only on Blocker issues. Useful for
        /// CI/pre-commit hooks: pipe a body in and assert the JSON
        /// report.
        #[arg(long, conflicts_with_all = ["dry_run", "yes"])]
        check: bool,
        /// With `--check`: skip LLM-backed checks (answer-coverage).
        /// Has no effect on a real send.
        #[arg(long, requires = "check")]
        no_llm: bool,
    },
    /// Reply to a message
    Reply {
        /// Message ID to reply to
        message_id: String,
        #[arg(long)]
        account: Option<String>,
        /// Inline reply body (skip $EDITOR)
        #[arg(long)]
        body: Option<String>,
        /// Read reply body from stdin
        #[arg(long)]
        body_stdin: bool,
        /// File path to attach (repeatable)
        #[arg(long, action = clap::ArgAction::Append)]
        attach: Vec<PathBuf>,
        /// Insert this signature by name instead of the scoped default
        #[arg(long, conflicts_with = "no_signature")]
        signature: Option<String>,
        /// Do not insert any signature
        #[arg(long)]
        no_signature: bool,
        /// Skip confirmation
        #[arg(long)]
        yes: bool,
        /// Show what would be sent
        #[arg(long)]
        dry_run: bool,
        /// After sending, remind if no reply has arrived by this time.
        /// Same forms as `mxr remind --when`: `in 2h`, `tomorrow 9am`,
        /// `monday 17:00`, RFC3339.
        #[arg(
            long,
            value_name = "TIME",
            requires = "yes",
            conflicts_with = "dry_run"
        )]
        remind_after: Option<String>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Reply to all recipients
    ReplyAll {
        /// Message ID to reply to
        message_id: String,
        #[arg(long)]
        account: Option<String>,
        /// Inline reply body
        #[arg(long)]
        body: Option<String>,
        /// Read body from stdin
        #[arg(long)]
        body_stdin: bool,
        /// File path to attach (repeatable)
        #[arg(long, action = clap::ArgAction::Append)]
        attach: Vec<PathBuf>,
        /// Insert this signature by name instead of the scoped default
        #[arg(long, conflicts_with = "no_signature")]
        signature: Option<String>,
        /// Do not insert any signature
        #[arg(long)]
        no_signature: bool,
        /// Skip confirmation
        #[arg(long)]
        yes: bool,
        /// Show what would be sent
        #[arg(long)]
        dry_run: bool,
        /// After sending, remind if no reply has arrived by this time.
        /// Same forms as `mxr remind --when`: `in 2h`, `tomorrow 9am`,
        /// `monday 17:00`, RFC3339.
        #[arg(
            long,
            value_name = "TIME",
            requires = "yes",
            conflicts_with = "dry_run"
        )]
        remind_after: Option<String>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Forward a message
    Forward {
        /// Message ID to forward
        message_id: String,
        #[arg(long)]
        account: Option<String>,
        /// Forward to recipient(s)
        #[arg(long)]
        to: Option<String>,
        /// Inline body
        #[arg(long)]
        body: Option<String>,
        /// Read body from stdin
        #[arg(long)]
        body_stdin: bool,
        /// File path to attach (repeatable)
        #[arg(long, action = clap::ArgAction::Append)]
        attach: Vec<PathBuf>,
        /// Insert this signature by name instead of the scoped default
        #[arg(long, conflicts_with = "no_signature")]
        signature: Option<String>,
        /// Do not insert any signature
        #[arg(long)]
        no_signature: bool,
        /// Skip confirmation
        #[arg(long)]
        yes: bool,
        /// Show what would be sent
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Manage drafts: list (default), recover orphaned in-flight sends,
    /// resume one for retry, or discard recovered drafts.
    Drafts {
        #[command(subcommand)]
        action: Option<DraftsAction>,
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Send a draft by ID
    Send {
        /// Draft ID to send
        draft_id: String,
        #[arg(long)]
        account: Option<String>,
        /// Show what would be sent (sender, recipients, subject, byte count) without sending
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        format: Option<OutputFormat>,
        /// Schedule the draft to be sent later instead of sending now.
        /// Same forms as `mxr snooze --until`: `in 2h`, `tomorrow 9am`,
        /// `monday 17:00`, RFC3339. Use `mxr unsend <draft-id>` to cancel.
        #[arg(long, value_name = "TIME", conflicts_with = "dry_run")]
        at: Option<String>,
        /// After sending now, remind if no reply has arrived by this time.
        /// Same forms as `mxr remind --when`: `in 2h`, `tomorrow 9am`,
        /// `monday 17:00`, RFC3339.
        #[arg(long, value_name = "TIME", conflicts_with_all = ["dry_run", "at", "check"])]
        remind_after: Option<String>,
        /// Run the safety pipeline against the draft and exit without
        /// sending. Exit non-zero only if a Blocker issue is present.
        #[arg(long, conflicts_with_all = ["dry_run", "at"])]
        check: bool,
        /// Single-use override token (issued by a previous failed
        /// `--check`) to bypass a Blocker.
        #[arg(long, value_name = "TOKEN")]
        override_safety: Option<String>,
        /// Skip LLM-backed safety checks (answer-coverage). Useful when
        /// the daemon's LLM is a rate-limited cloud model. Only honored
        /// with `--check`.
        #[arg(long, requires = "check")]
        no_llm: bool,
    },
    /// Cancel a previously-scheduled send. The draft itself is preserved.
    Unsend {
        /// Draft ID with a scheduled send to cancel
        draft_id: String,
        #[arg(long)]
        account: Option<String>,
    },

    // --- Phase 2: Mutations ---
    /// Archive a message (remove from inbox)
    Archive {
        /// Message ID(s). If omitted, reads IDs from stdin when stdin is piped.
        #[arg(value_name = "MESSAGE_ID", conflicts_with = "search")]
        message_ids: Vec<String>,
        /// Operate on messages matching search query
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        account: Option<String>,
        /// Skip confirmation
        #[arg(long)]
        yes: bool,
        /// Show what would happen
        #[arg(long)]
        dry_run: bool,
        /// Run as a daemon-side background job and poll with `mxr jobs`.
        #[arg(long = "async")]
        async_job: bool,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Mark message as read and archive it
    #[command(name = "read-archive")]
    ReadArchive {
        #[arg(value_name = "MESSAGE_ID", conflicts_with = "search")]
        message_ids: Vec<String>,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
        /// Run as a daemon-side background job and poll with `mxr jobs`.
        #[arg(long = "async")]
        async_job: bool,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Route queued messages to a target label, optionally marking read and archiving.
    Route {
        #[arg(value_name = "MESSAGE_ID", conflicts_with = "search")]
        message_ids: Vec<String>,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        account: Option<String>,
        #[arg(long = "to")]
        to_label: String,
        #[arg(long = "from-queue")]
        from_queue_label: String,
        #[arg(long)]
        archive: bool,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Move message to trash
    Trash {
        #[arg(value_name = "MESSAGE_ID", conflicts_with = "search")]
        message_ids: Vec<String>,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
        /// Run as a daemon-side background job and poll with `mxr jobs`.
        #[arg(long = "async")]
        async_job: bool,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Report message as spam
    Spam {
        #[arg(value_name = "MESSAGE_ID", conflicts_with = "search")]
        message_ids: Vec<String>,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
        /// Run as a daemon-side background job and poll with `mxr jobs`.
        #[arg(long = "async")]
        async_job: bool,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Star a message
    Star {
        #[arg(value_name = "MESSAGE_ID", conflicts_with = "search")]
        message_ids: Vec<String>,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
        /// Run as a daemon-side background job and poll with `mxr jobs`.
        #[arg(long = "async")]
        async_job: bool,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Unstar a message
    Unstar {
        #[arg(value_name = "MESSAGE_ID", conflicts_with = "search")]
        message_ids: Vec<String>,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
        /// Run as a daemon-side background job and poll with `mxr jobs`.
        #[arg(long = "async")]
        async_job: bool,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Mark message as read
    #[command(name = "read")]
    MarkRead {
        #[arg(value_name = "MESSAGE_ID", conflicts_with = "search")]
        message_ids: Vec<String>,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
        /// Run as a daemon-side background job and poll with `mxr jobs`.
        #[arg(long = "async")]
        async_job: bool,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Mark message as unread
    Unread {
        #[arg(value_name = "MESSAGE_ID", conflicts_with = "search")]
        message_ids: Vec<String>,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
        /// Run as a daemon-side background job and poll with `mxr jobs`.
        #[arg(long = "async")]
        async_job: bool,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Apply a label to a message
    Label {
        /// Label name
        name: String,
        /// Message ID(s). If omitted, reads IDs from stdin when stdin is piped.
        #[arg(value_name = "MESSAGE_ID", conflicts_with = "search")]
        message_ids: Vec<String>,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
        /// Run as a daemon-side background job and poll with `mxr jobs`.
        #[arg(long = "async")]
        async_job: bool,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Remove a label from a message
    Unlabel {
        /// Label name
        name: String,
        /// Message ID(s). If omitted, reads IDs from stdin when stdin is piped.
        #[arg(value_name = "MESSAGE_ID", conflicts_with = "search")]
        message_ids: Vec<String>,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
        /// Run as a daemon-side background job and poll with `mxr jobs`.
        #[arg(long = "async")]
        async_job: bool,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Move message to a label/folder
    #[command(name = "move")]
    MoveMsg {
        /// Target label
        label: String,
        /// Message ID(s). If omitted, reads IDs from stdin when stdin is piped.
        #[arg(value_name = "MESSAGE_ID", conflicts_with = "search")]
        message_ids: Vec<String>,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
        /// Run as a daemon-side background job and poll with `mxr jobs`.
        #[arg(long = "async")]
        async_job: bool,
        #[arg(long)]
        format: Option<OutputFormat>,
    },

    /// Undo a recent destructive mutation by its id (~60s window).
    /// The mutation id is printed by `archive`, `trash`, `spam`,
    /// `mark-read`, and `read-archive`; copy it from there.
    Undo {
        mutation_id: String,
        /// Show which undo would run without mutating state.
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        format: Option<OutputFormat>,
    },

    /// List or inspect background jobs (large batch mutations, progress, undo ids).
    ///
    /// Job history is persisted locally and survives a daemon restart; the
    /// most recent jobs are retained.
    Jobs {
        /// Optional job id to inspect. Omit to list recent jobs.
        job_id: Option<String>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },

    // --- Phase 2: Snooze ---
    /// Snooze a message until a specified time
    Snooze {
        #[arg(value_name = "MESSAGE_ID", conflicts_with = "search")]
        message_ids: Vec<String>,
        /// When to resurface. Accepts: configured presets
        /// (tomorrow|monday|weekend|tonight), conversational forms
        /// (`in 2h`, `monday 5pm`, `tomorrow 9am`), and RFC3339 timestamps
        /// (`2026-06-01T15:00:00Z`).
        #[arg(long)]
        until: String,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Unsnooze a message
    Unsnooze {
        #[arg(value_name = "MESSAGE_ID", conflicts_with = "all")]
        message_ids: Vec<String>,
        #[arg(long)]
        account: Option<String>,
        /// Unsnooze all
        #[arg(long)]
        all: bool,
        /// Show which messages would be unsnoozed without performing the mutation
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// List snoozed messages
    Snoozed {
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },

    // --- Phase 2: Unsubscribe ---
    /// Unsubscribe from a mailing list.
    ///
    /// Positional argument accepts either a message id (uses the
    /// List-Unsubscribe header on that exact message) or an email
    /// address (`alice@example.com`), which is rewritten to
    /// `--search "from:alice@example.com"` and acts on the most recent
    /// match. Combine with `--search` to scope an address-wide
    /// unsubscribe to a label or date range.
    Unsubscribe {
        #[arg(value_name = "MESSAGE_ID_OR_ADDRESS", conflicts_with = "search")]
        message_ids: Vec<String>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        dry_run: bool,
        /// Unsubscribe, then mark read and archive the sender's whole footprint.
        #[arg(long, alias = "archive-all")]
        purge: bool,
        /// With --purge, archive the footprint even if no List-Unsubscribe method exists.
        #[arg(long, requires = "purge")]
        archive_on_no_method: bool,
        #[arg(long)]
        format: Option<OutputFormat>,
    },

    /// Open message in browser. Pass a positional ID or `--search QUERY`
    /// (with `--first` for the latest match, or `--limit N` plus `--yes`
    /// to open many tabs at once).
    Open {
        message_id: Option<String>,
        #[arg(long, conflicts_with = "message_id")]
        search: Option<String>,
        #[arg(long)]
        account: Option<String>,
        #[arg(long, requires = "search", conflicts_with = "limit")]
        first: bool,
        #[arg(long, requires = "search")]
        limit: Option<u32>,
        /// Required when `--search` resolves to more than one match.
        /// Confirms you actually want N browser tabs.
        #[arg(long)]
        yes: bool,
    },

    /// Manage message attachments
    Attachments {
        #[command(subcommand)]
        action: AttachmentAction,
    },
    /// Inspect and respond to calendar invites in email
    Invite {
        #[command(subcommand)]
        action: InviteAction,
    },
    /// List calendar invites found in email
    Invites {
        #[command(subcommand)]
        action: InvitesAction,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Subcommand)]
pub enum WebAction {
    /// Stop the detached local web bridge started by `mxr web`.
    Stop,
}

#[derive(Debug, Clone, Subcommand)]
pub enum ChimesAction {
    /// Show current chime settings
    Status,
    /// Turn notification chimes on
    Enable,
    /// Turn notification chimes off
    Disable,
    /// Set the sound for one event
    Set {
        #[arg(value_enum)]
        event: ChimeEventArg,
        #[arg(value_enum)]
        sound: ChimeSoundArg,
    },
    /// Play the configured sound for one event
    Test {
        #[arg(value_enum)]
        event: ChimeEventArg,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ChimeEventArg {
    #[value(name = "new-mail", alias = "new_mail")]
    NewMail,
    Sent,
    Archived,
    Trashed,
    Spam,
    Snoozed,
    Unsnoozed,
    Reminder,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ChimeSoundArg {
    None,
    Bell,
    Glass,
    Pop,
    Sent,
    Archive,
    Thud,
    Alert,
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

#[derive(Debug, Clone, Subcommand)]
pub enum ScreenerAction {
    /// Show senders waiting for a decision (default if no subcommand)
    Queue {
        #[arg(long, default_value = "100")]
        limit: u32,
    },
    /// List all existing decisions
    List,
    /// Allow this sender into the inbox
    Allow {
        sender_email: String,
        /// Optional provider label to apply on ingest
        #[arg(long)]
        label: Option<String>,
    },
    /// Auto-trash and mark-read this sender's mail on ingest
    Deny {
        sender_email: String,
        #[arg(long)]
        label: Option<String>,
    },
    /// Route to a feed (skip inbox)
    Feed {
        sender_email: String,
        #[arg(long)]
        label: Option<String>,
    },
    /// Route to paper trail (archive on ingest)
    PaperTrail {
        sender_email: String,
        #[arg(long)]
        label: Option<String>,
    },
    /// Clear an existing decision
    Clear { sender_email: String },
}

#[derive(Debug, Clone, Subcommand)]
pub enum DemoAction {
    /// Exit demo mode: shut down the demo daemon and remove the active marker.
    /// Real-profile commands resume on the next invocation.
    Stop,
    /// Show whether demo mode is active and where its profile lives.
    Status,
    /// Wipe the demo profile (config + data) so the next `mxr demo` re-seeds
    /// from scratch.
    Reset,
}

#[derive(Debug, Clone, Subcommand)]
pub enum DraftsAction {
    /// Show all drafts (default if no subcommand)
    List,
    /// Show drafts that look orphaned mid-send (status `'sending'`,
    /// stale heartbeat). The startup loop already auto-resets these
    /// after 1h; this surfaces them earlier so you can act now.
    Recover,
    /// Force-reset an orphaned draft to `'draft'` status so it can be
    /// re-sent via the normal pipeline. No-op if the draft is already
    /// in `'draft'`.
    Resume {
        /// Draft ID to resume.
        draft_id: String,
    },
    /// Permanently delete a draft. Use this when a recovered draft is
    /// no longer wanted instead of leaving it in the drafts list.
    Discard {
        /// Draft ID to delete.
        draft_id: String,
    },
    /// Open an existing draft in `$EDITOR` and re-save it in place under
    /// the same draft id. No new draft is created and nothing is
    /// discarded.
    Edit {
        /// Draft ID to edit.
        draft_id: String,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum DraftAction {
    /// Refine an existing local draft.
    Refine {
        draft_id: String,
        #[arg(long)]
        shorter: bool,
        #[arg(long)]
        warmer: bool,
        #[arg(long = "more-formal")]
        more_formal: bool,
        #[arg(long = "less-emoji")]
        less_emoji: bool,
        #[arg(long = "add-context")]
        add_context: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ThreadsSort {
    /// Latest message first (default).
    #[value(name = "latest-desc", alias = "date-desc")]
    DateDesc,
    /// Oldest message first.
    #[value(name = "date-asc")]
    DateAsc,
}

impl From<ThreadsSort> for mxr_core::types::SortOrder {
    fn from(value: ThreadsSort) -> Self {
        match value {
            ThreadsSort::DateDesc => Self::DateDesc,
            ThreadsSort::DateAsc => Self::DateAsc,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum VoiceRegisterArg {
    Casual,
    Neutral,
    Formal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum DraftLengthArg {
    Short,
    Medium,
    Long,
}

#[derive(Debug, Clone, Subcommand)]
pub enum SnippetsAction {
    /// List all snippets (default if no subcommand)
    List,
    /// Create or update a snippet inline
    Set {
        /// Short keyword (no spaces). Used after `;` in compose.
        name: String,
        /// Snippet body. Use `{var_name}` for placeholders.
        body: String,
        /// Comma-separated list of declared `{var}` placeholders.
        #[arg(long)]
        vars: Option<String>,
    },
    /// Delete a snippet by name
    Remove { name: String },
}

#[derive(Debug, Clone, Subcommand)]
pub enum DeliveriesAction {
    /// List tracked deliveries (default).
    List {
        /// Which set: active (default), delivered, all, dismissed.
        #[arg(long, default_value = "active")]
        filter: String,
    },
    /// Show one delivery, including its source messages.
    Get { delivery_id: String },
    /// Mark a delivery delivered/done (leaves the active list).
    Resolve { delivery_id: String },
    /// Hide a delivery (false positive).
    Dismiss { delivery_id: String },
    /// Re-scan recent mail for deliveries.
    Scan {
        /// Window in days to scan (default 90).
        #[arg(long)]
        since_days: Option<u32>,
        /// Preview detections without writing or calling the LLM.
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum SignaturesAction {
    /// List all signatures (default if no subcommand)
    List,
    /// Create or update a signature inline
    Set {
        /// Human-readable signature name
        name: String,
        /// Signature body. The RFC 3676 `-- ` delimiter is inserted at compose time if absent.
        body: String,
    },
    /// Delete a signature by name
    Remove { name: String },
    /// List scoped default signatures
    Defaults,
    /// Set a scoped default signature
    Default {
        /// Signature name
        name: String,
        /// Which compose kind this default applies to
        #[arg(long, value_enum, default_value = "all")]
        kind: SignatureDefaultKindArg,
        /// Account selector (key, name, email, or id). Omit for global default.
        #[arg(long)]
        account: Option<String>,
        /// Exact from-address default within the account.
        #[arg(long = "from", requires = "account")]
        from_email: Option<String>,
    },
    /// Clear a scoped default signature
    ClearDefault {
        /// Which compose kind to clear
        #[arg(long, value_enum, default_value = "all")]
        kind: SignatureDefaultKindArg,
        /// Account selector (key, name, email, or id). Omit for global default.
        #[arg(long)]
        account: Option<String>,
        /// Exact from-address default within the account.
        #[arg(long = "from", requires = "account")]
        from_email: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SignatureDefaultKindArg {
    All,
    New,
    Reply,
}

#[derive(Debug, Clone, Subcommand)]
pub enum CommitmentsAction {
    /// Mark a commitment resolved by id.
    Resolve { id: String },
}

#[derive(Debug, Clone, Subcommand)]
pub enum VoiceAction {
    /// Show the profile (default if omitted).
    Show,
    /// Rebuild the profile from outbound mail.
    Rebuild,
}

#[derive(Debug, Clone, Subcommand)]
pub enum HumanizeAction {
    /// Score arbitrary text.
    Score { text: String },
    /// Rewrite arbitrary text using the LLM rewrite pass.
    Rewrite {
        text: String,
        #[arg(long = "max-iterations")]
        max_iterations: Option<u8>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CommitmentStatusArg {
    Open,
    Resolved,
    Expired,
}

#[derive(Debug, Clone, Subcommand)]
pub enum RepliesAction {
    /// List messages flagged for reply-later (default if no subcommand given)
    List,
    /// Walk the reply-later queue interactively
    Walk,
    /// Mark a message for reply-later
    Add { message_id: String },
    /// Clear the reply-later flag on a message
    Remove { message_id: String },
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
pub enum TriageSortArg {
    Date,
    Verdict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum TriageVerdictArg {
    Action,
    Fyi,
    Routine,
}

#[derive(Subcommand)]
pub enum InviteAction {
    /// Show the calendar invite attached to one message
    Show {
        message_id: String,
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Reply to a calendar invite
    Reply {
        message_id: String,
        #[arg(long)]
        account: Option<String>,
        #[arg(value_enum)]
        action: InviteReplyActionArg,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
}

#[derive(Subcommand)]
pub enum InvitesAction {
    /// List recent calendar invites
    List {
        #[arg(long, default_value_t = 50)]
        limit: u32,
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Backfill invite rows from already stored message bodies
    Backfill {
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum InviteReplyActionArg {
    Accept,
    Tentative,
    Decline,
}

impl From<InviteReplyActionArg> for mxr_protocol::CalendarInviteActionData {
    fn from(value: InviteReplyActionArg) -> Self {
        match value {
            InviteReplyActionArg::Accept => Self::Accept,
            InviteReplyActionArg::Tentative => Self::Tentative,
            InviteReplyActionArg::Decline => Self::Decline,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, ValueEnum)]
pub enum ActivityTierArg {
    Ephemeral,
    Standard,
    Important,
}

#[derive(Debug, Clone, PartialEq, Eq, ValueEnum)]
pub enum ActivitySourceArg {
    Human,
    Tui,
    Cli,
    Script,
    Web,
    Daemon,
    Agent,
    Mcp,
}

#[derive(Debug, Clone, PartialEq, Eq, ValueEnum)]
pub enum ActivityGroupByArg {
    Action,
    Day,
    Source,
    TargetKind,
    Hour,
}

#[derive(Debug, Clone, PartialEq, Eq, ValueEnum)]
pub enum ActivityExportFormatArg {
    Csv,
    Json,
    Ndjson,
}

#[derive(Debug, Clone, PartialEq, Eq, ValueEnum)]
pub enum ActivityClearWindow {
    #[value(name = "1h")]
    LastHour,
    #[value(name = "1d")]
    LastDay,
    #[value(name = "7d")]
    LastWeek,
    #[value(name = "30d")]
    LastMonth,
    All,
}

/// Shared filter args for activity read/redact subcommands.
#[derive(Debug, Clone, clap::Args)]
pub struct ActivityFilterArgs {
    /// Relative duration (e.g. `1h`, `3d`, `2w`) or ISO date inclusive lower bound.
    #[arg(long)]
    pub since: Option<String>,
    /// Relative duration or ISO date exclusive upper bound. Defaults to now.
    #[arg(long)]
    pub until: Option<String>,
    /// Originating client. Repeatable.
    #[arg(long, value_enum)]
    pub source: Vec<ActivitySourceArg>,
    /// Exact action token (e.g. `mail.archive`). Repeatable.
    #[arg(long)]
    pub action: Vec<String>,
    /// Match all actions starting with this prefix (e.g. `mail.`).
    #[arg(long)]
    pub prefix: Option<String>,
    /// Filter by target kind (`thread`, `message`, `draft`, `search`, ...).
    #[arg(long)]
    pub target_kind: Option<String>,
    /// Filter by exact target id.
    #[arg(long)]
    pub target_id: Option<String>,
    /// Retention tier. Repeatable.
    #[arg(long, value_enum)]
    pub tier: Vec<ActivityTierArg>,
    /// Filter by account id.
    #[arg(long)]
    pub account: Option<String>,
    /// FTS5 expression against context_json.
    #[arg(long)]
    pub query: Option<String>,
    /// Include tombstoned rows in the result.
    #[arg(long)]
    pub include_redacted: bool,
}

#[derive(Debug, Clone, Subcommand)]
pub enum ActivityAction {
    /// List recent activity in reverse chronological order.
    List {
        #[command(flatten)]
        filter: ActivityFilterArgs,
        /// Page size. Capped server-side to 500.
        #[arg(long, default_value_t = 50)]
        limit: u32,
        /// Resume from a previous cursor: `--cursor TS,ID`.
        #[arg(long)]
        cursor: Option<String>,
        #[arg(long, value_enum)]
        format: Option<OutputFormat>,
    },
    /// Group and count activity rows over a time window.
    Stats {
        #[command(flatten)]
        filter: ActivityFilterArgs,
        #[arg(long, value_enum, default_value_t = ActivityGroupByArg::Action)]
        group_by: ActivityGroupByArg,
        #[arg(long, value_enum)]
        format: Option<OutputFormat>,
    },
    /// Most-frequent actions in a window. Convenience over `stats --group-by action`.
    Top {
        #[command(flatten)]
        filter: ActivityFilterArgs,
        #[arg(long, default_value_t = 20)]
        limit: u32,
        #[arg(long, value_enum)]
        format: Option<OutputFormat>,
    },
    /// Export matching rows in CSV / JSON / NDJSON.
    Export {
        #[command(flatten)]
        filter: ActivityFilterArgs,
        #[arg(long, value_enum)]
        format: ActivityExportFormatArg,
        /// Write to this path instead of stdout.
        #[arg(long)]
        out: Option<String>,
    },
    /// Hard-delete rows older than `--before`. Destructive; confirms unless `--yes`.
    Prune {
        /// Relative duration (e.g. `90d`) or ISO date. Required.
        #[arg(long)]
        before: String,
        #[arg(long, value_enum)]
        tier: Option<ActivityTierArg>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        yes: bool,
    },
    /// Tombstone rows by id or filter. Destructive; confirms unless `--yes`.
    Redact {
        /// Comma-separated row ids.
        #[arg(long, num_args = 1.., value_delimiter = ',')]
        ids: Vec<i64>,
        #[command(flatten)]
        filter: ActivityFilterArgs,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        yes: bool,
    },
    /// Convenience tombstone over a recent time window. Browser-history style.
    Clear {
        #[arg(long = "last", value_enum)]
        window: ActivityClearWindow,
        /// Also tombstone important-tier rows (sends, redactions, etc.).
        #[arg(long)]
        include_important: bool,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        yes: bool,
    },
    /// Pause recording. With `--for DURATION`, auto-resumes after.
    Pause {
        #[arg(long = "for")]
        for_: Option<String>,
        #[arg(long)]
        quiet: bool,
    },
    /// Resume recording.
    Resume,
    /// Show current recorder status (paused state, retention defaults).
    Status {
        #[arg(long, value_enum)]
        format: Option<OutputFormat>,
    },
    /// Manage saved filter presets. Slug-keyed, like saved searches.
    Saved {
        #[command(subcommand)]
        action: ActivitySavedAction,
    },
    /// Follow new activity. Press Ctrl-C to stop.
    Tail {
        #[command(flatten)]
        filter: ActivityFilterArgs,
        /// Initial backfill — show this many recent rows before tailing.
        #[arg(short = 'n', long, default_value_t = 20)]
        lines: u32,
        /// Poll interval in seconds.
        #[arg(long, default_value_t = 2)]
        interval: u64,
        #[arg(long, value_enum)]
        format: Option<OutputFormat>,
    },
    /// Resolve a fuzzy time phrase ("yesterday afternoon", "last hour") and
    /// list activity from that window.
    Recall {
        /// Time phrase to resolve.
        phrase: String,
        #[arg(long, default_value_t = 50)]
        limit: u32,
        #[arg(long, value_enum)]
        format: Option<OutputFormat>,
    },
    /// Print a prose narrative of what you did in the window.
    Replay {
        #[arg(long, default_value = "1h")]
        since: String,
        #[arg(long, default_value_t = 200)]
        limit: u32,
        #[arg(long, value_enum)]
        format: Option<OutputFormat>,
    },
}

#[derive(Debug, Clone, Subcommand)]
#[expect(
    clippy::large_enum_variant,
    reason = "clap keeps subcommand argument fields inline to preserve the CLI parser shape"
)]
pub enum ActivitySavedAction {
    /// List all saved filter presets.
    List {
        #[arg(long, value_enum)]
        format: Option<OutputFormat>,
    },
    /// Save the current filter under a slug.
    Save {
        slug: String,
        #[arg(long)]
        name: String,
        #[command(flatten)]
        filter: ActivityFilterArgs,
    },
    /// Delete a preset by slug.
    Delete { slug: String },
    /// Apply a preset and list the matching rows.
    Open {
        slug: String,
        #[arg(long, default_value_t = 50)]
        limit: u32,
        #[arg(long, value_enum)]
        format: Option<OutputFormat>,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum DecisionsAction {
    /// Re-extract decisions from every thread within --since N days.
    /// Idempotent on unchanged thread content.
    Rebuild {
        #[arg(long)]
        account: Option<String>,
        #[arg(long = "since", default_value_t = 180)]
        since_days: u32,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// Show a single decision row by its id (returned by `mxr decisions
    /// --format json|ids`). Exits non-zero when the id is unknown so
    /// scripts can branch on presence.
    Show {
        id: String,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum BriefingAction {
    Thread {
        thread_id: String,
        #[arg(long)]
        refresh: bool,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    Recipient {
        email: String,
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        refresh: bool,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum CadenceAction {
    /// Add a contact to the watchlist.
    Watch {
        email: String,
        #[arg(long)]
        account: Option<String>,
        #[arg(long = "expected-days")]
        expected_days: Option<f64>,
        /// Expected cadence as a duration, e.g. `14d`, `2w`, or `30 days`.
        #[arg(long = "every")]
        every: Option<String>,
        #[arg(long)]
        note: Option<String>,
        /// Watch the contact even if it looks like a list sender.
        #[arg(long)]
        allow_list_sender: bool,
    },
    /// Remove a contact from the watchlist.
    Unwatch {
        email: String,
        #[arg(long)]
        account: Option<String>,
    },
    /// List currently watched contacts.
    List {
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
    /// List watched contacts whose interval has drifted past expected.
    Drift {
        #[arg(long)]
        account: Option<String>,
        #[arg(long)]
        format: Option<OutputFormat>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ArchiveAskModeArg {
    Hybrid,
    Lexical,
    Semantic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[clap(rename_all = "lowercase")]
pub enum StorageGroupByArg {
    Sender,
    Mimetype,
    Label,
    /// Per-message ranking: returns the single biggest emails with their
    /// IDs, so the output can drive `mxr search`/`mxr trash`/etc directly.
    Message,
}

impl StorageGroupByArg {
    pub fn as_core(self) -> Option<mxr_core::types::StorageGroupBy> {
        match self {
            Self::Sender => Some(mxr_core::types::StorageGroupBy::Sender),
            Self::Mimetype => Some(mxr_core::types::StorageGroupBy::Mimetype),
            Self::Label => Some(mxr_core::types::StorageGroupBy::Label),
            Self::Message => None,
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
                account: None,
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
                action,
                host,
                port,
                print_url,
                no_open,
                auto_port,
                strict_port,
                remote_host,
                foreground,
                detached_child,
            }) => {
                assert!(action.is_none());
                assert_eq!(host, "127.0.0.1");
                assert_eq!(port, 4321);
                assert!(print_url);
                assert!(!no_open);
                assert!(!auto_port);
                assert!(!strict_port);
                assert!(remote_host.is_none());
                assert!(!foreground);
                assert!(!detached_child);
            }
            other => panic!("unexpected parse result: {:?}", other.map(|_| "command")),
        }
    }

    #[test]
    fn parses_web_subcommand_with_remote_host() {
        let cli = Cli::parse_from([
            "mxr",
            "web",
            "--remote-host",
            "mxr.example.com:443",
            "--no-open",
        ]);
        match cli.command {
            Some(Command::Web {
                remote_host,
                no_open,
                ..
            }) => {
                assert_eq!(remote_host.as_deref(), Some("mxr.example.com:443"));
                assert!(no_open);
            }
            other => panic!("unexpected parse result: {:?}", other.map(|_| "command")),
        }
    }

    #[test]
    fn parses_web_stop_subcommand() {
        let cli = Cli::parse_from(["mxr", "web", "stop"]);
        match cli.command {
            Some(Command::Web { action, .. }) => {
                assert_eq!(action, Some(WebAction::Stop));
            }
            other => panic!("unexpected parse result: {:?}", other.map(|_| "command")),
        }
    }

    #[test]
    fn parses_web_foreground_flag() {
        let cli = Cli::parse_from(["mxr", "web", "--foreground"]);
        match cli.command {
            Some(Command::Web { foreground, .. }) => {
                assert!(foreground);
            }
            other => panic!("unexpected parse result: {:?}", other.map(|_| "command")),
        }
    }

    #[test]
    fn parses_web_subcommand_with_strict_port() {
        let cli = Cli::parse_from(["mxr", "web", "--port", "9999", "--strict-port"]);
        match cli.command {
            Some(Command::Web {
                port, strict_port, ..
            }) => {
                assert_eq!(port, 9999);
                assert!(strict_port);
            }
            other => panic!("unexpected parse result: {:?}", other.map(|_| "command")),
        }
    }

    #[test]
    fn parses_web_subcommand_with_auto_port() {
        let cli = Cli::parse_from(["mxr", "web", "--port", "9999", "--auto-port"]);
        match cli.command {
            Some(Command::Web {
                port, auto_port, ..
            }) => {
                assert_eq!(port, 9999);
                assert!(auto_port);
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
    fn parses_accounts_reauth_subcommand() {
        let cli = Cli::parse_from(["mxr", "accounts", "reauth", "personal"]);
        match cli.command {
            Some(Command::Accounts {
                action: Some(AccountsAction::Reauth { name }),
                ..
            }) => assert_eq!(name, "personal"),
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

    #[test]
    fn parses_count_quiet_flag() {
        let cli = Cli::parse_from(["mxr", "count", "is:unread", "--quiet"]);
        match cli.command {
            Some(Command::Count { query, quiet, .. }) => {
                assert_eq!(query, "is:unread");
                assert!(quiet);
            }
            other => panic!("unexpected parse result: {:?}", other.map(|_| "command")),
        }
    }

    #[test]
    fn message_mutations_accept_dry_run_format_and_stdin_mode() {
        let cases: &[&[&str]] = &[
            &["mxr", "archive", "--dry-run", "--format", "json"],
            &["mxr", "read-archive", "--dry-run", "--format", "json"],
            &["mxr", "trash", "--dry-run", "--format", "json"],
            &["mxr", "spam", "--dry-run", "--format", "json"],
            &["mxr", "star", "--dry-run", "--format", "json"],
            &["mxr", "unstar", "--dry-run", "--format", "json"],
            &["mxr", "read", "--dry-run", "--format", "json"],
            &["mxr", "unread", "--dry-run", "--format", "json"],
            &["mxr", "label", "FollowUp", "--dry-run", "--format", "json"],
            &[
                "mxr",
                "unlabel",
                "FollowUp",
                "--dry-run",
                "--format",
                "json",
            ],
            &["mxr", "move", "Done", "--dry-run", "--format", "json"],
            &[
                "mxr",
                "route",
                "--to",
                "Follow Up",
                "--from-queue",
                "Notto",
                "--archive",
                "--dry-run",
                "--format",
                "json",
            ],
            &[
                "mxr",
                "snooze",
                "--until",
                "tomorrow",
                "--dry-run",
                "--format",
                "json",
            ],
            &["mxr", "unsnooze", "--dry-run", "--format", "json"],
            &["mxr", "unsubscribe", "--dry-run", "--format", "json"],
        ];

        for args in cases {
            let cli = Cli::try_parse_from(*args)
                .unwrap_or_else(|error| panic!("{args:?} should parse: {error}"));
            match cli.command {
                Some(
                    Command::Archive {
                        message_ids,
                        dry_run,
                        format,
                        ..
                    }
                    | Command::ReadArchive {
                        message_ids,
                        dry_run,
                        format,
                        ..
                    }
                    | Command::Trash {
                        message_ids,
                        dry_run,
                        format,
                        ..
                    }
                    | Command::Spam {
                        message_ids,
                        dry_run,
                        format,
                        ..
                    }
                    | Command::Star {
                        message_ids,
                        dry_run,
                        format,
                        ..
                    }
                    | Command::Unstar {
                        message_ids,
                        dry_run,
                        format,
                        ..
                    }
                    | Command::MarkRead {
                        message_ids,
                        dry_run,
                        format,
                        ..
                    }
                    | Command::Unread {
                        message_ids,
                        dry_run,
                        format,
                        ..
                    }
                    | Command::Label {
                        message_ids,
                        dry_run,
                        format,
                        ..
                    }
                    | Command::Unlabel {
                        message_ids,
                        dry_run,
                        format,
                        ..
                    }
                    | Command::MoveMsg {
                        message_ids,
                        dry_run,
                        format,
                        ..
                    }
                    | Command::Route {
                        message_ids,
                        dry_run,
                        format,
                        ..
                    }
                    | Command::Snooze {
                        message_ids,
                        dry_run,
                        format,
                        ..
                    }
                    | Command::Unsnooze {
                        message_ids,
                        dry_run,
                        format,
                        ..
                    }
                    | Command::Unsubscribe {
                        message_ids,
                        dry_run,
                        format,
                        ..
                    },
                ) => {
                    assert!(message_ids.is_empty(), "{args:?} should allow stdin IDs");
                    assert!(dry_run, "{args:?} should set dry_run");
                    assert_eq!(
                        format,
                        Some(OutputFormat::Json),
                        "{args:?} should parse JSON"
                    );
                }
                other => panic!(
                    "unexpected command for {args:?}: {:?}",
                    other.map(|_| "command")
                ),
            }
        }
    }

    #[test]
    fn message_mutations_accept_multiple_positional_ids() {
        let id1 = uuid::Uuid::now_v7().to_string();
        let id2 = uuid::Uuid::now_v7().to_string();
        let cli = Cli::parse_from(["mxr", "archive", &id1, &id2, "--dry-run", "--format", "ids"]);

        match cli.command {
            Some(Command::Archive {
                message_ids,
                dry_run,
                format,
                ..
            }) => {
                assert_eq!(message_ids, vec![id1, id2]);
                assert!(dry_run);
                assert_eq!(format, Some(OutputFormat::Ids));
            }
            other => panic!("unexpected parse result: {:?}", other.map(|_| "command")),
        }
    }

    #[test]
    fn undo_accepts_dry_run_and_format() {
        let cli = Cli::parse_from(["mxr", "undo", "mut_123", "--dry-run", "--format", "json"]);
        match cli.command {
            Some(Command::Undo {
                mutation_id,
                dry_run,
                format,
            }) => {
                assert_eq!(mutation_id, "mut_123");
                assert!(dry_run);
                assert_eq!(format, Some(OutputFormat::Json));
            }
            other => panic!("unexpected parse result: {:?}", other.map(|_| "command")),
        }
    }
}
