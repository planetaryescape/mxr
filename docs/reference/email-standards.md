# Email RFCs & Standards Reference

Comprehensive reference of email standards relevant to building and maintaining mxr. Organized by category with status, supersession info, and implementation notes.

---

## 1. Core Email Message Format

| RFC | Title | Status | Notes |
|-----|-------|--------|-------|
| **RFC 5322** | Internet Message Format | Proposed Standard | **The** email format spec. Defines headers (From, To, Subject, Date, Message-ID), message structure, and syntax. Supersedes RFC 2822 (which superseded RFC 822). |
| **RFC 6854** | Update to IMF: Group Syntax in From/Sender | Proposed Standard | Updates RFC 5322 to allow group syntax in From: and Sender: fields. Important for mailing list messages. |
| **RFC 5322bis** (draft) | Internet Message Format (revision) | Draft (emailcore WG) | Part of the IETF emailcore effort to consolidate and clarify RFC 5322. Not yet published. |

### Supersession chain

`RFC 822` → `RFC 2822` → **`RFC 5322`** (current) + `RFC 6854` (update)

---

## 2. MIME (Multipurpose Internet Mail Extensions)

| RFC | Title | Status | Notes |
|-----|-------|--------|-------|
| **RFC 2045** | MIME Part One: Format of Internet Message Bodies | Draft Standard | Defines Content-Type, Content-Transfer-Encoding, MIME-Version headers. Foundational for any email with attachments or non-ASCII content. |
| **RFC 2046** | MIME Part Two: Media Types | Draft Standard | Defines multipart/mixed, multipart/alternative, multipart/related, message/rfc822, text/plain, text/html. Critical for attachment handling and HTML email. |
| **RFC 2047** | MIME Part Three: Non-ASCII Text in Headers | Draft Standard | Encoded-word syntax (`=?charset?encoding?text?=`) for Subject, From display names, etc. Essential for internationalized email. |
| **RFC 2048** | MIME Part Four: Registration Procedures | Best Current Practice | IANA media type registration. Superseded by RFC 4288, then RFC 6838. |
| **RFC 2049** | MIME Part Five: Conformance Criteria | Draft Standard | Defines what a "MIME-conformant" mail UA must do. Conformance checklist. |
| **RFC 2183** | Content-Disposition Header | Proposed Standard | Defines `inline` vs `attachment` disposition, `filename` parameter. Critical for attachment handling. |
| **RFC 2231** | MIME Parameter Value Extensions | Proposed Standard | Character set and language tagging for MIME parameters, continuation for long values. Updates RFC 2045 and RFC 2183. Important for non-ASCII filenames. |
| **RFC 2392** | Content-ID and Message-ID URLs | Proposed Standard | `cid:` and `mid:` URL schemes. Needed for inline images in HTML email (multipart/related). |
| **RFC 2557** | MIME Encapsulation of Aggregate Documents (MHTML) | Proposed Standard | Web pages in email as multipart/related. Relevant for complex HTML emails. |

### Encoding

- **Base64**: Defined in RFC 2045 Section 6.8 (also RFC 4648 for general base64)
- **Quoted-Printable**: Defined in RFC 2045 Section 6.7
- **Content-Transfer-Encoding**: Defined in RFC 2045 Section 6 (7bit, 8bit, binary, quoted-printable, base64)

---

## 3. SMTP (Simple Mail Transfer Protocol)

| RFC | Title | Status | Notes |
|-----|-------|--------|-------|
| **RFC 5321** | Simple Mail Transfer Protocol | Draft Standard | The current SMTP specification. Defines envelope (MAIL FROM, RCPT TO), relay, delivery. Essential for sending. Supersedes RFC 2821. |
| **RFC 5321bis** (draft-44) | SMTP (revision) | Draft (emailcore WG) | Consolidates RFC 5321 + RFC 1846, 7504, 7505. Expected to become the new SMTP standard. |
| **RFC 6152** | 8-bit MIME Transport Extension | Proposed Standard | Allows 8-bit content in SMTP (8BITMIME). |
| **RFC 3207** | SMTP Service Extension for Secure SMTP over TLS | Proposed Standard | STARTTLS for SMTP. Essential for secure sending. |
| **RFC 4954** | SMTP AUTH Extension | Proposed Standard | Authentication for SMTP submission. Required for sending via SMTP. |
| **RFC 6409** | Message Submission for Mail | Internet Standard | Defines submission port 587 (MSA). Distinguishes submission from relay. Supersedes RFC 4409. |
| **RFC 2920** | SMTP Command Pipelining | Proposed Standard | Performance optimization for SMTP. |
| **RFC 3461** | SMTP DSN Extension | Proposed Standard | Delivery Status Notifications. |
| **RFC 3030** | SMTP CHUNKING (BDAT) | Proposed Standard | Large/binary MIME message submission. |

