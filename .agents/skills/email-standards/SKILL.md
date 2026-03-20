---
name: email-standards
description: >
  Email RFC standards and protocol reference for the mxr email client. Use when:
  (1) parsing or constructing RFC 5322/MIME messages,
  (2) implementing IMAP protocol features or extensions,
  (3) handling SMTP sending,
  (4) working on email threading, addressing, or headers,
  (5) handling attachments, encoding, or internationalization,
  (6) deciding which RFC governs a feature.
metadata:
  author: planetaryescape
  version: "1.0.0"
  domain: email-protocols
  triggers: RFC, email, MIME, IMAP, SMTP, threading, headers, attachments, encoding, address, Message-ID, Content-Type, multipart, unsubscribe, calendar
---

# Email Standards for mxr

Quick-access reference for email protocol decisions. For comprehensive RFC listings, read `docs/reference/email-standards.md`.

## Task → RFC → mxr Implementation

| Task | RFCs | mxr Implementation |
|------|------|--------------------|
| Parse message format | RFC 5322 | `mail-parser` crate (IMAP); custom header extraction (Gmail API) |
| MIME structure / body parts | RFC 2045-2049 | `mail-parser` extracts parts; `walk_parts()` in `provider-gmail/src/parse.rs` |
| Attachment filenames | RFC 2183 + RFC 2231 | `mail-parser::MimeHeaders::attachment_name()` handles both |
| Multipart messages | RFC 2046 | `multipart/alternative` → text/plain + text/html; `multipart/mixed` → body + attachments |
| Encoded words in headers | RFC 2047 | `mail-parser` decodes automatically; Gmail API returns decoded |
| Threading | JWZ + RFC 5256 | Local JWZ in `crates/sync/src/engine.rs`; Gmail provides native `thread_id` |
| Message-ID generation | RFC 5322 §3.6.4 | `build_rfc2822()` in `provider-gmail/src/send.rs` uses `<uuid7.mxr@localhost>` |
| Reply construction | RFC 5322 §3.6.4 | Set In-Reply-To to parent Message-ID; build References chain; compose frontmatter `in_reply_to` |
| Address format | RFC 5322 §3.4 | `parse_address()`/`parse_address_list()` in `provider-gmail/src/parse.rs`; `mail-parser` for IMAP |
| Date format | RFC 5322 §3.3 | `chrono::DateTime` parsing; Gmail uses `internalDate` millis |
| SMTP sending | RFC 5321 | `lettre` crate handles protocol; `crates/provider-smtp/src/lib.rs` |
| IMAP protocol | RFC 3501 / RFC 9051 | `async-imap` crate; `crates/provider-imap/src/session.rs` |
| IMAP delta sync | RFC 7162 (CONDSTORE/QRESYNC) | UID-based sync cursor in `provider-imap/src/types.rs` |
| IMAP push | RFC 2177 (IDLE) | `async-imap` IDLE support; 29-minute re-issue timeout |
| IMAP mailbox discovery | RFC 6154 (SPECIAL-USE) | Folder name matching in `provider-imap/src/folders.rs` |
| List-Unsubscribe | RFC 2369 + RFC 8058 | `parse_list_unsubscribe()` in `provider-gmail/src/parse.rs` |
| Content-Transfer-Encoding | RFC 2045 §6 | `mail-parser` handles decoding; Gmail uses base64url |
| HTML → text rendering | No RFC (rendering choice) | `html2text` crate + optional external command |
| Compose markdown → HTML | No RFC (CommonMark) | `comrak` crate in `crates/compose/src/render.rs` |
| MIME message construction | RFC 2045-2046 | `lettre::MultiPart` for SMTP; raw RFC 2822 in `provider-gmail/src/send.rs` |
| Flag mapping | IMAP flags spec | `\Seen`→READ, `\Flagged`→STARRED, `\Draft`→DRAFT, `\Deleted`→TRASH, `\Answered`→ANSWERED |
| Internationalized email | RFC 6530-6533 | Not yet implemented |
| format=flowed | RFC 3676 | Not yet implemented |
| Calendar invitations | RFC 5545 + RFC 6047 | Not yet implemented |
| Authentication display | RFC 8601 | Not yet implemented |

## Library → RFC Coverage

### mail-parser 0.9
- RFC 5322: Full message parsing (headers, body, structure)
- RFC 2045-2049: MIME parsing, Content-Transfer-Encoding decoding
- RFC 2047: Encoded word decoding in headers
- RFC 2183: Content-Disposition parsing
- RFC 2231: Parameter value character set / language / continuation

