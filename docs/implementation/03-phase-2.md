# 03 — Phase 2: Compose + Mutations + IMAP + Batch Operations

> **Current Layout Note**
> This phase plan still uses the historical `mxr-*` crate names. Those names are real internal workspace crates again, with the repo-root package `mxr` as the install surface.

## Goal

Full read-write, multi-provider email client. After this phase, you can use mxr as your primary email client: read, compose, reply, reply-all, forward, archive, trash, spam, star, search, snooze, unsubscribe, batch operations, and IMAP sync. Both Gmail and IMAP accounts work. Every TUI action has a CLI equivalent for scripting.

## Prerequisites

Phase 1 complete:
- Gmail OAuth2 working, real sync + delta sync operational
- SQLite store with envelopes, bodies, labels, attachments, drafts tables
- Tantivy search index with query parser
- TUI with three-pane layout, thread view, command palette, search
- Config file parsing (accounts, general, render settings)
- `List-Unsubscribe` header parsed and stored at sync time

## Step 1: mxr-reader Crate

Reader mode is a dependency for compose (context blocks use reader-cleaned text) and TUI display, so it comes first.

### 1.1 Crate Setup

Add to workspace `Cargo.toml`:

```toml
[workspace]
members = [
    # ... existing members
    "crates/reader",
]

[workspace.dependencies]
# ... existing deps
mxr-reader = { path = "crates/reader" }
html2text = "0.14"
regex = "1"
once_cell = "1"
```

`crates/reader/Cargo.toml`:
```toml
[package]
name = "mxr-reader"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
mxr-core = { workspace = true }
html2text = { workspace = true }
regex = { workspace = true }
once_cell = { workspace = true }
chrono = { workspace = true }
tracing = { workspace = true }

[dev-dependencies]
pretty_assertions = "1"
```

### 1.2 Core Types

`crates/reader/src/lib.rs`:
```rust
mod html;
mod quotes;
mod signatures;
mod boilerplate;
mod tracking;
mod pipeline;

pub use pipeline::{clean, ReaderOutput, ReaderConfig};
pub use quotes::QuotedBlock;
```

`crates/reader/src/pipeline.rs`:
```rust
use crate::{quotes, signatures, boilerplate, tracking, html};
use chrono::{DateTime, Utc};

/// Configuration for the reader pipeline.
#[derive(Debug, Clone)]
pub struct ReaderConfig {
    /// External command for HTML-to-text conversion (e.g., "w3m -T text/html -dump").
    /// If None, uses built-in html2text.
    pub html_command: Option<String>,
    /// Whether to strip signatures.
    pub strip_signatures: bool,
    /// Whether to collapse quoted replies.
    pub collapse_quotes: bool,
    /// Whether to strip boilerplate/disclaimers.
    pub strip_boilerplate: bool,
    /// Whether to strip tracking/footer junk.
    pub strip_tracking: bool,
}

impl Default for ReaderConfig {
    fn default() -> Self {
        Self {
            html_command: None,
            strip_signatures: true,
            collapse_quotes: true,
            strip_boilerplate: true,
            strip_tracking: true,
        }
    }
}

/// Output from the reader pipeline.
#[derive(Debug, Clone)]
pub struct ReaderOutput {
    /// Cleaned content: just the human-written text.
    pub content: String,
    /// Quoted messages that were stripped (available for expansion).
    pub quoted_messages: Vec<QuotedBlock>,
    /// The signature that was stripped.
    pub signature: Option<String>,
    /// Line count stats for UI display.
    pub original_lines: usize,
    pub cleaned_lines: usize,
}

/// Run the full reader pipeline on a message body.
///
/// Accepts either plain text or HTML. If HTML, converts to plain text first.
pub fn clean(text: Option<&str>, html: Option<&str>, config: &ReaderConfig) -> ReaderOutput {
    // 1. Resolve to plain text
    let raw = match (text, html) {
        (Some(t), _) => t.to_string(),
        (None, Some(h)) => html::to_plain_text(h, config),
        (None, None) => String::new(),
    };

    let original_lines = raw.lines().count();
    let mut content = raw.clone();
    let mut quoted_messages = Vec::new();
    let mut signature = None;

    // 2. Extract and collapse quoted replies
    if config.collapse_quotes {
        let (cleaned, quotes) = quotes::collapse(&content);
        content = cleaned;
        quoted_messages = quotes;
    }

    // 3. Strip signatures
    if config.strip_signatures {
        let (cleaned, sig) = signatures::strip(&content);
        content = cleaned;
        signature = sig;
    }

    // 4. Strip boilerplate
    if config.strip_boilerplate {
        content = boilerplate::strip(&content);
    }

    // 5. Strip tracking junk
    if config.strip_tracking {
        content = tracking::strip(&content);
    }

    // 6. Clean up excessive whitespace
    content = normalize_whitespace(&content);

    let cleaned_lines = content.lines().count();

    ReaderOutput {
        content,
        quoted_messages,
        signature,
        original_lines,
        cleaned_lines,
    }
}

fn normalize_whitespace(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut blank_count = 0;
    for line in text.lines() {
        if line.trim().is_empty() {
            blank_count += 1;
            if blank_count <= 2 {
                result.push('\n');
            }
        } else {
            blank_count = 0;
            result.push_str(line);
            result.push('\n');
        }
    }
    result.trim().to_string()
}
```

### 1.3 HTML to Plain Text

`crates/reader/src/html.rs`:
```rust
use crate::pipeline::ReaderConfig;
use std::process::Command;

/// Convert HTML to plain text.
/// Uses external command if configured, otherwise built-in html2text.
pub fn to_plain_text(html: &str, config: &ReaderConfig) -> String {
    if let Some(cmd) = &config.html_command {
        match run_external_command(cmd, html) {
            Ok(text) => return text,
            Err(e) => {
                tracing::warn!("External html_command failed, falling back to built-in: {e}");
            }
        }
    }
    html2text::from_read(html.as_bytes(), 80)
}

fn run_external_command(cmd: &str, input: &str) -> Result<String, std::io::Error> {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    let output = Command::new(parts[0])
        .args(&parts[1..])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin {
                stdin.write_all(input.as_bytes())?;
            }
            child.wait_with_output()
        })?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
```

### 1.4 Signature Stripping

`crates/reader/src/signatures.rs`:
```rust
use once_cell::sync::Lazy;
use regex::Regex;

static SENT_FROM: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^sent from (my )?(iphone|ipad|android|galaxy|samsung|outlook|mail)").unwrap()
});

static PHONE_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"[\+]?\d[\d\s\-\(\)]{7,}").unwrap()
});

/// Strip the signature from plain text.
/// Returns (cleaned_text, extracted_signature).
pub fn strip(text: &str) -> (String, Option<String>) {
    let lines: Vec<&str> = text.lines().collect();

    // RFC 3676: "-- \n" delimiter (note trailing space)
    if let Some(pos) = lines.iter().position(|l| *l == "-- " || *l == "--") {
        let body = lines[..pos].join("\n");
        let sig = lines[pos + 1..].join("\n");
        return (body, Some(sig));
    }

    // Heuristic: "Sent from" pattern at end
    for i in (lines.len().saturating_sub(5)..lines.len()).rev() {
        if SENT_FROM.is_match(lines[i].trim()) {
            let body = lines[..i].join("\n");
            let sig = lines[i..].join("\n");
            return (body, Some(sig));
        }
    }

    // Heuristic: block of short lines at the end with phone numbers, titles
    // (common corporate signature pattern)
    let mut sig_start = None;
    let mut consecutive_short = 0;
    for i in (0..lines.len()).rev() {
        let line = lines[i].trim();
        if line.is_empty() {
            if consecutive_short >= 3 {
                sig_start = Some(i + 1);
                break;
            }
            consecutive_short = 0;
            continue;
        }
        let is_sig_line = line.len() < 60
            && (PHONE_PATTERN.is_match(line)
                || line.contains('@')
                || line.contains("http")
                || line.contains("www."));
        if is_sig_line {
            consecutive_short += 1;
        } else {
            break;
        }
    }

    if let Some(start) = sig_start {
        let body = lines[..start].join("\n");
        let sig = lines[start..].join("\n");
        return (body, Some(sig));
    }

    (text.to_string(), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_rfc3676_signature() {
        let text = "Hello world\n\nThanks!\n-- \nJohn Doe\nAcme Corp";
        let (body, sig) = strip(text);
        assert_eq!(body, "Hello world\n\nThanks!");
        assert_eq!(sig.unwrap(), "John Doe\nAcme Corp");
    }

    #[test]
    fn strips_sent_from() {
        let text = "Quick reply.\n\nSent from my iPhone";
        let (body, sig) = strip(text);
        assert_eq!(body, "Quick reply.");
        assert!(sig.unwrap().contains("Sent from"));
    }
}
```

### 1.5 Quote Collapsing

`crates/reader/src/quotes.rs`:
```rust
use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Debug, Clone)]
pub struct QuotedBlock {
    pub from: Option<String>,
    pub date: Option<String>,
    pub content: String,
}

static ON_WROTE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^On .+wrote:\s*$").unwrap()
});

static QUOTE_PREFIX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^>+\s?").unwrap()
});

/// Collapse quoted replies into summary blocks.
/// Returns (cleaned_text, extracted_quotes).
pub fn collapse(text: &str) -> (String, Vec<QuotedBlock>) {
    let lines: Vec<&str> = text.lines().collect();
    let mut result = Vec::new();
    let mut quotes = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        // Check for "On {date}, {person} wrote:" pattern
        if ON_WROTE.is_match(lines[i]) {
            let header = lines[i];
            let from = extract_from_on_wrote(header);
            let mut quote_lines = Vec::new();
            i += 1;

            // Collect all subsequent lines that are quoted (> prefix) or blank
            while i < lines.len() {
                if QUOTE_PREFIX.is_match(lines[i]) {
                    let stripped = QUOTE_PREFIX.replace(lines[i], "").to_string();
                    quote_lines.push(stripped);
                    i += 1;
                } else if lines[i].trim().is_empty() && i + 1 < lines.len() && QUOTE_PREFIX.is_match(lines[i + 1]) {
                    quote_lines.push(String::new());
                    i += 1;
                } else {
                    break;
                }
            }

            let label = if let Some(ref f) = from {
                format!("[previous message from {f}]")
            } else {
                "[previous message]".to_string()
            };
            result.push(label);

            quotes.push(QuotedBlock {
                from,
                date: None,
                content: quote_lines.join("\n"),
            });
            continue;
        }

        // Check for standalone > quoted blocks (no "On ... wrote:" header)
        if QUOTE_PREFIX.is_match(lines[i]) {
            let mut quote_lines = Vec::new();
            while i < lines.len() && (QUOTE_PREFIX.is_match(lines[i]) || lines[i].trim().is_empty()) {
                if QUOTE_PREFIX.is_match(lines[i]) {
                    let stripped = QUOTE_PREFIX.replace(lines[i], "").to_string();
                    quote_lines.push(stripped);
                } else {
                    // Only include blank lines if more quoted lines follow
                    if i + 1 < lines.len() && QUOTE_PREFIX.is_match(lines[i + 1]) {
                        quote_lines.push(String::new());
                    } else {
                        break;
                    }
                }
                i += 1;
            }

            result.push("[previous message]".to_string());
            quotes.push(QuotedBlock {
                from: None,
                date: None,
                content: quote_lines.join("\n"),
            });
            continue;
        }

        result.push(lines[i].to_string());
        i += 1;
    }

    (result.join("\n"), quotes)
}

fn extract_from_on_wrote(header: &str) -> Option<String> {
    // "On Mon, Mar 15, 2026, alice@example.com wrote:"
    // Try to extract the email or name before "wrote:"
    let lower = header.to_lowercase();
    if let Some(wrote_pos) = lower.rfind("wrote:") {
        let before = &header[..wrote_pos].trim();
        // Find the last comma-separated segment (usually the name/email)
        if let Some(last_comma) = before.rfind(',') {
            let candidate = before[last_comma + 1..].trim();
            if candidate.contains('@') || !candidate.is_empty() {
                return Some(candidate.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapses_on_wrote_block() {
        let text = "Thanks for the update.\n\nOn Mon, Mar 15, alice@example.com wrote:\n> Original message here\n> Second line";
        let (cleaned, quotes) = collapse(text);
        assert!(cleaned.contains("[previous message from alice@example.com]"));
        assert_eq!(quotes.len(), 1);
        assert!(quotes[0].content.contains("Original message here"));
    }

    #[test]
    fn collapses_bare_quotes() {
        let text = "My reply.\n\n> Some quoted text\n> More quoted text";
        let (cleaned, quotes) = collapse(text);
        assert!(cleaned.contains("[previous message]"));
        assert_eq!(quotes.len(), 1);
    }
}
```

### 1.6 Boilerplate and Tracking Stripping

`crates/reader/src/boilerplate.rs`:
```rust
use once_cell::sync::Lazy;
use regex::Regex;

static BOILERPLATE_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        Regex::new(r"(?i)this (email|message|communication) is confidential").unwrap(),
        Regex::new(r"(?i)if you (have )?received this (email|message) in error").unwrap(),
        Regex::new(r"(?i)^DISCLAIMER").unwrap(),
        Regex::new(r"(?i)this (email|message) (and any attachments )?(is|are) intended (only |solely )?for").unwrap(),
        Regex::new(r"(?i)please consider the environment before printing").unwrap(),
        Regex::new(r"(?i)any (views|opinions) expressed .* are (solely |those of )").unwrap(),
        Regex::new(r"(?i)privileged and confidential").unwrap(),
        Regex::new(r"(?i)if you are not the intended recipient").unwrap(),
    ]
});

/// Strip legal/confidentiality boilerplate from the end of the message.
pub fn strip(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();

    // Scan from the bottom for boilerplate start
    let mut boilerplate_start = None;
    for i in (0..lines.len()).rev() {
        let line = lines[i].trim();
        if line.is_empty() {
            continue;
        }
        let is_boilerplate = BOILERPLATE_PATTERNS.iter().any(|p| p.is_match(line));
        if is_boilerplate {
            boilerplate_start = Some(i);
        } else if boilerplate_start.is_some() {
            // Found non-boilerplate line above boilerplate block, stop scanning
            break;
        }
    }

    match boilerplate_start {
        Some(start) => lines[..start].join("\n"),
        None => text.to_string(),
    }
}
```

`crates/reader/src/tracking.rs`:
```rust
use once_cell::sync::Lazy;
use regex::Regex;

static TRACKING_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        Regex::new(r"(?i)view (this )?(email |message )?in (your )?browser").unwrap(),
        Regex::new(r"(?i)update your preferences").unwrap(),
        Regex::new(r"(?i)manage (your )?(email )?preferences").unwrap(),
        Regex::new(r"(?i)you (are )?receiving this (because|email)").unwrap(),
        Regex::new(r"(?i)to stop receiving these").unwrap(),
        Regex::new(r"(?i)click here to unsubscribe").unwrap(),
        Regex::new(r"(?i)no longer wish to receive").unwrap(),
        Regex::new(r"(?i)©\s*\d{4}").unwrap(),
        Regex::new(r"(?i)all rights reserved").unwrap(),
    ]
});

/// Strip tracking/footer junk lines.
pub fn strip(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();

    // Scan from bottom, strip contiguous tracking lines
    let mut content_end = lines.len();
    for i in (0..lines.len()).rev() {
        let line = lines[i].trim();
        if line.is_empty() {
            continue;
        }
        let is_tracking = TRACKING_PATTERNS.iter().any(|p| p.is_match(line));
        if is_tracking {
            content_end = i;
        } else {
            break;
        }
    }

    lines[..content_end].join("\n")
}
```

