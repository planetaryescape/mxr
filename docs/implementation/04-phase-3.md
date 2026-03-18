# 04 — Phase 3: Export + Rules + Polish

## Goal

mxr becomes a productivity platform, not just a client. Thread export enables AI workflows and sharing. The rules engine automates inbox organization. Shell hooks provide an escape hatch for custom automation. Multi-account support, performance optimization, and error UX improvements round out the release.

## Prerequisites

Phase 2 complete:
- Full read-write email client: compose, reply, forward, archive, trash, star, mark read/unread, label, search
- Reader mode pipeline working (HTML-to-text, signature stripping, quote collapsing, boilerplate removal)
- Snooze with wake loop in daemon
- Unsubscribe (one-click, mailto, browser)
- SMTP and Gmail API send paths
- Keybinding configuration (`keys.toml`)
- Config file parsing with XDG paths
- Tantivy search index with query parser
- SQLite store with all tables (envelopes, bodies, labels, attachments, drafts, snoozed)
- TUI with three-pane layout, thread view, command palette, multi-select

---

## Step 1: mxr-export Crate

Thread export in four formats: Markdown, JSON, Mbox (RFC 4155), and LLM Context. The LLM Context format reuses the reader mode pipeline from Phase 2.

### 1.1 Crate Setup

Add to workspace `Cargo.toml`:

```toml
[workspace]
members = [
    # ... existing members
    "crates/export",
]

[workspace.dependencies]
# ... existing deps
mxr-export = { path = "crates/export" }
```

`crates/export/Cargo.toml`:
```toml
[package]
name = "mxr-export"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
mxr-core = { workspace = true }
mxr-reader = { workspace = true }
chrono = { workspace = true }
serde = { workspace = true, features = ["derive"] }
serde_json = { workspace = true }
tracing = { workspace = true }

[dev-dependencies]
pretty_assertions = "1"
```

### 1.2 Core Types

`crates/export/src/lib.rs`:
```rust
mod markdown;
mod json;
mod mbox;
mod llm;

pub use crate::markdown::export_markdown;
pub use crate::json::export_json;
pub use crate::mbox::export_mbox;
pub use crate::llm::export_llm_context;

use chrono::{DateTime, Utc};

/// Format selection for thread export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Markdown,
    Json,
    Mbox,
    LlmContext,
}

impl ExportFormat {
    pub fn from_str_arg(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "markdown" | "md" => Some(Self::Markdown),
            "json" => Some(Self::Json),
            "mbox" => Some(Self::Mbox),
            "llm" => Some(Self::LlmContext),
            _ => None,
        }
    }
}

/// Input data for export. The caller (daemon/CLI) assembles this from store queries.
#[derive(Debug, Clone)]
pub struct ExportThread {
    pub thread_id: String,
    pub subject: String,
    pub messages: Vec<ExportMessage>,
}

#[derive(Debug, Clone)]
pub struct ExportMessage {
    pub id: String,
    pub from_name: Option<String>,
    pub from_email: String,
    pub to: Vec<String>,
    pub date: DateTime<Utc>,
    pub subject: String,
    pub body_text: Option<String>,
    pub body_html: Option<String>,
    pub headers_raw: Option<String>,
    pub attachments: Vec<ExportAttachment>,
}

#[derive(Debug, Clone)]
pub struct ExportAttachment {
    pub filename: String,
    pub size_bytes: u64,
    pub local_path: Option<String>,
}

/// Export a thread in the given format. Returns the exported string.
pub fn export(thread: &ExportThread, format: ExportFormat, reader_config: &mxr_reader::ReaderConfig) -> String {
    match format {
        ExportFormat::Markdown => export_markdown(thread),
        ExportFormat::Json => export_json(thread),
        ExportFormat::Mbox => export_mbox(thread),
        ExportFormat::LlmContext => export_llm_context(thread, reader_config),
    }
}
```

### 1.3 Markdown Exporter

`crates/export/src/markdown.rs`:
```rust
use crate::{ExportThread, ExportMessage};

pub fn export_markdown(thread: &ExportThread) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Thread: {}\n\n", thread.subject));

    for msg in &thread.messages {
        let sender = msg.from_name.as_deref().unwrap_or(&msg.from_email);
        let date = msg.date.format("%b %d, %Y %H:%M");
        out.push_str(&format!("## {} — {}\n\n", sender, date));

        if let Some(text) = &msg.body_text {
            out.push_str(text.trim());
        }
        out.push_str("\n\n");
    }

    // Footer
    let participants: Vec<&str> = thread.messages.iter()
        .map(|m| m.from_email.as_str())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    out.push_str(&format!(
        "---\nExported from mxr | {} messages | {} participants\n",
        thread.messages.len(),
        participants.len(),
    ));

    out
}
```

### 1.4 JSON Exporter

`crates/export/src/json.rs`:
```rust
use crate::{ExportThread, ExportMessage, ExportAttachment};
use serde::Serialize;

#[derive(Serialize)]
struct JsonThread {
    thread_id: String,
    subject: String,
    participants: Vec<String>,
    message_count: usize,
    messages: Vec<JsonMessage>,
}

#[derive(Serialize)]
struct JsonMessage {
    id: String,
    from: JsonAddress,
    date: String,
    body_text: Option<String>,
    attachments: Vec<JsonAttachment>,
}

#[derive(Serialize)]
struct JsonAddress {
    name: Option<String>,
    email: String,
}

#[derive(Serialize)]
struct JsonAttachment {
    filename: String,
    size_bytes: u64,
}

pub fn export_json(thread: &ExportThread) -> String {
    let participants: Vec<String> = thread.messages.iter()
        .map(|m| m.from_email.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let json_thread = JsonThread {
        thread_id: thread.thread_id.clone(),
        subject: thread.subject.clone(),
        message_count: thread.messages.len(),
        participants,
        messages: thread.messages.iter().map(|m| JsonMessage {
            id: m.id.clone(),
            from: JsonAddress {
                name: m.from_name.clone(),
                email: m.from_email.clone(),
            },
            date: m.date.to_rfc3339(),
            body_text: m.body_text.clone(),
            attachments: m.attachments.iter().map(|a| JsonAttachment {
                filename: a.filename.clone(),
                size_bytes: a.size_bytes,
            }).collect(),
        }).collect(),
    };

    serde_json::to_string_pretty(&json_thread).unwrap_or_default()
}
```

### 1.5 Mbox Exporter

`crates/export/src/mbox.rs`:
```rust
use crate::ExportThread;

/// Export thread as RFC 4155 mbox format.
/// Each message starts with "From " line followed by RFC 2822 headers + body.
pub fn export_mbox(thread: &ExportThread) -> String {
    let mut out = String::new();

    for msg in &thread.messages {
        // Mbox "From " line: From sender@email.com Tue Mar 17 09:45:00 2026
        let mbox_date = msg.date.format("%a %b %e %H:%M:%S %Y");
        out.push_str(&format!("From {} {}\n", msg.from_email, mbox_date));

        // Headers
        if let Some(raw) = &msg.headers_raw {
            out.push_str(raw);
        } else {
            // Reconstruct minimal headers
            out.push_str(&format!("From: {}\n", msg.from_email));
            out.push_str(&format!("Subject: {}\n", msg.subject));
            out.push_str(&format!("Date: {}\n", msg.date.to_rfc2822()));
            if !msg.to.is_empty() {
                out.push_str(&format!("To: {}\n", msg.to.join(", ")));
            }
        }
        out.push('\n');

        // Body (escape lines starting with "From " per mbox convention)
        if let Some(text) = &msg.body_text {
            for line in text.lines() {
                if line.starts_with("From ") {
                    out.push('>');
                }
                out.push_str(line);
                out.push('\n');
            }
        }
        out.push('\n');
    }

    out
}
```

### 1.6 LLM Context Exporter

`crates/export/src/llm.rs`:
```rust
use crate::{ExportThread, ExportMessage};
use mxr_reader::{ReaderConfig, clean};

/// Export thread optimized for AI consumption.
/// Uses the reader pipeline to strip noise, producing a token-efficient representation.
pub fn export_llm_context(thread: &ExportThread, reader_config: &ReaderConfig) -> String {
    let mut out = String::new();

    // Minimal header
    let participants: Vec<&str> = thread.messages.iter()
        .map(|m| m.from_email.as_str())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    out.push_str(&format!("Thread: {}\n", thread.subject));
    out.push_str(&format!("Participants: {}\n", participants.join(", ")));
    out.push_str(&format!("Messages: {}\n", thread.messages.len()));

    for msg in &thread.messages {
        out.push_str("\n---\n");

        let date = msg.date.format("%b %d %H:%M");
        out.push_str(&format!("[{}, {}]\n", msg.from_email, date));

        // Run reader pipeline for maximum noise reduction
        let reader_output = clean(
            msg.body_text.as_deref(),
            msg.body_html.as_deref(),
            reader_config,
        );
        out.push_str(&reader_output.content);
        out.push('\n');

        // Attachment metadata (no binary content)
        if !msg.attachments.is_empty() {
            let att_summary: Vec<String> = msg.attachments.iter()
                .map(|a| format!("{} ({}KB)", a.filename, a.size_bytes / 1024))
                .collect();
            out.push_str(&format!("\nAttachments: {}\n", att_summary.join(", ")));
        }
    }

    out
}
```

### 1.7 TUI Export Integration

**Files to modify**: `crates/tui/src/thread_view.rs`, `crates/tui/src/keybindings.rs`

Add `e` keybinding in thread view to show export format picker overlay:

```rust
/// Export format picker overlay shown on `e` in thread view.
pub enum ExportPickerAction {
    Markdown,
    Json,
    LlmContext,
    Mbox,
    Cancel,
}

fn handle_export_key(key: KeyEvent) -> Option<ExportPickerAction> {
    match key.code {
        KeyCode::Char('m') => Some(ExportPickerAction::Markdown),
        KeyCode::Char('j') => Some(ExportPickerAction::Json),
        KeyCode::Char('l') => Some(ExportPickerAction::LlmContext),
        KeyCode::Char('x') => Some(ExportPickerAction::Mbox),
        KeyCode::Esc => Some(ExportPickerAction::Cancel),
        _ => None,
    }
}
```

After format selection, the TUI:
1. Sends a `Command::ExportThread { thread_id, format }` to daemon
2. Daemon fetches thread data, calls `mxr_export::export()`
3. Result copied to clipboard via `arboard` crate (add to workspace dependencies)
4. If clipboard unavailable, save to `~/mxr/exports/{thread_id}.{ext}` and show path in status bar

### 1.8 CLI Export Command

**Files to modify**: `crates/cli/src/main.rs` (add `Export` variant to `Commands` enum)

```rust
/// Export a thread (or search results) in various formats.
///
/// Single thread: `mxr export THREAD_ID [--format markdown|json|mbox|llm] [--output ~/exports/]`
/// Bulk by search: `mxr export --search "query" --format mbox > archive.mbox`
#[derive(clap::Args)]
struct ExportArgs {
    /// Thread ID to export. Omit when using --search for bulk export.
    thread_id: Option<String>,
    /// Export format: markdown, json, mbox, llm
    #[arg(long, default_value = "markdown")]
    format: String,
    /// Save to file or directory instead of stdout.
    /// If a directory, filename is auto-generated: `{thread_id}.{ext}`
    #[arg(long, short)]
    output: Option<PathBuf>,
    /// Bulk export: export all threads matching search query.
    /// Concatenates results (especially useful with --format mbox).
    #[arg(long)]
    search: Option<String>,
}
```

Handler:
```rust
Commands::Export(args) => {
    let format = ExportFormat::from_str_arg(&args.format)
        .ok_or_else(|| anyhow::anyhow!(
            "Unknown format '{}'. Use: markdown, json, mbox, llm",
            args.format
        ))?;

    let client = connect_to_daemon().await?;

    // Bulk export via --search
    if let Some(query) = &args.search {
        let resp = client
            .send_command(Command::ExportSearch {
                query: query.clone(),
                format,
            })
            .await?;
        write_export_output(&resp.content, &args.output)?;
        return Ok(());
    }

    // Single thread export
    let thread_id = args.thread_id
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!(
            "Provide a THREAD_ID or use --search \"query\" for bulk export"
        ))?;

    let resp = client
        .send_command(Command::ExportThread {
            thread_id: thread_id.clone(),
            format,
        })
        .await?;

    write_export_output(&resp.content, &args.output)?;
    Ok(())
}

/// Write export content to stdout or file.
fn write_export_output(content: &str, output: &Option<PathBuf>) -> Result<()> {
    match output {
        Some(path) => {
            let target = if path.is_dir() {
                // Auto-generate filename in directory
                path.join("export")
            } else {
                path.clone()
            };
            std::fs::write(&target, content)?;
            eprintln!("Exported to {}", target.display());
            Ok(())
        }
        None => {
            print!("{}", content);
            Ok(())
        }
    }
}
```

### 1.9 IPC Protocol Extension

**Files to modify**: `crates/protocol/src/lib.rs`

```rust
// Add to Command enum
Command::ExportThread {
    thread_id: String,
    format: ExportFormat,
}
Command::ExportSearch {
    query: String,
    format: ExportFormat,
}

// Add to Response enum
Response::ExportResult {
    content: String,
}
```

### 1.10 Tests

