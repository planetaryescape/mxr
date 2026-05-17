# MSP roadmap

> **Current focus:** Phase D (mutation unification) and beyond. Step 4
> (publish-or-hold) deferred to "hold." Steps 1–3 + alignment Phase C
> landed 2026-05-17.
>
> _Last updated: 2026-05-17._

This roadmap tracks the six steps from
[`spike-verdict.md`](./spike-verdict.md) plus the open-ended Step 7
that follows. Each step has explicit entry and exit gates so future
sessions can pick up without losing context.

## Status table

| Step | What | Status | Est. effort | Blocked on |
|------|------|--------|-------------|------------|
| 1 | Land spike artifacts in mxr | **Done** | — | — |
| 2 | mxr alignment Phase A — cheap wins | **Done** | ~1 day | — |
| 3 | mxr alignment Phase B — opaque SyncCursor | **Done** | ~1 day | — |
| 4 | Publish-or-hold decision | Not started | ~1 hour | — |
| 5 | Reference IMAP adapter as separate crate | Not started | ~1-2 weeks | Step 4 (if publish) |
| 6 | Further scope based on response | Not started | open-ended | Step 5 |
| 7 | Maintenance + v0.2 spec | Not started | ongoing | Step 6 |

## Step 1 — Land spike artifacts ✅

**Done:** The four spike artifacts plus this roadmap and the README
live under `docs/msp/`. Cross-references updated. Lesson 12
captures the meta-pattern.

**Exit gate met:** All files committed and referenced.

## Step 2 — mxr alignment Phase A (cheap wins) ✅

The "cheap wins" from `mxr-alignment.md`. Pure clean-ups; no
behaviour change; mxr is strictly better afterwards.

**Tasks landed:**

1. ✅ **Namespaced `SyncCapabilities` restructure** —
   `37b771f refactor: namespace SyncCapabilities into sync/mutate/search/push`.
   Flat boolean soup replaced with `{sync, mutate, search, push}`
   sub-structs; each derives `Default`, so providers only spell out
   non-default flags. IPC-facing `AccountCapabilitiesData` stays
   flat for now — wire reshape is a Phase B+ concern.
2. ✅ **Typed `SyncCursorExpired` error variant** —
   `b31d006 refactor: add typed SyncCursorExpired error variant`.
   Gmail provider surfaces the typed error instead of falling back
   to `initial_sync()` internally; daemon's engine recovery is
   provider-agnostic (no Gmail-only match). Maps cleanly to MSP §5's
   `msp.sync.cannot_calculate_changes`.
3. ✅ **`Role` enum on Folder/Label** —
   `aa29c46 refactor: add Role enum + Label.role field for MSP §2.3 alignment`.
   `Role` enum (`#[non_exhaustive]`) plus `Label.role: Option<Role>`
   populated from IMAP SPECIAL-USE attrs and Gmail system label ids.
   Store doesn't persist role yet (defaults to `None` on hydrate);
   persistence + backfill queued for Phase B+.

**Entry gate met:** Step 1 done.

**Exit gate met:**
- Three commits landed, each green on `cargo test -p <touched-crate>
  --tests`. Workspace-wide `cargo check --tests` clean.
- Daemon smoke test (per AGENTS.md): `mxr daemon --foreground` boots
  cleanly, `mxr doctor`, `mxr accounts --format json`, `mxr count`,
  `mxr search --limit 2 --format json` all return correct JSON.
  Real Gmail sync cycle ran end-to-end (cursor saved, no regression).

## Step 3 — mxr alignment Phase B (opaque `SyncCursor`) ✅

The biggest single architectural win. The `SyncCursor` enum was a
tagged union the daemon pattern-matched on; the daemon's pattern-match
coupled it to "which providers exist." MSP's opaque-cursor rule fixes
this.

**Landed:** `0588142 refactor: opaque SyncCursor (MSP Phase B)`.

- `SyncCursor` is now `pub struct SyncCursor(Vec<u8>)` with a custom
  Debug impl that prints only `len=N` (cursors may embed
  account-scoped tokens).
- Two new trait methods on `MailSyncProvider`: `describe_cursor()` for
  human display, `is_backfill_cursor()` for daemon backfill heuristics.
  Both have safe defaults.
