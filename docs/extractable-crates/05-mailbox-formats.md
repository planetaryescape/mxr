---
candidate: mailbox-formats
status: tier-2
decision: ship-after-gmail-query
mxr_source: crates/export/src/mbox.rs (mbox writer only)
last_reviewed: 2026-05-16
audit_notes: |
  Passes the publishing bar (lessons/10): real ecosystem gap, multi-variant
  escaping + atomic Maildir delivery are non-trivial, audience is wider
  than format-flowed (archive tools, migration utilities, forensic
  analysis). Effort is meaningful (~1-2 days agent-assisted) but not "an
  afternoon." Ship after gmail-query.
---

# `mailbox-formats` (proposed name)

> Read and write classic local-mail storage formats: mbox (mboxo /
> mboxrd / mboxcl / mboxcl2), Maildir (RFC 6884 / qmail dir conventions),
> and MH. Single crate covering the canonical local formats.

## Decision: **Tier 2 — worth doing later**

mxr has a correct, well-tested mboxrd writer. That's a solid seed but not
a complete library. The Rust ecosystem's mailbox-format coverage is weak
across the board, so a unified crate would be valuable — but to be
credibly the canonical option, it needs reading, writing, and at least
two formats. That's meaningful new work beyond what mxr already provides.
Defer until after the Tier 1 ships.

## What mxr has today

**Source:** `crates/export/src/mbox.rs`

A mboxrd-style writer:

- Correct `From `-line escaping (prefix `>` per mboxrd convention)
- CRLF line endings (RFC 2822 / RFC 5322 conformant)
- `asctime` envelope date format
- Header reconstruction when raw headers are unavailable
- Tested for escaped From-lines, multiple messages, empty bodies

```rust
// Body (escape lines starting with "From " per mbox convention)
if let Some(text) = &msg.body_text {
    for line in text.lines() {
        if line.starts_with("From ") {
            out.push('>');  // mboxrd escaping
        }
        out.push_str(line);
        out.push_str("\r\n");
    }
}
```

What mxr does **not** have:

