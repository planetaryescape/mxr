# Build-from-spec carve-outs

Lesson 09 captured the pattern of carving a new crate out of an
existing one when production code lives in the seed. This file
captures the *next* shape down: extractions where the seed is genuinely
thin (mxr had only a writer; the published crate needs a reader, a
second format, and lock handling).

Captured 2026-05-17 after `mailbox-formats` shipped. mxr's
`crates/export/src/mbox.rs` was a single 195-line `export_mbox(thread)
-> String` function. The published `mailbox-formats v0.1.0` is
~1400 LoC across mbox reader+writer (4 variants), Maildir
reader+writer, `LockStrategy` (5 strategies, two platforms), and 47
tests. The mxr code anchored mboxrd; everything else was new,
spec-anchored work.

## When this lesson applies

Apply this lesson when, before Phase 0:

- mxr has a working partial implementation (good as proof the problem
  is real).
- The public crate would need 2-5× more code than the seed to be
  credible.
- The new code is anchored to public specs (RFCs, vendor docs,
  established conventions).
- You've done the publishing-bar test from lesson 10 and the answer is
  still "yes."

If the seed needs *less* new code than what's already there, lesson 09
is the right pattern (lift + refactor). If the seed needs 10× more
work, you're not extracting — you're starting a new project, and the
extraction question is premature.

## Rule 1: Anchor every component to a written spec

For `mailbox-formats`:

- mbox variants → RFC 4155 + Eric Allman's original docs (via
  Wikipedia)
- mboxo/rd/cl/cl2 escape rules → archived Unix sendmail/Berkeley mail
  documentation
- Maildir → D. J. Bernstein's spec at https://cr.yp.to/proto/maildir.html
- Maildir++ → Sam Varshavchik's Courier extension
- Locking → Dovecot's `mboxlocking` page

If you can't anchor a component to a written reference, you're making
it up. Making it up means future users hit edge cases that your
implementation handles differently from $other_implementation; you
have no defense ("but we follow spec X") and the crate's authority
gets weaker.

Honest fallback: if a component has no canonical spec (e.g. lock
behaviour on Windows), pick a documented convention and cite it.
README explicitly says "we follow Dovecot's convention," not "we
chose this."

## Rule 2: Test against the seed's existing test corpus first

mxr had 12 tests for `export_mbox()`. Those became the mboxrd test
anchor in the new crate's Phase 0. They proved the new writer hit the
same byte-output as the seed for the cases the seed cared about. Three
of them were the exact same `From `-escaping cases — porting them
caught one subtle regression early (asctime expected `Tue Mar 17` but
the new code emitted `Mon Mar 17` because my date math was off by a
day before I fixed it).

**Practice:** port the seed's existing tests *first*, before writing
new functionality. They establish the "we haven't regressed" floor.

## Rule 3: Pure functions get their own module, tested in isolation

mbox variant escape/unescape logic went into `src/mbox/variant.rs` as
8 pure functions on `&[u8]`. That module compiles + tests in
milliseconds. The reader and writer are just thin shells around those
functions plus I/O.

If you put the escape rules inline with reader/writer code, every
test exercises both the variant logic *and* the I/O. Slow, hard to
debug, hard to extend.

**Practice:** any per-variant or per-spec logic that can be expressed
as a pure function lives in its own module with its own test suite.
Reader and writer become thin shells.

## Rule 4: Build the writer before the reader

For round-trippable formats, the writer is the simpler half (you
choose the output shape). The reader has to handle whatever exists in
the wild.

For `mailbox-formats`, writing in Phase 0 step order:

1. Variant escape rules (pure functions)
2. Writer (uses escape rules)
3. Reader (uses unescape rules)
4. Round-trip property test (writer → reader equality)

The round-trip property test catches asymmetries: lines the writer
emits that the reader can't read back, or vice versa. Run it with
`proptest` for 64+ cases.

## Rule 5: `#[non_exhaustive]` + builder methods for external tests

External integration tests (under `tests/`) hit the
`cannot create non-exhaustive struct using struct expression` error
when they try to construct your public struct with literal fields.
Fix: add `with_X` builder methods. The Phase 0 plan for
`mailbox-formats` initially had only `RawMessage::new(headers, body)`;
the proptest in `tests/roundtrip.rs` needed to set
`envelope_from`/`timestamp` too. Adding three `with_*` methods cost
30 lines and made `tests/` happy.

