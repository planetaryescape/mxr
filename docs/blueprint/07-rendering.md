# mxr — Rendering & Reader Mode

## Rendering philosophy

**mxr shows you what the email says, not what it's trying to sell you.**

Distraction-free email is a feature, not a limitation. Newsletters are designed to hijack your attention: tracking pixels, flashy banners, "SALE ENDS TODAY" hero images, animated GIFs. All of that disappears when you strip to plain text. You're left with just the words.

mxr does not render images in the terminal. It does not try to be a web browser. It strips email down to its content and presents it cleanly. For the rare cases where you need the full rich rendering, one key opens the original in your system browser.

## Rendering pipeline

### Priority order

1. **text/plain**: If the email has a plain text part, show it. This covers person-to-person email well since most clients generate both parts.
2. **HTML → plain text**: If HTML-only, convert to clean text via `html2text` crate (built-in) or a configurable external tool.
3. **Browser escape hatch**: `o` keybinding opens the original HTML in the system browser via `xdg-open` (Linux) or `open` (macOS).

### HTML to plain text

Built-in: `html2text` Rust crate. Handles basic structure, links, emphasis.

Configurable override for power users:

```toml
# ~/.config/mxr/config.toml
[render]
# Use external tool for better HTML rendering (tables, complex layouts)
html_command = "w3m -T text/html -dump"
# or: html_command = "lynx -stdin -dump"
# Leave blank or omit to use built-in html2text
```

We considered:
- **Embedding a terminal browser**: Not worth the complexity. Users who want terminal HTML rendering can set `html_command` to w3m or lynx.
- **Rendering images in terminal** (sixel, kitty protocol): Rejected. Inconsistent terminal support, gimmicky, contradicts distraction-free philosophy. Images show as `[image: filename.png]` placeholders.
- **Building a custom HTML renderer**: Way too much scope for marginal benefit.

The external `html_command` approach is the Unix way: mxr handles mail, rendering is someone else's job. Just a clean handoff.

## Reader mode

Reader mode is a second pass on the already-converted plain text. It strips everything that isn't the actual human-written content.

### What reader mode strips

1. **Quoted replies**: Lines prefixed with `>`, or blocks starting with "On {date}, {person} wrote:". Collapsed into `[previous message from alice@example.com]` or hidden entirely. Toggle-able.

2. **Email signatures**: The `-- \n` delimiter (RFC 3676 standard). Also heuristic detection of corporate signatures: consecutive lines with phone numbers, addresses, job titles, legal disclaimers.

3. **Legal/confidentiality boilerplate**: "This email is confidential and intended only for...", "If you have received this email in error...", "DISCLAIMER: ...", etc.

4. **Tracking junk**: "View in browser" links, unsubscribe blocks (the actual unsub link is captured separately — see below), email footer cruft, "Sent from my iPhone" lines.

5. **Empty formatting**: Excessive blank lines, whitespace padding, ASCII art dividers.

### Reader mode output

```rust
pub struct ReaderOutput {
    /// The cleaned content: just the actual human-written text.
    pub content: String,
    /// Quoted messages that were stripped (available for expansion).
    pub quoted_messages: Vec<QuotedBlock>,
    /// The signature that was stripped (available for viewing if needed).
    pub signature: Option<String>,
    /// Attachment references found in the message.
    pub attachments: Vec<AttachmentRef>,
    /// Stats: how much was stripped (for UI display).
    pub original_lines: usize,
    pub cleaned_lines: usize,
}

pub struct QuotedBlock {
    pub from: Option<Address>,
    pub date: Option<DateTime<Utc>>,
    pub content: String,
}
```

### UI display

When reader mode is active (default), the TUI shows:
- The cleaned content
- A status line: `reader mode: 342 → 41 lines` (reinforces the value on every message)
- Attachments listed below the content

Keybinding: `R` toggles between reader mode and raw view. When you need to see the full original (quoted replies, signature, boilerplate), one key shows it.