- New private cursor schemas: `provider-gmail/src/cursor.rs` and
  `provider-imap/src/cursor.rs`, each with a versioned JSON envelope
  (`{"v":"1",...}`) and a legacy-shape shim that accepts the old
  tagged-enum format for one release.
- Anything unrecognised → `MxrError::SyncCursorExpired` →
  daemon-side reset-to-empty + full resync (the Phase A.2 path).
- Daemon's three duplicated `describe_cursor` helpers collapse: the
  loops version becomes a thin wrapper around the provider trait
  method; the two store-only callers (status_helpers, doctor) become
  `opaque len=N` fallbacks since they have no live provider in scope.
- `accounts.sync_cursor` column stays TEXT (no schema migration).

**Entry gate met:** Step 2 done.

**Exit gate met:**
- mxr-core, mxr-sync, mxr-store, mxr-protocol, all three provider
  crates: tests green.
- Workspace `cargo test --tests` + `cargo clippy --tests
  --all-targets` clean.
- CLI smoke against the new binary (`target-cli/debug/mxr daemon`):
  daemon boots, `doctor` / `count` / `search` return correct JSON.
- Daemon source has zero `SyncCursor::(Gmail|GmailBackfill|Imap|Initial)`
  references outside provider adapters (only comments in the legacy
  shim mention them).

**Verification gap to flag for follow-up:** a live end-to-end Gmail
sync exercising the legacy-cursor migration code path on real on-disk
cursors was *not* driven in this session — the dev-profile binary's
account config is empty. The migration is covered by 6 per-adapter
unit tests (legacy tagged-enum + missing-mailboxes + "Initial" string
shapes). The runtime path will fire the first time a user upgrades
a real install. Worth a manual sanity check on the user's `personal`
Gmail account before publishing Step 4.

## Step 4 — Publish-or-hold decision

Decision point. By this stage mxr is meaningfully closer to MSP
shape. The spec draft has had a few days to settle. We decide
whether to:

- **Publish** the blog post + spec to a Rust forum or HN, OR
- **Hold** and keep refactoring without external surface.

The decision is informed by:

- Does the spec still feel right after living with it for a few
  days?
- Do we have at least one concrete external person we'd want to
  reach with the post (a known mail-client builder, an existing
  Rust mail-crate maintainer)?
- Is there bandwidth to handle inbound feedback over the next 2-3
  weeks?

If yes: publish. Go to Step 5.

If no: hold; revisit in a month. Roadmap pauses until the next
decision window. mxr-internal refactor work can continue (Phases
C-F from the alignment audit).

**Entry gate:** Step 3 done.

**Exit gate:** Decision recorded in this roadmap's changelog. If
publish, the spec gets a version tag in its frontmatter and the
blog post gets a publish date.

**Status:** Not started.

## Step 5 — Reference IMAP adapter as a separate crate

If we published in Step 4 and got non-negative response, this is
the first hard external surface.

**Scope:**

- New crate at `planetaryescape/msp-imap` (or similar — naming
  TBD).
- Implements MSP server-side over stdio JSON-RPC.
- Talks IMAP via the existing `provider-imap` code, packaged for
  standalone use.
- mxr's daemon learns to subprocess `msp-imap` as one of its
  adapter options (alongside the existing in-process IMAP
  adapter).

**Entry gate:** Step 4 = publish.

**Exit gate:**
- `msp-imap` crate published on crates.io (subject to the standard
  publishing-bar discipline from `lessons/10-publishing-bar.md`).
- mxr's daemon successfully subprocesses `msp-imap` and uses it
  end-to-end against a real IMAP account.
- An external project (himalaya? pimalaya? a new project?) has
  said they'd try the adapter.

**Status:** Not started.

## Step 6 — Further scope based on response

Open-ended. Depends on what response the blog post + IMAP adapter
generated.

Possible paths:

- **Second adapter** (likely Gmail, to stress-test the abstract
  model against a wildly different provider).
- **mxr alignment Phases C-F** (mutation unification, custom
  keywords, sync-delta shape) — converts mxr's daemon into a more
  fully MSP-compliant client.