**File**: `crates/export/src/tests.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn sample_thread() -> ExportThread {
        ExportThread {
            thread_id: "thread_abc".into(),
            subject: "Deployment rollback plan".into(),
            messages: vec![
                ExportMessage {
                    id: "msg_1".into(),
                    from_name: Some("Alice".into()),
                    from_email: "alice@example.com".into(),
                    to: vec!["team@example.com".into()],
                    date: Utc::now(),
                    subject: "Deployment rollback plan".into(),
                    body_text: Some("What's the rollback strategy?".into()),
                    body_html: None,
                    headers_raw: None,
                    attachments: vec![],
                },
                ExportMessage {
                    id: "msg_2".into(),
                    from_name: Some("Bob".into()),
                    from_email: "bob@example.com".into(),
                    to: vec!["team@example.com".into()],
                    date: Utc::now(),
                    subject: "Re: Deployment rollback plan".into(),
                    body_text: Some("Use blue-green deployment.".into()),
                    body_html: None,
                    headers_raw: None,
                    attachments: vec![],
                },
            ],
        }
    }

    #[test]
    fn markdown_has_thread_header() {
        let thread = sample_thread();
        let result = export_markdown(&thread);
        assert!(result.starts_with("# Thread: Deployment rollback plan"));
    }

    #[test]
    fn markdown_has_footer_with_counts() {
        let thread = sample_thread();
        let result = export_markdown(&thread);
        assert!(result.contains("2 messages"));
        assert!(result.contains("2 participants"));
    }

    #[test]
    fn json_is_valid() {
        let thread = sample_thread();
        let result = export_json(&thread);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["message_count"], 2);
    }

    #[test]
    fn mbox_starts_with_from_line() {
        let thread = sample_thread();
        let result = export_mbox(&thread);
        assert!(result.starts_with("From alice@example.com"));
    }

    #[test]
    fn mbox_escapes_from_in_body() {
        let mut thread = sample_thread();
        thread.messages[0].body_text = Some("From the beginning...".into());
        let result = export_mbox(&thread);
        assert!(result.contains(">From the beginning..."));
    }

    #[test]
    fn llm_context_minimal_metadata() {
        let thread = sample_thread();
        let config = mxr_reader::ReaderConfig::default();
        let result = export_llm_context(&thread, &config);
        assert!(result.starts_with("Thread: "));
        assert!(result.contains("Participants: "));
        // No full headers in LLM context
        assert!(!result.contains("Subject:"));
    }

    #[test]
    fn export_format_parsing() {
        assert_eq!(ExportFormat::from_str_arg("markdown"), Some(ExportFormat::Markdown));
        assert_eq!(ExportFormat::from_str_arg("md"), Some(ExportFormat::Markdown));
        assert_eq!(ExportFormat::from_str_arg("json"), Some(ExportFormat::Json));
        assert_eq!(ExportFormat::from_str_arg("llm"), Some(ExportFormat::LlmContext));
        assert_eq!(ExportFormat::from_str_arg("mbox"), Some(ExportFormat::Mbox));
        assert_eq!(ExportFormat::from_str_arg("nope"), None);
    }
}
```

**Dependencies**: `mxr-core`, `mxr-reader` (Phase 2)

---

## Step 2: mxr-rules Crate — Declarative Rules Engine

### 2.1 Crate Setup

Add to workspace `Cargo.toml`:

```toml
[workspace]
members = [
    # ... existing members
    "crates/rules",
]

[workspace.dependencies]
mxr-rules = { path = "crates/rules" }
glob-match = "0.2"
```

`crates/rules/Cargo.toml`:
```toml
[package]
name = "mxr-rules"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
mxr-core = { workspace = true }
chrono = { workspace = true }
regex = { workspace = true }
glob-match = { workspace = true }
serde = { workspace = true, features = ["derive"] }
tracing = { workspace = true }
uuid = { workspace = true, features = ["v7"] }

[dev-dependencies]
pretty_assertions = "1"
```

### 2.2 Core Types

`crates/rules/src/lib.rs`:
```rust
pub mod condition;
pub mod action;
pub mod engine;
pub mod history;

pub use condition::{Conditions, FieldCondition, StringMatch};
pub use action::{RuleAction, SnoozeDuration};
pub use engine::{RuleEngine, EvaluationResult, DryRunResult};
pub use history::RuleExecutionLog;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a rule.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RuleId(pub String);

impl RuleId {
    pub fn new() -> Self {
        Self(Uuid::now_v7().to_string())
    }
}

/// A declarative mail rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub id: RuleId,
    pub name: String,
    pub enabled: bool,
    /// Lower number = runs first.
    pub priority: i32,
    pub conditions: Conditions,
    pub actions: Vec<RuleAction>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

### 2.3 Conditions

`crates/rules/src/condition.rs`:
```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Composable condition tree. Evaluated recursively.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Conditions {
    And { conditions: Vec<Conditions> },
    Or { conditions: Vec<Conditions> },
    Not { condition: Box<Conditions> },
    Field(FieldCondition),
}

/// Leaf-level condition against a single message field.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "field", rename_all = "snake_case")]
pub enum FieldCondition {
    From { pattern: StringMatch },
    To { pattern: StringMatch },
    Subject { pattern: StringMatch },
    HasLabel { label: String },
    HasAttachment,
    SizeGreaterThan { bytes: u64 },
    SizeLessThan { bytes: u64 },
    DateAfter { date: DateTime<Utc> },
    DateBefore { date: DateTime<Utc> },
    IsUnread,
    IsStarred,
    HasUnsubscribe,
    BodyContains { pattern: StringMatch },
}

/// How to match a string field.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum StringMatch {
    Exact(String),
    Contains(String),
    Regex(String),
    Glob(String),
}

/// A message-like view for condition evaluation.
/// The engine evaluates conditions against this trait so it
/// doesn't depend on mxr-core's concrete Envelope type directly.
pub trait MessageView {
    fn from_email(&self) -> &str;
    fn to_emails(&self) -> &[String];
    fn subject(&self) -> &str;
    fn labels(&self) -> &[String];
    fn has_attachment(&self) -> bool;
    fn size_bytes(&self) -> u64;
    fn date(&self) -> DateTime<Utc>;
    fn is_unread(&self) -> bool;
    fn is_starred(&self) -> bool;
    fn has_unsubscribe(&self) -> bool;
    fn body_text(&self) -> Option<&str>;
}

impl StringMatch {
    /// Evaluate this match against a haystack string.
    pub fn matches(&self, haystack: &str) -> bool {
        match self {
            StringMatch::Exact(s) => haystack == s,
            StringMatch::Contains(s) => haystack.to_lowercase().contains(&s.to_lowercase()),
            StringMatch::Regex(pattern) => {
                regex::Regex::new(pattern)
                    .map(|re| re.is_match(haystack))
                    .unwrap_or(false)
            }
            StringMatch::Glob(pattern) => {
                glob_match::glob_match(pattern, haystack)
            }
        }
    }
}

impl Conditions {
    /// Recursively evaluate the condition tree against a message.
    pub fn evaluate(&self, msg: &dyn MessageView) -> bool {
        match self {
            Conditions::And { conditions } => conditions.iter().all(|c| c.evaluate(msg)),
            Conditions::Or { conditions } => conditions.iter().any(|c| c.evaluate(msg)),
            Conditions::Not { condition } => !condition.evaluate(msg),
            Conditions::Field(field) => field.evaluate(msg),
        }
    }
}

