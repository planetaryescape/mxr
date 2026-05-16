---
candidate: outbound
status: wont-do
decision: wont-do
mxr_source: crates/outbound/
last_reviewed: 2026-05-16
---

> **Status: won't-do.** Fails the publishing bar
> ([`docs/extracted-crates/lessons/10-publishing-bar.md`](../../extracted-crates/lessons/10-publishing-bar.md)):
> the gap is real but modest, the effort is small (3-5 days, mostly API
> polish on existing code), and the audience is narrower than Tier 1 —
> only people sending mail, not every mail project. No deliverability
> mandate, no non-trivial algorithm. `mxr-outbound` stays internal.

# `mxr-outbound` — **Defer**

> Higher-level email composition: Markdown → multipart/alternative MIME
> (HTML + plain), inline-image embedding, attachment handling, header
> assembly. Sits one layer above `lettre`.

## Decision: **Defer**

Useful code. Real-but-thin gap above `lettre`. Audience smaller than
Tier 1 picks. Worth doing eventually as part of a small "mxr-flavoured"
family of email-utility crates; not worth doing soon.

## What mxr has today

**Source:** `crates/outbound/`

Capabilities:

- **Markdown rendering** via `comrak` → HTML body
- **Plain-text alt** generated from the Markdown source
- **multipart/alternative** assembly with both bodies
- **Inline image embedding** (cid: references resolved from
  attachment list)
- **Header assembly** including reply threading headers
  (`In-Reply-To`, `References`)
- **Attachment encoding** (base64, content-disposition, filename
  encoding per RFC 2231)

Builds on `lettre` for the underlying MIME primitives and SMTP types.
Adds the higher-level Markdown-to-email workflow on top.

## Ecosystem state

| Crate | Coverage |
|---|---|
| [`lettre`](https://crates.io/crates/lettre) | RFC 5321/5322 SMTP send + low-level MIME building |
| [`mail-builder`](https://crates.io/crates/mail-builder) (stalwart) | MIME building |
| [`mail-send`](https://crates.io/crates/mail-send) (stalwart) | SMTP send |
| [`emails`](https://crates.io/crates/emails) | Various levels |
| Hand-rolled Markdown → MIME | Common — most mail-sending apps re-implement |

**The gap:** none of the above offer one-call "give me Markdown, get a
ready-to-send multipart/alternative message with inline images and
attachments". You can compose this yourself with `lettre` +
`pulldown-cmark` (or `comrak`), but every project does it independently
and the wiring is fiddly (cid: rewriting, plain-text generation, header
encoding edge cases).

This is a **small-to-medium** gap. Not unfilled — you can do it
yourself — but a convenience layer would have an audience.

## Why "defer" not "ship"

- **Audience is narrower than Tier 1 picks.** Threading and query parsing
  serve every email project. Markdown → MIME composition serves only
  *sending* applications, a smaller set.
- **Risk of overlap with existing ergonomic crates.** Several attempts
  exist at varying maturity. We'd need to clearly outperform them in
  ergonomics, docs, or coverage.
- **Coupling to mxr's draft conventions.** The current code threads
  through mxr's compose flow (frontmatter, signature blocks, reply
  context). Stripping the mxr-specific bits leaves a smaller core.
- **Maintenance load.** MIME edge cases are infinite. Email rendering
  bugs accumulate forever.

## Why we might still do it

- **mxr earns credibility from a coherent crate family.** Pairing
  `mail-threading`, `mail-query`, `list-unsubscribe`, `format-flowed`,
  and `mail-compose` gives mxr a visible footprint as the "modern Rust
  email stack". That brand has value beyond individual crate downloads.
- **Reduces internal surface.** Extracting forces a clean public API
  inside mxr, which usually finds latent bugs.
- **Low ongoing cost** once shipped — composition is more stable than
  sync.

## Proposed scope (if/when we ship)

```rust
pub struct EmailMessage {
    pub from: Address,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub bcc: Vec<Address>,
    pub subject: String,
    pub markdown_body: String,
    pub attachments: Vec<Attachment>,
    pub inline_images: Vec<InlineImage>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub headers: HashMap<String, String>,
}

pub struct Attachment {
    pub filename: String,
    pub content_type: String,
    pub data: Vec<u8>,
}

pub struct InlineImage {
    pub cid: String,
    pub content_type: String,
    pub data: Vec<u8>,
}

/// Render to a `lettre::Message` ready to send.
pub fn to_lettre(msg: &EmailMessage) -> Result<lettre::Message>;

/// Render to raw RFC 5322 bytes.
pub fn to_rfc5322(msg: &EmailMessage) -> Result<Vec<u8>>;
```

Key design choice: **bind to `lettre`** or stay agnostic. Recommendation:
provide both `to_lettre()` and `to_rfc5322()`, feature-gate the lettre
integration. That way users on `mail-send` or rolling their own can use
the raw bytes path.

## Extraction plan (when we get to it)

1. Strip mxr-frontmatter logic — that lives in `mxr-compose`, not here.
2. Decouple from mxr's draft types; accept the plain `EmailMessage`
   above as input.
3. Decide on the lettre coupling. Recommended: feature-gated.
4. Add comprehensive examples covering: bare Markdown, inline images,
   attachments, threading headers, custom headers, Unicode subject
   encoding.
5. Test corpus including round-trip rendering with mail-parser to verify
   spec compliance.
6. Publish.

## Estimated effort

**3–5 days.** Mostly API polish and test corpus; the code itself is
mostly done.

## Risks and unknowns

- **`comrak` is heavy.** It's a CommonMark + GFM implementation; large
  binary impact. Consider whether to take it as a hard dep or feature-gate.
  Alternative: `pulldown-cmark` (smaller, less feature-complete).
- **Plain-text generation quality.** Markdown → plain text is not
  trivial (preserving structure, list markers, link targets). Use
  `html2text` or our own renderer? Decision to be made.
- **Header encoding edge cases.** RFC 2047 encoded-words, RFC 2231
  filename parameters, address group syntax — all need careful test
  coverage.
- **Email-client compatibility quirks.** Outlook eats certain HTML
  constructs; iOS Mail mishandles others. We can't fix this in the
  library, but document common pitfalls.

## When to re-evaluate

- After the Tier 1 ships are landed and mxr has visible footprint in
  the email-Rust space. At that point a `mail-compose` companion
  benefits from accumulated brand.
- If a competitor publishes a strong contender, contribute there
  instead of adding our own.

## Naming

Candidates:

- `mail-compose` — descriptive, pairs nicely with `mail-parser` and
  `mail-send`
- `email-compose` — okay
- `mxr-outbound` — too internal
- `markdown-mail` — okay, narrow

Recommended: **`mail-compose`**.

## TL;DR

Solid code, real but modest gap, modest audience. Defer until the Tier 1
crates establish footing, then ship as part of a coherent family.