### Implementation

Reader mode does NOT require ML. A `reader.rs` module with regex patterns and heuristics handles 90%:

- Signature detection: `-- \n` delimiter, phone/address patterns, "Sent from" patterns
- Quote detection: `>` prefix, "On ... wrote:" patterns
- Boilerplate detection: keyword matching on common legal/disclaimer phrases
- Tracking detection: "View in browser", "Unsubscribe", "Update your preferences" link patterns

This is the same pipeline used for:
- **Search indexing**: Tantivy indexes cleaned text, not raw HTML with signatures
- **Thread export**: LLM context format uses reader mode output
- **Rules matching**: Rules can match on cleaned content instead of wading through signature noise

One pipeline, multiple consumers.

## One-key unsubscribe

### Feature summary

Press `D` on any newsletter to unsubscribe. One key. Confirmation prompt. Done. `U` remains mark unread.

### How it works

Most legitimate newsletters include the `List-Unsubscribe` header (RFC 2369). It's a machine-readable header that email clients use to show unsubscribe buttons. mxr parses it at sync time.

The header comes in two forms:

```
List-Unsubscribe: <https://example.com/unsub?token=abc123>
List-Unsubscribe: <mailto:unsub@example.com?subject=unsubscribe>
```

There's also `List-Unsubscribe-Post` (RFC 8058) which enables one-click HTTP POST unsubscribe without opening a browser:

```
List-Unsubscribe-Post: List-Unsubscribe=One-Click
```

### Unsubscribe method priority

```rust
pub enum UnsubscribeMethod {
    /// RFC 8058 one-click POST. Best case: no browser needed.
    /// Daemon fires the POST request directly, silently.
    OneClick { url: String },
    /// HTTP link. Opens in browser for user to complete.
    HttpLink { url: String },
    /// Send an email to unsubscribe. Daemon sends it automatically.
    Mailto { address: String, subject: Option<String> },
    /// Extracted from HTML body (lower confidence than header-based).
    BodyLink { url: String },
    /// No unsubscribe method found.
    None,
}
```

Priority: OneClick > Mailto > HttpLink > BodyLink

### Keybinding flow

```
User presses D on a message
  → If unsubscribe method is None:
    "No unsubscribe option found for this message"
  → If unsubscribe method exists:
    open confirmation modal
  → User chooses:
    → Unsubscribe only
    → Unsubscribe + archive all from this sender
    → Cancel
  → On confirm:
    → OneClick: daemon fires HTTP POST silently. "Unsubscribed."
    → Mailto: daemon sends unsubscribe email automatically. "Unsubscribe email sent."
    → HttpLink/BodyLink: opens in browser. "Opened unsubscribe page in browser."
```

### Visual indicator

In the message list, messages with an available unsubscribe method show a small indicator (e.g., `[U]` or a dimmed icon). This lets users see which messages are newsletters at a glance.

### Where unsubscribe data comes from

1. **Primary**: `List-Unsubscribe` header — parsed at sync time, stored on the envelope as `unsubscribe_method` JSON field.
2. **Fallback**: HTML body scanning — if header is absent, scan the HTML body for common unsubscribe link patterns (href containing "unsubscribe", "opt-out", "manage preferences"). Lower confidence, stored as `BodyLink`.

### Why this matters

Unsubscribe is typically a multi-step process: open email in browser → scroll to bottom → find tiny unsubscribe link → click → confirm on website → maybe enter email again. mxr reduces this to: `D` → confirm. Combined with reader mode, the messaging is: **mxr strips the noise out of newsletters, and when you're done with one, one key to never see it again.**

### RFC 8058 one-click POST detail

The POST request is simple:

```
POST {url_from_List-Unsubscribe_header}
Content-Type: application/x-www-form-urlencoded

List-Unsubscribe=One-Click
```

Most well-behaved newsletter platforms (Substack, Mailchimp, ConvertKit, Buttondown) support this. The user never leaves the terminal.