impl FieldCondition {
    pub fn evaluate(&self, msg: &dyn MessageView) -> bool {
        match self {
            FieldCondition::From { pattern } => pattern.matches(msg.from_email()),
            FieldCondition::To { pattern } => {
                msg.to_emails().iter().any(|e| pattern.matches(e))
            }
            FieldCondition::Subject { pattern } => pattern.matches(msg.subject()),
            FieldCondition::HasLabel { label } => {
                msg.labels().iter().any(|l| l == label)
            }
            FieldCondition::HasAttachment => msg.has_attachment(),
            FieldCondition::SizeGreaterThan { bytes } => msg.size_bytes() > *bytes,
            FieldCondition::SizeLessThan { bytes } => msg.size_bytes() < *bytes,
            FieldCondition::DateAfter { date } => msg.date() > *date,
            FieldCondition::DateBefore { date } => msg.date() < *date,
            FieldCondition::IsUnread => msg.is_unread(),
            FieldCondition::IsStarred => msg.is_starred(),
            FieldCondition::HasUnsubscribe => msg.has_unsubscribe(),
            FieldCondition::BodyContains { pattern } => {
                msg.body_text().map_or(false, |body| pattern.matches(body))
            }
        }
    }
}
```

### 2.4 Actions

`crates/rules/src/action.rs`:
```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Actions a rule can perform on a matching message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuleAction {
    AddLabel { label: String },
    RemoveLabel { label: String },
    Archive,
    Trash,
    Star,
    MarkRead,
    MarkUnread,
    Snooze { duration: SnoozeDuration },
    /// Run external command with message JSON on stdin.
    ShellHook { command: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SnoozeDuration {
    Hours { count: u32 },
    Days { count: u32 },
    Until { date: DateTime<Utc> },
}
```

### 2.5 Rule Engine

`crates/rules/src/engine.rs`:
```rust
use crate::{Rule, RuleAction, RuleId};
use crate::condition::MessageView;
use crate::history::{RuleExecutionLog, RuleMatchEntry};
use chrono::Utc;

/// The rule engine: evaluates rules against messages.
pub struct RuleEngine {
    rules: Vec<Rule>,
}

/// Result of evaluating all rules against a single message.
#[derive(Debug, Clone)]
pub struct EvaluationResult {
    /// Message ID.
    pub message_id: String,
    /// Accumulated actions from all matching rules.
    pub actions: Vec<RuleAction>,
    /// Which rules matched.
    pub matched_rules: Vec<RuleId>,
}

/// Result of a dry-run evaluation.
#[derive(Debug, Clone)]
pub struct DryRunResult {
    pub rule_id: RuleId,
    pub rule_name: String,
    pub matches: Vec<DryRunMatch>,
}

#[derive(Debug, Clone)]
pub struct DryRunMatch {
    pub message_id: String,
    pub from: String,
    pub subject: String,
    pub actions: Vec<RuleAction>,
}

impl RuleEngine {
    pub fn new(mut rules: Vec<Rule>) -> Self {
        // Sort by priority (lower = first)
        rules.sort_by_key(|r| r.priority);
        Self { rules }
    }

    /// Evaluate all enabled rules against a message.
    /// Returns accumulated actions. Actions are NOT applied yet —
    /// the caller is responsible for executing them.
    pub fn evaluate(&self, msg: &dyn MessageView, message_id: &str) -> EvaluationResult {
        let mut actions = Vec::new();
        let mut matched_rules = Vec::new();

        for rule in &self.rules {
            if !rule.enabled {
                continue;
            }
            if rule.conditions.evaluate(msg) {
                tracing::debug!(
                    rule_name = %rule.name,
                    message_id = %message_id,
                    "Rule matched"
                );
                actions.extend(rule.actions.clone());
                matched_rules.push(rule.id.clone());
            }
        }

        EvaluationResult {
            message_id: message_id.to_string(),
            actions,
            matched_rules,
        }
    }

    /// Evaluate all enabled rules against a batch of messages.
    /// Returns evaluation results per message.
    pub fn evaluate_batch(
        &self,
        messages: &[(&dyn MessageView, &str)],
    ) -> Vec<EvaluationResult> {
        messages.iter()
            .map(|(msg, id)| self.evaluate(*msg, id))
            .filter(|r| !r.actions.is_empty())
            .collect()
    }

    /// Dry-run: evaluate a specific rule against messages without applying actions.
    pub fn dry_run(
        &self,
        rule_id: &RuleId,
        messages: &[(&dyn MessageView, &str, &str, &str)], // (msg, id, from, subject)
    ) -> Option<DryRunResult> {
        let rule = self.rules.iter().find(|r| &r.id == rule_id)?;

        let matches: Vec<DryRunMatch> = messages.iter()
            .filter(|(msg, _, _, _)| rule.conditions.evaluate(*msg))
            .map(|(_, id, from, subject)| DryRunMatch {
                message_id: id.to_string(),
                from: from.to_string(),
                subject: subject.to_string(),
                actions: rule.actions.clone(),
            })
            .collect();

        Some(DryRunResult {
            rule_id: rule.id.clone(),
            rule_name: rule.name.clone(),
            matches,
        })
    }
}
```

### 2.6 Rule Execution History

`crates/rules/src/history.rs`:
```rust
use crate::RuleId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A log entry for a rule execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleMatchEntry {
    pub rule_id: RuleId,
    pub rule_name: String,
    pub message_id: String,
    pub actions_applied: Vec<String>,
    pub timestamp: DateTime<Utc>,
    pub success: bool,
    pub error: Option<String>,
}

/// Persistent log of rule executions.
/// Stored in SQLite for auditability ("why was this archived?").
pub struct RuleExecutionLog;

impl RuleExecutionLog {
    /// Record a rule match.
    /// Caller passes the SQLite connection; this module just defines the insert.
    pub fn entry(
        rule_id: &RuleId,
        rule_name: &str,
        message_id: &str,
        actions: &[String],
        success: bool,
        error: Option<&str>,
    ) -> RuleMatchEntry {
        RuleMatchEntry {
            rule_id: rule_id.clone(),
            rule_name: rule_name.to_string(),
            message_id: message_id.to_string(),
            actions_applied: actions.to_vec(),
            timestamp: Utc::now(),
            success,
            error: error.map(String::from),
        }
    }
}
```

### 2.7 SQLite Schema for Rules History

**Files to modify**: `crates/store/src/migrations/`

Add migration:

```sql
-- Rule execution history for auditability
CREATE TABLE IF NOT EXISTS rule_execution_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    rule_id TEXT NOT NULL,
    rule_name TEXT NOT NULL,
    message_id TEXT NOT NULL,
    actions_applied TEXT NOT NULL,  -- JSON array of action descriptions
    timestamp TEXT NOT NULL,        -- ISO 8601
    success INTEGER NOT NULL DEFAULT 1,
    error TEXT,
    FOREIGN KEY (message_id) REFERENCES messages(id)
);

CREATE INDEX idx_rule_log_rule_id ON rule_execution_log(rule_id);
CREATE INDEX idx_rule_log_message_id ON rule_execution_log(message_id);
CREATE INDEX idx_rule_log_timestamp ON rule_execution_log(timestamp);
```

### 2.8 TOML Rule Deserialization

**Files to modify**: `crates/cli/src/config.rs` or equivalent config parsing module

The `[[rules]]` sections in `config.toml` map directly to `Vec<Rule>` via serde:

```rust
use mxr_rules::{Rule, RuleId, Conditions, RuleAction};

/// Top-level config struct gains a rules field.
#[derive(Deserialize)]
pub struct Config {
    // ... existing fields
    #[serde(default)]
    pub rules: Vec<RuleConfig>,
}

/// TOML-friendly representation of a rule.
/// Deserialized then converted to mxr_rules::Rule.
#[derive(Deserialize)]
pub struct RuleConfig {
    pub name: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_priority")]
    pub priority: i32,
    pub conditions: Conditions,
    pub actions: Vec<RuleAction>,
}

fn default_enabled() -> bool { true }
fn default_priority() -> i32 { 100 }

impl RuleConfig {
    pub fn into_rule(self) -> Rule {
        let now = chrono::Utc::now();
        Rule {
            id: RuleId::new(),
            name: self.name,
            enabled: self.enabled,
            priority: self.priority,
            conditions: self.conditions,
            actions: self.actions,
            created_at: now,
            updated_at: now,
        }
    }
}
```

### 2.9 Daemon Integration: Rule Evaluation on Sync

**Files to modify**: `crates/daemon/src/sync.rs`, `crates/daemon/src/handler.rs`

After sync fetches new messages, evaluate rules:

```rust
use mxr_rules::{RuleEngine, EvaluationResult, RuleAction};

/// Called after sync inserts new messages into the store.
async fn apply_rules_to_new_messages(
    engine: &RuleEngine,
    new_messages: &[impl MessageView],
    store: &Store,
    provider: &dyn MailSyncProvider,
) -> Result<()> {
    for msg in new_messages {
        let result = engine.evaluate(msg, msg.id());

        if result.actions.is_empty() {
            continue;
        }

        for action in &result.actions {
            match execute_action(action, msg, store, provider).await {
                Ok(()) => {
                    let entry = RuleExecutionLog::entry(
                        &result.matched_rules[0], // simplified; real impl tracks per-rule
                        "",
                        msg.id(),
                        &[format!("{:?}", action)],
                        true,
                        None,
                    );
                    store.insert_rule_log(&entry).await?;
                }
                Err(e) => {
                    tracing::error!(
                        message_id = %msg.id(),
                        action = ?action,
                        error = %e,
                        "Rule action failed"
                    );
                    let entry = RuleExecutionLog::entry(
                        &result.matched_rules[0],
                        "",
                        msg.id(),
                        &[format!("{:?}", action)],
                        false,
                        Some(&e.to_string()),
                    );
                    store.insert_rule_log(&entry).await?;
                }
            }
        }
    }
    Ok(())
}

async fn execute_action(
    action: &RuleAction,
    msg: &dyn MessageView,
    store: &Store,
    provider: &dyn MailSyncProvider,
) -> Result<()> {
    match action {
        RuleAction::AddLabel { label } => {
            provider.add_label(msg.id(), label).await?;
            store.add_label(msg.id(), label).await?;
        }
        RuleAction::RemoveLabel { label } => {
            provider.remove_label(msg.id(), label).await?;
            store.remove_label(msg.id(), label).await?;
        }
        RuleAction::Archive => {
            provider.remove_label(msg.id(), "INBOX").await?;
            store.remove_label(msg.id(), "INBOX").await?;
        }
        RuleAction::Trash => {
            provider.add_label(msg.id(), "TRASH").await?;
            store.add_label(msg.id(), "TRASH").await?;
        }
        RuleAction::Star => {
            provider.add_label(msg.id(), "STARRED").await?;
            store.set_starred(msg.id(), true).await?;
        }
        RuleAction::MarkRead => {
            provider.remove_label(msg.id(), "UNREAD").await?;
            store.set_unread(msg.id(), false).await?;
        }
        RuleAction::MarkUnread => {
            provider.add_label(msg.id(), "UNREAD").await?;
            store.set_unread(msg.id(), true).await?;
        }
        RuleAction::Snooze { duration } => {
            // Reuse snooze logic from Phase 2
            let wake_at = resolve_snooze_time(duration);
            store.insert_snooze(msg.id(), wake_at).await?;
            provider.remove_label(msg.id(), "INBOX").await?;
        }
        RuleAction::ShellHook { command } => {
            // Handled in Step 3
            execute_shell_hook(command, msg).await?;
        }
    }
    Ok(())
}
```

### 2.10 CLI: Rules Subcommands

**Files to modify**: `crates/cli/src/main.rs`

```rust
/// Rules management subcommands.
///
/// Full subcommand tree:
///   mxr rules                                          # List all rules (alias for `rules list`)
///   mxr rules list                                     # List all rules with status
///   mxr rules show RULE_ID                             # Show rule details (conditions, actions, history summary)
///   mxr rules add "name" --when "query" --then action  # Create a new rule
///   mxr rules edit RULE_ID                             # Open rule in $EDITOR (TOML format)
///   mxr rules enable RULE_ID                           # Enable a disabled rule
///   mxr rules disable RULE_ID                          # Disable a rule without deleting
///   mxr rules delete RULE_ID                           # Delete a rule (with confirmation)
///   mxr rules dry-run RULE_ID [--after DATE]           # Show what a rule would match
///   mxr rules dry-run --all                            # Dry-run all enabled rules
///   mxr rules history [RULE_ID]                        # Show execution history (last N matches)
#[derive(clap::Subcommand)]
enum RulesCommands {
    /// List all configured rules with enabled/disabled status.
    List,
    /// Show detailed information about a specific rule.
    Show {
        /// Rule ID or name.
        rule: String,
    },
    /// Add a new rule via CLI syntax.
    Add {
        /// Rule name.
        name: String,
        /// Condition expression: 'label:newsletters AND is:read'
        #[arg(long = "when")]
        condition: String,
        /// Action: archive, trash, star, mark-read, add-label:NAME, shell:COMMAND
        #[arg(long = "then")]
        action: String,
        /// Priority (lower = runs first). Default: 100.
        #[arg(long, default_value = "100")]
        priority: i32,
    },
    /// Open a rule in $EDITOR for modification (TOML format).
    Edit {
        /// Rule ID or name.
        rule: String,
    },
    /// Enable a previously disabled rule.
    Enable {
        /// Rule ID or name.
        rule: String,
    },
    /// Disable a rule without deleting it.
    Disable {
        /// Rule ID or name.
        rule: String,
    },
    /// Delete a rule permanently.
    Delete {
        /// Rule ID or name.
        rule: String,
        /// Skip confirmation prompt.
        #[arg(long)]
        yes: bool,
    },
    /// Dry-run a rule to see what it would match without applying actions.
    DryRun {
        /// Rule ID or name. Use --all for all rules.
        #[arg(required_unless_present = "all")]
        rule: Option<String>,
        /// Dry-run all enabled rules.
        #[arg(long)]
        all: bool,
        /// Only consider messages after this date (YYYY-MM-DD).
        #[arg(long)]
        after: Option<String>,
    },
    /// Show rule execution history (when rules fired and what they did).
    History {
        /// Rule ID or name. Omit to show history for all rules.
        rule: Option<String>,
        /// Limit number of entries. Default: 50.
        #[arg(long, default_value = "50")]
        limit: u32,
        /// Output format.
        #[arg(long, default_value = "table")]
        format: String,
    },
}
```

### 2.11 Tests

**File**: `crates/rules/src/tests.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::condition::*;
    use crate::action::*;
    use chrono::Utc;

    /// Test message for condition evaluation.
    struct TestMessage {
        from: String,
        to: Vec<String>,
        subject: String,
        labels: Vec<String>,
        has_attachment: bool,
        size: u64,
        date: chrono::DateTime<Utc>,
        is_unread: bool,
        is_starred: bool,
        has_unsub: bool,
        body: Option<String>,
    }

    impl MessageView for TestMessage {
        fn from_email(&self) -> &str { &self.from }
        fn to_emails(&self) -> &[String] { &self.to }
        fn subject(&self) -> &str { &self.subject }
        fn labels(&self) -> &[String] { &self.labels }
        fn has_attachment(&self) -> bool { self.has_attachment }
        fn size_bytes(&self) -> u64 { self.size }
        fn date(&self) -> chrono::DateTime<Utc> { self.date }
        fn is_unread(&self) -> bool { self.is_unread }
        fn is_starred(&self) -> bool { self.is_starred }
        fn has_unsubscribe(&self) -> bool { self.has_unsub }
        fn body_text(&self) -> Option<&str> { self.body.as_deref() }
    }

    fn newsletter_msg() -> TestMessage {
        TestMessage {
            from: "newsletter@substack.com".into(),
            to: vec!["user@example.com".into()],
            subject: "This Week in Rust #580".into(),
            labels: vec!["INBOX".into(), "newsletters".into()],
            has_attachment: false,
            size: 15000,
            date: Utc::now(),
            is_unread: false,
            is_starred: false,
            has_unsub: true,
            body: Some("Here's your weekly Rust digest...".into()),
        }
    }

    #[test]
    fn string_match_contains_case_insensitive() {
        let m = StringMatch::Contains("invoice".into());
        assert!(m.matches("Re: Invoice #2847"));
        assert!(m.matches("INVOICE attached"));
        assert!(!m.matches("Receipt attached"));
    }

    #[test]
    fn string_match_glob() {
        let m = StringMatch::Glob("*@substack.com".into());
        assert!(m.matches("newsletter@substack.com"));
        assert!(!m.matches("newsletter@gmail.com"));
    }

    #[test]
    fn string_match_regex() {
        let m = StringMatch::Regex(r"invoice\s*#\d+".into());
        assert!(m.matches("Re: Invoice #2847"));
        assert!(!m.matches("Receipt attached"));
    }

    #[test]
    fn and_condition() {
        let cond = Conditions::And {
            conditions: vec![
                Conditions::Field(FieldCondition::HasLabel { label: "newsletters".into() }),
                Conditions::Field(FieldCondition::HasUnsubscribe),
            ],
        };
        assert!(cond.evaluate(&newsletter_msg()));
    }

    #[test]
    fn not_condition() {
        let cond = Conditions::Not {
            condition: Box::new(Conditions::Field(FieldCondition::IsStarred)),
        };
        assert!(cond.evaluate(&newsletter_msg())); // not starred
    }

    #[test]
    fn or_condition() {
        let cond = Conditions::Or {
            conditions: vec![
                Conditions::Field(FieldCondition::IsStarred),
                Conditions::Field(FieldCondition::HasUnsubscribe),
            ],
        };
        assert!(cond.evaluate(&newsletter_msg())); // has unsub
    }

    #[test]
    fn engine_accumulates_actions() {
        let rules = vec![
            Rule {
                id: RuleId("r1".into()),
                name: "Archive newsletters".into(),
                enabled: true,
                priority: 10,
                conditions: Conditions::Field(FieldCondition::HasLabel { label: "newsletters".into() }),
                actions: vec![RuleAction::Archive],
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
            Rule {
                id: RuleId("r2".into()),
                name: "Mark read".into(),
                enabled: true,
                priority: 20,
                conditions: Conditions::Field(FieldCondition::HasUnsubscribe),
                actions: vec![RuleAction::MarkRead],
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
        ];
        let engine = RuleEngine::new(rules);
        let msg = newsletter_msg();
        let result = engine.evaluate(&msg, "msg_1");

        // Both rules match → both actions accumulated
        assert_eq!(result.actions.len(), 2);
        assert_eq!(result.matched_rules.len(), 2);
    }

    #[test]
    fn disabled_rules_skipped() {
        let rules = vec![
            Rule {
                id: RuleId("r1".into()),
                name: "Disabled rule".into(),
                enabled: false,
                priority: 1,
                conditions: Conditions::Field(FieldCondition::HasUnsubscribe),
                actions: vec![RuleAction::Archive],
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
        ];
        let engine = RuleEngine::new(rules);
        let msg = newsletter_msg();
        let result = engine.evaluate(&msg, "msg_1");
        assert!(result.actions.is_empty());
    }

    #[test]
    fn priority_ordering() {
        let rules = vec![
            Rule {
                id: RuleId("r_low".into()),
                name: "Low priority".into(),
                enabled: true,
                priority: 100,
                conditions: Conditions::Field(FieldCondition::HasUnsubscribe),
                actions: vec![RuleAction::MarkRead],
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
            Rule {
                id: RuleId("r_high".into()),
                name: "High priority".into(),
                enabled: true,
                priority: 1,
                conditions: Conditions::Field(FieldCondition::HasUnsubscribe),
                actions: vec![RuleAction::Archive],
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
        ];
        let engine = RuleEngine::new(rules);
        let msg = newsletter_msg();
        let result = engine.evaluate(&msg, "msg_1");

        // High priority rule (Archive) should be first
        assert!(matches!(result.actions[0], RuleAction::Archive));
        assert!(matches!(result.actions[1], RuleAction::MarkRead));
    }
}
```

**Dependencies**: `mxr-core`

---

## Step 3: Shell Hooks

Shell hooks are the escape hatch for automation the declarative rules engine can't handle natively. They pipe message JSON to an external command's stdin.

### 3.1 Shell Hook Executor

**File**: `crates/rules/src/shell_hook.rs`

```rust
use crate::condition::MessageView;
use serde::Serialize;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::timeout;

/// Default timeout for shell hooks.
const DEFAULT_HOOK_TIMEOUT: Duration = Duration::from_secs(30);

/// JSON payload piped to shell hook stdin.
#[derive(Serialize)]
pub struct ShellHookPayload {
    pub id: String,
    pub from: ShellHookAddress,
    pub subject: String,
    pub date: String,
    pub body_text: Option<String>,
    pub attachments: Vec<ShellHookAttachment>,
}

#[derive(Serialize)]
pub struct ShellHookAddress {
    pub name: Option<String>,
    pub email: String,
}

#[derive(Serialize)]
pub struct ShellHookAttachment {
    pub filename: String,
    pub size_bytes: u64,
    pub local_path: Option<String>,
}

/// Execute a shell hook command with message data on stdin.
///
/// Returns Ok(()) on exit code 0, Err on non-zero or timeout.
pub async fn execute_shell_hook(
    command: &str,
    payload: &ShellHookPayload,
    hook_timeout: Option<Duration>,
) -> Result<(), ShellHookError> {
    let timeout_dur = hook_timeout.unwrap_or(DEFAULT_HOOK_TIMEOUT);

    let json = serde_json::to_string(payload)
        .map_err(|e| ShellHookError::SerializationFailed(e.to_string()))?;

    // Parse command string — support shell-like quoting via sh -c
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| ShellHookError::SpawnFailed {
            command: command.to_string(),
            error: e.to_string(),
        })?;

    // Write JSON to stdin
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(json.as_bytes()).await
            .map_err(|e| ShellHookError::StdinWriteFailed(e.to_string()))?;
    }

    // Wait with timeout
    let result = timeout(timeout_dur, child.wait_with_output()).await
        .map_err(|_| ShellHookError::Timeout {
            command: command.to_string(),
            timeout: timeout_dur,
        })?
        .map_err(|e| ShellHookError::WaitFailed(e.to_string()))?;

    if result.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&result.stderr).to_string();
        Err(ShellHookError::NonZeroExit {
            command: command.to_string(),
            code: result.status.code(),
            stderr,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ShellHookError {
    #[error("Failed to serialize message to JSON: {0}")]
    SerializationFailed(String),
    #[error("Failed to spawn command '{command}': {error}")]
    SpawnFailed { command: String, error: String },
    #[error("Failed to write to command stdin: {0}")]
    StdinWriteFailed(String),
    #[error("Command '{command}' timed out after {timeout:?}")]
    Timeout { command: String, timeout: Duration },
    #[error("Failed to wait for command: {0}")]
    WaitFailed(String),
    #[error("Command '{command}' exited with code {code:?}: {stderr}")]
    NonZeroExit { command: String, code: Option<i32>, stderr: String },
}
```

### 3.2 Hook Timeout Configuration

**Files to modify**: `crates/cli/src/config.rs`

```rust
/// Add to [general] config section.
#[derive(Deserialize)]
pub struct GeneralConfig {
    // ... existing fields
    /// Timeout for shell hook commands in seconds. Default: 30.
    #[serde(default = "default_hook_timeout")]
    pub hook_timeout: u64,
}

fn default_hook_timeout() -> u64 { 30 }
```

### 3.3 Integration with Rule Action Executor

The `execute_action` function from Step 2.9 calls `execute_shell_hook` when the action is `ShellHook`. The message is converted to `ShellHookPayload` before passing to the hook.

**Files to modify**: `crates/daemon/src/handler.rs`

```rust
RuleAction::ShellHook { command } => {
    let payload = build_shell_hook_payload(msg, store).await?;
    let timeout = Duration::from_secs(config.general.hook_timeout);
    match shell_hook::execute_shell_hook(command, &payload, Some(timeout)).await {
        Ok(()) => {
            tracing::info!(command = %command, message_id = %msg.id(), "Shell hook succeeded");
        }
        Err(e) => {
            tracing::error!(command = %command, message_id = %msg.id(), error = %e, "Shell hook failed");
            return Err(e.into());
        }
    }
}
```

### 3.4 Tests

**File**: `crates/rules/src/shell_hook_tests.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn sample_payload() -> ShellHookPayload {
        ShellHookPayload {
            id: "msg_123".into(),
            from: ShellHookAddress {
                name: Some("Alice".into()),
                email: "alice@example.com".into(),
            },
            subject: "Invoice #2847".into(),
            date: "2026-03-17T10:30:00Z".into(),
            body_text: Some("Please find attached...".into()),
            attachments: vec![ShellHookAttachment {
                filename: "invoice.pdf".into(),
                size_bytes: 234567,
                local_path: Some("/tmp/mxr/invoice.pdf".into()),
            }],
        }
    }

    #[tokio::test]
    async fn hook_success_exit_zero() {
        let payload = sample_payload();
        let result = execute_shell_hook("cat > /dev/null", &payload, None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn hook_failure_exit_nonzero() {
        let payload = sample_payload();
        let result = execute_shell_hook("exit 1", &payload, None).await;
        assert!(matches!(result, Err(ShellHookError::NonZeroExit { .. })));
    }

    #[tokio::test]
    async fn hook_timeout() {
        let payload = sample_payload();
        let result = execute_shell_hook(
            "sleep 60",
            &payload,
            Some(Duration::from_millis(100)),
        ).await;
        assert!(matches!(result, Err(ShellHookError::Timeout { .. })));
    }

    #[tokio::test]
    async fn hook_receives_json_on_stdin() {
        let payload = sample_payload();
        // Pipe stdin through jq to verify it's valid JSON
        let result = execute_shell_hook(
            "python3 -c 'import sys, json; json.load(sys.stdin)'",
            &payload,
            None,
        ).await;
        assert!(result.is_ok());
    }
}
```

**Dependencies**: `mxr-rules` (Step 2), `tokio`, `serde_json`, `thiserror`

---

## Step 4: Multi-Account Support

### 4.1 Config Structure (Already Partially Defined)

The `config.toml` already supports `[accounts.NAME]` sections from Phase 1. Phase 3 activates multiple accounts simultaneously.

**Files to modify**: `crates/cli/src/config.rs`, `crates/daemon/src/sync.rs`, `crates/daemon/src/state.rs`

### 4.2 Account Types

**Files to modify**: `crates/core/src/types.rs`

```rust
/// A configured email account.
#[derive(Debug, Clone)]
pub struct Account {
    pub id: AccountId,
    pub name: String,
    pub email: String,
    pub sync_config: SyncConfig,
    pub send_config: SendConfig,
    pub is_default: bool,
}

/// Typed account identifier (matches the TOML key: "personal", "work", etc.)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AccountId(pub String);

impl std::fmt::Display for AccountId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
```

### 4.3 Per-Account Sync Loops

**Files to modify**: `crates/daemon/src/sync.rs`

```rust
use std::collections::HashMap;
use tokio::task::JoinHandle;

/// Manages sync loops for all configured accounts.
pub struct SyncManager {
    loops: HashMap<AccountId, JoinHandle<()>>,
}

impl SyncManager {
    pub fn new() -> Self {
        Self { loops: HashMap::new() }
    }

    /// Start sync loops for all configured accounts.
    pub async fn start_all(
        &mut self,
        accounts: &[Account],
        store: Store,
        rule_engine: RuleEngine,
    ) {
        for account in accounts {
            let store = store.clone();
            let engine = rule_engine.clone();
            let account = account.clone();

            let handle = tokio::spawn(async move {
                loop {
                    if let Err(e) = sync_account(&account, &store, &engine).await {
                        tracing::error!(
                            account = %account.id,
                            error = %e,
                            "Sync failed for account"
                        );
                    }
                    tokio::time::sleep(Duration::from_secs(account.sync_interval())).await;
                }
            });

            self.loops.insert(account.id.clone(), handle);
        }
    }

    /// Stop a specific account's sync loop.
    pub fn stop(&mut self, account_id: &AccountId) {
        if let Some(handle) = self.loops.remove(account_id) {
            handle.abort();
        }
    }
}
```

### 4.4 Label Namespacing

Labels stored in SQLite gain an `account_id` column to avoid collisions between accounts.

**Files to modify**: `crates/store/src/migrations/`

```sql
-- Add account_id to labels and messages tables
ALTER TABLE labels ADD COLUMN account_id TEXT NOT NULL DEFAULT 'default';
ALTER TABLE messages ADD COLUMN account_id TEXT NOT NULL DEFAULT 'default';

-- Update indexes
CREATE INDEX idx_labels_account ON labels(account_id);
CREATE INDEX idx_messages_account ON messages(account_id);
```

**Files to modify**: `crates/store/src/queries.rs`

All label and message queries gain an optional `account_id` filter:

```rust
/// Query messages, optionally filtered by account.
pub async fn list_messages(
    &self,
    account_id: Option<&AccountId>,
    label: Option<&str>,
    limit: u32,
) -> Result<Vec<Envelope>> {
    // If account_id is None, return messages from all accounts ("global view")
    // If Some, filter to that account
}
```

### 4.5 TUI Account Switcher

**Files to modify**: `crates/tui/src/sidebar.rs`, `crates/tui/src/app_state.rs`

```rust
/// Sidebar gains an accounts section above labels.
pub struct SidebarState {
    pub accounts: Vec<AccountEntry>,
    pub active_account: Option<AccountId>, // None = "All Accounts" view
    pub labels: Vec<LabelEntry>,
}

pub struct AccountEntry {
    pub id: AccountId,
    pub name: String,
    pub email: String,
    pub unread_count: u32,
}
```

The command palette also supports "Switch to Work", "Switch to Personal", "All Accounts" entries.

### 4.6 Default Account for Compose

When composing, the `from` field defaults to:
1. The active account (if viewing a specific account)
2. The `default_account` from config (if viewing "All Accounts")

**Files to modify**: `crates/tui/src/compose.rs`, `crates/compose/src/lib.rs`

### 4.7 Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_namespacing_prevents_collision() {
        // Two accounts can both have a "newsletters" label
        // Store queries with account filter return only that account's labels
    }

    #[test]
    fn global_view_shows_all_accounts() {
        // When active_account is None, messages from all accounts appear
    }

    #[test]
    fn compose_uses_active_account() {
        // If viewing "work" account, compose from = work email
    }

    #[test]
    fn compose_uses_default_when_global() {
        // If viewing "all", compose from = default_account email
    }
}
```

**Dependencies**: All existing crates gain `AccountId` awareness

---

## Step 5: HTML Rendering Config

External `html_command` support allows users to use `w3m`, `lynx`, or any tool for HTML-to-text conversion.

### 5.1 Implementation

This was already scaffolded in Phase 2's `mxr-reader` crate (`crates/reader/src/html.rs`). Phase 3 ensures the config path works end-to-end.

**Files to modify**: `crates/reader/src/html.rs`

The existing `run_external_command` function handles the piping. Phase 3 work:

1. **Config propagation**: Ensure `html_command` from `config.toml [render]` section reaches `ReaderConfig` throughout all call sites (TUI rendering, export LLM context, compose context blocks).

2. **Error handling**: If the external command is not found (e.g., `w3m` not installed), produce a clear error:

```rust
fn run_external_command(cmd: &str, input: &str) -> Result<String, HtmlRenderError> {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    let program = parts.first()
        .ok_or(HtmlRenderError::EmptyCommand)?;

    // Check if program exists before spawning
    if which::which(program).is_err() {
        return Err(HtmlRenderError::CommandNotFound {
            command: program.to_string(),
            suggestion: "Install it or remove html_command from config to use built-in renderer".into(),
        });
    }

    // ... existing spawn logic
}

#[derive(Debug, thiserror::Error)]
pub enum HtmlRenderError {
    #[error("html_command is empty")]
    EmptyCommand,
    #[error("Command '{command}' not found. {suggestion}")]
    CommandNotFound { command: String, suggestion: String },
    #[error("Command failed: {0}")]
    ExecutionFailed(#[from] std::io::Error),
}
```

3. **Fallback**: If external command fails, log a warning and fall back to built-in `html2text` (already implemented in Phase 2).

**New dependency**: `which` crate for checking command existence.

### 5.2 Tests

```rust
#[test]
fn fallback_to_builtin_when_command_missing() {
    let config = ReaderConfig {
        html_command: Some("nonexistent_command_xyz".into()),
        ..Default::default()
    };
    let result = to_plain_text("<p>Hello</p>", &config);
    assert!(result.contains("Hello")); // Falls back to built-in
}

#[test]
fn external_command_receives_html() {
    // Only run if w3m is available
    if which::which("w3m").is_ok() {
        let config = ReaderConfig {
            html_command: Some("w3m -T text/html -dump".into()),
            ..Default::default()
        };
        let result = to_plain_text("<p>Hello World</p>", &config);
        assert!(result.contains("Hello World"));
    }
}
```

**Dependencies**: `mxr-reader` (Phase 2), `which` crate

---

## Step 6: Tantivy Reindex

`mxr doctor --reindex` drops the Tantivy search index and rebuilds it from all data in SQLite.

### 6.1 Implementation

**Files to modify**: `crates/search/src/lib.rs`, `crates/cli/src/doctor.rs`

```rust
use tantivy::{Index, IndexWriter};
use std::path::Path;

/// Drop and rebuild the Tantivy index from SQLite data.
pub async fn reindex(
    index_path: &Path,
    store: &Store,
    progress: impl Fn(ReindexProgress),
) -> Result<()> {
    // 1. Count total messages for progress reporting
    let total = store.count_messages().await?;
    progress(ReindexProgress::Starting { total });

    // 2. Delete existing index
    if index_path.exists() {
        std::fs::remove_dir_all(index_path)?;
    }

    // 3. Create fresh index with schema
    let index = create_index(index_path)?;
    let mut writer = index.writer(50_000_000)?; // 50MB heap

    // 4. Stream all messages from SQLite and index in batches
    let batch_size = 500;
    let mut indexed = 0;

    let mut offset = 0;
    loop {
        let messages = store.fetch_messages_with_bodies(offset, batch_size).await?;
        if messages.is_empty() {
            break;
        }

        for msg in &messages {
            add_document_to_index(&mut writer, msg)?;
            indexed += 1;

            if indexed % 100 == 0 {
                progress(ReindexProgress::Indexing { indexed, total });
            }
        }

        // Commit every batch to avoid excessive memory use
        writer.commit()?;
        offset += batch_size;
    }

    // 5. Final commit
    writer.commit()?;
    progress(ReindexProgress::Complete { indexed });

    // 6. Verify
    let reader = index.reader()?;
    let searcher = reader.searcher();
    let doc_count = searcher.num_docs();
    if doc_count != indexed as u64 {
        tracing::warn!(
            expected = indexed,
            actual = doc_count,
            "Index document count mismatch after reindex"
        );
    }

    Ok(())
}

/// Progress callback data.
#[derive(Debug)]
pub enum ReindexProgress {
    Starting { total: usize },
    Indexing { indexed: usize, total: usize },
    Complete { indexed: usize },
}
```

### 6.2 CLI Integration

**Files to modify**: `crates/cli/src/doctor.rs`

```rust
if args.reindex {
    println!("Rebuilding search index from database...");

    reindex(&index_path, &store, |progress| {
        match progress {
            ReindexProgress::Starting { total } => {
                println!("Found {} messages to index.", total);
            }
            ReindexProgress::Indexing { indexed, total } => {
                print!("\rIndexing: {}/{} ({:.0}%)", indexed, total,
                    (indexed as f64 / total as f64) * 100.0);
                std::io::Write::flush(&mut std::io::stdout()).ok();
            }
            ReindexProgress::Complete { indexed } => {
                println!("\nReindex complete. {} messages indexed.", indexed);
            }
        }
    }).await?;
}
```

### 6.3 Tests

```rust
#[tokio::test]
async fn reindex_produces_searchable_index() {
    let dir = tempdir().unwrap();
    let store = create_test_store_with_messages(100).await;

    reindex(dir.path(), &store, |_| {}).await.unwrap();

    let index = Index::open_in_dir(dir.path()).unwrap();
    let reader = index.reader().unwrap();
    let searcher = reader.searcher();
    assert_eq!(searcher.num_docs(), 100);
}

#[tokio::test]
async fn reindex_replaces_existing_index() {
    let dir = tempdir().unwrap();
    let store = create_test_store_with_messages(50).await;

    // First index
    reindex(dir.path(), &store, |_| {}).await.unwrap();

    // Add more messages
    add_test_messages(&store, 50).await;

    // Reindex
    reindex(dir.path(), &store, |_| {}).await.unwrap();

    let index = Index::open_in_dir(dir.path()).unwrap();
    let reader = index.reader().unwrap();
    assert_eq!(reader.searcher().num_docs(), 100);
}
```

**Dependencies**: `mxr-search`, `mxr-store`, `tantivy`

---

## Step 7: Shell Completions

### 7.1 Implementation

**Files to modify**: `crates/cli/src/main.rs`

```rust
use clap::CommandFactory;
use clap_complete::{generate, Shell};

#[derive(clap::Subcommand)]
enum Commands {
    // ... existing commands

    /// Generate shell completions for bash, zsh, or fish.
    Completions {
        /// Shell to generate completions for.
        shell: Shell,
    },
}

// In the match handler:
Commands::Completions { shell } => {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    generate(shell, &mut cmd, name, &mut std::io::stdout());
    Ok(())
}
```

**New dependency**: `clap_complete` crate.

Add to workspace `Cargo.toml`:
```toml
[workspace.dependencies]
clap_complete = "4"
```

Add to `crates/cli/Cargo.toml`:
```toml
[dependencies]
clap_complete = { workspace = true }
```

### 7.2 Tests

```rust
#[test]
fn completions_generate_without_panic() {
    // Verify completions generation doesn't panic for each shell
    for shell in [Shell::Bash, Shell::Zsh, Shell::Fish] {
        let mut cmd = Cli::command();
        let mut buf = Vec::new();
        generate(shell, &mut cmd, "mxr", &mut buf);
        assert!(!buf.is_empty());
    }
}
```

**Dependencies**: `clap`, `clap_complete`

---

## Step 8: Performance Optimization

### 8.1 Virtual Scrolling in TUI

For large mailboxes (10k+ messages), the TUI must render only visible items, not the entire list.

**Files to modify**: `crates/tui/src/message_list.rs`

```rust
/// Virtual scrolling state for the message list.
pub struct VirtualList {
    /// Total number of items.
    pub total: usize,
    /// Index of the first visible item.
    pub viewport_start: usize,
    /// Number of items that fit in the viewport.
    pub viewport_size: usize,
    /// Currently selected index.
    pub selected: usize,
}

impl VirtualList {
    pub fn new(total: usize, viewport_size: usize) -> Self {
        Self {
            total,
            viewport_start: 0,
            viewport_size,
            selected: 0,
        }
    }

    /// Returns the range of items to fetch and render.
    pub fn visible_range(&self) -> std::ops::Range<usize> {
        let end = (self.viewport_start + self.viewport_size).min(self.total);
        self.viewport_start..end
    }

    /// Move selection down. Adjusts viewport if selection goes below visible area.
    pub fn move_down(&mut self) {
        if self.selected < self.total.saturating_sub(1) {
            self.selected += 1;
            if self.selected >= self.viewport_start + self.viewport_size {
                self.viewport_start = self.selected.saturating_sub(self.viewport_size - 1);
            }
        }
    }

    /// Move selection up. Adjusts viewport if selection goes above visible area.
    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            if self.selected < self.viewport_start {
                self.viewport_start = self.selected;
            }
        }
    }

    /// Page down.
    pub fn page_down(&mut self) {
        let jump = self.viewport_size / 2;
        self.selected = (self.selected + jump).min(self.total.saturating_sub(1));
        if self.selected >= self.viewport_start + self.viewport_size {
            self.viewport_start = self.selected.saturating_sub(self.viewport_size - 1);
        }
    }

    /// Jump to top.
    pub fn jump_top(&mut self) {
        self.selected = 0;
        self.viewport_start = 0;
    }

    /// Jump to bottom.
    pub fn jump_bottom(&mut self) {
        self.selected = self.total.saturating_sub(1);
        self.viewport_start = self.total.saturating_sub(self.viewport_size);
    }
}
```

The store query for the message list also needs windowed fetching:

```rust
/// Fetch messages for virtual scrolling — only the visible window.
pub async fn fetch_message_window(
    &self,
    account_id: Option<&AccountId>,
    label: Option<&str>,
    offset: usize,
    limit: usize,
) -> Result<Vec<Envelope>> {
    // SELECT ... ORDER BY date DESC LIMIT ?1 OFFSET ?2
}
```

### 8.2 Lazy Label Count Updates

**Files to modify**: `crates/daemon/src/sync.rs`, `crates/store/src/queries.rs`

Instead of recalculating label counts after every message mutation, batch updates:

```rust
/// Mark label counts as dirty. Recalculated on next UI tick or after batch completes.
pub struct LabelCountCache {
    counts: HashMap<String, u32>,
    dirty: bool,
}

impl LabelCountCache {
    pub fn invalidate(&mut self) {
        self.dirty = true;
    }

    /// Refresh counts from database if dirty.
    pub async fn refresh_if_dirty(&mut self, store: &Store) -> Result<()> {
        if self.dirty {
            self.counts = store.compute_label_counts().await?;
            self.dirty = false;
        }
        Ok(())
    }
}
```

### 8.3 SQLite Query Optimization

**Files to modify**: `crates/store/src/migrations/`

```sql
-- Analyze tables for query planner
ANALYZE;

-- Verify key indexes exist
CREATE INDEX IF NOT EXISTS idx_messages_date ON messages(date DESC);
CREATE INDEX IF NOT EXISTS idx_messages_thread_id ON messages(thread_id);
CREATE INDEX IF NOT EXISTS idx_messages_label_date ON message_labels(label, message_id);
CREATE INDEX IF NOT EXISTS idx_messages_account_label ON messages(account_id, date DESC);

-- Enable WAL mode for concurrent reads during sync
PRAGMA journal_mode = WAL;
```

### 8.4 Tantivy Commit Batching

**Files to modify**: `crates/search/src/lib.rs`

During sync, accumulate documents and commit in batches rather than per-message:

```rust
pub struct BatchIndexer {
    writer: IndexWriter,
    pending: usize,
    batch_size: usize,
}

impl BatchIndexer {
    pub fn new(writer: IndexWriter, batch_size: usize) -> Self {
        Self { writer, pending: 0, batch_size }
    }

    pub fn add(&mut self, doc: Document) -> Result<()> {
        self.writer.add_document(doc)?;
        self.pending += 1;

        if self.pending >= self.batch_size {
            self.writer.commit()?;
            self.pending = 0;
        }
        Ok(())
    }

    /// Commit any remaining pending documents.
    pub fn flush(&mut self) -> Result<()> {
        if self.pending > 0 {
            self.writer.commit()?;
            self.pending = 0;
        }
        Ok(())
    }
}
```

### 8.5 Profiling

Before optimizing, profile with real data:

1. Create a benchmark with `criterion`:
   - Load 10k messages from a fixture or test Gmail
   - Measure: initial render time, scroll latency, search query time, sync insertion rate
2. Use `tracing-subscriber` with timing spans for key operations
3. SQLite: run `EXPLAIN QUERY PLAN` on slow queries

**New dependency**: `criterion` (dev-dependency for benchmarks)

### 8.6 Tests

```rust
#[test]
fn virtual_list_scroll_within_bounds() {
    let mut list = VirtualList::new(10000, 50);
    list.move_down();
    assert_eq!(list.selected, 1);
    assert_eq!(list.viewport_start, 0);

    // Scroll past viewport
    for _ in 0..60 {
        list.move_down();
    }
    assert_eq!(list.selected, 61);
    assert!(list.viewport_start > 0);
    assert!(list.selected < list.viewport_start + list.viewport_size);
}

#[test]
fn virtual_list_jump_bottom() {
    let mut list = VirtualList::new(10000, 50);
    list.jump_bottom();
    assert_eq!(list.selected, 9999);
    assert_eq!(list.viewport_start, 9950);
}

#[test]
fn batch_indexer_commits_at_threshold() {
    // Verify that BatchIndexer commits every batch_size documents
    // and flush() commits remaining
}
```

**Dependencies**: All existing crates

---

## Step 9: Error UX Improvements

### 9.1 Context-Rich Error Messages

**Files to modify**: `crates/core/src/error.rs` (or create if needed)

Use `thiserror` with structured context throughout:

```rust
#[derive(Debug, thiserror::Error)]
pub enum MxrError {
    #[error("Authentication expired for account '{account}'. Run `mxr accounts add gmail` to re-authenticate.")]
    AuthExpired { account: String },

    #[error("Cannot connect to daemon. Is it running? Try `mxr daemon` to start it.")]
    DaemonNotRunning,

    #[error("Network error: {message}. Will retry in {retry_seconds}s.")]
    NetworkError { message: String, retry_seconds: u64 },

    #[error("Config error in {file} at {field}: {message}")]
    ConfigError { file: String, field: String, message: String },

    #[error("Gmail API error ({code}): {message}")]
    GmailApi { code: u16, message: String },

    #[error("Search index corrupted. Run `mxr doctor --reindex` to rebuild.")]
    IndexCorrupted,
}
```

### 9.2 Auth Expiry Detection

**Files to modify**: `crates/provider-gmail/src/auth.rs`, `crates/daemon/src/sync.rs`

```rust
/// Check if a Gmail API error indicates auth expiry.
fn is_auth_error(status: u16) -> bool {
    status == 401 || status == 403
}

/// Handle auth errors during sync.
async fn handle_sync_error(error: &ProviderError, account: &Account) {
    if let ProviderError::ApiError { status, .. } = error {
        if is_auth_error(*status) {
            tracing::error!(
                account = %account.id,
                "Authentication expired. User must re-authenticate."
            );
            // Notify TUI via IPC that account needs re-auth
            // TUI shows a banner: "Account 'work' needs re-authentication. Press Enter to start."
        }
    }
}
```

### 9.3 Network Failure Recovery

**Files to modify**: `crates/daemon/src/sync.rs`

```rust
use std::time::Duration;

/// Retry with exponential backoff.
pub async fn retry_with_backoff<F, Fut, T, E>(
    operation: F,
    max_retries: u32,
    base_delay: Duration,
) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut attempt = 0;
    loop {
        match operation().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                attempt += 1;
                if attempt >= max_retries {
                    tracing::error!(
                        attempts = attempt,
                        error = %e,
                        "Operation failed after max retries"
                    );
                    return Err(e);
                }
                let delay = base_delay * 2u32.pow(attempt - 1);
                tracing::warn!(
                    attempt = attempt,
                    next_retry_in = ?delay,
                    error = %e,
                    "Operation failed, retrying"
                );
                tokio::time::sleep(delay).await;
            }
        }
    }
}
```

### 9.4 Enhanced `mxr doctor`

**Files to modify**: `crates/cli/src/doctor.rs`

```rust
pub async fn run_doctor(args: &DoctorArgs) -> Result<()> {
    let mut issues = Vec::new();

    // 1. Config validation
    match load_config() {
        Ok(config) => println!("  Config: OK"),
        Err(e) => {
            println!("  Config: FAILED - {}", e);
            issues.push(format!("Fix config: {}", e));
        }
    }

    // 2. Database integrity
    match check_database_integrity(&store).await {
        Ok(()) => println!("  Database: OK"),
        Err(e) => {
            println!("  Database: ISSUE - {}", e);
            issues.push("Run `mxr doctor --reindex` to rebuild search index".into());
        }
    }

    // 3. Auth status per account
    for account in &config.accounts {
        match check_auth_status(account).await {
            Ok(()) => println!("  Auth ({}): OK", account.name),
            Err(e) => {
                println!("  Auth ({}): EXPIRED", account.name);
                issues.push(format!(
                    "Re-authenticate: `mxr accounts add gmail` for {}",
                    account.name
                ));
            }
        }
    }

    // 4. Search index health
    match check_search_index(&index_path).await {
        Ok(doc_count) => println!("  Search index: OK ({} documents)", doc_count),
        Err(e) => {
            println!("  Search index: CORRUPTED");
            issues.push("Run `mxr doctor --reindex`".into());
        }
    }

    // 5. Daemon status
    match check_daemon_running().await {
        Ok(()) => println!("  Daemon: RUNNING"),
        Err(_) => println!("  Daemon: NOT RUNNING (start with `mxr daemon`)"),
    }

    // 6. Last sync time
    if let Ok(last_sync) = store.last_sync_time().await {
        let ago = Utc::now() - last_sync;
        println!("  Last sync: {} ({} ago)", last_sync.format("%Y-%m-%d %H:%M"), humanize_duration(ago));
    }

    // Summary
    if issues.is_empty() {
        println!("\nAll checks passed.");
    } else {
        println!("\nSuggested fixes:");
        for (i, issue) in issues.iter().enumerate() {
            println!("  {}. {}", i + 1, issue);
        }
    }

    Ok(())
}
```

### 9.5 Tests

```rust
#[test]
fn auth_error_detection() {
    assert!(is_auth_error(401));
    assert!(is_auth_error(403));
    assert!(!is_auth_error(404));
    assert!(!is_auth_error(500));
}

#[tokio::test]
async fn retry_with_backoff_succeeds_on_second_attempt() {
    let attempt = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let attempt_clone = attempt.clone();

    let result = retry_with_backoff(
        || {
            let a = attempt_clone.clone();
            async move {
                let n = a.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if n == 0 {
                    Err("transient failure")
                } else {
                    Ok("success")
                }
            }
        },
        3,
        Duration::from_millis(1),
    ).await;

    assert_eq!(result, Ok("success"));
    assert_eq!(attempt.load(std::sync::atomic::Ordering::SeqCst), 2);
}

#[tokio::test]
async fn retry_with_backoff_exhausts_retries() {
    let result: Result<(), &str> = retry_with_backoff(
        || async { Err("persistent failure") },
        3,
        Duration::from_millis(1),
    ).await;

    assert!(result.is_err());
}
```

**Dependencies**: `thiserror`, `tokio`

---

## Step 10: CLI Surface Completion (A004)

Phase 3 adds the remaining CLI commands from A004 that don't exist in earlier phases: labels management, notify, events, and `--format ids` for search.

### 10.1 Labels Management CLI

**Files to modify**: `crates/cli/src/main.rs`, `crates/cli/src/labels.rs` (new)

```rust
/// Labels management subcommands.
///
///   mxr labels                              # List all labels with message counts
///   mxr labels create "name" [--color "#hex"]  # Create a new label
///   mxr labels delete "name"                # Delete a label (with confirmation)
///   mxr labels rename "old" "new"           # Rename a label
#[derive(clap::Subcommand)]
enum LabelsCommands {
    /// List all labels with message counts.
    List {
        /// Output format.
        #[arg(long, default_value = "table")]
        format: String,
        /// Filter to a specific account.
        #[arg(long)]
        account: Option<String>,
    },
    /// Create a new label.
    Create {
        /// Label name.
        name: String,
        /// Label color as hex (e.g. "#ff6600"). Provider support varies.
        #[arg(long)]
        color: Option<String>,
        /// Account to create the label in.
        #[arg(long)]
        account: Option<String>,
    },
    /// Delete a label.
    Delete {
        /// Label name.
        name: String,
        /// Skip confirmation prompt.
        #[arg(long)]
        yes: bool,
    },
    /// Rename a label.
    Rename {
        /// Current label name.
        old: String,
        /// New label name.
        new: String,
    },
}
```

IPC protocol additions:

```rust
Command::CreateLabel { name: String, color: Option<String>, account_id: Option<String> }
Command::DeleteLabel { name: String }
Command::RenameLabel { old: String, new: String }
```

### 10.2 Notify CLI (Status Bar Integration)

**Files to modify**: `crates/cli/src/main.rs`, `crates/cli/src/notify.rs` (new)

```rust
/// Unread summary for status bar integration (polybar, waybar, tmux, etc.)
///
///   mxr notify                           # One-line unread summary: "3 unread (1 personal, 2 work)"
///   mxr notify --format json             # {"total": 3, "accounts": [{"name": "personal", "unread": 1}, ...]}
///   mxr notify --watch                   # Continuous output on changes (for piping to notify-send)
#[derive(clap::Args)]
struct NotifyArgs {
    /// Output format: text, json
    #[arg(long, default_value = "text")]
    format: String,
    /// Continuous output mode — prints a new line whenever unread count changes.
    /// Designed for piping: `mxr notify --watch | while read line; do notify-send "mxr" "$line"; done`
    #[arg(long)]
    watch: bool,
}
```

Handler:

```rust
Commands::Notify(args) => {
    let client = connect_to_daemon().await?;

    if args.watch {
        // Subscribe to DaemonEvent::UnreadCountChanged stream
        let mut events = client.subscribe_events().await?;
        loop {
            match events.next().await {
                Some(DaemonEvent::UnreadCountChanged { counts }) => {
                    print_notify_line(&counts, &args.format);
                }
                Some(_) => continue,
                None => break,
            }
        }
    } else {
        let resp = client.send_command(Command::GetUnreadCounts).await?;
        print_notify_line(&resp.counts, &args.format);
    }
    Ok(())
}
```

### 10.3 Events CLI (Daemon Event Stream)

**Files to modify**: `crates/cli/src/main.rs`, `crates/cli/src/events.rs` (new)

```rust
/// Watch the daemon event stream in real time.
///
///   mxr events                                          # All events, human-readable
///   mxr events --type sync|message|snooze|rule|error|send  # Filter by event type
///   mxr events --format json                            # JSONL output for piping/parsing
#[derive(clap::Args)]
struct EventsArgs {
    /// Filter to specific event type(s). Comma-separated.
    #[arg(long = "type")]
    event_type: Option<String>,
    /// Output format: text, json (JSONL — one JSON object per line).
    #[arg(long, default_value = "text")]
    format: String,
}
```

Handler:

```rust
Commands::Events(args) => {
    let client = connect_to_daemon().await?;
    let type_filter: Option<Vec<String>> = args.event_type
        .as_ref()
        .map(|t| t.split(',').map(|s| s.trim().to_string()).collect());

    let mut events = client.subscribe_events().await?;
    loop {
        match events.next().await {
            Some(event) => {
                if let Some(ref filter) = type_filter {
                    if !filter.contains(&event.category()) {
                        continue;
                    }
                }
                match args.format.as_str() {
                    "json" => println!("{}", serde_json::to_string(&event).unwrap()),
                    _ => println!("{}", event.display_line()),
                }
            }
            None => break,
        }
    }
    Ok(())
}
```

### 10.4 Search `--format ids` Support

**Files to modify**: `crates/cli/src/search.rs`

Add `ids` as a valid format option for search results. Outputs one message/thread ID per line, designed for xargs piping:

```bash
mxr search "from:boss@work.com is:unread" --format ids | xargs -I{} mxr archive {}
```

```rust
// In search output formatting:
"ids" => {
    for result in &results {
        println!("{}", result.message_id);
    }
}
```

Auto-format detection (already in place from Phase 1): TTY -> table, piped -> json. `--format ids` is an explicit override for scripting use cases.

### 10.5 Tests

```rust
#[test]
fn labels_create_requires_name() {
    let cmd = Cli::try_parse_from(["mxr", "labels", "create"]);
    assert!(cmd.is_err());
}

#[test]
fn labels_create_accepts_color() {
    let cmd = Cli::try_parse_from(["mxr", "labels", "create", "work", "--color", "#ff6600"]);
    assert!(cmd.is_ok());
}

#[test]
fn labels_rename_requires_both_args() {
    let cmd = Cli::try_parse_from(["mxr", "labels", "rename", "old"]);
    assert!(cmd.is_err());
}

#[test]
fn notify_watch_flag() {
    let cmd = Cli::try_parse_from(["mxr", "notify", "--watch"]);
    assert!(cmd.is_ok());
}

#[test]
fn events_type_filter() {
    let cmd = Cli::try_parse_from(["mxr", "events", "--type", "sync,rule"]);
    assert!(cmd.is_ok());
}

#[test]
fn events_json_format() {
    let cmd = Cli::try_parse_from(["mxr", "events", "--format", "json"]);
    assert!(cmd.is_ok());
}

#[test]
fn search_format_ids() {
    let cmd = Cli::try_parse_from(["mxr", "search", "query", "--format", "ids"]);
    assert!(cmd.is_ok());
}

#[test]
fn export_search_bulk() {
    let cmd = Cli::try_parse_from([
        "mxr", "export", "--search", "label:old", "--format", "mbox",
    ]);
    assert!(cmd.is_ok());
}

#[test]
fn export_output_path() {
    let cmd = Cli::try_parse_from([
        "mxr", "export", "THREAD_123", "--output", "~/exports/",
    ]);
    assert!(cmd.is_ok());
}
```

**Dependencies**: `mxr-core`, `mxr-protocol`, `clap`, `serde_json`

---

## Step 11: Daemon Observability (A006)

Full observability lands in Phase 3: structured logging, log tailing, live status dashboard, health checks with exit codes, and event log population.

### 11.1 Structured Logging with `tracing`

**Files to modify**: `crates/daemon/src/main.rs`, `crates/daemon/src/logging.rs` (new)

The daemon already uses `tracing` for structured logging (established in Phase 1). Phase 3 adds:

1. **Log file output** with rotation
2. **Log level filtering** at runtime
3. **Category tagging** for all log events

```rust
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt, fmt};

/// Initialize daemon logging with file rotation and optional stderr output.
pub fn init_logging(config: &LoggingConfig) -> Result<()> {
    let log_dir = data_dir().join("logs");
    std::fs::create_dir_all(&log_dir)?;

    // Rolling file appender — rotates based on max_size_mb
    let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .max_log_files(config.max_files as usize)
        .filename_prefix("mxr")
        .filename_suffix("log")
        .build(&log_dir)?;

    let file_layer = fmt::layer()
        .with_writer(file_appender)
        .with_ansi(false)
        .json();

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(&config.level));

    let mut layers = tracing_subscriber::registry()
        .with(filter)
        .with(file_layer);

    if config.stderr {
        let stderr_layer = fmt::layer()
            .with_writer(std::io::stderr)
            .compact();
        layers = layers.with(stderr_layer);
    }

    layers.init();
    Ok(())
}
```

### 11.2 Logging Configuration

**Files to modify**: `crates/cli/src/config.rs`

```toml
[logging]
level = "info"                # debug | info | warn | error
max_size_mb = 50              # Max size per log file before rotation
max_files = 5                 # Number of rotated log files to keep
stderr = true                 # Also log to stderr (foreground mode)
event_retention_days = 90     # How long to keep event_log entries
```

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default = "default_max_size_mb")]
    pub max_size_mb: u64,
    #[serde(default = "default_max_files")]
    pub max_files: u32,
    #[serde(default = "default_stderr")]
    pub stderr: bool,
    #[serde(default = "default_event_retention_days")]
    pub event_retention_days: u32,
}

fn default_log_level() -> String { "info".into() }
fn default_max_size_mb() -> u64 { 50 }
fn default_max_files() -> u32 { 5 }
fn default_stderr() -> bool { true }
fn default_event_retention_days() -> u32 { 90 }
```

### 11.3 `mxr logs` CLI (Live Tailing)

**Files to modify**: `crates/cli/src/main.rs`, `crates/cli/src/logs.rs` (new)

```rust
/// Tail daemon logs (live, like `tail -f`).
///
///   mxr logs                                  # Live tail, all levels
///   mxr logs --level warn|error|info|debug    # Filter by minimum level
///   mxr logs --since "1h"                     # Show logs from last hour
///   mxr logs --since "2026-03-17 09:00"       # Show logs since specific time
///   mxr logs --grep "pattern"                 # Filter by text pattern (regex)
///   mxr logs --category sync|rule|send|auth|index  # Filter by category
///   mxr logs --format json                    # JSON output for piping
#[derive(clap::Args)]
struct LogsArgs {
    /// Minimum log level to show.
    #[arg(long)]
    level: Option<String>,
    /// Show logs since duration or datetime.
    /// Duration: "1h", "30m", "2d". Datetime: "2026-03-17 09:00".
    #[arg(long)]
    since: Option<String>,
    /// Filter log lines by regex pattern.
    #[arg(long)]
    grep: Option<String>,
    /// Filter by log category.
    #[arg(long)]
    category: Option<String>,
    /// Output format: text, json.
    #[arg(long, default_value = "text")]
    format: String,
}
```

Handler:

```rust
Commands::Logs(args) => {
    let log_path = data_dir().join("logs");

    // Parse --since into a timestamp
    let since = args.since.as_ref().map(|s| parse_since(s)).transpose()?;

    // Build log filter
    let filter = LogFilter {
        level: args.level.clone(),
        since,
        grep: args.grep.as_ref().map(|p| regex::Regex::new(p)).transpose()?,
        category: args.category.clone(),
    };

    // Read existing log lines matching filter
    let log_file = log_path.join("mxr.log");
    if log_file.exists() {
        let reader = std::io::BufReader::new(std::fs::File::open(&log_file)?);
        for line in reader.lines() {
            let line = line?;
            if filter.matches(&line) {
                print_log_line(&line, &args.format);
            }
        }
    }

    // Live tail: subscribe to daemon log stream
    let client = connect_to_daemon().await?;
    let mut stream = client.subscribe_logs(filter.to_subscribe_filter()).await?;
    loop {
        match stream.next().await {
            Some(entry) => print_log_line(&entry, &args.format),
            None => break,
        }
    }
    Ok(())
}
```

### 11.4 `mxr status --watch` (Live Dashboard)

**Files to modify**: `crates/cli/src/status.rs`

```rust
/// Live dashboard mode — like htop for mxr.
///
///   mxr status --watch
///
/// Displays a continuously updating view:
///   - Per-account sync status (last sync, next sync, messages synced)
///   - Unread counts per label
///   - Daemon uptime, memory usage
///   - Active rules and recent matches
///   - Event log tail (last 5 events)
///
/// Refreshes every 2 seconds. Press `q` or Ctrl-C to exit.
if args.watch {
    let client = connect_to_daemon().await?;
    let mut events = client.subscribe_events().await?;

    // Use crossterm raw mode for live dashboard
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = std::io::stdout();

    loop {
        // Fetch current status
        let status = client.send_command(Command::GetStatus).await?;

        // Clear and redraw
        crossterm::execute!(stdout, crossterm::terminal::Clear(crossterm::terminal::ClearType::All))?;
        crossterm::execute!(stdout, crossterm::cursor::MoveTo(0, 0))?;

        print_dashboard(&status, &mut stdout)?;

        // Check for quit key or event update
        if crossterm::event::poll(Duration::from_secs(2))? {
            if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
                if key.code == crossterm::event::KeyCode::Char('q') {
                    break;
                }
            }
        }
    }

    crossterm::terminal::disable_raw_mode()?;
}
```

### 11.5 `mxr doctor --check` (Health Check with Exit Codes)

**Files to modify**: `crates/cli/src/doctor.rs`

Extends the existing `mxr doctor` from Step 9 with monitoring-friendly features:

```rust
/// Health check mode for monitoring systems (Nagios, cron, scripts).
///
///   mxr doctor --check                    # Exit 0 = healthy, exit 1 = unhealthy
///   mxr doctor --check --format json      # JSON output for monitoring dashboards
///   mxr doctor --index-stats              # Tantivy index statistics
///   mxr doctor --store-stats              # SQLite store statistics
#[derive(clap::Args)]
struct DoctorArgs {
    // ... existing fields (reindex, etc.)

    /// Run health check and exit with code (0 = healthy, 1 = issues found).
    /// Designed for monitoring scripts and cron.
    #[arg(long)]
    check: bool,

    /// Output format for --check.
    #[arg(long, default_value = "text")]
    format: String,

    /// Show Tantivy search index statistics (document count, size, segments).
    #[arg(long)]
    index_stats: bool,

    /// Show SQLite store statistics (table row counts, db size, WAL size).
    #[arg(long)]
    store_stats: bool,
}
```

Handler for `--check`:

```rust
if args.check {
    let checks = run_all_health_checks(&config, &store, &index_path).await;
    let healthy = checks.iter().all(|c| c.status == CheckStatus::Ok);

    match args.format.as_str() {
        "json" => {
            let output = serde_json::json!({
                "healthy": healthy,
                "checks": checks.iter().map(|c| serde_json::json!({
                    "name": c.name,
                    "status": format!("{:?}", c.status),
                    "message": c.message,
                })).collect::<Vec<_>>(),
                "timestamp": chrono::Utc::now().to_rfc3339(),
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
        _ => {
            for check in &checks {
                let icon = match check.status {
                    CheckStatus::Ok => "OK",
                    CheckStatus::Warn => "WARN",
                    CheckStatus::Error => "FAIL",
                };
                println!("  {}: {} - {}", check.name, icon, check.message);
            }
        }
    }

    std::process::exit(if healthy { 0 } else { 1 });
}
```

Handler for `--index-stats` and `--store-stats`:

```rust
if args.index_stats {
    let stats = get_index_stats(&index_path)?;
    println!("Documents: {}", stats.doc_count);
    println!("Segments:  {}", stats.segment_count);
    println!("Size:      {}", humanize_bytes(stats.size_bytes));
}

if args.store_stats {
    let stats = get_store_stats(&store).await?;
    println!("Messages:    {}", stats.message_count);
    println!("Threads:     {}", stats.thread_count);
    println!("Labels:      {}", stats.label_count);
    println!("Attachments: {}", stats.attachment_count);
    println!("Events:      {}", stats.event_count);
    println!("DB size:     {}", humanize_bytes(stats.db_size_bytes));
    println!("WAL size:    {}", humanize_bytes(stats.wal_size_bytes));
}
```

### 11.6 event_log Table Population

**Files to modify**: `crates/store/src/migrations/`, `crates/daemon/src/sync.rs`, `crates/daemon/src/handler.rs`

Add the `event_log` table (schema from A006):

```sql
CREATE TABLE IF NOT EXISTS event_log (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp   INTEGER NOT NULL,
    level       TEXT NOT NULL CHECK (level IN ('error', 'warn', 'info')),
    category    TEXT NOT NULL,
    account_id  TEXT,
    message_id  TEXT,
    rule_id     TEXT,
    summary     TEXT NOT NULL,
    details     TEXT,
    FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE CASCADE
);

CREATE INDEX idx_event_log_time ON event_log(timestamp DESC);
CREATE INDEX idx_event_log_category ON event_log(category, timestamp DESC);
CREATE INDEX idx_event_log_level ON event_log(level, timestamp DESC);
```

Events are written during:
- **Sync**: sync started, sync completed (with message count), sync error
- **Rule execution**: rule matched, actions applied, rule error
- **Send**: message sent, send error
- **Auth**: auth refreshed, auth expired
- **Index**: reindex started, reindex completed

```rust
/// Insert an event into the event_log table.
pub async fn log_event(
    &self,
    level: &str,
    category: &str,
    account_id: Option<&str>,
    message_id: Option<&str>,
    rule_id: Option<&str>,
    summary: &str,
    details: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO event_log (timestamp, level, category, account_id, message_id, rule_id, summary, details)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
    )
    .bind(chrono::Utc::now().timestamp())
    .bind(level)
    .bind(category)
    .bind(account_id)
    .bind(message_id)
    .bind(rule_id)
    .bind(summary)
    .bind(details)
    .execute(&self.pool)
    .await?;
    Ok(())
}
```

### 11.7 Event Log Pruning

**Files to modify**: `crates/daemon/src/maintenance.rs` (new or extend existing)

```rust
/// Prune old event_log entries based on event_retention_days config.
/// Called periodically by daemon (e.g. once per day or at startup).
pub async fn prune_event_log(store: &Store, retention_days: u32) -> Result<u64> {
    let cutoff = chrono::Utc::now().timestamp() - (retention_days as i64 * 86400);
    let result = sqlx::query("DELETE FROM event_log WHERE timestamp < ?1")
        .bind(cutoff)
        .execute(&store.pool)
        .await?;
    let deleted = result.rows_affected();
    if deleted > 0 {
        tracing::info!(deleted = deleted, "Pruned old event log entries");
    }
    Ok(deleted)
}
```

### 11.8 Tests

```rust
#[tokio::test]
async fn event_log_insert_and_query() {
    let store = create_test_store().await;
    store.log_event("info", "sync", Some("personal"), None, None, "Sync completed: 42 new messages", None).await.unwrap();

    let events = store.query_event_log(None, None, 10).await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].category, "sync");
}

#[tokio::test]
async fn event_log_pruning() {
    let store = create_test_store().await;
    // Insert old event (91 days ago)
    let old_ts = chrono::Utc::now().timestamp() - (91 * 86400);
    sqlx::query("INSERT INTO event_log (timestamp, level, category, summary) VALUES (?1, 'info', 'sync', 'old')")
        .bind(old_ts)
        .execute(&store.pool).await.unwrap();
    // Insert recent event
    store.log_event("info", "sync", None, None, None, "recent", None).await.unwrap();

    let deleted = prune_event_log(&store, 90).await.unwrap();
    assert_eq!(deleted, 1);

    let remaining = store.query_event_log(None, None, 10).await.unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].summary, "recent");
}