---

## 4. IMAP (Internet Message Access Protocol)

### Core Protocol

| RFC | Title | Status | Notes |
|-----|-------|--------|-------|
| **RFC 9051** | IMAP Version 4rev2 | Proposed Standard | The current IMAP spec (August 2021). Incorporates many previously-optional extensions. Supersedes RFC 3501 for new implementations. |
| **RFC 3501** | IMAP Version 4rev1 | Proposed Standard | The previous IMAP standard. Still widely deployed. Most servers support rev1; rev2 adoption growing. mxr should support both. |

### Essential Extensions

| RFC | Title | Notes |
|-----|-------|-------|
| **RFC 7162** | CONDSTORE and QRESYNC | **Critical for delta sync.** Per-message MODSEQ for flag change detection. Quick resync after disconnect. Supersedes RFC 4551 and RFC 5162. |
| **RFC 2177** | IMAP IDLE | Real-time push notifications. Server sends EXISTS/EXPUNGE/FETCH unsolicited. Folded into RFC 9051. |
| **RFC 6851** | IMAP MOVE | Atomic MOVE command (vs COPY+DELETE+EXPUNGE). Reduces race conditions. |
| **RFC 6154** | LIST Extension for Special-Use Mailboxes | Identifies `\Inbox`, `\Drafts`, `\Sent`, `\Trash`, `\Junk`, `\Archive`, `\All`, `\Flagged`. Essential for automatic mailbox discovery. |
| **RFC 5256** | SORT and THREAD Extensions | Server-side sorting and threading (ORDEREDSUBJECT and REFERENCES algorithms). The REFERENCES algorithm is the JWZ threading algorithm formalized. |
| **RFC 4315** | UIDPLUS Extension | Returns UIDVALIDITY and UID in APPEND/COPY/MOVE responses. Important for tracking messages after operations. |
| **RFC 5258** | LIST Command Extensions | Enhanced LIST with RETURN options, pattern matching, LIST-STATUS. |
| **RFC 5819** | STATUS in Extended LIST | Get STATUS info (MESSAGES, UNSEEN) alongside LIST. Reduces round trips for sidebar. |
| **RFC 6855** | IMAP UTF-8 Support | UTF8=ACCEPT capability for internationalized mailbox names and headers. Folded into RFC 9051. |
| **RFC 8474** | IMAP Object Identifiers | Stable EMAILID, MAILBOXID, THREADID across renames and moves. Very useful for tracking. |
| **RFC 8970** | Message Preview Generation | Server-generated message previews (snippets). Avoids fetching full body for list views. |
| **RFC 5161** | ENABLE Extension | Client requests server capabilities. Folded into RFC 9051. |
| **RFC 5530** | IMAP Response Codes | Standardized error codes (ALREADYEXISTS, NONEXISTENT, etc.). |
| **RFC 7888** | IMAP LITERAL+ (Non-synchronizing Literals) | Performance improvement for APPEND and other literal-using commands. |
| **RFC 2342** | IMAP Namespace | Namespace discovery for personal/other-users/shared mailboxes. |
| **RFC 2971** | IMAP ID Extension | Client/server identification for debugging and statistics. |
| **RFC 4314** | IMAP ACL Extension | Access Control Lists for shared mailboxes. |
| **RFC 3516** | IMAP Binary Content Extension | Fetch binary content without base64 encoding overhead. |
| **RFC 5465** | IMAP NOTIFY Extension | More selective notifications than IDLE (multiple mailboxes). |
| **RFC 4978** | IMAP COMPRESS Extension | DEFLATE compression for the IMAP connection. |
| **RFC 9208** | IMAP QUOTA Extension | Mailbox quota reporting. |