### 1.7 Testing Strategy

Write integration tests with real-world email samples:

`crates/reader/tests/integration.rs`:
```rust
use mxr_reader::{clean, ReaderConfig};

#[test]
fn newsletter_stripped_to_content() {
    let html = include_str!("fixtures/newsletter.html");
    let output = clean(None, Some(html), &ReaderConfig::default());
    assert!(output.cleaned_lines < output.original_lines);
    assert!(output.content.len() > 0);
    // Should not contain tracking junk
    assert!(!output.content.to_lowercase().contains("view in browser"));
    assert!(!output.content.to_lowercase().contains("unsubscribe"));
}

#[test]
fn plain_email_with_signature() {
    let text = "Hey,\n\nCan we meet tomorrow at 3pm?\n\nThanks,\n-- \nAlice\nSenior Engineer\n+1 555-0123\nalice@company.com";
    let output = clean(Some(text), None, &ReaderConfig::default());
    assert_eq!(output.content.trim(), "Hey,\n\nCan we meet tomorrow at 3pm?\n\nThanks,");
    assert!(output.signature.is_some());
}

#[test]
fn reader_mode_stats_correct() {
    let text = "Content here.\n\nOn Mon, alice wrote:\n> Long quote\n> Another line\n> And more\n\n-- \nSig line\nPhone: 555-0123";
    let output = clean(Some(text), None, &ReaderConfig::default());
    assert!(output.original_lines > output.cleaned_lines);
}
```

Create `crates/reader/tests/fixtures/` directory with sample HTML newsletters for testing. Collect 5-10 real newsletter HTML samples (anonymized).

---

## Step 2: mxr-compose Crate

### 2.1 Crate Setup

Add to workspace `Cargo.toml`:

```toml
[workspace]
members = [
    # ... existing members
    "crates/compose",
]

[workspace.dependencies]
mxr-compose = { path = "crates/compose" }
serde_yaml = "0.9"
comrak = { version = "0.31", default-features = false, features = ["shortcodes"] }
```

`crates/compose/Cargo.toml`:
```toml
[package]
name = "mxr-compose"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
mxr-core = { workspace = true }
mxr-reader = { workspace = true }
serde = { workspace = true }
serde_yaml = { workspace = true }
serde_json = { workspace = true }
comrak = { workspace = true }
uuid = { workspace = true }
chrono = { workspace = true }
thiserror = { workspace = true }
tracing = { workspace = true }
tokio = { workspace = true }

[dev-dependencies]
tempfile = "3"
pretty_assertions = "1"
```

### 2.2 Draft File Format and Parsing

`crates/compose/src/frontmatter.rs`:
```rust
use serde::{Deserialize, Serialize};

/// YAML frontmatter for compose files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeFrontmatter {
    pub to: String,
    #[serde(default)]
    pub cc: String,
    #[serde(default)]
    pub bcc: String,
    pub subject: String,
    pub from: String,
    #[serde(default, rename = "in-reply-to", skip_serializing_if = "Option::is_none")]
    pub in_reply_to: Option<String>,
    #[serde(default)]
    pub attach: Vec<String>,
}

const FRONTMATTER_DELIMITER: &str = "---";
const CONTEXT_MARKER: &str = "# --- context (stripped before sending) ---";

/// Parse a compose file into frontmatter + body.
/// Strips the context block (everything after the context marker).
pub fn parse_compose_file(content: &str) -> Result<(ComposeFrontmatter, String), ComposeError> {
    let content = content.trim_start();

    // Extract YAML frontmatter between --- delimiters
    if !content.starts_with(FRONTMATTER_DELIMITER) {
        return Err(ComposeError::MissingFrontmatter);
    }

    let after_first = &content[FRONTMATTER_DELIMITER.len()..];
    let end_pos = after_first
        .find(&format!("\n{FRONTMATTER_DELIMITER}"))
        .ok_or(ComposeError::MissingFrontmatter)?;

    let yaml_str = &after_first[..end_pos];
    let rest = &after_first[end_pos + 1 + FRONTMATTER_DELIMITER.len()..];

    let frontmatter: ComposeFrontmatter =
        serde_yaml::from_str(yaml_str).map_err(|e| ComposeError::InvalidFrontmatter(e.to_string()))?;

    // Strip context block
    let body = if let Some(ctx_pos) = rest.find(CONTEXT_MARKER) {
        rest[..ctx_pos].to_string()
    } else {
        rest.to_string()
    };

    let body = body.trim().to_string();

    Ok((frontmatter, body))
}

/// Generate a compose file string from frontmatter + body + optional context.
pub fn render_compose_file(
    frontmatter: &ComposeFrontmatter,
    body: &str,
    context: Option<&str>,
) -> Result<String, ComposeError> {
    let yaml = serde_yaml::to_string(frontmatter)
        .map_err(|e| ComposeError::InvalidFrontmatter(e.to_string()))?;

    let mut output = format!("---\n{yaml}---\n\n{body}");

    if let Some(ctx) = context {
        output.push_str("\n\n");
        output.push_str(CONTEXT_MARKER);
        output.push('\n');
        for line in ctx.lines() {
            output.push_str(&format!("# {line}\n"));
        }
    }

    Ok(output)
}

#[derive(Debug, thiserror::Error)]
pub enum ComposeError {
    #[error("Missing YAML frontmatter delimiters (---)")]
    MissingFrontmatter,
    #[error("Invalid frontmatter: {0}")]
    InvalidFrontmatter(String),
    #[error("Attachment not found: {0}")]
    AttachmentNotFound(String),
    #[error("No recipients specified")]
    NoRecipients,
    #[error("Editor failed: {0}")]
    EditorFailed(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
```

### 2.3 Editor Spawning

`crates/compose/src/editor.rs`:
```rust
use crate::frontmatter::ComposeError;
use std::path::Path;
use tokio::process::Command;

/// Resolve which editor to use.
/// Priority: $EDITOR -> $VISUAL -> config_editor -> vi
pub fn resolve_editor(config_editor: Option<&str>) -> String {
    std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .unwrap_or_else(|_| {
            config_editor
                .map(|s| s.to_string())
                .unwrap_or_else(|| "vi".to_string())
        })
}

/// Spawn the editor and wait for it to exit.
/// For vim/neovim, positions cursor at the given line number.
pub async fn spawn_editor(
    editor: &str,
    file_path: &Path,
    cursor_line: Option<usize>,
) -> Result<bool, ComposeError> {
    let mut cmd = Command::new(editor);

    // Position cursor for vim/neovim/vi
    if let Some(line) = cursor_line {
        let editor_lower = editor.to_lowercase();
        if editor_lower.contains("vim") || editor_lower.contains("vi") || editor_lower.contains("nvim") {
            cmd.arg(format!("+{line}"));
        } else if editor_lower.contains("hx") || editor_lower.contains("helix") {
            // Helix uses file:line syntax
            let path_str = format!("{}:{line}", file_path.display());
            let status = Command::new(editor)
                .arg(&path_str)
                .status()
                .await
                .map_err(|e| ComposeError::EditorFailed(e.to_string()))?;
            return Ok(status.success());
        }
        // Other editors: just open the file without cursor positioning
    }

    cmd.arg(file_path);

    let status = cmd
        .status()
        .await
        .map_err(|e| ComposeError::EditorFailed(e.to_string()))?;

    Ok(status.success())
}
```

### 2.4 Markdown to Multipart Rendering

`crates/compose/src/render.rs`:
```rust
use comrak::{markdown_to_html, Options};

/// Minimal HTML template wrapping rendered markdown.
const HTML_TEMPLATE: &str = r#"<!DOCTYPE html>
<html>
<head><meta charset="utf-8"></head>
<body style="font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; font-size: 14px; line-height: 1.5; color: #333; max-width: 600px;">
{content}
</body>
</html>"#;

/// Rendered email parts from markdown source.
pub struct RenderedMessage {
    /// Raw markdown as text/plain part.
    pub plain: String,
    /// Rendered HTML as text/html part.
    pub html: String,
}

/// Convert markdown body to multipart-ready text/plain + text/html.
pub fn render_markdown(markdown: &str) -> RenderedMessage {
    let mut options = Options::default();
    options.extension.strikethrough = true;
    options.extension.table = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    options.render.unsafe_ = false; // Don't allow raw HTML in markdown

    let html_content = markdown_to_html(markdown, &options);
    let html = HTML_TEMPLATE.replace("{content}", &html_content);

    RenderedMessage {
        plain: markdown.to_string(),
        html,
    }
}
```

### 2.5 Attachment Handling

`crates/compose/src/attachments.rs`:
```rust
use crate::frontmatter::ComposeError;
use std::path::{Path, PathBuf};

/// Resolve and validate attachment paths from frontmatter.
/// Supports tilde expansion.
pub fn resolve_attachments(paths: &[String]) -> Result<Vec<ResolvedAttachment>, ComposeError> {
    paths.iter().map(|p| resolve_one(p)).collect()
}

pub struct ResolvedAttachment {
    pub path: PathBuf,
    pub filename: String,
    pub mime_type: String,
}

fn resolve_one(path_str: &str) -> Result<ResolvedAttachment, ComposeError> {
    let expanded = expand_tilde(path_str);
    let path = PathBuf::from(&expanded);

    if !path.exists() {
        return Err(ComposeError::AttachmentNotFound(path_str.to_string()));
    }

    let filename = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "attachment".to_string());

    // Simple MIME type detection by extension
    let mime_type = match path.extension().and_then(|e| e.to_str()) {
        Some("pdf") => "application/pdf",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("txt") => "text/plain",
        Some("csv") => "text/csv",
        Some("html" | "htm") => "text/html",
        Some("zip") => "application/zip",
        Some("doc") => "application/msword",
        Some("docx") => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        Some("xls") => "application/vnd.ms-excel",
        Some("xlsx") => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        _ => "application/octet-stream",
    }
    .to_string();

    Ok(ResolvedAttachment {
        path,
        filename,
        mime_type,
    })
}

fn expand_tilde(path: &str) -> String {
    if path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return format!("{}{}", home.display(), &path[1..]);
        }
    }
    path.to_string()
}
```

### 2.6 Compose Orchestration

`crates/compose/src/lib.rs`:
```rust
pub mod attachments;
pub mod editor;
pub mod frontmatter;
pub mod render;

use crate::frontmatter::{ComposeFrontmatter, ComposeError};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// The kind of compose action.
pub enum ComposeKind {
    New,
    Reply {
        in_reply_to: String,
        to: String,
        cc: String,
        subject: String,
        /// Reader-mode-cleaned thread content for context block.
        thread_context: String,
    },
    Forward {
        subject: String,
        /// Reader-mode-cleaned original message for context block.
        original_context: String,
    },
}

/// Create a draft file on disk and return its path + the cursor line.
pub fn create_draft_file(
    kind: ComposeKind,
    from: &str,
) -> Result<(PathBuf, usize), ComposeError> {
    let draft_id = Uuid::now_v7();
    let path = std::env::temp_dir().join(format!("mxr-draft-{draft_id}.md"));

    let (frontmatter, body, context) = match kind {
        ComposeKind::New => {
            let fm = ComposeFrontmatter {
                to: String::new(),
                cc: String::new(),
                bcc: String::new(),
                subject: String::new(),
                from: from.to_string(),
                in_reply_to: None,
                attach: Vec::new(),
            };
            (fm, String::new(), None)
        }
        ComposeKind::Reply {
            in_reply_to,
            to,
            cc,
            subject,
            thread_context,
        } => {
            let fm = ComposeFrontmatter {
                to,
                cc,
                bcc: String::new(),
                subject: format!("Re: {subject}"),
                from: from.to_string(),
                in_reply_to: Some(in_reply_to),
                attach: Vec::new(),
            };
            (fm, String::new(), Some(thread_context))
        }
        ComposeKind::Forward {
            subject,
            original_context,
        } => {
            let fm = ComposeFrontmatter {
                to: String::new(),
                cc: String::new(),
                bcc: String::new(),
                subject: format!("Fwd: {subject}"),
                from: from.to_string(),
                in_reply_to: None,
                attach: Vec::new(),
            };
            let body = "---------- Forwarded message ----------".to_string();
            (fm, body, Some(original_context))
        }
    };

    let content = frontmatter::render_compose_file(&frontmatter, &body, context.as_deref())?;

    // Calculate cursor line: first empty line after frontmatter closing ---
    let cursor_line = content
        .lines()
        .enumerate()
        .skip(1) // skip opening ---
        .find_map(|(i, line)| {
            if line == "---" {
                Some(i + 2) // line after ---, 1-indexed, +1 for blank line
            } else {
                None
            }
        })
        .unwrap_or(1);

    std::fs::write(&path, &content)?;

    Ok((path, cursor_line))
}

/// Validate a parsed draft before sending.
pub fn validate_draft(
    frontmatter: &ComposeFrontmatter,
    body: &str,
) -> Vec<ComposeValidation> {
    let mut issues = Vec::new();

    if frontmatter.to.trim().is_empty() {
        issues.push(ComposeValidation::Error("No recipients (to: field is empty)".into()));
    }

    if frontmatter.subject.trim().is_empty() {
        issues.push(ComposeValidation::Warning("Subject is empty".into()));
    }

    if body.trim().is_empty() {
        issues.push(ComposeValidation::Warning("Message body is empty".into()));
    }

    // Validate email addresses in to/cc/bcc
    for addr in frontmatter.to.split(',').chain(frontmatter.cc.split(',')).chain(frontmatter.bcc.split(',')) {
        let addr = addr.trim();
        if !addr.is_empty() && !addr.contains('@') {
            issues.push(ComposeValidation::Error(format!("Invalid email address: {addr}")));
        }
    }

    issues
}

pub enum ComposeValidation {
    Error(String),
    Warning(String),
}

impl ComposeValidation {
    pub fn is_error(&self) -> bool {
        matches!(self, ComposeValidation::Error(_))
    }
}
```