#[test]
fn doctor_check_exit_code_healthy() {
    // All checks pass -> exit 0
}

#[test]
fn doctor_check_exit_code_unhealthy() {
    // Any check fails -> exit 1
}

#[test]
fn logs_since_duration_parsing() {
    assert_eq!(parse_since("1h").unwrap(), /* 1 hour ago timestamp */);
    assert_eq!(parse_since("30m").unwrap(), /* 30 min ago timestamp */);
    assert_eq!(parse_since("2d").unwrap(), /* 2 days ago timestamp */);
    assert!(parse_since("2026-03-17 09:00").is_ok());
}
```

**Dependencies**: `tracing`, `tracing-subscriber`, `tracing-appender`, `crossterm`, `chrono`, `serde_json`, `regex`

---

## Step 12: TUI Batch Operations (A007)

Phase 3 adds the remaining TUI batch selection features from A007: pattern select, vim count support, and full Visual Line mode refinement.

### 12.1 Pattern Select (`*` prefix)

**Files to modify**: `crates/tui/src/input.rs`, `crates/tui/src/message_list.rs`

Pattern select uses a `*` prefix key that enters a pending state, waiting for the pattern character:

```rust
/// Pattern select: `*` enters pending mode, next key determines pattern.
///
///   *a    Select all in current view
///   *n    Select none (clear selection)
///   *r    Select all read messages
///   *u    Select all unread messages
///   *s    Select all starred messages
///   *t    Select all in current thread
#[derive(Debug, Clone, Copy)]
pub enum PatternSelect {
    All,
    None,
    Read,
    Unread,
    Starred,
    Thread,
}