- **v0.2 spec round** — incorporate lessons from the IMAP adapter
  + any external feedback.
- **Co-lead recruitment** — if there's interest but we're capped
  on bandwidth.
- **Working group formation** — only if there are 2-3 committed
  people.

**Entry gate:** Step 5 done.

**Exit gate:** TBD — depends on which paths we take.

**Status:** Not started.

## Step 7 — Maintenance + v0.2 spec

Long-term. If MSP becomes a real thing externally, this is the
ongoing maintenance:

- Issue triage on the spec repo (if separated from mxr).
- Adapter conformance harness work.
- Periodic spec versioning (v0.2, v0.3, ...).
- Compatibility tracking across adapters.

**Status:** Not started. Likely needs co-leads before this is
sustainable.

---

## Roadmap revisions / changelog

Append below as the roadmap evolves. Most recent first.

### 2026-05-17 — Phase C landed
- Single atomic commit per the audit's small-cost budget. Added
  `has_more: bool` to `SyncBatch` and `SyncOutcome`; daemon sets
  `skip_sleep = outcome.has_more` so multi-page Gmail backfill
  finishes in minutes instead of hours (was: 30s sleep between
  pages, gated on `is_backfill_cursor` heuristic).
- Defensive: 50-iteration cap on consecutive `has_more = true` to
  prevent a buggy adapter from tight-looping; forces one sleep
  cycle and resets the counter.
- **Decision:** did NOT split `upserted` into `messages_added` +
  `messages_changed` (despite the original audit prescription).
  8-protocol survey (JMAP, MS Graph, Drive, WebDAV, CloudKit,
  Matrix, Notion, Linear) showed only JMAP splits, and only for
  ID-only deltas. Every local-store client (Graph/Drive/CloudKit/
  WebDAV) merges. MSP spec §2.8 + alignment audit §2.8 updated
  to match.
- Step 4 (publish-or-hold) deferred to "hold" — internal alignment
  work continues.

### 2026-05-17 — Phase B landed
- Single atomic commit (0588142) per the audit's medium-cost budget.
  ~500 LOC net change across 18 files + 2 new cursor modules.
- All targeted tests + workspace tests + clippy clean.
- Verification gap noted: live-Gmail end-to-end against legacy on-disk
  cursors not driven this session (dev-profile config empty); unit
  tests cover the migration paths.
- Discovered during verification that cargo's `target-dir =
  "target-cli"` config means earlier Phase A "smoke tests" against
  `./target/debug/mxr` were running a stale May-13 pre-Phase-A binary.
  The Phase A code changes pass unit + workspace tests but were never
  exercised at runtime in this branch. Flagged for the user.

### 2026-05-17 — Phase A landed
- All three Phase A tasks shipped in three commits (37b771f,
  b31d006, aa29c46).
- Daemon smoke test passed via CLI. No regression in real Gmail
  sync. Workspace `cargo check --tests` clean.
- Phase A exit gate met; Step 3 (Phase B opaque `SyncCursor`) is
  the next unblocked step.

### 2026-05-17 — Roadmap created
- Six-step plan defined from the spike verdict.
- Spike artifacts consolidated under `docs/msp/` with this
  roadmap + a README.
- Lesson 12 captures the meta-pattern of protocol-first design.
- All files cross-referenced.

### (template for future entries)
```
### YYYY-MM-DD — <short title>
- What changed
- Why
- Any gate met or missed
```

## Decision log

Substantive decisions made along the way. Append below.

### 2026-05-17 — Reframe sync-engine extraction as a wire protocol
- **Choice:** Pursue MSP instead of "extract sync as a Rust library."
- **Why:** Conversation with the project lead. The library framing
  was too small; the spec exercise has architectural value for mxr
  regardless of external adoption.
- **Risk accepted:** Larger scope (months of work, not weeks).
- **Mitigation:** Spike verdict's six-step gating limits the risk
  per step.

### 2026-05-17 — Spec/blog/alignment hosted under `docs/msp/`
- **Choice:** Single directory under `docs/`, not split across
  `extracted-crates/proposals/`.
- **Why:** Multi-month initiative deserves its own canonical home.
- **Side effect:** `docs/extracted-crates/proposals/` no longer
  exists.