### 2.7 Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use frontmatter::parse_compose_file;

    #[test]
    fn roundtrip_new_message() {
        let (path, cursor) = create_draft_file(ComposeKind::New, "me@example.com").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        let (fm, body) = parse_compose_file(&content).unwrap();
        assert_eq!(fm.from, "me@example.com");
        assert!(fm.to.is_empty());
        assert!(body.is_empty());
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn roundtrip_reply() {
        let (path, _) = create_draft_file(
            ComposeKind::Reply {
                in_reply_to: "<msg-123@example.com>".into(),
                to: "alice@example.com".into(),
                cc: "bob@example.com".into(),
                subject: "Deployment plan".into(),
                thread_context: "From: alice@example.com\nDate: 2026-03-15\n\nHey team, what's the plan?".into(),
            },
            "me@example.com",
        )
        .unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        let (fm, body) = parse_compose_file(&content).unwrap();
        assert_eq!(fm.subject, "Re: Deployment plan");
        assert_eq!(fm.to, "alice@example.com");
        assert!(fm.in_reply_to.is_some());
        // Context block should be stripped by parser
        assert!(!body.contains("what's the plan?"));
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn context_block_stripped() {
        let content = "---\nto: alice@example.com\nsubject: test\nfrom: me@example.com\n---\n\nHello!\n\n# --- context (stripped before sending) ---\n# Some context here\n# More context";
        let (fm, body) = parse_compose_file(content).unwrap();
        assert_eq!(body, "Hello!");
        assert!(!body.contains("context"));
    }

    #[test]
    fn validates_missing_recipient() {
        let fm = ComposeFrontmatter {
            to: String::new(),
            cc: String::new(),
            bcc: String::new(),
            subject: "Test".into(),
            from: "me@example.com".into(),
            in_reply_to: None,
            attach: Vec::new(),
        };
        let issues = validate_draft(&fm, "body");
        assert!(issues.iter().any(|i| i.is_error()));
    }
}
```

---

## Step 3: mxr-provider-smtp Crate

### 3.1 Crate Setup

Add to workspace `Cargo.toml`:

```toml
[workspace]
members = [
    # ... existing members
    "crates/provider-smtp",
]

[workspace.dependencies]
mxr-provider-smtp = { path = "crates/provider-smtp" }
lettre = { version = "0.11", features = ["tokio1-rustls-tls", "builder"] }
keyring = { version = "3", features = ["apple-native", "linux-native"] }
```

`crates/provider-smtp/Cargo.toml`:
```toml
[package]
name = "mxr-provider-smtp"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
mxr-core = { workspace = true }
mxr-compose = { workspace = true }
lettre = { workspace = true }
keyring = { workspace = true }
serde = { workspace = true }
tokio = { workspace = true }
thiserror = { workspace = true }
tracing = { workspace = true }
async-trait = { workspace = true }

[dev-dependencies]
tokio = { workspace = true, features = ["test-util"] }
```

### 3.2 SMTP Config

`crates/provider-smtp/src/config.rs`:
```rust
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    /// Keyring reference (e.g., "mxr/work-smtp"). Looked up at runtime.
    pub password_ref: String,
    #[serde(default = "default_true")]
    pub use_tls: bool,
}

fn default_true() -> bool {
    true
}

impl SmtpConfig {
    /// Retrieve the SMTP password from the system keyring.
    pub fn resolve_password(&self) -> Result<String, SmtpError> {
        let entry = keyring::Entry::new(&self.password_ref, &self.username)
            .map_err(|e| SmtpError::Keyring(e.to_string()))?;
        entry
            .get_password()
            .map_err(|e| SmtpError::Keyring(format!("Failed to retrieve password from keyring: {e}")))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SmtpError {
    #[error("Keyring error: {0}")]
    Keyring(String),
    #[error("SMTP transport error: {0}")]
    Transport(String),
    #[error("Message build error: {0}")]
    MessageBuild(String),
}
```

### 3.3 MailSendProvider Implementation

`crates/provider-smtp/src/lib.rs`:
```rust
pub mod config;

use async_trait::async_trait;
use config::{SmtpConfig, SmtpError};
use lettre::{
    message::{header::ContentType, Attachment, MultiPart, SinglePart, Mailbox},
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use mxr_core::provider::{MailSendProvider, SendReceipt};
use mxr_core::types::Draft;

pub struct SmtpSendProvider {
    config: SmtpConfig,
}

impl SmtpSendProvider {
    pub fn new(config: SmtpConfig) -> Self {
        Self { config }
    }

    async fn build_transport(
        &self,
    ) -> Result<AsyncSmtpTransport<Tokio1Executor>, SmtpError> {
        let password = self.config.resolve_password()?;
        let creds = Credentials::new(self.config.username.clone(), password);

        let transport = if self.config.use_tls {
            if self.config.port == 465 {
                // Implicit TLS
                AsyncSmtpTransport::<Tokio1Executor>::relay(&self.config.host)
                    .map_err(|e| SmtpError::Transport(e.to_string()))?
                    .port(self.config.port)
                    .credentials(creds)
                    .build()
            } else {
                // STARTTLS (port 587)
                AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&self.config.host)
                    .map_err(|e| SmtpError::Transport(e.to_string()))?
                    .port(self.config.port)
                    .credentials(creds)
                    .build()
            }
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&self.config.host)
                .port(self.config.port)
                .credentials(creds)
                .build()
        };

        Ok(transport)
    }
}

#[async_trait]
impl MailSendProvider for SmtpSendProvider {
    fn name(&self) -> &str {
        "smtp"
    }

    async fn send(
        &self,
        draft: &Draft,
        from: &mxr_core::types::Address,
    ) -> Result<SendReceipt, mxr_core::error::Error> {
        let transport = self
            .build_transport()
            .await
            .map_err(|e| mxr_core::error::Error::Provider(e.to_string()))?;

        let from_mailbox: Mailbox = from
            .email
            .parse()
            .map_err(|e: lettre::address::AddressError| {
                mxr_core::error::Error::Provider(format!("Invalid from address: {e}"))
            })?;

        let mut builder = Message::builder().from(from_mailbox);

        // Add To recipients
        for addr in draft.to.split(',') {
            let addr = addr.trim();
            if !addr.is_empty() {
                let mailbox: Mailbox = addr.parse().map_err(|e: lettre::address::AddressError| {
                    mxr_core::error::Error::Provider(format!("Invalid to address: {e}"))
                })?;
                builder = builder.to(mailbox);
            }
        }

        // Add Cc recipients
        for addr in draft.cc.split(',') {
            let addr = addr.trim();
            if !addr.is_empty() {
                let mailbox: Mailbox = addr.parse().map_err(|e: lettre::address::AddressError| {
                    mxr_core::error::Error::Provider(format!("Invalid cc address: {e}"))
                })?;
                builder = builder.cc(mailbox);
            }
        }

        // Add Bcc recipients
        for addr in draft.bcc.split(',') {
            let addr = addr.trim();
            if !addr.is_empty() {
                let mailbox: Mailbox = addr.parse().map_err(|e: lettre::address::AddressError| {
                    mxr_core::error::Error::Provider(format!("Invalid bcc address: {e}"))
                })?;
                builder = builder.bcc(mailbox);
            }
        }

        builder = builder.subject(&draft.subject);

        if let Some(ref reply_to) = draft.in_reply_to {
            builder = builder.header(lettre::message::header::InReplyTo::new(reply_to.clone()));
        }

        // Build multipart body (text/plain + text/html)
        let rendered = mxr_compose::render::render_markdown(&draft.body);
        let mut multipart = MultiPart::alternative()
            .singlepart(
                SinglePart::builder()
                    .header(ContentType::TEXT_PLAIN)
                    .body(rendered.plain),
            )
            .singlepart(
                SinglePart::builder()
                    .header(ContentType::TEXT_HTML)
                    .body(rendered.html),
            );

        // TODO: Attach files from draft.attachments
        // For each attachment, read file bytes and add as attachment part

        let message = builder
            .multipart(multipart)
            .map_err(|e| mxr_core::error::Error::Provider(format!("Failed to build message: {e}")))?;

        transport
            .send(message)
            .await
            .map_err(|e| mxr_core::error::Error::Provider(format!("SMTP send failed: {e}")))?;

        Ok(SendReceipt {
            provider_message_id: None,
            sent_at: chrono::Utc::now(),
        })
    }
}
```

---

## Step 4: Gmail Send

### 4.1 RFC 2822 Message Building

Add to the existing `mxr-provider-gmail` crate:

`crates/provider-gmail/src/send.rs`:
```rust
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use mxr_compose::render::render_markdown;
use mxr_core::types::Draft;

/// Build an RFC 2822 message from a Draft and encode as base64url for Gmail API.
pub fn build_rfc2822(draft: &Draft, from: &str) -> Result<String, GmailSendError> {
    let rendered = render_markdown(&draft.body);

    let mut headers = Vec::new();
    headers.push(format!("From: {from}"));
    headers.push(format!("To: {}", draft.to));

    if !draft.cc.is_empty() {
        headers.push(format!("Cc: {}", draft.cc));
    }

    headers.push(format!("Subject: {}", draft.subject));
    headers.push(format!("Date: {}", chrono::Utc::now().to_rfc2822()));
    headers.push(format!("Message-ID: <{}.mxr@localhost>", uuid::Uuid::now_v7()));
    headers.push("MIME-Version: 1.0".to_string());

    if let Some(ref reply_to) = draft.in_reply_to {
        headers.push(format!("In-Reply-To: {reply_to}"));
        headers.push(format!("References: {reply_to}"));
    }

    // Generate MIME boundary
    let boundary = format!("mxr-{}", uuid::Uuid::now_v7());
    headers.push(format!(
        "Content-Type: multipart/alternative; boundary=\"{boundary}\""
    ));

    let mut message = headers.join("\r\n");
    message.push_str("\r\n\r\n");

    // text/plain part
    message.push_str(&format!("--{boundary}\r\n"));
    message.push_str("Content-Type: text/plain; charset=utf-8\r\n");
    message.push_str("Content-Transfer-Encoding: quoted-printable\r\n\r\n");
    message.push_str(&rendered.plain);
    message.push_str("\r\n");

    // text/html part
    message.push_str(&format!("--{boundary}\r\n"));
    message.push_str("Content-Type: text/html; charset=utf-8\r\n");
    message.push_str("Content-Transfer-Encoding: quoted-printable\r\n\r\n");
    message.push_str(&rendered.html);
    message.push_str("\r\n");

    message.push_str(&format!("--{boundary}--\r\n"));

    Ok(message)
}

/// Encode an RFC 2822 message as base64url for Gmail API.
pub fn encode_for_gmail(rfc2822: &str) -> String {
    URL_SAFE_NO_PAD.encode(rfc2822.as_bytes())
}

#[derive(Debug, thiserror::Error)]
pub enum GmailSendError {
    #[error("Failed to build message: {0}")]
    Build(String),
    #[error("Gmail API error: {0}")]
    Api(String),
}
```

### 4.2 Gmail MailSendProvider

Add to existing Gmail provider's `MailSendProvider` implementation:

```rust
#[async_trait]
impl MailSendProvider for GmailProvider {
    fn name(&self) -> &str {
        "gmail"
    }

    async fn send(
        &self,
        draft: &Draft,
        from: &Address,
    ) -> Result<SendReceipt, Error> {
        let rfc2822 = send::build_rfc2822(draft, &from.email)
            .map_err(|e| Error::Provider(e.to_string()))?;
        let encoded = send::encode_for_gmail(&rfc2822);

        let token = self.get_access_token().await?;
        let url = "https://gmail.googleapis.com/gmail/v1/users/me/messages/send";

        let resp = self
            .client
            .post(url)
            .bearer_auth(&token)
            .json(&serde_json::json!({ "raw": encoded }))
            .send()
            .await
            .map_err(|e| Error::Provider(format!("Gmail send request failed: {e}")))?;

        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Provider(format!("Gmail send failed: {body}")));
        }

        let result: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| Error::Provider(format!("Failed to parse send response: {e}")))?;

        let message_id = result["id"].as_str().map(|s| s.to_string());

        Ok(SendReceipt {
            provider_message_id: message_id,
            sent_at: chrono::Utc::now(),
        })
    }
}
```

### 4.3 Store Sent Message Locally

After a successful send, the daemon should:

```rust
/// Called by daemon after successful send via any provider.
async fn on_message_sent(
    store: &Store,
    search: &SearchIndex,
    draft: &Draft,
    receipt: &SendReceipt,
    account_id: &AccountId,
) -> Result<()> {
    // Insert envelope for sent message
    let envelope = Envelope {
        id: MessageId::new(),
        account_id: account_id.clone(),
        provider_message_id: receipt.provider_message_id.clone().unwrap_or_default(),
        thread_id: None, // Will be resolved on next sync
        subject: draft.subject.clone(),
        from: draft.from.clone(),
        to: draft.to.clone(),
        cc: draft.cc.clone(),
        date: receipt.sent_at,
        labels: vec!["SENT".to_string()],
        flags: MessageFlags::empty(),
        snippet: draft.body.chars().take(200).collect(),
        unsubscribe_method: None,
    };

    store.insert_envelope(&envelope).await?;
    search.index_envelope(&envelope).await?;

    // Remove draft from SQLite
    store.delete_draft(&draft.id).await?;

    Ok(())
}
```

---

## Step 5: Gmail Mutations

### 5.1 Mutation Methods on Gmail Provider

Add to `crates/provider-gmail/src/mutations.rs`:

```rust
use mxr_core::error::Error;

impl GmailProvider {
    /// Base URL for Gmail API.
    const BASE: &'static str = "https://gmail.googleapis.com/gmail/v1/users/me";

    /// Modify labels on a single message.
    async fn modify_message_labels(
        &self,
        message_id: &str,
        add: &[&str],
        remove: &[&str],
    ) -> Result<(), Error> {
        let token = self.get_access_token().await?;
        let url = format!("{}/messages/{message_id}/modify", Self::BASE);

        let body = serde_json::json!({
            "addLabelIds": add,
            "removeLabelIds": remove,
        });

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Provider(format!("Label modify failed: {e}")))?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(Error::Provider(format!("Gmail modify failed: {text}")));
        }

        Ok(())
    }

    /// Batch modify labels on multiple messages.
    async fn batch_modify_labels(
        &self,
        message_ids: &[String],
        add: &[&str],
        remove: &[&str],
    ) -> Result<(), Error> {
        let token = self.get_access_token().await?;
        let url = format!("{}/messages/batchModify", Self::BASE);

        let body = serde_json::json!({
            "ids": message_ids,
            "addLabelIds": add,
            "removeLabelIds": remove,
        });

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Provider(format!("Batch modify failed: {e}")))?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(Error::Provider(format!("Gmail batch modify failed: {text}")));
        }

        Ok(())
    }
}
```

### 5.2 MailSyncProvider Mutation Trait Methods

Implement the mutation methods on the `MailSyncProvider` trait for Gmail:

```rust
#[async_trait]
impl MailSyncProvider for GmailProvider {
    // ... existing sync methods ...

    async fn modify_labels(
        &self,
        provider_message_id: &str,
        add: &[String],
        remove: &[String],
    ) -> Result<()> {
        let add_refs: Vec<&str> = add.iter().map(|s| s.as_str()).collect();
        let remove_refs: Vec<&str> = remove.iter().map(|s| s.as_str()).collect();
        self.modify_message_labels(provider_message_id, &add_refs, &remove_refs)
            .await
    }

    async fn trash(&self, provider_message_id: &str) -> Result<()> {
        let token = self.get_access_token().await?;
        let url = format!("{}/messages/{provider_message_id}/trash", Self::BASE);

        let resp = self
            .client
            .post(&url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| Error::Provider(format!("Trash failed: {e}")))?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(Error::Provider(format!("Gmail trash failed: {text}")));
        }