impl PatternSelect {
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            'a' => Some(Self::All),
            'n' => Some(Self::None),
            'r' => Some(Self::Read),
            'u' => Some(Self::Unread),
            's' => Some(Self::Starred),
            't' => Some(Self::Thread),
            _ => None,
        }
    }
}

/// Apply pattern select to message list, returning indices to select.
pub fn apply_pattern_select(
    pattern: PatternSelect,
    messages: &[Envelope],
    current_thread_id: Option<&str>,
) -> Vec<usize> {
    match pattern {
        PatternSelect::All => (0..messages.len()).collect(),
        PatternSelect::None => vec![],
        PatternSelect::Read => messages.iter().enumerate()
            .filter(|(_, m)| !m.is_unread)
            .map(|(i, _)| i)
            .collect(),
        PatternSelect::Unread => messages.iter().enumerate()
            .filter(|(_, m)| m.is_unread)
            .map(|(i, _)| i)
            .collect(),
        PatternSelect::Starred => messages.iter().enumerate()
            .filter(|(_, m)| m.is_starred)
            .map(|(i, _)| i)
            .collect(),
        PatternSelect::Thread => {
            if let Some(tid) = current_thread_id {
                messages.iter().enumerate()
                    .filter(|(_, m)| m.thread_id == tid)
                    .map(|(i, _)| i)
                    .collect()
            } else {
                vec![]
            }
        }
    }
}
```

### 12.2 Vim Count Support

**Files to modify**: `crates/tui/src/input.rs`

Digit presses accumulate into a count prefix before a motion key:

```rust
/// Vim count accumulator for the input state machine.
///
///   5j      = move down 5
///   10k     = move up 10
///   V 10j   = enter visual, select 11 messages (current + 10 below)
///   3e      = archive 3 messages starting from cursor
pub struct CountAccumulator {
    digits: Vec<char>,
}