### IMAP4rev2 vs IMAP4rev1 Key Differences

- IMAP4rev2 mandates CONDSTORE, QRESYNC, IDLE, ENABLE, LITERAL+, NAMESPACE, UNSELECT, UIDPLUS
- Supports 63-bit body parts and message sizes
- Integrates UTF-8 support
- Removes deprecated features (`\Recent` flag)
- See RFC 9051 Appendix E for full diff

---

## 5. Email Authentication

| RFC | Title | Status | Notes |
|-----|-------|--------|-------|
| **RFC 7208** | Sender Policy Framework (SPF) | Proposed Standard | DNS-based sender authorization. Supersedes RFC 4408. |
| **RFC 6376** | DomainKeys Identified Mail (DKIM) | Internet Standard | Cryptographic message signing. |
| **RFC 7489** | DMARC | Informational | Ties SPF and DKIM together with policy. Being replaced by DMARCbis. |
| **DMARCbis** (draft-41) | DMARC Standards Track revision | Draft | Replaces RFC 7489. Expected early 2026. |
| **RFC 8617** | Authenticated Received Chain (ARC) | Experimental | Preserves authentication results through forwarding/mailing lists. |
| **RFC 8601** | Authentication-Results Header | Proposed Standard | Standardized header for conveying SPF, DKIM, DMARC results. Email clients should parse and display this. |
| **RFC 8616** | Email Authentication for Internationalized Mail | Proposed Standard | Extends DKIM, SPF for EAI (internationalized addresses). |

### For an email client

The client doesn't implement SPF/DKIM/DMARC checking (MTA's job). But it SHOULD:

- Parse `Authentication-Results` headers (RFC 8601) to display trust indicators
- Potentially display BIMI logos (see Modern Extensions)
- Understand ARC for forwarded message chains

---

## 6. Internationalization (EAI)

| RFC | Title | Status | Notes |
|-----|-------|--------|-------|
| **RFC 6530** | Overview and Framework for Internationalized Email | Proposed Standard | Framework document for EAI. |
| **RFC 6531** | SMTP Extension for Internationalized Email (SMTPUTF8) | Proposed Standard | Allows UTF-8 in SMTP envelope (MAIL FROM, RCPT TO). |
| **RFC 6532** | Internationalized Email Headers | Proposed Standard | Allows UTF-8 in message headers. Essential for non-ASCII addresses/names. |
| **RFC 6533** | Internationalized Delivery Status Notifications | Proposed Standard | UTF-8 DSNs. |
| **RFC 6855** | IMAP Support for UTF-8 | Proposed Standard | UTF8=ACCEPT for IMAP. |

---

## 7. Security & Encryption

| RFC | Title | Status | Notes |
|-----|-------|--------|-------|
| **RFC 8551** | S/MIME Version 4.0 Message Specification | Proposed Standard | Current S/MIME standard. Uses CMS/PKCS#7. Supersedes RFC 5751. |
| **RFC 9580** | OpenPGP (v6) | Proposed Standard | Current OpenPGP standard (July 2024). Defines X25519, Ed25519, SHA-256, AES-128. Supersedes RFC 4880. |
| **RFC 3156** | MIME Security with OpenPGP | Proposed Standard | PGP messages in MIME (multipart/encrypted, multipart/signed). Still current. |
| **RFC 9787** | Guidance on End-to-End Email Security | Informational | Published August 2025. Practical guidance for MUA implementers on E2E encryption. |
| **RFC 9788** | Header Protection for Cryptographically Protected Email | Proposed Standard | Published 2025. Protecting headers (Subject, From) in encrypted messages. |
| **RFC 8314** | TLS for Email Submission and Access | Proposed Standard | Requires TLS for IMAP (993), Submission (465). Deprecates STARTTLS in favor of implicit TLS. |

---

## 8. Transport Security (MTA-level)

