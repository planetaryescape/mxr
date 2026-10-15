use clap::Subcommand;

/// `mxr assistant`: talk to the in-app assistant about your mail.
#[derive(Debug, Subcommand)]
pub enum AssistantAction {
    /// Whether the assistant can run, and on which model
    Status,
    /// Send a message, starting a conversation unless --conversation is
    /// given, and wait for the reply. Changes the assistant wants to make,
    /// and questions it asks, are printed with the command that answers them.
    Send {
        message: String,
        /// Continue this conversation.
        #[arg(long)]
        conversation: Option<String>,
        /// The thread you mean by "this thread".
        #[arg(long)]
        thread: Option<String>,
        /// Messages you mean by "these" (repeatable).
        #[arg(long = "message")]
        messages: Vec<String>,
        /// Return as soon as the turn starts.
        #[arg(long)]
        no_wait: bool,
    },
    /// Approve a change the assistant is waiting on. It runs exactly the
    /// call shown.
    Approve {
        conversation: String,
        call_id: String,
        #[arg(long)]
        no_wait: bool,
    },
    /// Decline a change or question; nothing runs.
    Reject {
        conversation: String,
        call_id: String,
        /// Tell the assistant why, or what to do instead.
        #[arg(long)]
        reason: Option<String>,
        #[arg(long)]
        no_wait: bool,
    },
    /// Answer a question the assistant asked. For one question, use
    /// --choose and/or --other; for several, --answers-json.
    Answer {
        conversation: String,
        call_id: String,
        /// An option's label (repeatable for multi-select questions).
        #[arg(long = "choose")]
        choose: Vec<String>,
        /// Your own answer.
        #[arg(long)]
        other: Option<String>,
        /// One answer per question: [{"selected": ["Work"], "other": null}, ...]
        #[arg(long, conflicts_with_all = ["choose", "other"])]
        answers_json: Option<String>,
        #[arg(long)]
        no_wait: bool,
    },
    /// Let reversible changes (archive, move, star, read, labels) run
    /// without asking in one conversation. Sending, deleting, trash, spam
    /// and rules always ask.
    Trust {
        conversation: String,
        /// Ask before every change again.
        #[arg(long)]
        off: bool,
    },
    /// Stop a running turn, or close what is waiting without running it.
    Stop { conversation: String },
    /// Conversations, most recently active first
    List {
        #[arg(long)]
        limit: Option<u32>,
    },
    /// Show a whole conversation
    Show { conversation: String },
    /// Delete a conversation and its transcript
    Delete {
        conversation: String,
        /// Show what would be deleted, without deleting it
        #[arg(long)]
        dry_run: bool,
    },
}