impl CountAccumulator {
    pub fn new() -> Self {
        Self { digits: vec![] }
    }

    /// Feed a character. Returns true if it was consumed as a digit.
    pub fn feed(&mut self, c: char) -> bool {
        if c.is_ascii_digit() && !(self.digits.is_empty() && c == '0') {
            // Don't consume '0' as count prefix (it means jump-to-start in vim)
            self.digits.push(c);
            true
        } else {
            false
        }
    }

    /// Take the accumulated count and reset.
    /// Returns 1 if no digits were accumulated (default count).
    pub fn take(&mut self) -> usize {
        let count = if self.digits.is_empty() {
            1
        } else {
            self.digits.iter().collect::<String>().parse().unwrap_or(1)
        };
        self.digits.clear();
        count
    }

    /// Clear without consuming.
    pub fn clear(&mut self) {
        self.digits.clear();
    }
}
```

Integration into the main input handler:

```rust
// In the main key event loop:
KeyCode::Char(c) if c.is_ascii_digit() => {
    if count_acc.feed(c) {
        // Digit consumed, wait for motion key
        continue;
    }
}
KeyCode::Char('j') | KeyCode::Down => {
    let n = count_acc.take();
    for _ in 0..n {
        app.move_down();
    }
}
KeyCode::Char('k') | KeyCode::Up => {
    let n = count_acc.take();
    for _ in 0..n {
        app.move_up();
    }
}
```

### 12.3 Visual Line Mode Refinement

**Files to modify**: `crates/tui/src/input.rs`, `crates/tui/src/selection.rs`

Extends the basic Visual Line mode (from A005/A007) with full vim motions:

```rust
/// Visual Line mode state.
///
/// Enter with `V`. All motion keys extend selection from anchor to cursor.
/// Motions supported:
///   j/k       Move cursor (extends selection)
///   G         Extend selection to bottom
///   gg        Extend selection to top
///   Ctrl-d    Extend selection half-page down
///   Ctrl-u    Extend selection half-page up
///   Escape    Cancel visual mode, clear selection
///   Action key (e, #, s, etc.)  Apply to selection, exit visual mode
pub struct VisualLineMode {
    /// Anchor: where V was pressed.
    pub anchor: usize,
    /// Cursor: current position (extends selection).
    pub cursor: usize,
}