| RFC | Title | Status | Notes |
|-----|-------|--------|-------|
| **RFC 8461** | MTA Strict Transport Security (MTA-STS) | Proposed Standard | Forces TLS for SMTP delivery between MTAs. |
| **RFC 8460** | SMTP TLS Reporting (TLSRPT) | Proposed Standard | Reporting mechanism for TLS failures in transit. |
| **RFC 8689** | SMTP Require TLS | Proposed Standard | Per-message TLS enforcement. |

*Transport security RFCs are primarily MTA concerns, not directly implemented by an email client.*

---

## 9. Modern Protocols & Extensions

### JMAP (JSON Meta Application Protocol)

| RFC | Title | Status | Notes |
|-----|-------|--------|-------|
| **RFC 8620** | JMAP Core | Proposed Standard | Session, methods, push, blobs. Modern alternative to IMAP. |
| **RFC 8621** | JMAP for Mail | Proposed Standard | Email, Mailbox, Thread, EmailSubmission. The "IMAP replacement" for mail access. |
| **RFC 8887** | JMAP WebSocket Binding | Proposed Standard | Push notifications over WebSocket. |
| **RFC 9007** | JMAP for MDN | Proposed Standard | Message Disposition Notifications (read receipts). |
| **RFC 9661** | JMAP for Sieve Scripts | Proposed Standard | Manage Sieve filters via JMAP. |

JMAP relevance: Fastmail is the primary JMAP provider. Could be a future mxr adapter. Much simpler than IMAP for delta sync (built-in /changes method).

### BIMI

| Spec | Title | Status | Notes |
|------|-------|--------|-------|
| **BIMI** (draft) | Brand Indicators for Message Identification | IETF Draft | Not yet an RFC. Allows brands to display verified logos next to authenticated email. Requires DMARC p=quarantine or p=reject. |

---

## 10. Threading

| RFC/Spec | Title | Notes |
|----------|-------|-------|
| **RFC 5256** | IMAP SORT and THREAD Extensions | Server-side REFERENCES threading algorithm. Formalization of the JWZ algorithm. |
| **JWZ Threading** | Message Threading (jwz.org) | Jamie Zawinski's original algorithm. Uses References and In-Reply-To headers. The de-facto standard for client-side threading. |
| **RFC 5322 §3.6.4** | Identification Fields | Defines Message-ID, In-Reply-To, References headers. The raw data threading algorithms consume. |

### Threading headers

- **Message-ID**: Unique identifier for each message
- **In-Reply-To**: Message-ID of the direct parent
- **References**: Ordered list of ancestor Message-IDs (most reliable for threading)
- **Subject**: Fallback for subject-based threading (strip Re:, Fwd: prefixes)

### Implementation notes

