---
candidate: reader-quote-sig
status: wont-do
decision: wont-do
mxr_source: crates/reader/src/quotes.rs, crates/reader/src/signatures.rs
last_reviewed: 2026-05-16
---

> **Status: won't-do.** Fails the publishing bar
> ([`docs/extracted-crates/lessons/10-publishing-bar.md`](../../extracted-crates/lessons/10-publishing-bar.md)):
> the ecosystem gap is real (Mailgun's Talon and `email_reply_parser` are
> Python/Ruby; the Rust port is abandoned), but mxr's implementation is
> English-only heuristics — credibly closing the gap would require ~1-2
> weeks of corpus work (multilingual quotes, Outlook variants,
> classifier-grade signature detection). Shipping the current code as a
> v0.1.0 would mislead users. Revisit only if mxr's own product needs force
> the hardening — then the extraction is free.

# `reader-quote-sig` (proposed name, not final)

> Strip quoted replies and trailing signatures from email bodies. Like
> Mailgun's Talon (Python) or GitHub's email_reply_parser, but for Rust.

## Decision: **Tier 3 — defer**

There is a real, sizeable ecosystem gap here. Talon-class libraries are
widely used in Python. The Rust analogue (`email_reply_parser` crate)
has ~1,458 lifetime downloads and is effectively abandoned. The demand
is real.

**But** the mxr implementation is too lightweight to credibly anchor a
new crate today. Defer until either we invest in hardening it (week-plus
of work), or a clear external pull forces our hand.

## What mxr has today

**Sources:**
- `crates/reader/src/quotes.rs`
- `crates/reader/src/signatures.rs`

### Quotes (`quotes.rs`)

```rust
static ON_WROTE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^On .+wrote:\s*$").expect(...)
});
static QUOTE_PREFIX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^>+\s?").expect(...)
});
```

Detects:
- `"On X wrote:"` attribution lines
- `> `-prefixed quoted lines (any depth)

Does **not** detect:
- Non-English attributions (`"Le X a écrit :"`, `"Am X schrieb:"`,
  `"X 写道:"`, etc.)
- Outlook-style separators (`-----Original Message-----`)
- Gmail mobile attributions (`"---------- Forwarded message ---------"`)
- HTML blockquote markers (mxr operates on text after HTML→text
  conversion, but the blockquote semantic is lost)
- Indented quotes (some clients indent rather than prefix with `>`)
- Pipe-prefixed quotes (`| foo`)

### Signatures (`signatures.rs`)

```rust
// RFC 3676 check
if let Some(pos) = lines.iter().position(|l| *l == "-- " || *l == "--") {
    // split body / sig
}

// Heuristic: block of short lines at end
let is_sig_line = line.len() < 60 && (PHONE_PATTERN.is_match(line)
    || line.contains('@') || line.contains("http") || line.contains("www."));
```

Detects:
- RFC 3676 `"-- "` delimiter (with optional missing trailing space)
- `"Sent from my iPhone/Android/Outlook"` mobile-client signatures
- Trailing block of ≤3 short lines containing phone, email, http, or www

