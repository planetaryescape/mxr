---
candidate: mailbox-formats
source_doc: ../../extractable-crates/done/05-mailbox-formats.md
status: complete-extracted-and-published
external_repo: https://github.com/planetaryescape/mailbox-formats
crates_io: https://crates.io/crates/mailbox-formats
last_reviewed: 2026-05-17
---

# Move `mailbox-formats` to its own repo

> **Status: complete.** Published as
> [`mailbox-formats v0.1.0`](https://crates.io/crates/mailbox-formats) at
> [`planetaryescape/mailbox-formats`](https://github.com/planetaryescape/mailbox-formats).

The playbook applied here is captured in
[`../lessons/`](../lessons/). This file is the per-crate record.

## Goal

Build a real mbox+Maildir library from mxr's mboxrd-writer seed (a
single 195-line function returning a String). Publish as
`mailbox-formats`. Cut mxr over to consume it from the registry.

Done when:

- `planetaryescape/mailbox-formats` is the source of truth.
- `mailbox-formats = "0.1.0"` live on crates.io with docs.rs build.
- `mxr-export`'s `mbox.rs` is a thin adapter over `MboxWriter`.
- `crates/mailbox-formats/` removed from mxr.
- `mxr-export` tests pass against the registry version.

## What was different about this extraction

This was the first **build-from-spec** carve-out. Previous extractions
(`list-unsubscribe`, `mail-query`) lifted production code; the seed was
already most of the library. This time the seed was thin — Phase 0 was
mostly new code anchored to RFC 4155, the DJB Maildir spec, and Dovecot's
documented lock conventions. The mxr writer became the mboxrd test
anchor, not the bulk of the implementation.

Implications for the runbook:
- Phase 0 sub-steps numbered 13 (vs 8-10 for code hoists).
- The contract has to be *designed* rather than *discovered* — the
  prior question "what shape did the working code naturally have"
  becomes "what shape would a fresh library want, anchored to which
  specs."
- The risk surface is larger: every new component (variants, locking,
  Maildir flag parsing) could be wrong in subtle ways.

## Phase -1: Preflight

Per [`../lessons/01-preflight-checks.md`](../lessons/01-preflight-checks.md).
All green at start:

- [x] `cargo search mailbox-formats` confirms name free. API check
      returns 404.
- [x] No `mxr_*` coupling in `crates/export/src/mbox.rs` (verified —
      seed function takes mxr `ExportThread` but does NOT import core
      types; just a thin DTO).
- [x] Categories `["email", "filesystem", "parser-implementations"]`
      all validated via crates.io API.
- [x] MSRV `1.74` (consistent with prior crates).
- [x] Branch posture: `release-clean` with `git add -p` cost accepted.

## Phase 0: Build the library in-tree

Layout under `crates/mailbox-formats/`:

```text
src/
  lib.rs               # re-exports + crate docs
  raw_message.rs       # RawMessage + Flags + with-pattern builders
  error.rs             # Error enum
  lock.rs              # LockStrategy + Lock RAII + libc/windows-sys shims
  mbox/
    mod.rs             # MboxVariant public enum
    variant.rs         # per-variant escape/unescape (pure fns)
    writer.rs          # MboxWriter<W: Write> + Mboxcl Content-Length
    reader.rs          # streaming MboxReader<R: BufRead> + Auto sniff
  maildir/
    mod.rs             # Maildir struct
    flags.rs           # parse :2,SRF suffix (Maildir++)
    reader.rs          # iterate cur/+new/ sorted
    writer.rs          # atomic tmp→new delivery + flag updates
tests/
  roundtrip.rs         # proptest write→read equality
```

### Design decisions (confirmed before Phase 0)

- **v0.1.0 scope**: mbox (all 4 variants) + Maildir from day one. No
  half-library debate.
- **Byte-preserving message shape**: `RawMessage` with
  `Vec<(String, Vec<u8>)>` headers and `Vec<u8>` body. No `mail-parser`
  dep.
- **Full `LockStrategy` enum**: `None`, `Dotlock`, `Flock`, `Fcntl`,
  `FcntlThenDotlock`. Debian-default `FcntlThenDotlock`. Unix
  first-class; Windows degradation documented.
- **`#[non_exhaustive]` everywhere**. Builder methods
  (`with_envelope_from`, `with_timestamp`, `with_flags`) so external
  tests can still construct `RawMessage`.

### Implementation order

1. Scaffold + workspace member entry.
2. `raw_message.rs` + `error.rs` (types only).
3. `mbox/variant.rs` (pure fns, easy to test).
4. `mbox/writer.rs` (calls variant.rs; ports mxr's mboxrd test cases).
5. `mbox/reader.rs` (streaming, BufRead, Auto sniff).
6. `lock.rs` (Unix-first via libc; Windows via windows-sys LockFileEx).
7. `maildir/flags.rs` (parse :2,SRF suffixes).
8. `maildir/reader.rs` (cur+new iteration).
9. `maildir/writer.rs` (atomic tmp→new delivery, DJB filename).
10. `tests/roundtrip.rs` (proptest write→read equality).
11. README per lessons/02.
12. Wire `mxr-export` through the new crate.
13. Quality gates green; commit Phase 0.

### Behaviour changes from the mxr seed

- The seed wrote raw headers verbatim with their original folding. The
  adapter unfolds before passing to `MboxWriter`, which then emits
  unfolded. The snapshot test accepted this (semantically equivalent).
- The seed used `chrono` for asctime formatting. The new crate
  implements asctime directly via Howard Hinnant's date algorithm to
  avoid a chrono dep.

### Phase 0 quality gates

```bash
cargo fmt -- --check
scripts/cargo-test -p mailbox-formats --all-features --tests
scripts/cargo-test -p mxr-export --tests
cargo clippy -p mailbox-formats --all-targets --all-features --locked -- -D warnings
cargo publish --dry-run -p mailbox-formats --allow-dirty
```

43 unit + 3 proptest + 1 doctest = 47 in mailbox-formats; 47 in
mxr-export. All green.

Commit `3692a42 — feat: prepare mailbox-formats for external publish`.

## Phase 1: Subtree split + GitHub repo

Standard split + push. Repo created on 2026-05-17. 454 commits scanned,
1 retained. Cloned as sibling.

## Phase 2: Standalone-ify

- LICENSE-MIT, LICENSE-APACHE copied from `mxr/`.
- `.gitignore` = `/target` only.
- `clippy.toml` with `msrv = "1.74"`.
- `.github/workflows/{ci.yml, publish.yml}` from lessons/06 verbatim.
- One `cargo fmt` reformat in `src/lock.rs` after copying.

Commit `4d07296 — chore: make crate standalone and add CI + publish workflow`.

## Phase 3: Publish via tag push

`CARGO_REGISTRY_TOKEN` set by user (one-time browser flow, ~60s).

```bash
git tag -a v0.1.0 -m "v0.1.0 ..."
git push origin v0.1.0
```

Publish workflow ran fmt/test/doctest/clippy/dry-run gates then
`cargo publish` with the secret. Wall clock ~50s.

Verified on crates.io (HTTP/2 200).

## Phase 4: Cut mxr over

```toml
# mxr/Cargo.toml — workspace.dependencies
- mailbox-formats = { path = "crates/mailbox-formats", version = "0.1.0" }
+ mailbox-formats = "0.1.0"
```

```toml
# mxr/Cargo.toml — workspace.members
- "crates/mailbox-formats",
```

```bash
rm -rf crates/mailbox-formats
cargo check -p mxr-export
cargo tree -p mxr-export -i mailbox-formats
# mailbox-formats v0.1.0
# └── mxr-export v0.5.18
scripts/cargo-test -p mxr-export --tests
# test result: ok. 47 passed
```

Outbound chrono stash + restore for clean lockfile diff (lessons/08).
The lockfile diff was a clean single hunk plus the dev-dep cleanup
(proptest/serde/serde_json/tempfile no longer pulled by the registry
version because they're dev-deps only).

Commit `1e08c93 — chore: consume mailbox-formats from crates.io`.

## Phase 5: Docs and status surfaces

Three surfaces, table-first per lesson 05:

1. `docs/extractable-crates/README.md` — flipped row 05 from "Tier 2 —
   ship next" into the Done table with crates.io + repo links. The
   "ship next" pointer now points at row 07 (sync-engine, investigate-
   first).
2. `docs/extractable-crates/05-mailbox-formats.md` moved to
   `done/05-mailbox-formats.md` with `Status: Shipped` banner.
3. This file added as the executable migration record.

## Phase 6: Lessons captured

New lesson 11 added at
[`../lessons/11-build-from-spec-carve-outs.md`](../lessons/11-build-from-spec-carve-outs.md):
the patterns specific to extractions where the seed is thin and the
public crate is mostly new code anchored to specs. Distinguishes from
lesson 09's "carve out of existing crate" pattern (which assumed
production-credible code being lifted).

## Rollback plan

The cutover commit is `1e08c93`. To roll back:

```bash
git revert 1e08c93
cargo check -p mxr-export
```

The crates.io publish is immutable; rollback only affects mxr's
consumption. If a regression is found post-publish, fix in the
standalone repo and ship `v0.1.1`.

## Final checklist

- [x] Phase 0 in-tree library complete (47 tests pass)
- [x] mxr-export rewired through new crate
- [x] `planetaryescape/mailbox-formats` exists on GitHub
- [x] Standalone-ified
- [x] `CARGO_REGISTRY_TOKEN` secret set
- [x] `v0.1.0` tag + publish workflow green
- [x] `mailbox-formats v0.1.0` live on crates.io
- [x] mxr consumes registry version
- [x] `crates/mailbox-formats/` removed from mxr
- [x] mxr-export tests pass against registry
- [x] Docs surfaces updated
- [x] Lessons captured (file 11)
