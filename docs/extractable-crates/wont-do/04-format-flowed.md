---
candidate: format-flowed
status: wont-do
decision: wont-do
mxr_source: crates/mail-parse/src/lib.rs (decode_format_flowed)
last_reviewed: 2026-05-16
---

> **Status: won't-do.** Fails the publishing bar
> ([`docs/extracted-crates/lessons/10-publishing-bar.md`](../../extracted-crates/lessons/10-publishing-bar.md)):
> RFC 3676 is a 4-page spec, the encoder is mechanical, the decoder is
> ~30 lines, and the audience is narrow (Mutt/Alpine/mailing-list tooling).
> "Implement in an afternoon from the spec" is true. The mxr decoder stays
> internal in `mxr-mail-parse`. Kept for reference; do not revisit unless
> the ecosystem signal changes (e.g. a popular dependency suddenly needs
> this and no standalone exists).

# `format-flowed` (proposed name)

> Encode and decode `text/plain; format=flowed` per RFC 3676. Reflows
> soft-wrapped plain-text email bodies into single paragraphs while
> preserving hard line breaks and quote depth.

## Decision: **Tier 2 — worth doing later**

A small, focused, RFC-anchored crate that fills a real but niche gap. The
decoder exists in mxr and works. The encoder is missing. Ship after the
Tier 1 wins are out the door.

## What mxr has today

**Source:** `crates/mail-parse/src/lib.rs` — `decode_format_flowed()`

mxr's implementation handles:

- Space-stuffed lines (`" "` prefix to escape `From `, `>`, or space-
  starting content)
- Soft line breaks (lines ending with `" "` before newline → reflow)
- Hard line breaks (lines ending without trailing space → preserve)
- Quote depth (`>` prefixes counted; reflow respects depth)
- Signature line (`-- ` per RFC 3676 §4.3)
- DelSp parameter (`DelSp=Yes` deletes the trailing space when reflowing)

mxr does **not** have an encoder. mxr generally sends Markdown rendered
to HTML+plain alt, not format=flowed plain. So this would need new code.

## Ecosystem state

| Crate | Coverage |
|---|---|
| `mail-parser` | Surfaces format=flowed content type but does not decode/reflow |
| `mailparse` | Same |
| `lettre` | Does not encode format=flowed on send |
| Hand-rolled | Various projects implement it inline, mostly incompletely |

**No focused crate exists.** This is a small, real gap.

## Why this matters (slightly)

- **Mutt, Pine, Alpine, and Thunderbird (still default for plain-text
  modes) send format=flowed.** Reading them on narrow displays without
  decoding produces ugly text with arbitrary mid-sentence line breaks.

- **Mailing list archives are largely plain-text format=flowed.** Tools
  rendering archives for the web (mailing-list software, search engines,
  AI training data pipelines) need to reflow correctly.

- **Niche but real.** Not every email project needs it. But the ones
  that do currently roll their own, and most get the edge cases wrong
  (especially space-stuffing and DelSp).

## Why Tier 2 not Tier 1

- **Audience is smaller** than threading or query parsing.
- **Encoder is missing**, so the crate isn't whole.
- **Demand signal is low** — nobody is publicly asking for it in
  Rust forums.

It's the right shape to ship eventually, just not the right shape to
prioritise.

## Proposed public API

```rust
/// Decode a format=flowed plain-text body into paragraph-wrapped text.
pub fn decode(input: &str, options: DecodeOptions) -> String;

pub struct DecodeOptions {
    /// RFC 3676 §4.5. If `DelSp=yes` was on the Content-Type, set this true.
    pub del_sp: bool,
    /// Whether to preserve quote markers in output. Default: true.
    pub preserve_quotes: bool,
}

/// Encode plain text into format=flowed for sending.
pub fn encode(input: &str, options: EncodeOptions) -> String;

pub struct EncodeOptions {
    /// Target line width. RFC 3676 recommends 72. Hard maximum: 78.
    pub line_width: usize,
    /// Emit `DelSp=yes` style (delete trailing space on reflow).
    pub del_sp: bool,
}

/// Convenience: decode preserving quote depth as a structured tree.
pub fn decode_with_quotes(input: &str, options: DecodeOptions) -> Vec<QuotedBlock>;

pub struct QuotedBlock {
    pub depth: u32,
    pub text: String,
}
```

## Extraction plan

**Step 1 — Repo setup.** New repo, dual MIT/Apache.

**Step 2 — Move decoder.** Lift `decode_format_flowed` from
`mxr-mail-parse`. Strip any mxr-specific type deps.

**Step 3 — Write encoder.** New work. Wrap lines at `line_width`,
mark soft breaks with trailing space, escape space-stuffing lines,
honour quote depth.

**Step 4 — Test corpus.** Property test: `decode(encode(s)) == s`
(up to whitespace) for random plain-text inputs. Also test against
RFC 3676 §4.1 worked examples.

**Step 5 — Documentation.** Rustdoc with worked examples for each
direction. README explaining when to use it (Mutt/Alpine compatibility,
mailing list rendering).

**Step 6 — Publish.**

**Step 7 — Use inside mxr.** mxr's reader and outbound paths both
consume this crate.

## Estimated effort

**~Half a day, agent-assisted.** See
[00-publishing-strategy.md](./00-publishing-strategy.md) for the AI-era
effort framework. Encoder is mechanical work given a reference decoder
and the RFC 3676 §4.1 examples; agents handle that cleanly.

## TS / npm distribution

**Recommended approach: native TS port + shared JSON corpus.**

Stable RFC, finite case set, small audience but real demand (Mutt/Alpine
compatibility, mailing list archive rendering). Drift risk is essentially
zero since the spec hasn't changed since 1999. Apply the same
shared-corpus pattern as the Tier 1 crates — RFC 3676 §4.1 worked
examples become JSON fixtures consumed by both Rust and TS test
harnesses.

WASM is also viable; pick port for native debugging, WASM if you want
zero parity work. For format-flowed the choice doesn't matter much
either way.

## Risks and unknowns

- **DelSp semantics.** Some clients send `format=flowed` without
  declaring `DelSp`. Behaviour: default to `del_sp: false` per the RFC,
  document the trap.

- **Quote prefix normalisation.** Some clients emit `>` with no space
  before the content (`>foo`), others with space (`> foo`). Both are
  valid; preserve as-is.

- **Sig delimiter.** `-- ` (with trailing space) is the sig delimiter.
  Don't accidentally strip trailing spaces during quote-prefix
  normalisation; the sig delimiter would lose its meaning.

- **CRLF vs LF.** Input may have either. Normalise on input, output LF
  (callers can convert if they want CRLF for SMTP).

## When to re-evaluate

- If `mail-parser` adds first-class format=flowed decoding, half the
  crate's value evaporates. Check before starting. Note: mail-parser's
  author (stalwart) has historically kept it spec-faithful and not
  added interpretation layers; that pattern is likely to continue.

## Naming

Candidates:

- `format-flowed` — descriptive, matches the RFC parameter name
- `rfc3676` — too cryptic
- `mail-flowed` — okay
- `flowed-text` — okay

Recommended: **`format-flowed`**. Users searching for the problem find it
instantly.
