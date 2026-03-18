# mxr — Compose Flow

## Philosophy

mxr does not compete with your text editor. You already know how to write in neovim, helix, vim, kakoune, VS Code, or whatever `$EDITOR` points to. mxr uses that.

The compose flow is one of mxr's strongest product bets. People hate typing in web email clients. The input is laggy, the formatting is unpredictable, and there's no vim mode. mxr gives you your own editor with full muscle memory, markdown formatting, and a clean metadata model via YAML frontmatter.

## The compose file format

When a user composes or replies, mxr creates a temporary markdown file with YAML frontmatter:

### New message

```markdown
---
to: 
cc: 
bcc: 
subject: 
from: bk@example.com
attach: []
---

```

### Reply

```markdown
---
to: alice@example.com
cc: bob@example.com
bcc: 
subject: Re: Deployment rollback plan
from: bk@example.com
in-reply-to: <msg-id-123@example.com>
attach: []
---



# --- context (stripped before sending) ---
# From: alice@example.com
# Date: 2026-03-15 14:30
#
# Hey team, what's the rollback strategy for the
# v2.3 deployment? We need a plan by Friday.
#
# From: bob@example.com
# Date: 2026-03-15 15:12
#
# I think we should keep the v2.2 containers warm
# and use a blue-green switch. Alice, can you check
# if the load balancer config supports it?
```

### Forward

```markdown
---
to: 
cc: 
bcc: 
subject: Fwd: Deployment rollback plan
from: bk@example.com
attach: []
---

---------- Forwarded message ----------

# --- context (stripped before sending) ---
# Original from: alice@example.com
# Date: 2026-03-15 14:30
#
# Hey team, what's the rollback strategy for the
# v2.3 deployment? We need a plan by Friday.
```

## Key design decisions

### YAML frontmatter for metadata

We chose YAML frontmatter because:
- Anyone who's written a Hugo post, Obsidian note, or Jekyll page already knows this pattern
- It's human-readable and human-editable
- It separates routing metadata from body cleanly
- It's easy to parse programmatically (serde_yaml in Rust)
- It supports structured data (arrays for cc, attach lists)

The frontmatter fields:
- `to`: recipient (comma-separated for multiple)
- `cc`: carbon copy (comma-separated)
- `bcc`: blind carbon copy
- `subject`: email subject line
- `from`: sender address (pre-populated from account config)
- `in-reply-to`: message ID for threading (auto-populated for replies, invisible plumbing)
- `attach`: list of file paths for attachments

### Context block for reference

When replying, the original thread is included as a commented-out context block below the compose area. This solves the "I need to reference the original while writing" problem without building a split-pane multiplexer.

**Why not a split pane?** We considered building tmux-style pane splitting into mxr so users could view emails and compose side by side. We rejected this because:
- Our target users already run tmux, zellij, or a tiling WM. They already know how to split panes.
- Building a terminal multiplexer inside the email client is massive scope for marginal benefit.
- It violates the principle "mxr is an email client, not a terminal multiplexer."
- The context block solves 80% of the reference need with 20 lines of code.

The context block is:
- Populated from the thread's messages (reader-mode cleaned text, not raw HTML)
- Prefixed with `#` so it's visually distinct and ignored by markdown parsers
- Everything from `# --- context (stripped before sending) ---` downward is stripped before the markdown is converted and sent

### Cursor position

When $EDITOR opens, the cursor should land on the empty line between the frontmatter and the context block. The user starts typing immediately. In vim/neovim, this can be achieved by passing a `+{line_number}` argument to the editor command.

### Attachments via frontmatter

```yaml
attach:
  - ~/documents/invoice-2026-03.pdf
  - ~/documents/receipt.png
```

File paths are resolved relative to $HOME. Tilde expansion is supported. Files are verified to exist when the draft is processed. Missing files produce a clear error, not a silent failure.

## Compose lifecycle

```
1. User presses 'c' (compose) or 'r' (reply) or 'f' (forward) in TUI
   → TUI sends CreateDraft command to daemon

2. Daemon creates Draft in SQLite
   → Generates temp file at /tmp/mxr-draft-{uuid}.md
   → Populates frontmatter (to, subject, from, etc.)
   → For replies: populates context block from thread
   → Returns file path to TUI

3. TUI spawns $EDITOR with the temp file
   → Editor opens, user writes their message
   → User saves and quits

4. TUI detects editor exit
   → Sends UpdateDraft command to daemon with file path

5. Daemon reads the temp file
   → Parses YAML frontmatter → extracts routing metadata
   → Strips context block
   → Remaining markdown → this is the body
   → Updates Draft in SQLite

6. Daemon validates:
   → 'to' field is non-empty and valid email(s)
   → 'subject' is non-empty (warn if blank, don't block)
   → Attachment paths exist (error if not)

7. If validation passes:
   → Prompt user: "Send? [y/n]" (in TUI)
   → Or auto-send if configured

8. On send:
   → Convert markdown body to multipart:
     - text/plain: the raw markdown (or a plain-text rendering)
     - text/html: markdown rendered to HTML via comrak
   → Attach files from the attach list
   → Build RFC 2822 message
   → Dispatch to MailSendProvider (Gmail API or SMTP via lettre)
   → Store in sent messages locally
   → Clean up temp file
   → Remove draft from SQLite
```

## Markdown to multipart rendering

Email messages can be multipart: `text/plain` + `text/html`. mxr generates both from the markdown source.

### text/plain part

The raw markdown, with frontmatter and context block stripped. Most plain-text email clients will display this directly, and markdown is readable as plain text.

### text/html part

Rendered via `comrak` (Rust CommonMark/GFM parser). Supports:
- Headings, bold, italic, strikethrough
- Lists (ordered and unordered)
- Code blocks with syntax highlighting (optional)
- Links
- Block quotes
- Tables (GFM extension)

The HTML is wrapped in a minimal, clean template (no heavy CSS, no images, just semantic HTML that email clients render well).

## Draft management

Drafts persist in SQLite. If the user starts composing and quits $EDITOR without saving, or saves but doesn't send, the draft is preserved. They can resume later:

- `mxr drafts` — list all drafts
- Command palette: "Resume draft" → select from list → opens $EDITOR with the draft file

Drafts are local-only. They are not synced to the provider's drafts folder (Gmail Drafts, IMAP \Drafts). This is intentional: local drafts are faster and don't create noise on other devices. If we later want provider draft sync, it's an opt-in feature.

## Editor integration details

### Spawning the editor

```rust
let editor = std::env::var("EDITOR")
    .or_else(|_| std::env::var("VISUAL"))
    .unwrap_or_else(|_| "vi".to_string());

let status = Command::new(&editor)
    .arg(&draft_file_path)
    .status()
    .await?;

if status.success() {
    // Editor exited normally, process the draft
} else {
    // Editor was killed or errored, don't send
}
```

### Fallback chain

1. `$EDITOR` environment variable
2. `$VISUAL` environment variable
3. Config file: `editor = "nvim"`
4. Fallback: `vi`

### File watching (optional enhancement)

For editors that stay open (VS Code), we could watch the file for changes using `notify` crate (filesystem events). But for v1, the simple "wait for editor to exit" flow covers vim/neovim/helix/nano which are the primary target audience.