- JWZ algorithm is the gold standard for client-side threading
- Rust implementation: [mailthread-rs](https://github.com/asayers/mailthread-rs)
- RFC 5256 REFERENCES algorithm = JWZ algorithm formalized for IMAP servers
- Priority: References > In-Reply-To > Subject-based grouping

---

## 11. Calendar & Contacts

| RFC | Title | Status | Notes |
|-----|-------|--------|-------|
| **RFC 5545** | iCalendar Core | Proposed Standard | Calendar event format (VEVENT, VTODO). Supersedes RFC 2445. Relevant for meeting invitation parsing (text/calendar MIME parts). |
| **RFC 5546** | iTIP | Proposed Standard | REQUEST, REPLY, CANCEL for calendar scheduling. |
| **RFC 6047** | iMIP | Proposed Standard | How to transport iTIP via email (calendar invitation emails). |
| **RFC 6350** | vCard Format Specification | Proposed Standard | Contact card format (v4.0). Supersedes RFC 2426. |
| **RFC 9553** | JSContact | Proposed Standard | JSON representation for contacts (modern vCard alternative). |

---

## 12. List Headers & Unsubscribe

| RFC | Title | Notes |
|-----|-------|-------|
| **RFC 2369** | URLs as Meta-Syntax for Core Mail List Headers | List-Unsubscribe, List-Post, List-Help, List-Subscribe, List-Owner, List-Archive. |
| **RFC 2919** | List-Id: A Structured Field for Mailing Lists | Identifies messages from mailing lists. Useful for filtering/labeling. |
| **RFC 8058** | Signaling One-Click Functionality for List Email Headers | List-Unsubscribe-Post header for one-click unsubscribe. Required by Gmail/Yahoo sender guidelines since 2024. |

---

## 13. Sieve (Email Filtering)

| RFC | Title | Notes |
|-----|-------|-------|
| **RFC 5228** | Sieve: An Email Filtering Language | Server-side filtering language. Relevant for mxr's rules engine interop. |
| **RFC 5804** | ManageSieve Protocol | Remote management of Sieve scripts. |

---

## 14. HTML Email Rendering

There is **no formal RFC for HTML email**. The practical standards:

- **HTML 4.01 / XHTML 1.0**: Safest baseline for email rendering
- **Inline CSS only**: Most clients strip `<style>` tags and external stylesheets
- **Total size < 100KB**: Gmail clips emails larger than 102KB

### For mxr (terminal client)

Since mxr renders in terminal, HTML email means:

1. HTML-to-text conversion (reader mode) — what mxr does via `html2text`
2. No need to support CSS, tables, images in TUI
3. **RFC 3676** (Format=Flowed) for plain text wrapping is relevant
4. Browser escape hatch via `xdg-open` for complex HTML

| RFC | Title | Notes |
|-----|-------|-------|
| **RFC 3676** | Text/Plain Format Parameter (format=flowed) | Soft line wrapping for text/plain. Important for proper text display. |
| **RFC 2392** | Content-ID URLs | Inline images via `cid:` URLs in multipart/related. |

---

## 15. Miscellaneous

| RFC | Title | Notes |
|-----|-------|-------|
| **RFC 3282** | Content Language Header | Language tagging for message parts. |
| **RFC 3339** | Date and Time on the Internet: Timestamps | ISO 8601 profile used in email-adjacent protocols. |
| **RFC 3696** | Application Techniques for Checking and Transformation of Names | Practical guidance on email address validation. **Warning**: Contains known errata. |
| **RFC 5965** | Email Feedback Reports (ARF) | Abuse Reporting Format. Relevant for spam reporting. |

---

## 16. Common Pitfalls & Edge Cases

### Parsing

1. **Line endings**: Email requires CRLF (`\r\n`). Many systems use LF only. Be tolerant on input, strict on output.
2. **Header folding**: Long headers are folded with CRLF + whitespace. Must unfold before parsing.
3. **Encoded words (RFC 2047)**: `=?charset?B?base64?=` and `=?charset?Q?quoted-printable?=` in headers. Multiple encoded words may be adjacent (whitespace between them should be removed).
4. **RFC 2231 parameters**: Attachment filenames can be split across multiple parameters (`filename*0=`, `filename*1=`) and charset-tagged (`filename*=utf-8''encoded`).
5. **Obsolete syntax**: RFC 5322 defines an "obsolete" grammar that must be accepted but never generated.
6. **Plus addressing**: `user+tag@domain` is valid per RFC 5322. Never reject it.
7. **Quoted local parts**: `"weird@local"@domain` is valid. The local part can contain almost anything when quoted.
8. **Group syntax in From**: RFC 6854 allows it. Parsers must handle it.
9. **Missing or malformed Message-ID**: Common in spam and old mail. Threading must handle gracefully.
10. **Duplicate headers**: Some headers (Received) are legitimately repeated. Others (From, Subject) should appear once but may be duplicated in malformed mail.

### MIME

1. **Nested multipart**: Messages can have deeply nested multipart structures. Recursive parsing needed.
2. **Missing Content-Type**: Default is text/plain; charset=us-ascii per RFC 2045.
3. **Charset detection**: When charset is missing or wrong, fall back to heuristic detection.
4. **Filename encoding**: Check both `filename` (RFC 2183) and `name` (Content-Type parameter). Check RFC 2231 encoded forms.
5. **Inline vs attachment**: Both `Content-Disposition: inline` with `Content-ID` (for HTML inline images) and `Content-Disposition: attachment` must be handled.
6. **multipart/alternative ordering**: Last part is "preferred" (usually text/html). First is fallback (usually text/plain).

### IMAP

1. **UIDVALIDITY changes**: When UIDVALIDITY changes, all cached UIDs are invalid. Must re-sync entire mailbox.
2. **Untagged responses**: Can arrive at any time, even mid-command. Must be handled asynchronously.
3. **IDLE timeout**: Most servers drop IDLE after 29 minutes. Must re-issue IDLE periodically.
4. **Literal strings**: IMAP uses `{N}\r\n` for literal data. Non-synchronizing literal (LITERAL+) avoids round trips.
5. **Flag atomicity**: FLAGS response replaces all flags (not additive). Must handle accordingly.
6. **Deleted messages**: IMAP `\Deleted` flag + EXPUNGE is two-phase. Don't assume delete = gone immediately.

### Threading

1. **Missing References**: Some MUAs only set In-Reply-To, not References. Thread by In-Reply-To as fallback.
2. **Broken References**: Forwarded messages may have unrelated References. Subject matching as last resort.
3. **Subject normalization**: Strip `Re:`, `Fwd:`, `Fw:`, `RE:`, `FW:` (and internationalized variants: `SV:`, `AW:`, `Ref:`).

---

## 17. Priority Tiers for mxr

### Tier 1 — Implemented or handled by libraries

- RFC 5322 (message format) — via mail-parser
- RFC 2045-2049 (MIME) — via mail-parser
- RFC 2183 (Content-Disposition) — via mail-parser
- RFC 2231 (MIME parameter encoding) — via mail-parser
- RFC 2047 (encoded words) — via mail-parser
- RFC 3501 (IMAP4rev1) — via async-imap
- RFC 5321 (SMTP) — via lettre
- RFC 2369 (List-Unsubscribe) — custom parser in Gmail adapter
- RFC 8058 (One-Click Unsubscribe) — custom parser in Gmail adapter
- RFC 2177 (IDLE) — via async-imap
- RFC 6154 (SPECIAL-USE) — partial, via folder name matching
- JWZ threading — local implementation

### Tier 2 — Should implement next

- RFC 8601 (Authentication-Results display)
- RFC 8474 (OBJECTID) — stable message/mailbox IDs
- RFC 8970 (PREVIEW) — message snippets
- RFC 7162 (CONDSTORE/QRESYNC) — deeper integration for delta sync
- RFC 3676 (format=flowed) — plain text display
- RFC 6854 (Group syntax in From)
- RFC 5545/6047 (iCalendar/iMIP) — meeting invitations
- RFC 2369 in IMAP adapter (currently only Gmail)

### Tier 3 — Future considerations

- RFC 8620/8621 (JMAP) — future adapter
- RFC 8551/9580/3156 (S/MIME, OpenPGP) — encryption
- RFC 9787 (E2E security guidance)
- RFC 6530-6533 (EAI) — full internationalization
- BIMI — brand logos
- Sieve integration (RFC 5228/5804)

---

## 18. Reference Implementations & Resources

### Rust libraries

- **mail-parser** (Stalwart): RFC 5322, MIME, RFC 2047, RFC 2231 parsing
- **mail-auth** (Stalwart): DKIM, ARC, SPF, DMARC verification
- **lettre**: SMTP client
- **async-imap**: IMAP client
- **mailthread-rs**: JWZ threading algorithm

### How modern email clients approach RFC compliance

- **Thunderbird**: Most complete. IMAP4rev1, JMAP (experimental), S/MIME, OpenPGP, Sieve. Working on IMAP4rev2 + QRESYNC.
- **aerc**: Go terminal client. IMAP, SMTP, Maildir, notmuch. JWZ threading vendored since 0.21.0.
- **himalaya**: Rust email client. IMAP via imap-next. Supports rev1 and some rev2.
- **notmuch**: Maildir indexer. Full-text search via Xapian. Threading via References/In-Reply-To.

### Key resources

- [Stalwart Labs RFC list](https://stalw.art/docs/development/rfcs/)
- [JWZ Threading Algorithm](https://www.jwz.org/doc/threading.html)
- [JMAP Specifications](https://jmap.io/spec.html)
- [IETF emailcore WG](https://datatracker.ietf.org/group/emailcore/about/)
- [RFC 9787: E2E Email Security Guidance](https://www.rfc-editor.org/rfc/rfc9787.html)
- [Mozilla IMAP Extensions Support](https://wiki.mozilla.org/MailNews:Supported_IMAP_extensions)