        Ok(())
    }

    async fn set_read(&self, provider_message_id: &str, read: bool) -> Result<()> {
        if read {
            self.modify_message_labels(provider_message_id, &[], &["UNREAD"]).await
        } else {
            self.modify_message_labels(provider_message_id, &["UNREAD"], &[]).await
        }
    }

    async fn set_starred(&self, provider_message_id: &str, starred: bool) -> Result<()> {
        if starred {
            self.modify_message_labels(provider_message_id, &["STARRED"], &[]).await
        } else {
            self.modify_message_labels(provider_message_id, &[], &["STARRED"]).await
        }
    }
}
```

### 5.3 Daemon Mutation Handler

In the daemon, mutations flow through a handler that:
1. Calls the provider mutation
2. Updates local SQLite
3. Updates the Tantivy index
4. Notifies the TUI

```rust
/// Handle a mutation command from the TUI.
async fn handle_mutation(
    store: &Store,
    search: &SearchIndex,
    provider: &dyn MailSyncProvider,
    cmd: MutationCommand,
) -> Result<MutationResult> {
    match cmd {
        MutationCommand::Archive { message_ids } => {
            let provider_ids = store.get_provider_ids(&message_ids).await?;
            if provider.capabilities().batch_operations && provider_ids.len() > 1 {
                provider
                    .batch_modify_labels(&provider_ids, &[], &["INBOX"])
                    .await?;
            } else {
                for pid in &provider_ids {
                    provider.modify_labels(pid, &[], &["INBOX".into()]).await?;
                }
            }
            // Update local store
            for id in &message_ids {
                store.remove_label(id, "INBOX").await?;
                search.update_labels(id, &store.get_labels(id).await?).await?;
            }
            Ok(MutationResult::Success)
        }

        MutationCommand::Trash { message_ids } => {
            for id in &message_ids {
                let pid = store.get_provider_id(id).await?;
                provider.trash(&pid).await?;
                store.move_to_trash(id).await?;
                search.remove_document(id).await?;
            }
            Ok(MutationResult::Success)
        }

        MutationCommand::Star { message_id, starred } => {
            let pid = store.get_provider_id(&message_id).await?;
            provider.set_starred(&pid, starred).await?;
            store.set_starred(&message_id, starred).await?;
            Ok(MutationResult::Success)
        }

        MutationCommand::SetRead { message_id, read } => {
            let pid = store.get_provider_id(&message_id).await?;
            provider.set_read(&pid, read).await?;
            store.set_read(&message_id, read).await?;
            Ok(MutationResult::Success)
        }

        MutationCommand::ModifyLabels { message_id, add, remove } => {
            let pid = store.get_provider_id(&message_id).await?;
            provider.modify_labels(&pid, &add, &remove).await?;
            store.modify_labels(&message_id, &add, &remove).await?;
            search.update_labels(&message_id, &store.get_labels(&message_id).await?).await?;
            Ok(MutationResult::Success)
        }
    }
}
```

---

## Step 6: One-Key Unsubscribe

### 6.1 Unsubscribe Execution

Add to `crates/daemon/src/unsubscribe.rs`:

```rust
use mxr_core::types::UnsubscribeMethod;
use reqwest::Client;

/// Execute an unsubscribe action.
pub async fn execute_unsubscribe(
    method: &UnsubscribeMethod,
    client: &Client,
    send_provider: Option<&dyn MailSendProvider>,
) -> Result<UnsubscribeResult, Error> {
    match method {
        UnsubscribeMethod::OneClick { url } => {
            // RFC 8058: POST with List-Unsubscribe=One-Click body
            let resp = client
                .post(url)
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body("List-Unsubscribe=One-Click")
                .send()
                .await
                .map_err(|e| Error::Provider(format!("One-click unsubscribe failed: {e}")))?;

            if resp.status().is_success() {
                Ok(UnsubscribeResult::Success("Unsubscribed via one-click.".into()))
            } else {
                Ok(UnsubscribeResult::Failed(format!(
                    "One-click POST returned {}",
                    resp.status()
                )))
            }
        }

        UnsubscribeMethod::Mailto { address, subject } => {
            // Auto-send an unsubscribe email
            if let Some(provider) = send_provider {
                let draft = Draft {
                    to: address.clone(),
                    subject: subject
                        .clone()
                        .unwrap_or_else(|| "unsubscribe".to_string()),
                    body: "unsubscribe".to_string(),
                    ..Default::default()
                };
                provider.send(&draft, &draft.from).await?;
                Ok(UnsubscribeResult::Success("Unsubscribe email sent.".into()))
            } else {
                Ok(UnsubscribeResult::Failed(
                    "No send provider configured for mailto unsubscribe".into(),
                ))
            }
        }

        UnsubscribeMethod::HttpLink { url } | UnsubscribeMethod::BodyLink { url } => {
            // Open in browser
            open_in_browser(url)?;
            Ok(UnsubscribeResult::Success(
                "Opened unsubscribe page in browser.".into(),
            ))
        }

        UnsubscribeMethod::None => {
            Ok(UnsubscribeResult::NoMethod)
        }
    }
}

pub enum UnsubscribeResult {
    Success(String),
    Failed(String),
    NoMethod,
}