Does **not** detect:
- Multi-line corporate footers with disclaimers ("This email and any
  attachments…")
- vCard signatures
- ASCII-art signatures
- Multiple signatures (forwarded chain with each forwarder's sig)
- Image-only sigs (after HTML→text, become alt-text or vanish)

## Ecosystem state

| Library | Language | Health | Coverage |
|---|---|---|---|
| [Mailgun Talon](https://github.com/mailgun/talon) | Python | Active | Reply + sig, ML-augmented |
| [GitHub email_reply_parser](https://github.com/github/email_reply_parser) | Ruby | Maintained | Reply only |
| [email_reply_parser](https://crates.io/crates/email_reply_parser) (Rust port) | Rust | **Abandoned**, 1 version, ~1.4K downloads | Partial reply parsing |
| Hand-rolled | — | — | Every email project rolls its own |

This is a **large ecosystem gap**. The Rust port died, leaving the
problem unfilled.

## Why our code is not yet publication-ready

Three reasons:

1. **English-only attribution patterns.** A serious reply parser needs
   to handle the multi-language attribution corpus that Talon has
   accumulated over years (French, German, Spanish, Italian, Portuguese,
   Dutch, Polish, Russian, Chinese, Japanese, Korean, Arabic — at minimum).

2. **Outlook quoting style is missing.** Outlook is still very common in
   enterprise mail. Its `-----Original Message-----` separator and
   `From:`/`Sent:`/`To:`/`Subject:` block need explicit handling.

3. **Signature heuristics are too eager.** The "3 short lines at end"
   rule will false-positive on legitimate body content (a short closing
   paragraph, a quoted aphorism). Talon uses a trained classifier for
   the borderline cases. Our heuristic doesn't.

To ship credibly we'd need to either:

- **Invest in hardening** — gather a multilingual attribution corpus,
  add Outlook handling, tighten the sig heuristic with conservative
  rules — about a week of focused work.
- **Or port Talon's regex/heuristic layer** (its ML layer is optional)
  to Rust — clean-room or with a compatible licence — about two weeks.

Neither is a tomorrow project. Until we make that investment, shipping
the mxr code as-is would create a third sub-par crate in the space, not
a credible one.

## Why this is worth thinking about anyway

- **The gap is real and persistent.** The Python community has Talon.
  The Ruby community has email_reply_parser. The Rust community has
  nothing maintained. A good crate here would see broad use.
- **mxr would benefit.** Hardening our own implementation has direct
  value inside mxr (better summarisation context, cleaner LLM prompts,
  better thread rendering). Even if we never ship externally, the
  internal hardening is justified.
- **Possible co-investment.** This is the sort of problem where a public
  crate could attract outside contributors — exactly because so many
  projects need it.

## When to act

Trigger conditions for moving this to Tier 1 or 2:

1. We need to harden mxr's own reader anyway for an internal feature
   (better summarisation, thread compaction, LLM context trimming). At
   that point, doing the work behind a clean public API costs little
   extra.
2. Someone else publishes a credible Rust competitor. Contribute there
   instead.
3. A user-facing reason inside mxr drives us to add multi-language
   attribution handling — at which point publish what we built.

Until one of those fires, this stays Tier 3.

## Proposed public API (when we do it)

```rust
pub struct Reply {
    /// The reply text the user actually wrote, with quotes and signature stripped.
    pub body: String,
    /// The signature block (if detected).
    pub signature: Option<String>,
    /// The quoted portion (each level as a separate block).
    pub quoted: Vec<QuotedBlock>,
}

pub struct QuotedBlock {
    pub depth: u32,
    pub attribution: Option<String>, // "On X wrote:"
    pub text: String,
}

pub fn parse_reply(body: &str) -> Reply;
pub fn parse_reply_with(body: &str, options: ParseOptions) -> Reply;

pub struct ParseOptions {
    pub attribution_patterns: Vec<Regex>, // multilingual extensibility
    pub aggressive_signature: bool,        // tighter / looser sig detection
}
```

## Hardening checklist (if/when we ship)

- [ ] Multilingual attribution corpus (≥10 languages)
- [ ] Outlook separator handling
- [ ] Gmail mobile separator handling
- [ ] Nested-quote depth preservation
- [ ] Indented-quote detection (non-`>` styles)
- [ ] Conservative signature heuristic (false-positive-resistant)
- [ ] Corporate disclaimer detection (`"This email and any attachments"`,
      `"CONFIDENTIALITY NOTICE"`)
- [ ] Configurable patterns via `ParseOptions`
- [ ] Property tests + corpus tests (use public mailing-list archives
      as a corpus)

## Estimated effort to ship credibly

- **1 week** if we port the regex layer from Talon under a compatible licence
  (Talon is Apache-2.0, compatible).
- **2 weeks** for a clean-room implementation with our own corpus.

## Risks and unknowns

- **License of Talon corpus.** Talon's attribution patterns are
  Apache-2.0; we can adapt them. Talon's *trained model* is its own
  artefact; we'd ship without ML or train our own.
- **False-positive risk.** A reply parser that strips legitimate body
  content is worse than one that does nothing. Bias toward conservative
  detection — when in doubt, leave the content.

## Naming

Candidates:

- `reply-parser` — descriptive
- `mail-reply` — concise
- `email-reply` — concise
- `unquote` — cute but cryptic
- `talon-rs` — too tied to the Python project

Recommended: **`reply-parser`** if/when we ship.