- mbox reader
- mboxo / mboxcl / mboxcl2 variants (only mboxrd)
- Maildir writer
- Maildir reader (depends on the `maildir` crate elsewhere or doesn't read at all — mxr is API-first, not a local-storage client)
- MH support
- Lock-file handling for shared mbox files (`.lock` files, `flock`)

## Ecosystem state

| Crate | Coverage | Health |
|---|---|---|
| [`maildir`](https://crates.io/crates/maildir) | Maildir read + basic write | ~1.2K downloads, minimal |
| [`mbox-reader`](https://crates.io/crates/mbox-reader) | mbox read only | Low downloads, narrow |
| [`rust-mailbox`](https://github.com/meh/rust-mailbox) | mixed | Abandoned |
| `mailparse` / `mail-parser` | Parse individual messages, not container formats | Healthy but out of scope |

There is no maintained, unified crate covering both mbox (all variants)
and Maildir, with both reading and writing. This is a **medium-sized
gap** — niche but real, and the bar to credibly own the space is not
unreachable.

## Why this is Tier 2 not Tier 1

Three reasons:

1. **The mxr seed is half a library.** A writer-only mbox crate is a
   step backwards from `maildir` (which at least handles two operations).
   To be credible, the published crate needs reader + writer for at least
   mbox and Maildir.

2. **Smaller audience.** JWZ threading and Gmail-style query parsing
   serve every email client. Mailbox-format crates serve archive tools,
   migration utilities, and forensic analysers — narrower.

3. **Format-zoo complexity.** mboxo vs mboxrd vs mboxcl vs mboxcl2 are
   subtly different, and getting them all right requires careful
   conformance work. Worth doing well, but it's at least a week of focused
   effort.

## Proposed scope (when we do it)

**v1 — mbox.**
- All four variants: `MboxVariant::O | Rd | Cl | Cl2`
- Read: streaming iterator over messages
- Write: append-mode writer with correct escaping per variant
- Lock-file handling on Unix (`flock` + `.lock` sidecar)
- mxr's existing mboxrd writer becomes the seed for the `Rd` variant

**v2 — Maildir.**
- Read: iterate `cur/`, `new/`, `tmp/`
- Write: deliver into `tmp/` then rename to `new/` (atomic delivery)
- Maildir++ extensions (subfolders, flags in filenames)

**v3 — MH.**
- Read and write
- Sequence files
- Low priority — MH is rare in modern environments

## Proposed public API

```rust
// Common message representation. Use mail-parser's Message or our own?
// Most likely our own minimal Message struct that callers convert from/to.

pub struct RawMessage {
    pub headers: Vec<(String, Vec<u8>)>,
    pub body: Vec<u8>,
    pub envelope_from: Option<String>, // for mbox "From " line
    pub timestamp: DateTime<Utc>,
    pub flags: Flags, // seen, replied, flagged, draft, deleted
}

// mbox
pub struct MboxReader<R: BufRead> { /* ... */ }
impl<R: BufRead> Iterator for MboxReader<R> { type Item = Result<RawMessage>; }

pub struct MboxWriter<W: Write> { /* ... */ }
impl<W: Write> MboxWriter<W> {
    pub fn new(writer: W, variant: MboxVariant) -> Self;
    pub fn write_message(&mut self, msg: &RawMessage) -> Result<()>;
}

// Maildir
pub struct Maildir { root: PathBuf, /* ... */ }
impl Maildir {
    pub fn open(path: impl AsRef<Path>) -> Result<Self>;
    pub fn create(path: impl AsRef<Path>) -> Result<Self>;
    pub fn iter(&self) -> impl Iterator<Item = Result<MaildirEntry>>;
    pub fn deliver(&self, msg: &RawMessage) -> Result<MaildirEntry>;
}

pub struct MaildirEntry {
    pub path: PathBuf,
    pub flags: Flags,
    pub unique_id: String,
}
```

## Extraction plan (when we get to it)

**Phase 1 — mbox foundation (Tier 2 ship).**
1. New repo `mailbox-formats`.
2. Lift mxr's mboxrd writer; generalise to `MboxVariant`.
3. Implement mbox reader with proper `From `-line unescaping per variant.
4. Lock-file handling.
5. Round-trip property tests: write → read → assert equality on a corpus.
6. Publish.

**Phase 2 — Maildir.**
1. Maildir reader (iterate `cur/`, `new/`, `tmp/`).
2. Maildir writer (atomic delivery).
3. Flag parsing from filenames (`,S` `,R` `,F` `,D`).
4. Publish minor version.

**Phase 3 — MH.** Low priority. Ship if/when someone asks.

## Estimated effort

Agent-assisted reality (see
[00-publishing-strategy.md](./00-publishing-strategy.md)):

- Phase 1 (mbox reader + writer + variants + lock-file): **~1 day**
- Phase 2 (Maildir reader + writer + flag parsing): **~1 day**
- Phase 3 (MH): **half a day**, if ever

The pre-agent estimates assumed human typing for boilerplate
(per-variant escaping rules, lock-file platform matrix). Agents handle
that. Human time goes to spec-edge-case judgement and cross-platform
testing.

## TS / npm distribution

**Recommended approach: WASM, *if* we ship a JS distribution.**

The audience for a JS mailbox-format library is smaller than the Rust
audience (most JS mail tooling is web-shaped, not local-storage-shaped).
But if we do ship, WASM is correct here because:

- Byte-streaming mbox reading benefits from native perf for large
  archives (>1GB).
- File-system I/O happens on the JS side anyway; the WASM module is
  pure parse/serialise.
- The variant matrix (mboxo/rd/cl/cl2, Maildir, MH) plus platform
  quirks is exactly the kind of edge-case-heavy surface where TS-port
  drift would hurt.

Defer the JS decision until Phase 1 ships and a real npm consumer
appears. Don't pre-emptively build for a JS audience that may never
materialise here.

## Risks and unknowns

- **mbox variant ambiguity.** Real-world mbox files in the wild often
  don't declare their variant. Provide a `MboxVariant::Auto` detector
  that sniffs the first few messages.

- **Locking semantics differ across platforms.** Windows has no `flock`.
  Document the platform matrix; provide best-effort locking with a clear
  caveat.

- **Maildir delivery atomicity.** The `tmp/` → `new/` rename must be on
  the same filesystem. Document this. Provide a "deliver to" function that
  validates same-FS.

- **Character encodings.** mbox is byte-oriented; headers may be MIME-
  encoded or not. Keep `RawMessage::headers` as `Vec<u8>` to preserve
  fidelity.

- **Performance.** A 10GB mbox file should stream, not load into memory.
  The reader must be `BufRead`-based with bounded buffers.

## When to re-evaluate

- If `maildir` upstream adds proper Maildir write + a mbox feature, the
  gap closes. Check upstream activity before starting.
- If a competitor publishes a unified crate, contribute there instead.

## Naming

Candidates:

- `mailbox-formats` — descriptive, generic
- `mboxdir` — mashup, cute but cryptic
- `mailstore` — overloaded term
- `maildirmbox` — clunky
- `local-mail` — too vague

Recommended: **`mailbox-formats`**. Self-documenting.

## Companion direction

A `mailbox-cli` companion binary that converts between formats (mbox →
Maildir, Maildir → mbox) would be useful for users migrating between
mail clients. Out of scope for the library crate, but a natural follow-on.
