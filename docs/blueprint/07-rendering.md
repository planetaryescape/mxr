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
# file printed by `mxr config path`
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

### Untrusted HTML is parsed in a helper process

html5ever and html2text have super-linear paths that no cheap pre-scan bounds. Formatting-element reconstruction (`<p>`, 6,000 distinct `<b id=…>`, then 6,000 `</p><p>x`) builds about 36 million DOM elements from 113 KB. A tag with 100,000 attributes costs the tokenizer about 5 × 10^9 comparisons. Foster parenting, entity-encoded spans (`rowspan="6&#48;00"`) and tables hidden by a comment each have their own. An allocation failure aborts the process, which `catch_unwind` cannot stop. Six review rounds of input limits kept finding the next one.

So the limits are the first line and a process boundary is the second. `crates/reader/src/html_limits.rs` still refuses a document over 2 MiB, 20,000 tags, 64 attributes in a tag, 20,000 cells, 64 cells in a row or 200,000 grid slots, because that is cheap and keeps ordinary refusals fast. Every parse of mail HTML then runs in a child process, `mxr __html-worker`, which is the same executable in a hidden mode. The daemon, the TUI and each CLI invocation own their own child, started on the first parse.

Each parse is a named job (`reader.render`, `reader.split_quote`, `reading.walk_html`, `reading.article`, `reading.issue_reader_html`, `safety.html_views`, `outbound.validate_html`, `outbound.text_alternative`, `tui.rich_lines`, `daemon.cid_references`, `daemon.image_sources`) defined next to the code that uses it by `mxr-html-worker`'s `Job`. The same function runs in both places, so a normal message renders byte for byte as it did in-process. With no helper installed (unit tests, library use) the job runs on the calling thread.

The child limits itself before it reads a request:

| Limit | Value | Where it holds |
|---|---|---|
| Memory | 512 MiB above the child's starting size | A watchdog thread aborts the child when its peak resident size passes the ceiling (Linux and macOS). On Linux `RLIMIT_AS` also caps the child's address space at its starting size plus the same headroom. macOS refuses a cap near real usage (every process reserves hundreds of gigabytes of address space) and does not enforce `RLIMIT_DATA`, so the watchdog is its only memory limit. |
| CPU | 8 s per request | `RLIMIT_CPU`, re-armed before each request. The soft limit moves and never passes an inherited hard limit (`ulimit -t`), which cannot be raised. A backstop behind the wall-clock limit. |
| Wall clock | 5 s per request | The parent. It kills and reaps the child when a reply is late. |
| Stack | 64 MiB for the job thread | Deeply nested documents recurse; stack is address space, not memory, until used. |

When a request times out or the child dies, the caller gets `None` and uses the fallback it already has for a document over the budget: the linear tag strip for text, no links or images for extraction, a refusal for the send-side checks. The next request starts a fresh child. A request that times out is remembered by hash and answered with the fallback from then on in that process, so one hostile message cannot cost a timeout per view or per index pass. A request that kills the child is retried once on a fresh child, to tell a bad input from a child the operating system killed; dying twice marks the input the same way.

Two children serve at most two requests at once; more callers queue. Callers block: on a multi-thread tokio runtime the wait runs under `block_in_place`, so other tasks keep running. A document is never sent to the child when its job refuses its size (2 MiB for mail; 5 MiB for a fetched web page, the fetch policy's cap).

The child sees nothing from the environment but its limits. It exits when its stdin closes, so a parent that is killed leaves no child behind. On Linux the child is started from `/proc/self/exe`, so a daemon that outlives an upgrade starts the binary it is running. Elsewhere it is the path of the running executable, or the name the process was started as when that path is unknown. A child that cannot be started gives every parse its fallback; parsing never returns to the parent.

`mxr doctor` and the diagnostics page show one line from the daemon, for example `HTML helper: 2 fallbacks since start (4012 requests, 1 timeouts, 1 helper deaths, 2 restarts, 0 job errors)`. The counts hold no message content. `installed: false` means parsing is in-process, or there is no daemon to ask.

What the helper does not do: it is not a sandbox. The child can still read files and open sockets as the user; the limits are about resources, not authority. Output the helper returns is untrusted data like the input.

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
- **Search indexing**: Tantivy indexes the message's text with HTML converted to text and no markup, CSS or scripts. Quoted replies and signatures stay searchable, because the index runs none of the reader's removal steps. The index is rebuilt once when this text changes (`search_index.content-version` next to the index directory)
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