### lettre 0.11
- RFC 5321: SMTP protocol
- RFC 5322: Message construction (when using Message builder)
- RFC 2045: MIME message building
- STARTTLS, AUTH mechanisms

Note: mxr bypasses lettre's Message builder in Gmail send path (constructs RFC 2822 manually in `build_rfc2822()`).

### async-imap 0.10
- RFC 3501: IMAP4rev1 (SELECT, FETCH, STORE, COPY, EXPUNGE, LIST)
- RFC 2177: IDLE (if enabled)
- Partial RFC 7162: CONDSTORE support

### html2text 0.14
- No RFC — renders HTML to plain text for reader mode

### comrak 0.31
- No RFC — CommonMark markdown to HTML for compose

## mxr-Specific Patterns

### Two send paths
- **SMTP** (`provider-smtp`): Uses `lettre::Message::builder()` → `MultiPart::alternative()` with text/plain + text/html
- **Gmail** (`provider-gmail/src/send.rs`): Constructs raw RFC 2822 string manually → base64url encodes → POST to `messages/send`

### Two parse paths
- **IMAP** (`provider-imap/src/parse.rs`): `mail_parser::MessageParser::default().parse(raw_bytes)` → extract via trait methods
- **Gmail** (`provider-gmail/src/parse.rs`): Parse Gmail API JSON response → extract headers via `find_header()`, walk `payload.parts` for bodies

### Threading
- Gmail: native `thread_id` from API
- IMAP: local JWZ threading using Message-ID, In-Reply-To, References headers
- Priority: References > In-Reply-To > Subject-based grouping
- Phantom containers: when a referenced message is missing, create empty placeholder

### Internal model (provider-agnostic)
- `Envelope`: Message-ID, In-Reply-To, References, From, To, CC, BCC, Subject, Date, UnsubscribeMethod
- `MessageBody`: text_plain, text_html, attachments, fetched_at
- `SyncedMessage`: Envelope + MessageBody (eager body fetch — no lazy load)
- All provider-specific data mapped INTO this model by adapters

## Known Gaps & Bugs

1. **`build_rfc2822()` encoding bug**: Declares `Content-Transfer-Encoding: quoted-printable` but does not actually QP-encode the body content. Raw UTF-8 may contain lines > 76 chars or bare `=` signs.
2. **Incomplete References in replies**: Only sets `References: {in_reply_to}` — should copy the original References chain and append the parent Message-ID.
3. **Message-ID domain**: Uses `@localhost` instead of the sender's domain.
4. **No List-Unsubscribe in IMAP**: Parsing only implemented in Gmail adapter. IMAP adapter has raw headers via mail-parser but doesn't extract List-Unsubscribe.
5. **No format=flowed (RFC 3676)**: Plain text not wrapped/reflowed per format=flowed parameter.
6. **SMTP attachments**: `draft.attachments` populated but not used in lettre message construction.
7. **Comment syntax in addresses**: Gmail custom parser doesn't handle `addr (comment)` form.

## Decision Guidance

### Which RFC for which task?

- **"How should I parse this email?"** → Let mail-parser handle it. It covers RFC 5322, MIME, encoded words, parameter encoding. Don't reimplement.
- **"How should I construct a reply?"** → RFC 5322 §3.6.4: Set `In-Reply-To` to parent's Message-ID. Set `References` to parent's References + parent's Message-ID. Quote attribution line: `On {date}, {sender} wrote:`.
- **"How should I handle this IMAP feature?"** → Check if async-imap supports it. If not, check RFC 9051 (IMAP4rev2) for the capability string and protocol details.
- **"Should I add this IMAP extension?"** → See tier list in `docs/reference/email-standards.md` §17.
- **"How should attachment filenames be decoded?"** → mail-parser handles RFC 2183 `filename` and RFC 2231 `filename*` parameters. Check both `Content-Disposition` filename and `Content-Type` name parameter.
- **"How should I handle internationalized addresses?"** → RFC 6530-6533. Not yet implemented in mxr. When adding, ensure UTF-8 in SMTP envelope (SMTPUTF8), headers (RFC 6532), and IMAP (RFC 6855 UTF8=ACCEPT).

### When to use mail-parser vs manual parsing

- **Always prefer mail-parser** for RFC 5322/MIME parsing. It handles edge cases (obsolete syntax, charset detection, encoded words, nested multipart) that are extremely hard to get right manually.
- **Manual parsing only for**: Gmail API JSON responses (not RFC 822), IMAP protocol commands (handled by async-imap), and YAML frontmatter in compose.
- **Never manually parse**: RFC 822 raw bytes, MIME boundaries, Content-Transfer-Encoding, encoded words, or email addresses from raw headers.