impl VisualLineMode {
    pub fn new(position: usize) -> Self {
        Self {
            anchor: position,
            cursor: position,
        }
    }

    /// Returns the selected range (inclusive), ordered.
    pub fn selected_range(&self) -> std::ops::RangeInclusive<usize> {
        let start = self.anchor.min(self.cursor);
        let end = self.anchor.max(self.cursor);
        start..=end
    }

    /// Move cursor down by `count`, extending selection.
    pub fn move_down(&mut self, count: usize, max: usize) {
        self.cursor = (self.cursor + count).min(max);
    }

    /// Move cursor up by `count`, extending selection.
    pub fn move_up(&mut self, count: usize) {
        self.cursor = self.cursor.saturating_sub(count);
    }

    /// Extend selection to bottom.
    pub fn jump_bottom(&mut self, max: usize) {
        self.cursor = max;
    }

    /// Extend selection to top.
    pub fn jump_top(&mut self) {
        self.cursor = 0;
    }

    /// Extend selection half-page down.
    pub fn half_page_down(&mut self, viewport_size: usize, max: usize) {
        self.cursor = (self.cursor + viewport_size / 2).min(max);
    }

    /// Extend selection half-page up.
    pub fn half_page_up(&mut self, viewport_size: usize) {
        self.cursor = self.cursor.saturating_sub(viewport_size / 2);
    }
}
```

Visual mode integration with count support:

```rust
// In visual mode key handler:
if let Some(ref mut visual) = app.visual_mode {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            let n = count_acc.take();
            visual.move_down(n, app.message_count() - 1);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            let n = count_acc.take();
            visual.move_up(n);
        }
        KeyCode::Char('G') => visual.jump_bottom(app.message_count() - 1),
        KeyCode::Char('g') => {
            // Wait for second 'g' (reuse gg state machine)
            pending_g = true;
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            visual.half_page_down(app.viewport_size(), app.message_count() - 1);
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            visual.half_page_up(app.viewport_size());
        }
        KeyCode::Esc => {
            app.visual_mode = None;
            app.clear_selection();
        }
        // Action keys: apply to visual selection range
        KeyCode::Char('e') => {
            let range = visual.selected_range();
            app.archive_range(range);
            app.visual_mode = None;
        }
        // ... other action keys
        _ => {}
    }
}
```

### 12.4 Tests

```rust
#[test]
fn pattern_select_all() {
    let messages = vec![make_msg(true, false), make_msg(false, true), make_msg(true, true)];
    let selected = apply_pattern_select(PatternSelect::All, &messages, None);
    assert_eq!(selected, vec![0, 1, 2]);
}