**Practice:** before sealing a `#[non_exhaustive]` struct, list every
field a caller might want to construct in isolation. Add a `with_*`
method per field. This is mechanical.

## Rule 6: Cross-platform IO needs platform shims, not conditional
calls

Lock handling has different syscalls on Unix (`flock`, `fcntl`,
`link`) and Windows (`LockFileEx`, `UnlockFile`). The `lock.rs`
module has two platform modules under `cfg(unix)` / `cfg(windows)`,
each exposing the same internal API:

```rust
#[cfg(unix)] mod platform { ... pub(super) fn flock_exclusive(...) -> ...; }
#[cfg(windows)] mod platform { ... pub(super) fn flock_exclusive(...) -> ...; }
```

The shared code uses `platform::flock_exclusive(...)` and the right
implementation is selected at compile time. The Windows version may
degrade to a no-op (Fcntl on Windows: POSIX record locks don't exist)
but the shape stays consistent.

**Practice:** never `cfg!(unix)` inside a function body. Always put
the cfg at module boundary so the platform impls can diverge cleanly.

## Rule 7: Document the degradation matrix

Cross-platform crates can't pretend to be uniformly capable. The
`mailbox-formats` README has an explicit table:

```
| Strategy             | Where it works    | When to use                |
| `Flock`              | Unix + Windows    | Single-host advisory       |
| `Fcntl`              | Unix only         | NFS-aware POSIX record     |
| `FcntlThenDotlock`   | Unix (Win: dotlock) | Debian default           |
```

Users see what they're signing up for before they get a runtime
surprise. The table is denser than prose and easier to grep.

## Rule 8: Streaming readers need bounded buffers, not full-file slurps

For `mailbox-formats`, a 10 GB mbox file must stream. The `MboxReader`
uses `BufRead`, reads one line at a time, and emits one `RawMessage`
at a time. The reader holds at most one message in memory.

**Anti-pattern:** `let bytes = std::fs::read(path)?; parse_mbox(&bytes)`.
This works in tests, breaks in production.

The sniff buffer for `MboxVariant::Auto` is also bounded at 64 KiB.
Beyond that you'd be peeking at a meaningful fraction of the file
just to guess the format.

## How this lesson differs from lesson 09

Lesson 09 covers the case where mxr already has production-credible
code and the carve is mostly mechanical. Phase 0 is short; the
expensive work is the boundary conversion between the carved crate
and mxr's internal types.

This lesson (11) covers the case where mxr's seed is *thin* — it
proves the problem is real but the published crate needs to do more.
Phase 0 is long; the expensive work is the spec-anchored
implementation. The boundary conversion in mxr can be tiny because
the rewired adapter is shorter than the original code.

**One signal you're in this case:** the post-extraction `mxr-export`
adapter is *smaller* than what it replaced. `crates/export/src/mbox.rs`
went from 195 lines of mboxrd logic to ~110 lines of DTO conversion +
delegating call. The library did the heavy lifting.

## Mistakes from this extraction

- Initial `RawMessage::new()` took only `headers, body`. The proptest
  needed all fields. Added builder methods after the fact.
- The `from_line_has_asctime_date` test asserted `11:36:40` when the
  correct asctime output (UTC, no TZ adjustment) is `09:36:40`.
  Caught immediately because the actual decoded epoch was wrong in my
  head, not in the code. Worth running the asctime computation in a
  scratch script (`python3 -c "from datetime import...`)" before
  writing the test expectation, especially when the test asserts a
  specific time-of-day.
- The combined-lock test failed on first run because I tried to write-
  lock (`F_WRLCK`) a read-only file descriptor. Always open with
  `read(true).write(true)` for `fcntl(F_WRLCK)`.
- `#[non_exhaustive]` outer-attribute on a `mod` declaration doesn't
  propagate `#[allow(unsafe_code)]` to the unsafe blocks inside —
  use `#![allow(unsafe_code)]` inner attribute at the top of the
  module body instead.

These are the kinds of small bugs that hide in build-from-spec work.
Budget time to find them.

## What this lesson is not

This lesson is *not* an excuse to publish a crate before the work is
done. The publishing bar from lesson 10 still applies: real gap,
non-trivial work, *credible* seed. The seed is "credible" in this
context if the carved-out version handles the test cases mxr already
had. If mxr's mboxrd writer had been buggy, this extraction would
have inherited those bugs into the public crate.

Build-from-spec means *expanding* on a credible seed. It doesn't mean
publishing a v0.1.0 that pretends to be more than it is.