fn open_in_browser(url: &str) -> Result<(), Error> {
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(target_os = "linux")]
    let cmd = "xdg-open";
    #[cfg(target_os = "windows")]
    let cmd = "start";

    std::process::Command::new(cmd)
        .arg(url)
        .spawn()
        .map_err(|e| Error::Provider(format!("Failed to open browser: {e}")))?;

    Ok(())
}
```

### 6.2 TUI Unsubscribe Flow

In the TUI event handler:

```rust
Action::Unsubscribe => {
    if let Some(msg) = self.selected_message() {
        match &msg.unsubscribe_method {
            UnsubscribeMethod::None => {
                self.show_status("No unsubscribe option found for this message");
            }
            method => {
                let sender = msg.from.clone();
                self.show_confirm(
                    &format!("Unsubscribe from {sender}?"),
                    |confirmed, app| {
                        if confirmed {
                            app.send_command(Command::Unsubscribe {
                                message_id: msg.id.clone(),
                            });
                        }
                    },
                );
            }
        }
    }
}
```

### 6.3 Visual Indicator

In the message list rendering:

```rust
fn render_message_row(&self, msg: &Envelope, area: Rect, buf: &mut Buffer) {
    // ... existing rendering ...

    // Show [U] indicator for messages with unsubscribe method
    if !matches!(msg.unsubscribe_method, UnsubscribeMethod::None) {
        let indicator = Span::styled("[U]", Style::default().fg(Color::DarkGray));
        // Render indicator at appropriate position
    }
}
```

---

## Step 7: Local Snooze with Gmail Integration

### 7.1 Snooze Table Migration

```sql
CREATE TABLE IF NOT EXISTS snoozed (
    message_id TEXT PRIMARY KEY REFERENCES envelopes(id),
    account_id TEXT NOT NULL REFERENCES accounts(id),
    provider_message_id TEXT NOT NULL,
    wake_at TEXT NOT NULL,  -- ISO 8601 datetime
    original_labels TEXT NOT NULL,  -- JSON array of label strings
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_snoozed_wake_at ON snoozed(wake_at);
```

### 7.2 Snooze Options

```rust
use chrono::{DateTime, Datelike, Duration, Local, NaiveTime, Utc, Weekday};

#[derive(Debug, Clone)]
pub enum SnoozeOption {
    TomorrowMorning,
    NextMonday,
    Weekend,
    Tonight,
    Custom(DateTime<Utc>),
}

/// Resolve a snooze option to a concrete wake time.
pub fn resolve_snooze_time(
    option: SnoozeOption,
    config: &SnoozeConfig,
) -> DateTime<Utc> {
    let now = Local::now();

    match option {
        SnoozeOption::TomorrowMorning => {
            let tomorrow = now.date_naive() + Duration::days(1);
            let time = NaiveTime::from_hms_opt(config.morning_hour as u32, 0, 0).unwrap();
            tomorrow
                .and_time(time)
                .and_local_timezone(now.timezone())
                .single()
                .unwrap()
                .with_timezone(&Utc)
        }
        SnoozeOption::NextMonday => {
            let days_until_monday = (Weekday::Mon.num_days_from_monday() as i64
                - now.weekday().num_days_from_monday() as i64
                + 7)
                % 7;
            let days = if days_until_monday == 0 { 7 } else { days_until_monday };
            let monday = now.date_naive() + Duration::days(days);
            let time = NaiveTime::from_hms_opt(config.morning_hour as u32, 0, 0).unwrap();
            monday
                .and_time(time)
                .and_local_timezone(now.timezone())
                .single()
                .unwrap()
                .with_timezone(&Utc)
        }
        SnoozeOption::Weekend => {
            let target_day = match config.weekend_day.as_str() {
                "sunday" => Weekday::Sun,
                _ => Weekday::Sat,
            };
            let days_until = (target_day.num_days_from_monday() as i64
                - now.weekday().num_days_from_monday() as i64
                + 7)
                % 7;
            let days = if days_until == 0 { 7 } else { days_until };
            let weekend = now.date_naive() + Duration::days(days);
            let time = NaiveTime::from_hms_opt(config.weekend_hour as u32, 0, 0).unwrap();
            weekend
                .and_time(time)
                .and_local_timezone(now.timezone())
                .single()
                .unwrap()
                .with_timezone(&Utc)
        }
        SnoozeOption::Tonight => {
            let today = now.date_naive();
            let time = NaiveTime::from_hms_opt(config.evening_hour as u32, 0, 0).unwrap();
            let tonight = today
                .and_time(time)
                .and_local_timezone(now.timezone())
                .single()
                .unwrap()
                .with_timezone(&Utc);
            // If it's already past the evening hour, snooze to tomorrow evening
            if tonight <= Utc::now() {
                tonight + Duration::days(1)
            } else {
                tonight
            }
        }
        SnoozeOption::Custom(dt) => dt,
    }
}

#[derive(Debug, Clone)]
pub struct SnoozeConfig {
    pub morning_hour: u8,
    pub evening_hour: u8,
    pub weekend_day: String,
    pub weekend_hour: u8,
}

impl Default for SnoozeConfig {
    fn default() -> Self {
        Self {
            morning_hour: 9,
            evening_hour: 18,
            weekend_day: "saturday".into(),
            weekend_hour: 10,
        }
    }
}
```

### 7.3 Snooze Daemon Handler

```rust
/// Handle snooze command from TUI.
async fn handle_snooze(
    store: &Store,
    provider: &dyn MailSyncProvider,
    message_id: &MessageId,
    wake_at: DateTime<Utc>,
) -> Result<()> {
    let envelope = store.get_envelope(message_id).await?;
    let provider_id = &envelope.provider_message_id;

    // 1. Record current labels
    let original_labels = serde_json::to_string(&envelope.labels)?;

    // 2. Archive on Gmail (remove INBOX label)
    provider
        .modify_labels(provider_id, &[], &["INBOX".into()])
        .await?;

    // 3. Insert snooze record
    store
        .insert_snooze(message_id, &envelope.account_id, provider_id, wake_at, &original_labels)
        .await?;

    // 4. Remove from local inbox view
    store.remove_label(message_id, "INBOX").await?;

    Ok(())
}

/// Wake loop — runs alongside sync loop in daemon.
async fn snooze_wake_loop(store: Store, providers: ProviderMap) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
    loop {
        interval.tick().await;
        let now = Utc::now();
        match store.get_due_snoozes(now).await {
            Ok(due) => {
                for snoozed in due {
                    if let Err(e) = wake_snoozed(&store, &providers, &snoozed).await {
                        tracing::error!(
                            message_id = %snoozed.message_id,
                            "Failed to wake snoozed message: {e}"
                        );
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to check due snoozes: {e}");
            }
        }
    }
}

async fn wake_snoozed(
    store: &Store,
    providers: &ProviderMap,
    snoozed: &SnoozedMessage,
) -> Result<()> {
    let provider = providers.get(&snoozed.account_id)?;

    // 1. Re-add INBOX label on Gmail
    provider
        .modify_labels(&snoozed.provider_message_id, &["INBOX".into()], &[])
        .await?;

    // 2. Restore local labels
    let labels: Vec<String> = serde_json::from_str(&snoozed.original_labels)?;
    for label in &labels {
        store.add_label(&snoozed.message_id, label).await?;
    }

    // 3. Remove snooze record
    store.remove_snooze(&snoozed.message_id).await?;

    // 4. Notify TUI
    tracing::info!(message_id = %snoozed.message_id, "Snoozed message woke up");

    Ok(())
}
```

### 7.4 TUI Snooze Menu

```rust
Action::Snooze(_) => {
    if let Some(msg) = self.selected_message() {
        self.show_snooze_menu(msg.id.clone());
    }
}

fn show_snooze_menu(&mut self, message_id: MessageId) {
    self.mode = AppMode::SnoozeMenu { message_id };
    // Render options:
    // t = Tomorrow 9am
    // n = Next Monday 9am
    // w = Saturday 10am
    // e = Tonight 6pm
    // c = Custom...
}

fn handle_snooze_menu_key(&mut self, key: KeyCode) {
    let message_id = match &self.mode {
        AppMode::SnoozeMenu { message_id } => message_id.clone(),
        _ => return,
    };

    let option = match key {
        KeyCode::Char('t') => Some(SnoozeOption::TomorrowMorning),
        KeyCode::Char('n') => Some(SnoozeOption::NextMonday),
        KeyCode::Char('w') => Some(SnoozeOption::Weekend),
        KeyCode::Char('e') => Some(SnoozeOption::Tonight),
        KeyCode::Char('c') => {
            self.mode = AppMode::SnoozeCustomInput { message_id };
            return;
        }
        KeyCode::Esc => {
            self.mode = AppMode::Normal;
            return;
        }
        _ => None,
    };

    if let Some(opt) = option {
        let wake_at = resolve_snooze_time(opt, &self.config.snooze);
        self.send_command(Command::Snooze { message_id, wake_at });
        self.mode = AppMode::Normal;
    }
}
```

---

## Step 8: TUI Enhancements

### 8.1 Reader Mode Toggle

```rust
/// In message view rendering:
fn render_message_view(&self, msg: &MessageBody, area: Rect, buf: &mut Buffer) {
    let display_content = if self.reader_mode {
        let output = mxr_reader::clean(
            msg.text.as_deref(),
            msg.html.as_deref(),
            &self.reader_config,
        );
        self.reader_stats = Some((output.original_lines, output.cleaned_lines));
        output.content
    } else {
        msg.text
            .as_deref()
            .or(msg.html.as_deref())
            .unwrap_or("")
            .to_string()
    };

    // Render content with scrolling support
    let paragraph = Paragraph::new(display_content)
        .wrap(Wrap { trim: false })
        .scroll((self.scroll_offset, 0));
    paragraph.render(area, buf);
}

/// Status bar shows reader stats
fn render_status_bar(&self, area: Rect, buf: &mut Buffer) {
    let mut parts = vec![
        format!("[{}]", self.current_view_label()),
        format!("{} unread", self.unread_count),
        format!("synced {}", self.last_sync_ago()),
    ];

    if self.reader_mode {
        if let Some((orig, cleaned)) = self.reader_stats {
            parts.push(format!("reader: {} -> {} lines", orig, cleaned));
        } else {
            parts.push("reader mode".to_string());
        }
    }

    let status = parts.join(" | ");
    let span = Span::styled(status, Style::default().fg(Color::DarkGray));
    Paragraph::new(span).render(area, buf);
}
```

### 8.2 Compose/Reply/Reply-All/Forward Keybindings (A005)

> **Addendum A005**: Email action keybindings follow Gmail-native scheme. See `docs/blueprint/16-addendum.md` for full mapping.

```rust
// Gmail-native keybindings (A005):
// c = compose, r = reply, a = reply-all, f = forward
// e = archive, # = trash, ! = spam, s = star
// I = mark read, U = mark unread, l = apply label, v = move to label
// D = unsubscribe, Z = snooze, O = open in browser
// x = select/check message

Action::Compose => {
    self.send_command(Command::CreateDraft {
        kind: ComposeKind::New,
    });
    // Daemon creates draft file, returns path
    // On response:
    // let editor = resolve_editor(self.config.general.editor.as_deref());
    // spawn_editor(&editor, &draft_path, Some(cursor_line)).await;
    // After editor exit:
    // self.send_command(Command::ProcessDraft { draft_path });
}

Action::Reply => {
    if let Some(msg) = self.selected_message() {
        self.send_command(Command::CreateDraft {
            kind: ComposeKind::Reply {
                message_id: msg.id.clone(),
                reply_all: false,
            },
        });
    }
}

Action::ReplyAll => {
    if let Some(msg) = self.selected_message() {
        self.send_command(Command::CreateDraft {
            kind: ComposeKind::Reply {
                message_id: msg.id.clone(),
                reply_all: true,
            },
        });
    }
}

Action::Forward => {
    if let Some(msg) = self.selected_message() {
        self.send_command(Command::CreateDraft {
            kind: ComposeKind::Forward {
                message_id: msg.id.clone(),
            },
        });
    }
}

Action::Spam => {
    let ids = self.selected_or_cursor_ids();
    self.send_command(Command::Mutation(MutationCommand::Spam { message_ids: ids }));
}
```

### 8.3 Attachment Download and Open

```rust
Action::AttachmentList => {
    if let Some(body) = &self.current_body {
        if !body.attachments.is_empty() {
            self.mode = AppMode::AttachmentList;
        }
    }
}

fn handle_attachment_key(&mut self, key: KeyCode) {
    match key {
        KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
            let idx = (c as u8 - b'1') as usize;
            if let Some(body) = &self.current_body {
                if let Some(att) = body.attachments.get(idx) {
                    self.send_command(Command::DownloadAttachment {
                        message_id: self.current_message_id.clone().unwrap(),
                        attachment_id: att.id.clone(),
                    });
                }
            }
        }
        KeyCode::Char('o') => {
            // Open last downloaded attachment
            if let Some(path) = &self.last_downloaded_path {
                open_in_browser(&path.to_string_lossy());
            }
        }
        KeyCode::Esc => {
            self.mode = AppMode::Normal;
        }
        _ => {}
    }
}
```

### 8.4 Batch Operations — Selection Model (A007)

> **Addendum A007**: Batch operations use three selection modes: `x` toggle (Gmail), `V` visual line mode (vim), and `*` prefix pattern select.

```rust
/// Add to app state:
struct AppState {
    // ... existing fields ...
    selected_messages: HashSet<MessageId>,
    selection_mode: SelectionMode,
    visual_anchor: Option<usize>,  // index where V was pressed
}

#[derive(Default)]
enum SelectionMode {
    #[default]
    None,
    Visual,  // V mode: extending selection with j/k
}

// 'x' toggles selection on current message (Gmail style — A005/A007)
Action::ToggleSelect => {
    if let Some(msg) = self.cursor_message() {
        if self.selected_messages.contains(&msg.id) {
            self.selected_messages.remove(&msg.id);
        } else {
            self.selected_messages.insert(msg.id.clone());
        }
    }
}

// 'V' enters visual line mode (vim style — A007)
Action::VisualLineMode => {
    self.selection_mode = SelectionMode::Visual;
    self.visual_anchor = Some(self.cursor_index);
    if let Some(msg) = self.cursor_message() {
        self.selected_messages.insert(msg.id.clone());
    }
}

// In visual mode, j/k extends selection range from anchor to cursor
fn handle_visual_motion(&mut self, new_cursor: usize) {
    if let Some(anchor) = self.visual_anchor {
        self.selected_messages.clear();
        let (start, end) = if anchor <= new_cursor {
            (anchor, new_cursor)
        } else {
            (new_cursor, anchor)
        };
        for idx in start..=end {
            if let Some(msg) = self.message_at_index(idx) {
                self.selected_messages.insert(msg.id.clone());
            }
        }
    }
}

// Pattern select with '*' prefix (A007)
// *a = select all, *n = select none, *r = select read,
// *u = select unread, *s = select starred, *t = select current thread
Action::PatternSelect(pattern) => {
    match pattern {
        PatternSelect::All => {
            self.selected_messages = self.visible_messages()
                .iter().map(|m| m.id.clone()).collect();
        }
        PatternSelect::None => {
            self.selected_messages.clear();
        }
        PatternSelect::Read => {
            self.selected_messages = self.visible_messages()
                .iter().filter(|m| m.flags.contains(MessageFlags::READ))
                .map(|m| m.id.clone()).collect();
        }
        PatternSelect::Unread => {
            self.selected_messages = self.visible_messages()
                .iter().filter(|m| !m.flags.contains(MessageFlags::READ))
                .map(|m| m.id.clone()).collect();
        }
        PatternSelect::Starred => {
            self.selected_messages = self.visible_messages()
                .iter().filter(|m| m.flags.contains(MessageFlags::STARRED))
                .map(|m| m.id.clone()).collect();
        }
        PatternSelect::Thread => {
            if let Some(tid) = self.cursor_message().and_then(|m| m.thread_id.clone()) {
                self.selected_messages = self.visible_messages()
                    .iter().filter(|m| m.thread_id.as_ref() == Some(&tid))
                    .map(|m| m.id.clone()).collect();
            }
        }
    }
}

// Escape clears selection and exits visual mode
Action::Cancel => {
    self.selected_messages.clear();
    self.selection_mode = SelectionMode::None;
    self.visual_anchor = None;
}

// When messages are selected, action keys apply to ALL selected (A007).
// When no messages selected, actions apply to cursor position.
fn selected_or_cursor_ids(&self) -> Vec<MessageId> {
    if self.selected_messages.is_empty() {
        self.cursor_message()
            .map(|m| vec![m.id.clone()])
            .unwrap_or_default()
    } else {
        self.selected_messages.iter().cloned().collect()
    }
}

// Selection indicators in message list rendering
fn render_message_row(&self, msg: &Envelope, area: Rect, buf: &mut Buffer) {
    let is_selected = self.selected_messages.contains(&msg.id);
    let prefix = if is_selected { "▸ " } else { "  " };
    // ... render with selection highlight style
}

// Status bar shows selection count
fn render_status_bar(&self, area: Rect, buf: &mut Buffer) {
    if !self.selected_messages.is_empty() {
        parts.push(format!("{} selected", self.selected_messages.len()));
    }
    // ...
}
```

#### Batch confirmation (A007)

```toml
# Configurable batch confirmation threshold
[behavior]
batch_confirm = "destructive"  # "always" | "destructive" | "never"
```

```rust
/// Check whether batch confirmation is needed.
fn needs_batch_confirm(&self, action: &Action, count: usize) -> bool {
    if count <= 1 { return false; }
    match self.config.behavior.batch_confirm.as_str() {
        "always" => true,
        "never" => false,
        "destructive" | _ => matches!(action,
            Action::Trash | Action::Spam | Action::Unsubscribe
        ),
    }
}
```

#### Vim count support (A007)

```rust
/// Digit accumulator for vim-style counts (5j = move down 5)
digit_accumulator: Option<u32>,

fn handle_digit(&mut self, d: char) {
    let val = d.to_digit(10).unwrap();
    self.digit_accumulator = Some(
        self.digit_accumulator.unwrap_or(0) * 10 + val
    );
}

fn take_count(&mut self) -> usize {
    self.digit_accumulator.take().unwrap_or(1) as usize
}
```

---

## Step 9: Keybinding Configuration

### 9.1 Keybinding Parser

`crates/tui/src/keybindings.rs`:
```rust
use crossterm::event::{KeyCode, KeyModifiers, KeyEvent};
use std::collections::HashMap;
use serde::Deserialize;

/// Parsed keybinding configuration.
#[derive(Debug, Clone)]
pub struct KeybindingConfig {
    pub mail_list: HashMap<KeyBinding, String>,
    pub message_view: HashMap<KeyBinding, String>,
    pub thread_view: HashMap<KeyBinding, String>,
}

/// A single key or key combination.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyBinding {
    pub keys: Vec<KeyPress>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyPress {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

/// Parse a key string like "Ctrl-p", "gg", "G", "/", "Enter" into a KeyBinding.
pub fn parse_key_string(s: &str) -> Result<KeyBinding, String> {
    let mut keys = Vec::new();

    if s.starts_with("Ctrl-") {
        let ch = s[5..].chars().next().ok_or("Missing char after Ctrl-")?;
        keys.push(KeyPress {
            code: KeyCode::Char(ch),
            modifiers: KeyModifiers::CONTROL,
        });
    } else if s == "Enter" {
        keys.push(KeyPress {
            code: KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
        });
    } else if s == "Escape" || s == "Esc" {
        keys.push(KeyPress {
            code: KeyCode::Esc,
            modifiers: KeyModifiers::NONE,
        });
    } else if s == "Tab" {
        keys.push(KeyPress {
            code: KeyCode::Tab,
            modifiers: KeyModifiers::NONE,
        });
    } else {
        // Multi-char sequences (like "gg") or single chars
        for ch in s.chars() {
            let modifiers = if ch.is_uppercase() {
                KeyModifiers::SHIFT
            } else {
                KeyModifiers::NONE
            };
            keys.push(KeyPress {
                code: KeyCode::Char(ch),
                modifiers,
            });
        }
    }

    Ok(KeyBinding { keys })
}

/// Raw TOML structure for keys.toml
#[derive(Debug, Deserialize)]
pub struct KeysToml {
    #[serde(default)]
    pub mail_list: HashMap<String, String>,
    #[serde(default)]
    pub message_view: HashMap<String, String>,
    #[serde(default)]
    pub thread_view: HashMap<String, String>,
}

/// Load keybinding config from keys.toml, falling back to defaults.
pub fn load_keybindings(config_dir: &std::path::Path) -> KeybindingConfig {
    let keys_path = config_dir.join("keys.toml");
    let user_config = if keys_path.exists() {
        std::fs::read_to_string(&keys_path)
            .ok()
            .and_then(|s| toml::from_str::<KeysToml>(&s).ok())
    } else {
        None
    };

    let mut config = default_keybindings();

    if let Some(user) = user_config {
        // Override defaults with user config
        for (key, action) in &user.mail_list {
            if let Ok(kb) = parse_key_string(key) {
                config.mail_list.insert(kb, action.clone());
            }
        }
        for (key, action) in &user.message_view {
            if let Ok(kb) = parse_key_string(key) {
                config.message_view.insert(kb, action.clone());
            }
        }
        for (key, action) in &user.thread_view {
            if let Ok(kb) = parse_key_string(key) {
                config.thread_view.insert(kb, action.clone());
            }
        }
    }

    config
}

fn default_keybindings() -> KeybindingConfig {
    let mut mail_list = HashMap::new();
    let mut message_view = HashMap::new();
    let mut thread_view = HashMap::new();

    // Mail list defaults — Gmail-native scheme (A005)
    let ml_defaults = [
        // Navigation (vim-native)
        ("j", "move_down"), ("k", "move_up"), ("gg", "jump_top"),
        ("G", "jump_bottom"), ("Ctrl-d", "page_down"), ("Ctrl-u", "page_up"),
        ("H", "visible_top"), ("M", "visible_middle"), ("L", "visible_bottom"),
        ("zz", "center_current"),
        ("/", "search"), ("n", "next_search_result"), ("N", "prev_search_result"),
        ("Enter", "open"), ("o", "open"),
        ("q", "quit_view"), ("?", "help"),
        // Email actions (Gmail-native — A005)
        ("c", "compose"), ("r", "reply"), ("a", "reply_all"), ("f", "forward"),
        ("e", "archive"),       // Gmail: e (was 'a')
        ("#", "trash"),         // Gmail: # (was 'd')
        ("!", "spam"),          // Gmail: ! (NEW)
        ("s", "star"),
        ("I", "mark_read"),     // Gmail: Shift+I
        ("U", "mark_unread"),   // Gmail: Shift+U
        ("l", "apply_label"),   // Gmail: l
        ("v", "move_to_label"), // Gmail: v
        ("x", "toggle_select"), // Gmail: x (A007)
        // mxr-specific
        ("D", "unsubscribe"),   // (was 'U' — A005)
        ("Z", "snooze"),
        ("O", "open_in_browser"), // (was 'o' — A005)
        ("R", "toggle_reader_mode"),
        ("E", "export_thread"),
        ("V", "visual_line_mode"), // vim visual line (A007)
        ("Ctrl-p", "command_palette"),
        ("Tab", "switch_panes"),
        ("F", "toggle_fullscreen"),
        // Gmail go-to navigation (A005)
        ("gi", "go_inbox"), ("gs", "go_starred"), ("gt", "go_sent"),
        ("gd", "go_drafts"), ("ga", "go_all_mail"), ("gl", "go_label"),
    ];
    for (key, action) in ml_defaults {
        if let Ok(kb) = parse_key_string(key) {
            mail_list.insert(kb, action.to_string());
        }
    }

    // Message view defaults (A005)
    let mv_defaults = [
        ("j", "scroll_down"), ("k", "scroll_up"),
        ("R", "toggle_reader_mode"), ("O", "open_in_browser"),
        ("A", "attachment_list"),
        ("r", "reply"), ("a", "reply_all"), ("f", "forward"),
        ("e", "archive"), ("#", "trash"), ("!", "spam"),
        ("s", "star"), ("I", "mark_read"), ("U", "mark_unread"),
        ("D", "unsubscribe"),
    ];
    for (key, action) in mv_defaults {
        if let Ok(kb) = parse_key_string(key) {
            message_view.insert(kb, action.to_string());
        }
    }

    // Thread view defaults (A005)
    let tv_defaults = [
        ("j", "next_message"), ("k", "prev_message"),
        ("r", "reply"), ("a", "reply_all"), ("f", "forward"),
        ("R", "toggle_reader_mode"), ("E", "export_thread"),
        ("O", "open_in_browser"),
        ("e", "archive"), ("#", "trash"), ("!", "spam"),
        ("s", "star"), ("I", "mark_read"), ("U", "mark_unread"),
        ("D", "unsubscribe"),
    ];
    for (key, action) in tv_defaults {
        if let Ok(kb) = parse_key_string(key) {
            thread_view.insert(kb, action.to_string());
        }
    }

    KeybindingConfig {
        mail_list,
        message_view,
        thread_view,
    }
}
```

### 9.2 Action Resolution

```rust
/// Resolve a key sequence to an action name using the appropriate context map.
pub fn resolve_action(
    config: &KeybindingConfig,
    context: ViewContext,
    key_sequence: &[KeyPress],
) -> Option<String> {
    let map = match context {
        ViewContext::MailList => &config.mail_list,
        ViewContext::MessageView => &config.message_view,
        ViewContext::ThreadView => &config.thread_view,
    };

    let binding = KeyBinding {
        keys: key_sequence.to_vec(),
    };
    map.get(&binding).cloned()
}

/// Map action name strings to Action enum variants.
/// Includes all Gmail-native actions (A005) and batch operations (A007).
pub fn action_from_name(name: &str) -> Option<Action> {
    match name {
        // Navigation (vim-native)
        "move_down" => Some(Action::MoveDown(1)),
        "move_up" => Some(Action::MoveUp(1)),
        "jump_top" => Some(Action::JumpTop),
        "jump_bottom" => Some(Action::JumpBottom),
        "page_down" => Some(Action::PageDown),
        "page_up" => Some(Action::PageUp),
        "visible_top" => Some(Action::VisibleTop),
        "visible_middle" => Some(Action::VisibleMiddle),
        "visible_bottom" => Some(Action::VisibleBottom),
        "center_current" => Some(Action::CenterCurrent),
        "search" => Some(Action::OpenSearch),
        "next_search_result" => Some(Action::NextSearchResult),
        "prev_search_result" => Some(Action::PrevSearchResult),
        "open" => Some(Action::OpenSelected),
        "quit_view" => Some(Action::QuitView),
        "help" => Some(Action::Help),
        // Email actions (Gmail-native — A005)
        "compose" => Some(Action::Compose),
        "reply" => Some(Action::Reply),
        "reply_all" => Some(Action::ReplyAll),
        "forward" => Some(Action::Forward),
        "archive" => Some(Action::Archive),
        "trash" => Some(Action::Trash),
        "spam" => Some(Action::Spam),
        "star" => Some(Action::Star),
        "mark_read" => Some(Action::MarkRead),
        "mark_unread" => Some(Action::MarkUnread),
        "apply_label" => Some(Action::ApplyLabel),
        "move_to_label" => Some(Action::MoveToLabel),
        "toggle_select" => Some(Action::ToggleSelect),
        // mxr-specific
        "unsubscribe" => Some(Action::Unsubscribe),
        "snooze" => Some(Action::Snooze(SnoozeOption::TomorrowMorning)), // opens menu
        "open_in_browser" => Some(Action::OpenInBrowser),
        "toggle_reader_mode" => Some(Action::ToggleReaderMode),
        "export_thread" => Some(Action::ExportThread(ExportFormat::default())),
        "command_palette" => Some(Action::CommandPalette),
        "switch_panes" => Some(Action::SwitchPanes),
        "toggle_fullscreen" => Some(Action::ToggleFullscreen),
        "visual_line_mode" => Some(Action::VisualLineMode),
        // Scroll/navigation variants
        "scroll_down" => Some(Action::MoveDown(1)),
        "scroll_up" => Some(Action::MoveUp(1)),
        "next_message" => Some(Action::MoveDown(1)),
        "prev_message" => Some(Action::MoveUp(1)),
        "attachment_list" => Some(Action::AttachmentList),
        // Gmail go-to navigation (A005)
        "go_inbox" => Some(Action::GoTo(GoToTarget::Inbox)),
        "go_starred" => Some(Action::GoTo(GoToTarget::Starred)),
        "go_sent" => Some(Action::GoTo(GoToTarget::Sent)),
        "go_drafts" => Some(Action::GoTo(GoToTarget::Drafts)),
        "go_all_mail" => Some(Action::GoTo(GoToTarget::AllMail)),
        "go_label" => Some(Action::GoTo(GoToTarget::LabelPicker)),
        _ => None,
    }
}
```

### 9.3 Command Palette Integration

The command palette should display the user's configured keybinding (not the default) for each action:

```rust
fn build_palette_commands(&self) -> Vec<PaletteCommand> {
    let mut commands = Vec::new();

    // Find the keybinding string for a given action name in the current context
    let find_shortcut = |action_name: &str| -> Option<String> {
        let map = match self.current_view_context() {
            ViewContext::MailList => &self.keybindings.mail_list,
            ViewContext::MessageView => &self.keybindings.message_view,
            ViewContext::ThreadView => &self.keybindings.thread_view,
        };
        map.iter()
            .find(|(_, v)| v.as_str() == action_name)
            .map(|(k, _)| format_keybinding(k))
    };

    commands.push(PaletteCommand {
        label: "Compose new message".into(),
        shortcut: find_shortcut("compose"),
        action: Action::Compose,
        // ...
    });

    // ... etc for all commands
    commands
}
```

---

## Step 10: CLI Commands — Complete Surface (A001, A004)

> **Addendum A001** extends compose: CLI supports fully inline mode via flags, skipping `$EDITOR` when `--to` and `--body`/`--body-stdin` are both provided.
> **Addendum A004** requires every TUI action to have a CLI equivalent: all mutations, `cat` with reader mode, `thread`, `snoozed`, `attachments`, batch `--search` flag, and `--dry-run`.

### 10.1 Clap Subcommands

Add to the existing clap CLI in the daemon binary:

```rust
#[derive(clap::Subcommand)]
pub enum Commands {
    // ... existing commands (daemon, sync, search, doctor, accounts) ...

    /// Compose a new email
    Compose {
        /// Recipient(s), comma-separated
        #[arg(long)]
        to: Option<String>,
        /// CC recipient(s), comma-separated
        #[arg(long)]
        cc: Option<String>,
        /// BCC recipient(s), comma-separated
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
        /// Account name to send from (uses default if omitted)
        #[arg(long)]
        from: Option<String>,
        /// Skip confirmation prompt (for scripts/cron)
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
        /// Skip confirmation prompt
        #[arg(long)]
        yes: bool,
        /// Show what would be sent without sending
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
        /// Inline body (skip $EDITOR)
        #[arg(long)]
        body: Option<String>,
        /// Read body from stdin
        #[arg(long)]
        body_stdin: bool,
        /// Skip confirmation prompt
        #[arg(long)]
        yes: bool,
        /// Show what would be sent without sending
        #[arg(long)]
        dry_run: bool,
    },

    /// List drafts
    Drafts,

    /// Reply to all recipients of a message (A004)
    ReplyAll {
        /// Message ID to reply to
        message_id: String,
        /// Inline reply body (skip $EDITOR)
        #[arg(long)]
        body: Option<String>,
        /// Read reply body from stdin
        #[arg(long)]
        body_stdin: bool,
        /// Skip confirmation prompt
        #[arg(long)]
        yes: bool,
        /// Show what would be sent without sending
        #[arg(long)]
        dry_run: bool,
    },

    /// Send a draft
    Send {
        /// Draft ID to send
        draft_id: String,
    },

    // --- Reading commands (A004) ---

    /// Print message body (reader mode applied by default)
    Cat {
        /// Message ID
        message_id: String,
        /// Print body without reader mode
        #[arg(long)]
        raw: bool,
        /// Print original HTML body
        #[arg(long)]
        html: bool,
        /// Print full headers + body
        #[arg(long)]
        headers: bool,
        /// Print everything
        #[arg(long)]
        all: bool,
        /// Output as structured JSON
        #[arg(long)]
        format: Option<OutputFormat>,
    },

    /// Print full thread (chronological, reader mode applied)
    Thread {
        /// Thread ID
        thread_id: String,
        /// Output format
        #[arg(long)]
        format: Option<OutputFormat>,
    },

    // --- Single message mutations (A004) ---

    /// Archive a message (remove from inbox)
    Archive {
        /// Message ID (omit if using --search)
        message_id: Option<String>,
        /// Operate on all messages matching search query
        #[arg(long)]
        search: Option<String>,
        /// Skip confirmation
        #[arg(long)]
        yes: bool,
        /// Show what would happen without doing it
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

    /// Report message as spam (A004)
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
    Read {
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
        message_id: Option<String>,
        /// Label name
        name: String,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
    },

    /// Remove a label from a message
    Unlabel {
        message_id: Option<String>,
        /// Label name
        name: String,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
    },

    /// Move message to a label/folder
    Move {
        message_id: Option<String>,
        /// Target label
        label: String,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        dry_run: bool,
    },

    /// Snooze a message until a specified time
    Snooze {
        message_id: Option<String>,
        /// When to resurface: tomorrow|monday|weekend|tonight|DATE
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
        /// Unsnooze all snoozed messages
        #[arg(long)]
        all: bool,
    },

    /// List snoozed messages with wake times
    Snoozed,

    /// Unsubscribe from a mailing list
    Unsubscribe {
        message_id: Option<String>,
        /// Skip confirmation
        #[arg(long)]
        yes: bool,
        #[arg(long)]
        search: Option<String>,
        #[arg(long)]
        dry_run: bool,
    },

    /// Open message HTML in system browser
    Open {
        message_id: String,
    },

    // --- Attachments (A004) ---

    /// Manage message attachments
    #[command(subcommand)]
    Attachments(AttachmentCommands),
}

#[derive(clap::Subcommand)]
pub enum AttachmentCommands {
    /// List attachments for a message
    List {
        message_id: String,
    },
    /// Download attachment(s)
    Download {
        message_id: String,
        /// Attachment index (1-based, omit for all)
        index: Option<usize>,
        /// Output filename
        #[arg(long)]
        name: Option<String>,
        /// Output directory
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Open attachment with system handler
    Open {
        message_id: String,
        /// Attachment index (1-based)
        index: usize,
    },
}
```

### 10.2 CLI Handlers

**Inline compose behavior (Addendum A001):**
```
if --to AND (--body OR --body-stdin):
    → Build message from flags (no $EDITOR)
    → If --dry-run: print message summary, exit
    → If --yes: send immediately
    → Else: prompt "Send to alice@example.com? [y/n]"
else:
    → Open $EDITOR with whatever flags were provided
      pre-populated in YAML frontmatter
    → Normal editor compose flow
```

```rust
async fn handle_cli(cmd: Commands) -> Result<()> {
    match cmd {
        Commands::Compose { to, cc, bcc, subject, body, body_stdin, attach, from, yes, dry_run } => {
            let client = connect_to_daemon().await?;
            let config = load_config()?;
            let from_email = if let Some(acct) = from {
                config.account_email(&acct)?
            } else {
                config.default_account_email()?
            };

            // Read body from stdin if --body-stdin
            let body_text = if body_stdin {
                let mut buf = String::new();
                std::io::stdin().read_to_string(&mut buf)?;
                Some(buf)
            } else {
                body
            };

            // Inline compose path: --to AND (--body OR --body-stdin) provided
            let has_inline_body = body_text.is_some();
            let has_to = to.is_some();

            if has_to && has_inline_body {
                // Build draft directly from flags
                let draft = mxr_compose::build_draft_from_flags(
                    &from_email,
                    to.as_deref().unwrap(),
                    cc.as_deref(),
                    bcc.as_deref(),
                    subject.as_deref().unwrap_or(""),
                    body_text.as_deref().unwrap(),
                    &attach,
                )?;

                if dry_run {
                    mxr_compose::print_draft_summary(&draft);
                    return Ok(());
                }

                if !yes {
                    let recipients = to.as_deref().unwrap();
                    print!("Send to {recipients}? [y/n] ");
                    std::io::stdout().flush()?;
                    let mut input = String::new();
                    std::io::stdin().read_line(&mut input)?;
                    if input.trim().to_lowercase() != "y" {
                        println!("Aborted. Draft not saved.");
                        return Ok(());
                    }
                }

                client.send_command(Command::SendDraftInline { draft }).await?;
                println!("Message sent.");
            } else {
                // Editor path: open $EDITOR with flags pre-populated
                let (path, cursor) = mxr_compose::create_draft_file(
                    ComposeKind::New {
                        to: to.as_deref(),
                        cc: cc.as_deref(),
                        bcc: bcc.as_deref(),
                        subject: subject.as_deref(),
                        attachments: &attach,
                    },
                    &from_email,
                )?;

                let editor = mxr_compose::editor::resolve_editor(
                    config.general.editor.as_deref(),
                );
                let success = mxr_compose::editor::spawn_editor(
                    &editor, &path, Some(cursor),
                ).await?;

                if success {
                    let content = tokio::fs::read_to_string(&path).await?;
                    let (fm, body) = mxr_compose::frontmatter::parse_compose_file(&content)?;

                    let issues = mxr_compose::validate_draft(&fm, &body);
                    let errors: Vec<_> = issues.iter().filter(|i| i.is_error()).collect();
                    if !errors.is_empty() {
                        for e in errors {
                            eprintln!("Error: {e}");
                        }
                        return Ok(());
                    }

                    print!("Send? [y/n] ");
                    std::io::stdout().flush()?;
                    let mut input = String::new();
                    std::io::stdin().read_line(&mut input)?;
                    if input.trim().to_lowercase() == "y" {
                        client.send_command(Command::SendDraft { path: path.clone() }).await?;
                        println!("Message sent.");
                    } else {
                        println!("Draft saved.");
                    }
                } else {
                    println!("Editor exited with error. Draft saved.");
                }
            }

            Ok(())
        }

        Commands::Reply { message_id, body, body_stdin, yes, dry_run } => {
            let client = connect_to_daemon().await?;
            let config = load_config()?;

            // Ask daemon for message details + reader-cleaned thread context
            let resp = client
                .send_command(Command::PrepareReply { message_id: message_id.clone() })
                .await?;

            // Read body from stdin if --body-stdin
            let body_text = if body_stdin {
                let mut buf = String::new();
                std::io::stdin().read_to_string(&mut buf)?;
                Some(buf)
            } else {
                body
            };

            // Inline reply path (Addendum A001)
            if let Some(body_text) = body_text {
                let draft = mxr_compose::build_reply_from_flags(
                    &resp, &body_text,
                )?;

                if dry_run {
                    mxr_compose::print_draft_summary(&draft);
                    return Ok(());
                }

                if !yes {
                    print!("Send reply to {}? [y/n] ", resp.reply_to);
                    std::io::stdout().flush()?;
                    let mut input = String::new();
                    std::io::stdin().read_line(&mut input)?;
                    if input.trim().to_lowercase() != "y" {
                        println!("Aborted.");
                        return Ok(());
                    }
                }

                client.send_command(Command::SendDraftInline { draft }).await?;
                println!("Reply sent.");
            } else {
                // Editor path
                let (path, cursor) = mxr_compose::create_draft_file(
                    ComposeKind::Reply {
                        in_reply_to: resp.in_reply_to,
                        to: resp.reply_to,
                        cc: resp.cc,
                        subject: resp.subject,
                        thread_context: resp.thread_context,
                    },
                    &resp.from,
                )?;

                let editor = mxr_compose::editor::resolve_editor(
                    config.general.editor.as_deref(),
                );
                mxr_compose::editor::spawn_editor(&editor, &path, Some(cursor)).await?;

                // ... parse, validate, confirm, send ...
            }

            Ok(())
        }

        Commands::Forward { message_id, to, body, body_stdin, yes, dry_run } => {
            let client = connect_to_daemon().await?;
            let config = load_config()?;

            let resp = client
                .send_command(Command::PrepareForward { message_id: message_id.clone() })
                .await?;

            let body_text = if body_stdin {
                let mut buf = String::new();
                std::io::stdin().read_to_string(&mut buf)?;
                Some(buf)
            } else {
                body
            };

            // Inline forward path (Addendum A001): need both --to and --body
            if let (Some(to_addr), Some(body_text)) = (&to, &body_text) {
                let draft = mxr_compose::build_forward_from_flags(
                    &resp, to_addr, body_text,
                )?;

                if dry_run {
                    mxr_compose::print_draft_summary(&draft);
                    return Ok(());
                }

                if !yes {
                    print!("Forward to {to_addr}? [y/n] ");
                    std::io::stdout().flush()?;
                    let mut input = String::new();
                    std::io::stdin().read_line(&mut input)?;
                    if input.trim().to_lowercase() != "y" {
                        println!("Aborted.");
                        return Ok(());
                    }
                }

                client.send_command(Command::SendDraftInline { draft }).await?;
                println!("Forwarded.");
            } else {
                // Editor path with optional pre-population
                let (path, cursor) = mxr_compose::create_draft_file(
                    ComposeKind::Forward {
                        to: to.as_deref(),
                        subject: resp.subject,
                        forwarded_content: resp.forwarded_content,
                    },
                    &resp.from,
                )?;

                let editor = mxr_compose::editor::resolve_editor(
                    config.general.editor.as_deref(),
                );
                mxr_compose::editor::spawn_editor(&editor, &path, Some(cursor)).await?;
                // ... parse, validate, confirm, send ...
            }

            Ok(())
        }

        Commands::Drafts => {
            let client = connect_to_daemon().await?;
            let drafts = client.send_command(Command::ListDrafts).await?;

            if drafts.is_empty() {
                println!("No drafts.");
            } else {
                for draft in &drafts {
                    println!(
                        "{} | To: {} | Subject: {} | {}",
                        draft.id,
                        draft.to,
                        draft.subject,
                        draft.updated_at.format("%Y-%m-%d %H:%M"),
                    );
                }
            }

            Ok(())
        }

        Commands::Send { draft_id } => {
            let client = connect_to_daemon().await?;
            client
                .send_command(Command::SendDraftById { draft_id })
                .await?;
            println!("Message sent.");
            Ok(())
        }

        // ... other commands
    }
}
```

---

## Step 11: Spam Mutation + Expanded Mutation Handler (A004)

### 11.1 Spam Support on Gmail Provider

Add to `crates/provider-gmail/src/mutations.rs`:

```rust
async fn spam(&self, provider_message_id: &str) -> Result<(), Error> {
    // Gmail API: move to SPAM label
    self.modify_message_labels(provider_message_id, &["SPAM"], &["INBOX"]).await
}
```

### 11.2 Expanded Daemon Mutation Handler

The mutation handler from Step 5 must handle all mutations from A004:

```rust
async fn handle_mutation(
    store: &Store,
    search: &SearchIndex,
    provider: &dyn MailSyncProvider,
    cmd: MutationCommand,
) -> Result<MutationResult> {
    match cmd {
        // ... existing Archive, Trash, Star, SetRead, ModifyLabels ...

        MutationCommand::Spam { message_ids } => {
            for id in &message_ids {
                let pid = store.get_provider_id(id).await?;
                provider.spam(&pid).await?;
                store.modify_labels(id, &["SPAM".into()], &["INBOX".into()]).await?;
                search.update_labels(id, &store.get_labels(id).await?).await?;
            }
            Ok(MutationResult::Success)
        }

        MutationCommand::Move { message_id, target_label } => {
            let pid = store.get_provider_id(&message_id).await?;
            let current_labels = store.get_labels(&message_id).await?;
            // Remove from all folder-type labels, add target
            let remove: Vec<String> = current_labels.iter()
                .filter(|l| is_folder_label(l))
                .cloned()
                .collect();
            provider.modify_labels(&pid, &[target_label.clone()], &remove).await?;
            store.modify_labels(&message_id, &[target_label], &remove).await?;
            Ok(MutationResult::Success)
        }

        MutationCommand::OpenInBrowser { message_id } => {
            let body = store.get_body(&message_id).await?;
            if let Some(html) = &body.html {
                let path = std::env::temp_dir().join(format!("mxr-{message_id}.html"));
                std::fs::write(&path, html)?;
                open_in_browser(&format!("file://{}", path.display()))?;
            }
            Ok(MutationResult::Success)
        }
    }
}
```

### 11.3 CLI Batch Mutation Handler

All mutation CLI commands accept `--search` for batch operations (A004):

```rust
/// Generic handler for mutation CLI commands with --search support.
async fn handle_mutation_cli(
    client: &DaemonClient,
    message_id: Option<String>,
    search: Option<String>,
    yes: bool,
    dry_run: bool,
    build_cmd: impl Fn(Vec<String>) -> MutationCommand,
) -> Result<()> {
    let ids = if let Some(query) = search {
        let results = client.send_command(Command::Search { query }).await?;
        let ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();
        if ids.is_empty() {
            println!("No messages matched.");
            return Ok(());
        }
        if dry_run {
            println!("Would affect {} messages:", ids.len());
            for r in &results {
                println!("  {} | {}", r.id, r.subject);
            }
            return Ok(());
        }
        if !yes {
            print!("Apply to {} messages? [y/n] ", ids.len());
            std::io::stdout().flush()?;
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            if input.trim().to_lowercase() != "y" {
                println!("Aborted.");
                return Ok(());
            }
        }
        ids
    } else if let Some(id) = message_id {
        vec![id]
    } else {
        eprintln!("Error: provide MESSAGE_ID or --search");
        std::process::exit(1);
    };

    let cmd = build_cmd(ids);
    client.send_command(Command::Mutation(cmd)).await?;
    println!("Done.");
    Ok(())
}
```

---

## Step 12: IMAP Adapter — First-Party Provider (A008)

> **Addendum A008**: IMAP promoted to first-party in v1. New crate `crates/providers/imap/` implements `MailSyncProvider`. Validates provider-agnostic architecture against a genuinely different protocol.

### 12.1 Crate Setup

Add to workspace `Cargo.toml`:

```toml
[workspace]
members = [
    # ... existing members
    "crates/providers/imap",
]

[workspace.dependencies]
mxr-provider-imap = { path = "crates/providers/imap" }
async-imap = "0.10"
async-native-tls = "0.5"
```

`crates/providers/imap/Cargo.toml`:
```toml
[package]
name = "mxr-provider-imap"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
mxr-core = { workspace = true }
async-imap = { workspace = true }
async-native-tls = { workspace = true }
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
tracing = { workspace = true }
async-trait = { workspace = true }
chrono = { workspace = true }

[dev-dependencies]
tokio = { workspace = true, features = ["test-util"] }
```

### 12.2 IMAP Config

```rust
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ImapConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    /// Keyring reference (e.g., "mxr/fastmail-imap"). Looked up at runtime.
    pub password_ref: String,
    #[serde(default = "default_true")]
    pub use_tls: bool,
}

fn default_true() -> bool { true }
```

Config file example (from A008):
```toml
[accounts.fastmail]
name = "Fastmail"
email = "bk@fastmail.com"

[accounts.fastmail.sync]
provider = "imap"
host = "imap.fastmail.com"
port = 993
username = "bk@fastmail.com"
password_ref = "mxr/fastmail-imap"
use_tls = true

[accounts.fastmail.send]
provider = "smtp"
host = "smtp.fastmail.com"
port = 587
username = "bk@fastmail.com"
password_ref = "mxr/fastmail-smtp"
use_tls = true
```

### 12.3 Connection Management

```rust
use async_imap::Session;
use async_native_tls::TlsStream;
use tokio::net::TcpStream;

pub struct ImapConnection {
    session: Session<TlsStream<TcpStream>>,
}

impl ImapConnection {
    pub async fn connect(config: &ImapConfig) -> Result<Self, ImapProviderError> {
        let password = config.resolve_password()?;
        let tls = async_native_tls::TlsConnector::new();
        let tcp = TcpStream::connect(format!("{}:{}", config.host, config.port)).await?;
        let tls_stream = tls.connect(&config.host, tcp).await?;
        let client = async_imap::Client::new(tls_stream);
        let session = client.login(&config.username, &password).await
            .map_err(|(e, _)| ImapProviderError::Auth(e.to_string()))?;
        Ok(Self { session })
    }
}
```

### 12.4 Sync Strategy (A008 — layered)

```rust
/// Sync strategy selection based on server capabilities.
enum SyncStrategy {
    /// CONDSTORE/QRESYNC (RFC 7162): delta sync via MODSEQ.
    /// Fastmail, Dovecot support this.
    CondstoreQresync { last_modseq: u64 },
    /// UID-based polling: fallback. Track UIDVALIDITY + UIDNEXT per mailbox.
    UidPolling { uid_validity: u32, uid_next: u32 },
}

/// Detect best sync strategy from server capabilities.
async fn detect_sync_strategy(session: &mut Session<...>) -> SyncStrategy {
    let caps = session.capabilities().await.unwrap();
    if caps.has_str("CONDSTORE") || caps.has_str("QRESYNC") {
        // Enable CONDSTORE for this session
        session.run_command("ENABLE CONDSTORE").await.ok();
        SyncStrategy::CondstoreQresync { last_modseq: 0 }
    } else {
        SyncStrategy::UidPolling { uid_validity: 0, uid_next: 0 }
    }
}

/// Delta sync via CONDSTORE: fetch only messages changed since last MODSEQ.
async fn sync_condstore(
    session: &mut Session<...>,
    mailbox: &str,
    last_modseq: u64,
) -> Result<(Vec<Envelope>, u64), ImapProviderError> {
    session.select(mailbox).await?;
    let query = format!("1:* (CHANGEDSINCE {})", last_modseq);
    let messages = session.uid_fetch(&query, "(FLAGS ENVELOPE BODY.PEEK[HEADER])").await?;
    // ... parse into Envelope structs, return new highest modseq
    todo!()
}

/// UID-based polling fallback.
async fn sync_uid_polling(
    session: &mut Session<...>,
    mailbox: &str,
    uid_validity: u32,
    uid_next: u32,
) -> Result<Vec<Envelope>, ImapProviderError> {
    let status = session.select(mailbox).await?;
    if status.uid_validity != Some(uid_validity) {
        // UIDVALIDITY changed — full resync needed
        tracing::warn!(mailbox, "UIDVALIDITY changed, performing full resync");
        // Fetch all UIDs
    }
    // Fetch messages with UID >= uid_next
    let query = format!("{}:*", uid_next);
    let messages = session.uid_fetch(&query, "(FLAGS ENVELOPE BODY.PEEK[HEADER])").await?;
    // ... parse
    todo!()
}
```

### 12.5 IDLE for Push Notifications (A008)

```rust
/// IDLE (RFC 2177): real-time push notifications.
/// Runs in a separate task, notifies the daemon event loop on new messages.
async fn idle_loop(
    config: ImapConfig,
    mailbox: String,
    tx: tokio::sync::mpsc::Sender<ImapEvent>,
) -> Result<(), ImapProviderError> {
    let mut conn = ImapConnection::connect(&config).await?;
    conn.session.select(&mailbox).await?;

    loop {
        let idle = conn.session.idle();
        let handle = idle.init().await?;

        // Wait for server notification or timeout (29 min per RFC)
        let (reason, session) = tokio::time::timeout(
            std::time::Duration::from_secs(29 * 60),
            handle.wait_with_timeout(std::time::Duration::from_secs(29 * 60)),
        ).await??;

        conn.session = session;

        match reason {
            async_imap::extensions::idle::IdleResponse::NewData(_) => {
                tx.send(ImapEvent::NewMessages { mailbox: mailbox.clone() }).await.ok();
            }
            _ => {
                // Timeout or other — re-IDLE
            }
        }
    }
}
```

### 12.6 JWZ Threading Algorithm (A008)

Lives in sync crate as shared module (used by both IMAP and any provider lacking server-side threading):

```rust
// crates/sync/src/threading.rs

/// JWZ threading algorithm — reconstruct threads from In-Reply-To + References headers.
/// See https://www.jwz.org/doc/threading.html
pub fn thread_messages(messages: &[MessageForThreading]) -> Vec<ThreadTree> {
    // 1. Build ID table: map Message-ID → Container
    let mut id_table: HashMap<String, Container> = HashMap::new();

    for msg in messages {
        // Get or create container for this message
        let container = id_table.entry(msg.message_id.clone())
            .or_insert_with(Container::empty);
        container.message = Some(msg.clone());

        // Walk References header, link each pair as parent→child
        let mut prev_id: Option<&str> = None;
        for ref_id in &msg.references {
            id_table.entry(ref_id.clone()).or_insert_with(Container::empty);
            if let Some(parent_id) = prev_id {
                // Set parent_id as parent of ref_id (if not already)
                // (skip if would create cycle)
            }
            prev_id = Some(ref_id);
        }

        // Set last reference (or In-Reply-To) as parent of this message
        if let Some(parent_id) = msg.in_reply_to.as_deref().or(prev_id) {
            // Link parent_id → msg.message_id
        }
    }

    // 2. Find root set (containers with no parent)
    // 3. Prune empty containers
    // 4. Sort threads by date
    // 5. Return thread trees

    todo!()
}

pub struct MessageForThreading {
    pub message_id: String,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub date: chrono::DateTime<chrono::Utc>,
    pub subject: String,
}

pub struct Container {
    pub message: Option<MessageForThreading>,
    pub parent: Option<String>,
    pub children: Vec<String>,
}

pub struct ThreadTree {
    pub root_message_id: String,
    pub messages: Vec<String>, // ordered message IDs
}
```

### 12.7 Folder → Label Mapping (A008)

```rust
/// Map IMAP folders to mxr labels using RFC 6154 SPECIAL-USE attributes.
fn map_folder_to_label(folder_name: &str, special_use: Option<&str>) -> Label {
    match special_use {
        Some("\\Inbox") => Label { name: "INBOX".into(), kind: LabelKind::System },
        Some("\\Sent") => Label { name: "SENT".into(), kind: LabelKind::System },
        Some("\\Drafts") => Label { name: "DRAFT".into(), kind: LabelKind::System },
        Some("\\Trash") => Label { name: "TRASH".into(), kind: LabelKind::System },
        Some("\\Junk") | Some("\\Spam") => Label { name: "SPAM".into(), kind: LabelKind::System },
        Some("\\Archive") | Some("\\All") => Label { name: "ALL".into(), kind: LabelKind::System },
        Some("\\Flagged") => Label { name: "STARRED".into(), kind: LabelKind::System },
        _ => Label { name: folder_name.to_string(), kind: LabelKind::Folder },
    }
}

/// Detect SPECIAL-USE attributes via LIST command.
async fn detect_special_use_folders(
    session: &mut Session<...>,
) -> Result<HashMap<String, String>, ImapProviderError> {
    let folders = session.list(None, Some("*")).await?;
    let mut mapping = HashMap::new();
    for folder in folders.iter() {
        for attr in folder.attributes() {
            if let async_imap::types::NameAttribute::Custom(ref cow) = attr {
                let s = cow.as_ref();
                if s.starts_with('\\') {
                    mapping.insert(folder.name().to_string(), s.to_string());
                }
            }
        }
    }
    Ok(mapping)
}
```

### 12.8 IMAP Mutations (A008)

```rust
/// IMAP mutations via flag changes and COPY+DELETE.
impl ImapProvider {
    async fn set_flag(&self, uid: u32, mailbox: &str, flag: &str, add: bool) -> Result<()> {
        let mut conn = self.connect().await?;
        conn.session.select(mailbox).await?;
        if add {
            conn.session.uid_store(format!("{uid}"), format!("+FLAGS ({flag})")).await?;
        } else {
            conn.session.uid_store(format!("{uid}"), format!("-FLAGS ({flag})")).await?;
        }
        Ok(())
    }

    /// Archive = COPY to Archive folder + DELETE from source
    async fn archive(&self, uid: u32, source_mailbox: &str) -> Result<()> {
        let mut conn = self.connect().await?;
        conn.session.select(source_mailbox).await?;
        conn.session.uid_copy(format!("{uid}"), "Archive").await?;
        conn.session.uid_store(format!("{uid}"), "+FLAGS (\\Deleted)").await?;
        conn.session.expunge().await?;
        Ok(())
    }

    /// Move = COPY + DELETE (or MOVE if RFC 6851 supported)
    async fn move_message(&self, uid: u32, source: &str, target: &str) -> Result<()> {
        let mut conn = self.connect().await?;
        conn.session.select(source).await?;

        // Check if server supports MOVE (RFC 6851)
        let caps = conn.session.capabilities().await?;
        if caps.has_str("MOVE") {
            conn.session.uid_mv(format!("{uid}"), target).await?;
        } else {
            conn.session.uid_copy(format!("{uid}"), target).await?;
            conn.session.uid_store(format!("{uid}"), "+FLAGS (\\Deleted)").await?;
            conn.session.expunge().await?;
        }
        Ok(())
    }
}

#[async_trait]
impl MailSyncProvider for ImapProvider {
    fn name(&self) -> &str { "imap" }

    async fn set_read(&self, provider_id: &str, read: bool) -> Result<()> {
        let (mailbox, uid) = parse_provider_id(provider_id)?;
        self.set_flag(uid, &mailbox, "\\Seen", read).await
    }

    async fn set_starred(&self, provider_id: &str, starred: bool) -> Result<()> {
        let (mailbox, uid) = parse_provider_id(provider_id)?;
        self.set_flag(uid, &mailbox, "\\Flagged", starred).await
    }

    async fn trash(&self, provider_id: &str) -> Result<()> {
        let (mailbox, uid) = parse_provider_id(provider_id)?;
        self.move_message(uid, &mailbox, &self.trash_folder).await
    }

    async fn modify_labels(&self, provider_id: &str, add: &[String], remove: &[String]) -> Result<()> {
        let (mailbox, uid) = parse_provider_id(provider_id)?;
        // For IMAP, "labels" map to folder moves or flag changes
        // Flag-based labels (STARRED, READ) use flag changes
        // Folder-based labels use COPY + DELETE
        for label in add {
            match label.as_str() {
                "STARRED" => self.set_flag(uid, &mailbox, "\\Flagged", true).await?,
                folder => self.copy_to_folder(uid, &mailbox, folder).await?,
            }
        }
        for label in remove {
            match label.as_str() {
                "STARRED" => self.set_flag(uid, &mailbox, "\\Flagged", false).await?,
                "INBOX" => self.archive(uid, &mailbox).await?,
                _ => {} // Can't "remove" a folder label — it's a move
            }
        }
        Ok(())
    }
}
```

### 12.9 ProviderMeta for IMAP

```rust
/// Provider-specific metadata stored alongside envelopes.
/// For IMAP: UID, UIDVALIDITY, mailbox, flags.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImapProviderMeta {
    pub uid: u32,
    pub uid_validity: u32,
    pub mailbox: String,
    pub flags: Vec<String>,
}

/// Provider ID format for IMAP: "mailbox:uid" (e.g., "INBOX:12345")
fn format_provider_id(mailbox: &str, uid: u32) -> String {
    format!("{mailbox}:{uid}")
}

fn parse_provider_id(id: &str) -> Result<(String, u32), ImapProviderError> {
    let (mailbox, uid_str) = id.rsplit_once(':')
        .ok_or(ImapProviderError::InvalidProviderId(id.to_string()))?;
    let uid = uid_str.parse()
        .map_err(|_| ImapProviderError::InvalidProviderId(id.to_string()))?;
    Ok((mailbox.to_string(), uid))
}
```

### 12.10 CLI Account Setup

```rust
/// `mxr accounts add imap` — interactive IMAP account setup
async fn setup_imap_account() -> Result<()> {
    println!("IMAP Account Setup");
    println!("==================");

    let name = prompt("Account name (e.g., fastmail): ")?;
    let email = prompt("Email address: ")?;
    let host = prompt("IMAP host (e.g., imap.fastmail.com): ")?;
    let port: u16 = prompt("IMAP port [993]: ")?.parse().unwrap_or(993);
    let username = prompt(&format!("IMAP username [{email}]: "))?;
    let username = if username.is_empty() { email.clone() } else { username };

    let password = prompt_password("IMAP password: ")?;
    let keyring_ref = format!("mxr/{name}-imap");
    store_in_keyring(&keyring_ref, &username, &password)?;

    // Test connection
    println!("Testing IMAP connection...");
    let config = ImapConfig { host: host.clone(), port, username: username.clone(), password_ref: keyring_ref.clone(), use_tls: true };
    ImapConnection::connect(&config).await?;
    println!("IMAP connection successful.");

    // SMTP setup
    let smtp_host = prompt("SMTP host (e.g., smtp.fastmail.com): ")?;
    let smtp_port: u16 = prompt("SMTP port [587]: ")?.parse().unwrap_or(587);
    let smtp_password = prompt_password("SMTP password (Enter to reuse IMAP): ")?;
    let smtp_keyring_ref = format!("mxr/{name}-smtp");
    let smtp_pass = if smtp_password.is_empty() { password } else { smtp_password };
    store_in_keyring(&smtp_keyring_ref, &username, &smtp_pass)?;

    // Write config
    write_account_config(&name, &email, &host, port, &username, &keyring_ref,
                         &smtp_host, smtp_port, &smtp_keyring_ref)?;

    println!("Account '{name}' configured. Run `mxr sync --account {name}` to start syncing.");
    Ok(())
}
```

---

## Step 13: Reply-All Compose Path (A004)

### 13.1 ComposeKind::Reply with reply_all flag

Extend the existing `ComposeKind::Reply` to support reply-all:

```rust
ComposeKind::Reply {
    in_reply_to: String,
    to: String,
    cc: String,
    subject: String,
    thread_context: String,
    reply_all: bool,  // NEW: when true, CC includes all original recipients
}
```

### 13.2 Daemon PrepareReplyAll Handler

```rust
/// Build reply-all recipients: To = original sender, CC = all To + CC minus self.
async fn prepare_reply_all(
    store: &Store,
    message_id: &str,
    account_email: &str,
) -> Result<ReplyContext> {
    let envelope = store.get_envelope(message_id).await?;

    let to = envelope.from.clone();

    // CC = original To + original CC, minus the replying account's email
    let mut cc_addrs: Vec<String> = Vec::new();
    for addr in envelope.to.split(',').chain(envelope.cc.split(',')) {
        let addr = addr.trim();
        if !addr.is_empty() && !addr.eq_ignore_ascii_case(account_email) && addr != to {
            cc_addrs.push(addr.to_string());
        }
    }

    Ok(ReplyContext {
        in_reply_to: envelope.provider_message_id.clone(),
        reply_to: to,
        cc: cc_addrs.join(", "),
        subject: envelope.subject.clone(),
        from: account_email.to_string(),
        thread_context: build_thread_context(store, &envelope).await?,
    })
}
```

### 13.3 CLI reply-all Handler

```rust
Commands::ReplyAll { message_id, body, body_stdin, yes, dry_run } => {
    let client = connect_to_daemon().await?;
    let resp = client
        .send_command(Command::PrepareReply {
            message_id: message_id.clone(),
            reply_all: true,
        })
        .await?;

    // Same flow as Reply, but resp.cc contains all recipients
    // ... (identical to Reply handler, using resp.cc)
}
```

---

## Definition of Done

Phase 2 is complete when ALL of the following are true:

1. **Compose**: `mxr compose` opens `$EDITOR` with YAML frontmatter, user writes markdown, message sends as multipart (text/plain + text/html). Replies include reader-cleaned context block.
2. **Inline compose (A001)**: `mxr compose --to X --body Y` sends without opening `$EDITOR`. `--body-stdin` reads from pipe. `--yes` skips confirmation. `--dry-run` previews without sending. `mxr reply MSG --body Y`, `mxr reply-all MSG --body Y`, and `mxr forward MSG --to X --body Y` work inline too.
3. **Markdown invisible to recipients (A002)**: Sent messages are standard multipart: text/html (comrak-rendered) + text/plain (raw markdown). Recipients see a normal formatted email.
4. **SMTP Send**: SMTP provider works with lettre, supports STARTTLS and implicit TLS, reads password from keyring.
5. **Gmail Send**: Messages send via Gmail API as base64url-encoded RFC 2822. Sent messages stored locally.
6. **All mutations (A004)**: Archive, trash, spam, star/unstar, mark read/unread, apply/remove labels, move, snooze/unsnooze, unsubscribe all work via both Gmail API and IMAP. Batch operations for multi-select. Local store + search index updated after each mutation.
7. **Reader Mode**: HTML-to-text conversion works (built-in + configurable external command). Signatures stripped (RFC 3676 + heuristic). Quotes collapsed. Boilerplate removed. Stats displayed in status bar. `mxr cat` applies reader mode by default; `--raw` disables.
8. **Unsubscribe**: `D` keybinding (A005) with confirmation. RFC 8058 one-click POST works. Mailto auto-sends. HTTP links open browser. `[U]` indicator in message list.
9. **Snooze**: `Z` opens snooze menu. Snoozed messages archived on provider. Wake loop restores messages on schedule. `mxr snoozed` lists snoozed messages. `mxr snooze`/`mxr unsnooze` CLI commands work.
10. **Gmail-native keybindings (A005)**: `e` archive, `#` trash, `!` spam, `a` reply-all, `I` mark read, `U` mark unread, `l` apply label, `v` move to label, `D` unsubscribe, `O` open in browser, `x` select. Gmail `g` prefix navigation (`gi`, `gs`, `gt`, etc.).
11. **Batch operations (A007)**: `x` toggle select (Gmail), `V` + j/k visual line mode (vim), `*` prefix pattern select (`*a`, `*n`, `*r`, `*u`, `*s`, `*t`). Actions apply to selection when messages are selected. Selection indicators in message list and status bar. Configurable batch confirmation (`always`/`destructive`/`never`). Vim count support (`5j`).
12. **IMAP adapter (A008)**: `crates/providers/imap/` implements `MailSyncProvider`. CONDSTORE/QRESYNC delta sync, UID fallback, IDLE for push. JWZ threading algorithm in sync crate. Folder-to-label mapping with SPECIAL-USE detection. IMAP mutations (flag changes, COPY+DELETE for move). `mxr accounts add imap` CLI setup. ProviderMeta tracks UID, UIDVALIDITY, mailbox, flags.
13. **Keybindings**: `keys.toml` parsed, defaults compiled in (Gmail-native per A005), user overrides work. Command palette shows user's bindings.
14. **Complete CLI surface (A004)**: `mxr compose`, `mxr reply`, `mxr reply-all`, `mxr forward`, `mxr drafts`, `mxr send` (both interactive and inline). All mutations: `mxr archive`, `mxr trash`, `mxr spam`, `mxr star/unstar`, `mxr read/unread`, `mxr label/unlabel`, `mxr move`, `mxr snooze/unsnooze`, `mxr unsubscribe`, `mxr open`. Reading: `mxr cat` (with reader mode), `mxr thread`. Listing: `mxr snoozed`, `mxr attachments`. Batch: `--search` flag on all mutation commands. All mutations support `--dry-run`.
15. **Reply-all (A004)**: `mxr reply-all` as separate command from `mxr reply`. TUI keybinding `a` (Gmail-native). Reply-all correctly computes To = sender, CC = all original recipients minus self.
16. **Tests**: Unit tests for frontmatter parsing, reader pipeline, snooze time resolution, keybinding parsing, inline compose flag parsing, JWZ threading, IMAP folder mapping. Integration tests for compose roundtrip (editor + inline), mutation flow, batch operations, IMAP sync.
17. **CI passes**: `cargo check`, `cargo fmt`, `cargo clippy`, `cargo test` all green.

### User Acceptance Test

You can, as a daily workflow:
- Open mxr, browse inbox with reader mode cleaning away noise
- Reply to an email (cursor lands in right place, context block visible for reference)
- Reply-all with `a` key (Gmail-native)
- Archive processed emails with `e` (Gmail-native)
- Trash with `#`, spam with `!`
- Star important ones with `s`
- Mark read/unread with `I`/`U`
- Select multiple messages with `x`, apply actions to batch
- Use `V` + j/k to visually select a range, then archive all at once
- Snooze "deal with later" emails to tomorrow morning
- One-key unsubscribe from a newsletter with `D`
- Forward a message to a colleague
- Compose a new message with a PDF attachment
- Navigate with `gi` (inbox), `gs` (starred), `gt` (sent)
- All of the above without leaving the terminal
- All of the above with both Gmail and IMAP accounts

Scripted workflows also work (Addendum A001, A004):
- `echo "body" | mxr compose --to X --subject Y --body-stdin --yes` sends without interaction
- `mxr reply MSG_ID --body "Sounds good" --yes` replies inline
- `mxr reply-all MSG_ID --body "Agreed" --yes` reply-all inline
- `mxr archive --search "label:newsletters is:read" --yes` batch archive
- `mxr trash --search "from:spam@junk.com" --dry-run` preview batch trash
- `mxr cat MSG_ID` prints message with reader mode applied
- `mxr thread THREAD_ID` prints full thread
- `mxr snoozed` lists snoozed messages
- `mxr attachments download MSG_ID` downloads all attachments
- Cron jobs can compose and send via flags + `--yes`
- Recipients see normal formatted emails, not raw markdown (Addendum A002)

---

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| `comrak` HTML output renders poorly in email clients (Gmail, Outlook) | Recipients see broken formatting | Test rendered HTML in Gmail, Outlook, Apple Mail. Keep the HTML template minimal (no CSS that email clients strip). Fallback: text/plain is always readable. |
| `lettre` SMTP auth failures with corporate SMTP servers | Users can't send via work email | Support both STARTTLS (587) and implicit TLS (465). Log detailed SMTP errors. Document common corporate SMTP configurations. |
| Gmail API rate limits hit during batch mutations | Mutations fail mid-batch | Implement exponential backoff. Batch modify endpoint handles up to 1000 messages per call. Queue mutations if rate limited and retry. |
| Reader mode strips actual content (false positive) | Users miss important information | Reader mode is a toggle (`R`), not permanent. Conservative regex patterns (prefer under-stripping to over-stripping). Show stats so users see when large amounts were stripped. |
| `serde_yaml` parsing edge cases with email addresses in frontmatter | Compose file can't be parsed after editor save | Test with addresses containing special YAML chars (`+`, `.`, quoted strings). Use YAML flow scalars for email fields. Provide clear parse error messages pointing to the problematic field. |
| Snooze wake time missed if daemon is not running | Snoozed messages stay archived forever | On daemon startup, immediately check for overdue snoozes. Wake all overdue messages. Log warning if snoozes were overdue by >1 hour. |
| RFC 2822 message building edge cases (long headers, non-ASCII subjects, attachments) | Sent emails malformed or rejected | Use `lettre`'s `Message` builder for SMTP path (handles encoding). For Gmail API path, test with non-ASCII subjects (RFC 2047 encoding), long recipient lists, and various attachment types. |
| Keyring not available on headless Linux servers | Users can't store SMTP passwords | Implement encrypted file fallback (`$XDG_DATA_HOME/mxr/credentials.enc`) when keyring is unavailable. Document the fallback in setup flow. |
| Editor spawning fails in certain terminal environments (SSH, containers) | Users can't compose | Validate `$EDITOR` exists before spawning. Clear error message if not found. Support `MXR_EDITOR` env var as highest-priority override. |
| Multi-select + batch mutation inconsistency if some mutations fail | Partial state: some messages mutated, others not | Process batch mutations atomically where possible (Gmail batchModify). If individual failures occur, report which messages failed and allow retry. Don't update local state for failed mutations. |
| IMAP server diversity (different CONDSTORE/QRESYNC/IDLE support) | Sync failures on some IMAP servers | Layered strategy: CONDSTORE first, UID polling fallback, IDLE optional. Test against Dovecot, Fastmail, Proton Bridge. Log capability detection results. |
| IMAP folder vs Gmail label semantic mismatch | Users confused by different behavior | Document honestly: IMAP is folder-based, applying multiple labels creates copies. Don't pretend IMAP has Gmail-style labels. Show "(folder)" indicator in TUI. |
| JWZ threading algorithm edge cases (missing References, broken In-Reply-To) | Threads not reconstructed correctly | Fallback to subject-based grouping when header-based threading fails. Test with real-world email samples from various clients. |
| `async-imap` crate maturity vs production IMAP servers | Connection drops, protocol edge cases | Implement robust reconnection logic. Test against real servers. Consider fallback to `imap` (sync) crate if async version has blockers. |
| Batch operations on large selections (1000+ messages) | UI freezes, provider rate limits | Process batches in chunks (100 per API call for Gmail). Show progress indicator. Use `--dry-run` to preview before committing. |