#[test]
fn pattern_select_unread() {
    let messages = vec![
        make_msg_unread(true),
        make_msg_unread(false),
        make_msg_unread(true),
    ];
    let selected = apply_pattern_select(PatternSelect::Unread, &messages, None);
    assert_eq!(selected, vec![0, 2]);
}

#[test]
fn pattern_select_none_clears() {
    let messages = vec![make_msg(true, false)];
    let selected = apply_pattern_select(PatternSelect::None, &messages, None);
    assert!(selected.is_empty());
}

#[test]
fn count_accumulator_single_digit() {
    let mut acc = CountAccumulator::new();
    acc.feed('5');
    assert_eq!(acc.take(), 5);
}

#[test]
fn count_accumulator_multi_digit() {
    let mut acc = CountAccumulator::new();
    acc.feed('1');
    acc.feed('0');
    assert_eq!(acc.take(), 10);
}

#[test]
fn count_accumulator_default_is_one() {
    let mut acc = CountAccumulator::new();
    assert_eq!(acc.take(), 1);
}

#[test]
fn count_accumulator_zero_not_consumed() {
    let mut acc = CountAccumulator::new();
    assert!(!acc.feed('0')); // '0' alone is not a count prefix
}

#[test]
fn visual_line_selection_range() {
    let mut v = VisualLineMode::new(5);
    v.move_down(3, 100);
    assert_eq!(v.selected_range(), 5..=8);
}

#[test]
fn visual_line_selection_upward() {
    let mut v = VisualLineMode::new(10);
    v.move_up(3);
    assert_eq!(v.selected_range(), 7..=10);
}

#[test]
fn visual_line_jump_bottom() {
    let mut v = VisualLineMode::new(5);
    v.jump_bottom(99);
    assert_eq!(v.selected_range(), 5..=99);
}

#[test]
fn visual_line_jump_top() {
    let mut v = VisualLineMode::new(50);
    v.jump_top();
    assert_eq!(v.selected_range(), 0..=50);
}

#[test]
fn visual_line_half_page() {
    let mut v = VisualLineMode::new(10);
    v.half_page_down(40, 100);
    assert_eq!(v.cursor, 30);
    v.half_page_up(40);
    assert_eq!(v.cursor, 10);
}

#[test]
fn visual_with_count_5j_selects_6() {
    // V then 5j should select 6 messages (anchor + 5)
    let mut v = VisualLineMode::new(0);
    v.move_down(5, 100);
    let range = v.selected_range();
    assert_eq!(range.clone().count(), 6); // 0..=5
}
```

**Dependencies**: `crossterm`, `mxr-core`

---

## Definition of Done

Phase 3 is complete when ALL of the following are true:

1. **Export**: `mxr export THREAD_ID --format markdown|json|mbox|llm` outputs to stdout. `mxr export --search "query" --format mbox > archive.mbox` for bulk export. `mxr export THREAD_ID --output ~/exports/` saves to file. LLM format uses reader pipeline for noise reduction. `e` in TUI thread view shows format picker and copies to clipboard. `mxr export THREAD_ID --format llm | llm "Summarize"` works end-to-end.
2. **Rules Engine**: Rules defined in `config.toml` `[[rules]]` sections with composable conditions (And/Or/Not + field conditions). Rule evaluation happens automatically during sync (priority-ordered, actions accumulated then applied). Full `mxr rules` subcommand tree: list, show, add, edit, enable, disable, delete, dry-run, history. Execution history logged to SQLite for auditability.
3. **Shell Hooks**: `ShellHook` action type runs external commands with message JSON on stdin. Configurable timeout (default 30s). Exit code 0 = success, non-zero = logged error. Hooks execute reliably within the rule engine pipeline.
4. **Multi-Account**: Multiple `[accounts.*]` in config.toml. Per-account sync loops in daemon. Account switcher in TUI sidebar and command palette. Labels namespaced per account. Global view (all accounts) and per-account filtering. Default account for compose.
5. **HTML Rendering**: `html_command` in config pipes HTML to external tool (w3m, lynx). Falls back to built-in `html2text` if command unavailable. Clear error messages if configured command not found.
6. **Tantivy Reindex**: `mxr doctor --reindex` drops and rebuilds search index from SQLite. Progress reporting. Index integrity verified after rebuild.
7. **Shell Completions**: `mxr completions bash|zsh|fish` generates completions via clap. Instructions in help text.
8. **Performance**: Virtual scrolling in TUI (only visible items rendered). Lazy label count updates. SQLite WAL mode + verified indexes. Tantivy batch commits during sync. Smooth operation with 10k+ messages.
9. **Error UX**: Context-rich error messages with actionable suggestions. Auth expiry detected and surfaced in TUI. Network failures retried with exponential backoff. `mxr doctor` provides comprehensive diagnostics with fix suggestions.
10. **CLI Surface (A004)**: Labels management CLI (`mxr labels create/delete/rename`). `mxr notify` with `--format json` and `--watch` for status bar integration. `mxr events` with `--type` filter and `--format json` (JSONL). `mxr search --format ids` for xargs piping. Auto-format detection (TTY vs pipe) already in place from Phase 1.
11. **Daemon Observability (A006)**: `mxr logs` with live tailing, `--level`, `--since`, `--grep`, `--category`, `--format json`. `mxr status --watch` live dashboard. `mxr doctor --check` with exit codes for monitoring. `mxr doctor --check --format json` for dashboards. `mxr doctor --index-stats` and `--store-stats`. `[logging]` config section with `max_size_mb`, `max_files`, `event_retention_days`. Log file rotation via `tracing-appender`. `event_log` table populated during sync, rule execution, send, auth, and index operations. Event log pruning based on `event_retention_days`.
12. **TUI Batch Operations (A007)**: Pattern select with `*` prefix (`*a` all, `*n` none, `*r` read, `*u` unread, `*s` starred, `*t` thread). Vim count support (`5j` = move 5, `V 10j` = select 11). Full Visual Line mode refinement with `G`, `gg`, `Ctrl-d`, `Ctrl-u` extending selection.
13. **Tests**: Unit tests for export formats, condition evaluation, string matching, shell hook execution, virtual scrolling, pattern select, count accumulator, visual line mode, event log, log parsing. Integration tests for rule evaluation pipeline, reindex, retry logic, bulk export.
14. **CI passes**: `cargo check`, `cargo fmt`, `cargo clippy`, `cargo test` all green.

### User Acceptance Test

You can, as a daily workflow:
- Export a thread as LLM context and pipe it to an AI for summarization
- Bulk export by search: `mxr export --search "label:old" --format mbox > archive.mbox`
- Define rules in config.toml that auto-archive read newsletters and auto-label invoices
- Manage rules entirely from CLI: `mxr rules add/show/edit/enable/disable/delete`
- See rule dry-run output showing what would be affected before enabling
- Review rule execution history: `mxr rules history`
- Use a shell hook to process invoice emails with a custom script
- Switch between personal and work accounts in the TUI
- See unified "All Accounts" inbox or filter to a single account
- Create, rename, and delete labels from CLI: `mxr labels create "name" --color "#hex"`
- See unread count in your status bar via `mxr notify` or `mxr notify --watch`
- Watch daemon event stream: `mxr events --type sync,rule --format json`
- Pipe search results to batch operations: `mxr search "query" --format ids | xargs -I{} mxr archive {}`
- Tail daemon logs with filtering: `mxr logs --level warn --since "1h" --grep "gmail"`
- Monitor daemon health in a live dashboard: `mxr status --watch`
- Use `mxr doctor --check` in monitoring scripts (exit code 0/1)
- Inspect index and store stats: `mxr doctor --index-stats --store-stats`
- Select messages by pattern in TUI: `*u` for all unread, `*s` for all starred
- Use vim counts: `5j` to move 5, `V 10j` to select 11 messages
- Use Visual mode with `G`/`gg`/`Ctrl-d`/`Ctrl-u` to extend selection
- Browse a 10k+ message inbox without UI lag
- Run `mxr doctor` and get clear diagnostics with actionable suggestions
- Re-authenticate when tokens expire with a clear prompt

---

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| Rules engine regex patterns cause catastrophic backtracking (ReDoS) | Sync hangs, CPU spike on pathological input | Use `regex` crate which guarantees linear-time matching. Document that complex patterns may slow evaluation. Add per-rule evaluation timeout (100ms default). |
| Shell hooks executing arbitrary commands pose security risk | Malicious config could run dangerous commands | Hooks only execute commands defined in user's own config file. Document the risk clearly. Never execute commands from email content. Log all hook executions for auditability. |
| Shell hook timeout too short for legitimate use cases | Users' scripts get killed mid-execution | Make timeout configurable per-rule (`timeout = 120`) with a sane default (30s). Log when hooks are killed due to timeout. |
| Multi-account label collision (both accounts have "newsletters") | Wrong messages shown under a label | Namespace all labels with account_id in SQLite. Global view aggregates by label name across accounts but queries always join on account_id. |
| Multi-account sync loops competing for SQLite writes | Write contention, lock timeouts | SQLite WAL mode allows concurrent reads. Sync loops acquire write locks briefly per batch. If contention detected, log and retry with small delay. |
| Tantivy reindex takes too long for large mailboxes (100k+ messages) | User thinks it's hung, kills the process | Progress reporting with percentage and ETA. Batch commits every 500 documents to keep memory bounded. Document expected time (~1 minute per 10k messages). |
| Virtual scrolling breaks keyboard navigation edge cases (gg/G, search-jump) | User jumps to result but viewport doesn't follow | Comprehensive tests for all navigation commands. VirtualList always ensures `selected` is within `viewport_start..viewport_start+viewport_size`. |
| Mbox export loses information for HTML-only messages | Exported mbox missing message content | Convert HTML to text for mbox body. Include Content-Type header indicating plain text conversion. Document that mbox export is lossy for rich HTML emails. |
| LLM context export produces inconsistent output when reader mode strips too aggressively | AI gets incomplete context | LLM export uses same reader pipeline but with conservative settings (collapse quotes but don't strip everything). Include attachment metadata so AI knows what was referenced. |
| `clap_complete` generated completions become stale when commands change | Tab completion suggests wrong subcommands | Completions are generated at runtime (`mxr completions zsh`), not shipped as static files. Users regenerate after updating mxr. |
| Auth expiry during rule execution corrupts partially-applied actions | Some messages mutated, others not | Accumulate all rule actions first, then apply as a batch via Gmail batchModify. If auth fails mid-batch, log which actions succeeded and which failed. Allow `mxr rules replay` to re-apply. |
| Config TOML deserialization of complex condition trees is fragile | Users write invalid rules, get cryptic errors | Validate rules at config load time with clear errors ("Rule 'Archive newsletters': condition at index 2 has unknown field 'is_read', did you mean 'is_unread'?"). Provide example rules in docs. `mxr rules validate` subcommand. |
